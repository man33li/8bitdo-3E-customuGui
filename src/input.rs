//! Host inverse clutch and Linux input runtime, translated from gyro_clutch.py.
//! The Android 2.00 evidence recorded there identifies physical R4 as 0x40000000,
//! independent of gaming HID's nominal R4 slot. Sensor records are at
//! 0xfc + slot * 0x120; inverse hold is host policy, not a firmware feature.
//! Raw gyro layout follows that implementation/SDL: report 1, 32 bytes,
//! little-endian i16 XYZ at bytes 21/23/25. This is not Windows-bundle parity.
//! Receiver gyro uses the firmware mouse interface; wired GIP has no host gyro.
//! Physical receiver paddles use the vendor HID stream, enabled by Windows heartbeat.
//! Gyro processing follows GyroWiki/JoyShockMapper: soft tiered smoothing,
//! tightening, velocity-based sensitivity, then mouse or deadzone-compensated stick.
//! Mapping and profile logic are shared; device access is Linux-only.
//! Windows device entry points return explicit unsupported errors.

use crate::{Result, invalid, protocol::PhysicalInput};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    mem::size_of,
    os::{
        fd::{AsRawFd, RawFd},
        unix::{ffi::OsStrExt, fs::{FileTypeExt, OpenOptionsExt}},
    },
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError, TryRecvError},
    },
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{devices, rumble, run, run_receiver, run_wired};

