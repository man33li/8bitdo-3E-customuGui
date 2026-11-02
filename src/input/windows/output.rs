use super::super::{Event, Frame, LinkKind, AXES, MAX_EVENTS, RECEIVER_AXES};
use crate::{Result, invalid};
use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};
use ::windows::Win32::UI::Input::KeyboardAndMouse::*;

pub(super) struct VirtualPad {
    target: Option<Xbox360Wired<Client>>,
    state: XGamepad,
    link: LinkKind,
    desktop: Desktop,
}

/// Windows input for a Linux key/mouse destination code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    /// PC set-1 scan code and its E0 (extended) prefix; games read scan codes.
    Scan(u16, bool),
    /// Keys without a plain set-1 scan code: media, Pause, F13-F24.
    Virtual(u16),
    /// Down flags, up flags and XBUTTON data.
    Mouse(MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS, u32),
}

/// None means a gamepad code, or a key Windows output cannot represent.
pub(super) fn target(code: u16) -> Option<Target> {
    use Target::*;
    Some(match code {
        // Linux KEY_ESC..KEY_KPDOT, KEY_102ND, KEY_F11/F12 equal their set-1 scan codes.
        1..=83 | 86..=88 => Scan(code, false),
        96 => Scan(0x1c, true), 97 => Scan(0x1d, true), 98 => Scan(0x35, true),
        99 => Scan(0x37, true), 100 => Scan(0x38, true),
        102..=111 => Scan([0x47, 0x48, 0x49, 0x4b, 0x4d, 0x4f, 0x50, 0x51, 0x52, 0x53][usize::from(code - 102)], true),
        125..=127 => Scan(0x5b + (code - 125), true),
        113 => Virtual(0xad), 114 => Virtual(0xae), 115 => Virtual(0xaf), 119 => Virtual(0x13),
        163 => Virtual(0xb0), 164 => Virtual(0xb3), 165 => Virtual(0xb1), 166 => Virtual(0xb2),
        183..=194 => Virtual(0x7c + (code - 183)),
        272 => Mouse(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
        273 => Mouse(MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
        274 => Mouse(MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
        // BTN_SIDE/BTN_EXTRA are back/forward: XBUTTON1/XBUTTON2.
        275 => Mouse(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 1),
        276 => Mouse(MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 2),
        _ => return None,
    })
}

fn input(target: Target, down: bool) -> INPUT {
    match target {
        Target::Scan(scan, extended) => {
            let mut flags = KEYEVENTF_SCANCODE;
            if extended { flags |= KEYEVENTF_EXTENDEDKEY; }
            if !down { flags |= KEYEVENTF_KEYUP; }
            keyboard(KEYBDINPUT { wScan: scan, dwFlags: flags, ..Default::default() })
        }
        Target::Virtual(key) => keyboard(KEYBDINPUT {
            wVk: VIRTUAL_KEY(key), dwFlags: if down { KEYBD_EVENT_FLAGS(0) } else { KEYEVENTF_KEYUP }, ..Default::default()
        }),
        Target::Mouse(press, release, data) => mouse(MOUSEINPUT {
            mouseData: data, dwFlags: if down { press } else { release }, ..Default::default()
        }),
    }
}
fn keyboard(ki: KEYBDINPUT) -> INPUT { INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } } }
fn mouse(mi: MOUSEINPUT) -> INPUT { INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } } }

/// Injected keyboard/mouse state; every held key is released on close.
#[derive(Default)]
pub(super) struct Desktop { pressed: [u64; 5] }

impl Desktop {
    /// Translate one frame's desktop events; gamepad events are skipped.
    fn translate(&mut self, events: &[Event], inputs: &mut [INPUT; MAX_EVENTS]) -> Result<usize> {
        let (mut count, mut dx, mut dy) = (0, 0i32, 0i32);
        for event in events {
            match event.kind {
                1 => {
                    let Some(target) = target(event.code) else { continue; };
                    let down = match event.value { 0 => false, 1 => true, _ => return Err(invalid("invalid key value")) };
                    let (word, bit) = (usize::from(event.code / 64), 1u64 << (event.code % 64));
                    if down { self.pressed[word] |= bit; } else { self.pressed[word] &= !bit; }
                    inputs[count] = input(target, down);
                    count += 1;
                }
                2 => match event.code {
                    0 => dx = dx.saturating_add(event.value),
                    1 => dy = dy.saturating_add(event.value),
                    _ => return Err(invalid(format!("Windows mouse output cannot represent relative axis {}", event.code))),
                },
                _ => {}
            }
        }
        if dx != 0 || dy != 0 {
            inputs[count] = mouse(MOUSEINPUT { dx, dy, dwFlags: MOUSEEVENTF_MOVE, ..Default::default() });
            count += 1;
        }
        Ok(count)
    }

