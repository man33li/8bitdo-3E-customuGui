//! Windows HID input and ViGEm output, with scoped HidHide ownership.
use super::{DeviceInfo, HostProfile};
use crate::{Result, invalid, protocol::PhysicalInput};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool, mpsc::Receiver},
};

mod raw;
mod inventory;
mod output;
mod hide;

pub fn recover_owned_output() -> Result<()> { hide::recover_pending() }

fn receiver_axes() -> [super::AxisInfo; 8] {
    let stick = super::AxisInfo { value: 0, minimum: -32768, maximum: 32767, ..Default::default() };
    let trigger = super::AxisInfo { value: 0, minimum: 0, maximum: 255, ..Default::default() };
    let hat = super::AxisInfo { value: 0, minimum: -1, maximum: 1, ..Default::default() };
    [stick, stick, stick, stick, trigger, trigger, hat, hat]
}

fn receiver_pad(mapper: &mut super::Mapper, pad: crate::receiver::Pad, now: std::time::Duration) -> Result<super::Frame> {
    use super::Event;
    for (mask, code) in [(0x1000,304), (0x2000,305), (0x4000,307), (0x8000,308),
        (0x0100,310), (0x0200,311), (0x0020,314), (0x0010,315), (0x0400,316), (0x0040,317), (0x0080,318)] {
        mapper.receiver_event(Event { kind: 1, code, value: i32::from(pad.buttons & mask != 0) }, now)?;
    }
    let invert_y = |value: i16| (-i32::from(value)).clamp(-32768, 32767);
    for (code, value) in [(0, i32::from(pad.left_x)), (1, invert_y(pad.left_y)),
        (3, i32::from(pad.right_x)), (4, invert_y(pad.right_y)),
        (2, i32::from(pad.left_trigger)), (5, i32::from(pad.right_trigger)),
        (16, i32::from(pad.buttons & 0x0008 != 0) - i32::from(pad.buttons & 0x0004 != 0)),
        (17, i32::from(pad.buttons & 0x0002 != 0) - i32::from(pad.buttons & 0x0001 != 0))] {
        mapper.receiver_event(Event { kind: 3, code, value }, now)?;
    }
    mapper.receiver_event(Event::default(), now)
}

/// Xbox 360 button codes ViGEm can represent.
const XBOX: [u16; 11] = [304, 305, 307, 308, 310, 311, 314, 315, 316, 317, 318];
/// No receiver heartbeat is sent on Windows (its effect there is unverified), so firmware keeps
/// applying the slot mapping; Mapper::new_receiver without a slot map leaves those buttons as-is.
pub(super) const USB_NOTICE: &str = "Windows PC-mode USB: the controller slot's own button mapping stays active and host mappings apply on top of it. L4/R4/PL/PR reach the host only as the buttons the slot maps them to; host gyro needs the native-stick clutch.";

pub(super) fn validate_profile(profile: &HostProfile, link: super::LinkKind) -> Result<Option<&'static str>> {
    if profile.receiver_gyro {
        return Err(invalid("Windows does not support firmware mouse gyro; use native right-stick gyro"));
    }
    if profile.steam {
        return Err(invalid("Steam Input output (virtual DualSense Edge) is Linux-only"));
    }
    for destination in profile.mappings.values()
        .chain([&profile.trigger_buttons.left, &profile.trigger_buttons.right].into_iter().flatten().map(|trigger| &trigger.destination))
        .chain(profile.layers.iter().flat_map(|layer| layer.mappings.values().chain(
            [&layer.trigger_buttons.left, &layer.trigger_buttons.right].into_iter().flatten().map(|trigger| &trigger.destination))))
    {
        let code = super::key_code(destination)?;
        if !XBOX.contains(&code) && output::target(code).is_none() {
            return Err(invalid(format!("Windows output cannot represent destination {destination}")));
        }
    }
    let check_controls = |mappings: &std::collections::BTreeMap<String, String>,
        turbo: &std::collections::BTreeMap<String, f64>, toggle: &[String]| -> Result<()> {
        for source in turbo.keys().chain(toggle.iter()) {
            let code = super::key_code(source)?;
            if !XBOX.contains(&code) && !mappings.keys().any(|mapped| super::key_code(mapped).ok() == Some(code)) {
                return Err(invalid(format!("Windows output requires a mapping for turbo/toggle source {source}")));
            }
        }
        Ok(())
    };
    check_controls(&profile.mappings, &profile.turbo, &profile.toggle)?;
    for layer in &profile.layers {
        check_controls(
            if layer.mappings.is_empty() { &profile.mappings } else { &layer.mappings },
            if layer.turbo.is_empty() { &profile.turbo } else { &layer.turbo },
            if layer.toggle.is_empty() { &profile.toggle } else { &layer.toggle },
        )?;
    }
    if link == super::LinkKind::Wired {
        return Err(invalid("Windows wired GIP input is not supported; select PC-mode USB or Bluetooth"));
    }
    Ok((link == super::LinkKind::Receiver && !profile.native_gyro_clutch).then_some(USB_NOTICE))
}

