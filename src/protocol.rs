//! Offline codecs; this module never opens a controller transport.
//! Mobile APK 2.00 SHA256 7f225242d9c899b8b9c316ef3442c0679ecc0d6160cfe2e87fe89bb809e5f1e2:
//! libadvancelib.so Generate_CustomDataCmd 0x27280, wrapper 0x28080,
//! receive JNI 0x28c90, report factory 0x2d590, motion factory 0x2bf00.
//! Windows UWPAdvance.dll 1.4.9.0 SHA256
//! 1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352:
//! motion encoder 0x180c8a050, decoder 0x180c8c840, commit 0x180c7fc50.
//! Windows state normalization is not firmware validation or an inverse-hold mode.

use crate::{Result, invalid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub const CONFIG_SIZE: usize = 868;
pub const PACKET_SIZE: usize = 62;
const CHUNK_SIZE: usize = 43;
const MOTION_FLAG: u32 = 0x2020_1123;

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded codec field"),
    )
}
fn header(out: &mut [u8], opcode: u16, parameter: u16, count: u32, total: u32, offset: u32) {
    out[..2].copy_from_slice(&opcode.to_le_bytes());
    out[2..4].copy_from_slice(&parameter.to_le_bytes());
    out[4..8].copy_from_slice(&count.to_le_bytes());
    out[8..12].copy_from_slice(&total.to_le_bytes());
    out[12..16].copy_from_slice(&offset.to_le_bytes());
}
fn payload_crc(payload: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &byte in payload {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xa001 } else { 0 };
        }
    }
    crc
}
fn packet(
    opcode: u16,
    parameter: u16,
    count: u32,
    total: u32,
    offset: u32,
    payload: &[u8],
    crc: bool,
) -> [u8; PACKET_SIZE] {
    // All callers provide either zero or exactly twelve payload bytes.
    let mut out = [0; PACKET_SIZE];
    out[..3].copy_from_slice(&[0x81, (17 + payload.len()) as u8, 4]);
    let count = count
        | if crc {
            u32::from(payload_crc(payload)) << 16
        } else {
            0
        };
    header(&mut out[3..19], opcode, parameter, count, total, offset);
    out[19..19 + payload.len()].copy_from_slice(payload);
    out
}

pub fn report_packet(enabled: bool, crc: bool) -> [u8; PACKET_SIZE] {
    packet(7, if enabled { 0x110 } else { 0 }, 0, 0, 0, &[], crc)
}

/// Mobile command only. Does not append a full-profile save or commit.
pub fn motion_packet(slot: u8, record: &[u8], crc: bool) -> Result<[u8; PACKET_SIZE]> {
    if !(1..=3).contains(&slot) || record.len() != 12 {
        return Err(invalid(
            "Motion requires slot 1..3 and an exact 12-byte record",
        ));
    }
    Ok(packet(
        0x66,
        0,
        12,
        12,
        0xfc + u32::from(slot - 1) * 0x120,
        record,
        crc,
    ))
}

