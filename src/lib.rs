pub mod att;
pub mod gip;
pub mod gui;
pub mod input;
pub mod live;
pub mod profile;
pub mod profiles;
pub mod protocol;
pub mod receiver;
#[cfg(target_os = "linux")]
pub mod steam;
pub mod ui;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn interrupted() -> bool {
    INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn invalid(message: impl Into<String>) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()).into()
}
