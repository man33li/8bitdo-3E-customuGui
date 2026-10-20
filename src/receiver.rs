//! 2.4 GHz receiver (USB 2dc8:3024) configuration transport over its vendor
//! HID interface (usage page 0xFF00, usage 1, report id 0x81, 31-byte in/out).
//!
//! Windows 1.4.9.0 evidence (UWPAdvance.dll sha256 1f2325…8352):
//! - receiver open 0x180cb1580 selects FF00:1; input handler 0x180c4e5f0 publishes
//!   `[0x81, 0x14, …]` as the physical XInput stream and routes replies only when
//!   raw[2] is 4 (header at raw[2], payload at raw[18]) or 5;
//! - HID serializer 0x180c4bf40: inner = [16 + count, channel 4, header16, payload];
//! - report control factory 0x180caa500: opcode 7, parameter 0 (off) / 0x110 (on);
//! - read launcher 0x180caf1a0 → factory 0x180ca3840: opcode 2, count min(13, 868-offset),
//!   total 868, 2 s timeout, 5 retries; next offset = reply.offset + reply.count.
//!
//! Read requests replicate Windows' cold-cache emission exactly: Windows serializes
//! its zero-initialised cached config slice as payload, so each read carries
//! `count` zero bytes and inner length 16 + count.
//! Codec, transactions and heartbeat ownership are shared by Linux and Windows.
use crate::{Result, invalid, protocol};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Weak, LazyLock, atomic::{AtomicBool, Ordering}},
    time::{Duration, Instant},
};
use crate::interrupted;
#[cfg(target_os = "linux")]
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::fd::AsRawFd,
};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{DeviceInfo, Receiver, devices, find};

pub const VENDOR: u16 = 0x2dc8;
pub const PRODUCT: u16 = 0x3024;
const REPORT: u8 = 0x81;
const CHANNEL: u8 = 4;
const READ_CHUNK: usize = 13;
const TIMEOUT: Duration = Duration::from_secs(2);
const RETRIES: usize = 5;

#[cfg(any(windows, test))]
/// Match the vendor collection, not a sibling mouse/gamepad interface.
fn vendor_collection(vendor: u16, product: u16, usage_page: u16, usage: u16) -> bool {
    vendor == VENDOR && product == PRODUCT && usage_page == 0xff00 && usage == 1
}

#[cfg(any(windows, test))]
fn check_report_lengths(input: u16, output: u16) -> Result<()> {
    if input != 32 || output != 32 {
        return Err(invalid(format!("Unsupported receiver HID report lengths: input={input}, output={output}; expected 32/32 including report ID 0x81")));
    }
    Ok(())
}
#[cfg(target_os = "linux")]
/// Report descriptor prefix of the vendor collection: Usage Page 0xFF00, Usage 1.
const VENDOR_DESCRIPTOR: &[u8] = &[0x06, 0x00, 0xff, 0x09, 0x01];

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(400);
pub const PADDLE_MASK: u32 = 0x00200000 | 0x40000000 | 0x02000000 | 0x04000000;

/// Shared by all opens of one node. Configuration holds the same lock through
/// report restoration; other consumers skip ticks rather than blocking input.
#[derive(Default)]
struct Heartbeat {
    last: Option<Instant>,
}
impl Heartbeat {
    fn due(&mut self, now: Instant, needed: bool) -> bool {
        if !needed || self.last.is_some_and(|last| now.saturating_duration_since(last) < HEARTBEAT_INTERVAL) {
            return false;
        }
        self.last = Some(now);
        true
    }
}
type Heartbeats = parking_lot::Mutex<HashMap<PathBuf, Weak<parking_lot::Mutex<Heartbeat>>>>;
static HEARTBEATS: LazyLock<Heartbeats> = LazyLock::new(Heartbeats::default);

