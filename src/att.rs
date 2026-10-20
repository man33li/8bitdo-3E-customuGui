//! Shared Bluetooth configuration protocol with Linux ATT and Windows WinRT transports.
use crate::{Result, invalid, protocol};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use crate::interrupted;
use serde::Serialize;
#[cfg(target_os = "windows")]
use std::{
    io,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread,
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
use std::{
    collections::{HashMap, VecDeque},
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU16, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{AttClient, run_controller, scan};
#[cfg(target_os = "windows")]
use windows::Characteristic;

#[cfg(target_os = "linux")]
const LOCAL_MTU: u16 = 250;
const SERVICES: [&str; 2] = [
    "0000ff10-0000-1000-8000-00805f9b34fb",
    "00010203-0405-0607-0809-0a0b0c0d1910",
];
const TRANSPORTS: [(&str, &str, bool); 3] = [
    (SERVICES[0], "0000ff11-0000-1000-8000-00805f9b34fb", false),
    (SERVICES[0], "0000ff13-0000-1000-8000-00805f9b34fb", true),
    (SERVICES[1], "00010203-0405-0607-0809-0a0b0c0d2b11", false),
];
#[cfg(target_os = "linux")]
type Properties = HashMap<String, OwnedValue>;
#[cfg(target_os = "linux")]
type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

#[derive(Clone, Debug, Serialize)]
pub struct Radio {
    pub address: String,
    pub address_type: String,
    pub adapter_address: String,
    pub adapter_address_type: String,
}

// A local name may arrive only in a scan response. Later unnamed advertisements
// must not erase the identification or create duplicate candidates.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct Advertisements(std::collections::BTreeMap<u64, (&'static str, bool)>);

#[cfg(any(target_os = "windows", test))]
impl Advertisements {
    fn observe(&mut self, address: u64, address_type: &'static str, name: &str) {
        let entry = self.0.entry(address).or_default();
        if matches!(address_type, "public" | "random") {
            entry.0 = address_type;
        }
        entry.1 |= name == "81HF01";
    }

    fn radios(self) -> Vec<Radio> {
        self.0.into_iter().filter_map(|(address, (address_type, matched))| {
            (matched && !address_type.is_empty()).then(|| {
                let b = address.to_be_bytes();
                Radio {
                    address: format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", b[2], b[3], b[4], b[5], b[6], b[7]),
                    address_type: address_type.to_owned(),
                    // WinRT's watcher does not identify the receiving adapter.
                    adapter_address: String::new(),
                    adapter_address_type: String::new(),
                }
            })
        }).collect()
    }
}

#[cfg(any(target_os = "windows", test))]
fn bluetooth_address(value: &str) -> Result<u64> {
    let mut result = 0;
    let mut count = 0;
    for octet in value.split(':') {
        if octet.len() != 2 || !octet.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid("Invalid Bluetooth address"));
        }
        result = (result << 8) | u64::from(u8::from_str_radix(octet, 16)?);
        count += 1;
    }
    if count != 6 { return Err(invalid("Invalid Bluetooth address")); }
    Ok(result)
}

#[cfg(target_os = "linux")]
fn text(properties: &Properties, key: &str) -> Result<String> {
    let value = properties
        .get(key)
        .ok_or_else(|| invalid(format!("BlueZ property {key} absent")))?;
    Ok(<&str>::try_from(value)?.to_owned())
}

#[cfg(target_os = "linux")]
fn managed(bus: &zbus::blocking::Connection) -> Result<Objects> {
    let proxy =
        zbus::blocking::Proxy::new(bus, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager")?;
    Ok(proxy.call("GetManagedObjects", &())?)
}

#[cfg(target_os = "linux")]
/// Start and stop only discovery sessions acquired by this connection.
pub fn scan(timeout: Duration, stopped: &AtomicBool) -> Result<Vec<Radio>> {
    if timeout.is_zero() || timeout > Duration::from_secs(300) {
        return Err(invalid(
            "Discovery timeout must be between 0 and 300 seconds",
        ));
    }
    let bus = zbus::blocking::Connection::system()?;
    let objects = managed(&bus)?;
    let mut started = Vec::new();
    let mut failures = Vec::new();
    for (path, interfaces) in &objects {
        if interfaces.contains_key("org.bluez.Adapter1") {
            let proxy =
                zbus::blocking::Proxy::new(&bus, "org.bluez", path.as_str(), "org.bluez.Adapter1")?;
            match proxy.call::<_, _, ()>("StartDiscovery", &()) {
                Ok(()) => started.push(path.clone()),
                Err(error) => failures.push(error.to_string()),
            }
        }
    }
    if started.is_empty() {
        return Err(invalid(format!(
            "No usable Bluetooth adapter: {}",
            failures.join("; ")
        )));
    }
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !stopped.load(Ordering::Relaxed) && !interrupted() {
        thread::sleep(
            Duration::from_millis(50).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    let result = managed(&bus);
    let mut cleanup_error = None;
    for path in &started {
        let proxy =
            zbus::blocking::Proxy::new(&bus, "org.bluez", path.as_str(), "org.bluez.Adapter1")?;
        if let Err(error) = proxy.call::<_, _, ()>("StopDiscovery", &()) {
            cleanup_error = Some(error);
        }
    }
    if let Some(error) = cleanup_error {
        return Err(error.into());
    }
    if stopped.load(Ordering::Relaxed) || interrupted() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Bluetooth discovery interrupted",
        )
        .into());
    }
    let objects = result?;
    let mut radios = Vec::new();
    for interfaces in objects.values() {
        let Some(device) = interfaces.get("org.bluez.Device1") else {
            continue;
        };
        // Name is the advertisement identity; Alias is user-editable and not evidence.
        if text(device, "Name").ok().as_deref() != Some("81HF01") {
            continue;
        }
        let adapter_path = device
            .get("Adapter")
            .ok_or_else(|| invalid("BlueZ device has no adapter"))?;
        let adapter_path = <&zbus::zvariant::ObjectPath>::try_from(adapter_path)?;
        let adapter = objects
            .get(adapter_path)
            .and_then(|i| i.get("org.bluez.Adapter1"))
            .ok_or_else(|| invalid("BlueZ device adapter missing from inventory"))?;
        radios.push(Radio {
            address: text(device, "Address")?,
            address_type: text(device, "AddressType")?,
            adapter_address: text(adapter, "Address")?,
            adapter_address_type: text(adapter, "AddressType")?,
        });
    }
    radios.sort_by(|a, b| a.address.cmp(&b.address));
    Ok(radios)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn timeout_error(message: &str) -> crate::Error {
    io::Error::new(io::ErrorKind::TimedOut, message).into()
}
fn u16le(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}
fn uuid(data: &[u8]) -> Result<String> {
    if data.len() == 2 {
        return Ok(format!(
            "0000{:04x}-0000-1000-8000-00805f9b34fb",
            u16le(data, 0)
        ));
    }
    if data.len() != 16 {
        return Err(invalid("Invalid ATT UUID width"));
    }
    let mut b = [0; 16];
    for (out, value) in b.iter_mut().zip(data.iter().rev()) {
        *out = *value;
    }
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    ))
}

