//! Read-only discovery and conservative selection of physical gamepad devnodes.
use crate::{invalid, Result};
use std::{ffi::OsString, mem::{offset_of, size_of}, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}};
use ::windows::{core::PCWSTR, Win32::{
    Devices::{DeviceAndDriverInstallation::*, HumanInterfaceDevice::*},
    Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GENERIC_READ, HANDLE},
    Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Bluetooth, Receiver }

#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub kind: Kind,
    pub instance_id: String,
    pub input_report_bytes: u16,
    pub input_access: bool,
}

struct DeviceSet(HDEVINFO);
impl Drop for DeviceSet {
    fn drop(&mut self) { unsafe { let _ = SetupDiDestroyDeviceInfoList(self.0); } }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) { unsafe { let _ = CloseHandle(self.0); } }
}

struct Interface { path: PathBuf, id: String, parents: Vec<String> }

fn node_id(node: u32) -> Result<String> {
    let mut buffer = [0u16; MAX_DEVICE_ID_LEN as usize];
    let status = unsafe { CM_Get_Device_IDW(node, &mut buffer, 0) };
    if status != CR_SUCCESS { return Err(invalid(format!("Cannot read device instance ID: {status:?}"))); }
    let end = buffer.iter().position(|&c| c == 0).ok_or_else(|| invalid("Unterminated device instance ID"))?;
    String::from_utf16(&buffer[..end]).map_err(|e| invalid(format!("Invalid device instance ID: {e}")))
}

fn ancestry(mut node: u32) -> Result<Vec<String>> {
    let mut parents = Vec::new();
    loop {
        let mut parent = 0;
        let status = unsafe { CM_Get_Parent(&mut parent, node, 0) };
        if status == CR_NO_SUCH_DEVNODE { break; }
        if status != CR_SUCCESS { return Err(invalid(format!("Cannot inspect device ancestry: {status:?}"))); }
        if parent == node || parents.len() >= 128 { return Err(invalid("Invalid device ancestry")); }
        parents.push(node_id(parent)?);
        node = parent;
    }
    Ok(parents)
}

fn interfaces() -> Result<Vec<Interface>> {
    unsafe {
        let guid = HidD_GetHidGuid();
        let set = DeviceSet(SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)?);
        let mut result = Vec::new();
        for index in 0.. {
            let mut interface = SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
            if let Err(error) = SetupDiEnumDeviceInterfaces(set.0, None, &guid, index, &mut interface) {
                if error.code() == ERROR_NO_MORE_ITEMS.to_hresult() { break; }
                return Err(error.into());
            }
            let mut required = 0;
            if let Err(error) = SetupDiGetDeviceInterfaceDetailW(set.0, &interface, None, 0, Some(&mut required), None) {
                if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() { return Err(error.into()); }
            }
            if (required as usize) < size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() { return Err(invalid("Invalid HID interface detail size")); }
            let mut storage = vec![0u64; (required as usize).div_ceil(size_of::<u64>())];
            let detail = storage.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            SetupDiGetDeviceInterfaceDetailW(set.0, &interface, Some(detail), required, None, Some(&mut info))?;
            let offset = offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
            let name = std::slice::from_raw_parts(std::ptr::addr_of!((*detail).DevicePath).cast::<u16>(), (required as usize - offset) / 2);
            let end = name.iter().position(|&c| c == 0).ok_or_else(|| invalid("Unterminated HID interface path"))?;
            result.push(Interface { path: PathBuf::from(OsString::from_wide(&name[..end])), id: node_id(info.DevInst)?, parents: ancestry(info.DevInst)? });
        }
        Ok(result)
    }
}