#[derive(Default)]
/// Physical reports are change-driven (live: 1–2 per key change, none while held), so the
/// last report's state holds until the next one; there is no silence timeout.
pub struct Paddles {
    buttons: u32,
}
impl Paddles {
    pub fn update(&mut self, state: protocol::PhysicalInput) {
        let mut buttons = state.buttons & PADDLE_MASK;
        // Live: D-pad Up/Down/Left report their bit plus PL|PR (0x06000000) with no paddle
        // pressed. Drop that pair while a direction is held; a real PL+PR+D-pad combo is lost.
        const DPAD: u32 = 0x200 | 0x100 | 0x80 | 0x40;
        const PL_PR: u32 = 0x02000000 | 0x04000000;
        if state.buttons & DPAD != 0 && buttons & PL_PR == PL_PR { buttons &= !PL_PR; }
        self.buttons = buttons;
    }
    pub fn buttons(&self) -> u32 { self.buttons }
}

pub fn physical_input(raw: &[u8]) -> Result<Option<protocol::PhysicalInput>> {
    if raw.len() != 32 || raw[..2] != [REPORT, 0x1c] { return Ok(None); }
    protocol::physical_input(raw)
}

/// Windows c4cbd0 → c4d5f0: no acknowledgment or retry.
pub fn heartbeat_frame() -> [u8; 32] {
    frame(0x55, 0, 0, 0, 0, &[]).expect("empty payload")
}

/// Physical XInput-format state published on the vendor stream (live capture).
/// Only raw[2..22] (inner length 0x14) is report content; later bytes are stale
/// residue of earlier replies in the receiver's buffer (observed live), not state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Pad {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_x: i16,
    pub left_y: i16,
    pub right_x: i16,
    pub right_y: i16,
}

pub fn pad(raw: &[u8]) -> Option<Pad> {
    // byte3 bit 0x08 is reserved in the XInput layout; a 4-byte config ack
    // (len 0x14, raw[2]=4, raw[3]=opcode) would otherwise alias the pad stream.
    if raw.len() < 22 || raw[0] != REPORT || raw[1] != 0x14 || raw[3] & 0x08 != 0 {
        return None;
    }
    let i16_at = |at: usize| i16::from_le_bytes([raw[at], raw[at + 1]]);
    Some(Pad {
        buttons: u16::from_le_bytes([raw[2], raw[3]]),
        left_trigger: raw[4],
        right_trigger: raw[5],
        left_x: i16_at(6),
        left_y: i16_at(8),
        right_x: i16_at(10),
        right_y: i16_at(12),
    })
}

/// 32-byte output report: id, inner length, channel, header, payload, zero padding.
pub fn frame(opcode: u16, parameter: u16, count: u32, total: u32, offset: u32, payload: &[u8]) -> Result<[u8; 32]> {
    frame_on(CHANNEL, opcode, parameter, count, total, offset, payload)
}

fn frame_on(channel: u8, opcode: u16, parameter: u16, count: u32, total: u32, offset: u32, payload: &[u8]) -> Result<[u8; 32]> {
    if payload.len() > READ_CHUNK || (payload.len() as u32 != count && !payload.is_empty()) {
        return Err(invalid("Receiver payload must be empty or exactly count ≤ 13 bytes"));
    }
    let mut out = [0u8; 32];
    out[0] = REPORT;
    out[1] = (16 + payload.len()) as u8;
    out[2] = channel;
    out[3..5].copy_from_slice(&opcode.to_le_bytes());
    out[5..7].copy_from_slice(&parameter.to_le_bytes());
    out[7..11].copy_from_slice(&count.to_le_bytes());
    out[11..15].copy_from_slice(&total.to_le_bytes());
    out[15..19].copy_from_slice(&offset.to_le_bytes());
    out[19..19 + payload.len()].copy_from_slice(payload);
    Ok(out)
}