pub const AXES: [u16; 8] = [0, 1, 2, 5, 9, 10, 16, 17];
pub const KEY_CODES: [u16; 24] = [
    304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 317, 318, 319, 704, 705, 706,
    707, 708, 709, 710, 711,
];
// Same source slots as Bluetooth HID: L4/R4=HAPPY1/2, PL=BTN_Z, PR=BTN_C.
const PADDLE_SOURCES: [(usize, u32); 4] = [(16, 0x00200000), (17, 0x40000000), (5, 0x02000000), (2, 0x04000000)];
const BUS_BLUETOOTH: u16 = 5;
const BUS_VIRTUAL: u16 = 6;
const VENDOR: u16 = 0x2dc8;
const PRODUCT: u16 = 0x202f;
const VIRTUAL_PHYS: &str = "gyro-clutch/";
const MAX_EVENTS: usize = 67; // Layer edges, eight axes + two gyro decay edges, four REL, SYN.
const MOTION_TIMEOUT: Duration = Duration::from_millis(250);
const RECEIVER_AXES: [u16; 8] = [0, 1, 3, 4, 2, 5, 16, 17];
/// Live 2026-10-19: 90° yaw sweeps = +27659/-27522/+27975 counts, Motion level 6.
/// Other firmware sensitivities are uncalibrated: keep level 6 and adjust host gain.
pub const RECEIVER_COUNTS_PER_DEGREE: f64 = 308.0;
const BUTTONS: usize = 26; // 24 physical buttons plus two trigger thresholds.
pub const RECEIVER_NOTICE: &str = "Receiver: physical L4/R4/PL/PR sources use the Windows heartbeat. Without receiver_gyro, clutch settings are ignored; no raw IMU.";
pub const WIRED_NOTICE: &str = "WIRED: clutch settings are ignored; no host gyro/IMU. Physical paddles are unavailable unless the controller profile maps them to standard buttons.";
pub const RECEIVER_GYRO_NOTICE: &str = "2.4 GHz gyro clutch: set controller Motion target to Mouse; enable via press-toggle (turn on once) or hold. Physical R4/L4/PL/PR work directly as clutch buttons; no controller remap needed. With the mapper closed, firmware gyro moves the desktop cursor.";
pub const CABLE_GYRO_NOTICE: &str = "USB-cable gyro clutch: set controller Motion target to Mouse; enable via press-toggle (turn on once) or hold. The clutch must be a standard button (paddles only via the slot mapping). With the mapper closed, firmware gyro moves the desktop cursor.";
/// Receiver standard buttons plus vendor physical paddle sources.
pub const RECEIVER_CLUTCH_KEYS: &[(&str, u16)] = &[
    ("BTN_SOUTH", 304), ("BTN_EAST", 305), ("BTN_NORTH", 307), ("BTN_WEST", 308),
    ("BTN_TL", 310), ("BTN_TR", 311), ("BTN_SELECT", 314), ("BTN_START", 315),
    ("BTN_MODE", 316), ("BTN_THUMBL", 317), ("BTN_THUMBR", 318),
    ("L4", 704), ("R4", 705), ("PL", 309), ("PR", 306),
];
/// Keys the receiver's xpad evdev node must advertise. Paddles are absent there by design:
/// they arrive only as vendor physical reports while the heartbeat runs.
const RECEIVER_EVDEV_KEYS: [u16; 11] = [304, 305, 307, 308, 310, 311, 314, 315, 316, 317, 318];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Bluetooth,
    Receiver,
    Wired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stick {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StickMouse {
    pub stick: Stick,
    /// Pixels per second at full deflection; constant gain, no acceleration.
    pub sensitivity: f64,
    /// Normalized radial deadzone in [0, 1).
    pub deadzone: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerButton {
    pub destination: String,
    /// Normalized trigger threshold in (0, 1].
    pub threshold: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TriggerButtons {
    pub left: Option<TriggerButton>,
    pub right: Option<TriggerButton>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GyroMouse {
    /// Pixels/degree on both links (receiver calibrated at Motion level 6).
    pub sensitivity: f64,
    /// Soft tightening threshold in degrees/second; zero disables it.
    pub deadzone: f64,
    pub invert_x: bool,
    pub invert_y: bool,
}

impl Default for GyroMouse {
    fn default() -> Self {
        Self { sensitivity: 1.0, deadzone: 0.5, invert_x: false, invert_y: false }
    }
}

/// Optional gyro filters; units are real-world degrees/second and milliseconds.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GyroSettings {
    pub min_sensitivity: f64,
    pub max_sensitivity: f64,
    pub min_threshold: f64,
    pub max_threshold: f64,
    pub smooth_threshold: f64,
    pub smooth_time_ms: f64,
    pub stick_deadzone: f64,
    pub stick_saturation: f64,
    pub stick_power: f64,
    pub stick_smoothing_ms: f64,
}
impl Default for GyroSettings {
    fn default() -> Self {
        Self { min_sensitivity: 1.0, max_sensitivity: 1.0, min_threshold: 0.0,
            max_threshold: 75.0, smooth_threshold: 5.0, smooth_time_ms: 40.0,
            stick_deadzone: 0.15, stick_saturation: 180.0, stick_power: 1.0,
            stick_smoothing_ms: 8.0 }
    }
}
impl GyroSettings {
    fn validate(self) -> Result<()> {
        for (value, min, max) in [
            (self.min_sensitivity, 0.0, 100.0), (self.max_sensitivity, 0.0, 100.0),
            (self.min_threshold, 0.0, 2000.0), (self.max_threshold, 0.0, 2000.0),
            (self.smooth_threshold, 0.0, 2000.0), (self.smooth_time_ms, 0.0, 250.0),
            (self.stick_deadzone, 0.0, 0.99), (self.stick_saturation, 0.01, 2000.0),
            (self.stick_power, 0.1, 8.0), (self.stick_smoothing_ms, 0.0, 250.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(invalid("gyro filter parameter outside its finite supported range"));
            }
        }
        if self.max_threshold < self.min_threshold {
            return Err(invalid("gyro fast threshold must be at least slow threshold"));
        }
        Ok(())
    }
    fn sensitivity(self, speed: f64) -> f64 {
        let blend = if self.max_threshold > self.min_threshold {
            ((speed - self.min_threshold) / (self.max_threshold - self.min_threshold)).clamp(0.0, 1.0)
        } else if speed > self.min_threshold { 1.0 } else { 0.0 };
        self.min_sensitivity + (self.max_sensitivity - self.min_sensitivity) * blend
    }
    fn stick(self, velocity: [f64; 2]) -> [f64; 2] {
        let speed = velocity[0].hypot(velocity[1]);
        if speed == 0.0 { return [0.0; 2]; }
        let deflection = self.stick_deadzone + (1.0 - self.stick_deadzone)
            * (speed / self.stick_saturation).min(1.0).powf(self.stick_power);
        velocity.map(|v| v / speed * deflection * 32767.0)
    }
}

#[derive(Clone, Copy, Default)]
struct GyroSample { slow: [f64; 2], dt: f64 }
struct GyroFilter { samples: [GyroSample; 256], next: usize }
impl Default for GyroFilter {
    fn default() -> Self { Self { samples: [GyroSample::default(); 256], next: 0 } }
}
impl GyroFilter {
    // Split the sample, not the final result: large motion bypasses the window while
    // earlier small movements drain through it (GyroWiki soft tiered smoothing).
    fn sample(&mut self, velocity: [f64; 2], dt: f64, settings: GyroSettings, tightening: f64) -> [f64; 2] {
        let speed = velocity[0].hypot(velocity[1]);
        let direct = if settings.smooth_threshold > 0.0 && settings.smooth_time_ms > 0.0 {
            ((speed - settings.smooth_threshold * 0.5) / (settings.smooth_threshold * 0.5)).clamp(0.0, 1.0)
        } else { 1.0 };
        self.samples[self.next] = GyroSample { slow: velocity.map(|v| v * (1.0 - direct)), dt };
        self.next = (self.next + 1) % self.samples.len();
        let window = if settings.smooth_threshold > 0.0 { settings.smooth_time_ms / 1000.0 } else { 0.0 };
        let mut remaining = window;
        let mut result = velocity.map(|v| v * direct);
        if window > 0.0 {
            for offset in 0..self.samples.len() {
                let sample = self.samples[(self.next + self.samples.len() - 1 - offset) % self.samples.len()];
                if remaining <= 0.0 || sample.dt == 0.0 { break; }
                let weight = remaining.min(sample.dt);
                for index in 0..2 { result[index] += sample.slow[index] * weight / window; }
                remaining -= weight;
            }
        }
        let speed = result[0].hypot(result[1]);
        let tighten = if tightening > 0.0 { (speed / tightening).min(1.0) } else { 1.0 };
        let sensitivity = settings.sensitivity(speed * tighten);
        result.map(|v| v * tighten * sensitivity)
    }
}

/// Post-slot-map buttons (bits index KEY_CODES), before host mappings. Axes order:
/// LX/LY (source units), RX/RY (signed -32768..32767), triggers (source units), hat X/Y.
#[derive(Clone, Copy, Debug)]
pub struct PhysicalState { pub buttons: u32, pub axes: [i32; 8] }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    pub hold: String,
    pub mappings: BTreeMap<String, String>,
    pub turbo: BTreeMap<String, f64>,
    pub toggle: Vec<String>,
    pub stick_mouse: Option<StickMouse>,
    pub trigger_buttons: TriggerButtons,
}

/// Named Linux source buttons. HID bit 15 has no Linux key name but passes through.
pub const SOURCE_KEYS: &[(&str, u16)] = &[
    ("BTN_SOUTH", 304),
    ("BTN_EAST", 305),
    ("BTN_C", 306),
    ("BTN_NORTH", 307),
    ("BTN_WEST", 308),
    ("BTN_Z", 309),
    ("BTN_TL", 310),
    ("BTN_TR", 311),
    ("BTN_TL2", 312),
    ("BTN_TR2", 313),
    ("BTN_SELECT", 314),
    ("BTN_START", 315),
    ("BTN_MODE", 316),
    ("BTN_THUMBL", 317),
    ("BTN_THUMBR", 318),
    ("BTN_TRIGGER_HAPPY1", 704),
    ("BTN_TRIGGER_HAPPY2", 705),
    ("BTN_TRIGGER_HAPPY3", 706),
    ("BTN_TRIGGER_HAPPY4", 707),
    ("BTN_TRIGGER_HAPPY5", 708),
    ("BTN_TRIGGER_HAPPY6", 709),
    ("BTN_TRIGGER_HAPPY7", 710),
    ("BTN_TRIGGER_HAPPY8", 711),
];

/// Supported real EV_KEY destinations, including Linux's gamepad aliases.
pub const DESTINATION_KEYS: &[(&str, u16)] = &[
    ("BTN_SOUTH", 304),
    ("BTN_EAST", 305),
    ("BTN_C", 306),
    ("BTN_NORTH", 307),
    ("BTN_WEST", 308),
    ("BTN_Z", 309),
    ("BTN_TL", 310),
    ("BTN_TR", 311),
    ("BTN_TL2", 312),
    ("BTN_TR2", 313),
    ("BTN_SELECT", 314),
    ("BTN_START", 315),
    ("BTN_MODE", 316),
    ("BTN_THUMBL", 317),
    ("BTN_THUMBR", 318),
    ("BTN_A", 304),
    ("BTN_B", 305),
    ("BTN_X", 307),
    ("BTN_Y", 308),
    ("BTN_TRIGGER_HAPPY1", 704),
    ("BTN_TRIGGER_HAPPY2", 705),
    ("BTN_TRIGGER_HAPPY3", 706),
    ("BTN_TRIGGER_HAPPY4", 707),
    ("BTN_TRIGGER_HAPPY5", 708),
    ("BTN_TRIGGER_HAPPY6", 709),
    ("BTN_TRIGGER_HAPPY7", 710),
    ("BTN_TRIGGER_HAPPY8", 711),
    ("BTN_LEFT", 272),
    ("BTN_RIGHT", 273),
    ("BTN_MIDDLE", 274),
    ("BTN_SIDE", 275),
    ("BTN_EXTRA", 276),
    ("BTN_FORWARD", 277),
    ("BTN_BACK", 278),
    ("BTN_TASK", 279),
    ("KEY_ESC", 1),
    ("KEY_1", 2),
    ("KEY_2", 3),
    ("KEY_3", 4),
    ("KEY_4", 5),
    ("KEY_5", 6),
    ("KEY_6", 7),
    ("KEY_7", 8),
    ("KEY_8", 9),
    ("KEY_9", 10),
    ("KEY_0", 11),
    ("KEY_MINUS", 12),
    ("KEY_EQUAL", 13),
    ("KEY_BACKSPACE", 14),
    ("KEY_TAB", 15),
    ("KEY_Q", 16),
    ("KEY_W", 17),
    ("KEY_E", 18),
    ("KEY_R", 19),
    ("KEY_T", 20),
    ("KEY_Y", 21),
    ("KEY_U", 22),
    ("KEY_I", 23),
    ("KEY_O", 24),
    ("KEY_P", 25),
    ("KEY_LEFTBRACE", 26),
    ("KEY_RIGHTBRACE", 27),
    ("KEY_ENTER", 28),
    ("KEY_LEFTCTRL", 29),
    ("KEY_A", 30),
    ("KEY_S", 31),
    ("KEY_D", 32),
    ("KEY_F", 33),
    ("KEY_G", 34),
    ("KEY_H", 35),
    ("KEY_J", 36),
    ("KEY_K", 37),
    ("KEY_L", 38),
    ("KEY_SEMICOLON", 39),
    ("KEY_APOSTROPHE", 40),
    ("KEY_GRAVE", 41),
    ("KEY_LEFTSHIFT", 42),
    ("KEY_BACKSLASH", 43),
    ("KEY_Z", 44),
    ("KEY_X", 45),
    ("KEY_C", 46),
    ("KEY_V", 47),
    ("KEY_B", 48),
    ("KEY_N", 49),
    ("KEY_M", 50),
    ("KEY_COMMA", 51),
    ("KEY_DOT", 52),
    ("KEY_SLASH", 53),
    ("KEY_RIGHTSHIFT", 54),
    ("KEY_KPASTERISK", 55),
    ("KEY_LEFTALT", 56),
    ("KEY_SPACE", 57),
    ("KEY_CAPSLOCK", 58),
    ("KEY_F1", 59),
    ("KEY_F2", 60),
    ("KEY_F3", 61),
    ("KEY_F4", 62),
    ("KEY_F5", 63),
    ("KEY_F6", 64),
    ("KEY_F7", 65),
    ("KEY_F8", 66),
    ("KEY_F9", 67),
    ("KEY_F10", 68),
    ("KEY_NUMLOCK", 69),
    ("KEY_SCROLLLOCK", 70),
    ("KEY_KP7", 71),
    ("KEY_KP8", 72),
    ("KEY_KP9", 73),
    ("KEY_KPMINUS", 74),
    ("KEY_KP4", 75),
    ("KEY_KP5", 76),
    ("KEY_KP6", 77),
    ("KEY_KPPLUS", 78),
    ("KEY_KP1", 79),
    ("KEY_KP2", 80),
    ("KEY_KP3", 81),
    ("KEY_KP0", 82),
    ("KEY_KPDOT", 83),
    ("KEY_ZENKAKUHANKAKU", 85),
    ("KEY_102ND", 86),
    ("KEY_F11", 87),
    ("KEY_F12", 88),
    ("KEY_RO", 89),
    ("KEY_KATAKANA", 90),
    ("KEY_HIRAGANA", 91),
    ("KEY_HENKAN", 92),
    ("KEY_KATAKANAHIRAGANA", 93),
    ("KEY_MUHENKAN", 94),
    ("KEY_KPJPCOMMA", 95),
    ("KEY_KPENTER", 96),
    ("KEY_RIGHTCTRL", 97),
    ("KEY_KPSLASH", 98),
    ("KEY_SYSRQ", 99),
    ("KEY_RIGHTALT", 100),
    ("KEY_LINEFEED", 101),
    ("KEY_HOME", 102),
    ("KEY_UP", 103),
    ("KEY_PAGEUP", 104),
    ("KEY_LEFT", 105),
    ("KEY_RIGHT", 106),
    ("KEY_END", 107),
    ("KEY_DOWN", 108),
    ("KEY_PAGEDOWN", 109),
    ("KEY_INSERT", 110),
    ("KEY_DELETE", 111),
    ("KEY_MACRO", 112),
    ("KEY_MUTE", 113),
    ("KEY_VOLUMEDOWN", 114),
    ("KEY_VOLUMEUP", 115),
    ("KEY_POWER", 116),
    ("KEY_KPEQUAL", 117),
    ("KEY_KPPLUSMINUS", 118),
    ("KEY_PAUSE", 119),
    ("KEY_SCALE", 120),
    ("KEY_KPCOMMA", 121),
    ("KEY_HANGEUL", 122),
    ("KEY_HANJA", 123),
    ("KEY_YEN", 124),
    ("KEY_LEFTMETA", 125),
    ("KEY_RIGHTMETA", 126),
    ("KEY_COMPOSE", 127),
    ("KEY_STOP", 128),
    ("KEY_AGAIN", 129),
    ("KEY_PROPS", 130),
    ("KEY_UNDO", 131),
    ("KEY_FRONT", 132),
    ("KEY_COPY", 133),
    ("KEY_OPEN", 134),
    ("KEY_PASTE", 135),
    ("KEY_FIND", 136),
    ("KEY_CUT", 137),
    ("KEY_HELP", 138),
    ("KEY_MENU", 139),
    ("KEY_CALC", 140),
    ("KEY_SETUP", 141),
    ("KEY_SLEEP", 142),
    ("KEY_WAKEUP", 143),
    ("KEY_FILE", 144),
    ("KEY_SENDFILE", 145),
    ("KEY_DELETEFILE", 146),
    ("KEY_XFER", 147),
    ("KEY_PROG1", 148),
    ("KEY_PROG2", 149),
    ("KEY_WWW", 150),
    ("KEY_MSDOS", 151),
    ("KEY_COFFEE", 152),
    ("KEY_ROTATE_DISPLAY", 153),
    ("KEY_CYCLEWINDOWS", 154),
    ("KEY_MAIL", 155),
    ("KEY_BOOKMARKS", 156),
    ("KEY_COMPUTER", 157),
    ("KEY_BACK", 158),
    ("KEY_FORWARD", 159),
    ("KEY_CLOSECD", 160),
    ("KEY_EJECTCD", 161),
    ("KEY_EJECTCLOSECD", 162),
    ("KEY_NEXTSONG", 163),
    ("KEY_PLAYPAUSE", 164),
    ("KEY_PREVIOUSSONG", 165),
    ("KEY_STOPCD", 166),
    ("KEY_RECORD", 167),
    ("KEY_REWIND", 168),
    ("KEY_PHONE", 169),
    ("KEY_ISO", 170),
    ("KEY_CONFIG", 171),
    ("KEY_HOMEPAGE", 172),
    ("KEY_REFRESH", 173),
    ("KEY_EXIT", 174),
    ("KEY_MOVE", 175),
    ("KEY_EDIT", 176),
    ("KEY_SCROLLUP", 177),
    ("KEY_SCROLLDOWN", 178),
    ("KEY_KPLEFTPAREN", 179),
    ("KEY_KPRIGHTPAREN", 180),
    ("KEY_NEW", 181),
    ("KEY_REDO", 182),
    ("KEY_F13", 183),
    ("KEY_F14", 184),
    ("KEY_F15", 185),
    ("KEY_F16", 186),
    ("KEY_F17", 187),
    ("KEY_F18", 188),
    ("KEY_F19", 189),
    ("KEY_F20", 190),
    ("KEY_F21", 191),
    ("KEY_F22", 192),
    ("KEY_F23", 193),
    ("KEY_F24", 194),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HostProfile {
    pub button: String,
    pub sensitivity: f64,
    pub deadzone: f64,
    pub invert_x: bool,
    pub invert_y: bool,
    pub mappings: BTreeMap<String, String>,
    /// Source Linux button names to cycles per second, 1..=100 Hz.
    pub turbo: BTreeMap<String, f64>,
    /// Source Linux button names latched on rising edges.
    pub toggle: Vec<String>,
    pub stick_mouse: Option<StickMouse>,
    pub trigger_buttons: TriggerButtons,
    /// First held layer wins; every layer hold is consumed in every mode.
    pub layers: Vec<Layer>,
    /// Replaces gyro-to-right-stick, sharing its inverse clutch.
    pub gyro_mouse: Option<GyroMouse>,
    /// Opt in to grabbing the receiver's firmware gyro mouse interface.
    pub receiver_gyro: bool,
    /// Shared mouse/stick filters. Legacy sensitivity is an additional output gain;
    /// legacy deadzone becomes a soft tightening threshold, never a hard cutoff.
    pub gyro: GyroSettings,
    /// Linux: expose the controller to Steam Input as a virtual DualSense Edge (buttons,
    /// paddles, sticks, triggers, gyro) instead of the remapped uinput pad. Host remaps,
    /// layers and host gyro output are ignored; the slot key map still applies.
    pub steam: bool,
}

impl Default for HostProfile {
    fn default() -> Self {
        Self {
            button: "R4".into(),
            sensitivity: 1.0,
            deadzone: 0.5,
            invert_x: false,
            invert_y: false,
            mappings: BTreeMap::new(),
            turbo: BTreeMap::new(),
            toggle: Vec::new(),
            stick_mouse: None,
            trigger_buttons: TriggerButtons::default(),
            layers: Vec::new(),
            gyro_mouse: None,
            receiver_gyro: false,
            gyro: GyroSettings::default(),
            steam: false,
        }
    }
}

fn clutch_button(button: &str) -> Result<(u16, u32)> {
    match button {
        "R4" => Ok((705, 0x40000000)),
        "L4" => Ok((704, 0x00200000)),
        "PL" => Ok((309, 0x02000000)),
        "PR" => Ok((306, 0x04000000)),
        _ => Err(invalid("clutch button must be R4, L4, PL or PR")),
    }
}

fn key_code(name: &str) -> Result<u16> {
    DESTINATION_KEYS
        .iter()
        .find_map(|&(key, code)| (key == name).then_some(code))
        .ok_or_else(|| invalid(format!("unsupported Linux key name: {name}")))
}

fn receiver_clutch_button(button: &str) -> Result<u16> {
    RECEIVER_CLUTCH_KEYS.iter().find_map(|&(name, code)| (name == button).then_some(code))
        .ok_or_else(|| invalid("receiver gyro clutch must be R4, L4, PL, PR or a visible standard Linux button"))
}

/// USB cable: Null-mapped paddles produce no event anywhere (live 2026-10-20), so a paddle
/// clutch would silently never engage.
fn validate_cable_clutch(profile: &HostProfile) -> Result<()> {
    if profile.receiver_gyro && clutch_button(&profile.button).is_ok() {
        return Err(invalid(format!(
            "USB cable: paddle {} is not reported by the controller. Over the 2.4 GHz receiver or Bluetooth (the cable has no configuration link), map it to a standard button in the controller slot (Mapping page, then power-cycle the controller), and choose that button as the gyro clutch, e.g. R4→R3 with clutch BTN_THUMBR.",
            profile.button)));
    }
    Ok(())
}

impl HostProfile {
    /// Evdev transports without receiver gyro ignore Bluetooth clutch settings.
    pub fn validate_for_link(&self, link: LinkKind) -> Result<Option<&'static str>> {
        match link {
            LinkKind::Bluetooth => {
                self.validate()?;
                Ok(None)
            }
            LinkKind::Receiver if self.receiver_gyro => {
                self.validate_gyro_settings()?;
                self.validate_inner(Some(receiver_clutch_button(&self.button)?))?;
                Ok(Some(RECEIVER_GYRO_NOTICE))
            }
            LinkKind::Receiver | LinkKind::Wired => {
                self.validate_inner(None)?;
                Ok(Some(if link == LinkKind::Wired { WIRED_NOTICE } else { RECEIVER_NOTICE }))
            }
        }
    }

    /// Validate persisted settings independently of the selected transport.
    /// Clutch visibility and binding conflicts are checked by the selected runtime.
    pub fn validate_structure(&self) -> Result<()> {
        self.validate_gyro_settings()?;
        // Retain the selected receiver button when switching its opt-in back off.
        if self.receiver_gyro || clutch_button(&self.button).is_err() {
            receiver_clutch_button(&self.button)?;
        }
        self.validate_inner(None)
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_inner(Some(self.validate_clutch()?))
    }

    fn validate_clutch(&self) -> Result<u16> {
        let button = clutch_button(&self.button)?.0;
        self.validate_gyro_settings()?;
        Ok(button)
    }

    fn validate_gyro_settings(&self) -> Result<()> {
        self.gyro.validate()?;
        if !self.sensitivity.is_finite() || !(0.0 < self.sensitivity && self.sensitivity <= 100.0) {
            return Err(invalid(
                "sensitivity must be finite, greater than zero and at most 100",
            ));
        }
        if !self.deadzone.is_finite() || !(0.0..2000.0).contains(&self.deadzone) {
            return Err(invalid(
                "gyro deadzone must be finite and in [0, 2000) degrees/second",
            ));
        }
        Ok(())
    }

    fn validate_inner(&self, consumed: Option<u16>) -> Result<()> {
        Self::validate_controls(&self.mappings, &self.turbo, &self.toggle, self.stick_mouse, &self.trigger_buttons, consumed)?;
        if let Some(mouse) = self.gyro_mouse {
            if !mouse.sensitivity.is_finite() || !(0.0 < mouse.sensitivity && mouse.sensitivity <= 10000.0) {
                return Err(invalid("gyro mouse sensitivity must be finite and in (0, 10000] pixels/degree"));
            }
            if !mouse.deadzone.is_finite() || !(0.0..2000.0).contains(&mouse.deadzone) {
                return Err(invalid("gyro mouse deadzone must be finite and in [0, 2000) degrees/second"));
            }
        }
        let mut holds = 0u32;
        for layer in &self.layers {
            let index = validate_source(&layer.hold, consumed)?;
            if holds & (1 << index) != 0 {
                return Err(invalid("duplicate layer hold button"));
            }
            holds |= 1 << index;
            for source in layer.mappings.keys().chain(layer.turbo.keys()).chain(layer.toggle.iter()) {
                if validate_source(source, consumed)? == index {
                    return Err(invalid("layer hold cannot be mapped, turbo or toggle in that layer"));
                }
            }
            Self::validate_controls(&layer.mappings, &layer.turbo, &layer.toggle, layer.stick_mouse, &layer.trigger_buttons, consumed)?;
        }
        Ok(())
    }

    fn validate_controls(
        mappings: &BTreeMap<String, String>,
        turbo: &BTreeMap<String, f64>,
        toggle: &[String],
        stick_mouse: Option<StickMouse>,
        trigger_buttons: &TriggerButtons,
        consumed: Option<u16>,
    ) -> Result<()> {
        let mut seen = 0u32;
        for (source, destination) in mappings {
            let code = key_code(source)?;
            let index = KEY_CODES
                .iter()
                .position(|&key| key == code)
                .ok_or_else(|| {
                    invalid(format!(
                        "mapping source is not a controller button: {source}"
                    ))
                })?;
            if Some(code) == consumed {
                return Err(invalid(format!("{source} is consumed by the selected clutch")));
            }
            if seen & (1 << index) != 0 {
                return Err(invalid(format!(
                    "multiple aliases map the same source button: {source}"
                )));
            }
            seen |= 1 << index;
            key_code(destination)?;
        }
        for (source, rate) in turbo {
            if !rate.is_finite() || !(1.0..=100.0).contains(rate) {
                return Err(invalid("turbo rate must be finite and in [1, 100] Hz"));
            }
            validate_source(source, consumed)?;
        }
        let mut toggles = 0u32;
        for source in toggle {
            let index = validate_source(source, consumed)?;
            if toggles & (1 << index) != 0 {
                return Err(invalid("duplicate toggle source button"));
            }
            toggles |= 1 << index;
        }
        let mut turbo_sources = 0u32;
        for source in turbo.keys() {
            let index = validate_source(source, consumed)?;
            if turbo_sources & (1 << index) != 0 {
                return Err(invalid("duplicate turbo source button"));
            }
            turbo_sources |= 1 << index;
        }
        if let Some(mouse) = stick_mouse {
            if !mouse.sensitivity.is_finite() || !(0.0 < mouse.sensitivity && mouse.sensitivity <= 10000.0) {
                return Err(invalid("stick mouse sensitivity must be finite and in (0, 10000] pixels/second"));
            }
            if !mouse.deadzone.is_finite() || !(0.0..1.0).contains(&mouse.deadzone) {
                return Err(invalid("stick mouse deadzone must be finite and in [0, 1)"));
            }
        }
        for trigger in [&trigger_buttons.left, &trigger_buttons.right].into_iter().flatten() {
            key_code(&trigger.destination)?;
            if !trigger.threshold.is_finite() || !(0.0 < trigger.threshold && trigger.threshold <= 1.0) {
                return Err(invalid("trigger threshold must be finite and in (0, 1]"));
            }
        }
        Ok(())
    }
}

fn validate_source(source: &str, consumed: Option<u16>) -> Result<usize> {
    let code = key_code(source)?;
    if Some(code) == consumed {
        return Err(invalid("clutch button cannot have turbo or toggle"));
    }
    KEY_CODES.iter().position(|&key| key == code)
        .ok_or_else(|| invalid("source is not a controller button"))
}

/// Linux input_absinfo layout; AXES determines array order.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisInfo {
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

/// Explicit offline/replay defaults matching test_gyro_clutch.py; runtime queries evdev.
pub fn default_axes() -> [AxisInfo; 8] {
    let stick = AxisInfo {
        value: 127,
        minimum: 0,
        maximum: 255,
        flat: 15,
        fuzz: 0,
        resolution: 0,
    };
    let trigger = AxisInfo {
        value: 0,
        flat: 0,
        ..stick
    };
    let hat = AxisInfo {
        value: 0,
        minimum: -1,
        maximum: 1,
        flat: 0,
        fuzz: 0,
        resolution: 0,
    };
    [stick, stick, stick, stick, trigger, trigger, hat, hat]
}

fn validate_axes(axes: &[AxisInfo; 8]) -> Result<()> {
    for (index, axis) in axes.iter().enumerate() {
        let expected = if index < 6 { (0, 255) } else { (-1, 1) };
        if (axis.minimum, axis.maximum) != expected {
            return Err(invalid(format!(
                "ABS {} bounds do not match the Bluetooth report",
                AXES[index]
            )));
        }
        if axis.value < axis.minimum
            || axis.value > axis.maximum
            || axis.fuzz < 0
            || axis.resolution < 0
            || axis.flat < 0
            || axis.flat > axis.maximum - axis.minimum
        {
            return Err(invalid(format!("invalid ABS {} metadata", AXES[index])));
        }
    }
    for index in [2, 3] {
        if axes[index].flat >= 127 {
            return Err(invalid("right-stick Linux deadzone must be in [0, 127)"));
        }
    }
    Ok(())
}

fn validate_source_axes(axes: &[AxisInfo; 8], link: LinkKind) -> Result<()> {
    if link == LinkKind::Bluetooth {
        return validate_axes(axes);
    }
    for (index, axis) in axes.iter().enumerate() {
        let expected = if index < 4 { (-32768, 32767) } else if index < 6 {
            (0, if link == LinkKind::Wired { 1023 } else { 255 })
        } else { (-1, 1) };
        if (axis.minimum, axis.maximum) != expected
            || !(axis.minimum..=axis.maximum).contains(&axis.value)
            || axis.fuzz < 0 || axis.flat < 0 || axis.flat > axis.maximum - axis.minimum
            || axis.resolution < 0 {
            return Err(invalid("evdev axis metadata does not match the selected controller transport"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

/// One atomic EV_SYN-terminated update, serialized as only its populated events.
#[derive(Clone, Debug)]
pub struct Frame {
    events: [Event; MAX_EVENTS],
    len: usize,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            events: [Event {
                kind: 0,
                code: 0,
                value: 0,
            }; MAX_EVENTS],
            len: 0,
        }
    }
}

impl Serialize for Frame {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.as_slice().serialize(serializer)
    }
}

impl Frame {
    pub fn as_slice(&self) -> &[Event] {
        &self.events[..self.len]
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn len(&self) -> usize {
        self.len
    }
    fn push(&mut self, kind: u16, code: u16, value: i32) {
        // Fixed capacity includes trigger destinations and relative mouse motion.
        self.events[self.len] = Event { kind, code, value };
        self.len += 1;
    }
    fn sync(&mut self) {
        if !self.is_empty() {
            self.push(0, 0, 0);
        }
    }
}

struct Mode {
    hold_mask: u32,
    destination_codes: [u16; BUTTONS],
    destination_masks: [u32; BUTTONS],
    destination_count: usize,
    turbo_rates: [f64; BUTTONS],
    toggle_mask: u32,
    mouse: Option<StickMouse>,
    trigger_thresholds: [Option<f64>; 2],
}

impl Mode {
    fn compile(profile: &HostProfile, layer: Option<&Layer>, consumed: u32) -> Result<Self> {
        let mut mode = Self {
            hold_mask: 0,
            destination_codes: [0; BUTTONS],
            destination_masks: [0; BUTTONS],
            destination_count: 0,
            turbo_rates: [0.0; BUTTONS],
            toggle_mask: 0,
            mouse: layer.and_then(|layer| layer.stick_mouse).or(profile.stick_mouse),
            trigger_thresholds: [None; 2],
        };
        let mut mapping = [0; BUTTONS];
        mapping[..KEY_CODES.len()].copy_from_slice(&KEY_CODES);
        // A nonempty layer field replaces the corresponding complete base field.
        let mappings = layer.filter(|layer| !layer.mappings.is_empty()).map_or(&profile.mappings, |layer| &layer.mappings);
        let turbo = layer.filter(|layer| !layer.turbo.is_empty()).map_or(&profile.turbo, |layer| &layer.turbo);
        let toggle = layer.filter(|layer| !layer.toggle.is_empty()).map_or(&profile.toggle, |layer| &layer.toggle);
        let triggers = layer.filter(|layer| layer.trigger_buttons.left.is_some() || layer.trigger_buttons.right.is_some())
            .map_or(&profile.trigger_buttons, |layer| &layer.trigger_buttons);
        for (source, destination) in mappings {
            mapping[validate_source(source, None)?] = key_code(destination)?;
        }
        for (source, &rate) in turbo {
            mode.turbo_rates[validate_source(source, None)?] = rate;
        }
        for source in toggle {
            mode.toggle_mask |= 1 << validate_source(source, None)?;
        }
        for (index, trigger) in [&triggers.left, &triggers.right].into_iter().enumerate() {
            if let Some(trigger) = trigger {
                mapping[24 + index] = key_code(&trigger.destination)?;
                mode.trigger_thresholds[index] = Some(trigger.threshold);
            }
        }
        if let Some(layer) = layer {
            mode.hold_mask = 1 << validate_source(&layer.hold, None)?;
        }
        for (index, code) in mapping.into_iter().enumerate() {
            if code == 0 || consumed & (1 << index) != 0 {
                continue;
            }
            let destination = mode.destination_codes[..mode.destination_count].iter()
                .position(|&key| key == code).unwrap_or(mode.destination_count);
            if destination == mode.destination_count {
                mode.destination_codes[destination] = code;
                mode.destination_count += 1;
            }
            mode.destination_masks[destination] |= 1 << index;
        }
        Ok(mode)
    }
}

pub struct Mapper {
    axes: [AxisInfo; 8],
    /// Output ABS codes mirror the source layout: Bluetooth HID (right stick Z/RZ, triggers
    /// GAS/BRAKE) or xpad/xone (right stick RX/RY, triggers Z/RZ). Live: emitting the BT
    /// layout on the receiver made browsers/SDL read the right stick as the triggers.
    axis_codes: [u16; 8],
    physical_mask: u32,
    consumed_mask: u32,
    sensitivity: f64,
    deadzone: f64,
    signs: [f64; 2],
    held: bool,
    physical_right: [i32; 2],
    gyro_right: [f64; 2],
    values: [Option<i32>; 8],
    down_codes: [u16; BUTTONS],
    down_count: usize,
    modes: Vec<Mode>,
    /// EV_KEY codes the virtual gamepad advertises. Browsers/SDL number generic-pad buttons by
    /// advertised code order, so receiver/wired pads must mirror xpad's 11 buttons (plus explicit
    /// gamepad destinations); the 24-code Bluetooth set shifted every button live.
    gamepad_keys: Vec<u16>,
    mode: usize,
    closed: bool,
    link: LinkKind,
    origin: Instant,
    raw_pressed: u32,
    previous_pressed: u32,
    latched: u32,
    active: u32,
    turbo_started: [Duration; BUTTONS],
    analog: [i32; 8],
    pending_analog: [i32; 8],
    pending_pressed: u32,
    mouse: Option<StickMouse>,
    mouse_fraction: [f64; 2],
    gyro_mouse: Option<GyroMouse>,
    gyro_velocity: [f64; 2],
    gyro_fraction: [f64; 2],
    last_tick: Duration,
    receiver_gyro: bool,
    receiver_pending: [i32; 2],
    receiver_last_sample: Option<Duration>,
    receiver_motion_at: Option<Duration>,
    receiver_paddles: crate::receiver::Paddles,
    slot_map: Option<crate::profile::KeyMap>,
    physical_buttons: Option<u32>,
    source_analog: [i32; 8],
    angular: Option<[f64; 3]>,
    receiver_clutch_physical: Option<u32>,
    gyro_settings: GyroSettings,
    gyro_filter: GyroFilter,
    gyro_sample_at: Option<Duration>,
    receiver_interval: f64,
}

impl Mapper {
    pub fn new(profile: &HostProfile, axes: [AxisInfo; 8]) -> Result<Self> {
        Self::new_for_link(profile, axes, LinkKind::Bluetooth)
    }

    /// `axes` order: LX, LY, RX, RY, LT, RT, hat X, hat Y.
    pub fn new_receiver(profile: &HostProfile, axes: [AxisInfo; 8]) -> Result<Self> {
        Self::new_for_link(profile, axes, LinkKind::Receiver)
    }

    /// xone GIP axes: signed sticks, 10-bit triggers, then hat X/Y.
    pub fn new_wired(profile: &HostProfile, axes: [AxisInfo; 8]) -> Result<Self> {
        Self::new_for_link(profile, axes, LinkKind::Wired)
    }

    fn new_for_link(profile: &HostProfile, axes: [AxisInfo; 8], link: LinkKind) -> Result<Self> {
        Self::new_with_receiver_gyro(profile, axes, link, link == LinkKind::Receiver && profile.receiver_gyro)
    }

    fn new_with_receiver_gyro(profile: &HostProfile, axes: [AxisInfo; 8], link: LinkKind, receiver_gyro: bool) -> Result<Self> {
        profile.validate_for_link(link)?;
        validate_source_axes(&axes, link)?;
        let (mut consumed_mask, physical_mask) = if link == LinkKind::Bluetooth {
            let (button, physical_mask) = clutch_button(&profile.button)?;
            (1 << KEY_CODES.iter().position(|&key| key == button).unwrap(), physical_mask)
        } else if receiver_gyro {
            let button = receiver_clutch_button(&profile.button)?;
            let mask = 1 << KEY_CODES.iter().position(|&key| key == button).unwrap();
            (mask, mask)
        } else {
            (0, 0)
        };
        for layer in &profile.layers {
            consumed_mask |= 1 << validate_source(&layer.hold, None)?;
        }
        let mut modes = Vec::with_capacity(profile.layers.len() + 1);
        modes.push(Mode::compile(profile, None, consumed_mask)?);
        for layer in &profile.layers {
            modes.push(Mode::compile(profile, Some(layer), consumed_mask)?);
        }
        let gamepad_keys = if link == LinkKind::Bluetooth { KEY_CODES.to_vec() } else {
            let mut keys = RECEIVER_EVDEV_KEYS.to_vec();
            let triggers = |t: &TriggerButtons| [&t.left, &t.right].into_iter().flatten().map(|t| t.destination.clone()).collect::<Vec<_>>();
            let explicit = profile.mappings.values().cloned().chain(triggers(&profile.trigger_buttons))
                .chain(profile.layers.iter().flat_map(|layer| layer.mappings.values().cloned().chain(triggers(&layer.trigger_buttons))));
            for name in explicit {
                let code = key_code(&name)?;
                if code >= 304 && !keys.contains(&code) { keys.push(code); }
            }
            keys.sort_unstable();
            keys
        };
        Ok(Self {
            gamepad_keys,
            axes,
            axis_codes: if link == LinkKind::Bluetooth { AXES } else { RECEIVER_AXES },
            physical_mask,
            consumed_mask,
            sensitivity: profile.sensitivity,
            deadzone: profile.deadzone,
            signs: [
                if profile.invert_x { -1.0 } else { 1.0 },
                if profile.invert_y { 1.0 } else { -1.0 },
            ],
            held: false,
            physical_right: [0; 2],
            gyro_right: [0.0; 2],
            values: [None; 8],
            down_codes: [0; BUTTONS],
            down_count: 0,
            modes,
            mode: 0,
            closed: false,
            link,
            origin: Instant::now(),
            raw_pressed: 0,
            previous_pressed: 0,
            latched: 0,
            active: 0,
            turbo_started: [Duration::ZERO; BUTTONS],
            analog: std::array::from_fn(|index| neutral(index, axes[index])),
            pending_analog: std::array::from_fn(|index| neutral(index, axes[index])),
            pending_pressed: 0,
            mouse: profile.stick_mouse,
            mouse_fraction: [0.0; 2],
            gyro_mouse: if link == LinkKind::Bluetooth || receiver_gyro { profile.gyro_mouse } else { None },
            gyro_velocity: [0.0; 2],
            gyro_fraction: [0.0; 2],
            last_tick: Duration::ZERO,
            receiver_gyro,
            receiver_pending: [0; 2],
            receiver_last_sample: None,
            receiver_motion_at: None,
            receiver_paddles: crate::receiver::Paddles::default(),
            slot_map: None,
            physical_buttons: None,
            source_analog: std::array::from_fn(|index| neutral(index, axes[index])),
            angular: None,
            receiver_clutch_physical: clutch_button(&profile.button).ok().map(|(_, mask)| mask),
            gyro_settings: profile.gyro,
            gyro_filter: GyroFilter::default(),
            gyro_sample_at: None,
            receiver_interval: 0.004,
        })
    }

    pub fn set_key_map(&mut self, map: Option<crate::profile::KeyMap>) {
        self.slot_map = map;
    }

    /// Latest source sample, before filters/inversion; no roll or acceleration sensor.
    pub fn angular_velocity(&self) -> Option<[f64; 3]> { self.angular }

    pub fn physical_state(&self) -> PhysicalState {
        PhysicalState { buttons: self.raw_pressed, axes: self.analog }
    }

    fn apply_slot_map(&mut self) {
        use crate::profile::{PhysicalButton, MappingTarget};
        let Some(mut physical) = self.physical_buttons else { return; };
        if self.link == LinkKind::Receiver { physical = self.receiver_physical(); }
        let target = |source: PhysicalButton| self.slot_map.as_ref().map_or_else(
            || MappingTarget::from_raw(source.mask().bits()), |map| map.get(source));
        let mut keys = 0;
        let mut targets = 0;
        for source in PhysicalButton::ALL {
            if physical & source.mask().bits() == 0 { continue; }
            let mapped = target(source).raw();
            if mapped == 0 || (self.slot_map.is_none() && source.mask().bits() > 0xffff) {
                for (index, mask) in PADDLE_SOURCES {
                    if mask == source.mask().bits() { keys |= 1 << index; }
                }
            } else {
                targets |= mapped;
            }
        }
        for (mask, code) in [(0x2000,304),(0x1000,305),(0x10,307),(0x20,308),
            (0x400,310),(0x800,311),(8,314),(1,315),(2,317),(4,318)] {
            if targets & mask != 0 { keys |= 1 << KEY_CODES.iter().position(|&k| k == code).unwrap(); }
        }
        // Home (BTN_MODE) has no slot-map source; receiver keeps it from xpad.
        let home = 1 << KEY_CODES.iter().position(|&k| k == 316).unwrap();
        self.raw_pressed = keys | if self.link == LinkKind::Receiver { self.pending_pressed & home } else { 0 };
        self.analog = self.source_analog;
        self.analog[6] = i32::from(targets & 0x40 != 0) - i32::from(targets & 0x80 != 0);
        self.analog[7] = i32::from(targets & 0x100 != 0) - i32::from(targets & 0x200 != 0);
        for (source, mask, index) in [(PhysicalButton::Lt,0x4000,0),(PhysicalButton::Rt,0x8000,1)] {
            let slot = if self.link == LinkKind::Bluetooth { 5 - index } else { 4 + index };
            self.analog[slot] = if target(source).raw() == mask {
                self.source_analog[slot]
            } else { self.axes[slot].minimum };
            if PhysicalButton::ALL.into_iter().any(|other| other != source
                && physical & other.mask().bits() != 0 && target(other).raw() == mask) {
                self.analog[slot] = self.axes[slot].maximum;
            }
        }
    }

    /// Receiver source buttons once the heartbeat runs: standard buttons from xpad (raw,
    /// unremapped while the heartbeat runs; reliable), paddles from physical reports.
    /// Live 2026-10-19: physical `81 1c` reports drop presses/releases, so using them for
    /// standard buttons left the virtual pad stuck or silent.
    fn receiver_physical(&self) -> u32 {
        const XPAD: [(u16, u32); 10] = [(304, 0x2000), (305, 0x1000), (307, 0x10), (308, 0x20),
            (310, 0x400), (311, 0x800), (314, 8), (315, 1), (317, 2), (318, 4)];
        let mut physical = self.receiver_paddles.buttons();
        for (code, mask) in XPAD {
            let index = KEY_CODES.iter().position(|&key| key == code).unwrap();
            if self.pending_pressed & 1 << index != 0 { physical |= mask; }
        }
        physical |= match self.source_analog[6].signum() { -1 => 0x80, 1 => 0x40, _ => 0 };
        physical |= match self.source_analog[7].signum() { -1 => 0x200, 1 => 0x100, _ => 0 };
        // chisle: 10% digital threshold; firmware's own LT/RT button threshold unknown.
        for (index, mask) in [(4, 0x4000), (5, 0x8000)] {
            let info = &self.axes[index];
            if self.source_analog[index] - info.minimum > (info.maximum - info.minimum) / 10 { physical |= mask; }
        }
        physical
    }

    fn process_gyro(&mut self, angular: [f64; 3], dt: f64) {
        self.angular = Some(angular);
        let tightening = self.gyro_mouse.map_or(self.deadzone, |m| m.deadzone);
        let velocity = self.gyro_filter.sample([angular[1], -angular[0]], dt, self.gyro_settings, tightening);
        if let Some(mouse) = self.gyro_mouse {
            self.gyro_velocity = [velocity[0] * mouse.sensitivity * if mouse.invert_x { -1.0 } else { 1.0 },
                velocity[1] * mouse.sensitivity * if mouse.invert_y { -1.0 } else { 1.0 }];
            self.gyro_right = [0.0; 2];
        } else {
            let target = self.gyro_settings.stick(velocity.map(|v| v * self.sensitivity));
            let blend = if self.gyro_settings.stick_smoothing_ms > 0.0 {
                1.0 - (-dt * 1000.0 / self.gyro_settings.stick_smoothing_ms).exp()
            } else { 1.0 };
            for index in 0..2 {
                // velocity already has Y-down; signs[1] retains legacy inversion.
                let sign = if index == 0 { self.signs[0] } else { -self.signs[1] };
                self.gyro_right[index] += (target[index] * sign - self.gyro_right[index]) * blend;
            }
        }
    }

    fn gyro_pixels(&mut self, frame: &mut Frame, dt: f64) {
        if self.held { self.gyro_fraction = [0.0; 2]; return; }
        for index in 0..2 {
            let motion = self.gyro_fraction[index] + self.gyro_velocity[index] * dt;
            let pixels = motion.trunc().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
            self.gyro_fraction[index] = motion - f64::from(pixels);
            if pixels != 0 { frame.push(2, index as u16, pixels); }
        }
    }

    fn axis(&mut self, frame: &mut Frame, index: usize, value: i32) {
        if self.mouse.is_some_and(|mouse| {
            let start = if mouse.stick == Stick::Left { 0 } else { 2 };
            index == start || index == start + 1
        }) {
            return;
        }
        if self.values[index] != Some(value) {
            self.values[index] = Some(value);
            frame.push(3, self.axis_codes[index], value);
        }
    }

    fn right(&mut self, frame: &mut Frame) {
        for index in 0..2 {
            let motion = if self.held {
                0.0
            } else {
                self.gyro_right[index]
            };
            let mixed = (f64::from(self.physical_right[index]) + motion)
                .round_ties_even()
                .clamp(-32768.0, 32767.0);
            self.axis(frame, index + 2, mixed as i32);
        }
    }

    pub fn physical_input(&mut self, state: PhysicalInput) -> Frame {
        self.physical_input_at(state, self.origin.elapsed())
    }

    /// Clock-injected physical/clutch update; integrates motion up to this edge.
    pub fn physical_input_at(&mut self, state: PhysicalInput, now: Duration) -> Frame {
        let mut frame = self.tick_frame(now);
        if self.closed {
            return frame;
        }
        self.physical_buttons = Some(state.buttons);
        self.source_analog = [i32::from(state.left_x), i32::from(state.left_y),
            stick_value(state.right_x, self.axes[2]), stick_value(state.right_y, self.axes[3]),
            i32::from(state.right_trigger), i32::from(state.left_trigger), 0, 0];
        self.apply_slot_map();
        self.held = state.buttons & self.physical_mask != 0;
        if self.held { self.gyro_fraction = [0.0; 2]; }
        self.physical_right.copy_from_slice(&self.analog[2..4]);
        self.buttons(&mut frame, now.max(self.last_tick));
        for index in [0, 1, 4, 5, 6, 7] { self.axis(&mut frame, index, self.analog[index]); }
        self.right(&mut frame);
        frame.sync();
        frame
    }

    /// Heartbeat reports are authoritative for every button, not only paddles.
    pub fn receiver_physical_at(&mut self, state: PhysicalInput, now: Duration) -> Result<Frame> {
        if self.closed || self.link != LinkKind::Receiver {
            return Err(invalid("physical receiver report supplied to the wrong mapper"));
        }
        self.receiver_paddles.update(state);
        self.physical_buttons = Some(state.buttons);
        self.apply_slot_map();
        let mut frame = self.tick_frame(now);
        self.buttons(&mut frame, now.max(self.last_tick));
        for index in [4, 5, 6, 7] { self.axis(&mut frame, index, self.analog[index]); }
        frame.sync();
        Ok(frame)
    }

    fn update_receiver_paddles(&mut self) {
        if self.link != LinkKind::Receiver { return; }
        self.apply_slot_map();
        if self.receiver_gyro {
            self.held = self.receiver_clutch_held();
            if self.held { self.gyro_right = [0.0; 2]; self.gyro_fraction = [0.0; 2]; }
        }
    }

    fn receiver_clutch_held(&self) -> bool {
        if let (Some(_), Some(mask)) = (self.physical_buttons, self.receiver_clutch_physical) {
            self.receiver_physical() & mask != 0
        } else { self.raw_pressed & self.physical_mask != 0 }
    }

    pub fn input(&mut self, report: &[u8]) -> Result<Frame> {
        self.input_at(report, self.origin.elapsed())
    }

    /// Clock-injected Bluetooth report processing; time must be monotonic.
    pub fn input_at(&mut self, report: &[u8], now: Duration) -> Result<Frame> {
        if self.closed {
            return Err(invalid("mapper is closed"));
        }
        if self.link != LinkKind::Bluetooth {
            return Err(invalid("Bluetooth report supplied to receiver mapper"));
        }
        if report.len() != 32 || report[0] != 1 {
            return Err(invalid(
                "expected a 32-byte Ultimate 3 Bluetooth report (ID 0x01)",
            ));
        }
        let mut frame = self.tick_frame(now);
        if self.physical_buttons.is_none() {
            self.raw_pressed = u32::from(report[8]) | u32::from(report[9]) << 8 | u32::from(report[10]) << 16;
        }
        let pitch = f64::from(i16::from_le_bytes([report[23], report[24]])) * (2000.0 / 32767.0);
        let yaw = f64::from(i16::from_le_bytes([report[25], report[26]])) * (2000.0 / 32767.0);
        let dt = self.gyro_sample_at.map_or(0.004, |at| now.saturating_sub(at).as_secs_f64()).clamp(0.0001, 0.250);
        self.gyro_sample_at = Some(now);
        self.process_gyro([-pitch, -yaw, 0.0], dt);
        const HATS: [(i32, i32); 8] = [
            (0, -1),
            (1, -1),
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
        ];
        let (hat_x, hat_y) = HATS
            .get(usize::from(report[1] & 15))
            .copied()
            .unwrap_or((0, 0));
        self.source_analog[0] = i32::from(report[2]);
        self.source_analog[1] = i32::from(report[3]);
        self.source_analog[4] = i32::from(report[6]);
        self.source_analog[5] = i32::from(report[7]);
        self.analog = self.source_analog;
        self.analog[6] = hat_x;
        self.analog[7] = hat_y;
        self.apply_slot_map();
        self.buttons(&mut frame, now.max(self.last_tick));
        for index in [0, 1, 4, 5, 6, 7] { self.axis(&mut frame, index, self.analog[index]); }
        // HID right-stick bytes include firmware motion; use config physical sticks.
        self.right(&mut frame);
        frame.sync();
        Ok(frame)
    }

    fn buttons(&mut self, frame: &mut Frame, now: Duration) {
        let mode = self.modes.iter().position(|mode| mode.hold_mask & self.raw_pressed != 0).unwrap_or(0);
        if mode != self.mode {
            self.mode = mode;
            self.latched = 0;
            self.previous_pressed = 0;
            self.active = 0;
            self.mouse_fraction = [0.0; 2];
            let old_stick = self.mouse.map(|mouse| mouse.stick);
            self.mouse = self.modes[mode].mouse;
            let new_stick = self.mouse.map(|mouse| mouse.stick);
            if old_stick != new_stick {
                for stick in [old_stick, new_stick].into_iter().flatten() {
                    let start = if stick == Stick::Left { 0 } else { 2 };
                    for index in start..start + 2 {
                        // Entering mouse mode must release a previously deflected
                        // virtual stick; leaving restores the physical value.
                        let value = if Some(stick) == new_stick { neutral(index, self.axes[index]) } else { self.analog[index] };
                        if self.values[index] != Some(value) {
                            self.values[index] = Some(value);
                            frame.push(3, self.axis_codes[index], value);
                        }
                    }
                }
            }
        }
        let mode = &self.modes[self.mode];
        let mut pressed = self.raw_pressed & !self.consumed_mask;
        for index in 0..2 {
            if let Some(threshold) = mode.trigger_thresholds[index] {
                // Analog slots 4/5: receiver/wired ABS_Z=LT, ABS_RZ=RT; Bluetooth HID
                // Accelerator→ABS_GAS=RT, Brake→ABS_BRAKE=LT (live kernel decode).
                let slot = if self.link == LinkKind::Bluetooth { 5 - index } else { 4 + index };
                let axis = self.axes[slot];
                let value = f64::from(self.analog[slot] - axis.minimum)
                    / f64::from(axis.maximum - axis.minimum);
                if value >= threshold {
                    pressed |= 1 << (24 + index);
                }
            }
        }
        self.latched ^= (pressed & !self.previous_pressed) & mode.toggle_mask;
        self.previous_pressed = pressed;
        let active = (pressed & !mode.toggle_mask) | self.latched;
        let mut output = active;
        for index in 0..BUTTONS {
            let mask = 1 << index;
            let rate = mode.turbo_rates[index];
            if active & mask != 0 && self.active & mask == 0 {
                self.turbo_started[index] = now;
            }
            if active & mask != 0 && rate > 0.0 {
                let phase = now.saturating_sub(self.turbo_started[index]).as_secs_f64() * rate;
                if phase.fract() >= 0.5 {
                    output &= !mask;
                }
            }
        }
        self.active = active;
        let mut next = [0; BUTTONS];
        let mut count = 0;
        for index in 0..mode.destination_count {
            if output & mode.destination_masks[index] != 0 {
                next[count] = mode.destination_codes[index];
                count += 1;
            }
        }
        // Release stale destinations before pressing replacements; shared
        // many-to-one outputs retain ownership across a mode switch.
        for &code in &self.down_codes[..self.down_count] {
            if !next[..count].contains(&code) {
                frame.push(1, code, 0);
            }
        }
        for &code in &next[..count] {
            if !self.down_codes[..self.down_count].contains(&code) {
                frame.push(1, code, 1);
            }
        }
        self.down_codes[..count].copy_from_slice(&next[..count]);
        self.down_count = count;
    }

    fn tick_frame(&mut self, now: Duration) -> Frame {
        let mut frame = Frame::default();
        if self.closed {
            return frame;
        }
        // Backward clocks cannot move the integration origin or turbo phase.
        let now = now.max(self.last_tick);
        let elapsed = now.saturating_sub(self.last_tick).as_secs_f64();
        self.last_tick = now;
        let was_held = self.held;
        self.update_receiver_paddles();
        if self.receiver_gyro && self.held != was_held { self.right(&mut frame); }
        if let Some(mouse) = self.mouse {
            let start = if mouse.stick == Stick::Left { 0 } else { 2 };
            let mut vector = [0.0; 2];
            for (offset, value) in vector.iter_mut().enumerate() {
                let index = start + offset;
                if index == 2 || index == 3 {
                    *value = f64::from(self.analog[index]) / if self.analog[index] < 0 { 32768.0 } else { 32767.0 };
                } else {
                    let axis = self.axes[index];
                    let center = neutral(index, axis);
                    let span = if self.analog[index] < center { center - axis.minimum } else { axis.maximum - center };
                    *value = f64::from(self.analog[index] - center) / f64::from(span);
                }
            }
            let magnitude = vector[0].hypot(vector[1]);
            let gain = if magnitude > mouse.deadzone {
                ((magnitude.min(1.0) - mouse.deadzone) / (1.0 - mouse.deadzone)) / magnitude
            } else {
                0.0
            };
            for (index, value) in vector.into_iter().enumerate() {
                if gain == 0.0 {
                    self.mouse_fraction[index] = 0.0;
                    continue;
                }
                let motion = self.mouse_fraction[index] + value * gain * mouse.sensitivity * elapsed;
                let pixels = motion.trunc().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
                self.mouse_fraction[index] = motion - f64::from(pixels);
                if pixels != 0 {
                    frame.push(2, index as u16, pixels);
                }
            }
        }
        if self.link == LinkKind::Bluetooth && self.gyro_mouse.is_some() {
            self.gyro_pixels(&mut frame, elapsed);
        }
        if self.receiver_gyro && self.receiver_motion_at.is_some_and(|at|
            now.saturating_sub(at).as_secs_f64() > self.receiver_interval * 2.0) {
            // No sample for two measured report periods: decay smoothly, never insert
            // a hard zero between normal 4-ms reports. Stop exactly once below one count.
            if self.gyro_mouse.is_some() {
                // Drain only the smoothing tail, never replay the last firmware REL
                // velocity. Otherwise the last slow movement would be lost at rest.
                let dt = now.saturating_sub(self.gyro_sample_at.unwrap_or(now)).as_secs_f64();
                self.gyro_sample_at = Some(now);
                self.process_gyro([0.0; 3], dt);
                self.gyro_pixels(&mut frame, dt);
                let idle = now.saturating_sub(self.receiver_motion_at.unwrap()).as_secs_f64();
                if idle >= self.gyro_settings.smooth_time_ms / 1000.0 + self.receiver_interval * 2.0 {
                    self.receiver_motion_at = None;
                    self.gyro_filter = GyroFilter::default();
                }
            } else {
                let decay = (-elapsed / (self.gyro_settings.stick_smoothing_ms / 1000.0).max(self.receiver_interval)).exp();
                for value in &mut self.gyro_right { *value *= decay; if value.abs() < 0.5 { *value = 0.0; } }
                self.angular = Some([0.0; 3]);
                if self.gyro_right == [0.0; 2] {
                    self.receiver_motion_at = None;
                    self.gyro_filter = GyroFilter::default();
                }
                self.right(&mut frame);
            }
        }
        frame
    }

    /// Service turbo and linear stick mouse while the source is idle.
    pub fn tick(&mut self, now: Duration) -> Frame {
        let mut frame = self.tick_frame(now);
        if !self.closed {
            self.buttons(&mut frame, now.max(self.last_tick));
        }
        frame.sync();
        frame
    }

    /// Consume receiver xpad or wired xone events; SYN_REPORT commits atomically.
    /// SYN_DROPPED is fatal: the runtime closes/releases rather than latching.
    pub fn receiver_event(&mut self, event: Event, now: Duration) -> Result<Frame> {
        if self.closed || self.link == LinkKind::Bluetooth {
            return Err(invalid("evdev mapper is closed or has the wrong source"));
        }
        match (event.kind, event.code) {
            (0, 3) => return Err(invalid("evdev queue overran (SYN_DROPPED)")),
            (1, code) => {
                if self.link == LinkKind::Receiver && [704, 705, 309, 306].contains(&code) {
                    return Ok(Frame::default());
                }
                if let Some(index) = KEY_CODES.iter().position(|&key| key == code) {
                    if event.value == 1 { self.pending_pressed |= 1 << index; }
                    if event.value == 0 { self.pending_pressed &= !(1 << index); }
                }
            }
            (3, code) => {
                if let Some(index) = RECEIVER_AXES.iter().position(|&axis| axis == code) {
                    self.pending_analog[index] = event.value.clamp(self.axes[index].minimum, self.axes[index].maximum);
                }
            }
            (0, 0) => {
                let mut frame = self.tick_frame(now);
                self.analog = self.pending_analog;
                self.source_analog = self.pending_analog;
                self.raw_pressed = self.pending_pressed;
                self.update_receiver_paddles();
                if self.receiver_gyro {
                    self.held = self.receiver_clutch_held();
                    if self.held {
                        self.gyro_right = [0.0; 2];
                        self.gyro_fraction = [0.0; 2];
                    }
                    self.physical_right.copy_from_slice(&self.analog[2..4]);
                }
                self.buttons(&mut frame, now.max(self.last_tick));
                for index in 0..8 {
                    if !self.receiver_gyro || !(2..4).contains(&index) {
                        self.axis(&mut frame, index, self.analog[index]);
                    }
                }
                if self.receiver_gyro { self.right(&mut frame); }
                frame.sync();
                return Ok(frame);
            }
            _ => (),
        }
        Ok(Frame::default())
    }

    /// Firmware mouse REL counts, committed on SYN_REPORT; wheel/buttons are dropped.
    pub fn receiver_mouse_event(&mut self, event: Event, now: Duration) -> Result<Frame> {
        self.receiver_mouse_event_timed(event, now, now)
    }

    fn receiver_mouse_event_timed(&mut self, event: Event, now: Duration, sample_time: Duration) -> Result<Frame> {
        if self.closed || !self.receiver_gyro {
            return Err(invalid("receiver gyro mouse is disabled or mapper is closed"));
        }
        match (event.kind, event.code) {
            (0, 3) => return Err(invalid("receiver mouse queue overran (SYN_DROPPED)")),
            (2, code @ 0..=1) => {
                let value = &mut self.receiver_pending[usize::from(code)];
                *value = value.saturating_add(event.value);
            }
            (0, 0) => {
                let mut frame = self.tick_frame(now);
                let now = now.max(self.last_tick);
                let delta = std::mem::take(&mut self.receiver_pending);
                let gap = self.receiver_last_sample.map(|at| sample_time.saturating_sub(at).as_secs_f64());
                let elapsed = gap.filter(|&dt| dt > 0.0001 && dt <= 0.250)
                    .unwrap_or(self.receiver_interval);
                self.receiver_last_sample = Some(sample_time);
                self.receiver_interval = elapsed.clamp(0.001, 0.020);
                self.receiver_motion_at = Some(now);
                self.gyro_sample_at = Some(now);
                let velocity = delta.map(|v| f64::from(v) / (elapsed * RECEIVER_COUNTS_PER_DEGREE));
                self.process_gyro([-velocity[1], velocity[0], 0.0], elapsed);
                if self.held {
                    self.gyro_right = [0.0; 2];
                    self.gyro_velocity = [0.0; 2];
                    self.gyro_filter = GyroFilter::default();
                }
                if self.gyro_mouse.is_some() { self.gyro_pixels(&mut frame, elapsed); }
                else { self.right(&mut frame); }
                frame.sync();
                return Ok(frame);
            }
            _ => (),
        }
        Ok(Frame::default())
    }

    /// A radio stall removes only gyro, retaining physical sticks and held buttons.
    pub fn pause_motion(&mut self) -> Frame {
        let mut frame = Frame::default();
        if self.closed {
            return frame;
        }
        self.gyro_right = [0.0; 2];
        self.gyro_velocity = [0.0; 2];
        self.gyro_fraction = [0.0; 2];
        self.angular = None;
        self.gyro_filter = GyroFilter::default();
        self.right(&mut frame);
        frame.sync();
        frame
    }

    /// Idempotent neutralization. A closed mapper cannot latch a new input.
    pub fn close(&mut self) -> Frame {
        let mut frame = Frame::default();
        if self.closed {
            return frame;
        }
        self.closed = true;
        for &code in &self.down_codes[..self.down_count] {
            frame.push(1, code, 0);
        }
        self.down_count = 0;
        for index in 0..8 {
            self.axis(&mut frame, index, neutral(index, self.axes[index]));
        }
        if frame.is_empty() {
            frame.push(0, 0, 0);
        } else {
            frame.sync();
        }
        frame
    }
}

fn neutral(index: usize, axis: AxisInfo) -> i32 {
    match index {
        0 | 1 => (axis.minimum + axis.maximum) / 2,
        4 | 5 => axis.minimum,
        _ => 0,
    }
}

fn stick_value(value: u8, info: AxisInfo) -> i32 {
    let value = i32::from(value);
    let center = (info.minimum + info.maximum) / 2;
    let lower = center - info.flat;
    let upper = center + info.flat;
    if value < lower {
        (f64::from(value - lower) * 32768.0 / f64::from(lower - info.minimum)).round_ties_even()
            as i32
    } else if value > upper {
        (f64::from(value - upper) * 32767.0 / f64::from(info.maximum - upper)).round_ties_even()
            as i32
    } else {
        0
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceInfo {
    pub path: PathBuf,
    pub name: String,
    pub bus: u16,
    pub vendor: u16,
    pub product: u16,
    pub physical: String,
    pub hidraw: Option<PathBuf>,
    pub identity: String,
    pub input_access: bool,
    pub hidraw_access: bool,
    pub link: LinkKind,
}

#[cfg(target_os = "linux")]
fn text(path: impl AsRef<Path>) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}

#[cfg(target_os = "linux")]
fn sysfs_id(path: &Path, field: &str) -> Result<u16> {
    Ok(u16::from_str_radix(
        &text(path.join("id").join(field))?,
        16,
    )?)
}

#[cfg(target_os = "linux")]
fn raw_path(metadata: &Path) -> Result<PathBuf> {
    let resolved = fs::canonicalize(metadata)?;
    let root = resolved
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| invalid("cannot locate controller HID sysfs parent"))?;
    let mut result = None;
    for entry in fs::read_dir(root.join("hidraw"))? {
        let entry = entry?;
        if entry.file_name().as_bytes().starts_with(b"hidraw") {
            if result.is_some() {
                return Err(invalid("controller has multiple raw HID interfaces"));
            }
            result = Some(Path::new("/dev").join(entry.file_name()));
        }
    }
    result.ok_or_else(|| invalid("controller has no raw HID interface"))
}

#[cfg(target_os = "linux")]
fn accessible(path: &Path, mode: libc::c_int) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: path is NUL-terminated and remains alive during access.
    unsafe { libc::access(path.as_ptr(), mode) == 0 }
}

#[cfg(target_os = "linux")]
fn inventory_device(event: &Path) -> Result<Option<DeviceInfo>> {
    let metadata = event.join("device");
    let bus = sysfs_id(&metadata, "bustype")?;
    let vendor = sysfs_id(&metadata, "vendor")?;
    let product = sysfs_id(&metadata, "product")?;
    let physical = text(metadata.join("phys"))?;
    let name = text(metadata.join("name"))?;
    let Some(link) = identify_link(&metadata, bus, vendor, product, &name, &physical)? else {
        return Ok(None);
    };
    let unique = text(metadata.join("uniq"))?;
    let identity = if unique.is_empty() {
        physical.clone()
    } else {
        unique
    };
    if identity.is_empty() {
        return Err(invalid("controller has no stable identity"));
    }
    let path = Path::new("/dev/input").join(
        event
            .file_name()
            .ok_or_else(|| invalid("invalid input event path"))?,
    );
    let hidraw = if link == LinkKind::Bluetooth { raw_path(&metadata).ok() } else { None };
    let input_access = accessible(&path, libc::R_OK);
    let hidraw_access = hidraw
        .as_ref()
        .is_some_and(|path| accessible(path, libc::R_OK));
    Ok(Some(DeviceInfo {
        path,
        name,
        bus,
        vendor,
        product,
        physical,
        hidraw,
        identity,
        input_access,
        hidraw_access,
        link,
    }))
}

#[cfg(target_os = "linux")]
fn identify_link(
    metadata: &Path, bus: u16, vendor: u16, product: u16, name: &str, physical: &str,
) -> Result<Option<LinkKind>> {
    let resolved = fs::canonicalize(metadata)?;
    // Our own uinput outputs live directly under /sys/devices/virtual/input. BlueZ HID gamepads
    // are uhid devices under /sys/devices/virtual/misc/uhid/…/input and must stay visible (live).
    let uinput = resolved.parent().is_some_and(|parent| {
        parent.file_name().is_some_and(|name| name == "input")
            && parent.parent().and_then(Path::file_name).is_some_and(|name| name == "virtual")
            && parent.parent().and_then(Path::parent).and_then(Path::file_name).is_some_and(|name| name == "devices")
    });
    if physical.starts_with(VIRTUAL_PHYS) || uinput {
        return Ok(None);
    }
    Ok(match (bus, vendor, product) {
        (BUS_BLUETOOTH, VENDOR, PRODUCT) => Some(LinkKind::Bluetooth),
        (3, VENDOR, 0x3024) if physical.ends_with("/input0") => Some(LinkKind::Receiver),
        (BUS_VIRTUAL, _, _) if name == "Microsoft Xbox Controller" && physical.starts_with("gip")
            && wired_usb_parent(&resolved)? => Some(LinkKind::Wired),
        _ => None,
    })
}

#[cfg(target_os = "linux")]
fn usb_parent(resolved: &Path) -> Result<Option<PathBuf>> {
    for parent in resolved.ancestors() {
        match text(parent.join("idVendor")) {
            Ok(_) => return Ok(Some(parent.to_owned())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

#[cfg(target_os = "linux")]
fn wired_usb_parent(resolved: &Path) -> Result<bool> {
    let Some(parent) = usb_parent(resolved)? else { return Ok(false); };
    // The nearest USB device owns this node; never match an upstream hub.
    Ok(u16::from_str_radix(&text(parent.join("idVendor"))?, 16)? == VENDOR
        && u16::from_str_radix(&text(parent.join("idProduct"))?, 16)? == PRODUCT)
}

fn bitmap_has(bitmap: &str, bit: u16) -> Result<bool> {
    // sysfs prints native unsigned-long words, most significant first.
    let index = usize::from(bit) / usize::BITS as usize;
    let Some(word) = bitmap.split_whitespace().rev().nth(index) else { return Ok(false); };
    Ok(usize::from_str_radix(word, 16)? & (1usize << (u32::from(bit) % usize::BITS)) != 0)
}

#[cfg(target_os = "linux")]
/// Opt-in selection is sysfs-only. Disabled profiles do not even inspect the mouse.
fn receiver_mouse_path(profile: &HostProfile, device: &DeviceInfo, input_class: &Path) -> Result<Option<PathBuf>> {
    if device.link != LinkKind::Receiver || !profile.receiver_gyro { return Ok(None); }
    let event = device.path.file_name().ok_or_else(|| invalid("invalid input event path"))?;
    let resolved = fs::canonicalize(input_class.join(event).join("device"))?;
    let Some(parent) = usb_parent(&resolved)? else { return Ok(None); };
    let mut selected = None;
    for entry in fs::read_dir(input_class)? {
        let entry = entry?;
        if !entry.file_name().as_bytes().starts_with(b"event") { continue; }
        let metadata = entry.path().join("device");
        let candidate = (|| -> Result<bool> {
            let resolved = fs::canonicalize(&metadata)?;
            Ok(usb_parent(&resolved)?.as_ref() == Some(&parent)
                && sysfs_id(&metadata, "bustype")? == 3
                && sysfs_id(&metadata, "vendor")? == VENDOR
                && sysfs_id(&metadata, "product")? == 0x3024
                && text(metadata.join("phys"))?.ends_with("/input4")
                && bitmap_has(&text(metadata.join("capabilities/ev"))?, 2)?
                && bitmap_has(&text(metadata.join("capabilities/rel"))?, 0)?
                && bitmap_has(&text(metadata.join("capabilities/rel"))?, 1)?)
        })();
        match candidate {
            Ok(true) => {
                if selected.is_some() { return Err(invalid("receiver has multiple matching mouse nodes")); }
                selected = Some(Path::new("/dev/input").join(entry.file_name()));
            }
            Ok(false) => (),
            Err(error) if error.downcast_ref::<io::Error>().is_some_and(|error| error.kind() == io::ErrorKind::NotFound) => (),
            Err(error) => return Err(error),
        }
    }
    Ok(selected)
}

#[cfg(target_os = "linux")]
/// Passive inventory: reads sysfs only; never opens or grabs an input device.
pub fn devices() -> Result<Vec<DeviceInfo>> {
    let mut devices = Vec::new();
    for entry in fs::read_dir("/sys/class/input")? {
        let entry = entry?;
        if !entry.file_name().as_bytes().starts_with(b"event") {
            continue;
        }
        match inventory_device(&entry.path()) {
            Ok(Some(device)) => devices.push(device),
            Ok(None) => (),
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
            {
                ()
            }
            Err(error) => return Err(error),
        }
    }
    devices.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(devices)
}
#[cfg(target_os = "linux")]
/// Linux evdev grip rumble. Inventory is passive; only `Effect::play` opens a node.
/// FF_RUMBLE addresses the strong/left and weak/right grip motors, not triggers.
pub mod rumble {
    use super::*;

    const FF_RUMBLE: u16 = 0x50;
    const EV_FF: u16 = 0x15;
    const EVIOCSFF: libc::c_ulong = ioctl_code(1, b'E', 0x80, size_of::<FfEffect>());
    const EVIOCRMFF: libc::c_ulong = ioctl_code(1, b'E', 0x81, size_of::<libc::c_int>());

    // Linux UAPI include/uapi/linux/input.h, x86_64:
    // ff_periodic_effect makes ff_effect.u 32 bytes, aligned to eight.
    // Explicit padding keeps every byte passed to the kernel initialized.
    #[repr(C)]
    struct FfTrigger {
        button: u16,
        interval: u16,
    }

    #[repr(C)]
    struct FfReplay {
        length: u16,
        delay: u16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FfRumble {
        strong: u16,
        weak: u16,
    }

    #[repr(C)]
    union FfParameters {
        rumble: FfRumble,
        storage: [u64; 4],
    }

    #[repr(C)]
    struct FfEffect {
        kind: u16,
        id: i16,
        direction: u16,
        trigger: FfTrigger,
        replay: FfReplay,
        padding: u16,
        parameters: FfParameters,
    }

    impl FfEffect {
        fn new(strong: u16, weak: u16, duration: Duration) -> Result<Self> {
            if duration.is_zero() || duration > Duration::from_secs(1) {
                return Err(invalid("rumble duration must be greater than zero and at most one second"));
            }
            let mut parameters = FfParameters { storage: [0; 4] };
            parameters.rumble = FfRumble { strong, weak };
            Ok(Self {
                kind: FF_RUMBLE,
                id: -1,
                direction: 0,
                trigger: FfTrigger { button: 0, interval: 0 },
                replay: FfReplay {
                    // Round up: replay.length == 0 means unbounded playback.
                    length: duration.as_nanos().div_ceil(1_000_000) as u16,
                    delay: 0,
                },
                padding: 0,
                parameters,
            })
        }
    }



    fn inventory(event: &Path) -> Result<Option<PathBuf>> {
        let metadata = event.join("device");
        if identify_link(
            &metadata,
            sysfs_id(&metadata, "bustype")?,
            sysfs_id(&metadata, "vendor")?,
            sysfs_id(&metadata, "product")?,
            &text(metadata.join("name"))?,
            &text(metadata.join("phys"))?,
        )?.is_none() || !bitmap_has(&text(metadata.join("capabilities/ff"))?, FF_RUMBLE)? {
            return Ok(None);
        }
        Ok(Some(Path::new("/dev/input").join(
            event.file_name().ok_or_else(|| invalid("invalid input event path"))?,
        )))
    }

    /// List supported physical controllers advertising FF_RUMBLE in sysfs.
    /// Uses the same verified transport identity as the host mapper.
    /// Never opens `/dev/input`, uploads effects, grabs devices or tests access.
    pub fn devices() -> Result<Vec<PathBuf>> {
        let mut devices = Vec::new();
        for entry in fs::read_dir("/sys/class/input")? {
            let entry = entry?;
            if !entry.file_name().as_bytes().starts_with(b"event") {
                continue;
            }
            match inventory(&entry.path()) {
                Ok(Some(path)) => devices.push(path),
                Ok(None) => (),
                Err(error)
                    if error.downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) => (),
                Err(error) => return Err(error),
            }
        }
        devices.sort();
        Ok(devices)
    }

    /// An uploaded, playing effect. Retain until the UI deadline; Drop stops
    /// and removes it. The kernel also bounds playback to the requested length.
    pub struct Effect {
        file: File,
        id: i16,
    }

    impl Effect {
        /// Play grip rumble, magnitudes 0..=65535, for (0, 1 second].
        /// Requires read/write evdev access. Sub-millisecond lengths round up.
        /// No EVIOCGRAB; other input consumers remain unaffected.
        pub fn play(path: &Path, strong: u16, weak: u16, duration: Duration) -> Result<Self> {
            let mut request = FfEffect::new(strong, weak, duration)?;
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
                .open(path)?;
            if !file.metadata()?.file_type().is_char_device() {
                return Err(invalid("rumble target is not a character device"));
            }
            let mut effect = Self { file, id: -1 };
            // EVIOCSFF is encoded _IOW but writes the assigned id back.
            // SAFETY (inside ioctl_read): request matches the complete x86_64
            // ff_effect layout, including the full union and its alignment.
            let uploaded = ioctl_read(effect.file.as_raw_fd(), EVIOCSFF, &mut request);
            effect.id = request.id;
            uploaded?;
            if effect.id < 0 {
                return Err(invalid("kernel returned an invalid force-feedback effect id"));
            }
            // The guard already owns the id: failed playback still stops/removes.
            effect.write_playback(1)?;
            Ok(effect)
        }

        fn write_playback(&mut self, value: i32) -> Result<()> {
            let event = RawEvent { kind: EV_FF, code: self.id as u16, value, ..EMPTY_RAW_EVENT };
            // SAFETY: repr(C) RawEvent is Linux input_event on x86_64, with
            // every field initialized and no padding; bytes live through write.
            let bytes = unsafe {
                std::slice::from_raw_parts((&event as *const RawEvent).cast::<u8>(), size_of::<RawEvent>())
            };
            self.file.write_all(bytes)?;
            Ok(())
        }
    }

    impl Drop for Effect {
        fn drop(&mut self) {
            if self.id >= 0 {
                let _ = self.write_playback(0);
                // EVIOCRMFF takes the scalar id, not a pointer. Always attempt
                // removal even when stop fails (for example after disconnect).
                let _ = ioctl_value(self.file.as_raw_fd(), EVIOCRMFF, i32::from(self.id));
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        #[cfg(target_arch = "x86_64")]
        fn effect_and_playback_match_linux_x86_64_uapi() {
            use std::mem::{align_of, offset_of};
            assert_eq!(size_of::<FfTrigger>(), 4);
            assert_eq!(size_of::<FfReplay>(), 4);
            assert_eq!(size_of::<FfRumble>(), 4);
            assert_eq!(size_of::<FfParameters>(), 32);
            assert_eq!(align_of::<FfParameters>(), 8);
            assert_eq!(size_of::<FfEffect>(), 48);
            assert_eq!(align_of::<FfEffect>(), 8);
            assert_eq!(offset_of!(FfEffect, kind), 0);
            assert_eq!(offset_of!(FfEffect, id), 2);
            assert_eq!(offset_of!(FfEffect, direction), 4);
            assert_eq!(offset_of!(FfEffect, trigger), 6);
            assert_eq!(offset_of!(FfEffect, replay), 10);
            assert_eq!(offset_of!(FfEffect, parameters), 16);
            assert_eq!(offset_of!(FfRumble, weak), 2);
            assert_eq!(EVIOCSFF, 0x40304580);
            assert_eq!(EVIOCRMFF, 0x40044581);
            assert_eq!(size_of::<RawEvent>(), 24);
            assert_eq!(offset_of!(RawEvent, kind), 16);
            assert_eq!(offset_of!(RawEvent, code), 18);
            assert_eq!(offset_of!(RawEvent, value), 20);

            let request = FfEffect::new(0x1234, 0xabcd, Duration::from_millis(999)).unwrap();
            assert_eq!(request.kind, 0x50);
            assert_eq!(request.id, -1);
            assert_eq!(request.replay.length, 999);
            assert_eq!(request.trigger.button, 0);
            assert_eq!(request.trigger.interval, 0);
            assert_eq!(request.replay.delay, 0);
            // SAFETY: new initialized the entire union before setting rumble.
            let storage = unsafe { request.parameters.storage };
            assert_eq!(storage, [0xabcd1234, 0, 0, 0]);
        }

        #[test]
        fn durations_are_positive_and_bounded_before_device_access() {
            assert!(FfEffect::new(0, 0, Duration::ZERO).is_err());
            assert!(FfEffect::new(0, 0, Duration::from_secs(1) + Duration::from_nanos(1)).is_err());
            assert_eq!(FfEffect::new(0, 0, Duration::from_nanos(1)).unwrap().replay.length, 1);
            assert_eq!(FfEffect::new(u16::MAX, u16::MAX, Duration::from_secs(1)).unwrap().replay.length, 1000);
        }

        #[test]
        #[cfg(target_arch = "x86_64")]
        fn sysfs_bitmaps_use_high_word_first_and_require_rumble() {
            assert!(bitmap_has("107030000 0\n", FF_RUMBLE).unwrap());
            assert!(bitmap_has("10000 0", FF_RUMBLE).unwrap());
            assert!(!bitmap_has("20000 0", FF_RUMBLE).unwrap());
            assert!(!bitmap_has("10000", FF_RUMBLE).unwrap());
            assert!(!bitmap_has("0", FF_RUMBLE).unwrap());
            assert!(!bitmap_has("", FF_RUMBLE).unwrap());
            assert!(bitmap_has("invalid 0", FF_RUMBLE).is_err());
        }

        #[test]
        fn rumble_and_mapper_share_verified_wired_identity() {
            let fixture = crate::input::tests::Sysfs::new(false);
            assert_eq!(inventory(&fixture.event).unwrap(), Some(PathBuf::from("/dev/input/event7")));
            fs::write(fixture.metadata.join("capabilities/ff"), "0").unwrap();
            assert!(inventory(&fixture.event).unwrap().is_none());
            fs::write(fixture.metadata.join("capabilities/ff"), "107030000 0").unwrap();
            fs::write(fixture.usb.join("idVendor"), "045e").unwrap();
            assert!(inventory(&fixture.event).unwrap().is_none());
            let virtual_node = crate::input::tests::Sysfs::new(true);
            assert!(inventory(&virtual_node.event).unwrap().is_none());
        }

    }
}


#[cfg(target_os = "linux")]
// Linux asm-generic ioctl encoding, valid for the supported x86_64 Linux runtime.
const fn ioctl_code(direction: u32, group: u8, command: u8, size: usize) -> libc::c_ulong {
    ((direction << 30) | ((size as u32) << 16) | ((group as u32) << 8) | command as u32)
        as libc::c_ulong
}

#[cfg(target_os = "linux")]
fn ioctl_read<T>(fd: RawFd, request: libc::c_ulong, output: &mut T) -> Result<()> {
    // SAFETY: every call uses the kernel's exact fixed struct/array size.
    if unsafe { libc::ioctl(fd, request, output as *mut T) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn ioctl_value(fd: RawFd, request: libc::c_ulong, value: libc::c_int) -> Result<()> {
    // SAFETY: these requests take scalar integer arguments, not pointers.
    if unsafe { libc::ioctl(fd, request, value) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
/// The kernel answers ENOENT for strings a device never set (xpad/xone pads have
/// no `uniq`); that means "absent", not an error.
fn ioctl_string(fd: RawFd, group: u8, command: u8) -> Result<String> {
    let mut bytes = [0u8; 256];
    // SAFETY: bytes is a writable buffer of exactly the length encoded in the request.
    if unsafe { libc::ioctl(fd, ioctl_code(2, group, command, bytes.len()), bytes.as_mut_ptr()) } < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(String::new());
        }
        return Err(error.into());
    }
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    Ok(std::str::from_utf8(&bytes[..end])?.trim().to_owned())
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputId {
    bus: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Default)]
struct HidRawInfo {
    bus: u32,
    vendor: i16,
    product: i16,
}

fn has_bit(bits: &[u8], code: u16) -> bool {
    bits.get(usize::from(code) / 8)
        .is_some_and(|byte| byte & (1 << (code % 8)) != 0)
}

#[cfg(target_os = "linux")]
struct Source {
    file: File,
    grabbed: bool,
}

#[cfg(target_os = "linux")]
impl Source {
    fn open_receiver_mouse(path: &Path, profile: &HostProfile, device: &DeviceInfo) -> Result<Self> {
        let file = OpenOptions::new().read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW).open(path)?;
        if !file.metadata()?.file_type().is_char_device() {
            return Err(invalid("receiver mouse source is not a character device"));
        }
        if receiver_mouse_path(profile, device, Path::new("/sys/class/input"))?.as_deref() != Some(path) {
            return Err(invalid("receiver mouse changed before startup"));
        }
        let fd = file.as_raw_fd();
        let mut id = InputId::default();
        ioctl_read(fd, ioctl_code(2, b'E', 0x02, size_of::<InputId>()), &mut id)?;
        let physical = ioctl_string(fd, b'E', 0x07)?;
        let mut rel = [0u8; 4];
        ioctl_read(fd, ioctl_code(2, b'E', 0x22, rel.len()), &mut rel)?;
        if (id.bus, id.vendor, id.product) != (3, VENDOR, 0x3024)
            || !physical.ends_with("/input4") || !has_bit(&rel, 0) || !has_bit(&rel, 1) {
            return Err(invalid("receiver mouse is missing expected identity/relative axes"));
        }
        Ok(Self { file, grabbed: false })
    }

    fn open(device: &DeviceInfo) -> Result<(Self, InputId, [AxisInfo; 8], [u8; 4])> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&device.path)?;
        if !file.metadata()?.file_type().is_char_device() {
            return Err(invalid("source is not a character device"));
        }
        let fd = file.as_raw_fd();
        let mut id = InputId::default();
        ioctl_read(fd, ioctl_code(2, b'E', 0x02, size_of::<InputId>()), &mut id)?;
        let physical = ioctl_string(fd, b'E', 0x07)?;
        let name = ioctl_string(fd, b'E', 0x06)?;
        let unique = ioctl_string(fd, b'E', 0x08)?;
        let identity = if unique.is_empty() {
            &physical
        } else {
            &unique
        };
        let metadata = Path::new("/sys/class/input")
            .join(device.path.file_name().ok_or_else(|| invalid("invalid input event path"))?)
            .join("device");
        if identify_link(&metadata, id.bus, id.vendor, id.product, &name, &physical)? != Some(device.link)
            || (id.bus, id.vendor, id.product) != (device.bus, device.vendor, device.product)
            || identity != &device.identity
            || physical != device.physical
        {
            return Err(invalid(
                "selected controller changed before startup",
            ));
        }
        let mut event_bits = [0u8; 4];
        ioctl_read(
            fd,
            ioctl_code(2, b'E', 0x20, event_bits.len()),
            &mut event_bits,
        )?;
        let mut keys = [0u8; 96];
        ioctl_read(fd, ioctl_code(2, b'E', 0x21, keys.len()), &mut keys)?;
        if !has_bit(&event_bits, 1)
            || !has_bit(&event_bits, 3)
            || KEY_CODES.iter().filter(|&&key| device.link == LinkKind::Bluetooth
                || RECEIVER_EVDEV_KEYS.contains(&key))
                .any(|&key| !has_bit(&keys, key))
        {
            return Err(invalid(
                "controller is missing expected buttons/events",
            ));
        }
        let mut abs_bits = [0u8; 8];
        ioctl_read(fd, ioctl_code(2, b'E', 0x23, abs_bits.len()), &mut abs_bits)?;
        let mut axes = [AxisInfo::default(); 8];
        let axis_codes = if device.link == LinkKind::Bluetooth { &AXES } else { &RECEIVER_AXES };
        for (index, &code) in axis_codes.iter().enumerate() {
            if !has_bit(&abs_bits, code) {
                return Err(invalid("controller is missing expected axes"));
            }
            ioctl_read(
                fd,
                ioctl_code(2, b'E', 0x40 + code as u8, size_of::<AxisInfo>()),
                &mut axes[index],
            )?;
        }
        validate_source_axes(&axes, device.link)?;
        let mut properties = [0u8; 4];
        ioctl_read(
            fd,
            ioctl_code(2, b'E', 0x09, properties.len()),
            &mut properties,
        )?;
        Ok((
            Self {
                file,
                grabbed: false,
            },
            id,
            axes,
            properties,
        ))
    }

    fn grab(&mut self) -> Result<()> {
        ioctl_value(
            self.file.as_raw_fd(),
            ioctl_code(1, b'E', 0x90, size_of::<i32>()),
            1,
        )?;
        self.grabbed = true;
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        if self.grabbed {
            ioctl_value(
                self.file.as_raw_fd(),
                ioctl_code(1, b'E', 0x90, size_of::<i32>()),
                0,
            )?;
            self.grabbed = false;
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for Source {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct UInputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}
#[cfg(target_os = "linux")]
#[repr(C)]
struct UInputAbsSetup {
    code: u16,
    padding: u16,
    info: AxisInfo,
}
#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy)]
struct RawEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}
#[cfg(target_os = "linux")]
const EMPTY_RAW_EVENT: RawEvent = RawEvent {
    time: libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    },
    kind: 0,
    code: 0,
    value: 0,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputKind {
    Gamepad,
    Keyboard,
    Mouse,
}

impl OutputKind {
    fn accepts(self, event: Event) -> bool {
        match self {
            Self::Gamepad => event.kind == 3 || (event.kind == 1 && event.code >= 304),
            Self::Keyboard => event.kind == 1 && event.code < 256,
            Self::Mouse => event.kind == 2 || (event.kind == 1 && (272..280).contains(&event.code)),
        }
    }
}

#[cfg(target_os = "linux")]
struct VirtualDevice {
    file: File,
    kind: OutputKind,
}

#[cfg(target_os = "linux")]
impl VirtualDevice {
    fn create(
        source: &DeviceInfo,
        id: InputId,
        axes: &[AxisInfo; 8],
        properties: &[u8; 4],
        kind: OutputKind,
        gamepad_keys: &[u16],
    ) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        ioctl_value(fd, ioctl_code(1, b'U', 100, size_of::<i32>()), 1)?; // UI_SET_EVBIT EV_KEY
        let (suffix, name) = match kind {
            OutputKind::Gamepad => {
                ioctl_value(fd, ioctl_code(1, b'U', 100, size_of::<i32>()), 3)?; // EV_ABS
                for &code in gamepad_keys {
                    ioctl_value(
                        fd,
                        ioctl_code(1, b'U', 101, size_of::<i32>()),
                        i32::from(code),
                    )?;
                }
                for code in 0..32 {
                    if has_bit(properties, code) {
                        ioctl_value(
                            fd,
                            ioctl_code(1, b'U', 110, size_of::<i32>()),
                            i32::from(code),
                        )?;
                    }
                }
                ("gamepad", "8BitDo Ultimate 3E gyro clutch")
            }
            OutputKind::Keyboard => {
                // A separate keyboard, rather than keys on an ignored joystick,
                // gives udev/libinput an actual keyboard-class input device.
                for &(_, code) in DESTINATION_KEYS.iter().filter(|(_, code)| *code < 256) {
                    ioctl_value(
                        fd,
                        ioctl_code(1, b'U', 101, size_of::<i32>()),
                        i32::from(code),
                    )?;
                }
                ("keyboard", "8BitDo Ultimate 3E mapped keyboard")
            }
            OutputKind::Mouse => {
                ioctl_value(fd, ioctl_code(1, b'U', 100, size_of::<i32>()), 2)?; // EV_REL
                for code in [0, 1] {
                    // REL_X/Y advertise pointer class; no synthetic motion.
                    ioctl_value(fd, ioctl_code(1, b'U', 102, size_of::<i32>()), code)?;
                }
                for code in 272..280 {
                    ioctl_value(fd, ioctl_code(1, b'U', 101, size_of::<i32>()), code)?;
                }
                ioctl_value(fd, ioctl_code(1, b'U', 110, size_of::<i32>()), 0)?; // INPUT_PROP_POINTER
                ("mouse", "8BitDo Ultimate 3E mapped mouse")
            }
        };
        let physical = CString::new(format!("{VIRTUAL_PHYS}{}/{suffix}", source.path.display()))?;
        // SAFETY: UI_SET_PHYS consumes a valid NUL-terminated string during ioctl.
        if unsafe {
            libc::ioctl(
                fd,
                ioctl_code(1, b'U', 108, size_of::<*const libc::c_char>()),
                physical.as_ptr(),
            )
        } < 0
        {
            return Err(io::Error::last_os_error().into());
        }
        let mut setup = UInputSetup {
            id,
            name: [0; 80],
            ff_effects_max: 0,
        };
        setup.name[..name.len()].copy_from_slice(name.as_bytes());
        ioctl_read(
            fd,
            ioctl_code(1, b'U', 3, size_of::<UInputSetup>()),
            &mut setup,
        )?;
        if kind == OutputKind::Gamepad {
            let codes = if source.link == LinkKind::Bluetooth { &AXES } else { &RECEIVER_AXES };
            for (index, &code) in codes.iter().enumerate() {
                ioctl_value(
                    fd,
                    ioctl_code(1, b'U', 103, size_of::<i32>()),
                    i32::from(code),
                )?;
                let mut info = axes[index];
                info.value = neutral(index, info);
                info.fuzz = 0;
                if index == 2 || index == 3 {
                    info.minimum = -32768;
                    info.maximum = 32767;
                    info.flat = 0;
                    info.resolution = 0;
                }
                let mut axis = UInputAbsSetup {
                    code,
                    padding: 0,
                    info,
                };
                ioctl_read(
                    fd,
                    ioctl_code(1, b'U', 4, size_of::<UInputAbsSetup>()),
                    &mut axis,
                )?;
            }
        }
        ioctl_value(fd, ioctl_code(0, b'U', 1, 0), 0)?; // UI_DEV_CREATE
        Ok(Self { file, kind })
    }

    fn send(&mut self, frame: &Frame) -> Result<()> {
        if frame.is_empty() {
            return Ok(());
        }
        let mut events = [EMPTY_RAW_EVENT; MAX_EVENTS];
        let mut count = 0;
        for &event in frame.as_slice() {
            if self.kind.accepts(event) {
                events[count].kind = event.kind;
                events[count].code = event.code;
                events[count].value = event.value;
                count += 1;
            }
        }
        if count == 0 {
            return Ok(());
        }
        count += 1; // The next zero-initialized event is SYN_REPORT.
        // SAFETY: initialized repr(C) input_event array; on Linux its fields have
        // no uninitialized padding, and the byte view does not outlive events.
        let bytes = unsafe {
            std::slice::from_raw_parts(events.as_ptr().cast::<u8>(), count * size_of::<RawEvent>())
        };
        self.file.write_all(bytes)?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for VirtualDevice {
    fn drop(&mut self) {
        let _ = ioctl_value(self.file.as_raw_fd(), ioctl_code(0, b'U', 2, 0), 0);
    }
}

#[cfg(target_os = "linux")]
struct Outputs {
    gamepad: Option<VirtualDevice>,
    keyboard: Option<VirtualDevice>,
    mouse: Option<VirtualDevice>,
    /// Steam mode: a virtual DualSense Edge replaces every uinput device; Steam Input
    /// does remapping and gyro, so mapper frames are not emitted.
    steam: Option<crate::steam::Edge>,
}

#[cfg(target_os = "linux")]
impl Outputs {
    fn create(
        source: &DeviceInfo,
        id: InputId,
        axes: &[AxisInfo; 8],
        properties: &[u8; 4],
        mapper: &Mapper,
        steam: bool,
    ) -> Result<Self> {
        if steam {
            let edge = crate::steam::Edge::create().map_err(|error| invalid(format!(
                "Steam mode needs write access to /dev/uhid (udev: KERNEL==\"uhid\", TAG+=\"uaccess\"): {error}")))?;
            return Ok(Self { gamepad: None, keyboard: None, mouse: None, steam: Some(edge) });
        }
        let gamepad = VirtualDevice::create(source, id, axes, properties, OutputKind::Gamepad, &mapper.gamepad_keys)?;
        let keyboard = if mapper.modes.iter().any(|mode| mode.destination_codes[..mode.destination_count].iter().any(|&code| code < 256)) {
            Some(VirtualDevice::create(
                source,
                id,
                axes,
                properties,
                OutputKind::Keyboard, &[],
            )?)
        } else {
            None
        };
        let mouse = if mapper.gyro_mouse.is_some() || mapper.modes.iter().any(|mode| mode.mouse.is_some() || mode.destination_codes[..mode.destination_count].iter().any(|code| (272..280).contains(code))) {
            Some(VirtualDevice::create(
                source,
                id,
                axes,
                properties,
                OutputKind::Mouse, &[],
            )?)
        } else {
            None
        };
        Ok(Self {
            gamepad: Some(gamepad),
            keyboard,
            mouse,
            steam: None,
        })
    }

    fn send(&mut self, frame: &Frame) -> Result<()> {
        // Try every device even if one fails, especially when releasing keys.
        let gamepad = self.gamepad.as_mut().map(|device| device.send(frame)).transpose();
        let keyboard = self
            .keyboard
            .as_mut()
            .map(|device| device.send(frame))
            .transpose();
        let mouse = self
            .mouse
            .as_mut()
            .map(|device| device.send(frame))
            .transpose();
        gamepad?;
        keyboard?;
        mouse?;
        Ok(())
    }

    /// Publish the latest post-slot-map source state; the Edge thread emits it at 250 Hz.
    fn sync(&self, mapper: &Mapper) {
        if let Some(edge) = &self.steam { edge.update(&steam_state(mapper)); }
    }
}

#[cfg(target_os = "linux")]
/// Source buttons (KEY_CODES bits) → DualSense positions; Xbox X/Y sit where Square/Triangle do.
/// Extra buttons: L4/R4 (HAPPY1/2) → Fn left/right, PL/PR (BTN_Z/BTN_C) → back paddles.
const STEAM_BUTTONS: [(usize, u32); 17] = {
    use crate::steam::EdgeState as S;
    [(0, S::CROSS), (1, S::CIRCLE), (3, S::SQUARE), (4, S::TRIANGLE), (6, S::L1), (7, S::R1),
     (8, S::L2), (9, S::R2), (10, S::CREATE), (11, S::OPTIONS), (12, S::PS), (13, S::L3), (14, S::R3),
     (16, S::FN_LEFT), (17, S::FN_RIGHT), (5, S::BACK_LEFT), (2, S::BACK_RIGHT)]
};

#[cfg(target_os = "linux")]
fn steam_state(mapper: &Mapper) -> crate::steam::EdgeState {
    use crate::steam::EdgeState as S;
    let state = mapper.physical_state();
    let byte = |index: usize| {
        let info = &mapper.axes[index];
        let span = i64::from(info.maximum) - i64::from(info.minimum);
        if span <= 0 { return 128; }
        ((i64::from(state.axes[index]) - i64::from(info.minimum)).clamp(0, span) * 255 / span) as u8
    };
    let mut buttons = STEAM_BUTTONS.iter().filter(|(bit, _)| state.buttons & 1 << bit != 0).fold(0, |all, (_, mask)| all | mask);
    // Analog order: Bluetooth HID GAS(RT)/BRAKE(LT); xpad/xone Z(LT)/RZ(RT).
    let (lt, rt) = if mapper.link == LinkKind::Bluetooth { (5, 4) } else { (4, 5) };
    let triggers = [byte(lt), byte(rt)];
    if triggers[0] > 0 { buttons |= S::L2; }
    if triggers[1] > 0 { buttons |= S::R2; }
    for (axis, negative, positive) in [(6, S::DPAD_LEFT, S::DPAD_RIGHT), (7, S::DPAD_UP, S::DPAD_DOWN)] {
        match state.axes[axis].signum() { -1 => buttons |= negative, 1 => buttons |= positive, _ => {} }
    }
    let gyro = mapper.angular_velocity().unwrap_or([0.0; 3]);
    S {
        buttons,
        left_stick: [byte(0), byte(1)],
        right_stick: [byte(2), byte(3)],
        triggers,
        gyro_dps: gyro.map(|value| value as f32),
        // No accelerometer on either link: report the controller held level, at rest.
        accel_g: [0.0, 1.0, 0.0],
    }
}

#[cfg(target_os = "linux")]
struct Session {
    mapper: Mapper,
    output: Outputs,
    source: Source,
    receiver_mouse: Option<Source>,
}
#[cfg(target_os = "linux")]
impl Session {
    fn close(&mut self) -> Result<()> {
        let frame = self.mapper.close();
        let output = self.output.send(&frame);
        let source = self.source.release(); // Release even when neutralization fails.
        let mouse = self.receiver_mouse.as_mut().map(Source::release).transpose();
        output?;
        source?;
        mouse?;
        Ok(())
    }
}
#[cfg(target_os = "linux")]
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(target_os = "linux")]
fn open_raw(device: &DeviceInfo) -> Result<File> {
    let event = device
        .path
        .file_name()
        .ok_or_else(|| invalid("invalid event path"))?;
    let path = raw_path(&Path::new("/sys/class/input").join(event).join("device"))?;
    if device.hidraw.as_ref() != Some(&path) {
        return Err(invalid("raw HID interface changed before startup"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&path)?;
    if !file.metadata()?.file_type().is_char_device() {
        return Err(invalid("raw HID input is not a character device"));
    }
    let mut info = HidRawInfo::default();
    ioctl_read(
        file.as_raw_fd(),
        ioctl_code(2, b'H', 3, size_of::<HidRawInfo>()),
        &mut info,
    )?;
    if (info.bus, info.vendor as u16, info.product as u16)
        != (u32::from(BUS_BLUETOOTH), VENDOR, PRODUCT)
    {
        return Err(invalid(
            "raw HID identity does not match the Bluetooth controller",
        ));
    }
    let unique = ioctl_string(file.as_raw_fd(), b'H', 8)?; // HIDIOCGRAWUNIQ
    if !unique.is_empty() && unique != device.identity {
        return Err(invalid("raw HID belongs to another Bluetooth controller"));
    }
    Ok(file)
}

#[cfg(target_os = "linux")]
fn read_nonblocking(fd: RawFd, bytes: &mut [u8]) -> Result<Option<usize>> {
    loop {
        // SAFETY: bytes is a live writable slice of exactly the supplied length.
        let count = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
        if count > 0 {
            return Ok(Some(count as usize));
        }
        if count == 0 {
            return Err(invalid("Bluetooth controller disconnected"));
        }
        let error = io::Error::last_os_error();
        match error.kind() {
            io::ErrorKind::WouldBlock => return Ok(None),
            io::ErrorKind::Interrupted => continue,
            _ => return Err(error.into()),
        }
    }
}

#[cfg(target_os = "linux")]
fn run_loop(
    session: &mut Session,
    raw: &File,
    physical: &Receiver<PhysicalInput>,
    stopped: &AtomicBool,
) -> Result<()> {
    let mut deadline = Some(Instant::now() + Duration::from_secs(2));
    let mut started = false;
    let mut reports = [0u8; 4096];
    let mut source_events = [0u8; size_of::<RawEvent>() * 64];
    while !stopped.load(Ordering::Acquire) && !crate::interrupted() {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            if !started {
                return Err(invalid(
                    "no Bluetooth input reports received within two seconds",
                ));
            }
            session.output.send(&session.mapper.pause_motion())?;
            deadline = None;
        }
        // BLE is change-driven: disconnect is fatal; an idle live channel is not.
        for _ in 0..256 {
            match physical.try_recv() {
                Ok(state) => session.output.send(&session.mapper.physical_input(state))?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(invalid(
                        "configuration BLE physical-report connection disconnected",
                    ));
                }
            }
        }
        session.output.send(&session.mapper.tick(session.mapper.origin.elapsed()))?;
        session.output.sync(&session.mapper);
        let mut descriptors = [
            libc::pollfd {
                fd: raw.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: session.source.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // A bounded poll also services std::mpsc and stop signals without a
        // helper thread, waking at most five milliseconds after either changes.
        // SAFETY: descriptors contains exactly the advertised number of pollfd.
        let ready = unsafe {
            libc::poll(
                descriptors.as_mut_ptr(),
                descriptors.len() as libc::nfds_t,
                5,
            )
        };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.into());
        }
        if stopped.load(Ordering::Acquire) || crate::interrupted() {
            break;
        }
        if descriptors
            .iter()
            .any(|fd| fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0)
        {
            return Err(invalid("Bluetooth input device disconnected"));
        }
        if descriptors[0].revents & libc::POLLIN != 0 {
            for _ in 0..64 {
                let Some(count) = read_nonblocking(raw.as_raw_fd(), &mut reports)? else {
                    break;
                };
                session
                    .output
                    .send(&session.mapper.input(&reports[..count])?)?;
                started = true;
                deadline = Some(Instant::now() + MOTION_TIMEOUT);
            }
        }
        if descriptors[1].revents & libc::POLLIN != 0 {
            // Consume evdev's queue, but never use its firmware-mixed right stick.
            for _ in 0..8 {
                let Some(count) =
                    read_nonblocking(session.source.file.as_raw_fd(), &mut source_events)?
                else {
                    break;
                };
                if count % size_of::<RawEvent>() != 0 {
                    return Err(invalid("truncated evdev event"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
/// Own evdev/hidraw/uinput until stop or error. Stops for `stopped` or the crate's
/// signal flag; the caller must drop the physical sender on BLE failure.
/// SIGKILL cannot run cleanup, but descriptor closure releases the kernel grab.
/// Never grabs before receiving a valid first BLE state, never writes to hardware.
pub fn run(
    device: Option<&Path>,
    profile: &HostProfile,
    slot_map: Option<crate::profile::KeyMap>,
    physical: Receiver<PhysicalInput>,
    stopped: Arc<AtomicBool>,
) -> Result<()> {
    profile.validate()?;
    let selected = device.map(fs::canonicalize).transpose()?;
    let mut candidates = devices()?
        .into_iter()
        .filter(|candidate| candidate.link == LinkKind::Bluetooth)
        .filter(|candidate| selected.as_ref().is_none_or(|path| path == &candidate.path));
    let device = candidates.next().ok_or_else(|| {
        invalid("connect the Ultimate 3E in Bluetooth mode (2dc8:202f); no matching event device")
    })?;
    if candidates.next().is_some() {
        return Err(invalid(
            "multiple Ultimate 3 Bluetooth controllers; select an event device",
        ));
    }
    if device.hidraw.is_none() {
        return Err(invalid("controller has no unique raw HID interface"));
    }
    let (source, id, axes, properties) = Source::open(&device)?;
    // Configuration BLE reports are change-driven: nothing arrives until the user acts.
    eprintln!("Waiting for the first controller input on the configuration link: press any button.");
    let first = loop {
        if stopped.load(Ordering::Acquire) || crate::interrupted() {
            return Ok(());
        }
        match physical.recv_timeout(Duration::from_millis(20)) {
            Ok(state) => break state,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(invalid(
                    "configuration BLE disconnected before first physical state",
                ));
            }
        }
    };
    // Opening raw only after BLE startup prevents seconds-old queued motion.
    let raw = open_raw(&device)?;
    let mut mapper = Mapper::new(profile, axes)?;
    mapper.set_key_map(slot_map);
    let output = Outputs::create(&device, id, &axes, &properties, &mapper, profile.steam)?;
    let mut session = Session {
        mapper,
        output,
        source,
        receiver_mouse: None,
    };
    session.mapper.origin = Instant::now();
    session.output.send(&session.mapper.physical_input(first))?;
    // Reject an already-disconnected producer before suppressing physical input.
    loop {
        match physical.try_recv() {
            Ok(state) => session.output.send(&session.mapper.physical_input(state))?,
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                return Err(invalid("configuration BLE disconnected during startup"));
            }
        }
    }
    if stopped.load(Ordering::Acquire) || crate::interrupted() {
        return session.close();
    }
    session.source.grab()?;
    let result = run_loop(&mut session, &raw, &physical, &stopped);
    let cleanup = session.close();
    result?;
    cleanup
}

#[cfg(target_os = "linux")]
/// Grab the receiver's xpad node and own uinput outputs until stop/disconnect.
/// Opt-in gyro grabs its sibling firmware mouse; no BLE or raw IMU is used.
pub fn run_receiver(
    device: Option<&Path>,
    profile: &HostProfile,
    slot_map: Option<crate::profile::KeyMap>,
    stopped: Arc<AtomicBool>,
) -> Result<()> {
    run_evdev(device, profile, slot_map, stopped, LinkKind::Receiver)
}

#[cfg(target_os = "linux")]
/// Grab the verified 2dc8:202f xone GIP node; no BLE, IMU or physical paddles.
pub fn run_wired(
    device: Option<&Path>,
    profile: &HostProfile,
    stopped: Arc<AtomicBool>,
) -> Result<()> {
    run_evdev(device, profile, None, stopped, LinkKind::Wired)
}

#[cfg(target_os = "linux")]
fn run_evdev(
    device: Option<&Path>,
    profile: &HostProfile,
    slot_map: Option<crate::profile::KeyMap>,
    stopped: Arc<AtomicBool>,
    link: LinkKind,
) -> Result<()> {
    let mut notice = profile.validate_for_link(link)?;
    let selected = device.map(fs::canonicalize).transpose()?;
    let mut candidates = devices()?.into_iter().filter(|candidate| {
        candidate.link == link
            && selected.as_ref().is_none_or(|path| path == &candidate.path)
    });
    let device = candidates.next().ok_or_else(|| invalid(match link {
        LinkKind::Wired => "no matching wired 2dc8:202f xone GIP event device",
        _ => "no matching 2dc8:3024 receiver xpad event device",
    }))?;
    if candidates.next().is_some() {
        return Err(invalid("multiple controllers on this link; select an event device"));
    }
    let mut physical = if link == LinkKind::Receiver {
        Some(crate::receiver::Receiver::open(&crate::receiver::find()?)?.with_stop(Arc::clone(&stopped)))
    } else { None };
    let slot_map = if slot_map.is_none() {
        if let Some(receiver) = physical.as_mut() {
            match receiver.read_profiles() {
                Ok(bytes) => {
                    let image: &[u8; crate::profile::IMAGE_SIZE] = bytes.as_slice().try_into()
                        .map_err(|_| invalid("receiver returned incomplete profiles"))?;
                    crate::profile::Profiles::decode(image)?.selected_slot().map(|slot| slot.key_map.clone())
                }
                // Live 2026-10-20: a USB-cabled 3E enumerates as the same 2dc8:3024 composite
                // (xpad, firmware gyro mouse, vendor FF00) but never answers configuration or
                // sends heartbeat physical reports; firmware slot remapping stays active there.
                Err(error) if error.downcast_ref::<io::Error>().is_some_and(|e| e.kind() == io::ErrorKind::TimedOut) => {
                    validate_cable_clutch(profile)?;
                    eprintln!("No configuration reply: treating this as a USB-cabled controller. The controller's slot remapping stays active; paddles are visible only as the buttons the slot maps them to.");
                    if notice.is_some() { notice = Some(CABLE_GYRO_NOTICE); }
                    physical = None;
                    None
                }
                Err(error) => return Err(error),
            }
        } else { None }
    } else { slot_map };
    let (source, id, axes, properties) = Source::open(&device)?;
    let receiver_mouse = receiver_mouse_path(profile, &device, Path::new("/sys/class/input"))?
        .map(|path| Source::open_receiver_mouse(&path, profile, &device)).transpose()?;
    let mut mapper = Mapper::new_with_receiver_gyro(profile, axes, link, receiver_mouse.is_some())?;
    mapper.set_key_map(slot_map);
    let output = Outputs::create(&device, id, &axes, &properties, &mapper, profile.steam)?;
    let mut session = Session { mapper, output, source, receiver_mouse };
    if stopped.load(Ordering::Acquire) || crate::interrupted() {
        return session.close();
    }
    if let Some(notice) = notice { eprintln!("{notice}"); }
    session.source.grab()?;
    if let Some(mouse) = session.receiver_mouse.as_mut() { mouse.grab()?; }
    let result = receiver_loop(&mut session, physical.as_mut(), &stopped);
    let cleanup = session.close();
    result?;
    cleanup
}

#[cfg(target_os = "linux")]
fn receiver_loop(session: &mut Session, mut physical: Option<&mut crate::receiver::Receiver>, stopped: &AtomicBool) -> Result<()> {
    let fd = session.source.file.as_raw_fd();
    let mouse_fd = session.receiver_mouse.as_ref().map_or(-1, |mouse| mouse.file.as_raw_fd());
    let mut bytes = [0u8; size_of::<RawEvent>() * 64];
    // Discard events queued while uinput was being created, then snapshot held
    // keys/axes. Replayed stale edges must not accidentally toggle a binding.
    for source_fd in [fd, mouse_fd].into_iter().filter(|&fd| fd >= 0) {
        for _ in 0..64 {
            if read_nonblocking(source_fd, &mut bytes)?.is_none() { break; }
        }
    }
    let mut keys = [0u8; 96];
    ioctl_read(fd, ioctl_code(2, b'E', 0x18, keys.len()), &mut keys)?; // EVIOCGKEY
    for (index, &code) in KEY_CODES.iter().enumerate() {
        if has_bit(&keys, code) && !(session.mapper.link == LinkKind::Receiver && [704, 705, 309, 306].contains(&code)) {
            session.mapper.pending_pressed |= 1 << index;
        }
    }
    for (index, &code) in RECEIVER_AXES.iter().enumerate() {
        let mut axis = AxisInfo::default();
        ioctl_read(fd, ioctl_code(2, b'E', 0x40 + code as u8, size_of::<AxisInfo>()), &mut axis)?;
        session.mapper.pending_analog[index] = axis.value;
    }
    session.mapper.origin = Instant::now();
    let initial = session.mapper.receiver_event(Event::default(), Duration::ZERO)?;
    session.output.send(&initial)?;
    while !stopped.load(Ordering::Acquire) && !crate::interrupted() {
        if let Some(receiver) = physical.as_mut() {
            receiver.heartbeat(true, Instant::now())?;
            if let (_, Some(state)) = receiver.input(Duration::ZERO)? {
                let frame = session.mapper.receiver_physical_at(state, session.mapper.origin.elapsed())?;
                session.output.send(&frame)?;
            }
        }
        let mut descriptors = [
            libc::pollfd { fd, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: mouse_fd, events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: two live pollfds (negative fd is ignored); bounded wait services stop.
        if unsafe { libc::poll(descriptors.as_mut_ptr(), descriptors.len() as libc::nfds_t, 5) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted { continue; }
            return Err(error.into());
        }
        // Gamepad first: apply clutch edges before motion from the same poll.
        for (index, descriptor) in descriptors.iter().enumerate() {
            if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(invalid("evdev input device disconnected"));
            }
            if descriptor.revents & libc::POLLIN == 0 { continue; }
            for _ in 0..8 {
                let Some(count) = read_nonblocking(descriptor.fd, &mut bytes)? else { break; };
                if count % size_of::<RawEvent>() != 0 {
                    return Err(invalid("partial evdev event"));
                }
                for chunk in bytes[..count].chunks_exact(size_of::<RawEvent>()) {
                    // SAFETY: chunk contains one complete repr(C) input_event;
                    // read_unaligned handles the byte buffer's alignment.
                    let raw = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast::<RawEvent>()) };
                    let event = Event { kind: raw.kind, code: raw.code, value: raw.value };
                    let now = session.mapper.origin.elapsed();
                    let frame = if index == 0 { session.mapper.receiver_event(event, now)? }
                        else {
                            // evdev preserves per-report microseconds even when reads batch
                            // several reports. Receipt time would turn the batch into spikes.
                            let sample_time = if raw.time.tv_sec >= 0 && (0..1_000_000).contains(&raw.time.tv_usec) {
                                Duration::new(raw.time.tv_sec as u64, raw.time.tv_usec as u32 * 1000)
                            } else { now };
                            session.mapper.receiver_mouse_event_timed(event, now, sample_time)?
                        };
                    session.output.send(&frame)?;
                }
            }
        }
        let frame = session.mapper.tick(session.mapper.origin.elapsed());
        session.output.send(&frame)?;
        session.output.sync(&session.mapper);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    pub(super) struct Sysfs {
        root: PathBuf,
        pub(super) event: PathBuf,
        pub(super) metadata: PathBuf,
        pub(super) usb: PathBuf,
    }

    #[cfg(target_os = "linux")]
    impl Sysfs {
        pub(super) fn new(virtual_node: bool) -> Self {
            use std::sync::atomic::AtomicUsize;
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "ultimate-input-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            let usb = root.join("sys/devices/pci0000:00/usb1/1-2");
            let metadata = if virtual_node {
                root.join("sys/devices/virtual/input/input7")
            } else {
                usb.join("1-2:1.0/gip0/gip0.0/input/input7")
            };
            fs::create_dir_all(metadata.join("id")).unwrap();
            fs::create_dir_all(metadata.join("capabilities")).unwrap();
            fs::create_dir_all(&usb).unwrap();
            fs::write(usb.join("idVendor"), "2dc8\n").unwrap();
            fs::write(usb.join("idProduct"), "202f\n").unwrap();
            fs::write(metadata.join("uniq"), "").unwrap();
            fs::write(metadata.join("capabilities/ff"), "107030000 0").unwrap();
            let event = root.join("sys/class/input/event7");
            fs::create_dir_all(&event).unwrap();
            std::os::unix::fs::symlink(&metadata, event.join("device")).unwrap();
            let fixture = Self { root, event, metadata, usb };
            fixture.identity(6, VENDOR, PRODUCT, "Microsoft Xbox Controller", "gip0.0/input0");
            fixture
        }

        fn identity(&self, bus: u16, vendor: u16, product: u16, name: &str, physical: &str) {
            for (field, value) in [("bustype", bus), ("vendor", vendor), ("product", product)] {
                fs::write(self.metadata.join("id").join(field), format!("{value:04x}\n")).unwrap();
            }
            fs::write(self.metadata.join("name"), name).unwrap();
            fs::write(self.metadata.join("phys"), physical).unwrap();
        }

        fn link(&self) -> Option<LinkKind> {
            inventory_device(&self.event).unwrap().map(|device| device.link)
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for Sysfs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn structural_validation_allows_clutch_bindings_but_runtime_rejects_them() {
        let mut profile = HostProfile::default();
        profile.mappings.insert("BTN_TRIGGER_HAPPY2".into(), "KEY_A".into());
        profile.turbo.insert("BTN_TRIGGER_HAPPY2".into(), 10.0);
        profile.toggle.push("BTN_TRIGGER_HAPPY2".into());
        assert!(profile.validate_structure().is_ok());
        assert!(profile.validate_for_link(LinkKind::Receiver).is_ok());
        assert!(profile.validate().is_err());
        assert!(profile.validate_for_link(LinkKind::Bluetooth).is_err());
        profile.turbo.insert("BTN_SOUTH".into(), 0.0);
        assert!(profile.validate_structure().is_err());
    }

    #[test]
    fn structural_validation_checks_clutch_settings_even_for_receiver_profiles() {
        let mut profile = HostProfile::default();
        profile.button = "BTN_SOUTH".into();
        assert!(profile.validate_structure().is_ok());
        assert!(profile.validate().is_err());
        assert!(profile.validate_for_link(LinkKind::Receiver).is_ok());
        profile.button = "R4".into();
        for sensitivity in [0.0, 100.1, f64::NAN] {
            profile.sensitivity = sensitivity;
            assert!(profile.validate_structure().is_err());
            assert!(profile.validate_for_link(LinkKind::Receiver).is_ok());
        }
        profile.sensitivity = 1.0;
        for deadzone in [-1.0, 2000.0, f64::NAN] {
            profile.deadzone = deadzone;
            assert!(profile.validate_structure().is_err());
            assert!(profile.validate_for_link(LinkKind::Receiver).is_ok());
        }
    }

    #[test]
    fn cable_clutch_rejects_invisible_paddles_only_when_receiver_gyro_is_used() {
        let mut profile = HostProfile { receiver_gyro: true, ..HostProfile::default() };
        for paddle in ["R4", "L4", "PL", "PR"] {
            profile.button = paddle.into();
            assert!(validate_cable_clutch(&profile).is_err(), "{paddle} is invisible over the cable");
        }
        profile.button = "BTN_THUMBR".into();
        assert!(validate_cable_clutch(&profile).is_ok());
        let plain = HostProfile { button: "R4".into(), ..HostProfile::default() };
        assert!(validate_cable_clutch(&plain).is_ok(), "no gyro clutch, nothing to reject");
    }

    #[test]
    fn receiver_virtual_pad_advertises_xpad_buttons_plus_explicit_destinations() {
        // Live: advertising the 24-code Bluetooth set shifted every browser button index.
        let mapper = Mapper::new_receiver(&HostProfile::default(), receiver_axes()).unwrap();
        assert_eq!(mapper.gamepad_keys, RECEIVER_EVDEV_KEYS);
        let mut profile = HostProfile::default();
        profile.mappings.insert("BTN_TRIGGER_HAPPY1".into(), "BTN_TRIGGER_HAPPY3".into());
        profile.mappings.insert("BTN_SOUTH".into(), "KEY_SPACE".into());
        let mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        assert_eq!(mapper.gamepad_keys.last(), Some(&706), "explicit gamepad destination advertised");
        assert_eq!(mapper.gamepad_keys.len(), RECEIVER_EVDEV_KEYS.len() + 1, "keyboard destination not on the pad");
        assert_eq!(Mapper::new(&HostProfile::default(), default_axes()).unwrap().gamepad_keys, KEY_CODES, "Bluetooth unchanged");
    }

    fn receiver_axes() -> [AxisInfo; 8] {
        let mut axes = default_axes();
        for axis in &mut axes[..4] {
            axis.minimum = -32768;
            axis.maximum = 32767;
            axis.value = 0;
            axis.flat = 0;
        }
        axes
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn steam_state_uses_positional_buttons_and_per_link_trigger_order() {
        use crate::steam::EdgeState as S;
        // Receiver: xpad layout, analog 4 = LT (ABS_Z), 5 = RT (ABS_RZ).
        let mut mapper = Mapper::new_with_receiver_gyro(&HostProfile::default(), receiver_axes(), LinkKind::Receiver, false).unwrap();
        // A, X (west), L4, PR.
        mapper.raw_pressed = 1 << 0 | 1 << 3 | 1 << 16 | 1 << 2;
        mapper.analog = [32767, -32768, 0, 0, mapper.axes[4].maximum, mapper.axes[5].minimum, -1, 1];
        let state = steam_state(&mapper);
        assert_eq!(state.buttons, S::CROSS | S::SQUARE | S::FN_LEFT | S::BACK_RIGHT | S::L2 | S::DPAD_LEFT | S::DPAD_DOWN);
        assert_eq!((state.triggers, state.left_stick), ([255, 0], [255, 0]));
        // Bluetooth HID: analog 4 = GAS (RT), 5 = BRAKE (LT).
        let mut mapper = Mapper::new(&HostProfile::default(), default_axes()).unwrap();
        mapper.raw_pressed = 1 << 4 | 1 << 5;
        mapper.analog[4] = mapper.axes[4].maximum;
        mapper.analog[5] = mapper.axes[5].minimum;
        let state = steam_state(&mapper);
        assert_eq!(state.triggers, [0, 255]);
        assert_eq!(state.buttons & (S::TRIANGLE | S::BACK_LEFT | S::R2 | S::L2), S::TRIANGLE | S::BACK_LEFT | S::R2);
    }

    fn receiver_frame(mapper: &mut Mapper, now: Duration, events: &[(u16, u16, i32)]) -> Frame {
        for &(kind, code, value) in events {
            assert!(mapper.receiver_event(Event { kind, code, value }, now).unwrap().is_empty());
        }
        mapper.receiver_event(Event::default(), now).unwrap()
    }

    fn receiver_mouse_frame(mapper: &mut Mapper, now: Duration, delta: [i32; 2]) -> Frame {
        for (code, value) in delta.into_iter().enumerate() {
            assert!(mapper.receiver_mouse_event(Event { kind: 2, code: code as u16, value }, now).unwrap().is_empty());
        }
        mapper.receiver_mouse_event(Event::default(), now).unwrap()
    }

    #[test]
    fn soft_tiered_smoothing_preserves_slow_motion_and_bypasses_fast_motion() {
        let settings = GyroSettings { smooth_threshold: 5.0, smooth_time_ms: 40.0, ..GyroSettings::default() };
        let mut filter = GyroFilter::default();
        assert_eq!(filter.sample([1.0, 0.0], 0.010, settings, 0.0), [0.25, 0.0]);
        assert_eq!(filter.sample([1.0, 0.0], 0.010, settings, 0.0), [0.5, 0.0]);
        let mut fast = GyroFilter::default();
        assert_eq!(fast.sample([10.0, 0.0], 0.010, settings, 0.0), [10.0, 0.0]);
        let mut blend = GyroFilter::default();
        assert_eq!(blend.sample([3.75, 0.0], 0.010, settings, 0.0), [2.34375, 0.0]);
        let mut impulse = GyroFilter::default();
        let mut integrated = impulse.sample([1.0, 0.0], 0.010, settings, 0.0)[0] * 0.010;
        for _ in 0..5 { integrated += impulse.sample([0.0; 2], 0.010, settings, 0.0)[0] * 0.010; }
        assert!((integrated - 0.010).abs() < 1e-12, "split smoothing must not lose displacement");
        let settings = GyroSettings { smooth_threshold: 0.0, ..settings };
        assert_eq!(filter.sample([1.0, 0.0], 0.010, settings, 0.0), [1.0, 0.0], "disabled filter must not drain old samples");
    }

    #[test]
    fn gyro_acceleration_endpoints_tightening_and_stick_compensation() {
        let settings = GyroSettings { min_sensitivity: 1.0, max_sensitivity: 4.0,
            min_threshold: 10.0, max_threshold: 100.0, smooth_threshold: 0.0, ..GyroSettings::default() };
        assert_eq!(settings.sensitivity(0.0), 1.0);
        assert_eq!(settings.sensitivity(10.0), 1.0);
        assert_eq!(settings.sensitivity(55.0), 2.5);
        assert_eq!(settings.sensitivity(100.0), 4.0);
        assert_eq!(settings.sensitivity(1000.0), 4.0);
        let tightened = GyroFilter::default().sample([0.1,0.0],0.004,settings,0.5);
        assert!((tightened[0] - 0.02).abs() < 1e-12);
        let stick = GyroSettings::default();
        assert_eq!(stick.stick([0.0;2]), [0.0;2]);
        let small = stick.stick([0.001,0.0]);
        assert!(small[0] > 0.15 * 32767.0 && small[0] < 0.151 * 32767.0);
        assert_eq!(stick.stick([180.0,0.0]), [32767.0,0.0]);
        assert_eq!(stick.stick([360.0,0.0]), [32767.0,0.0]);
        let diagonal = stick.stick([180.0,180.0]);
        assert!((diagonal[0].hypot(diagonal[1]) - 32767.0).abs() < 1e-9);
    }

    #[test]
    fn old_profiles_load_new_gyro_defaults_and_reject_invalid_filters() {
        let profile: HostProfile = serde_json::from_str(r#"{"sensitivity":2.0,"deadzone":0.25,
            "invert_x":true,"invert_y":false,"receiver_gyro":true,
            "gyro_mouse":{"sensitivity":3.0,"deadzone":0.1,"invert_x":false,"invert_y":true}}"#).unwrap();
        assert_eq!(profile.sensitivity,2.0);
        assert_eq!(profile.gyro.stick_saturation,180.0);
        assert_eq!(profile.gyro.smooth_time_ms,40.0);
        assert!(profile.validate_structure().is_ok());
        for json in [r#"{"gyro":{"stick_saturation":0}}"#,r#"{"gyro":{"stick_deadzone":1}}"#,
            r#"{"gyro":{"min_threshold":100,"max_threshold":50}}"#,r#"{"gyro":{"smooth_time_ms":251}}"#] {
            assert!(serde_json::from_str::<HostProfile>(json).unwrap().validate_structure().is_err());
        }
        assert!(serde_json::from_str::<HostProfile>(r#"{"gyro":{"smooth_time":40}}"#).is_err());
    }

    #[test]
    fn slot_map_remaps_all_buttons_paddles_triggers_and_dpad_without_xpad_duplicates() {
        use crate::profile::{KeyMap,PhysicalButton as B,MappingTarget as T};
        let mut map = KeyMap { flag:0, mapping:std::array::from_fn(|i| {
            let mask=B::ALL[i].mask().bits(); if mask<=0xffff { T::from_raw(mask) } else { T::None }
        }) };
        map.set(B::P1,T::L3).unwrap();
        map.set(B::Lt,T::B).unwrap();
        map.set(B::A,T::Rt).unwrap();
        let mut mapper=Mapper::new_receiver(&HostProfile::default(),receiver_axes()).unwrap();
        mapper.set_key_map(Some(map.clone()));
        receiver_frame(&mut mapper,Duration::ZERO,&[(1,304,1),(3,2,180)]);
        assert!(mapper.raw_pressed&1!=0,"xpad identity before the heartbeat's first physical report");
        // Standard buttons/triggers come from xpad; physical reports supply only paddles
        // (live: their standard bits are lossy).
        let frame=mapper.receiver_physical_at(physical(0x02000000|0x40000000|0x2000,[127,127]),Duration::ZERO).unwrap();
        assert!(has(&frame,1,317,1),"PL maps to L3");
        assert!(has(&frame,1,705,1),"Null R4 remains a paddle source");
        assert!(has(&frame,1,305,1),"xpad LT maps to B");
        assert!(has(&frame,1,304,0),"xpad A is remapped away from A");
        assert_eq!(mapper.raw_pressed&(1<<5),0,"mapped PL must not duplicate its host source");
        assert_eq!(mapper.analog[4],0,"LT analog suppressed");
        assert_eq!(mapper.analog[5],255,"A targets full RT");
        receiver_frame(&mut mapper,Duration::from_millis(1),&[(3,2,200)]);
        assert_eq!(mapper.analog[4],0,"xpad updates cannot restore remapped LT");
        receiver_frame(&mut mapper,Duration::from_millis(2),&[(1,304,0)]);
        assert_eq!(mapper.analog[5],0,"xpad A release releases its RT target");
        receiver_frame(&mut mapper,Duration::from_millis(3),&[(1,304,1)]);
        mapper.receiver_physical_at(physical(0x2000,[127,127]),Duration::from_millis(4)).unwrap();
        receiver_frame(&mut mapper,Duration::from_millis(5),&[(1,304,0)]);
        assert_eq!(mapper.analog[5],0,"stale physical A bit never holds a button");
        receiver_frame(&mut mapper,Duration::from_millis(6),&[(3,17,-1)]);
        mapper.receiver_physical_at(physical(0x06000200,[127,127]),Duration::from_millis(7)).unwrap();
        assert_eq!(mapper.raw_pressed&((1<<5)|(1<<2)|(1<<13)),0,"D-pad PL|PR quirk filtered before mapping");
        assert_eq!(mapper.analog[7],-1);
        let mut default=Mapper::new_receiver(&HostProfile::default(),receiver_axes()).unwrap();
        receiver_frame(&mut default,Duration::ZERO,&[(1,304,1)]);
        default.receiver_physical_at(physical(0x02000000,[127,127]),Duration::ZERO).unwrap();
        assert_eq!(default.physical_state().buttons,1|(1<<5),"default slot = identity plus host paddle");
        let mut bluetooth=Mapper::new(&HostProfile::default(),default_axes()).unwrap();
        bluetooth.set_key_map(Some(map));
        bluetooth.physical_input_at(PhysicalInput{left_trigger:255,buttons:0x4000|0x02000000,..physical(0,[127,127])},Duration::ZERO);
        let stale=report(1,[0;3]);
        bluetooth.input_at(&stale,Duration::from_millis(4)).unwrap();
        assert_eq!(bluetooth.raw_pressed, (1<<1)|(1<<13));
        assert_eq!(bluetooth.analog[5],0,"BLE uses same trigger suppression");
    }

    fn receiver_gyro_profile() -> HostProfile {
        HostProfile { receiver_gyro: true, button: "BTN_THUMBR".into(), deadzone: 0.0,
            gyro: GyroSettings { smooth_threshold: 0.0, stick_smoothing_ms: 0.0, ..GyroSettings::default() },
            ..HostProfile::default() }
    }

    #[test]
    fn receiver_gyro_validation_requires_visible_clutch_and_consumes_bindings() {
        let mut profile = receiver_gyro_profile();
        assert!(profile.validate_structure().is_ok());
        assert_eq!(profile.validate_for_link(LinkKind::Receiver).unwrap(), Some(RECEIVER_GYRO_NOTICE));
        for &(button, _) in RECEIVER_CLUTCH_KEYS {
            profile.button = button.into();
            assert!(profile.validate_for_link(LinkKind::Receiver).is_ok());
        }
        for button in ["BTN_C", "BTN_Z", "BTN_TRIGGER_HAPPY2", "BTN_TL2", "KEY_SPACE"] {
            profile.button = button.into();
            assert!(profile.validate_for_link(LinkKind::Receiver).unwrap_err().to_string().contains("visible standard Linux button"));
        }
        profile.button = "BTN_THUMBR".into();
        profile.mappings.insert(profile.button.clone(), "KEY_SPACE".into());
        assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        profile.mappings.clear();
        profile.sensitivity = f64::NAN;
        assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        let old: HostProfile = serde_json::from_str("{}").unwrap();
        assert!(!old.receiver_gyro);
        assert!(serde_json::from_str::<HostProfile>(r#"{"receiver_gyroo":true}"#).is_err());
        let saved = serde_json::to_vec(&receiver_gyro_profile()).unwrap();
        let loaded: HostProfile = serde_json::from_slice(&saved).unwrap();
        assert!(loaded.receiver_gyro);
        assert!(loaded.validate_structure().is_ok());
    }

    #[test]
    fn receiver_physical_paddles_map_release_and_ignore_evdev_aliases() {
        let sample = PhysicalInput { buttons: 0, left_x: 0, left_y: 0, right_x: 255, right_y: 255, left_trigger: 255, right_trigger: 255 };
        for (name, source, mask) in [("BTN_TRIGGER_HAPPY1",704,0x00200000),("BTN_TRIGGER_HAPPY2",705,0x40000000),("BTN_Z",309,0x02000000),("BTN_C",306,0x04000000)] {
            let mut profile = HostProfile::default();
            profile.mappings.insert(name.into(),"KEY_SPACE".into());
            let mut mapper = Mapper::new_receiver(&profile,receiver_axes()).unwrap();
            receiver_frame(&mut mapper,Duration::ZERO,&[]);
            let fake = receiver_frame(&mut mapper,Duration::ZERO,&[(1,source,1)]);
            assert!(!has(&fake,1,57,1), "evdev never supplies physical paddles");
            let down = mapper.receiver_physical_at(PhysicalInput{buttons:mask|0x2000,..sample},Duration::ZERO).unwrap();
            assert!(has(&down,1,57,1));
            assert!(!has(&down,1,304,1), "standard buttons come from xpad, never lossy physical bits");
            assert_eq!(mapper.analog[2..4],[0,0], "physical axes do not replace xpad");
            // Physical reports are change-driven: silence and xpad frames keep the paddle held.
            receiver_frame(&mut mapper,Duration::from_millis(500),&[(1,304,1)]);
            assert!(!has(&mapper.tick(Duration::from_secs(10)),1,57,0), "held paddle survives report silence");
            let up = mapper.receiver_physical_at(sample,Duration::from_secs(11)).unwrap();
            assert!(has(&up,1,57,0), "next report releases it");
        }
    }

    #[test]
    fn receiver_direct_r4_clutch_needs_no_controller_remap() {
        let profile = HostProfile { receiver_gyro: true, ..HostProfile::default() };
        let mut mapper = Mapper::new_receiver(&profile,receiver_axes()).unwrap();
        receiver_frame(&mut mapper,Duration::ZERO,&[]);
        receiver_mouse_frame(&mut mapper,Duration::from_millis(10),[308,0]);
        assert!(mapper.values[2].unwrap() > 0);
        let state = PhysicalInput { buttons:0x40000000,left_x:128,left_y:128,right_x:128,right_y:128,left_trigger:0,right_trigger:0 };
        let held = mapper.receiver_physical_at(state,Duration::from_millis(11)).unwrap();
        assert!(has(&held,3,3,0));
        assert!(!held.as_slice().iter().any(|event|event.kind==1 && event.code==705));
        receiver_frame(&mut mapper,Duration::from_millis(12),&[(1,304,1)]);
        assert!(mapper.held,"xpad SYN must retain physical clutch");
        let blocked = receiver_mouse_frame(&mut mapper,Duration::from_millis(20),[100,0]);
        assert!(!blocked.as_slice().iter().any(|event|event.kind==3 && event.value!=0));
        mapper.receiver_physical_at(PhysicalInput{buttons:0,..state},Duration::from_millis(21)).unwrap();
        assert!(!mapper.held);
        receiver_mouse_frame(&mut mapper,Duration::from_millis(30),[308,0]);
        assert!(mapper.values[2].unwrap() > 0);
        mapper.receiver_physical_at(state,Duration::from_millis(31)).unwrap();
        mapper.tick(Duration::from_secs(10));
        assert!(mapper.held,"a held R4 stays held without further reports (live: change-driven)");
        let blocked = receiver_mouse_frame(&mut mapper,Duration::from_secs(10),[100,0]);
        assert!(!blocked.as_slice().iter().any(|event|event.kind==3 && event.value!=0));
    }

    #[test]
    fn receiver_gyro_clutch_consumed_suspends_then_resumes_on_fresh_motion() {
        let mut mapper = Mapper::new_receiver(&receiver_gyro_profile(), receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[]);
        let frame = receiver_mouse_frame(&mut mapper, Duration::from_millis(10), [1, 0]);
        assert!(frame.as_slice().iter().any(|event| event.kind == 3 && event.code == RECEIVER_AXES[2] && event.value > 0));
        let frame = receiver_frame(&mut mapper, Duration::from_millis(11), &[(1, 318, 1)]);
        assert!(has(&frame, 3, RECEIVER_AXES[2], 0));
        assert!(!frame.as_slice().iter().any(|event| event.kind == 1 && event.code == 318));
        let frame = receiver_mouse_frame(&mut mapper, Duration::from_millis(20), [100, -100]);
        assert!(!frame.as_slice().iter().any(|event| event.kind == 2 || (event.kind == 3 && event.value != 0)));
        let frame = receiver_frame(&mut mapper, Duration::from_millis(21), &[(1, 318, 0)]);
        assert!(!frame.as_slice().iter().any(|event| event.kind == 1 && event.code == 318));
        assert_eq!(mapper.gyro_right, [0.0; 2]);
        let frame = receiver_mouse_frame(&mut mapper, Duration::from_millis(30), [1, 0]);
        assert!(frame.as_slice().iter().any(|event| event.kind == 3 && event.code == RECEIVER_AXES[2] && event.value > 0));
    }

    #[test]
    fn receiver_gyro_mouse_accumulates_fractions_inverts_and_never_replays_velocity() {
        let mut profile = receiver_gyro_profile();
        profile.gyro_mouse = Some(GyroMouse { sensitivity: RECEIVER_COUNTS_PER_DEGREE * 0.25, deadzone: 0.0, invert_x: true, ..GyroMouse::default() });
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[]);
        for tick in 1..4 {
            let frame = receiver_mouse_frame(&mut mapper, Duration::from_millis(tick * 10), [1, -1]);
            assert!(!frame.as_slice().iter().any(|event| event.kind == 2));
        }
        let frame = receiver_mouse_frame(&mut mapper, Duration::from_millis(40), [1, -1]);
        assert!(has(&frame, 2, 0, -1));
        assert!(has(&frame, 2, 1, -1));
        assert!(!mapper.tick(Duration::from_millis(50)).as_slice().iter().any(|event| event.kind == 2));
        receiver_mouse_frame(&mut mapper, Duration::from_millis(51), [1, 1]);
        receiver_frame(&mut mapper, Duration::from_millis(52), &[(1, 318, 1)]);
        receiver_mouse_frame(&mut mapper, Duration::from_millis(60), [100, 100]);
        receiver_frame(&mut mapper, Duration::from_millis(61), &[(1, 318, 0)]);
        let frame = receiver_mouse_frame(&mut mapper, Duration::from_millis(70), [3, 3]);
        assert!(!frame.as_slice().iter().any(|event| event.kind == 2)); // clutch discarded prior fraction
        assert!(mapper.receiver_mouse_event(Event { kind: 0, code: 3, value: 0 }, Duration::from_millis(80)).is_err());
        mapper.close();
        assert!(mapper.receiver_mouse_event(Event::default(), Duration::from_millis(90)).is_err());
    }

    #[test]
    fn receiver_gyro_timing_holds_between_reports_and_releases_smoothly() {
        let profile = HostProfile { gyro: GyroSettings { smooth_threshold: 0.0, stick_smoothing_ms: 8.0, ..GyroSettings::default() }, ..receiver_gyro_profile() };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[(3, 3, 1000), (3, 4, -2000)]);
        receiver_mouse_frame(&mut mapper, Duration::from_micros(4000), [12, -12]);
        let first = mapper.values[2].unwrap();
        assert!(first > 1000);
        mapper.tick(Duration::from_micros(7500));
        assert_eq!(mapper.values[2], Some(first), "no reset between 4-ms motion reports");
        receiver_mouse_frame(&mut mapper, Duration::from_micros(8250), [12, -12]);
        let angular = mapper.angular_velocity().unwrap();
        assert!((angular[1] - 12.0 / (0.00425 * 308.0)).abs() < 0.00001, "fractional intervals survive");
        mapper.tick(Duration::from_millis(20));
        assert!(mapper.values[2].unwrap() > 1000, "release is gradual");
        mapper.tick(Duration::from_millis(200));
        assert_eq!(mapper.values[2], Some(1000));
        assert_eq!(mapper.values[3], Some(-2000));
        // Saturation clamps the radial vector, then mixing clamps each output axis.
        receiver_frame(&mut mapper, Duration::from_millis(201), &[(3, 3, 30000)]);
        receiver_mouse_frame(&mut mapper, Duration::from_millis(205), [10000, 0]);
        assert_eq!(mapper.values[2], Some(32767));
    }

    #[test]
    fn receiver_gyro_uses_kernel_sample_times_when_reports_are_batched() {
        let mut mapper=Mapper::new_receiver(&receiver_gyro_profile(),receiver_axes()).unwrap();
        for (sample_us,counts) in [(1_000_000,4),(1_004_125,8)] {
            let now=Duration::from_millis(20);
            let source=Duration::from_micros(sample_us);
            mapper.receiver_mouse_event_timed(Event{kind:2,code:0,value:counts},now,source).unwrap();
            mapper.receiver_mouse_event_timed(Event::default(),now,source).unwrap();
        }
        assert!((mapper.angular_velocity().unwrap()[1] - 8.0/(308.0*0.004125)).abs()<1e-9);
        let mut bluetooth=Mapper::new(&HostProfile::default(),default_axes()).unwrap();
        bluetooth.input_at(&report(0,[0,16384,-16384]),Duration::ZERO).unwrap();
        let angular=bluetooth.angular_velocity().unwrap();
        assert!((angular[0]+16384.0*2000.0/32767.0).abs()<1e-9);
        assert!((angular[1]-16384.0*2000.0/32767.0).abs()<1e-9);
        assert_eq!(angular[2],0.0);
        bluetooth.pause_motion();
        assert!(bluetooth.angular_velocity().is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn receiver_mouse_selection_requires_opt_in_same_usb_input_four_and_rel_xy() {
        let fixture = Sysfs::new(false);
        fixture.identity(3, VENDOR, 0x3024, "Generic X-Box pad", "usb-1-2/input0");
        fs::write(fixture.usb.join("idProduct"), "3024").unwrap();
        let device = inventory_device(&fixture.event).unwrap().unwrap();
        // Disabled selection returns before any filesystem lookup (and therefore open).
        assert!(receiver_mouse_path(&HostProfile::default(), &device, Path::new("/nonexistent-input-class")).unwrap().is_none());
        let class = fixture.event.parent().unwrap();
        let profile = receiver_gyro_profile();
        assert!(receiver_mouse_path(&profile, &device, class).unwrap().is_none());
        let mouse = fixture.usb.join("1-2:1.4/0003:2DC8:3024/input/input8");
        fs::create_dir_all(mouse.join("id")).unwrap();
        fs::create_dir_all(mouse.join("capabilities")).unwrap();
        for field in ["bustype", "vendor", "product"] {
            fs::copy(fixture.metadata.join("id").join(field), mouse.join("id").join(field)).unwrap();
        }
        fs::write(mouse.join("phys"), "usb-1-2/input4").unwrap();
        fs::write(mouse.join("capabilities/ev"), "7").unwrap();
        fs::write(mouse.join("capabilities/rel"), "103").unwrap();
        fs::create_dir_all(class.join("event8")).unwrap();
        std::os::unix::fs::symlink(&mouse, class.join("event8/device")).unwrap();
        assert_eq!(receiver_mouse_path(&profile, &device, class).unwrap(), Some(PathBuf::from("/dev/input/event8")));
        fs::write(mouse.join("capabilities/rel"), "101").unwrap();
        assert!(receiver_mouse_path(&profile, &device, class).unwrap().is_none());
        fs::write(mouse.join("capabilities/rel"), "103").unwrap();
        fs::write(mouse.join("phys"), "usb-1-2/input3").unwrap();
        assert!(receiver_mouse_path(&profile, &device, class).unwrap().is_none());
        fs::write(mouse.join("phys"), "usb-1-2/input4").unwrap();
        // A nearer USB parent, even with identical VID/PID, is a different device.
        fs::write(mouse.parent().unwrap().join("idVendor"), "2dc8").unwrap();
        fs::write(mouse.parent().unwrap().join("idProduct"), "3024").unwrap();
        assert!(receiver_mouse_path(&profile, &device, class).unwrap().is_none());
        let mut fallback = Mapper::new_with_receiver_gyro(&profile, receiver_axes(), LinkKind::Receiver, false).unwrap();
        let frame = receiver_frame(&mut fallback, Duration::ZERO, &[(1, 318, 1)]);
        assert!(has(&frame, 1, 318, 1)); // absent mouse retains ordinary receiver passthrough
        assert!(fallback.receiver_mouse_event(Event::default(), Duration::ZERO).is_err());
    }

    fn has(frame: &Frame, kind: u16, code: u16, value: i32) -> bool {
        frame.as_slice().contains(&Event { kind, code, value })
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn receiver_inventory_identifies_only_xpad_input_zero() {
        let fixture = Sysfs::new(false);
        fixture.identity(3, VENDOR, 0x3024, "Xbox 360 Controller", "usb-0000:00:14.0-1/input0");
        assert_eq!(fixture.link(), Some(LinkKind::Receiver));
        fixture.identity(3, VENDOR, 0x3024, "Xbox 360 Controller", "usb-0000:00:14.0-1/input4");
        assert_eq!(fixture.link(), None);
        fixture.identity(5, VENDOR, PRODUCT, "8BitDo Ultimate", "aa:bb:cc");
        assert_eq!(fixture.link(), Some(LinkKind::Bluetooth));
        fixture.identity(3, VENDOR, PRODUCT, "8BitDo Ultimate", "usb/input0");
        assert_eq!(fixture.link(), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wired_inventory_requires_the_actual_ultimate_3e_usb_parent() {
        let fixture = Sysfs::new(false);
        let device = inventory_device(&fixture.event).unwrap().unwrap();
        assert_eq!(device.link, LinkKind::Wired);
        assert_eq!(device.path, Path::new("/dev/input/event7"));
        assert_eq!(device.identity, "gip0.0/input0");
        assert!(device.hidraw.is_none());
        assert!(!device.hidraw_access);
        // Input IDs/names may be supplied by GIP; the USB parent is authoritative.
        fixture.identity(6, 0x045e, 0x02ea, "Microsoft Xbox Controller", "gip0.0/input0");
        assert_eq!(fixture.link(), Some(LinkKind::Wired));
        fs::write(fixture.usb.join("idVendor"), "045e").unwrap();
        assert_eq!(fixture.link(), None); // A genuine Microsoft controller.
        fs::write(fixture.usb.join("idVendor"), "2dc8").unwrap();
        fs::write(fixture.usb.join("idProduct"), "3024").unwrap();
        assert_eq!(fixture.link(), None);
        fs::write(fixture.usb.join("idProduct"), "202f").unwrap();
        fs::remove_file(fixture.usb.join("idVendor")).unwrap();
        assert_eq!(fixture.link(), None); // No USB identity → no guessed match.
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wired_inventory_checks_gip_metadata_and_nearest_usb_device() {
        let fixture = Sysfs::new(false);
        for (bus, name, physical) in [
            (3, "Microsoft Xbox Controller", "gip0.0/input0"),
            (6, "Other Controller", "gip0.0/input0"),
            (6, "Microsoft Xbox Controller", "usb-1/input0"),
            (6, "Microsoft Xbox Controller", "gyro-clutch/test/gamepad"),
        ] {
            fixture.identity(bus, VENDOR, PRODUCT, name, physical);
            assert_eq!(fixture.link(), None);
        }
        fixture.identity(6, VENDOR, PRODUCT, "Microsoft Xbox Controller", "gip0.0/input0");
        fs::write(fixture.usb.join("idVendor"), "045e").unwrap();
        let upstream = fixture.usb.parent().unwrap();
        fs::write(upstream.join("idVendor"), "2dc8").unwrap();
        fs::write(upstream.join("idProduct"), "202f").unwrap();
        assert_eq!(fixture.link(), None); // Never inherit an upstream hub's ID.
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inventory_rejects_virtual_uinput_outputs_for_every_link() {
        for virtual_tree in [false, true] {
            let fixture = Sysfs::new(virtual_tree);
            for (bus, product, name, physical) in [
                (5, PRODUCT, "8BitDo Ultimate", "aa:bb:cc"),
                (3, 0x3024, "Xbox 360 Controller", "usb-1/input0"),
                (6, PRODUCT, "Microsoft Xbox Controller", "gip0.0/input0"),
            ] {
                fixture.identity(bus, VENDOR, product, name,
                    if virtual_tree { physical } else { "gyro-clutch/test/gamepad" });
                assert_eq!(fixture.link(), None);
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inventory_keeps_bluez_uhid_gamepads() {
        // Live: BlueZ HID pads resolve to /sys/devices/virtual/misc/uhid/<hid>/input/inputN.
        let mut fixture = Sysfs::new(false);
        let uhid = fixture.root.join("sys/devices/virtual/misc/uhid/0005:2DC8:202F.000D/input/input47");
        fs::create_dir_all(uhid.parent().unwrap()).unwrap();
        fs::rename(&fixture.metadata, &uhid).unwrap();
        fs::remove_file(fixture.event.join("device")).unwrap();
        std::os::unix::fs::symlink(&uhid, fixture.event.join("device")).unwrap();
        fixture.metadata = uhid;
        fixture.identity(5, VENDOR, PRODUCT, "Ultimate 3 Controller", "aa:bb:cc:dd:ee:01");
        assert_eq!(fixture.link(), Some(LinkKind::Bluetooth));
    }

    #[test]
    fn wired_mapper_ignores_clutch_and_maps_ten_bit_triggers() {
        let mut axes = receiver_axes();
        axes[4].maximum = 1023;
        axes[5].maximum = 1023;
        let mut profile = HostProfile {
            button: "ignored clutch".into(), sensitivity: f64::NAN, deadzone: -1.0,
            ..HostProfile::default()
        };
        profile.mappings.insert("BTN_SOUTH".into(), "KEY_SPACE".into());
        profile.trigger_buttons.left = Some(TriggerButton { destination: "KEY_A".into(), threshold: 0.5 });
        assert_eq!(profile.validate_for_link(LinkKind::Wired).unwrap(), Some(WIRED_NOTICE));
        let mut mapper = Mapper::new_wired(&profile, axes).unwrap();
        let frame = receiver_frame(&mut mapper, Duration::ZERO, &[
            (1, 304, 1), (3, 2, 511), (3, 5, 1023), (3, 3, 32767),
        ]);
        assert!(has(&frame, 1, 57, 1));
        assert!(!has(&frame, 1, 30, 1));
        assert!(has(&frame, 3, RECEIVER_AXES[4], 511));
        assert!(has(&frame, 3, RECEIVER_AXES[5], 1023));
        assert!(has(&frame, 3, RECEIVER_AXES[2], 32767));
        let frame = receiver_frame(&mut mapper, Duration::from_millis(1), &[(3, 2, 512)]);
        assert!(has(&frame, 1, 30, 1));
        assert!(mapper.receiver_event(Event { kind: 0, code: 3, value: 0 }, Duration::ZERO).is_err());
        assert!(Mapper::new_wired(&profile, receiver_axes()).is_err());
        profile.turbo.insert("BTN_SOUTH".into(), 0.0);
        assert!(profile.validate_for_link(LinkKind::Wired).is_err());
        assert_eq!(serde_json::to_string(&LinkKind::Wired).unwrap(), "\"wired\"");
    }

    #[test]
    fn old_profile_json_defaults_and_receiver_clutch_notice() {
        let profile: HostProfile = serde_json::from_str(r#"{
            "button":"R4","sensitivity":1.0,"deadzone":0.5,
            "invert_x":false,"invert_y":false,"mappings":{"BTN_SOUTH":"KEY_SPACE"}
        }"#).unwrap();
        assert!(profile.turbo.is_empty());
        assert!(profile.toggle.is_empty());
        assert!(profile.stick_mouse.is_none());
        assert!(profile.trigger_buttons.left.is_none());
        assert!(profile.layers.is_empty());
        assert!(profile.gyro_mouse.is_none());
        profile.validate().unwrap();
        assert_eq!(profile.validate_for_link(LinkKind::Receiver).unwrap(), Some(RECEIVER_NOTICE));
        let ignored = HostProfile { button: "not a clutch".into(), sensitivity: f64::NAN, deadzone: -1.0, ..profile };
        assert!(ignored.validate().is_err());
        assert!(ignored.validate_for_link(LinkKind::Receiver).is_ok());
        let mut profile = HostProfile::default();
        profile.mappings.insert("BTN_TRIGGER_HAPPY2".into(), "KEY_A".into());
        assert!(profile.validate().is_err());
        assert!(profile.validate_for_link(LinkKind::Receiver).is_ok());
    }

    #[test]
    fn receiver_xinput_buttons_axes_and_many_to_one_frames() {
        let mut profile = HostProfile::default();
        profile.mappings.insert("BTN_SOUTH".into(), "KEY_SPACE".into());
        profile.mappings.insert("BTN_EAST".into(), "KEY_SPACE".into());
        profile.mappings.insert("BTN_TR".into(), "BTN_LEFT".into());
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        let frame = receiver_frame(&mut mapper, Duration::ZERO, &[
            (1, 304, 1), (1, 305, 1), (1, 311, 1),
            (3, 0, -32768), (3, 1, 32767), (3, 3, 12345), (3, 4, -23456),
            (3, 2, 128), (3, 5, 255), (3, 16, -1), (3, 17, 1),
        ]);
        assert_eq!(frame.as_slice().iter().filter(|event| event.kind == 1 && event.code == 57).count(), 1);
        assert!(has(&frame, 1, 57, 1));
        assert!(has(&frame, 1, 272, 1));
        // Output mirrors the xpad layout: right stick ABS_RX/RY (3/4), triggers ABS_Z/RZ (2/5).
        for (code, value) in [(0, -32768), (1, 32767), (3, 12345), (4, -23456), (2, 128), (5, 255), (16, -1), (17, 1)] {
            assert!(has(&frame, 3, code, value));
        }
        assert_eq!(frame.as_slice().last(), Some(&Event::default()));
        let frame = receiver_frame(&mut mapper, Duration::from_millis(1), &[(1, 304, 0)]);
        assert!(!has(&frame, 1, 57, 0));
        let frame = receiver_frame(&mut mapper, Duration::from_millis(2), &[(1, 305, 0), (1, 311, 0)]);
        assert!(has(&frame, 1, 57, 0));
        assert!(has(&frame, 1, 272, 0));
    }

    #[test]
    fn receiver_turbo_clock_and_shared_destination_ownership() {
        let mut profile = HostProfile::default();
        profile.turbo.insert("BTN_SOUTH".into(), 10.0);
        profile.mappings.insert("BTN_SOUTH".into(), "KEY_A".into());
        profile.mappings.insert("BTN_EAST".into(), "KEY_A".into());
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        let frame = receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 1)]);
        assert!(has(&frame, 1, 30, 1));
        assert!(mapper.tick(Duration::from_millis(49)).is_empty());
        assert!(has(&mapper.tick(Duration::from_millis(50)), 1, 30, 0));
        assert!(has(&mapper.tick(Duration::from_millis(100)), 1, 30, 1));
        receiver_frame(&mut mapper, Duration::from_millis(120), &[(1, 305, 1)]);
        assert!(!has(&mapper.tick(Duration::from_millis(150)), 1, 30, 0));
        let frame = receiver_frame(&mut mapper, Duration::from_millis(160), &[(1, 305, 0)]);
        assert!(has(&frame, 1, 30, 0));
        receiver_frame(&mut mapper, Duration::from_millis(170), &[(1, 304, 0)]);
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn receiver_toggle_edges_repeat_and_turbo_combination() {
        let mut profile = HostProfile::default();
        profile.toggle.push("BTN_SOUTH".into());
        profile.turbo.insert("BTN_SOUTH".into(), 5.0);
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        assert!(has(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 1)]), 1, 304, 1));
        assert!(receiver_frame(&mut mapper, Duration::from_millis(10), &[(1, 304, 2)]).is_empty());
        assert!(receiver_frame(&mut mapper, Duration::from_millis(20), &[(1, 304, 0)]).is_empty());
        assert!(has(&mapper.tick(Duration::from_millis(100)), 1, 304, 0));
        assert!(has(&mapper.tick(Duration::from_millis(200)), 1, 304, 1));
        assert!(has(&receiver_frame(&mut mapper, Duration::from_millis(210), &[(1, 304, 1)]), 1, 304, 0));
        receiver_frame(&mut mapper, Duration::from_millis(220), &[(1, 304, 0)]);
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn receiver_stick_mouse_integrates_previous_velocity_without_acceleration() {
        let profile = HostProfile {
            stick_mouse: Some(StickMouse { stick: Stick::Right, sensitivity: 100.0, deadzone: 0.2 }),
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        let initial = receiver_frame(&mut mapper, Duration::ZERO, &[(3, 3, 32767)]);
        assert!(!initial.as_slice().iter().any(|event| event.kind == 2 || (event.kind == 3 && event.code == RECEIVER_AXES[2])));
        assert!(mapper.tick(Duration::from_millis(3)).is_empty());
        assert!(has(&mapper.tick(Duration::from_millis(10)), 2, 0, 1));
        assert!(has(&mapper.tick(Duration::from_millis(110)), 2, 0, 10));
        let stopped = receiver_frame(&mut mapper, Duration::from_millis(210), &[(3, 3, 3000)]);
        assert!(has(&stopped, 2, 0, 10));
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
        receiver_frame(&mut mapper, Duration::from_secs(1), &[(3, 3, 0), (3, 4, -32768)]);
        assert!(has(&mapper.tick(Duration::from_millis(1100)), 2, 1, -10));
        mapper.close();
        assert!(mapper.tick(Duration::from_secs(2)).is_empty());
    }

    #[test]
    fn receiver_left_mouse_uses_signed_axes_and_radial_deadzone() {
        let profile = HostProfile {
            stick_mouse: Some(StickMouse { stick: Stick::Left, sensitivity: 100.0, deadzone: 0.5 }),
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[(3, 0, 10000), (3, 1, 10000)]);
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
        receiver_frame(&mut mapper, Duration::from_secs(1), &[(3, 0, -32768), (3, 1, 0)]);
        assert!(has(&mapper.tick(Duration::from_secs(2)), 2, 0, -100));
    }

    #[test]
    fn receiver_trigger_threshold_owns_shared_destination_until_both_release() {
        let mut profile = HostProfile::default();
        profile.trigger_buttons.left = Some(TriggerButton { destination: "KEY_SPACE".into(), threshold: 0.5 });
        profile.trigger_buttons.right = Some(TriggerButton { destination: "KEY_SPACE".into(), threshold: 1.0 });
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        assert!(!has(&receiver_frame(&mut mapper, Duration::ZERO, &[(3, 2, 127)]), 1, 57, 1));
        assert!(has(&receiver_frame(&mut mapper, Duration::ZERO, &[(3, 2, 128)]), 1, 57, 1));
        assert!(!has(&receiver_frame(&mut mapper, Duration::ZERO, &[(3, 5, 255), (3, 2, 0)]), 1, 57, 0));
        assert!(has(&receiver_frame(&mut mapper, Duration::ZERO, &[(3, 5, 254)]), 1, 57, 0));
    }

    #[test]
    fn receiver_close_releases_latches_thresholds_keys_and_axes() {
        let mut profile = HostProfile::default();
        profile.toggle.push("BTN_SOUTH".into());
        profile.mappings.insert("BTN_SOUTH".into(), "KEY_A".into());
        profile.mappings.insert("BTN_EAST".into(), "BTN_LEFT".into());
        profile.trigger_buttons.left = Some(TriggerButton { destination: "KEY_B".into(), threshold: 0.5 });
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 1), (1, 305, 1), (3, 0, 32767), (3, 2, 255)]);
        let frame = mapper.close();
        for code in [30, 48, 272] { assert!(has(&frame, 1, code, 0)); }
        assert!(has(&frame, 3, 0, 0));
        assert!(has(&frame, 3, RECEIVER_AXES[4], 0));
        assert!(mapper.close().is_empty());
        assert!(mapper.receiver_event(Event::default(), Duration::ZERO).is_err());
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        assert!(mapper.receiver_event(Event { kind: 0, code: 3, value: 0 }, Duration::ZERO).is_err());
    }

    #[test]
    fn extras_validate_ranges_and_source_aliases() {
        for rate in [f64::NAN, f64::INFINITY, 0.0, 100.1] {
            let mut profile = HostProfile::default();
            profile.turbo.insert("BTN_SOUTH".into(), rate);
            assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        }
        let mut profile = HostProfile::default();
        profile.toggle = vec!["BTN_A".into(), "BTN_SOUTH".into()];
        assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        profile.toggle.clear();
        profile.turbo.insert("BTN_A".into(), 10.0);
        profile.turbo.insert("BTN_SOUTH".into(), 20.0);
        assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        profile.turbo.clear();
        for sensitivity in [0.0, 10001.0, f64::NAN] {
            profile.stick_mouse = Some(StickMouse { stick: Stick::Left, sensitivity, deadzone: 0.0 });
            assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        }
        for deadzone in [-0.1, 1.0, f64::NAN] {
            profile.stick_mouse = Some(StickMouse { stick: Stick::Right, sensitivity: 100.0, deadzone });
            assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        }
        profile.stick_mouse = None;
        for threshold in [0.0, 1.1, f64::NAN] {
            profile.trigger_buttons.left = Some(TriggerButton { destination: "KEY_A".into(), threshold });
            assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        }
        let mut axes = receiver_axes();
        axes[0].maximum = 255;
        assert!(Mapper::new_receiver(&HostProfile::default(), axes).is_err());
    }

    fn shift_layer() -> Layer {
        Layer {
            name: "Shift".into(),
            hold: "BTN_TL".into(),
            mappings: BTreeMap::from([("BTN_SOUTH".into(), "KEY_A".into())]),
            ..Layer::default()
        }
    }

    fn key_events(frame: &Frame) -> Vec<(u16, i32)> {
        frame.as_slice().iter().filter(|event| event.kind == 1)
            .map(|event| (event.code, event.value)).collect()
    }

    #[test]
    fn layer_edges_release_old_keys_before_pressing_replacements() {
        let profile = HostProfile {
            mappings: BTreeMap::from([("BTN_SOUTH".into(), "KEY_SPACE".into())]),
            layers: vec![shift_layer()],
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 1)])), [(57, 1)]);
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 1)])), [(57, 0), (30, 1)]);
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 0)])), [(30, 0)]);
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 1)])), [(30, 1)]);
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 0)])), [(30, 0), (57, 1)]);
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 0)])), [(57, 0)]);
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
        assert!(key_events(&mapper.close()).is_empty());
    }

    #[test]
    fn layer_switch_preserves_shared_outputs_and_first_hold_has_priority() {
        let mut layer = shift_layer();
        layer.mappings.insert("BTN_EAST".into(), "KEY_SPACE".into());
        let mut second = shift_layer();
        second.hold = "BTN_TR".into();
        second.mappings.insert("BTN_SOUTH".into(), "KEY_B".into());
        let profile = HostProfile {
            mappings: BTreeMap::from([
                ("BTN_SOUTH".into(), "KEY_SPACE".into()),
                ("BTN_EAST".into(), "KEY_SPACE".into()),
                ("BTN_TL".into(), "KEY_ESC".into()),
            ]),
            layers: vec![layer, second],
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 1), (1, 305, 1)]);
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 1), (1, 311, 1)])), [(30, 1)]);
        let frame = receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 0)]);
        assert_eq!(key_events(&frame), [(30, 0), (57, 0), (48, 1), (305, 1)]);
        let frame = receiver_frame(&mut mapper, Duration::ZERO, &[(1, 311, 0)]);
        assert_eq!(key_events(&frame), [(48, 0), (305, 0), (57, 1)]);
        assert_eq!(key_events(&mapper.close()), [(57, 0)]);
    }

    #[test]
    fn layer_turbo_stops_on_exit_without_affecting_base_passthrough() {
        let mut layer = shift_layer();
        layer.turbo.insert("BTN_SOUTH".into(), 10.0);
        let profile = HostProfile { layers: vec![layer], ..HostProfile::default() };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        assert!(has(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 1), (1, 310, 1)]), 1, 30, 1));
        assert!(has(&mapper.tick(Duration::from_millis(50)), 1, 30, 0));
        assert!(has(&mapper.tick(Duration::from_millis(100)), 1, 30, 1));
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::from_millis(110), &[(1, 310, 0)])), [(30, 0), (304, 1)]);
        assert!(mapper.tick(Duration::from_millis(150)).is_empty());
        assert!(mapper.tick(Duration::from_millis(200)).is_empty());
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::from_millis(210), &[(1, 304, 0)])), [(304, 0)]);
    }

    #[test]
    fn layer_toggle_latches_are_cleared_on_exit_and_not_shared() {
        let mut layer = shift_layer();
        layer.toggle.push("BTN_SOUTH".into());
        let mut second = layer.clone();
        second.hold = "BTN_TR".into();
        second.mappings.insert("BTN_SOUTH".into(), "KEY_B".into());
        let profile = HostProfile { layers: vec![layer, second], ..HostProfile::default() };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 1), (1, 304, 1)]);
        assert!(receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 0)]).is_empty());
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 0), (1, 311, 1)])), [(30, 0)]);
        assert!(receiver_frame(&mut mapper, Duration::ZERO, &[(1, 311, 0), (1, 310, 1)]).is_empty());
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::from_secs(1), &[(1, 304, 1)])), [(30, 1)]);
        assert_eq!(key_events(&mapper.close()), [(30, 0)]);
    }

    #[test]
    fn layer_mouse_and_trigger_overrides_restore_base_on_exit() {
        let profile = HostProfile {
            trigger_buttons: TriggerButtons {
                left: Some(TriggerButton { destination: "KEY_SPACE".into(), threshold: 0.5 }),
                right: None,
            },
            layers: vec![Layer {
                stick_mouse: Some(StickMouse { stick: Stick::Left, sensitivity: 100.0, deadzone: 0.0 }),
                trigger_buttons: TriggerButtons {
                    left: None,
                    right: Some(TriggerButton { destination: "BTN_LEFT".into(), threshold: 0.5 }),
                },
                ..shift_layer()
            }],
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        receiver_frame(&mut mapper, Duration::ZERO, &[(3, 0, 32767), (3, 2, 255), (3, 5, 255)]);
        let entered = receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 1)]);
        assert!(has(&entered, 3, 0, 0));
        assert_eq!(key_events(&entered), [(57, 0), (272, 1)]);
        assert!(has(&mapper.tick(Duration::from_millis(100)), 2, 0, 10));
        let exited = receiver_frame(&mut mapper, Duration::from_millis(100), &[(1, 310, 0)]);
        assert!(has(&exited, 3, 0, 32767));
        assert_eq!(key_events(&exited), [(272, 0), (57, 1)]);
        assert!(mapper.tick(Duration::from_millis(200)).is_empty());
    }

    #[test]
    fn layer_empty_fields_inherit_base_controls() {
        let profile = HostProfile {
            mappings: BTreeMap::from([("BTN_SOUTH".into(), "KEY_SPACE".into())]),
            turbo: BTreeMap::from([("BTN_SOUTH".into(), 10.0)]),
            toggle: vec!["BTN_SOUTH".into()],
            layers: vec![Layer { mappings: BTreeMap::new(), ..shift_layer() }],
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::ZERO, &[(1, 310, 1), (1, 304, 1)])), [(57, 1)]);
        assert!(receiver_frame(&mut mapper, Duration::ZERO, &[(1, 304, 0)]).is_empty());
        assert_eq!(key_events(&mapper.tick(Duration::from_millis(50))), [(57, 0)]);
        assert_eq!(key_events(&mapper.tick(Duration::from_millis(100))), [(57, 1)]);
        assert_eq!(key_events(&receiver_frame(&mut mapper, Duration::from_millis(100), &[(1, 310, 0)])), [(57, 0)]);
    }

    #[test]
    fn layers_validate_sources_aliases_conflicts_and_nested_controls() {
        for hold in ["KEY_SPACE", "R4", "unknown"] {
            let profile = HostProfile { layers: vec![Layer { hold: hold.into(), ..shift_layer() }], ..HostProfile::default() };
            assert!(profile.validate_structure().is_err());
        }
        let profile = HostProfile { layers: vec![shift_layer(), shift_layer()], ..HostProfile::default() };
        assert!(profile.validate_structure().is_err());
        let profile = HostProfile {
            layers: vec![
                Layer { hold: "BTN_SOUTH".into(), mappings: BTreeMap::new(), ..Layer::default() },
                Layer { hold: "BTN_A".into(), ..Layer::default() },
            ],
            ..HostProfile::default()
        };
        assert!(profile.validate_structure().is_err());
        for source in ["BTN_SOUTH", "BTN_A"] {
            let profile = HostProfile { layers: vec![Layer { hold: source.into(), ..shift_layer() }], ..HostProfile::default() };
            assert!(profile.validate_structure().is_err());
        }
        let mut profile = HostProfile { layers: vec![shift_layer()], ..HostProfile::default() };
        profile.layers[0].turbo.insert("BTN_TL".into(), 10.0);
        assert!(profile.validate_structure().is_err());
        profile.layers[0].turbo.clear();
        profile.layers[0].toggle.push("BTN_TL".into());
        assert!(profile.validate_structure().is_err());
        profile.layers[0].toggle.clear();
        profile.layers[0].turbo.insert("BTN_SOUTH".into(), 0.0);
        assert!(profile.validate_structure().is_err());
        profile.layers[0].turbo.clear();
        profile.layers[0].mappings.insert("BTN_EAST".into(), "bad key".into());
        assert!(profile.validate_structure().is_err());
        profile.layers[0].mappings.remove("BTN_EAST");
        profile.layers[0].hold = "BTN_TRIGGER_HAPPY2".into();
        assert!(profile.validate_structure().is_ok());
        assert!(profile.validate_for_link(LinkKind::Receiver).is_ok());
        assert!(profile.validate_for_link(LinkKind::Bluetooth).is_err());
        assert!(serde_json::from_str::<HostProfile>(r#"{"layers":[{"hold":"BTN_TL","typo":true}]}"#).is_err());
        assert!(serde_json::from_str::<HostProfile>(r#"{"gyro_mouse":{"sensitivity":1,"deadzone":0,"invert_x":false,"invert_y":false,"typo":true}}"#).is_err());
        let minimal: HostProfile = serde_json::from_str(r#"{"layers":[{"name":"Shift","hold":"BTN_TL"}]}"#).unwrap();
        minimal.validate_structure().unwrap();
    }

    #[test]
    fn pre_layer_profiles_and_partial_gyro_options_keep_defaults() {
        let profile: HostProfile = serde_json::from_str(r#"{
            "mappings":{"BTN_SOUTH":"KEY_SPACE"},
            "turbo":{"BTN_SOUTH":10},
            "toggle":["BTN_EAST"],
            "stick_mouse":{"stick":"right","sensitivity":1000,"deadzone":0.15},
            "trigger_buttons":{"left":{"destination":"KEY_A","threshold":0.5}}
        }"#).unwrap();
        profile.validate_structure().unwrap();
        assert!(profile.layers.is_empty());
        assert!(profile.gyro_mouse.is_none());
        let profile: HostProfile = serde_json::from_str(r#"{"gyro_mouse":{"sensitivity":2}}"#).unwrap();
        profile.validate_structure().unwrap();
        let mouse = profile.gyro_mouse.unwrap();
        assert_eq!(mouse.sensitivity, 2.0);
        assert_eq!(mouse.deadzone, 0.5);
        assert!(!mouse.invert_x && !mouse.invert_y);
        assert!(serde_json::from_str::<HostProfile>(r#"{"unknown":0}"#).is_err());
    }

    #[test]
    fn bluetooth_layer_hold_is_detected_before_consumption() {
        let profile = HostProfile { layers: vec![shift_layer()], ..HostProfile::default() };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let entered = mapper.input_at(&report(1 | (1 << 6), [0; 3]), Duration::ZERO).unwrap();
        assert_eq!(key_events(&entered), [(30, 1)]);
        let exited = mapper.input_at(&report(1, [0; 3]), Duration::ZERO).unwrap();
        assert_eq!(key_events(&exited), [(30, 0), (304, 1)]);
        assert_eq!(key_events(&mapper.close()), [(304, 0)]);
    }

    #[test]
    fn gyro_mouse_integrates_fractional_pixels_without_right_stick_motion() {
        let profile = HostProfile {
            gyro_mouse: Some(GyroMouse { sensitivity: 0.5, deadzone: 0.0, invert_x: true, invert_y: true }),
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        mapper.physical_input_at(physical(0, [127, 127]), Duration::ZERO);
        let initial = mapper.input_at(&report(0, [0, 32767, 32767]), Duration::ZERO).unwrap();
        assert!(!initial.as_slice().iter().any(|event| event.kind == 2 || (event.kind == 3 && [2, 5].contains(&event.code) && event.value != 0)));
        let mut totals = [0; 2];
        for index in 1..=400 {
            let frame = mapper.tick(Duration::from_micros(index * 250));
            for event in frame.as_slice().iter().filter(|event| event.kind == 2) {
                totals[usize::from(event.code)] += event.value;
            }
        }
        assert_eq!(totals, [100, -100]);
        assert!(mapper.tick(Duration::from_millis(50)).is_empty());
        mapper.input_at(&report(0, [0, -32767, -32767]), Duration::from_millis(100)).unwrap();
        for index in 401..=800 {
            let frame = mapper.tick(Duration::from_micros(index * 250));
            for event in frame.as_slice().iter().filter(|event| event.kind == 2) {
                totals[usize::from(event.code)] += event.value;
            }
        }
        assert_eq!(totals, [0, 0]);
        mapper.pause_motion();
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
        assert_eq!(profile.validate_for_link(LinkKind::Receiver).unwrap(), Some(RECEIVER_NOTICE));
        assert_eq!(profile.validate_for_link(LinkKind::Wired).unwrap(), Some(WIRED_NOTICE));
    }

    #[test]
    fn gyro_mouse_clutch_gates_motion_at_edges_and_clears_fraction() {
        let profile = HostProfile {
            gyro_mouse: Some(GyroMouse { sensitivity: 0.5, deadzone: 0.0, invert_x: true, invert_y: false }),
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        mapper.input_at(&report(0, [0, 0, 32767]), Duration::ZERO).unwrap();
        assert!(mapper.tick(Duration::from_micros(750)).is_empty());
        assert!(mapper.physical_input_at(physical(0x40000000, [127, 127]), Duration::from_micros(750)).as_slice().iter().all(|event| event.kind != 2));
        mapper.input_at(&report(0, [0, 0, 32767]), Duration::from_millis(100)).unwrap();
        assert!(mapper.tick(Duration::from_millis(200)).is_empty());
        mapper.physical_input_at(physical(0, [127, 127]), Duration::from_millis(200));
        assert!(mapper.tick(Duration::from_micros(200250)).is_empty());
        assert!(has(&mapper.tick(Duration::from_millis(201)), 2, 0, 1));
        assert!(has(&mapper.physical_input_at(physical(0x40000000, [255, 127]), Duration::from_millis(202)), 2, 0, 1));
        assert!(mapper.tick(Duration::from_millis(300)).is_empty());
        mapper.close();
        assert!(mapper.tick(Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn gyro_mouse_deadzone_inversion_and_ranges_are_validated() {
        let mut profile = HostProfile {
            gyro_mouse: Some(GyroMouse { sensitivity: 1.0, deadzone: 100.0, invert_x: false, invert_y: false }),
            ..HostProfile::default()
        };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        mapper.input_at(&report(0, [0, 100, 100]), Duration::ZERO).unwrap();
        assert!(mapper.gyro_velocity[0] < 0.0 && mapper.gyro_velocity[0] > -1.0, "tightening reduces slow motion without a hard cutoff");
        mapper.input_at(&report(0, [0, 32767, 32767]), Duration::from_secs(1)).unwrap();
        let frame = mapper.tick(Duration::from_secs(2));
        assert!(has(&frame, 2, 0, -2000));
        assert!(has(&frame, 2, 1, 2000));
        for sensitivity in [0.0, -1.0, 10001.0, f64::NAN, f64::INFINITY] {
            profile.gyro_mouse.as_mut().unwrap().sensitivity = sensitivity;
            assert!(profile.validate_structure().is_err());
            assert!(profile.validate_for_link(LinkKind::Receiver).is_err());
        }
        profile.gyro_mouse.as_mut().unwrap().sensitivity = 1.0;
        for deadzone in [-1.0, 2000.0, f64::NAN, f64::INFINITY] {
            profile.gyro_mouse.as_mut().unwrap().deadzone = deadzone;
            assert!(profile.validate_structure().is_err());
        }
    }

    fn physical(buttons: u32, right: [u8; 2]) -> PhysicalInput {
        PhysicalInput {
            buttons,
            left_x: 128,
            left_y: 127,
            right_x: right[0],
            right_y: right[1],
            left_trigger: 0,
            right_trigger: 0,
        }
    }

    #[test]
    fn bluetooth_trigger_buttons_follow_physical_side() {
        // Live BT HID: byte 6 = Accelerator/ABS_GAS = RT, byte 7 = Brake/ABS_BRAKE = LT.
        let mut profile = HostProfile::default();
        profile.trigger_buttons.right = Some(TriggerButton { destination: "KEY_SPACE".into(), threshold: 0.5 });
        profile.trigger_buttons.left = Some(TriggerButton { destination: "KEY_ENTER".into(), threshold: 0.5 });
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let mut bytes = report(0, [0; 3]);
        bytes[6] = 255;
        assert_eq!(key_events(&mapper.input_at(&bytes, Duration::ZERO).unwrap()), [(57, 1)], "RT → right binding");
        bytes[6] = 0;
        bytes[7] = 255;
        assert_eq!(key_events(&mapper.input_at(&bytes, Duration::ZERO).unwrap()), [(57, 0), (28, 1)], "LT → left binding");
    }

    fn report(buttons: u32, gyro: [i16; 3]) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[..6].copy_from_slice(&[1, 8, 128, 127, 128, 127]);
        bytes[8..11].copy_from_slice(&buttons.to_le_bytes()[..3]);
        for (index, value) in gyro.into_iter().enumerate() {
            bytes[21 + index * 2..23 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn apply(state: &mut BTreeMap<(u16, u16), i32>, frame: Frame) {
        for event in frame.as_slice() {
            if event.kind != 0 {
                state.insert((event.kind, event.code), event.value);
            }
        }
    }

    #[test]
    fn physical_r4_preserves_overlapping_view_and_bypasses_firmware_stick() {
        let profile = HostProfile { gyro: GyroSettings { smooth_threshold: 0.0, stick_smoothing_ms: 0.0, ..GyroSettings::default() }, ..HostProfile::default() };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let mut state = BTreeMap::new();
        apply(&mut state, mapper.physical_input(physical(8, [128, 127])));
        apply(
            &mut state,
            mapper
                .input(&report((1 << 17) | (1 << 10), [0, 1000, 1000]))
                .unwrap(),
        );
        assert!(state[&(3, 2)] < 0);
        assert!(state[&(3, 5)] > 0);
        apply(
            &mut state,
            mapper.physical_input(physical(0x40000008, [255, 0])),
        );
        let mut firmware = report(1 << 10, [0, 30000, 30000]);
        firmware[4] = 0;
        firmware[5] = 255;
        apply(&mut state, mapper.input(&firmware).unwrap());
        assert_eq!(state[&(3, 2)], 32767);
        assert_eq!(state[&(3, 5)], -32768);
        assert_eq!(state[&(1, 314)], 1);
        assert!(!state.contains_key(&(1, 705)));
        apply(&mut state, mapper.physical_input(physical(8, [128, 127])));
        apply(
            &mut state,
            mapper.input(&report(1 << 10, [0, 1000, 1000])).unwrap(),
        );
        assert!(state[&(3, 2)] < 0);
        assert!(state[&(3, 5)] > 0);
        assert_eq!(state[&(1, 314)], 1);
        apply(&mut state, mapper.physical_input(physical(0, [127, 127])));
        apply(&mut state, mapper.input(&report(0, [0; 3])).unwrap());
        assert_eq!(state[&(1, 314)], 0);
        assert_eq!(state[&(3, 2)], 0);
        assert_eq!(state[&(3, 5)], 0);
    }

    #[test]
    fn motion_stall_preserves_stick_and_buttons_then_recovers() {
        let mut mapper = Mapper::new(&HostProfile::default(), default_axes()).unwrap();
        let mut state = BTreeMap::new();
        apply(&mut state, mapper.physical_input(physical(8, [128, 255])));
        apply(
            &mut state,
            mapper.input(&report(1 << 10, [0, 0, 2000])).unwrap(),
        );
        assert!(state[&(3, 2)] < 0);
        apply(&mut state, mapper.pause_motion());
        assert_eq!(state[&(3, 2)], 0);
        assert_eq!(state[&(3, 5)], 32767);
        assert_eq!(state[&(1, 314)], 1);
        apply(
            &mut state,
            mapper.physical_input(physical(0x40000008, [128, 255])),
        );
        apply(&mut state, mapper.physical_input(physical(8, [128, 255])));
        assert_eq!(state[&(3, 2)], 0);
        apply(
            &mut state,
            mapper.input(&report(1 << 10, [0, 0, -2000])).unwrap(),
        );
        assert!(state[&(3, 2)] > 0);
        assert_eq!(state[&(3, 5)], 32767);
        apply(&mut state, mapper.close());
        assert_eq!(state[&(3, 2)], 0);
        assert_eq!(state[&(3, 5)], 0);
        assert_eq!(state[&(1, 314)], 0);
    }

    #[test]
    fn saturated_mixing_releases_without_latching() {
        let profile = HostProfile { gyro: GyroSettings { smooth_threshold: 0.0, stick_smoothing_ms: 0.0, ..GyroSettings::default() }, ..HostProfile::default() };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let mut state = BTreeMap::new();
        apply(&mut state, mapper.physical_input(physical(0, [255, 0])));
        apply(
            &mut state,
            mapper.input(&report(0, [0, -32768, -32768])).unwrap(),
        );
        assert_eq!(state[&(3, 2)], 32767);
        assert_eq!(state[&(3, 5)], -32768);
        apply(&mut state, mapper.input(&report(0, [0; 3])).unwrap());
        assert_eq!(state[&(3, 2)], 32767);
        assert_eq!(state[&(3, 5)], -32768);
        apply(&mut state, mapper.physical_input(physical(0, [127, 127])));
        assert_eq!(state[&(3, 2)], 0);
        assert_eq!(state[&(3, 5)], 0);
    }

    #[test]
    fn overlapping_mappings_retain_destination_until_last_release() {
        for destination in ["BTN_SELECT", "KEY_SPACE", "BTN_LEFT"] {
            let mut profile = HostProfile::default();
            profile
                .mappings
                .insert("BTN_SOUTH".into(), destination.into());
            profile
                .mappings
                .insert("BTN_EAST".into(), destination.into());
            let code = key_code(destination).unwrap();
            let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
            let mut state = BTreeMap::new();
            apply(&mut state, mapper.input(&report(1, [0; 3])).unwrap());
            assert_eq!(state[&(1, code)], 1);
            let frame = mapper.input(&report(3, [0; 3])).unwrap();
            assert!(!frame.as_slice().iter().any(|event| event.kind == 1));
            apply(&mut state, frame);
            let frame = mapper.input(&report(2, [0; 3])).unwrap();
            assert!(!frame.as_slice().iter().any(|event| event.kind == 1));
            apply(&mut state, frame);
            assert_eq!(state[&(1, code)], 1);
            apply(&mut state, mapper.input(&report(0, [0; 3])).unwrap());
            assert_eq!(state[&(1, code)], 0);
            apply(&mut state, mapper.input(&report(3, [0; 3])).unwrap());
            let closed = mapper.close();
            assert_eq!(
                closed
                    .as_slice()
                    .iter()
                    .filter(|event| event.kind == 1 && event.code == code && event.value == 0)
                    .count(),
                1
            );
            apply(&mut state, closed);
            assert_eq!(state[&(1, code)], 0);
            assert!(mapper.close().is_empty());
            assert!(mapper.input(&report(1, [0; 3])).is_err());
        }
    }

    #[test]
    fn mapped_and_passthrough_sources_share_ownership() {
        let mut profile = HostProfile::default();
        profile
            .mappings
            .insert("BTN_SOUTH".into(), "BTN_SELECT".into());
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let mut state = BTreeMap::new();
        for buttons in [1, 1 | (1 << 10), 1 << 10] {
            apply(&mut state, mapper.input(&report(buttons, [0; 3])).unwrap());
            assert_eq!(state[&(1, 314)], 1);
        }
        apply(&mut state, mapper.input(&report(0, [0; 3])).unwrap());
        assert_eq!(state[&(1, 314)], 0);
    }

    #[test]
    fn each_clutch_uses_physical_mask_and_consumes_only_its_hid_slot() {
        for button in ["R4", "L4", "PL", "PR"] {
            let profile = HostProfile {
                button: button.into(),
                ..HostProfile::default()
            };
            let (code, mask) = clutch_button(button).unwrap();
            let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
            let mut state = BTreeMap::new();
            apply(
                &mut state,
                mapper.physical_input(physical(mask, [127, 127])),
            );
            apply(
                &mut state,
                mapper.input(&report(0xffffff, [0, 1000, 1000])).unwrap(),
            );
            assert_eq!(state[&(3, 2)], 0);
            assert_eq!(state[&(3, 5)], 0);
            assert!(!state.contains_key(&(1, code)));
            apply(&mut state, mapper.physical_input(physical(0, [127, 127])));
            assert!(state[&(3, 2)] < 0);
            assert!(state[&(3, 5)] > 0);
        }
    }

    #[test]
    fn gyro_tightening_inversion_and_hat_are_real_transforms() {
        let profile = HostProfile { sensitivity: 2.0, deadzone: 10.0, invert_x: true, invert_y: true,
            gyro: GyroSettings { smooth_threshold: 0.0, stick_deadzone: 0.0, stick_smoothing_ms: 0.0, ..GyroSettings::default() },
            ..HostProfile::default() };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let mut bytes = report(0, [0, 100, 100]);
        bytes[1] = 3;
        mapper.input_at(&bytes, Duration::ZERO).unwrap();
        let speed = (100.0 * 2000.0 / 32767.0_f64) * 2.0_f64.sqrt();
        let expected = (100.0 * 2000.0 / 32767.0 * (speed / 10.0) * 2.0 * 32767.0 / 180.0).round_ties_even() as i32;
        assert_eq!(mapper.values[2], Some(expected));
        assert_eq!(mapper.values[3], Some(-expected));
        assert_eq!(mapper.values[6..8], [Some(1), Some(1)]);
        mapper.input_at(&report(0, [0, 1000, 1000]), Duration::from_millis(4)).unwrap();
        let expected = (1000.0 * 2000.0 / 32767.0 * 2.0 * 32767.0 / 180.0_f64).round_ties_even() as i32;
        assert_eq!(mapper.values[2], Some(expected));
        assert_eq!(mapper.values[3], Some(-expected));
        assert_eq!(mapper.values[6..8], [Some(0), Some(0)]);
    }

    #[test]
    fn invalid_settings_reports_and_axis_bounds_are_rejected() {
        for value in [f64::NAN, f64::INFINITY, -1.0, 0.0, 100.1] {
            assert!(
                HostProfile {
                    sensitivity: value,
                    ..HostProfile::default()
                }
                .validate()
                .is_err()
            );
        }
        for value in [f64::NAN, f64::INFINITY, -1.0, 2000.0] {
            assert!(
                HostProfile {
                    deadzone: value,
                    ..HostProfile::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            HostProfile {
                button: "View".into(),
                ..HostProfile::default()
            }
            .validate()
            .is_err()
        );
        for (source, destination) in [
            ("KEY_A", "KEY_SPACE"),
            ("BTN_SOUTH", "KEY_UNKNOWN"),
            ("BTN_TRIGGER_HAPPY2", "KEY_A"),
        ] {
            let mut profile = HostProfile::default();
            profile.mappings.insert(source.into(), destination.into());
            assert!(profile.validate().is_err());
        }
        let mut profile = HostProfile::default();
        profile.mappings.insert("BTN_SOUTH".into(), "KEY_A".into());
        profile.mappings.insert("BTN_A".into(), "KEY_B".into());
        assert!(profile.validate().is_err());
        let mut axes = default_axes();
        axes[2].flat = 127;
        assert!(Mapper::new(&HostProfile::default(), axes).is_err());
        axes = default_axes();
        axes[0].maximum = 32767;
        assert!(Mapper::new(&HostProfile::default(), axes).is_err());
        axes = default_axes();
        axes[6].minimum = 0;
        assert!(Mapper::new(&HostProfile::default(), axes).is_err());
        let mut mapper = Mapper::new(&HostProfile::default(), default_axes()).unwrap();
        assert!(mapper.input(&[]).is_err());
        assert!(mapper.input(&[1; 31]).is_err());
        assert!(mapper.input(&[2; 32]).is_err());
        assert!(mapper.input(&[1; 33]).is_err());
    }

    #[test]
    fn shutdown_neutralizes_all_axes_keyboard_and_mouse() {
        let mut profile = HostProfile::default();
        profile
            .mappings
            .insert("BTN_SOUTH".into(), "KEY_LEFTCTRL".into());
        profile
            .mappings
            .insert("BTN_EAST".into(), "BTN_LEFT".into());
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let mut state = BTreeMap::new();
        apply(&mut state, mapper.physical_input(physical(0x3000, [255, 0])));
        let mut bytes = report(3, [0; 3]);
        bytes[1] = 3;
        bytes[2] = 255;
        bytes[3] = 0;
        bytes[6] = 255;
        bytes[7] = 255;
        apply(&mut state, mapper.input(&bytes).unwrap());
        assert_eq!(state[&(1, 29)], 1);
        assert_eq!(state[&(1, 272)], 1);
        apply(&mut state, mapper.close());
        for (index, &code) in AXES.iter().enumerate() {
            assert_eq!(state[&(3, code)], neutral(index, default_axes()[index]));
        }
        assert_eq!(state[&(1, 29)], 0);
        assert_eq!(state[&(1, 272)], 0);
        assert!(mapper.physical_input(physical(0, [0, 255])).is_empty());
        assert!(mapper.pause_motion().is_empty());
    }

    #[test]
    fn each_destination_routes_to_one_real_device_class() {
        for &(_, code) in DESTINATION_KEYS {
            let event = Event {
                kind: 1,
                code,
                value: 1,
            };
            let matches = [OutputKind::Gamepad, OutputKind::Keyboard, OutputKind::Mouse]
                .into_iter()
                .filter(|kind| kind.accepts(event))
                .count();
            assert_eq!(matches, 1, "destination {code}");
        }
        assert!(OutputKind::Keyboard.accepts(Event {
            kind: 1,
            code: 57,
            value: 0
        }));
        assert!(OutputKind::Mouse.accepts(Event {
            kind: 1,
            code: 272,
            value: 0
        }));
        assert!(OutputKind::Gamepad.accepts(Event {
            kind: 3,
            code: 2,
            value: 0
        }));
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn linux_ioctl_layouts_match_x86_64_uapi() {
        assert_eq!(size_of::<InputId>(), 8);
        assert_eq!(size_of::<HidRawInfo>(), 8);
        assert_eq!(size_of::<AxisInfo>(), 24);
        assert_eq!(size_of::<RawEvent>(), 24);
        assert_eq!(size_of::<UInputSetup>(), 92);
        assert_eq!(size_of::<UInputAbsSetup>(), 28);
        assert_eq!(ioctl_code(2, b'H', 3, size_of::<HidRawInfo>()), 0x80084803);
        assert_eq!(ioctl_code(1, b'E', 0x90, size_of::<i32>()), 0x40044590);
        assert_eq!(ioctl_code(1, b'U', 3, size_of::<UInputSetup>()), 0x405c5503);
        assert_eq!(
            ioctl_code(1, b'U', 4, size_of::<UInputAbsSetup>()),
            0x401c5504
        );
    }

    #[test]
    fn maximum_frame_is_bounded_and_serializes_only_active_events() {
        // An unchanged input emits no duplicate events once the output is steady.
        // Disable the intentionally evolving stick smoother for this frame-bound test.
        let profile = HostProfile { gyro: GyroSettings { stick_smoothing_ms: 0.0, ..GyroSettings::default() }, ..HostProfile::default() };
        let mut mapper = Mapper::new(&profile, default_axes()).unwrap();
        let frame = mapper.input(&report(0xffffff, [0, 32767, 32767])).unwrap();
        assert!(frame.len() <= MAX_EVENTS);
        assert_eq!(
            frame.as_slice().last(),
            Some(&Event {
                kind: 0,
                code: 0,
                value: 0
            })
        );
        let json = serde_json::to_value(&frame).unwrap();
        assert_eq!(json.as_array().unwrap().len(), frame.len());
        assert!(
            mapper
                .input(&report(0xffffff, [0, 32767, 32767]))
                .unwrap()
                .is_empty()
        );
        assert!(mapper.close().len() <= MAX_EVENTS);
    }
}