/// Sequential mobile read. RX starts at byte TWO, unlike TX's byte THREE.
/// Type1 masks count to its low byte, exactly as JNI 0x28d5f; RX CRC is not verified.
pub struct ConfigRead {
    bytes: [u8; CONFIG_SIZE],
    len: usize,
    crc: bool,
}
impl ConfigRead {
    pub fn new(crc: bool) -> Self {
        Self {
            bytes: [0; CONFIG_SIZE],
            len: 0,
            crc,
        }
    }
    pub fn complete(&self) -> bool {
        self.len == CONFIG_SIZE
    }
    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    pub fn request(&self) -> Result<[u8; PACKET_SIZE]> {
        if self.complete() {
            return Err(invalid("Profile read is already complete"));
        }
        Ok(packet(
            2,
            0,
            CHUNK_SIZE.min(CONFIG_SIZE - self.len) as u32,
            CONFIG_SIZE as u32,
            self.len as u32,
            &[],
            self.crc,
        ))
    }
    pub fn accept(&mut self, notification: &[u8]) -> Result<()> {
        if notification.len() < 18 {
            return Err(invalid("Truncated configuration reply header"));
        }
        let kind = u16_at(notification, 2);
        let command = u16_at(notification, 4);
        let mut count = u32_at(notification, 6);
        if self.crc {
            count &= 0xff;
        }
        let total = u32_at(notification, 10);
        let offset = u32_at(notification, 14);
        if command != 2 || total != CONFIG_SIZE as u32 {
            return Err(invalid("Reply does not belong to this profile read"));
        }
        if kind != 4 {
            return Err(invalid(
                "Unverified configuration response kind; backup refused",
            ));
        }
        if offset != self.len as u32 {
            return Err(invalid(
                "Duplicate or out-of-order profile fragment; backup refused",
            ));
        }
        if count == 0 || count as usize > CONFIG_SIZE - self.len {
            return Err(invalid("Invalid configuration fragment length"));
        }
        let count = count as usize;
        if notification.len() < 18 + count {
            return Err(invalid("Truncated configuration fragment payload"));
        }
        self.bytes[self.len..self.len + count].copy_from_slice(&notification[18..18 + count]);
        self.len += count;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhysicalInput {
    pub buttons: u32,
    pub left_x: u8,
    pub left_y: u8,
    pub right_x: u8,
    pub right_y: u8,
    pub left_trigger: u8,
    pub right_trigger: u8,
}

/// Observed BLE (64-byte) and receiver (32-byte) physical reports.
/// Bytes28..32 remain opaque; they are not IMU samples.
pub fn physical_input(bytes: &[u8]) -> Result<Option<PhysicalInput>> {
    if bytes.len() < 6 || bytes[2..6] != [4, 0, 3, 0] {
        return Ok(None);
    }
    let header_valid = match bytes.len() {
        64 => bytes[6..18] == [0; 12],
        32 => bytes[6] == 12 && bytes[7..18] == [0; 11],
        _ => false,
    };
    if !header_valid || bytes[..2] != [0x81, 0x1c] {
        return Err(invalid("Malformed Ultimate 3E physical-input report"));
    }
    Ok(Some(PhysicalInput {
        buttons: u32_at(bytes, 18),
        left_x: bytes[22],
        left_y: bytes[23],
        right_x: bytes[24],
        right_y: bytes[25],
        left_trigger: bytes[26],
        right_trigger: bytes[27],
    }))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

/// Strict contiguous hexadecimal: no whitespace, prefixes or ignored characters.
pub fn decode_hex(text: &str) -> Result<Vec<u8>> {
    fn digit(byte: u8) -> Result<u8> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(invalid("Nonhexadecimal character")),
        }
    }
    if text.len() % 2 != 0 {
        return Err(invalid("Odd hexadecimal length"));
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks_exact(2) {
        out.push(digit(pair[0])? << 4 | digit(pair[1])?);
    }
    Ok(out)
}

/// Wire offsets, not Windows managed feature offsets0x5c/0xfc/0x19c.
/// Read-only inspection preserves every opaque field and never asserts CRC validity.
pub fn inspect_config(data: &[u8]) -> Result<Value> {
    if data.len() != CONFIG_SIZE {
        return Err(invalid(format!(
            "Expected exactly {CONFIG_SIZE} profile bytes, got {}",
            data.len()
        )));
    }
    let profiles: Vec<Value> = (0..3).map(|slot| {
        let offset = 0xfc + slot * 0x120;
        let flag = u32_at(data, offset);
        let feature = data[offset + 4];
        let sensitivity = data[offset + 6];
        let key = u32_at(data, offset + 8);
        json!({"slot": slot + 1, "motion": {
            "enabled": flag == MOTION_FLAG,
            "flag": format!("0x{flag:08x}"),
            "feature": format!("0x{feature:02x}"),
            "target": match feature & 3 { 0 => "left-stick", 1 => "right-stick", 2 => "mouse", _ => "unknown" },
            "activation": if feature & 4 != 0 { "press-toggle" } else { "hold-to-enable" },
            "activation_key": match key { 0 => "none", 8 => "View", 0x00200000 => "L4", 0x02000000 => "PL", 0x04000000 => "PR", 0x40000000 => "R4", _ => "unknown" },
            "activation_key_mask": format!("0x{key:08x}"),
            "sensitivity_raw": sensitivity,
            "sensitivity_level": if sensitivity < 6 { Some(sensitivity + 1) } else { None },
            "deadzone_compensation": data[offset + 7],
            "reserved": data[offset + 5],
            "raw_hex": hex(&data[offset..offset + 12])
        }})
    }).collect();
    Ok(json!({
        "size": data.len(), "sha256": hex(&Sha256::digest(data)),
        "stored_crc": format!("0x{:04x}", u16_at(data, 0)),
        "current_slot_raw": data[2], "profile_bits": format!("0x{:02x}", data[3]),
        "tournament_mode": data[3] & 0x10 != 0, "profiles": profiles
    }))
}

/// Exclusive0600 backup; never truncates an existing file or follows a final symlink.
pub fn save_backup(path: &Path, data: &[u8]) -> Result<()> {
    if data.len() != CONFIG_SIZE {
        return Err(invalid("Refusing to save an incomplete profile backup"));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "linux")]
    options.mode(0o600);
    let mut file = options.open(path)?;
    if let Err(error) = file.write_all(data).and_then(|()| file.sync_all()) {
        // Remove only this newly-created partial backup, matching the Python utility.
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionTarget {
    LeftStick,
    RightStick,
    Mouse,
}

/// Windows enabled-record feature state only. No managed/wire record reinterpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsMotionState {
    pub target: MotionTarget,
    pub activation: u32,
    pub map_mode: u32,
    pub x_mode: u32,
    /// Decoded bit0x08. The stock encoder forces it on regardless of this value.
    pub open: bool,
}
impl WindowsMotionState {
    pub fn decode(feature: u8) -> Self {
        Self {
            target: match feature & 3 {
                1 => MotionTarget::RightStick,
                2 => MotionTarget::Mouse,
                _ => MotionTarget::LeftStick,
            },
            activation: u32::from(feature & 4 != 0),
            map_mode: u32::from(feature & 0x10 != 0),
            x_mode: u32::from(feature & 0x20 != 0),
            open: feature & 8 != 0,
        }
    }
    pub fn encode(self) -> u8 {
        let target = match self.target {
            MotionTarget::LeftStick => 0,
            MotionTarget::RightStick => 1,
            MotionTarget::Mouse => 2,
        };
        target
            | 8
            | if self.activation == 1 { 4 } else { 0 }
            | if self.map_mode == 1 { 0x10 } else { 0 }
            | if self.x_mode == 1 { 0x20 } else { 0 }
    }
}

pub const WINDOWS_HEADER_SIZE: usize = 16;
pub const WINDOWS_HID_SIZE: usize = 32;
pub const WINDOWS_GIP_SIZE: usize = 60;

/// Primitive Windows header marshal, not a mobile header or a received reply.
/// Type size16 is supplied by the historical native marshal experiment in
/// ultimate-windows-save-analysis.json; the new dossiers establish field order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsHeader {
    pub opcode: u16,
    pub parameter: u16,
    pub count: u32,
    pub total: u32,
    pub offset: u32,
}

impl WindowsHeader {
    pub fn encode(self) -> [u8; WINDOWS_HEADER_SIZE] {
        let mut out = [0; WINDOWS_HEADER_SIZE];
        header(
            &mut out,
            self.opcode,
            self.parameter,
            self.count,
            self.total,
            self.offset,
        );
        out
    }