/// Component-version read (factory 0x180caa310, receiver launcher 0x180caeb20 → queue
/// 0x180c4dbf0): channel 5, opcode 0xE1, total 40, 13-byte chunks, zero payload from
/// Windows' static 40-byte buffer. Replies are kind 5: header at raw[3], payload raw[19].
pub fn version_frame(offset: usize) -> Result<[u8; 32]> {
    if offset >= VERSION_SIZE {
        return Err(invalid("Version offset beyond 40-byte record"));
    }
    let count = READ_CHUNK.min(VERSION_SIZE - offset);
    frame_on(5, 0xe1, 0, count as u32, VERSION_SIZE as u32, offset as u32, &[0; READ_CHUNK][..count])
}

pub const VERSION_SIZE: usize = 40;

/// Windows decoder 0x180ca1120 reads bytes 4,12,20,28,36 of the 40-byte record and shows
/// each byte's hex digits as a decimal number (BCD-like: 0x82 → 82). None if not BCD.
pub fn version_components(record: &[u8; VERSION_SIZE]) -> [Option<u8>; 5] {
    [4, 12, 20, 28, 36].map(|at| {
        let byte = record[at];
        (byte >> 4 <= 9 && byte & 15 <= 9).then_some((byte >> 4) * 10 + (byte & 15))
    })
}

pub fn report_frame(enabled: bool) -> [u8; 32] {
    frame(7, if enabled { 0x110 } else { 0 }, 0, 0, 0, &[]).expect("empty payload")
}

/// Windows "Xinput Mode" off (UWP 0x180d347c0 → factory 0x180c87b50): opcode 0x0a,
/// parameter 0, channel 4. Live: the receiver re-enumerates as Xbox/GIP 2dc8:202f
/// (persistent); `gip::Gip::enable_xinput_mode` reverses it.
pub fn xbox_mode_frame() -> [u8; 32] {
    frame(0x0a, 0, 0, 0, 0, &[]).expect("empty payload")
}

pub fn read_frame(offset: usize) -> Result<[u8; 32]> {
    if offset >= protocol::CONFIG_SIZE {
        return Err(invalid("Read offset beyond profile image"));
    }
    let count = READ_CHUNK.min(protocol::CONFIG_SIZE - offset);
    frame(2, 0, count as u32, protocol::CONFIG_SIZE as u32, offset as u32, &[0; READ_CHUNK][..count])
}

#[cfg(target_os = "linux")]
/// Locate the receiver's vendor hidraw node from sysfs (no device is opened).
pub fn find() -> Result<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir("/sys/class/hidraw")? {
        let entry = entry?;
        let device = entry.path().join("device");
        let Ok(uevent) = fs::read_to_string(device.join("uevent")) else { continue };
        if !uevent.contains(&format!("HID_ID=0003:0000{VENDOR:04X}:0000{PRODUCT:04X}")) {
            continue;
        }
        let descriptor = fs::read(device.join("report_descriptor"))?;
        if descriptor.starts_with(VENDOR_DESCRIPTOR) {
            found.push(Path::new("/dev").join(entry.file_name()));
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(io::Error::new(io::ErrorKind::NotFound, "8BitDo 2.4 GHz receiver (2dc8:3024) vendor interface not found").into()),
        _ => Err(invalid("Multiple 2.4 GHz receivers; unplug all but one")),
    }
}

#[cfg(target_os = "linux")]
pub struct Receiver {
    file: File,
    stop: Option<Arc<AtomicBool>>,
    heartbeat: Arc<parking_lot::Mutex<Heartbeat>>,
}

#[cfg(target_os = "linux")]
impl Receiver {
    pub fn open(path: &Path) -> Result<Self> {
        let key = fs::canonicalize(path)?;
        let file = OpenOptions::new().read(true).write(true).open(&key)?;
        let mut registry = HEARTBEATS.lock();
        registry.retain(|_, state| state.strong_count() != 0);
        let heartbeat = registry.get(&key).and_then(Weak::upgrade).unwrap_or_else(|| {
            let state = Arc::new(parking_lot::Mutex::new(Heartbeat::default()));
            registry.insert(key, Arc::downgrade(&state));
            state
        });
        Ok(Self { file, stop: None, heartbeat })
    }


