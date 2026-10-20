//! Lossless Ultimate 3E profile images, documented in `local://spec-profile.md`.
//!
//! Decode accepts every 868-byte image, including out-of-domain values. Private
//! newtype storage and `Unknown` enum variants retain those values without
//! advertising them as editable Windows options. UI setters validate separately.
//! `encode` preserves the stored CRC, as Windows does (spec CRC row, E-M/E-SAVE);
//! call `fix_crc` explicitly before writing an integrity-repaired image.

use crate::{Result, invalid};
use serde::{Serialize, Serializer, ser::SerializeStruct};

pub const IMAGE_SIZE: usize = 868;
pub const SLOT_SIZE: usize = 0x120;
pub const RECORD_MAGIC: u32 = 0x2020_1123;

/// Reflected CRC16, polynomial 0x8408, initial 0xffff, no xorout.
/// Spec offline CRC row: only bytes 4..868 participate, not header bytes 2/3.
pub fn crc(image: &[u8; IMAGE_SIZE]) -> u16 {
    let mut value = 0xffff;
    for &byte in &image[4..] {
        value ^= u16::from(byte);
        for _ in 0..8 {
            value = (value >> 1) ^ if value & 1 != 0 { 0x8408 } else { 0 };
        }
    }
    value
}

pub fn verify_crc(image: &[u8; IMAGE_SIZE]) -> bool {
    u16::from_le_bytes([image[0], image[1]]) == crc(image)
}

/// Live 3E (2026-10-15): right after re-enumeration the receiver returned one 13-byte
/// chunk shifted by a byte with otherwise valid headers. Every link's stored CRC has matched
/// its body, so a mismatch means a corrupted transfer: re-read once, then refuse.
pub fn checked_read(mut read: impl FnMut() -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    let mut mismatch = String::new();
    for _ in 0..2 {
        let data = read()?;
        let image: &[u8; IMAGE_SIZE] = data.as_slice().try_into().map_err(|_| invalid("Profile read returned an incomplete image"))?;
        if verify_crc(image) { return Ok(data); }
        mismatch = format!("stored 0x{:04x}, computed 0x{:04x}", u16::from_le_bytes([image[0], image[1]]), crc(image));
    }
    Err(invalid(format!("Profile read corrupted twice (CRC {mismatch}); nothing was saved or loaded")))
}

macro_rules! bounded {
    ($name:ident, $storage:ty, $min:literal, $max:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        #[serde(transparent)]
        pub struct $name($storage);
        impl $name {
            pub fn new(value: u16) -> Result<Self> {
                if ($min..=$max).contains(&value) {
                    Ok(Self(value as $storage))
                } else {
                    Err(invalid(format!("{} must be {}..={}, got {}", stringify!($name), $min, $max, value)))
                }
            }
            pub const fn get(self) -> $storage { self.0 }
            /// Imported raw values can be outside the editable domain.
            pub fn is_valid(self) -> bool { ($min..=$max).contains(&(self.0 as u16)) }
        }
    };
}
bounded!(SlotIndex, u8, 0, 2, "Current slot: 0..2; preserves imported 0xff. Spec header Current slot row, E-PLAT.");
bounded!(Percent, u8, 0, 100, "Curve coordinates: 0..100. Spec Left/Right curve rows, 0x180d07230 E-STICK.");
bounded!(Level, u8, 0, 5, "Vibration and jitter levels: 0..5, not 0..255. Spec vibration/JitterSettingView rows, E-VIB/E-CURVEBOUND.");
bounded!(Sensitivity, u8, 0, 5, "3E motion stored sensitivity: 0..5, not legacy 0..9. Spec Sensitivity row, 0x180d45f60 E-SENS.");
bounded!(LightSpeed, u16, 1, 10, "Lighting speed: 1..10. Spec Speed row, 0x180c8cb80 E-LIGHT.");
bounded!(ByteValue, u8, 0, 255, "Raw trigger, brightness, RGB, compensation values: 0..255. Spec Triggers/Color/Compensation rows.");

impl Sensitivity {
    pub const fn display(self) -> u16 { self.0 as u16 + 1 }
    /// Spec Sensitivity row: display 1..6, never the legacy 1..10.
    pub fn from_display(value: u16) -> Result<Self> {
        value.checked_sub(1).ok_or_else(|| invalid("motion sensitivity display must be 1..=6")).and_then(Self::new)
    }
}
impl ByteValue {
    pub const fn percent(self) -> u8 { (self.0 as u16 * 100 / 255) as u8 }
}

macro_rules! wire_enum {
    ($name:ident, $raw:ty, $doc:literal, $($variant:ident = $value:literal => $label:literal),+ $(,)?) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        pub enum $name { $($variant,)+ Unknown($raw) }
        impl $name {
            pub const fn from_raw(raw: $raw) -> Self {
                match raw { $($value => Self::$variant,)+ value => Self::Unknown(value) }
            }
            pub const fn raw(self) -> $raw {
                match self { $(Self::$variant => $value,)+ Self::Unknown(value) => value }
            }
            pub const fn label(self) -> &'static str {
                match self { $(Self::$variant => $label,)+ Self::Unknown(_) => "Unknown" }
            }
            fn validate(self) -> Result<Self> {
                match self {
                    Self::Unknown(_) => Err(invalid(concat!(stringify!($name), ": unknown values are import-only; see type's spec row"))),
                    value => Ok(value),
                }
            }
        }
    };
}
wire_enum!(LightEffect, u16, "Spec Effect row, E-EFFECT: exactly 1..4.",
    Off = 1 => "Close", LightTracing = 2 => "Light-tracing", FireRing = 3 => "Fire Ring", Fixed = 4 => "Fixed");
wire_enum!(StickShape, u8, "Spec Stick shape row, E-STICK/E-SHAPE: raw 3 displays Square but is not an editable enum value.",
    Circular = 0 => "Circular", Square = 1 => "Square", Default = 2 => "Default");
wire_enum!(MotionTarget, u8, "Spec motion Mapping row, E-MOTIONUI: 3 is import-only.",
    LeftStick = 0 => "Left Stick", RightStick = 1 => "Right Stick", Mouse = 2 => "Mouse");
wire_enum!(MappingTarget, u32, "Spec 17-target picker row, 0x180d0d130 E-MAP. No paddles, Home, Swap, combinations or keyboard targets.",
    A = 0x2000 => "A", B = 0x1000 => "B", X = 0x10 => "X", Y = 0x20 => "Y",
    Lb = 0x400 => "LB", Lt = 0x4000 => "LT", L3 = 2 => "L3", Rb = 0x800 => "RB",
    Rt = 0x8000 => "RT", R3 = 4 => "R3", View = 8 => "View", Menu = 1 => "Menu",
    Up = 0x200 => "Up", Down = 0x100 => "Down", Left = 0x80 => "Left", Right = 0x40 => "Right", None = 0 => "Null");
