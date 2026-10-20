//! Wired 2dc8:202f configuration; authority: reconstruction/specs/spec-gip.md.
//!
//! Observed Windows: SendMessageId(P60, 0x13), replies 0x12, header at payload
//! offset 0/data at 16, 41-byte profile chunks, version kind 0xe105, write+commit.
//! Observed xone v0.5.7/Linux: interrupt endpoints on interface 0, guarded usbfs
//! disconnect-claim, audio interface 1 must be released before reconnecting 0.
//! Candidate, NOT captured Windows traffic: outgoing flags 0, own nonzero sequence,
//! one 64-byte packet per P60. No alternate flags/retries on failure. Warm takeover
//! and authentication survival require a parent-owned probe. Fresh Hello is rejected
//! safely: cold-start identify/auth/power-on is not claimed. No USB reset is sent.
//!
//! On Linux, `discover` and `battery` are passive sysfs readers. Battery exposes
//! the kernel's coarse capacity level and charging status, not a percentage.
//! Windows device access is unsupported; `battery` returns None without a backend.
//! `Gip::takeover` suspends gamepad/audio input; callers must stop active host force feedback first.
//! `read_profiles` is a direct read (does not disable reporting). Writes
//! disable/restore reporting and commit only after every
//! chunk is acknowledged. Call `close` to receive cleanup/rebind errors; Drop does
//! the same cleanup on unwinding and reports errors to stderr.
use crate::{Result, invalid, protocol};
use std::{collections::BTreeMap, path::PathBuf};
#[cfg(target_os = "linux")]
use crate::interrupted;
#[cfg(target_os = "linux")]
use std::{
    collections::VecDeque,
    ffi::CString,
    fs::{self, File, OpenOptions},
    io,
    mem::size_of,
    os::{fd::AsRawFd, unix::{fs::{OpenOptionsExt, FileTypeExt, MetadataExt}, ffi::OsStrExt}},
    path::Path,
    ptr,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{Gip, battery, discover, find};

pub const VENDOR: u16 = 0x2dc8;
pub const PRODUCT: u16 = 0x202f;
pub const VERSION_SIZE: usize = 40;
#[cfg(target_os = "linux")]
const CHUNK: usize = 41;
const ACK: u8 = 0x10;
const SYSTEM: u8 = 0x20;
const INITIAL: u8 = 0x40;
const FRAGMENT: u8 = 0x80;
#[cfg(target_os = "linux")]
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, serde::Serialize)]
pub struct Endpoint {
    pub address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Interface {
    pub number: u8,
    pub alternate: u8,
    pub active: bool,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub driver: Option<String>,
    pub endpoints: Vec<Endpoint>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct InputNode {
    pub node: PathBuf,
    pub name: String,
    pub phys: String,
    pub bus: String,
    pub vendor: String,
    pub product: String,
    pub events: String,
    pub force_feedback: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Device {
    pub sysfs: PathBuf,
    pub node: PathBuf,
    pub bus: u16,
    pub address: u16,
    pub writable: bool,
    pub interfaces: Vec<Interface>,
    pub input_nodes: Vec<InputNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Battery {
    pub level: String,
    pub status: String,
}
#[cfg(target_os = "linux")]
/// Read the selected USB device's kernel battery; unavailable sysfs data is None.
pub fn battery(device: &Device) -> Option<Battery> {
    battery_at(device, Path::new("/sys/class/power_supply"))
}
#[cfg(target_os = "linux")]
fn battery_at(device: &Device, root: &Path) -> Option<Battery> {
    let canonical = fs::canonicalize(&device.sysfs).ok()?;
    fs::read_dir(root).ok()?.filter_map(|entry| entry.ok()).find_map(|entry| {
        let path = entry.path();
        if !fs::canonicalize(path.join("device")).ok()?.starts_with(&canonical) ||
            fs::read_to_string(path.join("type")).ok()?.trim() != "Battery" {
            return None;
        }
        Some(Battery {
            level: fs::read_to_string(path.join("capacity_level")).ok()?.trim().to_owned(),
            status: fs::read_to_string(path.join("status")).ok()?.trim().to_owned(),
        })
    })
}
#[cfg(target_os = "linux")]
fn driver(path: &Path) -> Result<Option<String>> {
    match fs::read_link(path.join("driver")) {
        Ok(target) => Ok(target.file_name().map(|name| name.to_string_lossy().into_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
#[cfg(target_os = "linux")]
fn number(path: &Path, radix: u32) -> Result<u16> {
    Ok(u16::from_str_radix(fs::read_to_string(path)?.trim(), radix)?)
}
fn descriptors(bytes: &[u8]) -> Result<Vec<Interface>> {
    let mut interfaces: Vec<Interface> = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes.len() - offset < 2 || bytes[offset] < 2 || offset + bytes[offset] as usize > bytes.len() {
            return Err(invalid("Malformed cached USB descriptors"));
        }
        let item = &bytes[offset..offset + bytes[offset] as usize];
        match item[1] {
            4 if item.len() >= 9 => interfaces.push(Interface {
                number: item[2], alternate: item[3], class: item[5], subclass: item[6],
                protocol: item[7], active: false, driver: None, endpoints: Vec::new(),
            }),
            5 if item.len() >= 7 => {
                if let Some(interface) = interfaces.last_mut() {
                    interface.endpoints.push(Endpoint { address: item[2], attributes: item[3],
                        max_packet_size: u16::from_le_bytes([item[4], item[5]]) & 0x7ff, interval: item[6] });
                }
            }
            _ => {}
        }
        offset += item.len();
    }
    Ok(interfaces)
}
#[cfg(target_os = "linux")]
/// Read sysfs identity/descriptors/owners and faccessat permissions; never open USB.
pub fn discover() -> Result<Vec<Device>> {
    let mut devices = Vec::new();
    for entry in fs::read_dir("/sys/bus/usb/devices")? {
        let path = entry?.path();
        if !path.join("idVendor").exists() { continue; }
        if number(&path.join("idVendor"), 16)? != VENDOR || number(&path.join("idProduct"), 16)? != PRODUCT { continue; }
        let mut interfaces = descriptors(&fs::read(path.join("descriptors"))?)?;
        let name = path.file_name().ok_or_else(|| invalid("USB sysfs name missing"))?.to_string_lossy();
        let configuration = number(&path.join("bConfigurationValue"), 10)?;
        let mut owners = BTreeMap::new();
        for interface in &mut interfaces {
            if let std::collections::btree_map::Entry::Vacant(entry) = owners.entry(interface.number) {
                let path = path.with_file_name(format!("{name}:{configuration}.{}", interface.number));
                entry.insert((driver(&path)?, number(&path.join("bAlternateSetting"), 10)?));
            }
            let (owner, alternate) = &owners[&interface.number];
            interface.driver = owner.clone();
            interface.active = u16::from(interface.alternate) == *alternate;
        }
        if !interfaces.iter().any(|interface| interface.number == 0 &&
            (interface.class, interface.subclass, interface.protocol) == (0xff, 0x47, 0xd0)) { continue; }
        let bus = number(&path.join("busnum"), 10)?;
        let address = number(&path.join("devnum"), 10)?;
        let node = PathBuf::from(format!("/dev/bus/usb/{bus:03}/{address:03}"));
        let cpath = CString::new(node.as_os_str().as_bytes())?;
        // SAFETY: a valid NUL-terminated path; AT_EACCESS checks effective credentials/ACLs.
        let writable = unsafe { libc::faccessat(libc::AT_FDCWD, cpath.as_ptr(), libc::W_OK, libc::AT_EACCESS) } == 0;
        let canonical = fs::canonicalize(&path)?;
        let mut input_nodes = Vec::new();
        for event in fs::read_dir("/sys/class/input")? {
            let event = event?;
            if event.file_name().as_bytes().starts_with(b"event") &&
                fs::canonicalize(event.path().join("device")).is_ok_and(|device| device.starts_with(&canonical)) {
                let metadata = event.path().join("device");
                let text = |field: &str| -> Result<String> {
                    Ok(fs::read_to_string(metadata.join(field))?.trim().to_owned())
                };
                input_nodes.push(InputNode {
                    node: Path::new("/dev/input").join(event.file_name()),
                    name: text("name")?, phys: text("phys")?, bus: text("id/bustype")?,
                    vendor: text("id/vendor")?, product: text("id/product")?,
                    events: text("capabilities/ev")?, force_feedback: text("capabilities/ff")?,
                });
            }
        }
        devices.push(Device { sysfs: path, node, bus, address, writable, interfaces, input_nodes });
    }
    devices.sort_by_key(|device| (device.bus, device.address));
    Ok(devices)
}
#[cfg(target_os = "linux")]
pub fn find() -> Result<Device> {
    let mut devices = discover()?;
    match devices.len() {
        1 => Ok(devices.remove(0)),
        0 => Err(invalid("Wired 8BitDo 2dc8:202f GIP device not found")),
        _ => Err(invalid("Multiple wired 3E controllers; unplug all but one")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub kind: u8,
    pub flags: u8,
    pub sequence: u8,
    pub length: u32,
    /// Initial fragment: total message length. Continuation: payload offset.
    pub fragment: Option<u32>,
}
fn varint(value: u32, output: &mut [u8]) -> usize {
    let mut value = value;
    let mut count = 0;
    loop {
        output[count] = (value & 0x7f) as u8;
        value >>= 7;
        count += 1;
        if value == 0 { return count; }
        output[count - 1] |= 0x80;
    }
}
fn read_varint(bytes: &[u8], offset: &mut usize) -> Result<u32> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let byte = *bytes.get(*offset).ok_or_else(|| invalid("Truncated GIP varint"))?;
        *offset += 1;
        if shift == 28 && byte & 0x70 != 0 { return Err(invalid("Overflowing GIP varint")); }
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 { return Ok(value); }
    }
    // Downstream even-header padding may extend a five-byte u32 by one zero.
    if bytes.get(*offset) == Some(&0) {
        *offset += 1;
        return Ok(value);
    }
    Err(invalid("Overlong GIP varint"))
}
impl Header {
    /// Encode downstream even-length headers. Padding extends the length varint,
    /// BEFORE the fragment total/offset varint, matching xone's encoder.
    pub fn encode(self, output: &mut [u8; 16]) -> Result<usize> {
        if (self.flags & FRAGMENT != 0) != self.fragment.is_some() || self.flags & INITIAL != 0 && self.fragment.is_none() {
            return Err(invalid("Inconsistent GIP fragmentation flags"));
        }
        output[..3].copy_from_slice(&[self.kind, self.flags, self.sequence]);
        let length_bytes = varint(self.length, &mut output[3..]);
        let mut fragment_bytes = [0; 5];
        let fragment_length = self.fragment.map_or(0, |value| varint(value, &mut fragment_bytes));
        let mut end = 3 + length_bytes;
        if (end + fragment_length) % 2 != 0 {
            output[end - 1] |= 0x80;
            output[end] = 0;
            end += 1;
        }
        output[end..end + fragment_length].copy_from_slice(&fragment_bytes[..fragment_length]);
        Ok(end + fragment_length)
    }
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize)> {
        if bytes.len() < 4 { return Err(invalid("Truncated GIP header")); }
        let mut end = 3;
        let length = read_varint(bytes, &mut end)?;
        let fragment = if bytes[1] & FRAGMENT != 0 { Some(read_varint(bytes, &mut end)?) } else { None };
        if bytes[1] & INITIAL != 0 && fragment.is_none() { return Err(invalid("Initial GIP flag without fragmentation")); }
        if length as usize > bytes.len() - end { return Err(invalid("Truncated GIP packet payload")); }
        Ok((Self { kind: bytes[0], flags: bytes[1], sequence: bytes[2], length, fragment }, end))
    }
}
/// Transport ACK; sequence belongs to the incoming packet, never our TX counter.
pub fn ack_frame(header: Header, accepted: u16, remaining: u16) -> [u8; 13] {
    let mut frame = [1, SYSTEM, header.sequence, 9, 0, header.kind, SYSTEM | (header.flags & 0xf), 0, 0, 0, 0, 0, 0];
    frame[7..9].copy_from_slice(&accepted.to_le_bytes());
    frame[11..13].copy_from_slice(&remaining.to_le_bytes());
    frame
}
/// Exact candidate 13/00 header plus existing Windows P60 serializer.
pub fn command_frame(sequence: u8, channel: u8, command: &mut protocol::WindowsCommand<'_>) -> Result<[u8; 64]> {
    if sequence == 0 { return Err(invalid("GIP vendor sequence zero is reserved")); }
    let mut frame = [0; 64];
    frame[..4].copy_from_slice(&[0x13, 0, sequence, 60]);
    frame[4..].copy_from_slice(&command.gip60_buffer(channel)?);
    Ok(frame)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route { Normal, Alternate, Tolerated, Version, Status }
#[derive(Debug)]
pub struct Reply<'a> {
    pub route: Route,
    pub kind: u16,
    pub echo: u16,
    pub count: usize,
    pub total: usize,
    pub offset: usize,
    pub data: &'a [u8],
}
/// None = independent physical input, not configuration progress.
pub fn parse_reply(payload: &[u8]) -> Result<Option<Reply<'_>>> {
    if payload.starts_with(&[4, 0, 3]) { return Ok(None); }
    // Live 3E over GIP (2026-10-15): version replies are kind u8 5, opcode u16 e1, parameter u16,
    // count/total/offset u32 (17-byte header), the receiver's raw[2..19] layout.
    if payload.len() >= 17 && payload[..3] == [5, 0xe1, 0] {
        let word = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().expect("bounded")) as usize;
        let count = word(5);
        if count > payload.len() - 17 { return Err(invalid("Wired version reply count exceeds actual data")); }
        return Ok(Some(Reply { route: Route::Version, kind: 5, echo: 0xe1, count, total: word(9), offset: word(13), data: &payload[17..] }));
    }
    if payload.len() < 16 { return Err(invalid("Truncated wired vendor reply header")); }
    let header = protocol::WindowsHeader::decode(&payload[..16])?;
    let route = match header.opcode {
        4 => Route::Normal,
        0 => Route::Alternate,
        5 if header.parameter == 0x0700 => Route::Tolerated,
        5 => return Err(invalid(format!("Wired vendor status kind 5: {:04x}", header.parameter))),
        0x0137 | 0x0139 | 0x013b => Route::Status,
        0x3700 | 0x3900 | 0x3b00 if header.parameter & 0xff == 1 => Route::Status,
        _ => return Err(invalid(format!("Unsupported wired vendor reply kind {:04x}", header.opcode))),
    };
    // `count` is the acknowledged byte count; live write acks (`04 00 01 00 29 00 00 00 …`)
    // carry no data, so only data-consuming callers may require `data.len() >= count`.
    let count = header.count as usize;
    Ok(Some(Reply { route, kind: header.opcode, echo: header.parameter, count,
        total: header.total as usize, offset: header.offset as usize, data: &payload[16..] }))
}
struct Assembly { bytes: Vec<u8>, total: usize }
#[derive(Default)]
struct Reassembly { pending: BTreeMap<(u8, u8, u8), Assembly> }
impl Reassembly {
    /// Continuation whose initial fragment was never seen (began before takeover).
    fn is_orphan(&self, header: Header) -> bool {
        header.fragment.is_some() && header.flags & INITIAL == 0
            && !self.pending.contains_key(&(header.kind, header.flags & 0xf, header.sequence))
    }
    fn accept(&mut self, header: Header, payload: &[u8]) -> Result<(Option<Vec<u8>>, u16, u16)> {
        let key = (header.kind, header.flags & 0xf, header.sequence);
        let Some(position) = header.fragment else {
            return Ok((Some(payload.to_vec()), payload.len().try_into()?, 0));
        };
        if header.flags & INITIAL != 0 {
            let total = position as usize;
            if total == 0 || total > u16::MAX as usize || payload.len() > total || self.pending.contains_key(&key) || self.pending.len() >= 8 {
                return Err(invalid("Invalid initial GIP fragment"));
            }
            self.pending.insert(key, Assembly { bytes: Vec::with_capacity(total), total });
        }
        let assembly = self.pending.get_mut(&key).ok_or_else(|| invalid("GIP continuation without initial fragment"))?;
        if header.flags & INITIAL == 0 && position as usize != assembly.bytes.len() ||
            payload.len() > assembly.total - assembly.bytes.len() {
            return Err(invalid("Noncontiguous or oversized GIP fragment"));
        }
        assembly.bytes.extend_from_slice(payload);
        let accepted = assembly.bytes.len() as u16;
        let remaining = (assembly.total - assembly.bytes.len()) as u16;
        // GIP terminates a fragmented transfer with an empty completion packet.
        // Keep state after the last data chunk so it can be ACKed before completion.
        let complete = if payload.is_empty() {
            if remaining != 0 { return Err(invalid("Premature GIP fragment completion")); }
            Some(self.pending.remove(&key).expect("present").bytes)
        } else { None };
        Ok((complete, accepted, remaining))
    }
}

#[cfg(target_os = "linux")]
// Linux usbdevice_fs.h ABI. No dependency on libusb or kernel module changes.
const fn ioctl(direction: u32, number: u32, size: usize) -> libc::c_ulong {
    ((direction << 30) | ((size as u32) << 16) | (b'U' as u32) << 8 | number) as libc::c_ulong
}
#[cfg(target_os = "linux")]
#[repr(C)]
struct DisconnectClaim { interface: u32, flags: u32, driver: [u8; 256] }
#[cfg(target_os = "linux")]
#[repr(C)]
struct Driver { interface: u32, driver: [u8; 256] }
#[cfg(target_os = "linux")]
#[repr(C)]
struct NestedIoctl { interface: i32, code: i32, data: *mut libc::c_void }
#[cfg(target_os = "linux")]
#[repr(C)]
struct Urb {
    kind: u8, endpoint: u8, status: i32, flags: u32, buffer: *mut libc::c_void,
    buffer_length: i32, actual_length: i32, start_frame: i32, packets: u32,
    error_count: i32, signal: u32, context: *mut libc::c_void,
}
#[cfg(target_os = "linux")]
impl Urb {
    fn new(endpoint: u8, buffer: &mut [u8; 64]) -> Box<Self> {
        Box::new(Self { kind: 1, endpoint, status: 0, flags: 0, buffer: buffer.as_mut_ptr().cast(),
            buffer_length: 64, actual_length: 0, start_frame: 0, packets: 0, error_count: 0,
            signal: 0, context: ptr::null_mut() })
    }
}
#[cfg(target_os = "linux")]
struct Usb { file: File }
#[cfg(target_os = "linux")]
impl Usb {
    fn call<T>(&self, request: libc::c_ulong, data: &mut T) -> io::Result<i32> {
        // SAFETY: each caller supplies the struct/pointer required by this usbfs request.
        let result = unsafe { libc::ioctl(self.file.as_raw_fd(), request, data as *mut T) };
        if result < 0 { Err(io::Error::last_os_error()) } else { Ok(result) }
    }
    fn owner(&self, interface: u32) -> Result<Option<String>> {
        let mut value = Driver { interface, driver: [0; 256] };
        match self.call(ioctl(1, 8, size_of::<Driver>()), &mut value) {
            Ok(_) => Ok(Some(String::from_utf8_lossy(&value.driver[..value.driver.iter().position(|byte| *byte == 0).unwrap_or(256)]).into_owned())),
            Err(error) if error.raw_os_error() == Some(libc::ENODATA) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}
#[cfg(target_os = "linux")]
trait OwnershipIoctl {
    fn claim(&self, interface: u32) -> Result<()>;
    fn release(&self, interface: u32) -> Result<()>;
    fn connect(&self) -> Result<i32>;
}
#[cfg(target_os = "linux")]
impl OwnershipIoctl for Usb {
    fn claim(&self, interface: u32) -> Result<()> {
        let mut value = DisconnectClaim { interface, flags: 1, driver: [0; 256] };
        value.driver[..10].copy_from_slice(b"xone-wired");
        self.call(ioctl(2, 27, size_of::<DisconnectClaim>()), &mut value)?;
        Ok(())
    }
    fn release(&self, mut interface: u32) -> Result<()> {
        self.call(ioctl(2, 16, size_of::<u32>()), &mut interface)?;
        Ok(())
    }
    fn connect(&self) -> Result<i32> {
        let mut value = NestedIoctl { interface: 0, code: ioctl(0, 23, 0) as i32, data: ptr::null_mut() };
        Ok(self.call(ioctl(3, 18, size_of::<NestedIoctl>()), &mut value)?)
    }
}
#[cfg(target_os = "linux")]
struct Ownership<T: OwnershipIoctl> { usb: T, claimed: [bool; 2] }
#[cfg(target_os = "linux")]
impl<T: OwnershipIoctl> Ownership<T> {
    fn claim(&mut self, interface: usize) -> Result<()> {
        self.usb.claim(interface as u32)?;
        self.claimed[interface] = true;
        Ok(())
    }
    fn restore(&mut self) -> Result<Option<i32>> {
        let reconnect = self.claimed[0];
        let mut errors = Vec::new();
        for interface in [1, 0] {
            if self.claimed[interface] {
                self.claimed[interface] = false;
                if let Err(error) = self.usb.release(interface as u32) { errors.push(format!("release if{interface}: {error}")); }
            }
        }
        let mut result = None;
        if reconnect {
            match self.usb.connect() {
                Ok(value) => result = Some(value),
                Err(error) => errors.push(format!("CONNECT if0: {error}")),
            }
        }
        if errors.is_empty() { Ok(result) } else { Err(invalid(errors.join("; "))) }
    }
}
#[cfg(target_os = "linux")]
impl<T: OwnershipIoctl> Drop for Ownership<T> {
    fn drop(&mut self) {
        if let Err(error) = self.restore() { eprintln!("Wired ownership restore failed: {error}; physical replug may be required"); }
    }
}
#[derive(Debug, serde::Serialize)]
pub struct Rebind {
    pub connect_result: i32,
    pub interface0_driver: Option<String>,
    pub interface1_driver: Option<String>,
}
/// Windows calibration uses vendor payload bytes 20..25, never standard GIP input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalibrationKind { Sticks, Triggers }
impl CalibrationKind {
    pub fn selector(self) -> u16 { match self { Self::Sticks => 1, Self::Triggers => 2 } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stick { Left, Right }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Top, Right, Bottom, Left }
impl Direction {
    fn axis_gate(self, x: i16, y: i16) -> bool {
        let (major, minor) = match self {
            Self::Top => (-y, x), Self::Right => (x, y),
            Self::Bottom => (y, x), Self::Left => (-x, y),
        };
        major > 120 && minor.abs() <= 29
    }
    fn accepts(self, x: i16, y: i16) -> bool {
        i32::from(x).pow(2) + i32::from(y).pow(2) > 0x3f00 && self.axis_gate(x, y)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalibrationStep {
    Stick { state: u8, stick: Stick, direction: Direction },
    Trigger { state: u8, stick: Stick, value: u8 },
    Complete,
}
impl CalibrationStep {
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Stick { stick: Stick::Left, direction: Direction::Top, state: 1 } =>
                "LS: push to top edge; rotate clockwise slowly three times.",
            Self::Stick { stick: Stick::Right, direction: Direction::Top, state: 14 } =>
                "RS: push to top edge; rotate clockwise slowly three times.",
            Self::Stick { stick: Stick::Left, direction: Direction::Top, .. } => "LS: continue clockwise to top edge.",
            Self::Stick { stick: Stick::Left, direction: Direction::Right, .. } => "LS: continue clockwise to right edge.",
            Self::Stick { stick: Stick::Left, direction: Direction::Bottom, .. } => "LS: continue clockwise to bottom edge.",
            Self::Stick { stick: Stick::Left, direction: Direction::Left, .. } => "LS: continue clockwise to left edge.",
            Self::Stick { stick: Stick::Right, direction: Direction::Top, .. } => "RS: continue clockwise to top edge.",
            Self::Stick { stick: Stick::Right, direction: Direction::Right, .. } => "RS: continue clockwise to right edge.",
            Self::Stick { stick: Stick::Right, direction: Direction::Bottom, .. } => "RS: continue clockwise to bottom edge.",
            Self::Stick { stick: Stick::Right, direction: Direction::Left, .. } => "RS: continue clockwise to left edge.",
            Self::Trigger { stick: Stick::Left, value: 255, .. } => "LT: press fully (255).",
            Self::Trigger { stick: Stick::Left, .. } => "LT: release fully (0).",
            Self::Trigger { stick: Stick::Right, value: 255, .. } => "RT: press fully (255).",
            Self::Trigger { stick: Stick::Right, .. } => "RT: release fully (0).",
            Self::Complete => "Movement sequence complete; sending End once.",
        }
    }
}
pub fn calibration_axis(value: u8) -> i16 {
    i16::from(if (123..=131).contains(&value) { 127 } else { value }) - 127
}
/// Pure host progression; completion means movements accepted, not firmware success.
pub struct Calibration { kind: CalibrationKind, state: u8 }
impl Calibration {
    pub fn new(kind: CalibrationKind) -> Self { Self { kind, state: 1 } }
    pub fn step(&self) -> CalibrationStep {
        match self.kind {
            CalibrationKind::Sticks if self.state <= 26 => CalibrationStep::Stick {
                state: self.state, stick: if self.state <= 13 { Stick::Left } else { Stick::Right },
                direction: match ((self.state - 1) % 13) % 4 {
                    0 => Direction::Top, 1 => Direction::Right, 2 => Direction::Bottom, _ => Direction::Left,
                },
            },
            CalibrationKind::Triggers if self.state <= 7 => CalibrationStep::Trigger {
                state: self.state, stick: if self.state <= 4 { Stick::Left } else { Stick::Right },
                value: if self.state % 2 == 1 { 255 } else { 0 },
            },
            _ => CalibrationStep::Complete,
        }
    }
    pub fn advance(&mut self, physical: [u8; 6]) -> CalibrationStep {
        let accepted = match self.step() {
            CalibrationStep::Stick { stick, direction, .. } => {
                let offset = if stick == Stick::Left { 0 } else { 2 };
                direction.accepts(calibration_axis(physical[offset]), calibration_axis(physical[offset + 1]))
            },
            CalibrationStep::Trigger { stick, value, .. } =>
                physical[if stick == Stick::Left { 4 } else { 5 }] == value,
            CalibrationStep::Complete => false,
        };
        if accepted { self.state += 1; }
        self.step()
    }
}
#[cfg(target_os = "linux")]
struct CalibrationSession {
    selector: u16,
    report_deadline: Instant,
    physical_seen: bool,
    reporting_enabled: bool,
    started: bool,
    end_attempted: bool,
}
fn physical_values(payload: &[u8]) -> Result<Option<[u8; 6]>> {
    if !payload.starts_with(&[4, 0, 3]) { return Ok(None); }
    Ok(Some(payload.get(20..26).ok_or_else(|| invalid("Truncated wired physical input"))?
        .try_into().expect("six bytes")))
}
/// Live 3E over GIP (2026-10-16): no vendor `04 00 03` stream arrives, even after the
/// acknowledged report enable; input comes only as standard GIP input (kind 0x20, 32 bytes):
/// buttons u16, LT u16, RT u16 (0..1023), LX, LY, RX, RY i16 (Y positive = up, as xone).
/// Converted to the vendor u8 layout the Windows gates expect: sticks center 127/128 with
/// top = small Y, triggers 0..255.
fn standard_values(payload: &[u8]) -> Option<[u8; 6]> {
    if payload.len() < 14 { return None; }
    let word = |at: usize| u16::from_le_bytes([payload[at], payload[at + 1]]);
    let stick = |at: usize| ((word(at) as i16 >> 8) + 128) as u8;
    let trigger = |at: usize| (word(at).min(1023) >> 2) as u8;
    Some([stick(6), 255 - stick(8), stick(10), 255 - stick(12), trigger(2), trigger(4)])
}
/// Vendor physical (kind 0x12) or, as on the live 3E, standard input (kind 0x20).
fn input_values(kind: u8, flags: u8, payload: &[u8]) -> Result<Option<[u8; 6]>> {
    match kind {
        0x12 if flags & 0xf == 0 => physical_values(payload),
        0x20 if flags & 0xf == 0 => Ok(standard_values(payload)),
        _ => Ok(None),
    }
}
fn calibration_request(opcode: u16, selector: u16) -> Result<protocol::WindowsCommand<'static>> {
    if !matches!(selector, 1 | 2) { return Err(invalid("Calibration selector must be 1 (sticks) or 2 (triggers)")); }
    Ok(request(opcode, selector, 0, 0, 0, &[]))
}
fn calibration_ack(reply: &Reply<'_>, opcode: u16) -> Result<Option<()>> {
    if reply.route != Route::Normal || reply.echo != opcode { return Ok(None); }
    if reply.count != 0 || reply.total != 0 || reply.offset != 0 {
        return Err(invalid("Malformed wired calibration acknowledgment; completion unknown"));
    }
    Ok(Some(()))
}
#[cfg(target_os = "linux")]
struct Packet { bytes: [u8; 64], length: usize, offset: usize }
#[cfg(target_os = "linux")]
pub struct Gip {
    owner: Ownership<Usb>,
    device: Device,
    rx: Box<Urb>, tx: Box<Urb>,
    rx_buffer: Box<[u8; 64]>, tx_buffer: Box<[u8; 64]>,
    rx_pending: bool, tx_pending: bool,
    queue: VecDeque<Packet>,
    assembly: Reassembly,
    sequence: u8,
    reporting_changed: bool,
    calibration: Option<CalibrationSession>,
    physical: Option<[u8; 6]>,
    cleaning: bool,
    closed: bool,
}
#[cfg(target_os = "linux")]
impl Gip {
    /// Explicit exclusive takeover. Revalidates identity and current kernel owners.
    pub fn takeover(path: &Path) -> Result<Self> {
        let device = discover()?.into_iter().find(|device| device.node == path)
            .ok_or_else(|| invalid("Path is not a discovered wired 2dc8:202f GIP device"))?;
        let interface = device.interfaces.iter().find(|interface| interface.number == 0 && interface.active &&
            (interface.class, interface.subclass, interface.protocol) == (0xff, 0x47, 0xd0))
            .ok_or_else(|| invalid("Missing active GIP interface 0"))?;
        let endpoint = |input: bool| -> Result<u8> {
            let mut endpoints = interface.endpoints.iter().filter(|ep| ep.attributes & 3 == 3 &&
                ep.max_packet_size == 64 && (ep.address & 0x80 != 0) == input);
            let address = endpoints.next().ok_or_else(|| invalid("GIP requires 64-byte interrupt IN/OUT endpoints"))?.address;
            if endpoints.next().is_some() { return Err(invalid("Ambiguous GIP endpoints")); }
            Ok(address)
        };
        let input = endpoint(true)?;
        let output = endpoint(false)?;
        if interface.driver.as_deref() != Some("xone-wired") {
            return Err(invalid("Refusing takeover: interface 0 is not owned by xone-wired"));
        }
        let file = OpenOptions::new().read(true).write(true).custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW).open(path)?;
        let metadata = file.metadata()?;
        let expected_minor = u32::from(device.bus.checked_sub(1).ok_or_else(|| invalid("Invalid USB bus zero"))?) * 128
            + u32::from(device.address.checked_sub(1).ok_or_else(|| invalid("Invalid USB address zero"))?);
        if !metadata.file_type().is_char_device() || libc::major(metadata.rdev()) != 189 || libc::minor(metadata.rdev()) != expected_minor {
            return Err(invalid("Opened path is not the expected usbfs character device"));
        }
        let usb = Usb { file };
        if usb.owner(0)?.as_deref() != Some("xone-wired") { return Err(invalid("Interface 0 owner changed before takeover")); }
        let mut owner = Ownership { usb, claimed: [false; 2] };
        owner.claim(0)?;
        if device.interfaces.iter().any(|interface| interface.number == 1) {
            match owner.usb.owner(1)?.as_deref() {
                Some("xone-wired") => owner.claim(1)?,
                None => {}
                Some(other) => return Err(invalid(format!("Refusing to detach unexpected interface 1 owner {other}"))),
            }
        }
        let mut rx_buffer = Box::new([0; 64]);
        let mut tx_buffer = Box::new([0; 64]);
        let rx = Urb::new(input, &mut rx_buffer);
        let tx = Urb::new(output, &mut tx_buffer);
        let mut session = Self { owner, device, rx, tx, rx_buffer, tx_buffer,
            rx_pending: false, tx_pending: false, queue: VecDeque::new(), assembly: Reassembly::default(),
            sequence: 0, reporting_changed: false, calibration: None, physical: None, cleaning: false, closed: false };
        session.submit_rx()?;
        // Receive is outstanding before any OUT; bounded warm-stream observation.
        let deadline = Instant::now() + Duration::from_millis(100);
        while session.message(deadline)?.is_some() {}
        Ok(session)
    }
    fn submit_rx(&mut self) -> Result<()> {
        self.owner.usb.call(ioctl(2, 10, size_of::<Urb>()), self.rx.as_mut())?;
        self.rx_pending = true;
        Ok(())
    }
    fn reap(&mut self, blocking: bool) -> Result<bool> {
        let mut completed: *mut Urb = ptr::null_mut();
        match self.owner.usb.call(ioctl(1, if blocking { 12 } else { 13 }, size_of::<*mut Urb>()), &mut completed) {
            Err(error) if !blocking && error.raw_os_error() == Some(libc::EAGAIN) => return Ok(false),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        if ptr::eq(completed, self.rx.as_ref()) {
            self.rx_pending = false;
            if self.rx.status != 0 { return Err(io::Error::from_raw_os_error(-self.rx.status).into()); }
            let length = self.rx.actual_length;
            if !(0..=64).contains(&length) { return Err(invalid("Invalid USB IN completion length")); }
            if length != 0 {
                if self.queue.len() >= 256 { return Err(invalid("GIP receive queue overflow")); }
                self.queue.push_back(Packet { bytes: *self.rx_buffer, length: length as usize, offset: 0 });
            }
            self.submit_rx()?;
        } else if ptr::eq(completed, self.tx.as_ref()) {
            self.tx_pending = false;
            if self.tx.status != 0 { return Err(io::Error::from_raw_os_error(-self.tx.status).into()); }
            if self.tx.actual_length != self.tx.buffer_length { return Err(invalid("Short GIP USB OUT transfer")); }
        } else { return Err(invalid("Unknown usbfs URB completion")); }
        Ok(true)
    }
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > 64 || self.tx_pending { return Err(invalid("Invalid GIP output transfer")); }
        self.tx_buffer[..bytes.len()].copy_from_slice(bytes);
        self.tx.buffer_length = bytes.len() as i32;
        self.owner.usb.call(ioctl(2, 10, size_of::<Urb>()), self.tx.as_mut())?;
        self.tx_pending = true;
        let deadline = Instant::now() + TIMEOUT;
        while self.tx_pending {
            if interrupted() && !self.cleaning { return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into()); }
            if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "GIP USB OUT timeout").into()); }
            if !self.reap(false)? { std::thread::sleep(Duration::from_millis(1)); }
        }
        Ok(())
    }
    fn message(&mut self, deadline: Instant) -> Result<Option<(Header, Vec<u8>)>> {
        loop {
            if interrupted() && !self.cleaning { return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into()); }
            if Instant::now() >= deadline { return Ok(None); }
            if let Some(packet) = self.queue.pop_front() {
                let (header, size) = Header::decode(&packet.bytes[packet.offset..packet.length])?;
                let start = packet.offset + size;
                let end = start + header.length as usize;
                if end < packet.length {
                    self.queue.push_front(Packet { bytes: packet.bytes, length: packet.length, offset: end });
                }
                // Live: a fragmented message started before takeover finishes after it.
                // Its tail has no initial fragment here; drop it instead of failing.
                if self.assembly.is_orphan(header) {
                    continue;
                }
                let (complete, accepted, remaining) = self.assembly.accept(header, &packet.bytes[start..end])?;
                // xone ACKs the last nonempty fragment even without flag 10;
                // the sender then emits the empty completion packet.
                if header.flags & ACK != 0 || header.fragment.is_some() && header.length != 0 && remaining == 0 {
                    self.send(&ack_frame(header, accepted, remaining))?;
                }
                if let Some(payload) = complete {
                    if header.kind == 2 && header.flags & SYSTEM != 0 {
                        return Err(invalid("Fresh GIP Hello after takeover: cold-start identify/auth handshake unproven; restoring xone ownership"));
                    }
                    return Ok(Some((header, payload)));
                }
            } else if !self.reap(false)? { std::thread::sleep(Duration::from_millis(1)); }
        }
    }
    fn transact<T>(&mut self, channel: u8, command: &mut protocol::WindowsCommand<'_>, timeout: Duration,
        mut accept: impl FnMut(&Reply<'_>) -> Result<Option<T>>) -> Result<T> {
        self.sequence = self.sequence.wrapping_add(1).max(1);
        self.send(&command_frame(self.sequence, channel, command)?)?;
        let deadline = Instant::now() + timeout;
        while let Some((header, payload)) = self.message(deadline)? {
            if let Some(values) = input_values(header.kind, header.flags, &payload)? {
                self.physical = Some(values);
                if let Some(session) = &mut self.calibration { session.physical_seen = true; }
                continue;
            }
            if header.kind != 0x12 || header.flags & 0xf != 0 { continue; }
            eprintln!("GIP reply type={:02x} flags={:02x} sequence={:02x} payload_len={} payload={}",
                header.kind, header.flags, header.sequence, payload.len(), hex(&payload));
            if let Some(reply) = parse_reply(&payload)? {
                if let Some(value) = accept(&reply)? { return Ok(value); }
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, format!("No matching wired reply for opcode {:04x}; candidate flags 00 not retried", command.opcode)).into())
    }
    pub fn read_version(&mut self) -> Result<[u8; VERSION_SIZE]> {
        let zeros = [0; VERSION_SIZE];
        let mut command = request(0xe1, 0, VERSION_SIZE, VERSION_SIZE, 0, &zeros);
        self.transact(5, &mut command, TIMEOUT, |reply| {
            if reply.route != Route::Version { return Ok(None); }
            if reply.data.len() < VERSION_SIZE { return Err(invalid("Truncated wired version: need 40 actual data bytes")); }
            Ok(Some(reply.data[..VERSION_SIZE].try_into().expect("bounded")))
        })
    }
    /// Direct first-probe read: does NOT change vendor report mode.
    pub fn read_profiles(&mut self) -> Result<Vec<u8>> {
        let mut image = vec![0; protocol::CONFIG_SIZE];
        let mut offset = 0;
        while offset < image.len() {
            let count = CHUNK.min(image.len() - offset);
            let zeros = [0; CHUNK];
            let mut command = request(2, 0, count, image.len(), offset, &zeros[..count]);
            let received = self.transact(4, &mut command, Duration::from_secs(3), |reply| {
                if reply.route != Route::Normal || reply.echo != 2 { return Ok(None); }
                validate_progress(reply, offset, count, protocol::CONFIG_SIZE)?;
                if reply.data.len() < reply.count { return Err(invalid("Wired profile reply count exceeds actual data")); }
                image[offset..offset + reply.count].copy_from_slice(&reply.data[..reply.count]);
                Ok(Some(reply.count))
            })?;
            offset += received;
        }
        check_crc(&image)?;
        Ok(image)
    }
    fn reporting(&mut self, enabled: bool) -> Result<()> {
        let mut command = request(7, if enabled { 0x110 } else { 0 }, 0, 0, 0, &[]);
        self.transact(4, &mut command, TIMEOUT, |reply| {
            if reply.route == Route::Normal && reply.echo == 7 { Ok(Some(())) } else { Ok(None) }
        })
    }
    /// Windows GIP heartbeat (0x180cc3090 → c4d5f0, every 400 ms, no ack): `04 55 00` + zeros.
    /// Static evidence (spec-gip-paddles.md) says it unlocks vendor `04 00 03` physical reports.
    pub fn heartbeat(&mut self) -> Result<()> {
        self.sequence = self.sequence.wrapping_add(1).max(1);
        self.send(&command_frame(self.sequence, 4, &mut request(0x55, 0, 0, 0, 0, &[]))?)
    }
    /// One persistent Start attempt, 10 s vendor-ack deadline; never resend on timeout.
    /// A lost ack leaves device state unknown. There is no rollback operation.
    pub fn calibration_start(&mut self, selector: u16) -> Result<()> {
        let command = calibration_request(0x0e, selector)?;
        if self.calibration.is_some() { return Err(invalid("Calibration already started; no overlapping session permitted")); }
        self.physical = None;
        self.calibration = Some(CalibrationSession { selector, report_deadline: Instant::now() + Duration::from_secs(1),
            physical_seen: false, reporting_enabled: false, started: false, end_attempted: false });
        self.calibration_command(command, Duration::from_secs(10))?;
        self.calibration.as_mut().expect("present").started = true;
        Ok(())
    }
    /// End is only for a finished movement sequence, never cancellation or cleanup.
    pub fn calibration_end(&mut self, selector: u16) -> Result<()> {
        let command = calibration_request(0x0f, selector)?;
        let session = self.calibration.as_mut().filter(|session|
            session.selector == selector && session.started && !session.end_attempted)
            .ok_or_else(|| invalid("End requires an acknowledged matching Start and cannot be repeated"))?;
        if interrupted() { return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted before End").into()); }
        session.end_attempted = true;
        let result = self.calibration_command(command, Duration::from_secs(100));
        self.physical = None;
        result
    }
    fn calibration_report_deadline(&self, deadline: Instant) -> Instant {
        self.calibration.as_ref().filter(|session| !session.physical_seen && !session.reporting_enabled)
            .map_or(deadline, |session| deadline.min(session.report_deadline))
    }
    fn ensure_calibration_reporting(&mut self) -> Result<()> {
        if interrupted() { return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into()); }
        if self.calibration.as_ref().is_some_and(|session|
            !session.physical_seen && !session.reporting_enabled && Instant::now() >= session.report_deadline) {
            // Do not wait in reporting(): it could consume the outstanding Start ack.
            // This transient enable is sent once; reporting is never disabled here.
            self.calibration.as_mut().expect("present").reporting_enabled = true;
            self.sequence = self.sequence.wrapping_add(1).max(1);
            self.send(&command_frame(self.sequence, 4, &mut request(7, 0x110, 0, 0, 0, &[]))?)?;
        }
        Ok(())
    }
    fn calibration_command(&mut self, mut command: protocol::WindowsCommand<'_>, timeout: Duration) -> Result<()> {
        if interrupted() { return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted before calibration command").into()); }
        self.sequence = self.sequence.wrapping_add(1).max(1);
        self.send(&command_frame(self.sequence, 4, &mut command)?)?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.ensure_calibration_reporting()?;
            let Some((header, payload)) = self.message(self.calibration_report_deadline(deadline))? else { continue; };
            if let Some(values) = input_values(header.kind, header.flags, &payload)? {
                self.physical = Some(values);
                self.calibration.as_mut().expect("started").physical_seen = true;
                continue;
            }
            if header.kind != 0x12 || header.flags & 0xf != 0 { continue; }
            eprintln!("GIP calibration reply sequence={:02x} payload={}", header.sequence, hex(&payload));
            if let Some(reply) = parse_reply(&payload)? {
                if calibration_ack(&reply, command.opcode)?.is_some() { return Ok(()); }
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut,
            format!("Calibration opcode {:04x}: completion unknown (vendor acknowledgment timeout; not retried)", command.opcode)).into())
    }
    /// Diagnostic: next complete GIP message (kind, flags, payload); transport ACKs handled.
    pub fn next_message(&mut self, timeout: Duration) -> Result<Option<(u8, u8, Vec<u8>)>> {
        Ok(self.message(Instant::now() + timeout)?.map(|(header, payload)| (header.kind, header.flags, payload)))
    }
    /// Live-proven transient vendor report enable (opcode 7, parameter 0x110); waits for its ack.
    pub fn enable_reporting(&mut self) -> Result<()> { self.reporting(true) }
    /// Receive vendor physical bytes 20..25, preserving transport ACK/reassembly.
    pub fn next_physical(&mut self, timeout: Duration) -> Result<Option<[u8; 6]>> {
        if interrupted() { return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into()); }
        if let Some(values) = self.physical.take() { return Ok(Some(values)); }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.ensure_calibration_reporting()?;
            let Some((header, payload)) = self.message(self.calibration_report_deadline(deadline))? else { continue; };
            if let Some(values) = input_values(header.kind, header.flags, &payload)? {
                if let Some(session) = &mut self.calibration { session.physical_seen = true; }
                return Ok(Some(values));
            }
        }
        Ok(None)
    }
    /// Windows "Xinput Mode" enable (UWP action 0x180d347c0 → factory 0x180c87b50):
    /// opcode 0x0a, parameter 1, channel 4, count/total/offset 0, no payload. Persistent
    /// receiver setting; the device may disconnect and re-enumerate. One send, no retry.
    /// Ok means the vendor acknowledgment arrived, not that the receiver is back in 3024.
    pub fn enable_xinput_mode(&mut self) -> Result<()> {
        let mut command = xinput_mode_request();
        self.transact(4, &mut command, TIMEOUT, |reply| {
            Ok((reply.route == Route::Normal && reply.echo == 0x0a).then_some(()))
        })
    }
    /// Persistent write + commit. Caller must read back before declaring success.
    pub fn write_profiles(&mut self, image: &[u8; protocol::CONFIG_SIZE]) -> Result<()> {
        if self.calibration.is_some() { return Err(invalid("Profile writes cannot disable reporting during calibration")); }
        check_crc(image)?;
        // Mark before sending: even a timed-out disable may have reached firmware.
        self.reporting_changed = true;
        let result = (|| {
            self.reporting(false)?;
            let mut offset = 0;
            while offset < image.len() {
                let count = CHUNK.min(image.len() - offset);
                let mut command = request(1, 0, count, image.len(), offset, &image[offset..offset + count]);
                self.transact(4, &mut command, TIMEOUT, |reply| {
                    if reply.route != Route::Normal || reply.echo != 1 { return Ok(None); }
                    validate_progress(reply, offset, count, image.len())?;
                    if reply.count != count { return Err(invalid("Partial wired write acknowledgment; commit refused")); }
                    Ok(Some(()))
                })?;
                offset += count;
            }
            self.transact(4, &mut protocol::WindowsCommand::commit(), TIMEOUT, |reply| {
                Ok((reply.route == Route::Normal && reply.echo == 6).then_some(()))
            })
        })();
        let restored = self.restore_reporting();
        combine(result, restored)
    }
    fn restore_reporting(&mut self) -> Result<()> {
        if self.reporting_changed {
            // Cleanup must not be interrupted after report-disable reached firmware.
            self.cleaning = true;
            let result = (|| {
                if self.tx_pending {
                    self.cancel()?;
                    self.submit_rx()?;
                }
                self.reporting(true)
            })();
            self.cleaning = false;
            result?;
            self.reporting_changed = false;
        }
        Ok(())
    }
    fn cancel(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        for (urb, pending) in [(&mut self.rx, self.rx_pending), (&mut self.tx, self.tx_pending)] {
            if pending {
                // DISCARDURB takes the submitted URB address, not a pointer-to-pointer.
                if let Err(error) = self.owner.usb.call(ioctl(0, 11, 0), urb.as_mut()) {
                    if error.raw_os_error() != Some(libc::EINVAL) { errors.push(error.to_string()); }
                }
            }
        }
        while self.rx_pending || self.tx_pending {
            let mut completed: *mut Urb = ptr::null_mut();
            match self.owner.usb.call(ioctl(1, 12, size_of::<*mut Urb>()), &mut completed) {
                Ok(_) if ptr::eq(completed, self.rx.as_ref()) => self.rx_pending = false,
                Ok(_) if ptr::eq(completed, self.tx.as_ref()) => self.tx_pending = false,
                Ok(_) => { errors.push("Unknown URB during cleanup".into()); break; }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => { errors.push(error.to_string()); break; }
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(invalid(errors.join("; "))) }
    }
    fn cleanup(&mut self) -> Result<Rebind> {
        if self.closed { return Err(invalid("GIP session already closed")); }
        let reports = self.restore_reporting();
        let cancelled = self.cancel();
        let restored = self.owner.restore();
        self.closed = true;
        let connect_result = combine(combine(reports, cancelled), restored)?.ok_or_else(|| invalid("No GIP interface to reconnect"))?;
        // xone reprobe may reset/re-enumerate the device: rediscover instead of
        // trusting the old bus/device number. Snapshot owners for explicit handback.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let devices = match discover() {
                Ok(devices) => devices,
                Err(error) if Instant::now() < deadline => {
                    // Re-enumeration can briefly remove sysfs files mid-snapshot.
                    let _ = error;
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(error) => return Err(invalid(format!("CONNECT returned {connect_result}; handback discovery failed: {error}"))),
            };
            if let Some(device) = devices.into_iter().find(|device| device.sysfs == self.device.sysfs) {
                let binding = |number| device.interfaces.iter().find(|interface| interface.number == number).and_then(|interface| interface.driver.clone());
                let interface0_driver = binding(0);
                let interface1_driver = binding(1);
                let expected_audio = self.device.interfaces.iter().any(|interface| interface.number == 1 && interface.driver.as_deref() == Some("xone-wired"));
                if interface0_driver.as_deref() == Some("xone-wired") && (!expected_audio || interface1_driver.as_deref() == Some("xone-wired")) {
                    return Ok(Rebind { connect_result, interface0_driver, interface1_driver });
                }
            }
            if Instant::now() >= deadline { return Err(invalid(format!("CONNECT returned {connect_result}, but xone interface bindings were not restored; physical replug may be required"))); }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    pub fn close(mut self) -> Result<Rebind> { self.cleanup() }
}
#[cfg(target_os = "linux")]
impl Drop for Gip {
    fn drop(&mut self) {
        if !self.closed {
            if let Err(error) = self.cleanup() { eprintln!("Wired GIP cleanup failed: {error}"); }
        }
        // owner is declared first: its fd closes before URB/buffer fields are freed,
        // including a failed cancellation/reap path. usbfs close kills pending URBs.
    }
}
fn xinput_mode_request() -> protocol::WindowsCommand<'static> {
    request(0x0a, 1, 0, 0, 0, &[])
}
fn request(opcode: u16, parameter: u16, count: usize, total: usize, offset: usize, data: &[u8]) -> protocol::WindowsCommand<'_> {
    protocol::WindowsCommand { data_offset: 0, wire_offset: offset as u32, total: total as u32,
        count: count as u32, opcode, parameter, payload: Some(data) }
}
fn validate_progress(reply: &Reply<'_>, offset: usize, requested: usize, total: usize) -> Result<()> {
    if reply.total != total || reply.offset != offset || reply.count == 0 || reply.count > requested ||
        reply.count > total - offset { return Err(invalid("Malformed/no-progress wired profile acknowledgment")); }
    Ok(())
}
fn check_crc(image: &[u8]) -> Result<()> {
    let image: &[u8; protocol::CONFIG_SIZE] = image.try_into().map_err(|_| invalid("Wired profile image must be 868 bytes"))?;
    if crate::profile::crc(image) != u16::from_le_bytes([image[0], image[1]]) {
        // Live: when the receiver loses the controller's configuration channel it answers
        // every chunk with 0xff padding (version records also 0xff) until a power cycle.
        if image[..32].iter().all(|&byte| byte == 0xff) {
            return Err(invalid("Receiver returned an empty (0xff) profile buffer: it lost the controller's configuration channel. Power-cycle the controller and retry; nothing was saved"));
        }
        return Err(invalid("Wired profile image CRC invalid; refusing transfer/result"));
    }
    Ok(())
}
fn combine<T, U>(first: Result<T>, second: Result<U>) -> Result<U> {
    match (first, second) {
        (Ok(_), Ok(value)) => Ok(value),
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(invalid(format!("{error}; cleanup ALSO failed: {cleanup}"))),
    }
}
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::{cell::RefCell, rc::Rc};
    #[cfg(target_os = "linux")]
    struct BatterySysfs {
        root: PathBuf,
        supplies: PathBuf,
        entry: PathBuf,
        device: Device,
    }
    #[cfg(target_os = "linux")]
    impl BatterySysfs {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "ultimate-battery-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            let usb = root.join("sys/devices/pci0000:00/usb3/3-2/3-2.2");
            let gip = usb.join("3-2.2:1.0/gip0/gip0.0");
            fs::create_dir_all(&gip).unwrap();
            let sysfs = root.join("sys/bus/usb/devices/3-2.2");
            fs::create_dir_all(sysfs.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&usb, &sysfs).unwrap();
            let supplies = root.join("sys/class/power_supply");
            let entry = supplies.join("gip0.0");
            fs::create_dir_all(&entry).unwrap();
            std::os::unix::fs::symlink(&gip, entry.join("device")).unwrap();
            for (field, value) in [("type", "Battery\n"), ("capacity_level", "Low\n"), ("status", "Discharging\n")] {
                fs::write(entry.join(field), value).unwrap();
            }
            let device = Device { sysfs, node: PathBuf::from("/not-opened"), bus: 3, address: 2,
                writable: false, interfaces: Vec::new(), input_nodes: Vec::new() };
            Self { root, supplies, entry, device }
        }
    }
    #[cfg(target_os = "linux")]
    impl Drop for BatterySysfs {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn battery_selects_matching_canonical_usb_descendant() {
        let fixture = BatterySysfs::new();
        let unrelated = fixture.supplies.join("other");
        fs::create_dir_all(&unrelated).unwrap();
        std::os::unix::fs::symlink(&fixture.root, unrelated.join("device")).unwrap();
        for (field, value) in [("type", "Battery"), ("capacity_level", "Full"), ("status", "Charging")] {
            fs::write(unrelated.join(field), value).unwrap();
        }
        assert_eq!(battery_at(&fixture.device, &fixture.supplies),
            Some(Battery { level: "Low".into(), status: "Discharging".into() }));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn battery_rejects_nonmatching_device_and_non_battery_type() {
        let mut fixture = BatterySysfs::new();
        let selected = fixture.device.sysfs.clone();
        // A sibling sharing the textual prefix is not a path descendant.
        fixture.device.sysfs = fixture.root.join("sys/devices/pci0000:00/usb3/3-2/3-2");
        fs::create_dir_all(&fixture.device.sysfs).unwrap();
        assert_eq!(battery_at(&fixture.device, &fixture.supplies), None);
        fixture.device.sysfs = selected;
        fs::write(fixture.entry.join("type"), "Mains\n").unwrap();
        assert_eq!(battery_at(&fixture.device, &fixture.supplies), None);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn battery_missing_or_unreadable_sysfs_data_is_unavailable() {
        for field in ["device", "type", "capacity_level", "status"] {
            let fixture = BatterySysfs::new();
            fs::remove_file(fixture.entry.join(field)).unwrap();
            assert_eq!(battery_at(&fixture.device, &fixture.supplies), None, "{field}");
            // A directory cannot be read as a sysfs text attribute.
            fs::create_dir(fixture.entry.join(field)).unwrap();
            assert_eq!(battery_at(&fixture.device, &fixture.supplies), None, "{field}");
        }
        let fixture = BatterySysfs::new();
        assert_eq!(battery_at(&fixture.device, &fixture.root.join("absent")), None);
        fs::remove_file(&fixture.device.sysfs).unwrap();
        assert_eq!(battery_at(&fixture.device, &fixture.supplies), None);
    }
    #[test]
    fn xinput_mode_frame_matches_recovered_windows_payload() {
        // spec-receiver-mode.md: 13 00 SS 3c || 04 0a 00 01 00 || 55 zero bytes.
        let frame = command_frame(7, 4, &mut xinput_mode_request()).unwrap();
        let mut expected = [0u8; 64];
        expected[..9].copy_from_slice(&[0x13, 0, 7, 0x3c, 4, 0x0a, 0, 1, 0]);
        assert_eq!(frame, expected);
    }
    #[test]
    fn gip_header_varints_and_fragment_padding() {
        for length in [0, 60, 127, 128, 16383, 16384, 65535] {
            for fragment in [None, Some(0), Some(128), Some(65535)] {
                let header = Header { kind: 0x12, flags: if fragment.is_some() { FRAGMENT } else { 0 }, sequence: 9, length, fragment };
                let mut encoded = [0; 16];
                let size = header.encode(&mut encoded).unwrap();
                assert_eq!(size % 2, 0);
                let mut packet = encoded[..size].to_vec();
                packet.resize(size + length as usize, 0);
                assert_eq!(Header::decode(&packet).unwrap(), (header, size));
            }
        }
        for value in [0x0fff_ffff, 0x1000_0000, u32::MAX] {
            let mut encoded = [0; 6];
            let size = varint(value, &mut encoded);
            assert_eq!(read_varint(&encoded[..size], &mut 0).unwrap(), value);
            encoded[size - 1] |= 0x80;
            encoded[size] = 0;
            assert_eq!(read_varint(&encoded[..size + 1], &mut 0).unwrap(), value);
        }
        let mut buffer = [0; 16];
        let header = Header { kind: 0x12, flags: FRAGMENT | INITIAL, sequence: 1, length: 3, fragment: Some(8) };
        assert_eq!(header.encode(&mut buffer).unwrap(), 6);
        assert_eq!(&buffer[..6], &[0x12, 0xc0, 1, 0x83, 0, 8]);
        for packet in [&[0x12, 0, 1, 0x80][..], &[0x12, 0, 1, 0xff, 0xff, 0xff, 0xff, 0x10], &[0x12, 0, 1, 2, 0]] {
            assert!(Header::decode(packet).is_err());
        }
    }
    #[test]
    fn gip_fragments_and_ack_bytes() {
        let mut state = Reassembly::default();
        let initial = Header { kind: 0x12, flags: FRAGMENT | INITIAL | ACK, sequence: 0x42, length: 3, fragment: Some(8) };
        let continuation = Header { flags: FRAGMENT | ACK, length: 5, fragment: Some(3), ..initial };
        assert!(state.is_orphan(continuation), "tail of a pre-takeover message is dropped");
        assert!(!state.is_orphan(initial));
        let (message, accepted, remaining) = state.accept(initial, b"abc").unwrap();
        assert!(!state.is_orphan(continuation));
        assert!(message.is_none());
        assert_eq!(ack_frame(initial, accepted, remaining), [1, 0x20, 0x42, 9, 0, 0x12, 0x20, 3, 0, 0, 0, 5, 0]);
        let (message, accepted, remaining) = state.accept(continuation, b"defgh").unwrap();
        assert!(message.is_none());
        assert_eq!((accepted, remaining), (8, 0));
        let completion = Header { flags: FRAGMENT, length: 0, fragment: Some(8), ..initial };
        assert_eq!(state.accept(completion, b"").unwrap().0.unwrap(), b"abcdefgh");
        assert!(state.is_orphan(continuation));
        assert!(state.accept(continuation, b"again").is_err());
        assert_eq!(ack_frame(Header { flags: ACK, ..initial }, 60, 0), [1, 0x20, 0x42, 9, 0, 0x12, 0x20, 60, 0, 0, 0, 0, 0]);
    }
    #[test]
    fn gip_p60_wrapping() {
        let zeros = [0; 40];
        let frame = command_frame(7, 5, &mut request(0xe1, 0, 40, 40, 0, &zeros)).unwrap();
        assert_eq!(&frame[..9], &[0x13, 0, 7, 60, 5, 0xe1, 0, 0, 0]);
        assert_eq!(&frame[9..21], &[40, 0, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&frame[21..], &[0; 43]);
    }
    fn payload(kind: u16, echo: u16, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 16];
        bytes[..2].copy_from_slice(&kind.to_le_bytes());
        bytes[2..4].copy_from_slice(&echo.to_le_bytes());
        bytes[4..8].copy_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(data);
        bytes
    }
    #[test]
    fn live_gip_version_reply_uses_17_byte_header() {
        let bytes = protocol::decode_hex("05e1000000280000002800000000000000033a018200000114020d012000000000033c011900000000033b01360002000001ef013000000000").unwrap();
        let reply = parse_reply(&bytes).unwrap().unwrap();
        assert_eq!((reply.route, reply.count, reply.total, reply.offset, reply.data.len()), (Route::Version, 40, 40, 0, 40));
        assert_eq!(&reply.data[..4], &[3, 0x3a, 1, 0x82]);
        assert!(parse_reply(&bytes[..56]).is_err(), "count 40 needs 40 data bytes");
    }
    #[test]
    fn gip_reply_routes_and_physical_bypass() {
        for (kind, echo, route) in [(4, 2, Route::Normal), (0, 2, Route::Alternate), (5, 0x0700, Route::Tolerated),
            (0x0137, 0, Route::Status), (0x3900, 1, Route::Status)] {
            let bytes = payload(kind, echo, b"data");
            let reply = parse_reply(&bytes).unwrap().unwrap();
            assert_eq!((reply.route, reply.kind, reply.echo, reply.count), (route, kind, echo, 4));
            assert_eq!(reply.data, b"data");
        }
        assert!(parse_reply(&[4, 0, 3]).unwrap().is_none());
        assert!(parse_reply(&payload(5, 0, &[])).is_err());
        assert!(parse_reply(&payload(0x3900, 0, &[])).is_err());
        // Live write acknowledgment: count 41 acknowledged, no data bytes.
        let ack = protocol::decode_hex("04000100290000006403000000000000").unwrap();
        let reply = parse_reply(&ack).unwrap().unwrap();
        assert_eq!((reply.route, reply.echo, reply.count, reply.total, reply.offset, reply.data.len()), (Route::Normal, 1, 41, 868, 0, 0));
    }
    #[cfg(target_os = "linux")]
    struct Mock { log: Rc<RefCell<Vec<String>>>, fail_release: bool }
    #[cfg(target_os = "linux")]
    impl OwnershipIoctl for Mock {
        fn claim(&self, interface: u32) -> Result<()> { self.log.borrow_mut().push(format!("claim{interface}")); Ok(()) }
        fn release(&self, interface: u32) -> Result<()> {
            self.log.borrow_mut().push(format!("release{interface}"));
            if self.fail_release { Err(invalid("release error")) } else { Ok(()) }
        }
        fn connect(&self) -> Result<i32> { self.log.borrow_mut().push("connect0".into()); Ok(1) }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn gip_restore_order_on_success_error_and_panic() {
        for fail_release in [false, true] {
            let log = Rc::new(RefCell::new(Vec::new()));
            let mut owner = Ownership { usb: Mock { log: log.clone(), fail_release }, claimed: [false; 2] };
            owner.claim(0).unwrap(); owner.claim(1).unwrap();
            assert_eq!(owner.restore().is_err(), fail_release);
            drop(owner);
            assert_eq!(*log.borrow(), ["claim0", "claim1", "release1", "release0", "connect0"]);
        }
        let log = Rc::new(RefCell::new(Vec::new()));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _owner = Ownership { usb: Mock { log: log.clone(), fail_release: false }, claimed: [true; 2] };
            panic!("simulate transaction panic");
        }));
        assert_eq!(*log.borrow(), ["release1", "release0", "connect0"]);
    }
    #[test]
    fn gip_descriptor_endpoints_are_not_guessed() {
        let bytes = [9, 4, 0, 0, 2, 0xff, 0x47, 0xd0, 0, 7, 5, 0x83, 3, 64, 0, 4, 7, 5, 2, 3, 64, 0, 4];
        let interfaces = descriptors(&bytes).unwrap();
        assert_eq!(interfaces[0].endpoints.iter().map(|ep| ep.address).collect::<Vec<_>>(), [0x83, 2]);
        assert!(descriptors(&bytes[..bytes.len()-1]).is_err());
    }
    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn gip_usbfs_abi() {
        assert_eq!(size_of::<Urb>(), 56);
        assert_eq!(std::mem::offset_of!(Urb, buffer), 16);
        assert_eq!(std::mem::offset_of!(Urb, context), 48);
        assert_eq!(size_of::<DisconnectClaim>(), 264);
        assert_eq!(size_of::<NestedIoctl>(), 16);
        assert_eq!(ioctl(2, 10, size_of::<Urb>()), 0x8038550a);
        assert_eq!(ioctl(1, 13, size_of::<*mut Urb>()), 0x4008550d);
        assert_eq!(ioctl(2, 27, size_of::<DisconnectClaim>()), 0x8108551b);
        assert_eq!(ioctl(3, 18, size_of::<NestedIoctl>()), 0xc0105512);
        assert_eq!(ioctl(0, 23, 0), 0x5517);
    }
}