    fn send(&mut self, report: &[u8; 32]) -> Result<()> {
        let written = self.file.write(report)?;
        if written != report.len() {
            return Err(invalid("Short receiver output report"));
        }
        Ok(())
    }

    /// Next input report, or None at the deadline. The pad stream (~124 Hz) is returned too.
    fn next(&mut self, deadline: Instant, buffer: &mut [u8; 64]) -> Result<Option<usize>> {
        loop {
            if interrupted() || self.stop.as_ref().is_some_and(|stop| stop.load(Ordering::Relaxed)) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into());
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            let mut poll = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            let wait = deadline.saturating_duration_since(now).as_millis().clamp(1, 100) as i32;
            // SAFETY: one valid pollfd for the duration of the call.
            let ready = unsafe { libc::poll(&mut poll, 1, wait) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted { continue; }
                return Err(error.into());
            }
            if ready == 0 { continue; }
            if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(io::Error::new(io::ErrorKind::NotConnected, "Receiver disconnected").into());
            }
            let length = self.file.read(buffer)?;
            if length == 0 {
                return Err(io::Error::new(io::ErrorKind::NotConnected, "Receiver disconnected").into());
            }
            return Ok(Some(length));
        }
    }

    /// Drain both report kinds without treating XInput residue as physical keys.
    pub fn input(&mut self, timeout: Duration) -> Result<(Option<Pad>, Option<protocol::PhysicalInput>)> {
        let deadline = Instant::now() + timeout;
        let mut buffer = [0u8; 64];
        let mut newest = None;
        let mut physical = None;
        if !timeout.is_zero() {
            let Some(length) = self.next(deadline, &mut buffer)? else { return Ok((None, None)); };
            newest = pad(&buffer[..length]);
            physical = physical_input(&buffer[..length])?;
        }
        loop {
            let mut poll = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one valid pollfd; zero timeout never blocks.
            let ready = unsafe { libc::poll(&mut poll, 1, 0) };
            if ready < 0 { return Err(io::Error::last_os_error().into()); }
            if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(io::Error::new(io::ErrorKind::NotConnected, "Receiver disconnected").into());
            }
            if ready == 0 || poll.revents & libc::POLLIN == 0 { return Ok((newest, physical)); }
            let length = self.file.read(&mut buffer)?;
            if length == 0 {
                return Err(io::Error::new(io::ErrorKind::NotConnected, "Receiver disconnected").into());
            }
            if let Some(state) = pad(&buffer[..length]) {
                newest = Some(state);
            }
            if let Some(state) = physical_input(&buffer[..length])? { physical = Some(state); }
        }
    }

}

impl Receiver {
    pub fn pad(&mut self, timeout: Duration) -> Result<Option<Pad>> {
        Ok(self.input(timeout)?.0)
    }

    /// Cancel in-flight waits when `stop` becomes true (checked at ≤100 ms granularity).
    /// Reporting is still re-enabled after a cancelled configuration transfer.
    pub fn with_stop(mut self, stop: Arc<AtomicBool>) -> Self {
        self.stop = Some(stop);
        self
    }

    /// Call while a physical-input consumer is active. Multiple opens share one
    /// 400-ms schedule; transactions suppress ticks on every open.
    pub fn heartbeat(&mut self, needed: bool, now: Instant) -> Result<()> {
        let state = Arc::clone(&self.heartbeat);
        let Some(mut schedule) = state.try_lock() else { return Ok(()); };
        if schedule.due(now, needed) { self.send(&heartbeat_frame())?; }
        Ok(())
    }
    pub fn set_reporting(&mut self, enabled: bool) -> Result<()> {
        self.send(&report_frame(enabled))
    }

    /// One send; the receiver disconnects instead of acknowledging (observed live).
    pub fn enter_xbox_mode(&mut self) -> Result<()> {
        self.send(&xbox_mode_frame())
    }