pub fn devices() -> Result<Vec<DeviceInfo>> {
    Ok(inventory::discover()?.into_iter().map(|source| {
        let bluetooth = source.kind == inventory::Kind::Bluetooth;
        DeviceInfo {
            path: source.path.clone(), name: "8BitDo Ultimate 3E".into(),
            bus: if bluetooth { 5 } else { 3 }, vendor: 0x2dc8,
            product: if bluetooth { 0x202f } else { 0x3024 },
            physical: source.instance_id.clone(), identity: source.instance_id,
            hidraw: Some(source.path), input_access: source.input_access, hidraw_access: source.input_access,
            link: if bluetooth { super::LinkKind::Bluetooth } else { super::LinkKind::Receiver },
        }
    }).collect())
}

fn select(device: Option<&Path>, kind: inventory::Kind) -> Result<inventory::Source> {
    let mut found = inventory::discover()?.into_iter().filter(|source| source.kind == kind
        && device.is_none_or(|path| path.as_os_str().to_string_lossy().eq_ignore_ascii_case(&source.path.as_os_str().to_string_lossy())));
    let source = found.next().ok_or_else(|| invalid("No matching 8BitDo Windows input device"))?;
    if found.next().is_some() { return Err(invalid("Multiple matching 8BitDo input devices; select one explicitly")); }
    Ok(source)
}

fn first_physical(physical: &Receiver<PhysicalInput>, stopped: &AtomicBool) -> Result<Option<PhysicalInput>> {
    loop {
        if stopped.load(std::sync::atomic::Ordering::Acquire) || crate::interrupted() { return Ok(None); }
        match physical.recv_timeout(std::time::Duration::from_millis(20)) {
            Ok(state) => return Ok(Some(state)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Err(invalid("Configuration BLE disconnected before first physical report")),
        }
    }
}

fn newest_physical(physical: &Receiver<PhysicalInput>, mut state: PhysicalInput) -> Result<PhysicalInput> {
    loop {
        match physical.try_recv() {
            Ok(next) => state = next,
            Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(state),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err(invalid("Configuration BLE physical-report connection disconnected")),
        }
    }
}

fn drain_physical(mapper: &mut super::Mapper, output: &mut output::VirtualPad,
    physical: &Receiver<PhysicalInput>, native: bool) -> Result<()> {
    loop {
        match physical.try_recv() {
            Ok(state) => {
                let now = mapper.origin.elapsed();
                let frame = if native { mapper.native_physical_at(state, now)? } else { mapper.physical_input_at(state, now) };
                output.send(&frame)?;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err(invalid("Configuration BLE physical-report connection disconnected")),
        }
    }
}

pub fn run(device: Option<&Path>, profile: &HostProfile, slot_map: Option<crate::profile::KeyMap>,
    physical: Receiver<PhysicalInput>, stopped: Arc<AtomicBool>) -> Result<()> {
    use std::{sync::atomic::Ordering, time::{Duration, Instant}};
    profile.validate_runtime(super::LinkKind::Bluetooth)?;
    hide::check_available()?;
    let source = select(device, inventory::Kind::Bluetooth)?;
    let Some(first) = first_physical(&physical, &stopped)? else { return Ok(()); };
    let mut raw = raw::Raw::open(&source.path)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut report = [0; 32];
    loop {
        if stopped.load(Ordering::Acquire) || crate::interrupted() { return Ok(()); }
        if Instant::now() >= deadline { return Err(invalid("No Bluetooth gaming HID report received within two seconds")); }
        if raw.read(Duration::from_millis(5), &mut report)?.is_some() && report[0] == 1 { break; }
    }
    let ids = inventory::safe_hide_ids(&source)?;
    let first = newest_physical(&physical, first)?;
    let mut mapper = super::Mapper::new(profile, super::default_axes())?;
    mapper.set_key_map(slot_map);
    let mut output = output::VirtualPad::create(super::LinkKind::Bluetooth)?;
    if stopped.load(Ordering::Acquire) || crate::interrupted() { return Ok(()); }
    let guard = hide::acquire(&ids)?;
    let result = (|| {
        output.send(&mapper.physical_input_at(first, mapper.origin.elapsed()))?;
        drain_physical(&mut mapper, &mut output, &physical, false)?;
        let frame = mapper.input_at(&report, mapper.origin.elapsed())?;
        output.send(&frame)?;
        let mut motion_deadline = Instant::now() + super::MOTION_TIMEOUT;
        while !stopped.load(Ordering::Acquire) && !crate::interrupted() {
            drain_physical(&mut mapper, &mut output, &physical, false)?;
            if raw.read(Duration::from_millis(5), &mut report)?.is_some() {
                drain_physical(&mut mapper, &mut output, &physical, false)?;
                if report[0] == 1 {
                    output.send(&mapper.input_at(&report, mapper.origin.elapsed())?)?;
                    motion_deadline = Instant::now() + super::MOTION_TIMEOUT;
                }
            }
            if Instant::now() >= motion_deadline { output.send(&mapper.pause_motion())?; }
            output.send(&mapper.tick(mapper.origin.elapsed()))?;
        }
        Ok(())
    })();
    finish(result, output, guard)
}

/// Release virtual output before restoring HidHide, so games never see both pads.
fn finish(result: Result<()>, output: output::VirtualPad, mut guard: hide::Guard) -> Result<()> {
    drop(output);
    let cleanup = guard.cleanup();
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(invalid(format!("{error}; HidHide cleanup failed: {cleanup}"))),
        (Err(error), _) => Err(error),
        (_, cleanup) => cleanup,
    }
}

fn first_pad(receiver: &mut crate::receiver::Receiver, stopped: &AtomicBool) -> Result<Option<crate::receiver::Pad>> {
    use std::{sync::atomic::Ordering, time::{Duration, Instant}};
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if stopped.load(Ordering::Acquire) || crate::interrupted() { return Ok(None); }
        if Instant::now() >= deadline { return Err(invalid("No PC-mode receiver input report received within two seconds")); }
        if let Some(pad) = receiver.input(Duration::from_millis(5))?.0 { return Ok(Some(pad)); }
    }
}