    /// Parses only the primitive header; does not authenticate a device response.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != WINDOWS_HEADER_SIZE {
            return Err(invalid("Windows header requires exactly 16 bytes"));
        }
        Ok(Self {
            opcode: u16_at(bytes, 0),
            parameter: u16_at(bytes, 2),
            count: u32_at(bytes, 4),
            total: u32_at(bytes, 8),
            offset: u32_at(bytes, 12),
        })
    }
}

/// Native command value: +0 data offset, +4 wire offset, +8 total, +12 count,
/// +16 opcode, +18 parameter, +24 nullable managed byte-array reference.
/// `None` is a native null payload, NOT an empty non-null array.
/// Serializers update only `count`, after constructing the header: null payload
/// leaves the marshalled request count intact but sets command count to zero.
/// Errors leave this state unchanged. No queue advancement or I/O is modeled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsCommand<'a> {
    pub data_offset: u32,
    pub wire_offset: u32,
    pub total: u32,
    pub count: u32,
    pub opcode: u16,
    pub parameter: u16,
    pub payload: Option<&'a [u8]>,
}

impl<'a> WindowsCommand<'a> {
    /// Factory0x180c7fc50, Evidence
    /// ev_cdb9e40c5bb7e25cd463cf26e2d43a35b0abd929ff0c90b9efdb58be2cde6b5b.
    /// Windows parameter0, never the Android commit parameter0x0067.
    pub const fn commit() -> Self {
        Self {
            data_offset: 0,
            wire_offset: 0,
            total: 0,
            count: 0,
            opcode: 6,
            parameter: 0,
            payload: None,
        }
    }

    fn fragment_header(&self, capacity: u32, hid: bool) -> Result<WindowsHeader> {
        let mut count = self.count.min(capacity);
        if hid {
            // Native arithmetic is unsigned32; reject wrap/underflow at the Rust
            // trust boundary rather than exposing native Array.Copy exceptions.
            let end = self
                .data_offset
                .checked_add(count)
                .ok_or_else(|| invalid("Windows HID data offset plus count overflows"))?;
            let remaining = self
                .total
                .checked_sub(self.data_offset)
                .ok_or_else(|| invalid("Windows HID data offset exceeds total"))?;
            if end > self.total {
                count = remaining;
            }
        }
        Ok(WindowsHeader {
            opcode: self.opcode,
            parameter: self.parameter,
            count,
            total: self.total,
            offset: self.wire_offset,
        })
    }

