//! USB DualSense Edge over Linux UHID; no physical-controller I/O.
//!
//! Protocol references (read 2026-10-19):
//! - HHD descriptor, stock features, button layout and 333 ns sensor ticks:
//!   https://github.com/hhd-dev/hhd/blob/master/src/hhd/controller/virtual/dualsense/const.py
//!   https://github.com/hhd-dev/hhd/blob/master/src/hhd/controller/virtual/dualsense/__init__.py
//! - InputPlumber USB target and descriptor (original capture credited to LtChipotle):
//!   https://github.com/ShadowBlip/InputPlumber/blob/main/src/input/target/dualsense.rs
//!   https://github.com/ShadowBlip/InputPlumber/blob/main/src/drivers/dualsense/report_descriptor.rs
//! - SDL identification, calibration, Edge bits and timestamp decoding:
//!   https://github.com/libsdl-org/SDL/blob/main/src/joystick/hidapi/SDL_hidapi_ps5.c
//!   https://github.com/libsdl-org/SDL/blob/main/include/SDL3/SDL_sensor.h
//! - Kernel probe requires features 09/20/05, exactly 20/64/41 bytes:
//!   https://github.com/torvalds/linux/blob/master/drivers/hid/hid-playstation.c
//!   https://github.com/torvalds/linux/blob/master/include/uapi/linux/uhid.h
//!
//! USB has no Bluetooth CRC or A1/A2/A3 prefix. Sony VID/PID 054c:0df2 selects
//! Edge support (17 SDL buttons); retain HHD's Sony name for client compatibility.
//! Steam needs hidraw access, PlayStation controller support enabled, and four
//! independent bits in byte 10, not duplicate ordinary gamepad buttons.
//!
//! Calibration deliberately uses zero bias, gyro endpoints +/-8192 at +/-512
//! dps, accelerometer endpoints +/-8192 at +/-1 g. Both drivers calculate gyro
//! sensitivity (512+512)*1024/(8192-(-8192)) = 64: raw has 16 units/dps,
//! kernel output 1024 units/dps, SDL output radians/s. Accelerometer sensitivity
//! 2*8192/(8192-(-8192)) = 1: raw/kernel have 8192 units/g; SDL returns m/s².
//! Unlike HHD's 20 units/dps calibration, these also equal SDL's fallback units.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

const PERIOD: Duration = Duration::from_millis(4);
const GYRO_UNITS_PER_DPS: f32 = 16.0;
const ACCEL_UNITS_PER_G: f32 = 8192.0;
const UHID_DATA_MAX: usize = 4096;
const UHID_EVENT_SIZE: usize = 4376;
const UHID_DESTROY: u32 = 1;
const UHID_START: u32 = 2;
const UHID_STOP: u32 = 3;
const UHID_OUTPUT: u32 = 6;
const UHID_GET_REPORT: u32 = 9;
const UHID_GET_REPORT_REPLY: u32 = 10;
const UHID_CREATE2: u32 = 11;
const UHID_INPUT2: u32 = 12;
const UHID_SET_REPORT: u32 = 13;
const UHID_SET_REPORT_REPLY: u32 = 14;
const FEATURE_REPORT: u8 = 0;
const OUTPUT_REPORT: u8 = 1;

/// Latest complete controller state. No host remapping occurs in this module.
#[derive(Clone, Copy, Debug)]
pub struct EdgeState {
    pub buttons: u32,
    /// 0..255, center 128; Y increases downwards.
    pub left_stick: [u8; 2],
    pub right_stick: [u8; 2],
    /// Left, right.
    pub triggers: [u8; 2],
    /// Pitch front-up, yaw clockwise from above, roll right-side-down, in dps.
    pub gyro_dps: [f32; 3],
    /// SDL axes: +X right, +Y up, +Z towards player; includes gravity.
    pub accel_g: [f32; 3],
}

impl Default for EdgeState {
    fn default() -> Self {
        Self {
            buttons: 0,
            left_stick: [128; 2],
            right_stick: [128; 2],
            triggers: [0; 2],
            gyro_dps: [0.0; 3],
            accel_g: [0.0, 1.0, 0.0],
        }
    }
}

