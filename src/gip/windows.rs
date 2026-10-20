//! Windows GUI-only build: no GIP discovery or device backend.
use super::{Battery, Device, Rebind, VERSION_SIZE};
use crate::{Result, invalid, protocol};
use std::{path::Path, time::Duration};

/// Battery data is unavailable without a Windows backend. The shared Option API
/// cannot report an unsupported error, so absence is represented by None.
pub fn battery(_device: &Device) -> Option<Battery> {
    None
}

pub fn discover() -> Result<Vec<Device>> {
    Err(invalid("GIP device discovery not yet supported on Windows"))
}

pub fn find() -> Result<Device> {
    Err(invalid("GIP device lookup not yet supported on Windows"))
}

pub struct Gip {
    _private: (),
}

impl Gip {
    pub fn takeover(_path: &Path) -> Result<Self> {
        Err(invalid("GIP device takeover not yet supported on Windows"))
    }

    pub fn read_version(&mut self) -> Result<[u8; VERSION_SIZE]> {
        Err(invalid("GIP version reads not yet supported on Windows"))
    }

    pub fn read_profiles(&mut self) -> Result<Vec<u8>> {
        Err(invalid("GIP profile reads not yet supported on Windows"))
    }

    pub fn write_profiles(&mut self, _image: &[u8; protocol::CONFIG_SIZE]) -> Result<()> {
        Err(invalid("GIP profile writes not yet supported on Windows"))
    }

    pub fn calibration_start(&mut self, _selector: u16) -> Result<()> {
        Err(invalid("GIP calibration not yet supported on Windows"))
    }

    pub fn calibration_end(&mut self, _selector: u16) -> Result<()> {
        Err(invalid("GIP calibration not yet supported on Windows"))
    }

    pub fn next_message(&mut self, _timeout: Duration) -> Result<Option<(u8, u8, Vec<u8>)>> {
        Err(invalid("GIP message reception not yet supported on Windows"))
    }

    pub fn enable_reporting(&mut self) -> Result<()> {
        Err(invalid("GIP input reporting not yet supported on Windows"))
    }

    pub fn heartbeat(&mut self) -> Result<()> {
        Err(invalid("GIP heartbeat not yet supported on Windows"))
    }

    pub fn next_physical(&mut self, _timeout: Duration) -> Result<Option<[u8; 6]>> {
        Err(invalid("GIP physical input not yet supported on Windows"))
    }

    pub fn enable_xinput_mode(&mut self) -> Result<()> {
        Err(invalid("GIP XInput mode switching not yet supported on Windows"))
    }

    pub fn close(self) -> Result<Rebind> {
        Err(invalid("GIP device cleanup not yet supported on Windows"))
    }
}
