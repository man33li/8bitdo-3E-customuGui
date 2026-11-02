//! HidHide changes are journaled before the first driver mutation.
use crate::{Result, invalid};
use serde::{Deserialize, Serialize};
use std::{
    ffi::c_void,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn DeviceIoControl(handle: *mut c_void, code: u32, input: *const c_void, input_len: u32,
        output: *mut c_void, output_len: u32, returned: *mut u32, overlapped: *mut c_void) -> i32;
    fn QueryDosDeviceW(name: *const u16, target: *mut u16, max: u32) -> u32;
}

const GET_WHITE: u32 = 0x8001_6000;
const SET_WHITE: u32 = 0x8001_6004;
const GET_BLACK: u32 = 0x8001_6008;
const SET_BLACK: u32 = 0x8001_600c;
const GET_ACTIVE: u32 = 0x8001_6010;
const SET_ACTIVE: u32 = 0x8001_6014;
const GET_INVERSE: u32 = 0x8001_6018;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    white: Vec<String>,
    black: Vec<String>,
    active: bool,
    inverse: bool,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    version: u8,
    // Each successive state differs by exactly one driver operation.
    states: Vec<State>,
}

fn driver_error(operation: &str, error: std::io::Error) -> crate::Error {
    std::io::Error::new(error.kind(), format!("HidHide {operation}: {error}")).into()
}

fn open_driver() -> Result<File> {
    OpenOptions::new().read(true).share_mode(7).open(r"\\.\HidHide")
        .map_err(|e| driver_error("opening control device", e))
}

pub(super) fn recover_pending() -> Result<()> {
    let profile = crate::ui::default_profile_path();
    let directory = profile.parent().ok_or_else(|| invalid("Profile path has no configuration directory"))?;
    if directory.join("hidhide-journal.json").try_exists()? {
        check_available()?;
    }
    Ok(())
}

/// Recover an interrupted session and check that the installed driver is accessible.
pub(super) fn check_available() -> Result<()> {
    let dir = journal_dir()?;
    let _lock = lock(&dir)?;
    let driver = open_driver()?;
    recover(&driver, &dir.join("hidhide-journal.json"), false)?;
    read_state(&driver)?;
    Ok(())
}

fn ioctl(driver: &File, code: u32, input: &[u8], output: &mut [u8]) -> Result<u32> {
    let mut returned = 0;
    let ok = unsafe { DeviceIoControl(driver.as_raw_handle(), code,
        if input.is_empty() { null() } else { input.as_ptr().cast() }, input.len() as u32,
        if output.is_empty() { null_mut() } else { output.as_mut_ptr().cast() }, output.len() as u32,
        &mut returned, null_mut()) };
    if ok == 0 {
        return Err(driver_error(&format!("IOCTL {code:#010x}"), std::io::Error::last_os_error()));
    }
    Ok(returned)
}

fn read_list(driver: &File, code: u32) -> Result<Vec<String>> {
    let size = ioctl(driver, code, &[], &mut [])? as usize;
    if size < 2 || size % 2 != 0 || size > 16 * 1024 * 1024 {
        return Err(invalid(format!("HidHide IOCTL {code:#010x} returned invalid list size {size}")));
    }
    let mut bytes = vec![0u8; size];
    let returned = ioctl(driver, code, &[], &mut bytes)? as usize;
    if returned != size { return Err(invalid("HidHide list changed while reading")); }
    let wide: Vec<u16> = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    if wide != [0] && !wide.ends_with(&[0, 0]) { return Err(invalid("HidHide returned an unterminated list")); }
    let mut list = Vec::new();
    for part in wide.split(|&c| c == 0) {
        if part.is_empty() { continue; }
        list.push(String::from_utf16(part).map_err(|_| invalid("HidHide returned invalid UTF-16"))?);
    }
    if list.iter().enumerate().any(|(i, item)| list[..i].iter().any(|prior| prior.eq_ignore_ascii_case(item))) {
        return Err(invalid("HidHide returned duplicate list entries"));
    }
    Ok(list)
}

fn read_flag(driver: &File, code: u32) -> Result<bool> {
    let mut value = [0u8];
    if ioctl(driver, code, &[], &mut value)? != 1 || value[0] > 1 {
        return Err(invalid(format!("HidHide IOCTL {code:#010x} returned an invalid flag")));
    }
    Ok(value[0] != 0)
}

fn read_state(driver: &File) -> Result<State> {
    Ok(State { white: read_list(driver, GET_WHITE)?, black: read_list(driver, GET_BLACK)?,
        active: read_flag(driver, GET_ACTIVE)?, inverse: read_flag(driver, GET_INVERSE)? })
}

