//! Windows GUI-only boundary: shared mapping remains available without device I/O.
use super::{DeviceInfo, HostProfile};
use crate::{Result, invalid, protocol::PhysicalInput};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool, mpsc::Receiver},
};

pub fn devices() -> Result<Vec<DeviceInfo>> {
    Err(invalid("Input device discovery not yet supported on Windows"))
}

pub fn run(
    _device: Option<&Path>,
    _profile: &HostProfile,
    _slot_map: Option<crate::profile::KeyMap>,
    _physical: Receiver<PhysicalInput>,
    _stopped: Arc<AtomicBool>,
) -> Result<()> {
    Err(invalid("Bluetooth input mapping not yet supported on Windows"))
}

pub fn run_receiver(
    _device: Option<&Path>,
    _profile: &HostProfile,
    _slot_map: Option<crate::profile::KeyMap>,
    _stopped: Arc<AtomicBool>,
) -> Result<()> {
    Err(invalid("Receiver input mapping not yet supported on Windows"))
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
    fn device_entry_points_report_unsupported() {
        let profile = HostProfile::default();
        let stopped = Arc::new(AtomicBool::new(false));
        let (_sender, physical) = std::sync::mpsc::channel();
        assert!(devices().is_err());
        assert!(run(None, &profile, None, physical, Arc::clone(&stopped)).is_err());
        assert!(run_receiver(None, &profile, None, Arc::clone(&stopped)).is_err());
        assert!(run_wired(None, &profile, stopped).is_err());
        assert!(rumble::devices().is_err());
        assert!(rumble::Effect::play(Path::new("unused"), 0, 0, std::time::Duration::ZERO).is_err());
    }
}