pub fn run_native_clutch(device: Option<&Path>, profile: &HostProfile,
    physical: Receiver<PhysicalInput>, stopped: Arc<AtomicBool>) -> Result<()> {
    use std::{sync::atomic::Ordering, time::Duration};
    profile.validate_runtime(super::LinkKind::Receiver)?;
    if !profile.native_gyro_clutch { return Err(invalid("Native gyro clutch is not enabled")); }
    hide::check_available()?;
    let source = select(device, inventory::Kind::Receiver)?;
    let Some(first) = first_physical(&physical, &stopped)? else { return Ok(()); };
    let mut receiver = crate::receiver::Receiver::open(&source.path)?;
    let Some(first_pad) = first_pad(&mut receiver, &stopped)? else { return Ok(()); };
    let ids = inventory::safe_hide_ids(&source)?;
    let first = newest_physical(&physical, first)?;
    let mut mapper = super::Mapper::new_receiver(profile, receiver_axes())?;
    let mut output = output::VirtualPad::create(super::LinkKind::Receiver)?;
    if stopped.load(Ordering::Acquire) || crate::interrupted() { return Ok(()); }
    let guard = hide::acquire(&ids)?;
    let result = (|| {
        output.send(&mapper.native_physical_at(first, mapper.origin.elapsed())?)?;
        drain_physical(&mut mapper, &mut output, &physical, true)?;
        let now = mapper.origin.elapsed();
        output.send(&receiver_pad(&mut mapper, first_pad, now)?)?;
        while !stopped.load(Ordering::Acquire) && !crate::interrupted() {
            drain_physical(&mut mapper, &mut output, &physical, true)?;
            let pad = receiver.input(Duration::from_millis(5))?.0;
            drain_physical(&mut mapper, &mut output, &physical, true)?;
            if let Some(pad) = pad {
                let now = mapper.origin.elapsed();
                output.send(&receiver_pad(&mut mapper, pad, now)?)?;
            }
            output.send(&mapper.tick(mapper.origin.elapsed()))?;
        }
        Ok(())
    })();
    finish(result, output, guard)
}