    pub(super) fn send(&mut self, events: &[Event]) -> Result<()> {
        let mut inputs = [INPUT::default(); MAX_EVENTS];
        let count = self.translate(events, &mut inputs)?;
        inject(&inputs[..count])
    }

    pub(super) fn release(&mut self) -> Result<()> {
        let mut inputs = Vec::new();
        for code in 0..self.pressed.len() as u16 * 64 {
            if self.pressed[usize::from(code / 64)] & 1 << (code % 64) != 0 {
                inputs.push(input(target(code).expect("pressed codes are desktop targets"), false));
            }
        }
        self.pressed = [0; 5];
        inject(&inputs)
    }
}

fn inject(inputs: &[INPUT]) -> Result<()> {
    if inputs.is_empty() { return Ok(()); }
    // SAFETY: initialized INPUT records; size is the record size SendInput expects.
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(invalid(format!("SendInput injected {sent} of {} keyboard/mouse events: {}", inputs.len(), std::io::Error::last_os_error())));
    }
    Ok(())
}

impl VirtualPad {
    pub(super) fn create(link: LinkKind) -> Result<Self> {
        if link == LinkKind::Wired {
            return Err(invalid("Windows wired input is not supported"));
        }
        let client = Client::connect().map_err(|e| invalid(format!("ViGEm connection failed: {e}")))?;
        let mut pad = Self {
            target: Some(Xbox360Wired::new(client, TargetId::XBOX360_WIRED)),
            state: XGamepad::default(),
            link, desktop: Desktop::default(),
        };
        let target = pad.target.as_mut().unwrap();
        target.plugin().map_err(|e| invalid(format!("ViGEm plug-in failed: {e}")))?;
        target.wait_ready().map_err(|e| invalid(format!("ViGEm target not ready: {e}")))?;
        target.update(&pad.state).map_err(|e| invalid(format!("ViGEm initial update failed: {e}")))?;
        Ok(pad)
    }

    pub(super) fn send(&mut self, frame: &Frame) -> Result<()> {
        if self.target.is_none() {
            return Err(invalid("ViGEm target is closed"));
        }
        let result = (|| {
            let next = convert(self.state, self.link, frame.as_slice())?;
            self.desktop.send(frame.as_slice())?;
            if !frame.is_empty() {
                self.target.as_mut().unwrap().update(&next)
                    .map_err(|e| invalid(format!("ViGEm update failed: {e}")))?;
            }
            self.state = next;
            Ok(())
        })();
        if result.is_err() {
            self.close();
        }
        result
    }

    fn close(&mut self) {
        if let Err(error) = self.desktop.release() { eprintln!("Releasing keyboard/mouse output: {error}"); }
        if let Some(mut target) = self.target.take() {
            let _ = target.update(&XGamepad::default());
            let _ = target.unplug();
        }
    }
}

impl Drop for VirtualPad {
    fn drop(&mut self) { self.close(); }
}