#[cfg(test)]
mod calibration_tests {
    use super::*;

    #[test]
    fn start_end_frames_both_selectors_are_exactly_64_bytes() {
        for selector in [1, 2] {
            for opcode in [0x0e, 0x0f] {
                let frame = command_frame(0x42, 4, &mut calibration_request(opcode, selector).unwrap()).unwrap();
                let mut expected = [0; 64];
                expected[..9].copy_from_slice(&[0x13, 0, 0x42, 0x3c, 4, opcode as u8, 0, selector as u8, 0]);
                assert_eq!(frame, expected, "opcode={opcode:x} selector={selector}");
            }
        }
        for selector in [0, 3, u16::MAX] { assert!(calibration_request(0x0e, selector).is_err()); }
    }

    #[test]
    fn calibration_ack_requires_exact_zero_progress_and_matching_opcode() {
        for opcode in [0x0e, 0x0f] {
            let mut bytes = [0; 60];
            bytes[..4].copy_from_slice(&[4, 0, opcode as u8, 0]);
            assert_eq!(calibration_ack(&parse_reply(&bytes).unwrap().unwrap(), opcode).unwrap(), Some(()));
            assert_eq!(calibration_ack(&parse_reply(&bytes).unwrap().unwrap(), opcode ^ 1).unwrap(), None);
            for offset in [4, 8, 12] {
                bytes[offset] = 1;
                assert!(calibration_ack(&parse_reply(&bytes).unwrap().unwrap(), opcode).is_err());
                bytes[offset] = 0;
            }
            bytes[0] = 0; // Alternate route is not the required kind-4 completion.
            assert_eq!(calibration_ack(&parse_reply(&bytes).unwrap().unwrap(), opcode).unwrap(), None);
        }
        assert!(parse_reply(&[4, 0, 0x0e, 0]).is_err());
    }