impl EdgeState {
    pub const SQUARE: u32 = 1 << 0;
    pub const CROSS: u32 = 1 << 1;
    pub const CIRCLE: u32 = 1 << 2;
    pub const TRIANGLE: u32 = 1 << 3;
    pub const L1: u32 = 1 << 4;
    pub const R1: u32 = 1 << 5;
    pub const L2: u32 = 1 << 6;
    pub const R2: u32 = 1 << 7;
    pub const CREATE: u32 = 1 << 8;
    pub const OPTIONS: u32 = 1 << 9;
    pub const L3: u32 = 1 << 10;
    pub const R3: u32 = 1 << 11;
    pub const PS: u32 = 1 << 12;
    pub const TOUCHPAD: u32 = 1 << 13;
    pub const MUTE: u32 = 1 << 14;
    pub const FN_LEFT: u32 = 1 << 16;
    pub const FN_RIGHT: u32 = 1 << 17;
    pub const BACK_LEFT: u32 = 1 << 18;
    pub const BACK_RIGHT: u32 = 1 << 19;
    pub const DPAD_UP: u32 = 1 << 20;
    pub const DPAD_DOWN: u32 = 1 << 21;
    pub const DPAD_LEFT: u32 = 1 << 22;
    pub const DPAD_RIGHT: u32 = 1 << 23;
}

struct Shared {
    state: Mutex<EdgeState>,
    rumble: Mutex<Option<[u8; 2]>>,
    stop: AtomicBool,
}

/// Owns a virtual USB Edge. A dedicated thread owns its UHID descriptor.
pub struct Edge {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Edge {
    pub fn create() -> crate::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(EdgeState::default()),
            rumble: Mutex::new(None),
            stop: AtomicBool::new(false),
        });
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker_state = Arc::clone(&shared);
        let worker = thread::Builder::new().name("steam-edge".into()).spawn(move || {
            let result = (|| -> io::Result<()> {
                let mut mac = [0; 6];
                File::open("/dev/urandom")?.read_exact(&mut mac)?;
                // MAC is stored little-endian in feature 09; first displayed octet
                // must be locally administered, unicast. Same identity in uniq.
                mac[5] = (mac[5] & !3) | 2;
                let mut fd = OpenOptions::new().read(true).write(true)
                    .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open("/dev/uhid")?;
                write_event(&mut fd, &create_event(mac))?;
                let _ = ready_tx.send(Ok(()));
                let result = serve(&mut fd, &worker_state, mac);
                let destroy = UHID_DESTROY.to_ne_bytes();
                let cleanup = write_event(&mut fd, &destroy);
                result.and(cleanup)
            })();
            if let Err(error) = result {
                // Startup errors go to the caller; runtime failures cannot be
                // returned by the nonblocking update API, so report them once.
                let message = error.to_string();
                if ready_tx.send(Err(error)).is_err() {
                    eprintln!("DualSense Edge UHID stopped: {message}");
                }
            }
        })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { shared, worker: Some(worker) }),
            outcome => {
                let _ = worker.join();
                match outcome {
                    Ok(Err(error)) => Err(error.into()),
                    _ => Err(crate::invalid("DualSense Edge UHID thread exited during creation")),
                }
            }
        }
    }

    /// Copies state only; never performs device I/O or queues obsolete samples.
    pub fn update(&self, state: &EdgeState) {
        *self.shared.state.lock() = *state;
    }

    /// Latest left/right compatible motor levels; LED-only reports preserve them.
    pub fn rumble(&self) -> Option<[u8; 2]> {
        *self.shared.rumble.lock()
    }
}