fn convert(mut state: XGamepad, link: LinkKind, events: &[Event]) -> Result<XGamepad> {
    if link == LinkKind::Wired {
        return Err(invalid("Windows wired input is not supported"));
    }
    if events.is_empty() { return Ok(state); }
    if events.last() != Some(&Event::default()) {
        return Err(invalid("Xbox output frame lacks SYN_REPORT"));
    }
    for event in &events[..events.len() - 1] {
        match event.kind {
            1 => {
                // Profile validation excludes these as destinations; they are unassigned source identities.
                if matches!(event.code, 306 | 309 | 319 | 704..=711) {
                    if !(0..=1).contains(&event.value) { return Err(invalid("invalid Xbox button value")); }
                    continue;
                }
                if target(event.code).is_some() { continue; }
                let flag = match event.code {
                    304 => XButtons::A, 305 => XButtons::B,
                    307 => XButtons::X, 308 => XButtons::Y,
                    310 => XButtons::LB, 311 => XButtons::RB,
                    314 => XButtons::BACK, 315 => XButtons::START,
                    316 => XButtons::GUIDE, 317 => XButtons::LTHUMB,
                    318 => XButtons::RTHUMB,
                    _ => return Err(invalid(format!("Xbox output cannot represent key {}", event.code))),
                };
                match event.value {
                    0 => state.buttons.raw &= !flag,
                    1 => state.buttons.raw |= flag,
                    _ => return Err(invalid("invalid Xbox button value")),
                }
            }
            3 => {
                let codes = if link == LinkKind::Bluetooth { &AXES } else { &RECEIVER_AXES };
                let index = codes.iter().position(|&code| code == event.code)
                    .ok_or_else(|| invalid(format!("Xbox output cannot represent axis {}", event.code)))?;
                match index {
                    0 | 1 => {
                        let value = if link == LinkKind::Bluetooth {
                            bluetooth_stick(event.value)?
                        } else { signed_stick(event.value)? };
                        if index == 0 { state.thumb_lx = value; }
                        else { state.thumb_ly = value.saturating_neg(); }
                    }
                    2 | 3 => {
                        let value = signed_stick(event.value)?;
                        if index == 2 { state.thumb_rx = value; }
                        else { state.thumb_ry = value.saturating_neg(); }
                    }
                    4 | 5 => {
                        let value = u8::try_from(event.value).map_err(|_| invalid("invalid Xbox trigger value"))?;
                        if (index == 4) != (link == LinkKind::Bluetooth) {
                            state.left_trigger = value;
                        } else {
                            state.right_trigger = value;
                        }
                    }
                    6 | 7 => {
                        if !(-1..=1).contains(&event.value) { return Err(invalid("invalid Xbox hat value")); }
                        let (negative, positive) = if index == 6 {
                            (XButtons::LEFT, XButtons::RIGHT)
                        } else { (XButtons::UP, XButtons::DOWN) };
                        state.buttons.raw &= !(negative | positive);
                        if event.value < 0 { state.buttons.raw |= negative; }
                        if event.value > 0 { state.buttons.raw |= positive; }
                    }
                    _ => unreachable!(),
                }
            }
            2 => {}
            _ => return Err(invalid(format!("Xbox output cannot represent event type {}", event.kind))),
        }
    }
    Ok(state)
}

fn signed_stick(value: i32) -> Result<i16> {
    i16::try_from(value).map_err(|_| invalid("invalid Xbox stick value"))
}