fn set_list(driver: &File, code: u32, list: &[String]) -> Result<()> {
    let mut bytes = Vec::new();
    for item in list {
        for c in item.encode_utf16().chain([0]) { bytes.extend_from_slice(&c.to_le_bytes()); }
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    ioctl(driver, code, &bytes, &mut [])?;
    Ok(())
}

fn transition(driver: &File, from: &State, to: &State) -> Result<()> {
    if read_state(driver)? != *from { return Err(invalid("HidHide settings changed unexpectedly; journal retained")); }
    let changed = usize::from(from.white != to.white) + usize::from(from.black != to.black)
        + usize::from(from.active != to.active) + usize::from(from.inverse != to.inverse);
    if changed != 1 || from.inverse != to.inverse { return Err(invalid("Invalid HidHide journal transition")); }
    if from.white != to.white { set_list(driver, SET_WHITE, &to.white)?; }
    else if from.black != to.black { set_list(driver, SET_BLACK, &to.black)?; }
    else { ioctl(driver, SET_ACTIVE, &[u8::from(to.active)], &mut [])?; }
    if read_state(driver)? != *to { return Err(invalid("HidHide settings changed during update; journal retained")); }
    Ok(())
}

fn image_name() -> Result<String> {
    let path = std::env::current_exe()?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.len() < 3 || wide[1] != b':' as u16 || wide[2] != b'\\' as u16 {
        return Err(invalid("HidHide requires an executable on a DOS drive"));
    }
    let drive = [wide[0], wide[1], 0];
    let mut target = vec![0u16; 32768];
    let length = unsafe { QueryDosDeviceW(drive.as_ptr(), target.as_mut_ptr(), target.len() as u32) };
    if length == 0 { return Err(driver_error("resolving executable device path", std::io::Error::last_os_error())); }
    let end = target.iter().position(|&c| c == 0).ok_or_else(|| invalid("Invalid DOS device path"))?;
    target.truncate(end);
    target.extend_from_slice(&wide[2..]);
    String::from_utf16(&target).map_err(|_| invalid("Executable path is not valid UTF-16"))
}

fn contains(list: &[String], value: &str) -> bool {
    list.iter().any(|item| item.eq_ignore_ascii_case(value))
}

fn plan(mut state: State, ids: &[String], image: &str) -> Result<Journal> {
    if ids.is_empty() || ids.iter().any(|id| id.is_empty() || id.contains('\0')) {
        return Err(invalid("HidHide requires nonempty validated device instance IDs"));
    }
    let mut states = vec![state.clone()];
    if state.inverse {
        return Err(invalid("HidHide inverse mode cannot reliably hide devices from other applications; disable it manually"));
    }
    if !state.active && state.black.iter().any(|id| !contains(ids, id)) {
        return Err(invalid("Enabling HidHide would activate unrelated existing blacklist entries"));
    }
    if !contains(&state.white, image) {
        state.white.push(image.to_owned());
        states.push(state.clone());
    }
    for id in ids {
        if !contains(&state.black, id) { state.black.push(id.clone()); }
    }
    if state != *states.last().unwrap() { states.push(state.clone()); }
    if !state.active { state.active = true; states.push(state); }
    Ok(Journal { version: 1, states })
}

fn journal_dir() -> Result<PathBuf> {
    let profile = crate::ui::default_profile_path();
    let dir = profile.parent().ok_or_else(|| invalid("Profile path has no configuration directory"))?.to_owned();
    fs::create_dir_all(&dir).map_err(|e| driver_error(&format!("creating recovery directory {}", dir.display()), e))?;
    Ok(dir)
}

fn lock(dir: &Path) -> Result<File> {
    OpenOptions::new().read(true).write(true).create(true).share_mode(0)
        .open(dir.join("hidhide.lock"))
        .map_err(|e| driver_error(&format!("locking recovery journal in {}; ensure the directory is writable and no other session is running", dir.display()), e))
}

fn valid_journal(journal: &Journal) -> bool {
    if journal.version != 1 || journal.states.is_empty() || journal.states.len() > 4 { return false; }
    let mut previous_kind = 0;
    for pair in journal.states.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.inverse != b.inverse { return false; }
        let kind = if a.black == b.black && a.active == b.active && b.white.starts_with(&a.white)
            && b.white.len() == a.white.len() + 1 && !contains(&a.white, &b.white[a.white.len()]) {
            1
        } else if a.white == b.white && a.active == b.active && b.black.starts_with(&a.black)
            && b.black.len() > a.black.len()
            && b.black[a.black.len()..].iter().enumerate().all(|(i, id)| {
                !id.is_empty() && !id.contains('\0') && !contains(&b.black[..a.black.len() + i], id)
            }) {
            2
        } else if a.white == b.white && a.black == b.black && !a.active && b.active {
            3
        } else { return false; };
        if kind <= previous_kind { return false; }
        previous_kind = kind;
    }
    true
}