    /// Next kind-4 reply echoing `opcode` (header at raw[2], payload at raw[18]).
    fn reply(&mut self, opcode: u16, deadline: Instant, buffer: &mut [u8; 64]) -> Result<Option<usize>> {
        while let Some(length) = self.next(deadline, buffer)? {
            if length >= 18 && buffer[0] == REPORT && buffer[2..4] == [4, 0] && buffer[4..6] == opcode.to_le_bytes() {
                return Ok(Some(length));
            }
        }
        Ok(None)
    }

    /// Component version record (kind-5 replies: raw[2]=5, header raw[3..19], payload raw[19..]).
    pub fn read_version(&mut self) -> Result<[u8; VERSION_SIZE]> {
        let heartbeat = Arc::clone(&self.heartbeat);
        let _pause = heartbeat.lock();
        let mut record = [0u8; VERSION_SIZE];
        let mut buffer = [0u8; 64];
        let mut offset = 0;
        'chunks: while offset < VERSION_SIZE {
            let request = version_frame(offset)?;
            for _ in 0..RETRIES {
                self.send(&request)?;
                let deadline = Instant::now() + TIMEOUT;
                while let Some(length) = self.next(deadline, &mut buffer)? {
                    let raw = &buffer[..length];
                    let field = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().expect("bounded")) as usize;
                    if length < 19 || raw[0] != REPORT || raw[2] != 5 || raw[3..5] != [0xe1, 0] || field(11) != VERSION_SIZE || field(15) != offset {
                        continue;
                    }
                    let count = field(7);
                    if count == 0 || offset + count > VERSION_SIZE || 19 + count > length {
                        return Err(invalid("Malformed version reply"));
                    }
                    record[offset..offset + count].copy_from_slice(&raw[19..19 + count]);
                    offset += count;
                    continue 'chunks;
                }
            }
            return Err(io::Error::new(io::ErrorKind::TimedOut, format!("No version reply at byte {offset}")).into());
        }
        Ok(record)
    }

    /// Runs `body` with physical reporting disabled (acknowledged first, as Windows
    /// does) and ALWAYS re-enables reporting afterwards, including on error.
    fn configuration<T>(&mut self, body: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let heartbeat = Arc::clone(&self.heartbeat);
        let _pause = heartbeat.lock();
        let result = self.disable_acknowledged().and_then(|()| body(self));
        let restored = self.set_reporting(true);
        match (result, restored) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(error)) => Err(invalid(format!("Transfer completed but report re-enable failed; replug receiver: {error}"))),
            (Err(error), Ok(())) => Err(error),
            (Err(error), Err(restore)) => Err(invalid(format!("{error}; report re-enable ALSO failed ({restore}); replug receiver"))),
        }
    }

    /// Windows read transaction (launcher 0x180caf1a0).
    pub fn read_profiles(&mut self) -> Result<Vec<u8>> {
        self.configuration(|receiver| crate::profile::checked_read(|| receiver.read_image()))
    }

    /// Windows save transaction (launcher 0x180cae960 → factory 0x180c887c0, then commit
    /// 0x180cae7c0 → factory 0x180c7fc50): 67 opcode-1 chunks (count ≤ 13, total 868),
    /// each acknowledged before the next, then opcode 6. PERSISTENT on the controller.
    /// Refuses images whose stored CRC does not match bytes 4..867, and never commits
    /// after an incomplete transfer.
    pub fn write_profiles(&mut self, image: &[u8; protocol::CONFIG_SIZE]) -> Result<()> {
        if crate::profile::crc(image) != u16::from_le_bytes([image[0], image[1]]) {
            return Err(invalid("Refusing to write a profile image with an invalid CRC"));
        }
        self.configuration(|receiver| {
            let mut buffer = [0u8; 64];
            let mut offset = 0;
            while offset < protocol::CONFIG_SIZE {
                let count = READ_CHUNK.min(protocol::CONFIG_SIZE - offset);
                let request = frame(1, 0, count as u32, protocol::CONFIG_SIZE as u32, offset as u32, &image[offset..offset + count])?;
                offset = receiver.acknowledged(&request, 1, &mut buffer, |reply| {
                    let field = |at: usize| u32::from_le_bytes(reply[at..at + 4].try_into().expect("bounded"));
                    (field(10) == protocol::CONFIG_SIZE as u32 && field(14) == offset as u32 && field(6) == count as u32)
                        .then_some(offset + count)
                })?;
            }
            receiver.acknowledged(&frame(6, 0, 0, 0, 0, &[])?, 6, &mut buffer, |_| Some(()))
        })
    }

    /// Sends `request` (retrying identically) until a kind-4 reply echoing `opcode`
    /// satisfies `accept`.
    fn acknowledged<T>(&mut self, request: &[u8; 32], opcode: u16, buffer: &mut [u8; 64], mut accept: impl FnMut(&[u8]) -> Option<T>) -> Result<T> {
        for _ in 0..RETRIES {
            self.send(request)?;
            let deadline = Instant::now() + TIMEOUT;
            while let Some(length) = self.reply(opcode, deadline, buffer)? {
                if let Some(value) = accept(&buffer[..length]) {
                    return Ok(value);
                }
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, format!("No acknowledgment for opcode {opcode} after {RETRIES} attempts")).into())
    }

    /// Windows waits for the opcode-7 acknowledgment before configuration traffic.
    fn disable_acknowledged(&mut self) -> Result<()> {
        let mut buffer = [0u8; 64];
        for _ in 0..RETRIES {
            self.set_reporting(false)?;
            if self.reply(7, Instant::now() + TIMEOUT, &mut buffer)?.is_some() {
                return Ok(());
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "Controller did not acknowledge report disable; is it awake and paired to this receiver?").into())
    }

    fn read_image(&mut self) -> Result<Vec<u8>> {
        let mut transfer = protocol::ConfigRead::new(false);
        let mut buffer = [0u8; 64];
        while !transfer.complete() {
            let request = read_frame(transfer.data().len())?;
            let reply = self.acknowledged(&request, 2, &mut buffer, |reply| {
                (reply[10..14] == (protocol::CONFIG_SIZE as u32).to_le_bytes()).then(|| reply.to_vec())
            })?;
            transfer.accept(&reply)?;
        }
        Ok(transfer.data().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_caps_filter_rejects_other_collections_and_products() {
        assert!(vendor_collection(0x2dc8, 0x3024, 0xff00, 1));
        for (vendor, product, page, usage) in [
            (0x2dc8, 0x202f, 0xff00, 1),
            (0x045e, 0x3024, 0xff00, 1),
            (0x2dc8, 0x3024, 1, 5),
            (0x2dc8, 0x3024, 1, 2),
            (0x2dc8, 0x3024, 0xff00, 2),
        ] {
            assert!(!vendor_collection(vendor, product, page, usage));
        }
    }

    #[test]
    fn hid_report_lengths_already_include_report_id() {
        assert!(check_report_lengths(32, 32).is_ok());
        for (input, output) in [(31, 32), (32, 31), (33, 33), (0, 32)] {
            let error = check_report_lengths(input, output).unwrap_err().to_string();
            assert!(error.contains(&format!("input={input}, output={output}")));
            assert!(error.contains("including report ID 0x81"));
        }
    }

    #[test]
    fn heartbeat_bytes_and_shared_cadence_pause_and_handoff() {
        let mut expected = [0; 32];
        expected[..4].copy_from_slice(&[0x81, 0x10, 4, 0x55]);
        assert_eq!(heartbeat_frame(), expected);
        let mut schedule = Heartbeat::default();
        let start = Instant::now();
        assert!(!schedule.due(start, false));
        assert!(schedule.due(start, true)); // GUI.
        assert!(!schedule.due(start, true)); // Mapper on the same receiver.
        assert!(!schedule.due(start + Duration::from_millis(399), true));
        assert!(schedule.due(start + HEARTBEAT_INTERVAL, true));
        assert!(!schedule.due(start + HEARTBEAT_INTERVAL, true));
        assert!(!schedule.due(start + Duration::from_secs(2), false));
        assert!(schedule.due(start + Duration::from_secs(3), true));
        assert!(!schedule.due(start + Duration::from_secs(3), true), "no catch-up burst");

    }

    #[cfg(target_os = "linux")]
    #[test]
    fn heartbeat_shared_schedule_pauses_and_hands_off() {
        let expected = heartbeat_frame();
        let start = Instant::now();
        use std::os::{fd::OwnedFd, unix::net::UnixStream};
        let (source, mut peer) = UnixStream::pair().unwrap();
        peer.set_nonblocking(true).unwrap();
        let heartbeat = Arc::new(parking_lot::Mutex::new(Heartbeat::default()));
        let mut receiver = Receiver { file: File::from(OwnedFd::from(source)), stop: None, heartbeat: Arc::clone(&heartbeat) };
        let mut mapper = Receiver { file: receiver.file.try_clone().unwrap(), stop: None, heartbeat: Arc::clone(&heartbeat) };
        let pause = heartbeat.lock(); // Same exclusion held by configuration/version.
        receiver.heartbeat(true, start).unwrap();
        mapper.heartbeat(true, start).unwrap();
        let mut bytes = [0; 64];
        assert_eq!(peer.read(&mut bytes).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        drop(pause);
        receiver.heartbeat(true, start).unwrap();
        mapper.heartbeat(true, start).unwrap();
        assert_eq!(peer.read(&mut bytes).unwrap(), 32);
        assert_eq!(&bytes[..32], &expected);
        receiver.heartbeat(false, start + HEARTBEAT_INTERVAL).unwrap(); // Leave Test.
        mapper.heartbeat(true, start + HEARTBEAT_INTERVAL).unwrap();
        assert_eq!(peer.read(&mut bytes).unwrap(), 32, "mapper takes over the next shared tick");
        drop(mapper);
        receiver.heartbeat(false, start + Duration::from_secs(5)).unwrap();
        assert_eq!(peer.read(&mut bytes).unwrap_err().kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn receiver_physical_capture_and_xinput_residue_are_distinct() {
        let raw = protocol::decode_hex("811c040003000c00000000000000000000000020000080808080000004002000").unwrap();
        let state = physical_input(&raw).unwrap().unwrap();
        assert_eq!(state, protocol::PhysicalInput { buttons: 0x2000, left_x: 128, left_y: 128, right_x: 128, right_y: 128, left_trigger: 0, right_trigger: 0 });
        assert!(pad(&raw).is_none());
        for key in [0x2000u32, 0x00200000, 0x40000000, 0x04000000, 0x02000000] {
            let mut raw = raw.clone();
            raw[18..22].copy_from_slice(&key.to_le_bytes());
            assert_eq!(physical_input(&raw).unwrap().unwrap().buttons, key);
            raw[1] = 0x14;
            assert!(pad(&raw).is_some());
            assert!(physical_input(&raw).unwrap().is_none(), "XInput residue cannot become paddles");
        }
        assert!(physical_input(&raw[..31]).unwrap().is_none());
        let mut bad = raw.clone(); bad[0] = 0;
        assert!(physical_input(&bad).unwrap().is_none());
        bad = raw.clone(); bad[7] = 1;
        assert!(physical_input(&bad).is_err());
        bad = raw; bad[4] = 2;
        assert!(physical_input(&bad).unwrap().is_none());
        let mut xinput = [0; 32];
        xinput[..6].copy_from_slice(&[0x81,0x14,4,0,3,0]);
        assert!(pad(&xinput).is_some());
        assert!(physical_input(&xinput).unwrap().is_none(), "D-left + LT=3 is not a malformed physical report");
    }

    #[test]
    fn physical_paddles_hold_until_next_report_and_ignore_dpad_artifact() {
        let mut paddles = Paddles::default();
        let state = physical_input(&protocol::decode_hex("811c040003000c00000000000000000000000020000080808080000004002000").unwrap()).unwrap().unwrap();
        paddles.update(protocol::PhysicalInput { buttons: 0x40000000, ..state });
        assert_eq!(paddles.buttons(), 0x40000000, "held R4 persists without further reports");
        paddles.update(protocol::PhysicalInput { buttons: 0x2000, ..state });
        assert_eq!(paddles.buttons(), 0);
        // Captured live: D-pad Up = 0x06000200, Down = 0x06000100, Left = 0x06000080.
        for dpad in [0x06000200, 0x06000100, 0x06000080] {
            paddles.update(protocol::PhysicalInput { buttons: dpad, ..state });
            assert_eq!(paddles.buttons(), 0, "0x{dpad:08x}");
        }
        paddles.update(protocol::PhysicalInput { buttons: 0x02000200, ..state });
        assert_eq!(paddles.buttons(), 0x02000000, "single paddle with D-pad kept");
    }

    #[test]
    fn frames_match_recovered_windows_serializer() {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(&report_frame(false)[..8]), "8110040700000000");
        assert_eq!(hex(&report_frame(true)[..8]), "8110040700100100");
        // Exact Windows cold-cache read: len 0x1d, count 13, total 868, offset 0, 13 zero bytes.
        assert_eq!(hex(&read_frame(0).unwrap()), "811d04020000000d000000640300000000000000000000000000000000000000");
        // Last read is clamped to the remaining 868 - 858 = 10 bytes.
        let last = read_frame(858).unwrap();
        assert_eq!(u32::from_le_bytes(last[7..11].try_into().unwrap()), 10);
        assert_eq!(last[1], 26);
        assert_eq!(u32::from_le_bytes(last[15..19].try_into().unwrap()), 858);
        assert!(read_frame(868).is_err());
        assert!(frame(1, 0, 14, 0, 0, &[0; 14]).is_err());
    }

    #[test]
    fn service_frames_match_recovered_factories() {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        // SpecTransport raw32 for the cold first version read.
        assert_eq!(hex(&version_frame(0).unwrap()), "811d05e10000000d000000280000000000000000000000000000000000000000");
        let last = version_frame(39).unwrap();
        assert_eq!((last[1], u32::from_le_bytes(last[7..11].try_into().unwrap())), (17, 1));
        let mut record = [0u8; VERSION_SIZE];
        record[4] = 0x82;
        record[12] = 0x1a;
        assert_eq!(version_components(&record), [Some(82), None, Some(0), Some(0), Some(0)]);
    }

    #[test]
    fn xbox_mode_frame_is_the_live_incident_frame() {
        let mut expected = [0u8; 32];
        expected[..4].copy_from_slice(&[0x81, 0x10, 4, 0x0a]);
        assert_eq!(xbox_mode_frame(), expected);
    }
    #[test]
    fn pad_stream_decodes_live_capture() {
        let mut raw = [0u8; 32];
        raw[..14].copy_from_slice(&[0x81, 0x14, 0x10, 0x02, 0x44, 0x27, 0xff, 0x7f, 0x38, 0x02, 0, 0, 0, 0x80]);
        raw[28] = 0xa0;
        let state = pad(&raw).unwrap();
        assert_eq!((state.buttons, state.left_trigger, state.right_trigger), (0x0210, 0x44, 0x27));
        assert_eq!((state.left_x, state.left_y, state.right_y), (32767, 0x0238, -32768));
        raw[1] = 0x1d;
        assert!(pad(&raw).is_none(), "configuration replies are not pad state");
        let mut ack = [0u8; 32];
        ack[..5].copy_from_slice(&[0x81, 0x14, 0x04, 0x09, 0x00]);
        assert!(pad(&ack).is_none(), "4-byte configuration acknowledgment is not pad state");
    }
}