    #[test]
    fn live_standard_gip_input_feeds_calibration_gates() {
        // Captured 2026-10-16: LT fully pressed (0x3ff), kind 0x20, 32 bytes.
        let mut lt = protocol::decode_hex("0000ff0300000000000000000000000000000000000000000000000000000000").unwrap();
        assert_eq!(input_values(0x20, 0, &lt).unwrap(), Some([128, 127, 128, 127, 255, 0]));
        lt[2..4].copy_from_slice(&[0, 0]);
        lt[4..6].copy_from_slice(&1023u16.to_le_bytes());
        assert_eq!(input_values(0x20, 0, &lt).unwrap().unwrap()[4..], [0, 255]);
        // Stick up (GIP +Y) must read as top (small vendor Y), left as small X.
        let mut stick = [0u8; 32];
        stick[6..8].copy_from_slice(&(-32768i16).to_le_bytes());
        stick[8..10].copy_from_slice(&32767i16.to_le_bytes());
        let values = input_values(0x20, 0, &stick).unwrap().unwrap();
        assert_eq!((values[0], values[1]), (0, 0));
        assert_eq!(input_values(0x20, 0x01, &stick).unwrap(), None, "secondary-client messages are not this pad's input");
        assert_eq!(input_values(0x03, 0, &stick).unwrap(), None);
    }
    #[test]
    fn physical_extraction_uses_vendor_payload_offsets_not_gip_or_ble_offsets() {
        let mut payload = [0xaa; 60];
        payload[..3].copy_from_slice(&[4, 0, 3]);
        payload[20..26].copy_from_slice(&[0, 255, 123, 131, 255, 0]);
        assert_eq!(physical_values(&payload).unwrap(), Some([0, 255, 123, 131, 255, 0]));
        assert!(physical_values(&payload[..25]).is_err());
        assert_eq!(physical_values(&[0x20, 0, 3]).unwrap(), None);
        assert!(parse_reply(&payload).unwrap().is_none(), "physical input must never acknowledge calibration");
        let header = Header { kind: 0x12, flags: ACK, sequence: 0x35, length: 60, fragment: None };
        let (message, accepted, remaining) = Reassembly::default().accept(header, &payload).unwrap();
        assert_eq!(physical_values(&message.unwrap()).unwrap(), Some([0, 255, 123, 131, 255, 0]));
        assert_eq!(ack_frame(header, accepted, remaining), [1, 0x20, 0x35, 9, 0, 0x12, 0x20, 60, 0, 0, 0, 0, 0]);
    }