impl Drop for Edge {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn create_event(mac: [u8; 6]) -> [u8; UHID_EVENT_SIZE] {
    let mut event = [0; UHID_EVENT_SIZE];
    event[..4].copy_from_slice(&UHID_CREATE2.to_ne_bytes());
    let name = b"Sony Interactive Entertainment DualSense Edge Wireless Controller";
    event[4..4 + name.len()].copy_from_slice(name);
    let uniq = format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[5], mac[4], mac[3], mac[2], mac[1], mac[0]);
    let phys = format!("ultimate-control/edge/{uniq}");
    event[132..132 + phys.len()].copy_from_slice(phys.as_bytes());
    event[196..196 + uniq.len()].copy_from_slice(uniq.as_bytes());
    event[260..262].copy_from_slice(&(DESCRIPTOR.len() as u16).to_ne_bytes());
    event[262..264].copy_from_slice(&3u16.to_ne_bytes()); // BUS_USB
    event[264..268].copy_from_slice(&0x054cu32.to_ne_bytes());
    event[268..272].copy_from_slice(&0x0df2u32.to_ne_bytes());
    event[272..276].copy_from_slice(&256u32.to_ne_bytes());
    event[280..280 + DESCRIPTOR.len()].copy_from_slice(&DESCRIPTOR);
    event
}

// Each write is one UHID event: retry EINTR, never split a partial event.
fn write_event(fd: &mut File, event: &[u8]) -> io::Result<()> {
    loop {
        match fd.write(event) {
            Ok(n) if n == event.len() => return Ok(()),
            Ok(_) => return Err(io::Error::new(io::ErrorKind::WriteZero, "short UHID event write")),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

fn serve(fd: &mut File, shared: &Shared, mac: [u8; 6]) -> io::Result<()> {
    let start = Instant::now();
    let mut next = start;
    let mut sequence = 0u32;
    let mut started = false;
    let mut event = [0; UHID_EVENT_SIZE];
    while !shared.stop.load(Ordering::Relaxed) {
        // Bounded draining prevents a noisy output client starving the IMU clock.
        for _ in 0..32 {
            event.fill(0); // short kernel events are zero-extended by the UHID ABI
            match fd.read(&mut event) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "UHID closed")),
                Ok(n) if n >= 4 => match u32::from_ne_bytes(event[..4].try_into().unwrap()) {
                    UHID_START => started = true,
                    UHID_STOP => started = false,
                    UHID_GET_REPORT | UHID_SET_REPORT => {
                        let (reply, len) = control_reply(&event, mac, shared);
                        write_event(fd, &reply[..len])?;
                    }
                    UHID_OUTPUT if n >= 4 + UHID_DATA_MAX + 3 => {
                        let size = u16::from_ne_bytes(event[4100..4102].try_into().unwrap()) as usize;
                        if event[4102] == OUTPUT_REPORT && size <= UHID_DATA_MAX {
                            if let Some(levels) = parse_rumble(&event[4..4 + size]) {
                                *shared.rumble.lock() = Some(levels);
                            }
                        }
                    }
                    _ => {}
                },
                Ok(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "short UHID event header")),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        let now = Instant::now();
        if now >= next {
            if started {
                // Actual monotonic elapsed time, in 1/3 us ticks. Scheduling
                // delays must not make Steam integrate a fictional fixed dt.
                let timestamp = (start.elapsed().as_micros().wrapping_mul(3) as u32).wrapping_add(1);
                let state = *shared.state.lock();
                let report = input_report(&state, sequence, timestamp);
                let mut input = [0; 70];
                input[..4].copy_from_slice(&UHID_INPUT2.to_ne_bytes());
                input[4..6].copy_from_slice(&64u16.to_ne_bytes());
                input[6..].copy_from_slice(&report);
                write_event(fd, &input)?;
                sequence = sequence.wrapping_add(1);
            }
            next += PERIOD;
            if next <= now {
                next = now + PERIOD; // skip missed periods, never burst old samples
            }
        }
        let remaining = next.saturating_duration_since(Instant::now());
        let timeout_ms = remaining.as_millis() + u128::from(remaining.subsec_nanos() % 1_000_000 != 0);
        let mut poll = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: poll points to one valid pollfd, owned fd remains open.
        let result = unsafe { libc::poll(&mut poll, 1, timeout_ms.min(4) as i32) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        } else if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "UHID poll disconnected"));
        }
    }
    Ok(())
}