    fn fragment_payload(&self, count: u32) -> Result<&[u8]> {
        let Some(payload) = self.payload else {
            return Ok(&[]);
        };
        if count == 0 {
            return Ok(&[]);
        }
        if self.data_offset > i32::MAX as u32 {
            return Err(invalid(
                "Windows payload offset exceeds native array index range",
            ));
        }
        let start = self.data_offset as usize;
        let end = start
            .checked_add(count as usize)
            .ok_or_else(|| invalid("Windows payload range overflows"))?;
        payload
            .get(start..end)
            .ok_or_else(|| invalid("Windows payload is shorter than offset plus clamped count"))
    }

    /// Offline HID32 inner array, BEFORE the receiver's0x81 prefix/truncation.
    /// DLL SHA2561f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352:
    /// 0x180c4bf40 ev_1be32ae684f4f56abad3cb5ce718c4a8fc22a3c5809961827619415fc6e23e5c;
    /// 0x180c4c260 ev_35c2d1cdd654196cc01f3f8e8e7e9949de7c755979395e161f0671be147e9938.
    /// State32 allocates31 intermediate bytes, reserving2 plus header16 → count≤13.
    /// Unlike GIP, count also clamps to total-data_offset, NOT total-wire_offset.
    /// Channel is explicit: static initialization was not recovered.
    pub fn hid32_buffer(&mut self, channel: u8) -> Result<[u8; WINDOWS_HID_SIZE]> {
        let header = self.fragment_header(13, true)?;
        let payload = self.fragment_payload(header.count)?;
        let mut out = [0; WINDOWS_HID_SIZE];
        out[0] = (WINDOWS_HEADER_SIZE + payload.len()) as u8;
        out[1] = channel;
        out[2..18].copy_from_slice(&header.encode());
        out[18..18 + payload.len()].copy_from_slice(payload);
        self.count = payload.len() as u32;
        Ok(out)
    }

    /// Offline GIP60 device-API array, NOT established as a raw USB packet.
    /// Same DLL digest as hid32_buffer:
    /// 0x180c4aef0 ev_b52ce936fc8776bb9d10b45608d017fc15c9a5ce9b3bff9d4f88bc1fc8b5685d;
    /// 0x180c4b240 ev_3f1107aa90d45502dee14b770ea70bb5853a2b7468d6c92be8c6d83421a53320.
    /// State60 allocates59 intermediate bytes, reserving2 plus header16 → count≤41.
    /// Wrapper copies58 bytes at offset1, leaving byte59 zero; max payload ends
    /// at byte57, so bytes58/59 are zero. No remaining-total clamp exists here.
    /// Connection-selected mode remains unbound.
    pub fn gip60_buffer(&mut self, channel: u8) -> Result<[u8; WINDOWS_GIP_SIZE]> {
        let header = self.fragment_header(41, false)?;
        let payload = self.fragment_payload(header.count)?;
        let mut out = [0; WINDOWS_GIP_SIZE];
        out[0] = channel;
        out[1..17].copy_from_slice(&header.encode());
        out[17..17 + payload.len()].copy_from_slice(payload);
        self.count = payload.len() as u32;
        Ok(out)
    }

    /// Original state65 branch of 0x180c4aef0 / 0x180c4b240.
    /// Same command header,46-byte payload capacity,65-byte array handed to HID.
    /// This is the intermediate array, not an established device output report.
    pub fn gip_hid65_buffer(&mut self, channel: u8) -> Result<[u8; 65]> {
        let header = self.fragment_header(46, false)?;
        let payload = self.fragment_payload(header.count)?;
        let mut out = [0; 65];
        out[0] = channel;
        out[1..17].copy_from_slice(&header.encode());
        out[17..17 + payload.len()].copy_from_slice(payload);
        self.count = payload.len() as u32;
        Ok(out)
    }
}

/// Receiver branch0x180c4b5d5..0x180c4b61c, same DLL digest as above, Evidence
/// ev_0ee0609027dbc4c6e7bb6c9861dc9a2fbeadaf89344236e3ab4fdc104eec7a48:
/// prepend0x81, retain at most31 bytes. Other model branches differ.
/// Exactly models this output array, not OS I/O or a complete live transaction.
pub fn windows_hid_frame(inner: &[u8]) -> [u8; 32] {
    let mut out = [0; 32];
    out[0] = 0x81;
    let count = inner.len().min(31);
    out[1..1 + count].copy_from_slice(&inner[..count]);
    out
}

