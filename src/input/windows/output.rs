use super::super::{Event, Frame, LinkKind, AXES, RECEIVER_AXES};
use crate::{Result, invalid};
use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};

pub(super) struct VirtualPad {
    target: Option<Xbox360Wired<Client>>,
    state: XGamepad,
    link: LinkKind,
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
            link,
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
        for event in [event(1, 320, 1), event(1, 704, 2), event(1, 304, 2), event(2, 0, 1), event(3, 1, 256),
            event(3, 2, 32768), event(3, 9, -1), event(3, 16, 2), event(3, 99, 0), Event::default()] {
            assert!(convert(XGamepad::default(), LinkKind::Bluetooth, &frame(&[event])).is_err());
        }
        assert!(convert(XGamepad::default(), LinkKind::Bluetooth, &[event(1, 304, 1)]).is_err());
        assert!(convert(XGamepad::default(), LinkKind::Receiver, &frame(&[event(3, 0, 32768)])).is_err());
        assert!(convert(XGamepad::default(), LinkKind::Wired, &[]).is_err());
    }
}