fn recover(driver: &File, path: &Path, required: bool) -> Result<()> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let mut data = Vec::new();
    Read::by_ref(&mut file).take(16 * 1024 * 1024).read_to_end(&mut data)?;
    let journal: Journal = serde_json::from_slice(&data).map_err(|e| invalid(format!("Invalid HidHide recovery journal: {e}")))?;
    if !valid_journal(&journal) {
        return Err(invalid("Invalid HidHide recovery journal; refusing to modify driver"));
    }
    let current = read_state(driver)?;
    let index = journal.states.iter().position(|state| *state == current)
        .ok_or_else(|| invalid("HidHide settings differ from recovery journal; manual recovery required"))?;
    for i in (1..=index).rev() { transition(driver, &journal.states[i], &journal.states[i - 1])?; }
    drop(file);
    fs::remove_file(path)?;
    Ok(())
}

/// Owns the exclusive HidHide control handle and this process's journal lock.
pub(super) struct Guard {
    driver: File,
    _lock: File,
    journal: PathBuf,
    cleaned: bool,
}

impl Guard {
    /// Restore only the settings recorded as ours, or leave the journal for recovery.
    pub(super) fn cleanup(&mut self) -> Result<()> {
        if !self.cleaned {
            recover(&self.driver, &self.journal, true)?;
            self.cleaned = true;
        }
        Ok(())
    }
}

impl Drop for Guard {
    fn drop(&mut self) { let _ = self.cleanup(); }
}

/// Call only after opening the source; IDs must already be validated gamepad nodes.
pub(super) fn acquire(ids: &[String]) -> Result<Guard> {
    let dir = journal_dir()?;
    let lock = lock(&dir)?;
    // HidHide permits one control handle at a time, preventing other clients' read/write races.
    let driver = open_driver()?;
    let path = dir.join("hidhide-journal.json");
    recover(&driver, &path, false)?;
    let journal = plan(read_state(&driver)?, ids, &image_name()?)?;
    let data = serde_json::to_vec(&journal)?;
    let staging = dir.join(format!("hidhide-journal-{}.tmp", std::process::id()));
    let write = (|| -> Result<()> {
        let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(&staging)
            .map_err(|e| driver_error(&format!("creating recovery journal {}", staging.display()), e))?;
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staging, &path)?;
        Ok(())
    })();
    if write.is_err() { let _ = fs::remove_file(&staging); }
    write?;
    let mut guard = Guard { driver, _lock: lock, journal: path, cleaned: false };
    for pair in journal.states.windows(2) {
        if let Err(error) = transition(&guard.driver, &pair[0], &pair[1]) {
            return match guard.cleanup() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(invalid(format!("{error}; HidHide recovery failed: {cleanup}"))),
            };
        }
    }
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State { white: vec!["other.exe".into()], black: vec!["HID\\OTHER".into()], active: false, inverse: false }
    }

    #[test]
    fn plan_keeps_unrelated_entries_and_tracks_only_additions() {
        let original = state();
        let journal = plan(original.clone(), &["HID\\OTHER".into(), "HID\\GAMEPAD".into()], "our.exe").unwrap();
        assert_eq!(journal.states[0], original);
        assert_eq!(journal.states.last().unwrap().black, ["HID\\OTHER", "HID\\GAMEPAD"]);
        assert_eq!(journal.states.last().unwrap().white, ["other.exe", "our.exe"]);
        assert!(journal.states.last().unwrap().active);
        assert!(valid_journal(&journal));
        assert_eq!(journal.states.len(), 4);
    }

    #[test]
    fn existing_entries_are_not_owned_and_inverse_is_preserved() {
        let mut original = state();
        original.active = true;
        original.white.push("our.exe".into());
        let journal = plan(original.clone(), &["hid\\other".into()], "our.exe").unwrap();
        assert_eq!(journal.states, vec![original.clone()]);
        original.inverse = true;
        assert!(plan(original, &["HID\\GAMEPAD".into()], "our.exe").is_err());
    }

    #[test]
    fn refuses_unrelated_activation_and_corrupt_ownership() {
        assert!(plan(state(), &["HID\\GAMEPAD".into()], "our.exe").is_err());
        let mut journal = plan(state(), &["HID\\OTHER".into(), "HID\\GAMEPAD".into()], "our.exe").unwrap();
        journal.states[2].black.remove(0);
        assert!(!valid_journal(&journal));
    }
}