#[cfg(target_os = "linux")]
fn sockaddr(address: &str, address_type: &str) -> Result<[u8; 14]> {
    let mut result = [0; 14];
    result[..2].copy_from_slice(&(libc::AF_BLUETOOTH as u16).to_ne_bytes());
    result[10..12].copy_from_slice(&4u16.to_le_bytes());
    result[12] = match address_type {
        "public" => 1,
        "random" => 2,
        _ => return Err(invalid("Unknown LE address type")),
    };
    let octets: Vec<_> = address.split(':').collect();
    if octets.len() != 6
        || octets
            .iter()
            .any(|s| s.len() != 2 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(invalid("Invalid Bluetooth address"));
    }
    for (i, octet) in octets.iter().rev().enumerate() {
        result[4 + i] = u8::from_str_radix(octet, 16)?;
    }
    Ok(result)
}

#[cfg(target_os = "linux")]
fn poll_fd(fd: i32, events: i16, deadline: Instant) -> io::Result<()> {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ATT socket timed out",
            ));
        }
        let mut pfd = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        let millis = deadline
            .saturating_duration_since(now)
            .as_millis()
            .max(1)
            .min(i32::MAX as u128) as i32;
        // pfd is a valid, uniquely borrowed native pollfd for the duration of poll.
        let result = unsafe { libc::poll(&mut pfd, 1, millis) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if result == 0 {
            continue;
        }
        if pfd.revents & libc::POLLNVAL != 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "ATT descriptor closed",
            ));
        }
        if pfd.revents & (events | libc::POLLERR | libc::POLLHUP) != 0 {
            return Ok(());
        }
    }
}