fn caps(path: &Path) -> Result<(HIDD_ATTRIBUTES, HIDP_CAPS)> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if wide[..wide.len()-1].contains(&0) { return Err(invalid("NUL in HID interface path")); }
    unsafe {
        let handle = Handle(CreateFileW(PCWSTR(wide.as_ptr()), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, Default::default(), None)
            .map_err(|e| invalid(format!("Cannot inspect HID {}: {e}", path.display())))?);
        let mut attributes = HIDD_ATTRIBUTES { Size: size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
        if !HidD_GetAttributes(handle.0, &mut attributes) { return Err(invalid(format!("Cannot read HID attributes for {}: {}", path.display(), std::io::Error::last_os_error()))); }
        let mut data = PHIDP_PREPARSED_DATA::default();
        if !HidD_GetPreparsedData(handle.0, &mut data) { return Err(invalid(format!("Cannot read HID descriptor for {}: {}", path.display(), std::io::Error::last_os_error()))); }
        let mut caps = HIDP_CAPS::default();
        let status = HidP_GetCaps(data, &mut caps);
        HidD_FreePreparsedData(data);
        if status != HIDP_STATUS_SUCCESS { return Err(invalid(format!("Cannot parse HID descriptor for {}: {status:?}", path.display()))); }
        Ok((attributes, caps))
    }
}

fn readable(path: &Path) -> bool {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if wide[..wide.len() - 1].contains(&0) { return false; }
    unsafe {
        CreateFileW(PCWSTR(wide.as_ptr()), GENERIC_READ.0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, Default::default(), None)
            .map(|handle| Handle(handle)).is_ok()
    }
}

fn gamepad(caps: &HIDP_CAPS) -> bool { caps.UsagePage == 1 && (caps.Usage == 4 || caps.Usage == 5) }
fn same(a: &str, b: &str) -> bool { a.eq_ignore_ascii_case(b) }
fn usb_root(id: &str) -> bool {
    let upper = id.to_ascii_uppercase();
    let Some((device, _)) = upper.strip_prefix("USB\\").and_then(|v| v.split_once('\\')) else { return false; };
    device.starts_with("VID_2DC8&PID_3024") && !device.contains("&MI_") &&
        matches!(device.as_bytes().get(17), None | Some(b'&'))
}
fn receiver_root(item: &Interface) -> Result<String> {
    let roots: Vec<_> = item.parents.iter().filter(|id| usb_root(id)).collect();
    match roots.as_slice() {
        [root] => Ok((*root).clone()),
        _ => Err(invalid(format!("Receiver {} has {} matching USB ancestors; refusing ambiguous association", item.path.display(), roots.len()))),
    }
}

pub fn discover() -> Result<Vec<Source>> {
    let mut result = Vec::new();
    for item in interfaces()? {
        let lower = item.path.to_string_lossy().to_ascii_lowercase();
        let bluetooth = item.parents.iter().any(|id| {
            let upper = id.to_ascii_uppercase(); upper.starts_with("BTHLEDEVICE\\") || upper.starts_with("BTHENUM\\")
        });
        let receiver = lower.contains("vid_2dc8&pid_3024");
        if !bluetooth && !receiver { continue; }
        let Ok((attributes, descriptor)) = caps(&item.path) else { continue; };
        let kind = if bluetooth && attributes.VendorID == 0x2dc8 && attributes.ProductID == 0x202f && gamepad(&descriptor) && descriptor.InputReportByteLength >= 32 {
            Kind::Bluetooth
        } else if receiver && attributes.VendorID == 0x2dc8 && attributes.ProductID == 0x3024 && descriptor.UsagePage == 0xff00 && descriptor.Usage == 1 {
            receiver_root(&item)?;
            Kind::Receiver
        } else { continue; };
        let input_access = readable(&item.path);
        result.push(Source { path: item.path, kind, instance_id: item.id, input_report_bytes: descriptor.InputReportByteLength, input_access });
    }
    Ok(result)
}