impl MappingTarget {
    pub const ALL: [Self; 17] = [Self::A, Self::B, Self::X, Self::Y, Self::Lb, Self::Lt, Self::L3, Self::Rb, Self::Rt, Self::R3, Self::View, Self::Menu, Self::Up, Self::Down, Self::Left, Self::Right, Self::None];
}
impl LightEffect { pub const ALL: [Self; 4] = [Self::Off, Self::LightTracing, Self::FireRing, Self::Fixed]; }
impl StickShape {
    pub const ALL: [Self; 3] = [Self::Default, Self::Circular, Self::Square];
    pub const fn display_label(self) -> &'static str {
        match self { Self::Unknown(3) => "Square", _ => self.label() }
    }
}
impl MotionTarget { pub const ALL: [Self; 3] = [Self::LeftStick, Self::RightStick, Self::Mouse]; }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Side { Left, Right }
impl Side {
    pub const fn index(self) -> usize { match self { Self::Left => 0, Self::Right => 1 } }
    pub const fn label(self) -> &'static str { match self { Self::Left => "Left", Self::Right => "Right" } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Axis { X, Y }
impl Axis { pub const fn label(self) -> &'static str { match self { Self::X => "X", Self::Y => "Y" } } }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum VibrationMotor { LeftGrip, RightGrip, LeftTrigger, RightTrigger }
impl VibrationMotor {
    pub const fn index(self) -> usize { match self { Self::LeftGrip => 0, Self::RightGrip => 1, Self::LeftTrigger => 2, Self::RightTrigger => 3 } }
    const fn shift(self) -> u32 { match self { Self::LeftGrip => 8, Self::RightGrip => 11, Self::LeftTrigger => 14, Self::RightTrigger => 17 } }
    pub const fn label(self) -> &'static str { match self { Self::LeftGrip => "Grip Vibration - Left", Self::RightGrip => "Grip Vibration - Right", Self::LeftTrigger => "Trigger Vibration - Left", Self::RightTrigger => "Trigger Vibration - Right" } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum StickSwap { None, LeftStickDpad, LeftStickRightStick }
impl StickSwap {
    pub const fn label(self) -> &'static str { match self { Self::None => "None", Self::LeftStickDpad => "Swap LS/D-pad", Self::LeftStickRightStick => "Swap LS/RS" } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EnableMethod { Hold, Toggle }
impl EnableMethod { pub const fn label(self) -> &'static str { match self { Self::Hold => "Press and hold to enable", Self::Toggle => "Press once to enable/disable" } } }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MotionMode { Instantaneous, Continuous }
impl MotionMode { pub const fn label(self) -> &'static str { match self { Self::Instantaneous => "Instantaneous (angular velocity)", Self::Continuous => "Continuous (acceleration)" } } }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MotionAxis { Z, X }
impl MotionAxis { pub const fn label(self) -> &'static str { match self { Self::Z => "Z axis", Self::X => "X axis" } } }

/// Physical masks, NOT XInput report bits. Spec Mapping/physical-button table,
/// E-RESET/E-BUTTON; P1/P2 avoid the contradictory PL/PR naming (spec row 122).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PhysicalButtons(u32);
impl PhysicalButtons {
    pub const NONE: Self = Self(0);
    pub const MENU: Self = Self(0x0000_0001);
    pub const L3: Self = Self(0x0000_0002);
    pub const R3: Self = Self(0x0000_0004);
    pub const VIEW: Self = Self(0x0000_0008);
    pub const X: Self = Self(0x0000_0010);
    pub const Y: Self = Self(0x0000_0020);
    pub const RIGHT: Self = Self(0x0000_0040);
    pub const LEFT: Self = Self(0x0000_0080);
    pub const DOWN: Self = Self(0x0000_0100);
    pub const UP: Self = Self(0x0000_0200);
    pub const LB: Self = Self(0x0000_0400);
    pub const RB: Self = Self(0x0000_0800);
    pub const B: Self = Self(0x0000_1000);
    pub const A: Self = Self(0x0000_2000);
    pub const LT: Self = Self(0x0000_4000);
    pub const RT: Self = Self(0x0000_8000);
    pub const L4: Self = Self(0x0020_0000);
    pub const P1: Self = Self(0x0200_0000);
    pub const P2: Self = Self(0x0400_0000);
    pub const R4: Self = Self(0x4000_0000);
    pub const KNOWN: u32 = 0x4620_ffff;
    pub const fn bits(self) -> u32 { self.0 }
    pub const fn from_bits_retain(bits: u32) -> Self { Self(bits) }
    pub const fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
    pub const fn union(self, other: Self) -> Self { Self(self.0 | other.0) }
    pub fn labels(self) -> impl Iterator<Item = &'static str> {
        PhysicalButton::ALL.into_iter().filter(move |button| self.contains(button.mask())).map(PhysicalButton::label)
    }
    fn validate_enable(self) -> Result<Self> {
        if self.0 == 0 || (self.0.count_ones() == 1 && self.0 & !Self::KNOWN == 0) { Ok(self) }
        else { Err(invalid("motion enable button must be None or one of 20 physical buttons; spec Enable-button picker, 0x180d49440 E-MOTIONUI (no Home/Swap/combinations)")) }
    }
}

/// Mapping source order is the wire array order (spec table indices 0..19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum PhysicalButton { L3, R3, L4, R4, Up, Down, Left, Right, A, B, X, Y, Lb, Rb, Lt, Rt, P1, P2, View, Menu }
impl PhysicalButton {
    pub const ALL: [Self; 20] = [Self::L3, Self::R3, Self::L4, Self::R4, Self::Up, Self::Down, Self::Left, Self::Right, Self::A, Self::B, Self::X, Self::Y, Self::Lb, Self::Rb, Self::Lt, Self::Rt, Self::P1, Self::P2, Self::View, Self::Menu];
    pub const fn index(self) -> usize { self as usize }
    pub const fn mask(self) -> PhysicalButtons {
        PhysicalButtons([2, 4, 0x0020_0000, 0x4000_0000, 0x200, 0x100, 0x80, 0x40, 0x2000, 0x1000, 0x10, 0x20, 0x400, 0x800, 0x4000, 0x8000, 0x0200_0000, 0x0400_0000, 8, 1][self.index()])
    }
    pub const fn label(self) -> &'static str {
        ["L3", "R3", "L4", "R4", "Up", "Down", "Left", "Right", "A", "B", "X", "Y", "LB", "RB", "LT", "RT", "P1", "P2", "View", "Menu"][self.index()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProfileBits(u8);
impl ProfileBits {
    pub const fn raw(self) -> u8 { self.0 }
    pub const fn tournament(self) -> bool { self.0 & 0x10 != 0 }
    pub const fn auto_reconnect(self) -> bool { self.0 & 8 != 0 }
    pub const fn polling_hz(self) -> u16 { if self.tournament() { 1000 } else { 250 } }
    /// Preserves unknown flags, unlike Windows' destructive disable action.
    pub fn set_tournament(&mut self, value: bool) { set_bit8(&mut self.0, 0x10, value); }
    pub fn set_auto_reconnect(&mut self, value: bool) { set_bit8(&mut self.0, 8, value); }
    /// Spec Tournament Mode row: 2000/4000 Hz belong to another model.
    pub fn set_polling_hz(&mut self, value: u16) -> Result<()> {
        match value { 250 => self.set_tournament(false), 1000 => self.set_tournament(true), _ => return Err(invalid("3E polling rate must be 250 or 1000 Hz; spec Tournament Mode row, E-RATE")) }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct FeatureFlags(u32);
impl FeatureFlags {
    pub const fn raw(self) -> u32 { self.0 }
    pub const fn shape(self) -> StickShape { StickShape::from_raw(((self.0 >> 20) & 3) as u8) }
    pub fn set_shape(&mut self, shape: StickShape) -> Result<()> {
        let raw = shape.validate()?.raw();
        self.0 = (self.0 & !0x0030_0000) | (u32::from(raw) << 20);
        Ok(())
    }
    pub const fn inverted(self, side: Side, axis: Axis) -> bool { self.0 & Self::invert_mask(side, axis) != 0 }
    const fn invert_mask(side: Side, axis: Axis) -> u32 {
        1 << (match side { Side::Left => 0, Side::Right => 2 } + match axis { Axis::X => 0, Axis::Y => 1 })
    }
    pub fn set_inverted(&mut self, side: Side, axis: Axis, value: bool) { set_bit32(&mut self.0, Self::invert_mask(side, axis), value); }
    pub const fn swaps_left_dpad(self) -> bool { self.0 & 0x10 != 0 }
    pub const fn swaps_sticks(self) -> bool { self.0 & 0x20 != 0 }
    /// Spec swap rows, E-STICK: the two swaps are mutually exclusive.
    pub fn set_stick_swap(&mut self, value: StickSwap) {
        self.0 = (self.0 & !0x30) | match value { StickSwap::None => 0, StickSwap::LeftStickDpad => 0x10, StickSwap::LeftStickRightStick => 0x20 };
    }
    pub const fn swap_triggers(self) -> bool { self.0 & 0x40 != 0 }
    pub fn set_swap_triggers(&mut self, value: bool) { set_bit32(&mut self.0, 0x40, value); }
    pub const fn impulse_sync(self) -> bool { self.0 & 0x80 != 0 }
    pub fn set_impulse_sync(&mut self, value: bool) { set_bit32(&mut self.0, 0x80, value); }
    pub const fn vibration(self, motor: VibrationMotor) -> Level { Level(((self.0 >> motor.shift()) & 7) as u8) }
    pub fn set_vibration(&mut self, motor: VibrationMotor, value: u16) -> Result<()> {
        let value = Level::new(value)?;
        self.0 = (self.0 & !(7 << motor.shift())) | (u32::from(value.get()) << motor.shift());
        Ok(())
    }
}

/// Raw UTF-16BE, preserving malformed surrogates and bytes after NUL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProfileName([u8; 32]);
impl ProfileName {
    pub const fn raw(&self) -> &[u8; 32] { &self.0 }
    pub fn display(&self) -> String {
        let units: [u16; 16] = std::array::from_fn(|i| u16::from_be_bytes([self.0[2 * i], self.0[2 * i + 1]]));
        let length = units.iter().position(|&unit| unit == 0).unwrap_or(16);
        String::from_utf16_lossy(&units[..length])
    }
    /// Spec Slot names / row 246: 16 UTF-16 units, not 16 Unicode scalars.
    /// NUL cannot roundtrip through the Windows name display; refuse it.
    pub fn set(&mut self, name: &str) -> Result<()> {
        if name.contains('\0') || name.encode_utf16().count() > 16 {
            return Err(invalid("name must contain no NUL and fit 16 UTF-16 code units; spec Slot names and 0x180cc78e0 name decoder"));
        }
        self.0.fill(0);
        for (i, unit) in name.encode_utf16().enumerate() { self.0[i * 2..i * 2 + 2].copy_from_slice(&unit.to_be_bytes()); }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KeyMap {
    pub flag: u32,
    pub mapping: [MappingTarget; 20],
}
impl KeyMap {
    pub fn get(&self, source: PhysicalButton) -> MappingTarget { self.mapping[source.index()] }
    pub fn set(&mut self, source: PhysicalButton, target: MappingTarget) -> Result<()> {
        self.mapping[source.index()] = target.validate()?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Color {
    pub brightness: ByteValue,
    pub red: ByteValue,
    pub green: ByteValue,
    pub blue: ByteValue,
}
impl Color {
    pub fn new(brightness: u16, red: u16, green: u16, blue: u16) -> Result<Self> {
        Ok(Self { brightness: ByteValue::new(brightness)?, red: ByteValue::new(red)?, green: ByteValue::new(green)?, blue: ByteValue::new(blue)? })
    }
    pub const fn from_packed(raw: u32) -> Self {
        Self { brightness: ByteValue(raw as u8), red: ByteValue((raw >> 8) as u8), green: ByteValue((raw >> 16) as u8), blue: ByteValue((raw >> 24) as u8) }
    }
    pub const fn packed(self) -> u32 { self.brightness.0 as u32 | ((self.red.0 as u32) << 8) | ((self.green.0 as u32) << 16) | ((self.blue.0 as u32) << 24) }
    pub const fn brightness_percent(self) -> u8 { self.brightness.percent() }
    /// Windows calls max(R,G,B) "Saturation"; this is not HSV saturation.
    pub fn saturation_percent(self) -> u8 { (u16::from(self.red.0.max(self.green.0).max(self.blue.0)) * 100 / 255) as u8 }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Lighting {
    pub flag: u32,
    pub effect: LightEffect,
    pub speed: LightSpeed,
    pub fire_ring: [Color; 2],
    pub light_tracing: [Color; 2],
    pub fixed: [Color; 24],
}
impl Lighting {
    pub fn set_effect(&mut self, effect: LightEffect) -> Result<()> { self.effect = effect.validate()?; Ok(()) }
    pub fn set_speed(&mut self, value: u16) -> Result<()> { self.speed = LightSpeed::new(value)?; Ok(()) }
    /// Spec Fixed color row: Windows fills every LED with the selected color.
    pub fn set_fixed_color(&mut self, color: Color) { self.fixed.fill(color); }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct MotionFlags(u8);
impl MotionFlags {
    pub const fn raw(self) -> u8 { self.0 }
    pub const fn target(self) -> MotionTarget { MotionTarget::from_raw(self.0 & 3) }
    pub fn set_target(&mut self, target: MotionTarget) -> Result<()> {
        self.0 = (self.0 & !3) | target.validate()?.raw(); Ok(())
    }
    pub const fn enable_method(self) -> EnableMethod { if self.0 & 4 == 0 { EnableMethod::Hold } else { EnableMethod::Toggle } }
    pub fn set_enable_method(&mut self, value: EnableMethod) { set_bit8(&mut self.0, 4, value == EnableMethod::Toggle); }
    pub const fn unlabeled_flag(self) -> bool { self.0 & 8 != 0 }
    pub const fn mode(self) -> MotionMode { if self.0 & 0x10 == 0 { MotionMode::Instantaneous } else { MotionMode::Continuous } }
    pub const fn axis(self) -> MotionAxis { if self.0 & 0x20 == 0 { MotionAxis::Z } else { MotionAxis::X } }
    // Axis/mode are imported legacy state: new 3E page has no controls for them
    // (spec Motion controls row 219), so deliberately no UI-facing setters.
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Motion {
    pub flag: u32,
    pub feature: MotionFlags,
    pub reserve: [u8; 1],
    pub sensitivity: Sensitivity,
    pub compensation: ByteValue,
    pub enable_button: PhysicalButtons,
}
impl Motion {
    pub fn set_sensitivity(&mut self, value: u16) -> Result<()> { self.sensitivity = Sensitivity::new(value)?; Ok(()) }
    pub fn set_compensation(&mut self, value: u16) -> Result<()> { self.compensation = ByteValue::new(value)?; Ok(()) }
    pub fn set_enable_button(&mut self, value: PhysicalButtons) -> Result<()> { self.enable_button = value.validate_enable()?; Ok(()) }
    pub const fn compensation_percent(&self) -> u8 { self.compensation.percent() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CurvePoint { pub input: Percent, pub output: Percent }
impl CurvePoint {
    pub fn new(input: u16, output: u16) -> Result<Self> { Ok(Self { input: Percent::new(input)?, output: Percent::new(output)? }) }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StickCurve { pub points: [CurvePoint; 4] }
impl StickCurve {
    /// Custom P1/P2 drag/D-pad must lie strictly between both neighbors on
    /// both axes. Spec strict-neighbor row 173, E-CURVEBOUND. Equal x is only
    /// authored by the Digital preset; imported values are never rejected.
    pub fn set_custom_point(&mut self, index: usize, input: u16, output: u16) -> Result<()> {
        if !(1..=2).contains(&index) { return Err(invalid("custom curve point must be P1 or P2; spec Custom interior points row")); }
        let point = CurvePoint::new(input, output)?;
        let before = self.points[index - 1];
        let after = self.points[index + 1];
        if !(before.input.0 < point.input.0 && point.input.0 < after.input.0 && before.output.0 < point.output.0 && point.output.0 < after.output.0) {
            return Err(invalid("custom point must be strictly between neighboring x/y coordinates; spec row 173, E-CURVEBOUND"));
        }
        self.points[index] = point;
        Ok(())
    }
    /// Endpoint deadzone/anti-deadzone sliders each have domain 0..100.
    pub fn set_endpoint(&mut self, high: bool, input: u16, output: u16) -> Result<()> {
        self.points[if high { 3 } else { 0 }] = CurvePoint::new(input, output)?; Ok(())
    }
    /// Spec preset table: preserve endpoints; reset differs from Default.
    pub fn apply_preset(&mut self, preset: CurvePreset) {
        let (a, b) = match preset { CurvePreset::Default => ((36, 33), (65, 66)), CurvePreset::Dynamic => ((36, 16), (65, 83)), CurvePreset::Precision => ((36, 16), (73, 66)), CurvePreset::Fast => ((15, 16), (36, 66)), CurvePreset::Digital => ((29, 16), (29, 66)), CurvePreset::Stable => ((51, 50), (73, 66)) };
        self.points[1] = CurvePoint { input: Percent(a.0), output: Percent(a.1) };
        self.points[2] = CurvePoint { input: Percent(b.0), output: Percent(b.1) };
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CurvePreset { Default, Dynamic, Precision, Fast, Digital, Stable }
impl CurvePreset {
    pub const fn label(self) -> &'static str { match self { Self::Default => "Default", Self::Dynamic => "Dynamic", Self::Precision => "Precision", Self::Fast => "Fast", Self::Digital => "Digital", Self::Stable => "Stable" } }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeadZone {
    pub flag: u32,
    pub curves: [StickCurve; 2],
    pub jitter: [Level; 2],
    pub trigger_lower: [ByteValue; 2],
    pub trigger_upper: [ByteValue; 2],
    pub reserve: [u8; 2],
}
impl DeadZone {
    pub fn set_jitter(&mut self, side: Side, value: u16) -> Result<()> { self.jitter[side.index()] = Level::new(value)?; Ok(()) }
    /// No lower<=upper check: spec row 173 explicitly marks that invariant
    /// inference, not an established Windows/firmware rejection rule.
    pub fn set_trigger_lower(&mut self, side: Side, value: u16) -> Result<()> { self.trigger_lower[side.index()] = ByteValue::new(value)?; Ok(()) }
    pub fn set_trigger_upper(&mut self, side: Side, value: u16) -> Result<()> { self.trigger_upper[side.index()] = ByteValue::new(value)?; Ok(()) }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Slot {
    pub flag: u32,
    pub name: ProfileName,
    pub key_map: KeyMap,
    pub feature_flag: u32,
    pub feature: FeatureFlags,
    pub lighting: Lighting,
    pub motion: Motion,
    pub dead_zone: DeadZone,
}
impl Slot {
    pub const fn is_valid(&self) -> bool { self.flag == RECORD_MAGIC }
    fn decode(bytes: &[u8]) -> Self {
        Self {
            flag: read32(bytes, 0), name: ProfileName(bytes[4..0x24].try_into().unwrap()),
            key_map: KeyMap { flag: read32(bytes, 0x24), mapping: std::array::from_fn(|i| MappingTarget::from_raw(read32(bytes, 0x28 + i * 4))) },
            feature_flag: read32(bytes, 0x78), feature: FeatureFlags(read32(bytes, 0x7c)),
            lighting: Lighting {
                flag: read32(bytes, 0x80), effect: LightEffect::from_raw(read16(bytes, 0x84)), speed: LightSpeed(read16(bytes, 0x86)),
                fire_ring: std::array::from_fn(|i| Color::from_packed(read32(bytes, 0x88 + i * 4))),
                light_tracing: std::array::from_fn(|i| Color::from_packed(read32(bytes, 0x90 + i * 4))),
                fixed: std::array::from_fn(|i| Color::from_packed(read32(bytes, 0x98 + i * 4))),
            },
            motion: Motion {
                flag: read32(bytes, 0xf8), feature: MotionFlags(bytes[0xfc]), reserve: [bytes[0xfd]], sensitivity: Sensitivity(bytes[0xfe]),
                compensation: ByteValue(bytes[0xff]), enable_button: PhysicalButtons(read32(bytes, 0x100)),
            },
            dead_zone: DeadZone {
                flag: read32(bytes, 0x104),
                curves: std::array::from_fn(|side| StickCurve { points: std::array::from_fn(|point| {
                    let i = 0x108 + side * 8 + point * 2;
                    CurvePoint { input: Percent(bytes[i]), output: Percent(bytes[i + 1]) }
                }) }),
                jitter: [Level(bytes[0x118]), Level(bytes[0x119])],
                trigger_lower: [ByteValue(bytes[0x11a]), ByteValue(bytes[0x11b])],
                trigger_upper: [ByteValue(bytes[0x11c]), ByteValue(bytes[0x11d])],
                reserve: [bytes[0x11e], bytes[0x11f]],
            },
        }
    }
    fn encode(&self, bytes: &mut [u8]) {
        write32(bytes, 0, self.flag); bytes[4..0x24].copy_from_slice(self.name.raw());
        write32(bytes, 0x24, self.key_map.flag);
        for (i, target) in self.key_map.mapping.iter().enumerate() { write32(bytes, 0x28 + i * 4, target.raw()); }
        write32(bytes, 0x78, self.feature_flag); write32(bytes, 0x7c, self.feature.raw());
        write32(bytes, 0x80, self.lighting.flag); write16(bytes, 0x84, self.lighting.effect.raw()); write16(bytes, 0x86, self.lighting.speed.0);
        for (base, colors) in [(0x88, self.lighting.fire_ring.as_slice()), (0x90, self.lighting.light_tracing.as_slice()), (0x98, self.lighting.fixed.as_slice())] {
            for (i, color) in colors.iter().enumerate() { write32(bytes, base + i * 4, color.packed()); }
        }
        write32(bytes, 0xf8, self.motion.flag); bytes[0xfc] = self.motion.feature.raw(); bytes[0xfd] = self.motion.reserve[0];
        bytes[0xfe] = self.motion.sensitivity.0; bytes[0xff] = self.motion.compensation.0; write32(bytes, 0x100, self.motion.enable_button.bits());
        write32(bytes, 0x104, self.dead_zone.flag);
        for (side, curve) in self.dead_zone.curves.iter().enumerate() {
            for (point, value) in curve.points.iter().enumerate() {
                let i = 0x108 + side * 8 + point * 2; bytes[i] = value.input.0; bytes[i + 1] = value.output.0;
            }
        }
        for side in 0..2 {
            bytes[0x118 + side] = self.dead_zone.jitter[side].0;
            bytes[0x11a + side] = self.dead_zone.trigger_lower[side].0;
            bytes[0x11c + side] = self.dead_zone.trigger_upper[side].0;
        }
        bytes[0x11e..0x120].copy_from_slice(&self.dead_zone.reserve);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profiles {
    pub stored_crc: u16,
    pub current_slot: SlotIndex,
    pub profile_bits: ProfileBits,
    pub slots: [Slot; 3],
}
impl Profiles {
    /// Every image decodes, even with unknown enums, malformed names or bad CRC.
    pub fn decode(image: &[u8; IMAGE_SIZE]) -> Result<Self> {
        Ok(Self { stored_crc: read16(image, 0), current_slot: SlotIndex(image[2]), profile_bits: ProfileBits(image[3]), slots: std::array::from_fn(|i| Slot::decode(&image[4 + i * SLOT_SIZE..4 + (i + 1) * SLOT_SIZE])) })
    }
    /// Byte-exact, preserving stored CRC. Explicit `fix_crc` opts into repair.
    pub fn encode(&self) -> [u8; IMAGE_SIZE] {
        let mut image = [0; IMAGE_SIZE];
        write16(&mut image, 0, self.stored_crc); image[2] = self.current_slot.0; image[3] = self.profile_bits.raw();
        for (i, slot) in self.slots.iter().enumerate() { slot.encode(&mut image[4 + i * SLOT_SIZE..4 + (i + 1) * SLOT_SIZE]); }
        image
    }
    pub fn fix_crc(&mut self) { self.stored_crc = crc(&self.encode()); }
    /// Windows ResetView's slot defaults (0x180c818d0), with a repaired CRC.
    /// Runtime slot and profile bits are device-managed: opcode 1 cannot change
    /// either header byte. See reconstruction/specs/spec-reset.md.
    pub fn factory_defaults(&self) -> Profiles {
        let mut bytes = [0; SLOT_SIZE];
        for source in PhysicalButton::ALL {
            write32(&mut bytes, 0x28 + source.index() * 4, source.mask().bits());
        }
        write32(&mut bytes, 0x7c, 0x002b_6d00);
        write16(&mut bytes, 0x84, 4);
        write16(&mut bytes, 0x86, 5);
        for (at, color) in [(0x88, 0x0000_f144), (0x8c, 0x007a_fc44), (0x90, 0xfff7_1d44), (0x94, 0xe100_af44)] {
            write32(&mut bytes, at, color);
        }
        for at in (0x98..0xf8).step_by(4) { write32(&mut bytes, at, 0x0064_fb44); }
        bytes[0xfc] = 0x18;
        bytes[0xfe] = 5;
        for at in [0x108, 0x110] {
            bytes[at..at + 8].copy_from_slice(&[8, 0, 37, 33, 66, 66, 95, 100]);
        }
        bytes[0x118..0x11a].fill(2);
        bytes[0x11c..0x11e].fill(255);
        let slot = Slot::decode(&bytes);
        let mut profiles = Profiles {
            stored_crc: 0,
            current_slot: self.current_slot,
            profile_bits: self.profile_bits,
            slots: std::array::from_fn(|_| slot.clone()),
        };
        profiles.fix_crc();
        profiles
    }
    /// Computed from current fields, so edits cannot leave a stale UI indicator.
    pub fn crc_valid(&self) -> bool { verify_crc(&self.encode()) }
    pub fn set_current_slot(&mut self, index: u16) -> Result<()> { self.current_slot = SlotIndex::new(index)?; Ok(()) }
    pub fn selected_slot(&self) -> Option<&Slot> { self.slots.get(self.current_slot.0 as usize) }
}
impl Serialize for Profiles {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("Profiles", 5)?;
        state.serialize_field("stored_crc", &self.stored_crc)?;
        state.serialize_field("crc_valid", &self.crc_valid())?;
        state.serialize_field("current_slot", &self.current_slot)?;
        state.serialize_field("profile_bits", &self.profile_bits)?;
        state.serialize_field("slots", &self.slots)?;
        state.end()
    }
}

fn read16(bytes: &[u8], at: usize) -> u16 { u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) }
fn read32(bytes: &[u8], at: usize) -> u32 { u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) }
fn write16(bytes: &mut [u8], at: usize, value: u16) { bytes[at..at + 2].copy_from_slice(&value.to_le_bytes()); }
fn write32(bytes: &mut [u8], at: usize, value: u32) { bytes[at..at + 4].copy_from_slice(&value.to_le_bytes()); }
fn set_bit8(raw: &mut u8, mask: u8, value: bool) { if value { *raw |= mask; } else { *raw &= !mask; } }
fn set_bit32(raw: &mut u32, mask: u32, value: bool) { if value { *raw |= mask; } else { *raw &= !mask; } }

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> [u8; IMAGE_SIZE] {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)).unwrap().try_into().unwrap()
    }
    #[test]
    fn checked_read_retries_once_then_refuses_live_shifted_chunk() {
        let good = fixture("ultimate-3e-profiles-receiver-20261015-2209.bin");
        // Live corruption: chunk at 455 shifted right by one with 0xe1 inserted.
        let mut shifted = good;
        shifted[0x1c8] = 0xe1;
        shifted[0x1c9..0x1d4].copy_from_slice(&good[0x1c8..0x1d3]);
        assert!(!verify_crc(&shifted));
        let mut reads = vec![good.to_vec(), shifted.to_vec()];
        assert_eq!(checked_read(|| Ok(reads.pop().unwrap())).unwrap(), good.to_vec());
        let mut calls = 0;
        let error = checked_read(|| { calls += 1; Ok(shifted.to_vec()) }).unwrap_err().to_string();
        assert_eq!(calls, 2);
        assert!(error.contains("stored 0x4876"), "{error}");
        let mut calls = 0;
        checked_read(|| { calls += 1; Ok(good.to_vec()) }).unwrap();
        assert_eq!(calls, 1);
    }

    #[test]
    fn profile_backup_roundtrips_and_crcs() {
        let mut count = 0;
        for entry in std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if (name.starts_with("ultimate-3e-profiles") || name == "ultimate-3e-appimage-profiles.bin") && name.ends_with(".bin") {
                let image = fixture(&name);
                let profiles = Profiles::decode(&image).unwrap();
                assert_eq!(profiles.encode(), image, "{name}");
                assert!(verify_crc(&image), "{name}");
                assert!(profiles.crc_valid(), "{name}");
                let mut repaired = profiles.clone();
                repaired.fix_crc();
                assert_eq!(repaired.encode(), image, "{name}");
                count += 1;
            }
        }
        assert!(count >= 4, "expected profile image fixtures");
        assert_eq!(crc(&fixture("ultimate-3e-profiles.bin")), 0xa1c4);
        assert_eq!(crc(&fixture("ultimate-3e-profiles-before-native-clutch-1790102027225.bin")), 0x6fad);
        assert_eq!(crc(&fixture("ultimate-3e-profiles-receiver-1.bin")), 0x1d33);
    }

    #[test]
    fn profile_arbitrary_bytes_are_lossless_even_with_invalid_crc() {
        let mut seed = 0x6a09_e667_f3bc_c909u64;
        for _ in 0..1024 {
            let image = std::array::from_fn(|_| {
                seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
                seed as u8
            });
            let mut decoded = Profiles::decode(&image).unwrap();
            assert_eq!(decoded.encode(), image);
            assert_eq!(decoded.crc_valid(), verify_crc(&image));
            decoded.fix_crc();
            let encoded = decoded.encode();
            assert!(verify_crc(&encoded));
            assert_eq!(&encoded[2..], &image[2..]);
        }
        for fill in [0, 0xff] {
            let image = [fill; IMAGE_SIZE];
            assert_eq!(Profiles::decode(&image).unwrap().encode(), image);
        }
    }

    #[test]
    fn factory_reset_is_idempotent_and_changes_only_documented_spans() {
        let image = std::array::from_fn(|i| (i.wrapping_mul(73) ^ 0xa5) as u8);
        let before = Profiles::decode(&image).unwrap();
        let reset = before.factory_defaults();
        let after = reset.encode();
        assert_eq!(before.encode(), image, "reset must not mutate its input");
        assert_eq!(reset.factory_defaults(), reset);
        assert!(verify_crc(&after));
        // The entire slot image is replaced; device-managed header bytes survive.
        assert_eq!(&after[2..4], &image[2..4]);
        for (index, slot) in reset.slots.iter().enumerate() {
            let at = 4 + index * SLOT_SIZE;
            for marker in [0, 0x24, 0x78, 0x80, 0xf8, 0x104] {
                assert_eq!(&after[at + marker..at + marker + 4], &[0; 4]);
            }
            assert_eq!(slot.name.raw(), &[0; 32]);
            for source in PhysicalButton::ALL {
                assert_eq!(slot.key_map.get(source).raw(), source.mask().bits());
            }
            assert_eq!(slot.feature.raw(), 0x002b_6d00);
            assert_eq!(slot.lighting.effect, LightEffect::Fixed);
            assert_eq!(slot.lighting.speed.get(), 5);
            assert_eq!(slot.lighting.fire_ring.map(|c| c.packed()), [0x0000_f144, 0x007a_fc44]);
            assert_eq!(slot.lighting.light_tracing.map(|c| c.packed()), [0xfff7_1d44, 0xe100_af44]);
            assert!(slot.lighting.fixed.iter().all(|c| c.packed() == 0x0064_fb44));
            assert_eq!(&after[at + 0xfc..at + 0x104], &[0x18, 0, 5, 0, 0, 0, 0, 0]);
            assert_eq!(&after[at + 0x108..at + 0x120], &[8, 0, 37, 33, 66, 66, 95, 100, 8, 0, 37, 33, 66, 66, 95, 100, 2, 2, 0, 0, 255, 255, 0, 0]);
        }
    }
    #[test]
    fn factory_reset_preserves_every_device_managed_header_value() {
        for bits in 0..=u8::MAX {
            for slot in [0, 1, 2, 0xff] {
                let mut image = [0; IMAGE_SIZE];
                image[2] = slot;
                image[3] = bits;
                let after = Profiles::decode(&image).unwrap().factory_defaults().encode();
                assert_eq!(&after[2..4], &[slot, bits]);
                assert!(verify_crc(&after));
            }
        }
    }
    #[test]
    fn factory_reset_receiver_backup_preview_matches_recovered_constants() {
        let before = fixture("ultimate-3e-profiles-receiver-before-edit.bin");
        let after = Profiles::decode(&before).unwrap().factory_defaults().encode();
        assert_eq!(&after[..4], &[0x02, 0xc0, 0xff, before[3]]);
        assert_eq!(before.iter().zip(after).filter(|(a, b)| **a != *b).count(), 141);
        assert_eq!(crc(&after), 0xc002);
    }

    #[test]
    fn profile_validators_reject_every_out_of_domain_u16() {
        for value in 0..=u16::MAX {
            assert_eq!(SlotIndex::new(value).is_ok(), value <= 2);
            assert_eq!(Percent::new(value).is_ok(), value <= 100);
            assert_eq!(Level::new(value).is_ok(), value <= 5);
            assert_eq!(Sensitivity::new(value).is_ok(), value <= 5);
            assert_eq!(Sensitivity::from_display(value).is_ok(), (1..=6).contains(&value));
            assert_eq!(LightSpeed::new(value).is_ok(), (1..=10).contains(&value));
            assert_eq!(ByteValue::new(value).is_ok(), value <= 255);
        }
    }

    #[test]
    fn profile_setters_reject_unsupported_choices_without_mutating() {
        let image = fixture("ultimate-3e-profiles.bin");
        let mut p = Profiles::decode(&image).unwrap();
        assert!(p.set_current_slot(3).is_err());
        assert!(p.profile_bits.set_polling_hz(2000).is_err());
        assert!(p.profile_bits.set_polling_hz(4000).is_err());
        let s = &mut p.slots[0];
        assert!(s.name.set("12345678901234567").is_err());
        assert!(s.name.set("😀😀😀😀😀😀😀😀a").is_err());
        assert!(s.name.set("a\0b").is_err());
        for raw in [0x0020_0000, 0x4000_0000, 0x0200_0000, 0x0400_0000, 0x0002_0000, 0x0800_0000, 0x0800_0001, 0x3000] {
            assert!(s.key_map.set(PhysicalButton::A, MappingTarget::from_raw(raw)).is_err());
        }
        assert!(s.feature.set_shape(StickShape::Unknown(3)).is_err());
        for motor in [VibrationMotor::LeftGrip, VibrationMotor::RightGrip, VibrationMotor::LeftTrigger, VibrationMotor::RightTrigger] {
            assert!(s.feature.set_vibration(motor, 6).is_err());
        }
        assert!(s.lighting.set_effect(LightEffect::Unknown(0)).is_err());
        assert!(s.lighting.set_effect(LightEffect::Unknown(5)).is_err());
        assert!(s.lighting.set_speed(0).is_err());
        assert!(s.lighting.set_speed(11).is_err());
        assert!(s.motion.feature.set_target(MotionTarget::Unknown(3)).is_err());
        assert!(s.motion.set_sensitivity(6).is_err());
        assert!(s.motion.set_compensation(256).is_err());
        for raw in [0x0002_0000, 0x0800_0000, 0x0040_0000, 0x3000, 0xffff_ffff] {
            assert!(s.motion.set_enable_button(PhysicalButtons::from_bits_retain(raw)).is_err());
        }
        assert!(s.dead_zone.set_jitter(Side::Left, 6).is_err());
        assert!(s.dead_zone.set_trigger_lower(Side::Left, 256).is_err());
        assert!(s.dead_zone.set_trigger_upper(Side::Right, 256).is_err());
        for index in [0, 3, usize::MAX] {
            assert!(s.dead_zone.curves[0].set_custom_point(index, 30, 30).is_err());
        }
        assert!(s.dead_zone.curves[0].set_custom_point(1, 0, 30).is_err());
        assert!(s.dead_zone.curves[0].set_custom_point(1, 30, 0).is_err());
        assert!(s.dead_zone.curves[0].set_custom_point(1, 101, 30).is_err());
        assert!(s.dead_zone.curves[0].set_endpoint(false, 101, 0).is_err());
        assert!(CurvePoint::new(0, 101).is_err());
        for args in [(256, 0, 0, 0), (0, 256, 0, 0), (0, 0, 256, 0), (0, 0, 0, 256)] {
            assert!(Color::new(args.0, args.1, args.2, args.3).is_err());
        }
        assert_eq!(p.encode(), image, "failed setters must not partially mutate");
    }

    fn assert_locality(image: &[u8; IMAGE_SIZE], p: &mut Profiles, allowed: std::ops::Range<usize>) {
        p.fix_crc();
        let edited = p.encode();
        assert!(verify_crc(&edited));
        assert!(edited[allowed.clone()] != image[allowed.clone()], "field edit must change bytes {allowed:#x?}");
        for i in 2..IMAGE_SIZE {
            if !allowed.contains(&i) { assert_eq!(edited[i], image[i], "unexpected changed byte {i:#x}"); }
        }
    }

    #[test]
    fn profile_edits_touch_only_documented_bytes_and_crc() {
        let image = fixture("ultimate-3e-profiles.bin");
        type Edit = fn(&mut Slot);
        let edits: &[(std::ops::Range<usize>, Edit)] = &[
            (0..4, |s| s.flag ^= 1),
            (4..0x24, |s| s.name.set("😀配置").unwrap()),
            (0x24..0x28, |s| s.key_map.flag ^= 1),
            (0x78..0x7c, |s| s.feature_flag ^= 1),
            (0x7c..0x80, |s| s.feature.set_shape(StickShape::Circular).unwrap()),
            (0x7c..0x80, |s| s.feature.set_inverted(Side::Left, Axis::X, !s.feature.inverted(Side::Left, Axis::X))),
            (0x7c..0x80, |s| s.feature.set_stick_swap(StickSwap::LeftStickDpad)),
            (0x7c..0x80, |s| s.feature.set_swap_triggers(!s.feature.swap_triggers())),
            (0x7c..0x80, |s| s.feature.set_impulse_sync(!s.feature.impulse_sync())),
            (0x7c..0x80, |s| s.feature.set_vibration(VibrationMotor::LeftGrip, 0).unwrap()),
            (0x7c..0x80, |s| s.feature.set_vibration(VibrationMotor::RightGrip, 0).unwrap()),
            (0x7c..0x80, |s| s.feature.set_vibration(VibrationMotor::LeftTrigger, 0).unwrap()),
            (0x7c..0x80, |s| s.feature.set_vibration(VibrationMotor::RightTrigger, 0).unwrap()),
            (0x80..0x84, |s| s.lighting.flag ^= 1),
            (0x84..0x86, |s| s.lighting.set_effect(if s.lighting.effect == LightEffect::Off { LightEffect::Fixed } else { LightEffect::Off }).unwrap()),
            (0x86..0x88, |s| s.lighting.set_speed(1).unwrap()),
            (0x88..0x8c, |s| s.lighting.fire_ring[0] = Color::new(255, 1, 2, 3).unwrap()),
            (0x8c..0x90, |s| s.lighting.fire_ring[1] = Color::new(255, 1, 2, 3).unwrap()),
            (0x90..0x94, |s| s.lighting.light_tracing[0] = Color::new(255, 1, 2, 3).unwrap()),
            (0x94..0x98, |s| s.lighting.light_tracing[1] = Color::new(255, 1, 2, 3).unwrap()),
            (0x98..0xf8, |s| s.lighting.set_fixed_color(Color::new(255, 1, 2, 3).unwrap())),
            (0xf8..0xfc, |s| s.motion.flag ^= 1),
            (0xfc..0xfd, |s| s.motion.feature.set_target(MotionTarget::Mouse).unwrap()),
            (0xfc..0xfd, |s| s.motion.feature.set_enable_method(if s.motion.feature.enable_method() == EnableMethod::Hold { EnableMethod::Toggle } else { EnableMethod::Hold })),
            (0xfd..0xfe, |s| s.motion.reserve[0] ^= 1),
            (0xfe..0xff, |s| s.motion.set_sensitivity(0).unwrap()),
            (0xff..0x100, |s| s.motion.set_compensation(128).unwrap()),
            (0x100..0x104, |s| s.motion.set_enable_button(PhysicalButtons::A).unwrap()),
            (0x104..0x108, |s| s.dead_zone.flag ^= 1),
            (0x108..0x10a, |s| s.dead_zone.curves[0].set_endpoint(false, 10, 2).unwrap()),
            (0x10a..0x10c, |s| s.dead_zone.curves[0].set_custom_point(1, 35, 32).unwrap()),
            (0x10c..0x10e, |s| s.dead_zone.curves[0].set_custom_point(2, 67, 67).unwrap()),
            (0x10e..0x110, |s| s.dead_zone.curves[0].set_endpoint(true, 90, 99).unwrap()),
            (0x110..0x112, |s| s.dead_zone.curves[1].set_endpoint(false, 10, 2).unwrap()),
            (0x112..0x114, |s| s.dead_zone.curves[1].set_custom_point(1, 35, 32).unwrap()),
            (0x114..0x116, |s| s.dead_zone.curves[1].set_custom_point(2, 67, 67).unwrap()),
            (0x116..0x118, |s| s.dead_zone.curves[1].set_endpoint(true, 90, 99).unwrap()),
            (0x118..0x119, |s| s.dead_zone.set_jitter(Side::Left, 0).unwrap()),
            (0x119..0x11a, |s| s.dead_zone.set_jitter(Side::Right, 0).unwrap()),
            (0x11a..0x11b, |s| s.dead_zone.set_trigger_lower(Side::Left, 1).unwrap()),
            (0x11b..0x11c, |s| s.dead_zone.set_trigger_lower(Side::Right, 1).unwrap()),
            (0x11c..0x11d, |s| s.dead_zone.set_trigger_upper(Side::Left, 254).unwrap()),
            (0x11d..0x11e, |s| s.dead_zone.set_trigger_upper(Side::Right, 254).unwrap()),
            (0x11e..0x120, |s| s.dead_zone.reserve[0] ^= 1),
        ];
        for slot in 0..3 {
            let base = 4 + slot * SLOT_SIZE;
            for (range, edit) in edits {
                let mut p = Profiles::decode(&image).unwrap();
                edit(&mut p.slots[slot]);
                assert_locality(&image, &mut p, base + range.start..base + range.end);
            }
            for source in PhysicalButton::ALL {
                let mut p = Profiles::decode(&image).unwrap();
                let target = if p.slots[slot].key_map.get(source) == MappingTarget::None { MappingTarget::A } else { MappingTarget::None };
                p.slots[slot].key_map.set(source, target).unwrap();
                let at = base + 0x28 + source.index() * 4;
                assert_locality(&image, &mut p, at..at + 4);
            }
        }
        let mut p = Profiles::decode(&image).unwrap();
        p.set_current_slot(0).unwrap();
        assert_locality(&image, &mut p, 2..3);
        let mut p = Profiles::decode(&image).unwrap();
        p.profile_bits.set_tournament(!p.profile_bits.tournament());
        assert_locality(&image, &mut p, 3..4);
        let mut p = Profiles::decode(&image).unwrap();
        p.profile_bits.set_auto_reconnect(!p.profile_bits.auto_reconnect());
        assert_locality(&image, &mut p, 3..4);
    }

    #[test]
    fn profile_ui_scales_names_masks_and_imported_values() {
        assert_eq!(Sensitivity::new(5).unwrap().display(), 6);
        assert_eq!(ByteValue::new(128).unwrap().percent(), 50);
        assert_eq!(ByteValue::new(255).unwrap().percent(), 100);
        let color = Color::new(68, 40, 128, 0).unwrap();
        assert_eq!(color.packed(), 0x0080_2844);
        assert_eq!(color.brightness_percent(), 26);
        assert_eq!(color.saturation_percent(), 50);
        let mut p = Profiles::decode(&fixture("ultimate-3e-profiles.bin")).unwrap();
        assert_eq!(p.slots[0].name.display(), "Config 1");
        p.slots[0].name.set("😀😀😀😀😀😀😀😀").unwrap();
        assert_eq!(p.slots[0].name.display(), "😀😀😀😀😀😀😀😀");
        assert_eq!(p.slots[0].name.raw()[..4], [0xd8, 0x3d, 0xde, 0x00]);
        let mut known = 0;
        for button in PhysicalButton::ALL {
            known |= button.mask().bits();
            p.slots[0].motion.set_enable_button(button.mask()).unwrap();
            assert_eq!(button.mask().labels().collect::<Vec<_>>(), [button.label()]);
        }
        assert_eq!(known, PhysicalButtons::KNOWN);
        for target in MappingTarget::ALL { p.slots[0].key_map.set(PhysicalButton::A, target).unwrap(); }
        p.slots[0].feature.set_stick_swap(StickSwap::LeftStickDpad);
        p.slots[0].feature.set_stick_swap(StickSwap::LeftStickRightStick);
        assert!(!p.slots[0].feature.swaps_left_dpad());
        assert!(p.slots[0].feature.swaps_sticks());
        p.slots[0].dead_zone.curves[0].apply_preset(CurvePreset::Digital);
        assert_eq!(p.slots[0].dead_zone.curves[0].points[1].input.get(), 29);
        assert_eq!(p.slots[0].dead_zone.curves[0].points[2].input.get(), 29);
        let mut raw = [0xff; IMAGE_SIZE];
        let decoded = Profiles::decode(&raw).unwrap();
        assert!(decoded.selected_slot().is_none());
        assert!(!decoded.current_slot.is_valid());
        assert!(!decoded.slots[0].motion.sensitivity.is_valid());
        assert_eq!(decoded.slots[0].feature.shape().display_label(), "Square");
        raw[3] = 0xff;
        let mut decoded = Profiles::decode(&raw).unwrap();
        decoded.profile_bits.set_tournament(false);
        assert_eq!(decoded.profile_bits.raw(), 0xef);
        decoded.profile_bits.set_auto_reconnect(false);
        assert_eq!(decoded.profile_bits.raw(), 0xe7);
        let json = serde_json::to_value(&decoded).unwrap();
        assert_eq!(json["current_slot"], 255);
        assert_eq!(json["crc_valid"], decoded.crc_valid());
        assert_eq!(json["slots"][0]["motion"]["reserve"][0], 255);
    }
}