fn bluetooth_stick(value: i32) -> Result<i16> {
    if !(0..=255).contains(&value) { return Err(invalid("invalid Bluetooth stick value")); }
    let delta = value - 127;
    Ok(if delta < 0 { (delta * 32768 / 127) as i16 } else { (delta * 32767 / 128) as i16 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: u16, code: u16, value: i32) -> Event { Event { kind, code, value } }
    fn frame(events: &[Event]) -> Vec<Event> {
        let mut result = events.to_vec();
        result.push(Event::default());
        result
    }

    #[test]
    fn bluetooth_axes_and_incremental_buttons() {
        let state = convert(XGamepad::default(), LinkKind::Bluetooth, &frame(&[
            event(3, 0, 0), event(3, 1, 255), event(3, 2, 32767), event(3, 5, -32768),
            event(3, 9, 123), event(3, 10, 255), event(3, 16, -1), event(3, 17, -1),
            event(1, 304, 1), event(1, 308, 1),
        ])).unwrap();
        assert_eq!((state.thumb_lx, state.thumb_ly, state.thumb_rx, state.thumb_ry), (-32768, -32767, 32767, 32767));
        assert_eq!((state.left_trigger, state.right_trigger), (255, 123));
        assert_eq!(state.buttons.raw, XButtons::LEFT | XButtons::UP | XButtons::A | XButtons::Y);
        let next = convert(state, LinkKind::Bluetooth, &frame(&[event(3, 0, 127), event(3, 16, 1), event(1, 304, 0)])).unwrap();
        assert_eq!(next.thumb_lx, 0);
        assert_eq!(next.thumb_ly, -32767);
        assert_eq!(next.buttons.raw, XButtons::RIGHT | XButtons::UP | XButtons::Y);
    }

    #[test]
    fn receiver_axes_buttons_and_releases() {
        let state = convert(XGamepad::default(), LinkKind::Receiver, &frame(&[
            event(3, 0, -32768), event(3, 1, -32768), event(3, 3, -42), event(3, 4, 42),
            event(3, 2, 255), event(3, 5, 1), event(3, 16, 1), event(3, 17, 1),
            event(1, 305, 1), event(1, 307, 1), event(1, 310, 1), event(1, 311, 1),
            event(1, 314, 1), event(1, 315, 1), event(1, 316, 1), event(1, 317, 1), event(1, 318, 1),
        ])).unwrap();
        assert_eq!((state.thumb_lx, state.thumb_ly, state.thumb_rx, state.thumb_ry), (-32768, 32767, -42, -42));
        assert_eq!((state.left_trigger, state.right_trigger), (255, 1));
        assert_eq!(state.buttons.raw, XButtons::RIGHT | XButtons::DOWN | XButtons::B | XButtons::X |
            XButtons::LB | XButtons::RB | XButtons::BACK | XButtons::START | XButtons::GUIDE | XButtons::LTHUMB | XButtons::RTHUMB);
        let next = convert(state, LinkKind::Receiver, &frame(&[event(3, 16, 0), event(3, 17, 0), event(1, 316, 0)])).unwrap();
        assert_eq!(next.buttons.raw, state.buttons.raw & !(XButtons::RIGHT | XButtons::DOWN | XButtons::GUIDE));
    }

    #[test]
    fn unassigned_extra_buttons_leave_xbox_state_unchanged() {
        let original = XGamepad { buttons: XButtons::A.into(), ..XGamepad::default() };
        for code in [306, 309, 319, 704, 705, 706, 707, 708, 709, 710, 711] {
            for value in [0, 1] {
                assert_eq!(convert(original, LinkKind::Bluetooth, &frame(&[event(1, code, value)])).unwrap(), original);
            }
        }
    }

    #[test]
    fn rejects_unrepresentable_or_malformed_frames() {
        for event in [event(1, 320, 1), event(1, 704, 2), event(1, 304, 2), event(3, 1, 256),
            event(3, 2, 32768), event(3, 9, -1), event(3, 16, 2), event(3, 99, 0), Event::default()] {
            assert!(convert(XGamepad::default(), LinkKind::Bluetooth, &frame(&[event])).is_err());
        }
        assert!(convert(XGamepad::default(), LinkKind::Bluetooth, &[event(1, 304, 1)]).is_err());
        assert!(convert(XGamepad::default(), LinkKind::Receiver, &frame(&[event(3, 0, 32768)])).is_err());
        assert!(convert(XGamepad::default(), LinkKind::Wired, &[]).is_err());
    }

    #[test]
    fn desktop_events_translate_coalesce_and_track_held_keys() {
        let mut desktop = Desktop::default();
        let mut inputs = [INPUT::default(); MAX_EVENTS];
        let events = frame(&[event(1, 30, 1), event(1, 103, 1), event(1, 304, 1), event(2, 0, 3),
            event(2, 1, -2), event(2, 0, 4), event(1, 276, 1)]);
        assert_eq!(desktop.translate(&events, &mut inputs).unwrap(), 4, "gamepad A skipped; one coalesced move");
        // SAFETY: each union field read matches the record's INPUT type.
        unsafe {
            assert_eq!((inputs[0].Anonymous.ki.wScan, inputs[0].Anonymous.ki.dwFlags), (30, KEYEVENTF_SCANCODE));
            assert_eq!((inputs[1].Anonymous.ki.wScan, inputs[1].Anonymous.ki.dwFlags), (0x48, KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY));
            assert_eq!((inputs[2].Anonymous.mi.dwFlags, inputs[2].Anonymous.mi.mouseData), (MOUSEEVENTF_XDOWN, 2));
            assert_eq!((inputs[3].Anonymous.mi.dx, inputs[3].Anonymous.mi.dy, inputs[3].Anonymous.mi.dwFlags), (7, -2, MOUSEEVENTF_MOVE));
        }
        assert_eq!(convert(XGamepad::default(), LinkKind::Receiver, &frame(&[event(1, 30, 1), event(2, 0, 5)])).unwrap(),
            XGamepad::default(), "keyboard/mouse events never touch the Xbox state");
        desktop.translate(&frame(&[event(1, 30, 0), event(1, 103, 0)]), &mut inputs).unwrap();
        assert_eq!(desktop.pressed, [0, 0, 0, 0, 1 << (276 % 64)], "only the still-held mouse button remains");
        for bad in [event(1, 30, 2), event(2, 8, 1)] {
            assert!(desktop.translate(&frame(&[bad]), &mut inputs).is_err());
        }
    }
}