/// PC-mode USB (cable or 2.4 GHz receiver) host remapping without configuration BLE.
pub fn run_receiver(device: Option<&Path>, profile: &HostProfile,
    _slot_map: Option<crate::profile::KeyMap>, stopped: Arc<AtomicBool>) -> Result<()> {
    use std::{sync::atomic::Ordering, time::Duration};
    profile.validate_runtime(super::LinkKind::Receiver)?;
    if profile.native_gyro_clutch { return Err(invalid("Native gyro clutch requires configuration BLE")); }
    hide::check_available()?;
    let source = select(device, inventory::Kind::Receiver)?;
    let mut receiver = crate::receiver::Receiver::open(&source.path)?;
    let Some(first_pad) = first_pad(&mut receiver, &stopped)? else { return Ok(()); };
    let ids = inventory::safe_hide_ids(&source)?;
    // No slot map: see USB_NOTICE; firmware already applied it to these reports.
    let mut mapper = super::Mapper::new_receiver(profile, receiver_axes())?;
    let mut output = output::VirtualPad::create(super::LinkKind::Receiver)?;
    if stopped.load(Ordering::Acquire) || crate::interrupted() { return Ok(()); }
    let guard = hide::acquire(&ids)?;
    let result = (|| {
        let now = mapper.origin.elapsed();
        output.send(&receiver_pad(&mut mapper, first_pad, now)?)?;
        while !stopped.load(Ordering::Acquire) && !crate::interrupted() {
            if let Some(pad) = receiver.input(Duration::from_millis(5))?.0 {
                let now = mapper.origin.elapsed();
                output.send(&receiver_pad(&mut mapper, pad, now)?)?;
            }
            output.send(&mapper.tick(mapper.origin.elapsed()))?;
        }
        Ok(())
    })();
    finish(result, output, guard)
}

pub fn run_wired(
    _device: Option<&Path>,
    _profile: &HostProfile,
    _stopped: Arc<AtomicBool>,
) -> Result<()> {
    Err(invalid("Wired input mapping not yet supported on Windows"))
}

pub mod rumble {
    use crate::{Result, invalid};
    use std::{path::{Path, PathBuf}, time::Duration};

    pub fn devices() -> Result<Vec<PathBuf>> {
        Err(invalid("Rumble device discovery not yet supported on Windows"))
    }

    pub struct Effect {
        _private: (),
    }

    impl Effect {
        pub fn play(_path: &Path, _strong: u16, _weak: u16, _duration: Duration) -> Result<Self> {
            Err(invalid("Rumble playback not yet supported on Windows"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_pad_converts_xinput_axes_and_buttons() {
        let mut profile = HostProfile::default();
        profile.native_gyro_clutch = true;
        let mut mapper = super::super::Mapper::new_receiver(&profile, receiver_axes()).unwrap();
        let frame = receiver_pad(&mut mapper, crate::receiver::Pad {
            buttons: 0x1000 | 0x0001, left_y: 32767, right_y: -32768,
            ..Default::default()
        }, std::time::Duration::ZERO).unwrap();
        let has = |kind, code, value| frame.as_slice().iter().any(|event| (event.kind, event.code, event.value) == (kind, code, value));
        assert!(has(1, 304, 1));
        assert!(has(3, 1, -32767));
        assert!(has(3, 4, 32767));
        assert!(has(3, 17, -1));
    }

    #[test]
    fn windows_accepts_xbox_and_desktop_destinations_only() {
        use super::super::LinkKind::{Bluetooth, Receiver, Wired};
        let mut profile = HostProfile::default();
        profile.mappings.insert("BTN_SOUTH".into(), "KEY_A".into());
        profile.trigger_buttons.right = Some(super::super::TriggerButton { destination: "BTN_LEFT".into(), threshold: 0.5 });
        profile.stick_mouse = Some(super::super::StickMouse { stick: super::super::Stick::Right, sensitivity: 1000.0, deadzone: 0.15 });
        assert_eq!(validate_profile(&profile, Bluetooth).unwrap(), None);
        assert_eq!(validate_profile(&profile, Receiver).unwrap(), Some(USB_NOTICE));
        assert!(validate_profile(&profile, Wired).is_err());
        for unrepresentable in ["BTN_TRIGGER_HAPPY3", "KEY_LINEFEED", "BTN_FORWARD"] {
            let mut bad = profile.clone();
            bad.mappings.insert("BTN_EAST".into(), unrepresentable.into());
            assert!(validate_profile(&bad, Bluetooth).is_err(), "{unrepresentable}");
        }
        profile.turbo.insert("BTN_TRIGGER_HAPPY1".into(), 10.0);
        assert!(validate_profile(&profile, Bluetooth).is_err(), "an unmapped paddle has no Windows output");
        profile.mappings.insert("BTN_TRIGGER_HAPPY1".into(), "KEY_SPACE".into());
        assert!(validate_profile(&profile, Bluetooth).is_ok());
        profile.steam = true;
        assert!(validate_profile(&profile, Bluetooth).is_err());
    }
}