    fn oriented(direction: Direction, major: i16, minor: i16) -> (i16, i16) {
        match direction {
            Direction::Top => (minor, -major), Direction::Right => (major, minor),
            Direction::Bottom => (minor, major), Direction::Left => (-major, minor),
        }
    }

    #[test]
    fn stick_gates_cover_radius_major_minor_sign_and_center_coercion() {
        for direction in [Direction::Top, Direction::Right, Direction::Bottom, Direction::Left] {
            for minor in [-30, -29, 0, 29, 30] {
                for major in [-127, 120, 121, 127] {
                    let (x, y) = oriented(direction, major, minor);
                    assert_eq!(direction.axis_gate(x, y), major > 120 && minor.abs() <= 29);
                }
            }
            let (x, y) = oriented(direction, 127, 0);
            assert_eq!(i32::from(x).pow(2) + i32::from(y).pow(2), 16129);
            assert!(direction.accepts(x, y), "16129 > 0x3f00, not >= 16130");
            let (x, y) = oriented(direction, 127, 1);
            assert_eq!(i32::from(x).pow(2) + i32::from(y).pow(2), 16130);
            assert!(direction.accepts(x, y));
            for (major, minor, expected) in [(126, 0, false), (120, 29, false), (121, 29, false),
                (124, 29, true), (127, 29, true), (127, -29, true), (127, 30, false), (127, -30, false)] {
                let (x, y) = oriented(direction, major, minor);
                assert_eq!(direction.accepts(x, y), expected, "{direction:?} {major} {minor}");
            }
        }
        for value in 0..=255 {
            assert_eq!(calibration_axis(value), if (123..=131).contains(&value) { 0 } else { i16::from(value) - 127 });
        }
        assert_eq!([122, 123, 127, 131, 132].map(calibration_axis), [-5, 0, 0, 0, 5]);
    }

