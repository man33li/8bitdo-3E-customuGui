//! Win32 HID transport; protocol transactions live in the parent module.
use super::*;
use std::{mem::size_of, os::windows::ffi::{OsStrExt, OsStringExt}, ffi::OsString};
use ::windows::{
    core::{GUID, PCWSTR},
    Win32::{
        Devices::{
            DeviceAndDriverInstallation::*,
            HumanInterfaceDevice::*,
            Properties::{DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID, DEVPROPTYPE},
        },
        Foundation::{CloseHandle, HANDLE, GENERIC_READ, GENERIC_WRITE, ERROR_IO_PENDING, ERROR_NO_MORE_ITEMS, ERROR_INSUFFICIENT_BUFFER, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
        System::{IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED}, Threading::{CreateEventW, ResetEvent, WaitForSingleObject}},
    },
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: each owned handle is closed exactly once, after I/O is reaped.
        unsafe { let _ = CloseHandle(self.0); }
    }
}

struct DeviceSet(HDEVINFO);
impl Drop for DeviceSet {
    fn drop(&mut self) {
        // SAFETY: the enumeration set is owned by this guard.
        unsafe { let _ = SetupDiDestroyDeviceInfoList(self.0); }
    }
}

fn open_handle(path: &Path, access: u32) -> Result<Handle> {
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if name[..name.len() - 1].contains(&0) { return Err(invalid("NUL in receiver interface path")); }
    // SAFETY: the UTF-16 path is terminated and valid for this call.
    let handle = unsafe {
        CreateFileW(PCWSTR(name.as_ptr()), access, FILE_SHARE_READ | FILE_SHARE_WRITE, None,
            OPEN_EXISTING, FILE_FLAG_OVERLAPPED, None)
    }.map_err(|error| invalid(format!("Cannot open receiver interface {}: {error}", path.display())))?;
    Ok(Handle(handle))
}