fn control_reply(event: &[u8], mac: [u8; 6], shared: &Shared) -> ([u8; 76], usize) {
    let mut reply = [0; 76];
    let kind = u32::from_ne_bytes(event[..4].try_into().unwrap());
    reply[..4].copy_from_slice(&if kind == UHID_GET_REPORT { UHID_GET_REPORT_REPLY } else { UHID_SET_REPORT_REPLY }.to_ne_bytes());
    // Request ids are native-endian opaque values; copy them unchanged.
    reply[4..8].copy_from_slice(&event[4..8]);
    let mut error = libc::EOPNOTSUPP as u16;
    let mut len = 10;
    if kind == UHID_GET_REPORT {
        len = 12;
        if event.len() >= 10 && event[9] == FEATURE_REPORT {
            if let Some((data, size)) = feature_report(event[8], mac) {
                error = 0;
                reply[10..12].copy_from_slice(&(size as u16).to_ne_bytes());
                reply[12..12 + size].copy_from_slice(&data[..size]);
                len += size;
            }
        }
    } else if event.len() >= 12 && event[8] == 2 && event[9] == OUTPUT_REPORT {
        let size = u16::from_ne_bytes(event[10..12].try_into().unwrap()) as usize;
        if size <= UHID_DATA_MAX && event.len() >= 12 + size {
            if let Some(levels) = parse_rumble(&event[12..12 + size]) {
                *shared.rumble.lock() = Some(levels);
            }
            // Valid USB output controls (LEDs/audio/haptics) are accepted even
            // when there is no physical equivalent. Unknown feature writes fail.
            if size >= 5 && event[12] == 2 {
                error = 0;
            }
        }
    }
    reply[8..10].copy_from_slice(&error.to_ne_bytes());
    (reply, len)
}

fn feature_report(id: u8, mac: [u8; 6]) -> Option<([u8; 64], usize)> {
    let mut data = [0; 64];
    data[0] = id;
    let size = match id {
        0x05 => {
            for offset in [7, 11, 15, 23, 27, 31] {
                data[offset..offset + 2].copy_from_slice(&8192i16.to_le_bytes());
                data[offset + 2..offset + 4].copy_from_slice(&(-8192i16).to_le_bytes());
            }
            data[19..21].copy_from_slice(&512i16.to_le_bytes());
            data[21..23].copy_from_slice(&512i16.to_le_bytes());
            data[35] = 0x0b; // stock sensor temperature calibration tail
            41
        }
        0x09 => {
            data[1..7].copy_from_slice(&mac);
            data[7..16].copy_from_slice(&[0x08, 0x25, 0x00, 0x1e, 0x00, 0xee, 0x74, 0xd0, 0xbc]);
            20
        }
        0x20 => {
            // HHD/InputPlumber captured Edge firmware report (feature version
            // 1.84). Identification is VID/PID, not fabricated capabilities 03.
            data.copy_from_slice(&[
                0x20, 0x4a, 0x75, 0x6e, 0x20, 0x31, 0x39, 0x20,
                0x32, 0x30, 0x32, 0x33, 0x31, 0x34, 0x3a, 0x34,
                0x37, 0x3a, 0x33, 0x34, 0x03, 0x00, 0x44, 0x00,
                0x08, 0x02, 0x00, 0x01, 0x36, 0x00, 0x00, 0x01,
                0xc1, 0xc8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x54, 0x01, 0x00, 0x00,
                0x14, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x01, 0x00,
                0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ]);
            64
        }
        _ => return None,
    };
    Some((data, size))
}

fn parse_rumble(report: &[u8]) -> Option<[u8; 2]> {
    if report.len() < 5 || report[0] != 2 {
        return None;
    }
    // Compatible vibration v1 (flag0 bit0) or v2 (flag2 bit2).
    // Steam also uses a short, flags-zero USB rumble report (InputPlumber).
    let compatible = report[1] & 1 != 0 || report.get(39).is_some_and(|flag| flag & 4 != 0);
    let steam_short = report.len() == 48 && report[1] == 0;
    (compatible || steam_short).then_some([report[4], report[3]])
}