#[cfg(test)]
mod windows_buffer_tests {
    use super::*;

    fn command(payload: Option<&[u8]>) -> WindowsCommand<'_> {
        WindowsCommand {
            data_offset: 2,
            wire_offset: 0x1234_5678,
            total: 32,
            count: u32::MAX,
            opcode: 0x1234,
            parameter: 0xabcd,
            payload,
        }
    }

    fn vector(text: &str, size: usize) -> Vec<u8> {
        let mut bytes = decode_hex(text).unwrap();
        bytes.resize(size, 0);
        bytes
    }

    #[test]
    fn exact_buffers_preserve_fields_offsets_channels_and_padding() {
        let data: [u8; 64] = std::array::from_fn(|i| i as u8);
        let mut hid = command(Some(&data));
        let inner = hid.hid32_buffer(0x7e).unwrap();
        assert_eq!(
            inner.as_slice(),
            vector(
                "1d7e3412cdab0d000000200000007856341202030405060708090a0b0c0d0e",
                32
            )
        );
        assert_eq!(
            windows_hid_frame(&inner).as_slice(),
            vector(
                "811d7e3412cdab0d000000200000007856341202030405060708090a0b0c0d0e",
                32
            )
        );
        assert_eq!(hid.count, 13);
        assert_eq!(hid.data_offset, 2);
        assert_eq!(hid.wire_offset, 0x1234_5678);
        let mut gip = command(Some(&data));
        let buffer = gip.gip60_buffer(0x9a).unwrap();
        assert_eq!(
            buffer.as_slice(),
            vector(
                "9a3412cdab29000000200000007856341202030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a",
                60
            )
        );
        assert_eq!(gip.count, 41);
        assert_eq!(&buffer[58..], &[0, 0]);
    }

    #[test]
    fn null_payload_retains_header_count_but_zeros_state_and_hid_length() {
        let mut hid = command(None);
        assert_eq!(
            hid.hid32_buffer(0x7e).unwrap().as_slice(),
            vector("107e3412cdab0d0000002000000078563412", 32)
        );
        assert_eq!(hid.count, 0);
        let mut gip = command(None);
        assert_eq!(
            gip.gip60_buffer(0x9a).unwrap().as_slice(),
            vector("9a3412cdab290000002000000078563412", 60)
        );
        assert_eq!(gip.count, 0);
        for hid_mode in [true, false] {
            let mut empty = command(Some(&[]));
            let before = empty;
            let error = if hid_mode {
                empty.hid32_buffer(4).unwrap_err()
            } else {
                empty.gip60_buffer(4).unwrap_err()
            };
            assert_eq!(
                error.to_string(),
                "Windows payload is shorter than offset plus clamped count"
            );
            assert_eq!(empty, before);
            empty.count = 0;
            if hid_mode {
                assert_eq!(empty.hid32_buffer(4).unwrap()[0], 16);
            } else {
                assert_eq!(&empty.gip60_buffer(4).unwrap()[17..], &[0; 43]);
            }
        }
    }

    #[test]
    fn remaining_total_clamp_is_hid_only_and_uses_data_offset() {
        let data = [0x55; 64];
        let mut hid = command(Some(&data));
        hid.total = 3;
        let mut gip = hid;
        assert_eq!(
            hid.hid32_buffer(4).unwrap().as_slice(),
            vector("11043412cdab01000000030000007856341255", 32)
        );
        assert_eq!(hid.count, 1);
        let buffer = gip.gip60_buffer(4).unwrap();
        assert_eq!(WindowsHeader::decode(&buffer[1..17]).unwrap().count, 41);
        assert_eq!(&buffer[17..58], &[0x55; 41]);
        assert_eq!(gip.count, 41);
        hid.data_offset = 3;
        hid.count = 99;
        assert_eq!(
            hid.hid32_buffer(4).unwrap().as_slice(),
            vector("10043412cdab000000000300000078563412", 32)
        );
        assert_eq!(hid.count, 0);
        // GIP also allows data_offset > total; it checks the backing array only.
        gip.data_offset = 4;
        assert!(gip.gip60_buffer(4).is_ok());
    }

    #[test]
    fn full_profile_fragments_reassemble_without_crc_recomputation() {
        let mut image: [u8; CONFIG_SIZE] = std::array::from_fn(|i| (i * 7) as u8);
        image[..2].copy_from_slice(&[0x34, 0x12]);
        for hid_mode in [true, false] {
            let mut command = WindowsCommand {
                data_offset: 0,
                wire_offset: 0,
                total: CONFIG_SIZE as u32,
                count: CONFIG_SIZE as u32,
                opcode: 1,
                parameter: 0,
                payload: Some(&image),
            };
            let mut reassembled = Vec::with_capacity(CONFIG_SIZE);
            let mut fragments = 0;
            let mut last_count = 0;
            while reassembled.len() < CONFIG_SIZE {
                let (header, count) = if hid_mode {
                    let inner = command.hid32_buffer(4).unwrap();
                    let frame = windows_hid_frame(&inner);
                    let header = WindowsHeader::decode(&frame[3..19]).unwrap();
                    let count = header.count as usize;
                    assert_eq!(frame[1] as usize, 16 + count);
                    reassembled.extend_from_slice(&frame[19..19 + count]);
                    assert!(frame[19 + count..].iter().all(|&b| b == 0));
                    (header, count)
                } else {
                    let buffer = command.gip60_buffer(4).unwrap();
                    let header = WindowsHeader::decode(&buffer[1..17]).unwrap();
                    let count = header.count as usize;
                    reassembled.extend_from_slice(&buffer[17..17 + count]);
                    assert!(buffer[17 + count..].iter().all(|&b| b == 0));
                    (header, count)
                };
                assert_eq!(header.opcode, 1);
                assert_eq!(header.parameter, 0);
                assert_eq!(header.total, 868);
                assert_eq!(header.offset, command.wire_offset);
                assert_eq!(command.count, header.count);
                // Explicit caller advancement, not an emulated acknowledgment.
                command.data_offset += count as u32;
                command.wire_offset += count as u32;
                command.count = command.total - command.data_offset;
                fragments += 1;
                last_count = count;
            }
            assert_eq!(reassembled, image);
            assert_eq!(&reassembled[..2], &[0x34, 0x12]);
            assert_eq!(
                (fragments, last_count),
                if hid_mode { (67, 10) } else { (22, 7) }
            );
        }
    }

    #[test]
    fn trust_boundary_errors_are_explicit_and_do_not_mutate_commands() {
        for (offset, total, message) in [
            (33, 32, "Windows HID data offset exceeds total"),
            (
                u32::MAX,
                u32::MAX,
                "Windows HID data offset plus count overflows",
            ),
        ] {
            let mut command = command(None);
            command.data_offset = offset;
            command.total = total;
            let before = command;
            assert_eq!(command.hid32_buffer(4).unwrap_err().to_string(), message);
            assert_eq!(command, before);
        }
        let mut command = command(Some(&[0; 64]));
        command.data_offset = i32::MAX as u32 + 1;
        let before = command;
        assert_eq!(
            command.gip60_buffer(4).unwrap_err().to_string(),
            "Windows payload offset exceeds native array index range"
        );
        assert_eq!(command, before);
        for bytes in [&[0; 15][..], &[0; 17][..]] {
            assert_eq!(
                WindowsHeader::decode(bytes).unwrap_err().to_string(),
                "Windows header requires exactly 16 bytes"
            );
        }
    }

    #[test]
    fn commit_is_zero_parameter_for_both_buffers() {
        let mut command = WindowsCommand::commit();
        assert_eq!(
            command.hid32_buffer(4).unwrap().as_slice(),
            vector("100406", 32)
        );
        assert_eq!(
            command.gip60_buffer(0x7e).unwrap().as_slice(),
            vector("7e0600", 60)
        );
        assert_eq!(command.count, 0);
    }

    #[test]
    fn alternate_state65_preserves_full_payload_and_null_read_header() {
        let data: [u8; 64] = std::array::from_fn(|i| i as u8);
        let mut command = command(Some(&data));
        let frame = command.gip_hid65_buffer(0x7e).unwrap();
        assert_eq!(WindowsHeader::decode(&frame[1..17]).unwrap().count, 46);
        assert_eq!(&frame[17..63], &data[2..48]);
        assert_eq!(&frame[63..], &[0, 0]);
        assert_eq!(command.count, 46);
        command.payload = None;
        let read = command.gip_hid65_buffer(0x7e).unwrap();
        assert_eq!(WindowsHeader::decode(&read[1..17]).unwrap().count, 46);
        assert_eq!(command.count, 0);
        assert_eq!(&read[17..], &[0; 48]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn padded(text: &str, size: usize) -> Vec<u8> {
        let mut bytes = decode_hex(text).unwrap();
        bytes.resize(size, 0);
        bytes
    }
    fn reply(kind: u16, count: u32, offset: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0xaa, 0xbb];
        out.resize(18, 0);
        header(&mut out[2..], kind, 2, count, CONFIG_SIZE as u32, offset);
        out.extend_from_slice(payload);
        out
    }
    #[test]
    fn native_mobile_vectors() {
        assert_eq!(
            ConfigRead::new(false).request().unwrap().as_slice(),
            padded("811104020000002b0000006403000000000000", 62)
        );
        assert_eq!(
            ConfigRead::new(true).request().unwrap().as_slice(),
            padded("811104020000002b00ffff6403000000000000", 62)
        );
        assert_eq!(
            report_packet(true, false).as_slice(),
            padded("81110407001001000000000000000000000000", 62)
        );
        assert_eq!(
            report_packet(false, false).as_slice(),
            padded("81110407000000000000000000000000000000", 62)
        );
        let record = decode_hex("231120202900020a00000040").unwrap();
        assert_eq!(
            motion_packet(3, &record, false).unwrap().as_slice(),
            padded(
                "811d04660000000c0000000c0000003c030000231120202900020a00000040",
                62
            )
        );
        assert_eq!(
            motion_packet(1, &record, true).unwrap().as_slice(),
            padded(
                "811d04660000000c00734f0c000000fc000000231120202900020a00000040",
                62
            )
        );
        for slot in [0, 4, 255] {
            assert!(motion_packet(slot, &record, false).is_err());
        }
        assert!(motion_packet(1, &record[..11], false).is_err());
        assert!(motion_packet(1, &[0; 13], false).is_err());
    }
    #[test]
    fn rejects_ambiguous_fragments_without_mutation() {
        let first = reply(4, 43, 0, &[7; 43]);
        let invalid = [
            first.clone(),
            reply(4, 43, 43, &[0; 42]),
            reply(5, 43, 43, &[0; 43]),
            reply(4, 868, 43, &[0; 868]),
            reply(4, 0, 43, &[]),
            reply(4, u32::MAX, 43, &[]),
            vec![0; 17],
            reply(4, 1, 44, &[0]),
        ];
        let mut read = ConfigRead::new(false);
        read.accept(&first).unwrap();
        for packet in invalid {
            assert!(read.accept(&packet).is_err());
            assert_eq!(read.data(), &[7; 43]);
        }
        let mut wrong_command = reply(4, 1, 43, &[0]);
        wrong_command[4] = 3;
        assert!(read.accept(&wrong_command).is_err());
        let mut wrong_total = reply(4, 1, 43, &[0]);
        wrong_total[10] = 0;
        assert!(read.accept(&wrong_total).is_err());
    }
    #[test]
    fn reassembles_exact_opaque_image_and_masks_type1_count() {
        let image: [u8; CONFIG_SIZE] = std::array::from_fn(|i| (i * 7) as u8);
        for crc in [false, true] {
            let mut read = ConfigRead::new(crc);
            let mut frames = 0;
            while !read.complete() {
                let request = read.request().unwrap();
                let count = (u32_at(&request, 7) & 0xffff) as usize;
                let offset = u32_at(&request, 15) as usize;
                assert_eq!(count, 43.min(CONFIG_SIZE - offset));
                let encoded_count = count as u32 | if crc { 0xabcd_ff00 } else { 0 };
                read.accept(&reply(
                    4,
                    encoded_count,
                    offset as u32,
                    &image[offset..offset + count],
                ))
                .unwrap();
                frames += 1;
            }
            assert_eq!(frames, 21);
            assert_eq!(read.data(), image);
            assert!(read.request().is_err());
            assert!(read.accept(&reply(4, 1, 868, &[0])).is_err());
        }
    }
    #[test]
    fn decodes_observed_physical_report() {
        let report = padded(
            "811c040003000000000000000000000000000800004080808080000006ff",
            64,
        );
        assert_eq!(
            physical_input(&report).unwrap().unwrap(),
            PhysicalInput {
                buttons: 0x40000008,
                left_x: 128,
                left_y: 128,
                right_x: 128,
                right_y: 128,
                left_trigger: 0,
                right_trigger: 0
            }
        );
        assert!(physical_input(&report[..29]).is_err());
        let mut malformed = report;
        malformed[6] = 12;
        assert!(physical_input(&malformed).is_err());
        assert!(
            physical_input(&report_packet(true, false))
                .unwrap()
                .is_none()
        );
        assert!(physical_input(&[]).unwrap().is_none());
    }
    #[test]
    fn inspection_schema_and_sha256() {
        let zero = [0; CONFIG_SIZE];
        let result = inspect_config(&zero).unwrap();
        assert_eq!(result["size"], 868);
        assert_eq!(
            result["sha256"],
            "60b2789a68bdcbb6bbda951c3a24854e64d8f51ccdaad0d7db302b5123af1cbb"
        );
        assert_eq!(result["stored_crc"], "0x0000");
        assert_eq!(
            result["profiles"][0]["motion"],
            json!({"enabled": false, "flag": "0x00000000", "feature": "0x00", "target": "left-stick", "activation": "hold-to-enable", "activation_key": "none", "activation_key_mask": "0x00000000", "sensitivity_raw": 0, "sensitivity_level": 1, "deadzone_compensation": 0, "reserved": 0, "raw_hex": "000000000000000000000000"})
        );
        let mut image = zero;
        image[..4].copy_from_slice(&[0x34, 0x12, 255, 0x91]);
        image[0x21c..0x228].copy_from_slice(&decode_hex("23112020ffaaffcc78563412").unwrap());
        let result = inspect_config(&image).unwrap();
        assert_eq!(result["stored_crc"], "0x1234");
        assert_eq!(result["current_slot_raw"], 255);
        assert_eq!(result["profile_bits"], "0x91");
        assert_eq!(result["tournament_mode"], true);
        let motion = &result["profiles"][1]["motion"];
        assert_eq!(motion["enabled"], true);
        assert_eq!(motion["target"], "unknown");
        assert_eq!(motion["activation"], "press-toggle");
        assert_eq!(motion["activation_key"], "unknown");
        assert_eq!(motion["reserved"], 170);
        assert_eq!(motion["sensitivity_level"], Value::Null);
        assert_eq!(motion["raw_hex"], "23112020ffaaffcc78563412");
        assert!(inspect_config(&image[..867]).is_err());
        assert!(inspect_config(&[0; 869]).is_err());
    }
    #[test]
    fn backup_is_exclusive_private_and_exact() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ultimate-protocol-{}-{}.bin",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let image: [u8; CONFIG_SIZE] = std::array::from_fn(|i| i as u8);
        assert!(save_backup(&path, &image[..867]).is_err());
        assert!(!path.exists());
        save_backup(&path, &image).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(saved, image);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(save_backup(&path, &[0; CONFIG_SIZE]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), image);
        assert_eq!(
            inspect_config(&saved).unwrap(),
            inspect_config(&image).unwrap()
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn strict_hex() {
        assert_eq!(decode_hex("00aAfF12").unwrap(), [0, 170, 255, 18]);
        assert!(decode_hex("").unwrap().is_empty());
        for invalid in ["0", "abc", "0g", "aa bb", " 00", "0x12", "é"] {
            assert!(decode_hex(invalid).is_err());
        }
    }
    #[test]
    fn windows_normalization_matches_native_vectors() {
        for (input, expected) in [
            (0x21, 0x29),
            (0x29, 0x29),
            (0x2b, 0x28),
            (0x2d, 0x2d),
            (0x69, 0x29),
            (0xa9, 0x29),
            (0xe9, 0x29),
        ] {
            assert_eq!(WindowsMotionState::decode(input).encode(), expected);
        }
        let mut state = WindowsMotionState::decode(1);
        assert!(!state.open);
        assert_eq!(state.encode(), 9);
        state.activation = 1;
        assert_eq!(state.encode(), 13);
        state.activation = 2;
        assert_eq!(state.encode(), 9);
        state.map_mode = 1;
        state.x_mode = 1;
        assert_eq!(state.encode(), 0x39);
        assert_eq!(WindowsMotionState::decode(2).target, MotionTarget::Mouse);
        assert_eq!(
            WindowsMotionState::decode(3).target,
            MotionTarget::LeftStick
        );
        let changed = (0..=255u8)
            .filter(|&feature| WindowsMotionState::decode(feature).encode() != feature)
            .count();
        assert_eq!(changed, 232);
    }
    #[test]
    fn windows_commit_and_receiver_vectors() {
        let commit = WindowsCommand::commit().hid32_buffer(4).unwrap();
        assert_eq!(commit.as_slice(), padded("100406", 32));
        assert_eq!(
            windows_hid_frame(&commit).as_slice(),
            padded("81100406", 32)
        );
        assert_eq!(windows_hid_frame(&[]).as_slice(), padded("81", 32));
        assert_eq!(windows_hid_frame(&[9]).as_slice(), padded("8109", 32));
        let input: [u8; 40] = std::array::from_fn(|i| i as u8);
        let framed = windows_hid_frame(&input);
        assert_eq!(framed[0], 0x81);
        assert_eq!(framed[1..], input[..31]);
    }
}