#[cfg(target_os = "linux")]
fn send(fd: i32, packet: &[u8], timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        // Owned sequenced-packet descriptor; kernel copies from the valid slice.
        let result =
            unsafe { libc::send(fd, packet.as_ptr().cast(), packet.len(), libc::MSG_NOSIGNAL) };
        if result >= 0 {
            return if result as usize == packet.len() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Short ATT packet send",
                ))
            };
        }
        let error = io::Error::last_os_error();
        match error.kind() {
            io::ErrorKind::Interrupted => continue,
            io::ErrorKind::WouldBlock => poll_fd(fd, libc::POLLOUT, deadline)?,
            _ => return Err(error),
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct Packet {
    bytes: [u8; 518],
    length: usize,
}
#[cfg(target_os = "linux")]
impl Packet {
    fn data(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    fn application_data(&self) -> &[u8] {
        &self.data()[3..]
    }
}
#[cfg(target_os = "linux")]
enum Incoming {
    Packet(Packet),
    Failed(String),
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
struct Characteristic {
    service: String,
    uuid: String,
    handle: u16,
    end: u16,
    properties: u8,
}

#[cfg(target_os = "linux")]
pub struct AttClient {
    socket: Arc<OwnedFd>,
    mtu: Arc<AtomicU16>,
    negotiated: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    receiver: Receiver<Incoming>,
    reader: Option<JoinHandle<()>>,
    notifications: VecDeque<Packet>,
    notification_handle: Option<u16>,
    timeout: Duration,
}

#[cfg(target_os = "linux")]
impl AttClient {
    fn clear_notifications(&mut self) {
        self.notifications.clear();
    }
    pub fn connect(radio: &Radio, timeout: Duration, stopped: Arc<AtomicBool>) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(300) {
            return Err(invalid("ATT timeout outside 0..300 seconds"));
        }
        let source = sockaddr(&radio.adapter_address, &radio.adapter_address_type)?;
        let destination = sockaddr(&radio.address, &radio.address_type)?;
        // socket returns an owned fd or -1; every successful fd enters OwnedFd immediately.
        let fd = unsafe {
            libc::socket(
                libc::AF_BLUETOOTH,
                libc::SOCK_SEQPACKET | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let socket = unsafe { OwnedFd::from_raw_fd(fd) };
        let security = [1u8, 0];
        if unsafe { libc::setsockopt(fd, 274, 4, security.as_ptr().cast(), 2) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        if unsafe { libc::bind(fd, source.as_ptr().cast(), source.len() as u32) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        if unsafe { libc::connect(fd, destination.as_ptr().cast(), destination.len() as u32) } < 0 {
            let error = io::Error::last_os_error();
            if !matches!(
                error.raw_os_error(),
                Some(libc::EINPROGRESS | libc::EAGAIN | libc::EINTR)
            ) {
                return Err(error.into());
            }
            poll_fd(fd, libc::POLLOUT, Instant::now() + timeout)?;
            let mut code = 0i32;
            let mut length = std::mem::size_of::<i32>() as u32;
            if unsafe {
                libc::getsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    (&mut code as *mut i32).cast(),
                    &mut length,
                )
            } < 0
            {
                return Err(io::Error::last_os_error().into());
            }
            if code != 0 {
                return Err(io::Error::from_raw_os_error(code).into());
            }
        }
        Ok(Self::from_socket(socket, timeout, stopped))
    }

    fn from_socket(socket: OwnedFd, timeout: Duration, stopped: Arc<AtomicBool>) -> Self {
        let socket = Arc::new(socket);
        let mtu = Arc::new(AtomicU16::new(23));
        let negotiated = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let (sender, receiver) = mpsc::sync_channel(64);
        let (s, m, n, a) = (
            socket.clone(),
            mtu.clone(),
            negotiated.clone(),
            alive.clone(),
        );
        let reader = thread::spawn(move || {
            let receive = || -> Result<()> {
                while a.load(Ordering::Relaxed) {
                    match poll_fd(
                        s.as_raw_fd(),
                        libc::POLLIN,
                        Instant::now() + Duration::from_millis(100),
                    ) {
                        Ok(()) => (),
                        Err(error) if error.kind() == io::ErrorKind::TimedOut => continue,
                        Err(error) => return Err(error.into()),
                    }
                    let mut packet = Packet {
                        bytes: [0; 518],
                        length: 0,
                    };
                    let length = unsafe {
                        libc::recv(
                            s.as_raw_fd(),
                            packet.bytes.as_mut_ptr().cast(),
                            518,
                            libc::MSG_TRUNC,
                        )
                    };
                    if length < 0 {
                        let error = io::Error::last_os_error();
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) {
                            continue;
                        }
                        return Err(error.into());
                    }
                    if length == 0 {
                        return Err(invalid("ATT peer disconnected"));
                    }
                    if length as usize > 518 || length > m.load(Ordering::Relaxed) as isize {
                        return Err(invalid("ATT packet exceeds negotiated MTU"));
                    }
                    packet.length = length as usize;
                    let data = packet.data();
                    match data[0] {
                        2 => {
                            let peer = parse_mtu(data)?;
                            send(s.as_raw_fd(), &[3, 250, 0], timeout)?;
                            m.store(peer.min(LOCAL_MTU), Ordering::Relaxed);
                            n.store(true, Ordering::Relaxed);
                        }
                        3 => {
                            // Apply MTU in the receiver before a queued notification:
                            // the request owner may not have drained this response yet.
                            let peer = parse_mtu(data)?;
                            m.store(peer.min(LOCAL_MTU), Ordering::Relaxed);
                            n.store(true, Ordering::Relaxed);
                            sender.try_send(Incoming::Packet(packet)).map_err(|_| {
                                invalid("ATT receive queue overflow or owner closed")
                            })?;
                        }
                        0x1b | 0x1d => {
                            if data.len() < 3 || u16le(data, 1) == 0 {
                                return Err(invalid("Malformed ATT notification"));
                            }
                            if data[0] == 0x1d {
                                send(s.as_raw_fd(), &[0x1e], timeout)?;
                            }
                            sender.try_send(Incoming::Packet(packet)).map_err(|_| {
                                invalid("ATT receive queue overflow or owner closed")
                            })?;
                        }
                        0x04 | 0x06 | 0x08 | 0x0a | 0x0c | 0x0e | 0x10 | 0x12 | 0x16 | 0x18
                        | 0x20 => {
                            let handle = if data.len() >= 3 {
                                [data[1], data[2]]
                            } else {
                                [0, 0]
                            };
                            send(
                                s.as_raw_fd(),
                                &[1, data[0], handle[0], handle[1], 6],
                                timeout,
                            )?;
                        }
                        0x52 | 0xd2 => {
                            if data.len() < if data[0] == 0xd2 { 15 } else { 3 } {
                                return Err(invalid("Malformed peer ATT command"));
                            }
                        }
                        _ => {
                            sender.try_send(Incoming::Packet(packet)).map_err(|_| {
                                invalid("ATT receive queue overflow or owner closed")
                            })?;
                        }
                    }
                }
                Ok(())
            };
            if let Err(error) = receive() {
                let _ = sender.try_send(Incoming::Failed(error.to_string()));
            }
            a.store(false, Ordering::Relaxed);
        });
        Self {
            socket,
            mtu,
            negotiated,
            alive,
            stopped,
            receiver,
            reader: Some(reader),
            notifications: VecDeque::with_capacity(64),
            notification_handle: None,
            timeout,
        }
    }

    fn abort(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
        unsafe {
            libc::shutdown(self.socket.as_raw_fd(), libc::SHUT_RDWR);
        }
    }
    fn check_stop(&self) -> Result<()> {
        if self.stopped.load(Ordering::Relaxed) || interrupted() {
            return Err(
                io::Error::new(io::ErrorKind::Interrupted, "ATT operation interrupted").into(),
            );
        }
        Ok(())
    }
    fn incoming(&mut self, deadline: Instant, ignore_stop: bool) -> Result<Option<Packet>> {
        loop {
            if !ignore_stop {
                self.check_stop()?;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            match self
                .receiver
                .recv_timeout(remaining.min(Duration::from_millis(100)))
            {
                Ok(Incoming::Packet(packet)) => return Ok(Some(packet)),
                Ok(Incoming::Failed(error)) => {
                    self.abort();
                    return Err(invalid(error));
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    self.abort();
                    return Err(invalid("ATT receive thread closed"));
                }
            }
        }
    }

    fn request(&mut self, packet: &[u8], bounds: (u16, u16), ignore_stop: bool) -> Result<Packet> {
        if !ignore_stop {
            self.check_stop()?;
        }
        if !self.alive.load(Ordering::Relaxed) {
            return Err(invalid("ATT bearer closed"));
        }
        let result = (|| -> Result<Packet> {
            send(self.socket.as_raw_fd(), packet, self.timeout)?;
            let deadline = Instant::now() + self.timeout;
            loop {
                let response = self
                    .incoming(deadline, ignore_stop)?
                    .ok_or_else(|| timeout_error("ATT response timed out"))?;
                let data = response.data();
                if matches!(data[0], 0x1b | 0x1d) {
                    if self.notification_handle == Some(u16le(data, 1)) {
                        if self.notifications.len() == 64 {
                            return Err(invalid("ATT notification queue overflow"));
                        }
                        self.notifications.push_back(response);
                    }
                    continue;
                }
                if data[0] == 1 {
                    if data.len() != 5 || data[1] != packet[0] || data[4] == 0 {
                        return Err(invalid("Malformed or mismatched ATT error response"));
                    }
                    let handle = u16le(data, 2);
                    if (handle != 0 || data[4] == 0x0a) && !(bounds.0..=bounds.1).contains(&handle)
                    {
                        return Err(invalid("ATT error handle outside requested range"));
                    }
                    return Ok(response);
                }
                if data[0] != packet[0] + 1 {
                    return Err(invalid("Unexpected ATT response opcode"));
                }
                if data[0] == 3 {
                    let peer = parse_mtu(data)?;
                    self.mtu.store(peer.min(LOCAL_MTU), Ordering::Relaxed);
                    self.negotiated.store(true, Ordering::Relaxed);
                }
                if data[0] == 0x13 && data.len() != 1 {
                    return Err(invalid("Malformed ATT write response"));
                }
                return Ok(response);
            }
        })();
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn discover(
        &mut self,
        opcode: u8,
        start: u16,
        end: u16,
        attribute: Option<u16>,
    ) -> Result<Option<Packet>> {
        let mut request = [0u8; 7];
        request[0] = opcode;
        request[1..3].copy_from_slice(&start.to_le_bytes());
        request[3..5].copy_from_slice(&end.to_le_bytes());
        let length = if let Some(attribute) = attribute {
            request[5..7].copy_from_slice(&attribute.to_le_bytes());
            7
        } else {
            5
        };
        let response = self.request(&request[..length], (start, end), false)?;
        if response.bytes[0] == 1 {
            if response.bytes[4] == 0x0a {
                return Ok(None);
            }
            return Err(invalid(format!(
                "ATT discovery error 0x{:02x} at0x{:04x}",
                response.bytes[4],
                u16le(response.data(), 2)
            )));
        }
        Ok(Some(response))
    }

    fn characteristics(&mut self) -> Result<Vec<Characteristic>> {
        let mut services = Vec::new();
        let mut start = 1u32;
        while start <= 65535 {
            let Some(packet) = self.discover(0x10, start as u16, 65535, Some(0x2800))? else {
                break;
            };
            let data = packet.data();
            let width = entries(data, &[6, 20])?;
            for row in data[2..].chunks_exact(width) {
                let first = u16le(row, 0);
                let last = u16le(row, 2);
                if (first as u32) < start || first > last {
                    return Err(invalid("Invalid or overlapping ATT primary service range"));
                }
                services.push((first, last, uuid(&row[4..])?));
                start = last as u32 + 1;
            }
        }
        let mut discovered = Vec::new();
        for (first, last, service) in services {
            if !SERVICES.contains(&service.as_str()) {
                continue;
            }
            let mut declarations: Vec<(u16, u8, u16, String)> = Vec::new();
            let mut start = first as u32;
            while start <= last as u32 {
                let Some(packet) = self.discover(8, start as u16, last, Some(0x2803))? else {
                    break;
                };
                let data = packet.data();
                let width = entries(data, &[7, 21])?;
                for row in data[2..].chunks_exact(width) {
                    let declaration = u16le(row, 0);
                    let properties = row[2];
                    let handle = u16le(row, 3);
                    if declaration <= first
                        || declaration >= handle
                        || handle > last
                        || (declaration as u32) < start
                        || declarations.last().is_some_and(|d| declaration <= d.2)
                    {
                        return Err(invalid(
                            "Invalid or overlapping ATT characteristic declaration",
                        ));
                    }
                    declarations.push((declaration, properties, handle, uuid(&row[5..])?));
                    start = declaration as u32 + 1;
                }
            }
            for (index, (_, properties, handle, id)) in declarations.iter().enumerate() {
                let end = declarations.get(index + 1).map_or(last, |d| d.0 - 1);
                discovered.push(Characteristic {
                    service: service.clone(),
                    uuid: id.clone(),
                    handle: *handle,
                    end,
                    properties: *properties,
                });
            }
        }
        if !self.negotiated.load(Ordering::Relaxed) {
            self.request(&[2, 250, 0], (0, 0), false)?;
        }
        Ok(discovered)
    }

    fn subscribe(&mut self, characteristic: &Characteristic) -> Result<u16> {
        if characteristic.properties & 0x10 == 0 {
            return Err(invalid("Configuration characteristic cannot notify"));
        }
        let mut start = characteristic.handle as u32 + 1;
        let mut cccd = None;
        while start <= characteristic.end as u32 {
            let Some(packet) = self.discover(4, start as u16, characteristic.end, None)? else {
                break;
            };
            let data = packet.data();
            if data.len() < 2 || !matches!(data[1], 1 | 2) {
                return Err(invalid("Malformed ATT descriptor format"));
            }
            let width = if data[1] == 1 { 4 } else { 18 };
            if data.len() == 2 || (data.len() - 2) % width != 0 {
                return Err(invalid("Malformed ATT descriptor rows"));
            }
            for row in data[2..].chunks_exact(width) {
                let handle = u16le(row, 0);
                if (handle as u32) < start || handle > characteristic.end {
                    return Err(invalid("Descriptor outside characteristic range"));
                }
                if uuid(&row[2..])? == "00002902-0000-1000-8000-00805f9b34fb" {
                    if cccd.replace(handle).is_some() {
                        return Err(invalid("Multiple CCCDs in one characteristic"));
                    }
                }
                start = handle as u32 + 1;
            }
        }
        let cccd = cccd.ok_or_else(|| invalid("Notification characteristic has no CCCD"))?;
        self.notification_handle = Some(characteristic.handle);
        let response = self.request(
            &[0x12, cccd as u8, (cccd >> 8) as u8, 1, 0],
            (cccd, cccd),
            false,
        )?;
        if response.bytes[0] == 1 {
            return Err(invalid("ATT subscription rejected"));
        }
        Ok(cccd)
    }

    fn unsubscribe(&mut self, cccd: u16) -> Result<()> {
        let response = self.request(
            &[0x12, cccd as u8, (cccd >> 8) as u8, 0, 0],
            (cccd, cccd),
            true,
        )?;
        if response.bytes[0] == 1 {
            return Err(invalid("ATT unsubscription rejected"));
        }
        self.notification_handle = None;
        self.notifications.clear();
        Ok(())
    }

    fn write(
        &mut self,
        characteristic: &Characteristic,
        value: &[u8],
        ignore_stop: bool,
    ) -> Result<()> {
        if characteristic.properties & 0x0c == 0 {
            return Err(invalid("Configuration characteristic not writable"));
        }
        if value.len() > self.mtu.load(Ordering::Relaxed) as usize - 3 {
            return Err(invalid("ATT MTU cannot carry native command"));
        }
        let without_response = characteristic.properties & 4 != 0;
        let mut wire = [0; 518];
        wire[0] = if without_response { 0x52 } else { 0x12 };
        wire[1..3].copy_from_slice(&characteristic.handle.to_le_bytes());
        wire[3..3 + value.len()].copy_from_slice(value);
        if without_response {
            if !ignore_stop {
                self.check_stop()?;
            }
            send(
                self.socket.as_raw_fd(),
                &wire[..value.len() + 3],
                self.timeout,
            )?;
        } else {
            let response = self.request(
                &wire[..value.len() + 3],
                (characteristic.handle, characteristic.handle),
                ignore_stop,
            )?;
            if response.bytes[0] == 1 {
                return Err(invalid("ATT write rejected"));
            }
        }
        Ok(())
    }

    fn notification(&mut self, timeout: Duration) -> Result<Option<Packet>> {
        self.notification_with_stop(timeout, false)
    }

    fn notification_with_stop(&mut self, timeout: Duration, ignore_stop: bool) -> Result<Option<Packet>> {
        if let Some(packet) = self.notifications.pop_front() {
            return Ok(Some(packet));
        }
        let deadline = Instant::now() + timeout;
        while let Some(packet) = self.incoming(deadline, ignore_stop)? {
            let data = packet.data();
            if !matches!(data[0], 0x1b | 0x1d) {
                self.abort();
                return Err(invalid("Unsolicited ATT response"));
            }
            if self.notification_handle == Some(u16le(data, 1)) {
                return Ok(Some(packet));
            }
        }
        Ok(None)
    }
}

#[cfg(target_os = "linux")]
impl Drop for AttClient {
    fn drop(&mut self) {
        self.abort();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn parse_mtu(data: &[u8]) -> Result<u16> {
    if data.len() != 3 {
        return Err(invalid("Malformed ATT MTU exchange"));
    }
    let mtu = u16le(data, 1);
    if !(23..=517).contains(&mtu) {
        return Err(invalid("Invalid peer ATT MTU"));
    }
    Ok(mtu)
}
fn entries(data: &[u8], widths: &[usize]) -> Result<usize> {
    if data.len() < 2
        || !widths.contains(&(data[1] as usize))
        || data.len() == 2
        || (data.len() - 2) % (data[1] as usize) != 0
    {
        return Err(invalid("Malformed ATT discovery entries"));
    }
    Ok(data[1] as usize)
}

// Android 2.00: 0x27280 caps a 3E frame at 62 - 19 = 43 payload bytes;
// 0x2aed0 supplies opcode 1/parameter 0/total 868, 0x2c0b0 commits with 6/0x67.
fn profile_write_frame(image: &[u8; protocol::CONFIG_SIZE], offset: usize, crc: bool) -> Result<[u8; protocol::PACKET_SIZE]> {
    if offset >= image.len() {
        return Err(invalid("Profile write offset outside image"));
    }
    let payload = &image[offset..(offset + 43).min(image.len())];
    let mut packet = protocol::report_packet(false, crc);
    packet[1] = (17 + payload.len()) as u8;
    packet[3..7].copy_from_slice(&[1, 0, 0, 0]);
    packet[7..11].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    if crc {
        let mut checksum = 0xffffu16;
        for &byte in payload {
            checksum ^= u16::from(byte);
            for _ in 0..8 {
                checksum = (checksum >> 1) ^ if checksum & 1 != 0 { 0xa001 } else { 0 };
            }
        }
        packet[9..11].copy_from_slice(&checksum.to_le_bytes());
    }
    packet[11..15].copy_from_slice(&(image.len() as u32).to_le_bytes());
    packet[15..19].copy_from_slice(&(offset as u32).to_le_bytes());
    packet[19..19 + payload.len()].copy_from_slice(payload);
    Ok(packet)
}

fn profile_commit_frame(crc: bool) -> [u8; protocol::PACKET_SIZE] {
    let mut packet = protocol::report_packet(false, crc);
    packet[3..7].copy_from_slice(&[6, 0, 0x67, 0]);
    packet
}

/// RX starts at byte 2, not TX's byte 3. Nonmatching notifications do not advance writes.
fn profile_write_ack(data: &[u8], opcode: u16, count: u32, total: u32, offset: u32, crc: bool) -> Result<bool> {
    if data.len() < 6 || u16le(data, 4) & 0xff != opcode {
        return Ok(false);
    }
    if data.len() < 18 {
        return Err(invalid("Truncated Bluetooth write acknowledgment"));
    }
    let word = |at| u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    let received_count = if crc { word(6) & 0xff } else { word(6) };
    // Live 3E (2026-10-15): the commit (opcode 6, total 0) is acknowledged with total 868.
    let total_ok = word(10) == total || (opcode == 6 && total == 0 && word(10) == protocol::CONFIG_SIZE as u32);
    if u16le(data, 2) != 4 || u16le(data, 4) != opcode
        || received_count != count || !total_ok || word(14) != offset {
        let hex: String = data.iter().take(24).map(|byte| format!("{byte:02x}")).collect();
        return Err(invalid(format!(
            "Bluetooth write acknowledgment mismatch for opcode 0x{opcode:x} (expected count {count}, total {total}, offset {offset}); reply {hex}"
        )));
    }
    Ok(true)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub struct Connection {
    pub address: String,
    characteristic: Characteristic,
    client: AttClient,
    crc: bool,
    #[cfg(target_os = "linux")]
    cccd: u16,
}
#[cfg(target_os = "linux")]
impl Connection {
    pub fn open(
        address: Option<&str>,
        timeout: Duration,
        stopped: Arc<AtomicBool>,
    ) -> Result<Self> {
        let mut radios = scan(timeout, &stopped)?;
        if let Some(address) = address {
            sockaddr(address, "public")?;
            radios.retain(|d| d.address.eq_ignore_ascii_case(address));
        }
        if radios.len() != 1 {
            return Err(invalid(if radios.is_empty() {
                "No 81HF01 configuration advertisement. Hold the front pixel-logo/profile button for three seconds until configuration LEDs blink; close the mobile app."
            } else {
                "Multiple configuration radios; select --address from scan"
            }));
        }
        let radio = radios.pop().unwrap();
        let mut client = AttClient::connect(&radio, timeout, stopped)?;
        // Receiver remains active during startup, including duplicate peer MTU requests.
        thread::sleep(Duration::from_millis(500));
        let characteristics = client.characteristics()?;
        let mut selected = None;
        for (service, id, crc) in TRANSPORTS {
            if let Some(c) = characteristics
                .iter()
                .find(|c| c.service == service && c.uuid == id)
            {
                selected = Some((c.clone(), crc));
                break;
            }
        }
        let (characteristic, crc) =
            selected.ok_or_else(|| invalid("No supported Ultimate application characteristic"))?;
        if characteristic.properties & 0x10 == 0
            || characteristic.properties & 0x0c == 0
            || client.mtu.load(Ordering::Relaxed) < 65
        {
            return Err(invalid(
                "Configuration characteristic lacks notification/write/62-byte MTU support",
            ));
        }
        let cccd = client.subscribe(&characteristic)?;
        Ok(Self {
            address: radio.address,
            client,
            characteristic,
            crc,
            cccd,
        })
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl Connection {

    fn settle(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            self.client.check_stop()?;
            thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    /// Returns a complete image only. Reporting restoration executes on every path.
    pub fn read_profiles(&mut self) -> Result<Vec<u8>> {
        self.settle()?;
        let result = crate::profile::checked_read(|| -> Result<Vec<u8>> {
            self.client.write(
                &self.characteristic,
                &protocol::report_packet(false, self.crc),
                false,
            )?;
            let mut transfer = protocol::ConfigRead::new(self.crc);
            while !transfer.complete() {
                self.client
                    .write(&self.characteristic, &transfer.request()?, false)?;
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    let packet = self
                        .client
                        .notification(deadline.saturating_duration_since(Instant::now()))?
                        .ok_or_else(|| timeout_error("Profile read timed out"))?;
                    let data = packet.application_data();
                    if data.len() >= 18
                        && matches!(u16le(data, 2), 1 | 4 | 5)
                        && u16le(data, 4) & 0xff == 2
                        && u32::from_le_bytes(data[10..14].try_into().unwrap())
                            == protocol::CONFIG_SIZE as u32
                    {
                        transfer.accept(data)?;
                        break;
                    }
                }
            }
            Ok(transfer.data().to_vec())
        });
        if let Err(error) = self.client.write(
            &self.characteristic,
            &protocol::report_packet(true, self.crc),
            true,
        ) {
            return Err(invalid(format!(
                "Could not restore controller reporting; reconnect controller: {error}. Read result: {}",
                result
                    .as_ref()
                    .err()
                    .map_or_else(|| "complete".into(), |e| e.to_string())
            )));
        }
        result
    }

    fn acknowledged(&mut self, frame: &[u8; protocol::PACKET_SIZE], ignore_stop: bool) -> Result<()> {
        self.client.write(&self.characteristic, frame, ignore_stop)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let packet = self.client
                .notification_with_stop(deadline.saturating_duration_since(Instant::now()), ignore_stop)?
                .ok_or_else(|| timeout_error(&format!("Bluetooth acknowledgment timed out (opcode 0x{:x}, offset {})", u16le(frame, 3), u32::from_le_bytes(frame[15..19].try_into().unwrap()))))?;
            if profile_write_ack(packet.application_data(), u16le(frame, 3), u32::from(frame[7]),
                u32::from_le_bytes(frame[11..15].try_into().unwrap()),
                u32::from_le_bytes(frame[15..19].try_into().unwrap()), self.crc)? {
                return Ok(());
            }
        }
    }

    /// Verified Android full-write/commit protocol, shared across transports.
    /// Transfers the supplied image unchanged: the on-device CRC covers bytes 4..867.
    /// Android's unrelated all-868 pre-send CRC rewrite is deliberately not reproduced.
    pub fn write_profiles(&mut self, image: &[u8; protocol::CONFIG_SIZE]) -> Result<()> {
        if !crate::profile::verify_crc(image) {
            return Err(invalid("Profile image checksum is invalid; Bluetooth write refused"));
        }
        self.settle()?;
        self.client.clear_notifications();
        let result = (|| {
            self.acknowledged(&protocol::report_packet(false, self.crc), false)?;
            for offset in (0..image.len()).step_by(43) {
                self.acknowledged(&profile_write_frame(image, offset, self.crc)?, false)?;
            }
            self.acknowledged(&profile_commit_frame(self.crc), false)
        })();
        // Establish cleanup before disabling: a failed send may already have reached firmware.
        if let Err(error) = self.acknowledged(&protocol::report_packet(true, self.crc), true) {
            return Err(invalid(format!(
                "Could not restore controller reporting; reconnect controller: {error}. Write result: {}",
                result.as_ref().err().map_or_else(|| "complete".into(), |e| e.to_string())
            )));
        }
        result
    }

    pub fn enable_physical(&mut self) -> Result<protocol::PhysicalInput> {
        if self.characteristic.uuid != TRANSPORTS[2].1 || self.crc {
            return Err(invalid("Physical input verified only on 3E 2B11 endpoint"));
        }
        self.settle()?;
        self.client.clear_notifications();
        self.client.write(
            &self.characteristic,
            &protocol::report_packet(true, false),
            false,
        )?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(state) =
                self.next_physical(deadline.saturating_duration_since(Instant::now()))?
            {
                return Ok(state);
            }
            if Instant::now() >= deadline {
                return Err(timeout_error(
                    "No physical-button report received from configuration BLE",
                ));
            }
        }
    }

    pub fn next_physical(&mut self, timeout: Duration) -> Result<Option<protocol::PhysicalInput>> {
        let deadline = Instant::now() + timeout;
        while let Some(packet) = self
            .client
            .notification(deadline.saturating_duration_since(Instant::now()))?
        {
            if let Some(state) = protocol::physical_input(packet.application_data())? {
                return Ok(Some(state));
            }
        }
        Ok(None)
    }

    #[cfg(target_os = "linux")]
    pub fn close(&mut self) -> Result<()> {
        if self.client.alive.load(Ordering::Relaxed) {
            self.client.unsubscribe(self.cccd)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
/// Own both input sources; a failure on either source tears down the virtual pad.
pub fn run_controller(
    device: Option<&std::path::Path>,
    profile: &crate::input::HostProfile,
    slot_map: Option<crate::profile::KeyMap>,
    address: Option<&str>,
    timeout: Duration,
    stopped: Arc<AtomicBool>,
) -> Result<()> {
    // The CLI also selects receiver/wired devices; their runtime validates link policy.
    let controllers = crate::input::devices()?;
    let candidates: Vec<_> = controllers
        .iter()
        .filter(|c| device.is_none_or(|d| c.path == d))
        .collect();
    if candidates.len() != 1 {
        return Err(invalid(if candidates.is_empty() {
            "Connect an Ultimate 3E controller; no matching input device"
        } else {
            "Multiple controllers; select --device"
        }));
    }
    let device = candidates[0].path.clone();
    match candidates[0].link {
        crate::input::LinkKind::Receiver => return crate::input::run_receiver(Some(&device), profile, slot_map, stopped),
        crate::input::LinkKind::Wired => return crate::input::run_wired(Some(&device), profile, stopped),
        crate::input::LinkKind::Bluetooth => profile.validate()?,
    }
    let mut connection = Connection::open(address, timeout, stopped.clone())?;
    let slot_map = if slot_map.is_none() {
        let bytes = connection.read_profiles()?;
        let image: &[u8; crate::profile::IMAGE_SIZE] = bytes.as_slice().try_into()
            .map_err(|_| invalid("Bluetooth returned incomplete profiles"))?;
        crate::profile::Profiles::decode(image)?.selected_slot().map(|slot| slot.key_map.clone())
    } else { slot_map };
    let first = connection.enable_physical()?;
    let (sender, receiver) = mpsc::sync_channel(32);
    sender.send(first)?;
    let input_profile = profile.clone();
    let input_stop = stopped.clone();
    let worker = thread::spawn(move || {
        crate::input::run(Some(&device), &input_profile, slot_map, receiver, input_stop)
    });
    let mut radio_error = None;
    while !worker.is_finished() && !stopped.load(Ordering::Relaxed) && !interrupted() {
        match connection.next_physical(Duration::from_millis(100)) {
            Ok(Some(state)) => {
                if let Err(error) = sender.try_send(state) {
                    if !worker.is_finished() {
                        radio_error =
                            Some(invalid(format!("Physical input consumer stalled: {error}")));
                    }
                    break;
                }
            }
            Ok(None) => (),
            Err(error) => {
                radio_error = Some(error);
                break;
            }
        }
    }
    stopped.store(true, Ordering::Relaxed);
    drop(sender);
    let input_result = worker
        .join()
        .map_err(|_| invalid("Input worker panicked"))?;
    let close_result = connection.close();
    if let Some(error) = radio_error {
        return Err(error);
    }
    input_result?;
    close_result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advertisements_merge_scan_responses_without_erasing_identity() {
        let mut advertisements = Advertisements::default();
        advertisements.observe(0xaabbccddeeff, "random", "");
        advertisements.observe(0xaabbccddeeff, "random", "81HF01");
        advertisements.observe(0xaabbccddeeff, "unknown", "");
        advertisements.observe(0x112233445566, "public", "other");
        advertisements.observe(0x102030405060, "public", "81HF01");
        advertisements.observe(0, "unknown", "81HF01");
        let radios = advertisements.radios();
        assert_eq!(radios.len(), 2);
        assert_eq!(radios[0].address, "10:20:30:40:50:60");
        assert_eq!(radios[0].address_type, "public");
        assert_eq!(radios[1].address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(radios[1].address_type, "random");
    }
    #[test]
    fn windows_bluetooth_address_checks_every_octet() {
        assert_eq!(bluetooth_address("aA:BB:cc:DD:ee:FF").unwrap(), 0xaabbccddeeff);
        for bad in ["", "AA:BB:CC:DD:EE", "AA:BB:CC:DD:EE:FF:00", "A:BB:CC:DD:EE:FF", "AA:BB:CC:DD:EE:GG"] {
            assert!(bluetooth_address(bad).is_err(), "{bad}");
        }
    }
    fn padded(hex: &str) -> [u8; protocol::PACKET_SIZE] {
        let bytes = protocol::decode_hex(hex).unwrap();
        let mut out = [0; protocol::PACKET_SIZE];
        out[..bytes.len()].copy_from_slice(&bytes);
        out
    }
    #[test]
    fn mobile_full_write_frames_match_recovered_fields_and_chunk_bounds() {
        // Static vectors from APK 0x27280/0x28080/0x2aed0, not hardware captures.
        let image = [0; protocol::CONFIG_SIZE];
        assert_eq!(profile_write_frame(&image, 0, false).unwrap(),
            padded("813c04010000002b0000006403000000000000"));
        assert_eq!(profile_write_frame(&image, 43, false).unwrap(),
            padded("813c04010000002b000000640300002b000000"));
        assert_eq!(profile_write_frame(&image, 860, false).unwrap(),
            padded("8119040100000008000000640300005c030000"));
        assert_eq!(profile_write_frame(&image, 0, true).unwrap(),
            padded("813c04010000002b0043546403000000000000"));
        assert_eq!(profile_write_frame(&image, 860, true).unwrap(),
            padded("811904010000000800400b640300005c030000"));
        assert_eq!(profile_commit_frame(false),
            padded("81110406006700000000000000000000000000"));
        assert_eq!(profile_commit_frame(true),
            padded("811104060067000000ffff0000000000000000"));
        assert!(profile_write_frame(&image, image.len(), false).is_err());
        let image = std::array::from_fn(|i| i as u8);
        for offset in (0..image.len()).step_by(43) {
            let frame = profile_write_frame(&image, offset, false).unwrap();
            let n = (image.len() - offset).min(43);
            assert_eq!(&frame[19..19+n], &image[offset..offset+n]);
            assert!(frame[19+n..].iter().all(|&b| b == 0));
        }
    }
    #[test]
    fn mobile_write_ack_requires_exact_kind_command_count_total_and_offset() {
        // RX contract recovered from JNI 0x28c90 + handleChannel 0x28810.
        let ack = protocol::decode_hex("8111040001002b000000640300002b000000").unwrap();
        assert!(profile_write_ack(&ack, 1, 43, 868, 43, false).unwrap());
        assert!(!profile_write_ack(&ack, 6, 0, 0, 0, false).unwrap());
        for index in [2, 3, 5, 6, 7, 8, 9, 10, 14] {
            let mut bad = ack.clone();bad[index] ^= 1;
            assert!(profile_write_ack(&bad, 1, 43, 868, 43, false).is_err(), "byte {index}");
        }
        assert!(profile_write_ack(&ack[..17], 1, 43, 868, 43, false).is_err());
        let mut type1 = ack.clone();type1[8..10].copy_from_slice(&[0x12, 0x34]);
        assert!(profile_write_ack(&type1, 1, 43, 868, 43, true).unwrap());
        let commit = protocol::decode_hex("811104000600000000000000000000000000").unwrap();
        assert!(profile_write_ack(&commit, 6, 0, 0, 0, false).unwrap());
        // Exact live 3E commit acknowledgment (total echoed as 868).
        let live = protocol::decode_hex("811104000600000000006403000000000000").unwrap();
        assert!(profile_write_ack(&live, 6, 0, 0, 0, false).unwrap());
        let mut wrong = live.clone();wrong[10] = 0x65;
        assert!(profile_write_ack(&wrong, 6, 0, 0, 0, false).is_err());
        assert!(!profile_write_ack(&[0x81,0x1c,4,0,3,0], 1, 43, 868, 0, false).unwrap());
    }
    #[cfg(target_os = "linux")]
    fn pair() -> (AttClient, OwnedFd) {
        let mut fds = [-1; 2];
        assert_eq!(
            unsafe {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                    0,
                    fds.as_mut_ptr(),
                )
            },
            0
        );
        let local = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let peer = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        let flags = unsafe { libc::fcntl(local.as_raw_fd(), libc::F_GETFL) };
        assert_eq!(
            unsafe { libc::fcntl(local.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        (
            AttClient::from_socket(
                local,
                Duration::from_secs(1),
                Arc::new(AtomicBool::new(false)),
            ),
            peer,
        )
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn full_write_is_ack_gated_and_restores_reports_after_error_or_cancellation() {
        for scenario in 0..3 {
            let (mut client, peer) = pair();
            client.mtu.store(250, Ordering::Relaxed);
            client.notification_handle = Some(17);
            let stopped = Arc::clone(&client.stopped);
            let mut connection = Connection {
                address: "socket-peer".into(), client,
                characteristic: Characteristic {
                    service: SERVICES[1].into(), uuid: TRANSPORTS[2].1.into(),
                    handle: 17, end: 19, properties: 0x1c,
                },
                crc: false, cccd: 18,
            };
            let worker = thread::spawn(move || {
                let fd = peer.as_raw_fd();
                poll_fd(fd, libc::POLLIN, Instant::now() + Duration::from_secs(5)).unwrap();
                let mut chunks = 0;let mut committed = false;
                loop {
                    let frame = receive(fd);
                    assert_eq!(&frame[..3], &[0x52,17,0]);
                    let command = &frame[3..];
                    let opcode = u16le(command, 3);
                    let restoring = opcode == 7 && u16le(command, 5) == 0x110;
                    if opcode == 1 {
                        assert_eq!(u32::from_le_bytes(command[15..19].try_into().unwrap()), chunks * 43);
                        chunks += 1;
                    }
                    if opcode == 6 {
                        assert_eq!(chunks, 21);assert_eq!(u16le(command, 5), 0x67);
                        committed = true;
                    }
                    let mut reply = vec![0x1b,17,0,0x81,0x11,4,0];
                    reply.extend_from_slice(&opcode.to_le_bytes());
                    reply.extend_from_slice(&command[7..19]);
                    if opcode == 1 && scenario == 1 { reply[9] += 1; }
                    if opcode == 1 && scenario == 2 { stopped.store(true, Ordering::Relaxed); }
                    send(fd, &reply, Duration::from_secs(1)).unwrap();
                    if restoring {
                        assert_eq!(committed, scenario == 0);
                        assert!(chunks > 0);
                        return;
                    }
                }
            });
            let image = crate::gui::state::defaults().unwrap().encode();
            assert_eq!(connection.write_profiles(&image).is_ok(), scenario == 0);
            worker.join().unwrap();
        }
    }
    #[cfg(target_os = "linux")]
    fn receive(fd: i32) -> Vec<u8> {
        poll_fd(fd, libc::POLLIN, Instant::now() + Duration::from_secs(2)).unwrap();
        let mut b = [0; 518];
        let n = unsafe { libc::recv(fd, b.as_mut_ptr().cast(), 518, 0) };
        assert!(n > 0);
        b[..n as usize].to_vec()
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn duplicate_peer_mtu_does_not_consume_pending_discovery() {
        let (mut client, peer) = pair();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let fd = peer.as_raw_fd();
            send(fd, &[2, 250, 0], Duration::from_secs(1)).unwrap();
            send(fd, &[2, 250, 0], Duration::from_secs(1)).unwrap();
            assert_eq!(receive(fd), [3, 250, 0]);
            assert_eq!(receive(fd), [3, 250, 0]);
            ready_tx.send(()).unwrap();
            assert_eq!(receive(fd), [0x10, 1, 0, 255, 255, 0, 40]);
            send(fd, &[2, 100, 0], Duration::from_secs(1)).unwrap();
            assert_eq!(receive(fd), [3, 250, 0]);
            send(fd, &[0x11, 6, 1, 0, 5, 0, 0, 24], Duration::from_secs(1)).unwrap();
            done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let response = client
            .discover(0x10, 1, 65535, Some(0x2800))
            .unwrap()
            .unwrap();
        assert_eq!(response.data(), [0x11, 6, 1, 0, 5, 0, 0, 24]);
        assert_eq!(client.mtu.load(Ordering::Relaxed), 100);
        done_tx.send(()).unwrap();
        worker.join().unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn malformed_response_closes_bearer() {
        let (mut client, peer) = pair();
        let worker = thread::spawn(move || {
            receive(peer.as_raw_fd());
            send(peer.as_raw_fd(), &[0x13, 0], Duration::from_secs(1)).unwrap();
        });
        assert!(client.request(&[0x12, 1, 0, 0], (1, 1), false).is_err());
        assert!(!client.alive.load(Ordering::Relaxed));
        worker.join().unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn addresses_are_checked_before_socket_creation() {
        assert!(sockaddr("11:22:33:44:55:GG", "public").is_err());
        assert!(sockaddr("11:22:33:44:55:66", "invalid").is_err());
        assert_eq!(
            &sockaddr("11:22:33:44:55:66", "random").unwrap()[4..13],
            &[0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 4, 0, 2]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn local_mtu_response_applies_before_immediate_indication() {
        let (mut client, peer) = pair();
        client.notification_handle = Some(17);
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let fd = peer.as_raw_fd();
            assert_eq!(receive(fd), [2, 250, 0]);
            send(fd, &[3, 250, 0], Duration::from_secs(1)).unwrap();
            let mut indication = [0u8; 67];
            indication[..3].copy_from_slice(&[0x1d, 17, 0]);
            send(fd, &indication, Duration::from_secs(1)).unwrap();
            assert_eq!(receive(fd), [0x1e]);
            done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        send(
            client.socket.as_raw_fd(),
            &[2, 250, 0],
            Duration::from_secs(1),
        )
        .unwrap();
        let response = client
            .incoming(Instant::now() + Duration::from_secs(1), false)
            .unwrap()
            .unwrap();
        assert_eq!(response.data(), [3, 250, 0]);
        let indication = client
            .notification(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        assert_eq!(indication.length, 67);
        assert_eq!(client.mtu.load(Ordering::Relaxed), 250);
        done_tx.send(()).unwrap();
        worker.join().unwrap();
    }
}