fn sensor_raw(value: f32, units: f32) -> i16 {
    // Rust saturating float->int conversion handles infinities, NaN becomes 0.
    (value * units).round() as i16
}

fn input_report(state: &EdgeState, sequence: u32, timestamp: u32) -> [u8; 64] {
    let mut data = [0; 64];
    data[0] = 1;
    data[1..3].copy_from_slice(&state.left_stick);
    data[3..5].copy_from_slice(&state.right_stick);
    data[5..7].copy_from_slice(&state.triggers);
    data[7] = sequence as u8;
    let b = state.buttons;
    let x = i8::from(b & EdgeState::DPAD_RIGHT != 0) - i8::from(b & EdgeState::DPAD_LEFT != 0);
    let y = i8::from(b & EdgeState::DPAD_DOWN != 0) - i8::from(b & EdgeState::DPAD_UP != 0);
    let hat = match (x, y) {
        (0, -1) => 0, (1, -1) => 1, (1, 0) => 2, (1, 1) => 3,
        (0, 1) => 4, (-1, 1) => 5, (-1, 0) => 6, (-1, -1) => 7,
        _ => 8,
    };
    data[8] = ((b & 0xf) << 4) as u8 | hat;
    data[9] = (b >> 4) as u8;
    data[10] = ((b >> 12) & 7) as u8 | ((b >> 12) & 0xf0) as u8;
    data[12..16].copy_from_slice(&sequence.to_le_bytes());
    // SDL uses right-handed axes: yaw CCW from above, roll left-side-down.
    // Convert the mapper's explicitly opposite yaw/roll convention here only.
    let gyro = [state.gyro_dps[0], -state.gyro_dps[1], -state.gyro_dps[2]];
    for axis in 0..3 {
        let offset = 16 + axis * 2;
        data[offset..offset + 2].copy_from_slice(&sensor_raw(gyro[axis], GYRO_UNITS_PER_DPS).to_le_bytes());
        let offset = 22 + axis * 2;
        data[offset..offset + 2].copy_from_slice(&sensor_raw(state.accel_g[axis], ACCEL_UNITS_PER_G).to_le_bytes());
    }
    data[28..32].copy_from_slice(&timestamp.to_le_bytes());
    data[33] = 0x80; // both touch contacts inactive, not phantom touches at 0,0
    data[37] = 0x80;
    data[53] = 0x2a; // full battery, 100%
    data[54] = 0x08; // USB connected, no headset
    data
}