fn attributes_and_caps(handle: HANDLE) -> Result<(HIDD_ATTRIBUTES, HIDP_CAPS)> {
    let mut attributes = HIDD_ATTRIBUTES { Size: size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
    let mut preparsed = PHIDP_PREPARSED_DATA::default();
    // SAFETY: valid HID handle and initialized writable output structures.
    unsafe {
        if !HidD_GetAttributes(handle, &mut attributes) { return Err(io::Error::last_os_error().into()); }
        if !HidD_GetPreparsedData(handle, &mut preparsed) { return Err(io::Error::last_os_error().into()); }
        let mut caps = HIDP_CAPS::default();
        let status = HidP_GetCaps(preparsed, &mut caps);
        HidD_FreePreparsedData(preparsed);
        if status != HIDP_STATUS_SUCCESS { return Err(invalid(format!("HidP_GetCaps failed: {status:?}"))); }
        Ok((attributes, caps))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct DeviceInfo {
    pub path: PathBuf,
    pub vendor: u16,
    pub product: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub input_report_bytes: u16,
    pub output_report_bytes: u16,
    pub access: String,
    pub instance_id: String,
    pub container_id: Option<String>,
    pub parents: Vec<String>,
}

/// Inspect only the selected vendor collection; no reports or heartbeats are sent.
pub fn devices() -> Result<Vec<DeviceInfo>> {
    // SAFETY: SetupDi structures use the generated Windows ABI and required cbSize.
    unsafe {
        let guid = HidD_GetHidGuid();
        let set = DeviceSet(SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)?);
        let mut found = Vec::new();
        for index in 0.. {
            let mut interface = SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
            if let Err(error) = SetupDiEnumDeviceInterfaces(set.0, None, &guid, index, &mut interface) {
                if error.code() == ERROR_NO_MORE_ITEMS.to_hresult() { break; }
                return Err(error.into());
            }
            let mut required = 0;
            let sizing = SetupDiGetDeviceInterfaceDetailW(set.0, &interface, None, 0, Some(&mut required), None);
            if let Err(error) = sizing {
                if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() { return Err(error.into()); }
            }
            if (required as usize) < size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() {
                return Err(invalid("Invalid HID interface detail size"));
            }
            // u64 backing provides the generated structure's alignment on Windows x64.
            let mut storage = vec![0u64; (required as usize).div_ceil(size_of::<u64>())];
            let detail = storage.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            SetupDiGetDeviceInterfaceDetailW(set.0, &interface, Some(detail), required, None, Some(&mut info))?;
            let offset = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
            let name = std::slice::from_raw_parts(std::ptr::addr_of!((*detail).DevicePath).cast::<u16>(), (required as usize - offset) / 2);
            let end = name.iter().position(|&value| value == 0).ok_or_else(|| invalid("Unterminated HID interface path"))?;
            let path = PathBuf::from(OsString::from_wide(&name[..end]));
            // This is only a cheap prefilter; attributes AND descriptor usage below are authoritative.
            let lower = path.to_string_lossy().to_ascii_lowercase();
            if !lower.contains("vid_2dc8&pid_3024") { continue; }
            let handle = open_handle(&path, 0)?;
            let (attributes, caps) = attributes_and_caps(handle.0)?;
            if !vendor_collection(attributes.VendorID, attributes.ProductID, caps.UsagePage, caps.Usage) { continue; }
            let access = match open_handle(&path, GENERIC_READ.0 | GENERIC_WRITE.0) {
                Ok(_) => "read_write".to_owned(),
                Err(error) => error.to_string(),
            };
            let mut instance = vec![0u16; MAX_DEVICE_ID_LEN as usize];
            SetupDiGetDeviceInstanceIdW(set.0, &info, Some(&mut instance), None)?;
            let instance_id = String::from_utf16_lossy(&instance[..instance.iter().position(|&c| c == 0).unwrap_or(instance.len())]);
            let mut property_type = DEVPROPTYPE::default();
            let mut container = [0u8; 16];
            let container_id = if SetupDiGetDevicePropertyW(set.0, &info, &DEVPKEY_Device_ContainerId, &mut property_type, Some(&mut container), None, 0).is_ok() && property_type == DEVPROP_TYPE_GUID {
                Some(format!("{:?}", std::ptr::read_unaligned(container.as_ptr().cast::<GUID>())))
            } else { None };
            let mut parents = Vec::new();
            let mut node = info.DevInst;
            loop {
                let mut parent = 0;
                if CM_Get_Parent(&mut parent, node, 0) != CR_SUCCESS { break; }
                let mut id = vec![0u16; MAX_DEVICE_ID_LEN as usize];
                if CM_Get_Device_IDW(parent, &mut id, 0) != CR_SUCCESS { break; }
                parents.push(String::from_utf16_lossy(&id[..id.iter().position(|&c| c == 0).unwrap_or(id.len())]));
                node = parent;
            }
            found.push(DeviceInfo { path, vendor: attributes.VendorID, product: attributes.ProductID,
                usage_page: caps.UsagePage, usage: caps.Usage, input_report_bytes: caps.InputReportByteLength,
                output_report_bytes: caps.OutputReportByteLength, access, instance_id, container_id, parents });
        }
        Ok(found)
    }
}

pub fn find() -> Result<PathBuf> {
    let mut found = devices()?;
    match found.len() {
        1 => Ok(found.remove(0).path),
        0 => Err(io::Error::new(io::ErrorKind::NotFound, "8BitDo 2.4 GHz receiver (2dc8:3024) vendor interface not found").into()),
        _ => Err(invalid("Multiple 2.4 GHz receivers; unplug all but one")),
    }
}

struct Operation {
    overlapped: OVERLAPPED,
    event: Handle,
}
impl Operation {
    fn new() -> Result<Box<Self>> {
        // SAFETY: unnamed manual-reset event, owned until all I/O completes.
        let event = Handle(unsafe { CreateEventW(None, true, false, PCWSTR::null())? });
        Ok(Box::new(Self { overlapped: OVERLAPPED { hEvent: event.0, ..Default::default() }, event }))
    }
}

pub struct Receiver {
    file: Handle,
    read: Box<Operation>,
    write: Box<Operation>,
    pub(super) stop: Option<Arc<AtomicBool>>,
    pub(super) heartbeat: Arc<parking_lot::Mutex<Heartbeat>>,
}

impl Receiver {
    pub fn open(path: &Path) -> Result<Self> {
        let file = open_handle(path, GENERIC_READ.0 | GENERIC_WRITE.0)?;
        let (attributes, caps) = attributes_and_caps(file.0)?;
        if !vendor_collection(attributes.VendorID, attributes.ProductID, caps.UsagePage, caps.Usage) {
            return Err(invalid("Not the 8BitDo 2dc8:3024 HID FF00:1 vendor collection"));
        }
        check_report_lengths(caps.InputReportByteLength, caps.OutputReportByteLength)?;
        // Windows interface paths are case-insensitive; all opens share one schedule/transaction lock.
        let key = PathBuf::from(path.as_os_str().to_string_lossy().to_lowercase());
        let mut registry = HEARTBEATS.lock();
        registry.retain(|_, state| state.strong_count() != 0);
        let heartbeat = registry.get(&key).and_then(Weak::upgrade).unwrap_or_else(|| {
            let state = Arc::new(parking_lot::Mutex::new(Heartbeat::default()));
            registry.insert(key, Arc::downgrade(&state));
            state
        });
        Ok(Self { file, read: Operation::new()?, write: Operation::new()?, stop: None, heartbeat })
    }

    /// Complete or cancel/reap before any borrowed buffer or OVERLAPPED can be released.
    fn finish(file: HANDLE, operation: &mut Operation, stop: Option<&AtomicBool>, deadline: Instant, cancellable: bool) -> Result<Option<usize>> {
        loop {
            let stopped = cancellable && (interrupted() || stop.is_some_and(|stop| stop.load(Ordering::Relaxed)));
            let now = Instant::now();
            let wait = if stopped || now >= deadline { 0 } else { deadline.saturating_duration_since(now).as_millis().clamp(1, 100) as u32 };
            // SAFETY: operation event and OVERLAPPED remain live through completion/reaping.
            let ready = unsafe { WaitForSingleObject(operation.event.0, wait) };
            if ready == WAIT_OBJECT_0 && !stopped {
                let mut transferred = 0;
                unsafe { GetOverlappedResult(file, &operation.overlapped, &mut transferred, false)?; }
                return Ok(Some(transferred as usize));
            }
            if stopped || now >= deadline || ready != WAIT_TIMEOUT {
                let wait_error = (ready != WAIT_OBJECT_0 && ready != WAIT_TIMEOUT).then(io::Error::last_os_error);
                // CancelIoEx can race successful completion. Always reap; a completed report must not be discarded.
                let mut transferred = 0;
                let completed = unsafe {
                    let _ = CancelIoEx(file, Some(&operation.overlapped));
                    GetOverlappedResult(file, &operation.overlapped, &mut transferred, true)
                };
                if stopped { return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into()); }
                if let Some(error) = wait_error { return Err(error.into()); }
                return match completed {
                    Ok(()) => Ok(Some(transferred as usize)),
                    Err(error) if error.code() == ::windows::Win32::Foundation::ERROR_OPERATION_ABORTED.to_hresult() => Ok(None),
                    Err(error) => Err(error.into()),
                };
            }
        }
    }

    pub(super) fn send(&mut self, report: &[u8; 32]) -> Result<()> {
        // SAFETY: previous operations are always reaped before reusing this event.
        unsafe { ResetEvent(self.write.event.0)?; }
        self.write.overlapped = OVERLAPPED { hEvent: self.write.event.0, ..Default::default() };
        // SAFETY: no write remains pending from previous calls; buffer lives until finish reaps I/O.
        let started = unsafe { WriteFile(self.file.0, Some(report), None, Some(&mut self.write.overlapped)) };
        if let Err(error) = started {
            if error.code() != ERROR_IO_PENDING.to_hresult() { return Err(error.into()); }
        }
        let length = Self::finish(self.file.0, &mut self.write, None, Instant::now() + TIMEOUT, false)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Receiver output report timed out"))?;
        // HID output reports are delivered whole. Windows counts the report-ID byte; Wine's
        // hidraw-backed bus reports 31 of 32 (ID excluded, observed). Anything else is short.
        if length != report.len() && length != report.len() - 1 { return Err(invalid(format!("Short receiver output report ({length} of {} bytes transferred)", report.len()))); }
        Ok(())
    }

    pub(super) fn next(&mut self, deadline: Instant, buffer: &mut [u8; 64]) -> Result<Option<usize>> {
        if interrupted() || self.stop.as_ref().is_some_and(|stop| stop.load(Ordering::Relaxed)) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into());
        }
        // SAFETY: previous reads are always reaped before reusing this event.
        unsafe { ResetEvent(self.read.event.0)?; }
        self.read.overlapped = OVERLAPPED { hEvent: self.read.event.0, ..Default::default() };
        // Caps were validated as 32 including ID; never add a synthetic leading report byte.
        let started = unsafe { ReadFile(self.file.0, Some(&mut buffer[..32]), None, Some(&mut self.read.overlapped)) };
        if let Err(error) = started {
            if error.code() != ERROR_IO_PENDING.to_hresult() { return Err(error.into()); }
        }
        let length = Self::finish(self.file.0, &mut self.read, self.stop.as_deref(), deadline, true)?;
        if length.is_some_and(|length| length != 32) { return Err(invalid("Short receiver input report (expected 32 bytes including report ID)")); }
        Ok(length)
    }

    pub fn input(&mut self, timeout: Duration) -> Result<(Option<Pad>, Option<protocol::PhysicalInput>)> {
        // Other opens must not consume replies while a configuration transaction holds the lock.
        let heartbeat = Arc::clone(&self.heartbeat);
        let Some(_reader) = heartbeat.try_lock() else { return Ok((None, None)); };
        let mut buffer = [0u8; 64];
        let mut newest = None;
        let mut physical = None;
        let mut deadline = Instant::now() + timeout;
        while let Some(length) = self.next(deadline, &mut buffer)? {
            if let Some(state) = pad(&buffer[..length]) { newest = Some(state); }
            if let Some(state) = physical_input(&buffer[..length])? { physical = Some(state); }
            deadline = Instant::now();
        }
        Ok((newest, physical))
    }
}
