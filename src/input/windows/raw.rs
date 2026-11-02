use crate::{Result, invalid};
use std::{io, mem::size_of, os::windows::ffi::OsStrExt, path::Path, time::Duration};
use ::windows::{core::PCWSTR, Win32::{
    Devices::HumanInterfaceDevice::*,
    Foundation::{CloseHandle, HANDLE, GENERIC_READ, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Storage::FileSystem::{CreateFileW, ReadFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
    System::{IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED}, Threading::{CreateEventW, ResetEvent, WaitForSingleObject}},
}};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) { unsafe { let _ = CloseHandle(self.0); } }
}

pub(super) struct Raw {
    file: Handle,
    event: Handle,
    overlapped: Box<OVERLAPPED>,
}

impl Raw {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if name[..name.len()-1].contains(&0) { return Err(invalid("NUL in Bluetooth HID path")); }
        // The open handle is checked again against the actual HID attributes and descriptor.
        let file = Handle(unsafe { CreateFileW(PCWSTR(name.as_ptr()), GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAG_OVERLAPPED, None) }
            .map_err(|e| invalid(format!("Cannot open Bluetooth gaming HID: {e}")))?);
        let mut attributes = HIDD_ATTRIBUTES { Size: size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
        let mut data = PHIDP_PREPARSED_DATA::default();
        let caps = unsafe {
            if !HidD_GetAttributes(file.0, &mut attributes) || !HidD_GetPreparsedData(file.0, &mut data) {
                return Err(io::Error::last_os_error().into());
            }
            let mut caps = HIDP_CAPS::default();
            let status = HidP_GetCaps(data, &mut caps);
            HidD_FreePreparsedData(data);
            if status != HIDP_STATUS_SUCCESS { return Err(invalid(format!("HidP_GetCaps: {status:?}"))); }
            caps
        };
        if attributes.VendorID != 0x2dc8 || attributes.ProductID != 0x202f
            || caps.UsagePage != 1 || ![4, 5].contains(&caps.Usage) || caps.InputReportByteLength != 32 {
            return Err(invalid("Selected HID is not the 32-byte 8BitDo Bluetooth gamepad collection"));
        }
        let event = Handle(unsafe { CreateEventW(None, true, false, PCWSTR::null())? });
        let overlapped = Box::new(OVERLAPPED { hEvent: event.0, ..Default::default() });
        Ok(Self { file, event, overlapped })
    }

    pub(super) fn read(&mut self, timeout: Duration, buffer: &mut [u8; 32]) -> Result<Option<()>> {
        unsafe { ResetEvent(self.event.0)?; }
        *self.overlapped = OVERLAPPED { hEvent: self.event.0, ..Default::default() };
        let result = unsafe { ReadFile(self.file.0, Some(&mut buffer[..]), None, Some(&mut *self.overlapped)) };
        if let Err(e) = result { if e.code() != ERROR_IO_PENDING.to_hresult() { return Err(e.into()); } }
        let ready = unsafe { WaitForSingleObject(self.event.0, timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32) };
        if ready != WAIT_OBJECT_0 {
            let wait_error = (ready != WAIT_TIMEOUT).then(io::Error::last_os_error);
            let mut transferred = 0;
            let completed = unsafe {
                let _ = CancelIoEx(self.file.0, Some(&*self.overlapped));
                GetOverlappedResult(self.file.0, &*self.overlapped, &mut transferred, true)
            };
            if let Some(error) = wait_error { return Err(error.into()); }
            match completed {
                Ok(()) if transferred == 32 => return Ok(Some(())),
                Ok(()) => return Err(invalid("Short Bluetooth HID report")),
                Err(e) if e.code() == ERROR_OPERATION_ABORTED.to_hresult() => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
        let mut transferred = 0;
        unsafe { GetOverlappedResult(self.file.0, &*self.overlapped, &mut transferred, false)?; }
        if transferred != 32 { return Err(invalid("Short Bluetooth HID report")); }
        Ok(Some(()))
    }
}