/// Revalidate the selected source before returning only gamepad collection/function IDs.
pub fn safe_hide_ids(source: &Source) -> Result<Vec<String>> {
    let found = discover()?;
    let matches: Vec<_> = found.iter().filter(|s| s.kind == source.kind && same(&s.instance_id, &source.instance_id) && s.path.as_os_str().to_string_lossy().eq_ignore_ascii_case(&source.path.as_os_str().to_string_lossy())).collect();
    if matches.len() != 1 { return Err(invalid(format!("Selected source {} is missing or ambiguous", source.path.display()))); }
    let items = interfaces()?;
    let selected: Vec<_> = items.iter().filter(|item| same(&item.id, &source.instance_id) && item.path.as_os_str().to_string_lossy().eq_ignore_ascii_case(&source.path.as_os_str().to_string_lossy())).collect();
    if selected.len() != 1 { return Err(invalid("Selected source interface disappeared or is ambiguous")); }
    if source.kind == Kind::Bluetooth {
        for item in items.iter().filter(|item| same(&item.id, &source.instance_id)) {
            let (_, descriptor) = caps(&item.path)?;
            if !gamepad(&descriptor) {
                return Err(invalid(format!("Bluetooth source {} shares its device node with a non-gamepad HID collection", source.path.display())));
            }
        }
        return Ok(vec![matches[0].instance_id.clone()]);
    }
    let root = receiver_root(selected[0])?;
    let mut hid = Vec::new();
    for item in &items {
        if same(&item.id, &source.instance_id) || !item.parents.iter().any(|id| same(id, &root)) { continue; }
        let (_, descriptor) = caps(&item.path)?;
        if gamepad(&descriptor) && !hid.iter().any(|id: &String| same(id, &item.id)) { hid.push(item.id.clone()); }
    }
    let mut xusb = Vec::new();
    unsafe {
        let set = DeviceSet(SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)?);
        for index in 0.. {
            let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if let Err(error) = SetupDiEnumDeviceInfo(set.0, index, &mut info) {
                if error.code() == ERROR_NO_MORE_ITEMS.to_hresult() { break; }
                return Err(error.into());
            }
            let id = node_id(info.DevInst)?;
            if same(&id, &root) || !ancestry(info.DevInst)?.iter().any(|parent| same(parent, &root)) { continue; }
            // A USB interface is eligible only when Windows binds it to the XUSB driver.
            let mut bytes = [0u8; 512];
            let mut ty = 0;
            let mut required = 0;
            let service = if SetupDiGetDeviceRegistryPropertyW(set.0, &info, SPDRP_SERVICE, Some(&mut ty), Some(&mut bytes), Some(&mut required)).is_ok() && ty == 1 && required >= 2 && required as usize <= bytes.len() && required % 2 == 0 {
                let words: Vec<u16> = bytes[..required as usize].chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
                String::from_utf16_lossy(&words[..words.iter().position(|&c| c == 0).unwrap_or(words.len())])
            } else { String::new() };
            let upper = id.to_ascii_uppercase();
            let xusb_node = upper.starts_with("XUSB\\") || (upper.starts_with("USB\\VID_2DC8&PID_3024&MI_") && matches!(service.to_ascii_lowercase().as_str(), "xusb" | "xusb21" | "xusb22"));
            if xusb_node && !xusb.iter().any(|other: &String| same(other, &id)) { xusb.push(id); }
        }
    }
    if hid.len() != 1 || xusb.len() != 1 {
        return Err(invalid(format!("Receiver {} has {} gamepad HID and {} XUSB candidates; refusing ambiguous or missing association", source.path.display(), hid.len(), xusb.len())));
    }
    for item in items.iter().filter(|item| same(&item.id, &hid[0]) || same(&item.id, &xusb[0])) {
        let (_, descriptor) = caps(&item.path)?;
        if !gamepad(&descriptor) {
            return Err(invalid(format!("Receiver gamepad node {} shares a non-gamepad HID collection", item.id)));
        }
    }
    hid.extend(xusb);
    Ok(hid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_root_excludes_interfaces_and_other_products() {
        assert!(usb_root("USB\\VID_2DC8&PID_3024\\serial"));
        assert!(usb_root("USB\\VID_2DC8&PID_3024&REV_0100\\serial"));
        assert!(!usb_root("USB\\VID_2DC8&PID_3024&MI_00\\serial"));
        assert!(!usb_root("USB\\VID_2DC8&PID_30240\\serial"));
        assert!(!usb_root("USB\\VID_2DC8&PID_202F\\serial"));
    }
}