    #[test]
    fn sticks_require_all_26_ordered_cardinals_then_complete_once() {
        let mut calibration = Calibration::new(CalibrationKind::Sticks);
        let cardinals = [[127, 0], [255, 127], [127, 255], [0, 127]];
        for state in 1..=26 {
            let stick = if state <= 13 { Stick::Left } else { Stick::Right };
            let index = ((state - 1) % 13 % 4) as usize;
            let directions = [Direction::Top, Direction::Right, Direction::Bottom, Direction::Left];
            let step = CalibrationStep::Stick { state, stick, direction: directions[index] };
            assert_eq!(calibration.step(), step);
            assert!(!step.prompt().is_empty());
            assert_eq!(calibration.advance([123, 131, 131, 123, 0, 0]), step, "center coercion cannot advance");
            for wrong in 1..4 {
                let mut sample = [127, 127, 127, 127, 0, 0];
                let offset = if stick == Stick::Left { 0 } else { 2 };
                sample[offset..offset + 2].copy_from_slice(&cardinals[(index + wrong) % 4]);
                assert_eq!(calibration.advance(sample), step, "out-of-order state {state}");
            }
            let offset = if stick == Stick::Left { 0 } else { 2 };
            let mut other_stick = [127, 127, 127, 127, 0, 0];
            other_stick[2 - offset..4 - offset].copy_from_slice(&cardinals[index]);
            assert_eq!(calibration.advance(other_stick), step, "wrong stick");
            let mut sample = [127, 127, 127, 127, 0, 0];
            sample[offset..offset + 2].copy_from_slice(&cardinals[index]);
            calibration.advance(sample);
        }
        assert_eq!(calibration.step(), CalibrationStep::Complete);
        assert_eq!(calibration.advance([0; 6]), CalibrationStep::Complete);
    }

    #[test]
    fn triggers_require_exact_lt_255_0_255_0_then_rt_255_0_255() {
        let mut calibration = Calibration::new(CalibrationKind::Triggers);
        for (index, (trigger, value)) in [(4, 255), (4, 0), (4, 255), (4, 0), (5, 255), (5, 0), (5, 255)].into_iter().enumerate() {
            let step = CalibrationStep::Trigger { state: index as u8 + 1,
                stick: if trigger == 4 { Stick::Left } else { Stick::Right }, value };
            assert_eq!(calibration.step(), step);
            assert!(!step.prompt().is_empty());
            for wrong in 0..=255 {
                if wrong == value { continue; }
                let mut sample = [127, 127, 127, 127, 255 - value, 255 - value];
                sample[trigger] = wrong;
                sample[9 - trigger] = value;
                assert_eq!(calibration.advance(sample), step, "wrong endpoint or wrong trigger");
            }
            let mut sample = [127, 127, 127, 127, 0, 0];
            sample[trigger] = value;
            calibration.advance(sample);
        }
        assert_eq!(calibration.step(), CalibrationStep::Complete);
        assert_eq!(calibration.advance([255; 6]), CalibrationStep::Complete);
    }
}
