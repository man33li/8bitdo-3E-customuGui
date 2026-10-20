//! Native Clay + raylib UI. `ultimate-control gui` starts the configuration worker;
//! only `gui --demo` manufactures profiles and performs no device I/O.
//! Receiver configuration takes precedence over one wired 2dc8:202f device
//! (including an Xbox-mode receiver); Bluetooth connection remains explicit.
//! Wired reads/saves briefly pause gamepad input and refuse an active host mapper.
//! Each wired transaction releases USB ownership and checks xone driver handback.
//! Test observes wired xone evdev read-only/nonblocking only while visible and
//! the host mapper is stopped; reads/saves close that reader before takeover.
//! Bluetooth Test uses change-driven configuration physical reports, including paddles.
//! Receiver Test and mapper share one 400-ms heartbeat schedule for physical paddles;
//! configuration transactions suppress it across every open.
//!
//! Pages: Home, Mapping, Sticks, Triggers, Vibration, Lighting (alias RGB),
//! Motion, Profiles, Settings, Test, Remap. `--page NAME --screenshot /absolute/file.png`
//! waits for live profiles, refuses overwrite, captures after ten settled frames.
//! `--bench` reports layout/render timings and changed-pad worker-receipt→swap
//! latency (not device latency). The worker coalesces input in one latest-only
//! mailbox; idle pages block on native events/worker notification (250 ms cap).
//! `--debug-pointer` overlays a passthrough crosshair at Clay's pointer and native
//! GLFW / raylib-scaled coordinates, window/framebuffer sizes, and current DPI.
//! Native metrics are re-queried each frame, including mixed-DPI monitor moves.
//! Tab/arrows and controller D-pad navigate; Enter/A activates; left/right
//! adjust; Escape/B closes dialogs.
//! Test/active drags use a 1 ms input cadence; other pad-visible pages wake on
//! changed receiver input rather than polling at 1000 Hz.
//!
//! All profile/header edits are staged in the lossless typed codec. Save writes
//! all three slots with repaired CRC, then requires an identical 868-byte
//! read-back. Unchanged images are not written; success reports changed byte
//! counts and offsets against the loaded baseline. Revert re-reads the controller.
//! Vibration levels edit persistent profile fields when loaded; an independent
//! 600 ms grip test appears only for kernel FF_RUMBLE-capable controller nodes.
//! FF_RUMBLE cannot independently address trigger motors.
//! Home starts/stops the host mapper; Remap edits its separate persisted JSON.
//! Receiver mapping uses xpad; Bluetooth uses the existing BLE/gyro runtime.
//! Bytes beyond the receiver pad payload are ignored, never battery/status data.
//! Bluetooth profile writes, calibration, firmware flashing and full-device
//! reset are not advertised as implemented transports.
#[allow(non_camel_case_types,non_snake_case,non_upper_case_globals,dead_code,clippy::all)]
pub(super) mod ffi { include!(concat!(env!("OUT_DIR"),"/gui_ffi.rs")); }
pub mod clay;
pub mod state;
pub mod widgets;
mod coordinates;
pub(crate) mod test_input;
pub(crate) mod app;
mod assets;
mod guide;
pub use app::{Options,run};

/// Thread-safe, coalesced notification; safe before window creation/after close.
pub(crate) fn wake() { unsafe { ffi::Ultimate_WakeEvents(); } }