// Exact 389-byte Edge USB descriptor shared by HHD and InputPlumber.
const DESCRIPTOR: [u8; 389] = [
    0x05, 0x01, 0x09, 0x05, 0xa1, 0x01, 0x85, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x32, 0x09, 0x35,
    0x09, 0x33, 0x09, 0x34, 0x15, 0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x95, 0x06, 0x81, 0x02, 0x06,
    0x00, 0xff, 0x09, 0x20, 0x95, 0x01, 0x81, 0x02, 0x05, 0x01, 0x09, 0x39, 0x15, 0x00, 0x25, 0x07,
    0x35, 0x00, 0x46, 0x3b, 0x01, 0x65, 0x14, 0x75, 0x04, 0x95, 0x01, 0x81, 0x42, 0x65, 0x00, 0x05,
    0x09, 0x19, 0x01, 0x29, 0x0f, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x0f, 0x81, 0x02, 0x06,
    0x00, 0xff, 0x09, 0x21, 0x95, 0x0d, 0x81, 0x02, 0x06, 0x00, 0xff, 0x09, 0x22, 0x15, 0x00, 0x26,
    0xff, 0x00, 0x75, 0x08, 0x95, 0x34, 0x81, 0x02, 0x85, 0x02, 0x09, 0x23, 0x95, 0x3f, 0x91, 0x02,
    0x85, 0x05, 0x09, 0x33, 0x95, 0x28, 0xb1, 0x02, 0x85, 0x08, 0x09, 0x34, 0x95, 0x2f, 0xb1, 0x02,
    0x85, 0x09, 0x09, 0x24, 0x95, 0x13, 0xb1, 0x02, 0x85, 0x0a, 0x09, 0x25, 0x95, 0x1a, 0xb1, 0x02,
    0x85, 0x20, 0x09, 0x26, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0x21, 0x09, 0x27, 0x95, 0x04, 0xb1, 0x02,
    0x85, 0x22, 0x09, 0x40, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0x80, 0x09, 0x28, 0x95, 0x3f, 0xb1, 0x02,
    0x85, 0x81, 0x09, 0x29, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0x82, 0x09, 0x2a, 0x95, 0x09, 0xb1, 0x02,
    0x85, 0x83, 0x09, 0x2b, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0x84, 0x09, 0x2c, 0x95, 0x3f, 0xb1, 0x02,
    0x85, 0x85, 0x09, 0x2d, 0x95, 0x02, 0xb1, 0x02, 0x85, 0xa0, 0x09, 0x2e, 0x95, 0x01, 0xb1, 0x02,
    0x85, 0xe0, 0x09, 0x2f, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0xf0, 0x09, 0x30, 0x95, 0x3f, 0xb1, 0x02,
    0x85, 0xf1, 0x09, 0x31, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0xf2, 0x09, 0x32, 0x95, 0x34, 0xb1, 0x02,
    0x85, 0xf4, 0x09, 0x35, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0xf5, 0x09, 0x36, 0x95, 0x03, 0xb1, 0x02,
    0x85, 0x60, 0x09, 0x41, 0x95, 0x3f, 0xb1, 0x02, 0x85, 0x61, 0x09, 0x42, 0xb1, 0x02, 0x85, 0x62,
    0x09, 0x43, 0xb1, 0x02, 0x85, 0x63, 0x09, 0x44, 0xb1, 0x02, 0x85, 0x64, 0x09, 0x45, 0xb1, 0x02,
    0x85, 0x65, 0x09, 0x46, 0xb1, 0x02, 0x85, 0x68, 0x09, 0x47, 0xb1, 0x02, 0x85, 0x70, 0x09, 0x48,
    0xb1, 0x02, 0x85, 0x71, 0x09, 0x49, 0xb1, 0x02, 0x85, 0x72, 0x09, 0x4a, 0xb1, 0x02, 0x85, 0x73,
    0x09, 0x4b, 0xb1, 0x02, 0x85, 0x74, 0x09, 0x4c, 0xb1, 0x02, 0x85, 0x75, 0x09, 0x4d, 0xb1, 0x02,
    0x85, 0x76, 0x09, 0x4e, 0xb1, 0x02, 0x85, 0x77, 0x09, 0x4f, 0xb1, 0x02, 0x85, 0x78, 0x09, 0x50,
    0xb1, 0x02, 0x85, 0x79, 0x09, 0x51, 0xb1, 0x02, 0x85, 0x7a, 0x09, 0x52, 0xb1, 0x02, 0x85, 0x7b,
    0x09, 0x53, 0xb1, 0x02, 0xc0,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn i16_at(data: &[u8], offset: usize) -> i32 {
        i16::from_le_bytes(data[offset..offset + 2].try_into().unwrap()) as i32
    }

    #[test]
    fn buttons_use_real_dualsense_and_edge_bits() {
        let expected = [
            (EdgeState::SQUARE, 8, 0x10), (EdgeState::CROSS, 8, 0x20),
            (EdgeState::CIRCLE, 8, 0x40), (EdgeState::TRIANGLE, 8, 0x80),
            (EdgeState::L1, 9, 1), (EdgeState::R1, 9, 2),
            (EdgeState::L2, 9, 4), (EdgeState::R2, 9, 8),
            (EdgeState::CREATE, 9, 0x10), (EdgeState::OPTIONS, 9, 0x20),
            (EdgeState::L3, 9, 0x40), (EdgeState::R3, 9, 0x80),
            (EdgeState::PS, 10, 1), (EdgeState::TOUCHPAD, 10, 2), (EdgeState::MUTE, 10, 4),
            (EdgeState::FN_LEFT, 10, 0x10), (EdgeState::FN_RIGHT, 10, 0x20),
            (EdgeState::BACK_LEFT, 10, 0x40), (EdgeState::BACK_RIGHT, 10, 0x80),
        ];
        for (button, offset, bit) in expected {
            let report = input_report(&EdgeState { buttons: button, ..Default::default() }, 0, 1);
            let mut buttons = report[8..12].to_vec();
            buttons[0] &= 0xf0;
            let mut expected = [0; 4];
            expected[offset - 8] = bit;
            assert_eq!(buttons, expected, "button {button:#x}");
        }
    }

    #[test]
    fn hat_covers_eight_directions_neutral_and_opposites() {
        let u = EdgeState::DPAD_UP;
        let d = EdgeState::DPAD_DOWN;
        let l = EdgeState::DPAD_LEFT;
        let r = EdgeState::DPAD_RIGHT;
        for (buttons, hat) in [(u, 0), (u | r, 1), (r, 2), (d | r, 3), (d, 4),
            (d | l, 5), (l, 6), (u | l, 7), (0, 8), (u | d, 8), (l | r, 8), (u | d | r, 2)] {
            let report = input_report(&EdgeState { buttons, ..Default::default() }, 0, 1);
            assert_eq!(report[8], hat);
        }
    }

    #[test]
    fn usb_layout_preserves_analog_and_timing_fields() {
        let state = EdgeState { left_stick: [0, 255], right_stick: [127, 129],
            triggers: [23, 244], ..Default::default() };
        let report = input_report(&state, 0x123456ff, 0xfffffff0);
        assert_eq!(&report[..8], &[1, 0, 255, 127, 129, 23, 244, 255]);
        assert_eq!(&report[12..16], &0x123456ffu32.to_le_bytes());
        assert_eq!(&report[28..32], &0xfffffff0u32.to_le_bytes());
        assert_eq!((report[33], report[37], report[53], report[54]), (0x80, 0x80, 0x2a, 8));
        assert_eq!(input_report(&state, 0x100, 12000)[7], 0);
        // Both consumers use wrapping subtraction, then divide ticks by 3.
        let previous = u32::MAX - 5999;
        let next = previous.wrapping_add(12000);
        assert_eq!(next.wrapping_sub(previous) / 3, 4000);
    }

    #[test]
    fn feature_calibration_roundtrips_sdl_and_kernel_math() {
        let (cal, len) = feature_report(5, [0; 6]).unwrap();
        assert_eq!(len, 41);
        assert_eq!(&cal[..7], &[5, 0, 0, 0, 0, 0, 0]);
        assert_eq!((i16_at(&cal, 19), i16_at(&cal, 21)), (512, 512));
        assert_eq!(&cal[35..41], &[11, 0, 0, 0, 0, 0]);
        for gyro_dps in [[0.0; 3], [90.0, -180.0, 45.0], [-2048.0, 2047.0, -123.25]] {
            let accel_g = [0.25, 1.0, -3.5];
            let report = input_report(&EdgeState { gyro_dps, accel_g, ..Default::default() }, 0, 1);
            let expected_gyro = [gyro_dps[0], -gyro_dps[1], -gyro_dps[2]];
            for axis in 0..3 {
                let plus = i16_at(&cal, 7 + axis * 4);
                let minus = i16_at(&cal, 9 + axis * 4);
                let bias = i16_at(&cal, 1 + axis * 2);
                assert_eq!((plus, minus, bias), (8192, -8192, 0));
                let numerator = (i16_at(&cal, 19) + i16_at(&cal, 21)) * 1024;
                let raw = i16_at(&report, 16 + axis * 2);
                let sdl_sensitivity = numerator as f32 / (plus - minus) as f32;
                // SDL rejects calibration outside 0.5..1.5 times its default.
                assert!((1.0 - sdl_sensitivity / 64.0).abs() <= 0.5);
                let sdl_rad_s = (raw - bias) as f32 * sdl_sensitivity / 1024.0 * std::f32::consts::PI / 180.0;
                // mult_frac() avoids the overflowing intermediate product.
                let kernel_units = numerator as i64 * raw as i64
                    / ((plus - bias).abs() + (minus - bias).abs()) as i64;
                assert!((sdl_rad_s.to_degrees() - expected_gyro[axis]).abs() < 0.001);
                assert!((kernel_units as f32 / 1024.0 - expected_gyro[axis]).abs() < 0.001);
                let plus = i16_at(&cal, 23 + axis * 4);
                let minus = i16_at(&cal, 25 + axis * 4);
                let bias = plus - (plus - minus) / 2;
                let raw = i16_at(&report, 22 + axis * 2);
                let sdl_m_s2 = (raw - bias) as f32 * (2.0 * 8192.0 / (plus - minus) as f32) / 8192.0 * 9.80665;
                let kernel_units = 2 * 8192 * (raw - bias) / (plus - minus);
                assert!((sdl_m_s2 / 9.80665 - accel_g[axis]).abs() < 0.0001);
                assert_eq!(kernel_units as f32 / 8192.0, accel_g[axis]);
            }
        }
    }

    #[test]
    fn sensor_encoding_saturates_and_neutralizes_nan() {
        assert_eq!(sensor_raw(f32::INFINITY, 16.0), i16::MAX);
        assert_eq!(sensor_raw(f32::NEG_INFINITY, 16.0), i16::MIN);
        assert_eq!(sensor_raw(f32::NAN, 16.0), 0);
        assert_eq!(sensor_raw(0.03125, 16.0), 1);
    }

    #[test]
    fn rumble_uses_validity_and_left_right_order() {
        let mut report = [0; 63];
        report[..5].copy_from_slice(&[2, 3, 0, 0x33, 0x99]);
        assert_eq!(parse_rumble(&report), Some([0x99, 0x33]));
        report[1] = 2;
        assert_eq!(parse_rumble(&report), None);
        report[39] = 4;
        assert_eq!(parse_rumble(&report), Some([0x99, 0x33]));
        report[39] = 0;
        report[1] = 0;
        assert_eq!(parse_rumble(&report[..48]), Some([0x99, 0x33]));
        assert_eq!(parse_rumble(&report), None);
        assert_eq!(parse_rumble(&report[..4]), None);
        report[0] = 0x31;
        assert_eq!(parse_rumble(&report), None);
    }

    #[test]
    fn feature_requests_reply_with_exact_size_id_and_unknown_errors() {
        let shared = Shared { state: Mutex::new(Default::default()), rumble: Mutex::new(None), stop: AtomicBool::new(false) };
        let mac = [1, 2, 3, 4, 5, 6];
        let mut event = [0; 16];
        event[..4].copy_from_slice(&UHID_GET_REPORT.to_ne_bytes());
        event[4..8].copy_from_slice(&0xabcdu32.to_ne_bytes());
        for (id, size) in [(5, 41), (9, 20), (0x20, 64)] {
            event[8] = id;
            let (reply, len) = control_reply(&event, mac, &shared);
            assert_eq!(len, size + 12);
            assert_eq!(&reply[4..8], &event[4..8]);
            assert_eq!(&reply[8..10], &[0, 0]);
            assert_eq!(u16::from_ne_bytes(reply[10..12].try_into().unwrap()) as usize, size);
            assert_eq!(reply[12], id);
            if id == 9 { assert_eq!(&reply[13..19], &mac); }
        }
        event[8] = 3; // third-party capabilities not used by Sony SDL path
        let (reply, len) = control_reply(&event, mac, &shared);
        assert_eq!(len, 12);
        assert_ne!(&reply[8..10], &[0, 0]);
        event[..4].copy_from_slice(&UHID_SET_REPORT.to_ne_bytes());
        let (reply, len) = control_reply(&event, mac, &shared);
        assert_eq!(len, 10);
        assert_ne!(&reply[8..10], &[0, 0]);
    }
}
