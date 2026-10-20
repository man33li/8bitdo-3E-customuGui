//! Blocking-worker WinRT GATT transport; application framing stays in att.rs.
use super::{Advertisements, Connection, Radio, TRANSPORTS, bluetooth_address as address, timeout_error};
use crate::{Result, interrupted, invalid, protocol};
use std::{
    io,
    marker::PhantomData,
    path::Path,
    sync::{Arc, atomic::{AtomicBool, AtomicU16, Ordering}, mpsc::{self, Receiver, RecvTimeoutError}},
    thread,
    time::{Duration, Instant},
};
use ::windows::{
    core::GUID,
    Devices::Bluetooth::{BluetoothAddressType, BluetoothCacheMode, BluetoothError, BluetoothLEDevice},
    Devices::Bluetooth::Advertisement::{BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher, BluetoothLEAdvertisementWatcherStoppedEventArgs, BluetoothLEScanningMode},
    Devices::Bluetooth::GenericAttributeProfile::{GattCharacteristic, GattCharacteristicProperties, GattClientCharacteristicConfigurationDescriptorValue as Cccd, GattCommunicationStatus, GattDeviceService, GattSession, GattSessionStatus, GattSessionStatusChangedEventArgs, GattValueChangedEventArgs, GattWriteOption},
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, DataWriter},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
};

// windows-future is deliberately transitive. Keep its operation type inferred.
// Poll before get(): get itself has no deadline; Cancel may complete asynchronously.
macro_rules! complete {
    ($operation:expr, $timeout:expr, $stopped:expr, $ignore_stop:expr) => {{
        (|| -> Result<_> {
            let operation = $operation?;
            let deadline = Instant::now() + $timeout;
            while operation.Status()?.0 == 0 { // AsyncStatus::Started
                if !$ignore_stop && ($stopped.load(Ordering::Relaxed) || interrupted()) {
                    let _ = operation.Cancel();
                    return Err(cancelled());
                }
                if Instant::now() >= deadline {
                    let _ = operation.Cancel();
                    return Err(timeout_error("Windows Bluetooth operation timed out; cancellation requested"));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(operation.get()?)
        })()
    }};
}

struct Apartment(PhantomData<*mut ()>);
impl Apartment {
    fn new() -> Result<Self> {
        // Each connection stays on its owning blocking MTA worker.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)?; }
        Ok(Self(PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) { unsafe { RoUninitialize(); } }
}
fn cancelled() -> crate::Error {
    io::Error::new(io::ErrorKind::Interrupted, "Bluetooth operation interrupted").into()
}
fn check_stop(stopped: &AtomicBool) -> Result<()> {
    if stopped.load(Ordering::Relaxed) || interrupted() { Err(cancelled()) } else { Ok(()) }
}
fn valid_timeout(timeout: Duration) -> Result<()> {
    if timeout.is_zero() || timeout > Duration::from_secs(300) {
        Err(invalid("Bluetooth timeout must be between 0 and 300 seconds"))
    } else { Ok(()) }
}
fn gatt_status(status: GattCommunicationStatus, protocol_error: Option<u8>, action: &str) -> Result<()> {
    if status == GattCommunicationStatus::Success { return Ok(()); }
    Err(invalid(format!("Windows Bluetooth {action} failed: {status:?}, ATT protocol error {protocol_error:?}; close competing apps and check Bluetooth access")))
}

pub fn scan(timeout: Duration, stopped: &AtomicBool) -> Result<Vec<Radio>> {
    valid_timeout(timeout)?;
    check_stop(stopped)?;
    let _apartment = Apartment::new()?;
    let watcher = BluetoothLEAdvertisementWatcher::new()?;
    watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;
    enum Observation { Advertisement(u64, &'static str, String), Stopped(BluetoothError) }
    let (sender, receiver) = mpsc::channel();
    let advertisement_sender = sender.clone();
    let received = watcher.Received(&TypedEventHandler::new(move |_, args: ::windows::core::Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
        let Some(args) = args.as_ref() else { return Ok(()); };
        let kind = match args.BluetoothAddressType()? {
            BluetoothAddressType::Public => "public",
            BluetoothAddressType::Random => "random",
            _ => "unknown",
        };
        let _ = advertisement_sender.send(Observation::Advertisement(args.BluetoothAddress()?, kind, args.Advertisement()?.LocalName()?.to_string()));
        Ok(())
    }))?;
    let stopped_token = match watcher.Stopped(&TypedEventHandler::new(move |_, args: ::windows::core::Ref<BluetoothLEAdvertisementWatcherStoppedEventArgs>| {
        if let Some(args) = args.as_ref() { let _ = sender.send(Observation::Stopped(args.Error()?)); }
        Ok(())
    })) {
        Ok(token) => token,
        Err(error) => { let _ = watcher.RemoveReceived(received); return Err(error.into()); }
    };
    let result = (|| {
        watcher.Start()?;
        let deadline = Instant::now() + timeout;
        let mut advertisements = Advertisements::default();
        while Instant::now() < deadline {
            check_stop(stopped)?;
            match receiver.recv_timeout(Duration::from_millis(50).min(deadline.saturating_duration_since(Instant::now()))) {
                Ok(Observation::Advertisement(address, kind, name)) => advertisements.observe(address, kind, &name),
                Ok(Observation::Stopped(error)) => return Err(invalid(format!("Windows Bluetooth discovery stopped: {error:?}"))),
                Err(RecvTimeoutError::Timeout) => (),
                Err(RecvTimeoutError::Disconnected) => return Err(invalid("Bluetooth discovery event channel closed")),
            }
        }
        check_stop(stopped)?;
        Ok(advertisements.radios())
    })();
    let stop = watcher.Stop();
    let remove_received = watcher.RemoveReceived(received);
    let remove_stopped = watcher.RemoveStopped(stopped_token);
    let radios = result?;
    stop?;
    remove_received?;
    remove_stopped?;
    Ok(radios)
}

struct Service(GattDeviceService);
impl Drop for Service {
    fn drop(&mut self) { let _ = self.0.Close(); }
}
pub(super) struct Characteristic {
    pub(super) uuid: String,
    value: GattCharacteristic,
    option: GattWriteOption,
}
pub(super) struct Packet {
    bytes: [u8; 514],
    length: usize,
}
impl Packet {
    pub(super) fn application_data(&self) -> &[u8] { &self.bytes[..self.length] }
}

pub struct AttClient {
    device: BluetoothLEDevice,
    session: Option<GattSession>,
    service: Option<Service>,
    subscription: Option<(GattCharacteristic, i64)>,
    status_token: Option<i64>,
    mtu_token: Option<i64>,
    receiver: Receiver<Result<Packet>>,
    sender: mpsc::Sender<Result<Packet>>,
    stopped: Arc<AtomicBool>,
    disconnected: Arc<AtomicBool>,
    mtu: Arc<AtomicU16>,
    timeout: Duration,
    closed: bool,
    // Last: WinRT objects are released before uninitializing the apartment.
    _apartment: Apartment,
}
impl AttClient {
    pub fn connect(radio: &Radio, timeout: Duration, stopped: Arc<AtomicBool>) -> Result<Self> {
        valid_timeout(timeout)?;
        check_stop(&stopped)?;
        let apartment = Apartment::new()?;
        let address = address(&radio.address)?;
        let kind = match radio.address_type.as_str() {
            "public" => BluetoothAddressType::Public,
            "random" => BluetoothAddressType::Random,
            _ => return Err(invalid("Unknown LE address type")),
        };
        let device = complete!(BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(address, kind), timeout, stopped, false)?;
        let (sender, receiver) = mpsc::channel();
        let mut client = Self {
            device, session: None, service: None, subscription: None, status_token: None, mtu_token: None,
            sender, receiver, stopped, disconnected: Arc::new(AtomicBool::new(false)),
            mtu: Arc::new(AtomicU16::new(23)), timeout, closed: false, _apartment: apartment,
        };
        let session = complete!(GattSession::FromDeviceIdAsync(&client.device.BluetoothDeviceId()?), timeout, client.stopped, false)?;
        client.session = Some(session.clone());
        let active = Arc::new(AtomicBool::new(session.SessionStatus()? == GattSessionStatus::Active));
        let disconnected = client.disconnected.clone();
        let sender = client.sender.clone();
        client.status_token = Some(session.SessionStatusChanged(&TypedEventHandler::new(move |_, args: ::windows::core::Ref<GattSessionStatusChangedEventArgs>| {
            if let Some(args) = args.as_ref() {
                if args.Status()? == GattSessionStatus::Active {
                    active.store(true, Ordering::Relaxed);
                } else if active.load(Ordering::Relaxed) || args.Error()? != BluetoothError::Success {
                    disconnected.store(true, Ordering::Relaxed);
                    let _ = sender.send(Err(invalid(format!("Windows Bluetooth session disconnected: {:?}", args.Error()?))));
                }
            }
            Ok(())
        }))?);
        let mtu = client.mtu.clone();
        client.mtu_token = Some(session.MaxPduSizeChanged(&TypedEventHandler::new(move |session: ::windows::core::Ref<GattSession>, _| {
            if let Some(session) = session.as_ref() { mtu.store(session.MaxPduSize()?, Ordering::Relaxed); }
            Ok(())
        }))?);
        session.SetMaintainConnection(true)?;
        client.mtu.store(session.MaxPduSize()?, Ordering::Relaxed);
        Ok(client)
    }

    fn characteristic(&mut self) -> Result<(Characteristic, bool)> {
        for (service_uuid, characteristic_uuid, crc) in TRANSPORTS {
            self.check_stop()?;
            let services = complete!(self.device.GetGattServicesForUuidWithCacheModeAsync(GUID::try_from(service_uuid)?, BluetoothCacheMode::Uncached), self.timeout, self.stopped, false)?;
            gatt_status(services.Status()?, services.ProtocolError().ok().and_then(|v| v.Value().ok()), "service discovery")?;
            let list = services.Services()?;
            if list.Size()? > 1 { return Err(invalid("Multiple matching Bluetooth configuration services")); }
            if list.Size()? == 0 { continue; }
            let service = Service(list.GetAt(0)?);
            let characteristics = complete!(service.0.GetCharacteristicsForUuidWithCacheModeAsync(GUID::try_from(characteristic_uuid)?, BluetoothCacheMode::Uncached), self.timeout, self.stopped, false)?;
            gatt_status(characteristics.Status()?, characteristics.ProtocolError().ok().and_then(|v| v.Value().ok()), "characteristic discovery")?;
            let list = characteristics.Characteristics()?;
            if list.Size()? > 1 { return Err(invalid("Multiple matching Bluetooth configuration characteristics")); }
            if list.Size()? == 0 { continue; }
            let value = list.GetAt(0)?;
            let properties = value.CharacteristicProperties()?;
            if !properties.contains(GattCharacteristicProperties::Notify) { return Err(invalid("Configuration characteristic lacks Notify")); }
            let option = if properties.contains(GattCharacteristicProperties::WriteWithoutResponse) {
                GattWriteOption::WriteWithoutResponse
            } else if properties.contains(GattCharacteristicProperties::Write) {
                GattWriteOption::WriteWithResponse
            } else { return Err(invalid("Configuration characteristic is not writable")); };
            self.service = Some(service);
            let deadline = Instant::now() + self.timeout;
            while self.mtu.load(Ordering::Relaxed) < (protocol::PACKET_SIZE + 3) as u16 {
                self.check_stop()?;
                if Instant::now() >= deadline {
                    return Err(invalid(format!("Unsupported Bluetooth link: negotiated ATT MTU {} cannot carry a native 62-byte command (requires at least 65)", self.mtu.load(Ordering::Relaxed))));
                }
                thread::sleep(Duration::from_millis(20));
            }
            return Ok((Characteristic { uuid: characteristic_uuid.into(), value, option }, crc));
        }
        Err(invalid("No supported Ultimate application characteristic"))
    }

    fn subscribe(&mut self, characteristic: &Characteristic) -> Result<()> {
        let sender = self.sender.clone();
        let token = characteristic.value.ValueChanged(&TypedEventHandler::new(move |_, args: ::windows::core::Ref<GattValueChangedEventArgs>| {
            let Some(args) = args.as_ref() else { return Ok(()); };
            let result = (|| -> Result<Packet> {
                let buffer = args.CharacteristicValue()?;
                let length = buffer.Length()? as usize;
                let mut packet = Packet { bytes: [0; 514], length };
                if length == 0 || length > packet.bytes.len() { return Err(invalid("Invalid Bluetooth application notification length")); }
                DataReader::FromBuffer(&buffer)?.ReadBytes(&mut packet.bytes[..length])?;
                Ok(packet)
            })();
            let _ = sender.send(result);
            Ok(())
        }))?;
        self.subscription = Some((characteristic.value.clone(), token));
        let result = complete!(characteristic.value.WriteClientCharacteristicConfigurationDescriptorWithResultAsync(Cccd::Notify), self.timeout, self.stopped, false)?;
        gatt_status(result.Status()?, result.ProtocolError().ok().and_then(|v| v.Value().ok()), "notification subscription")
    }

    pub(super) fn check_stop(&self) -> Result<()> {
        check_stop(&self.stopped)?;
        if self.closed || self.disconnected.load(Ordering::Relaxed) { return Err(invalid("Bluetooth configuration connection closed")); }
        Ok(())
    }
    pub(super) fn clear_notifications(&mut self) {
        while self.receiver.try_recv().is_ok() {}
    }
    pub(super) fn write(&mut self, characteristic: &Characteristic, value: &[u8], ignore_stop: bool) -> Result<()> {
        if !ignore_stop { self.check_stop()?; }
        if self.closed || self.disconnected.load(Ordering::Relaxed) { return Err(invalid("Bluetooth configuration connection closed")); }
        if value.len() + 3 > self.mtu.load(Ordering::Relaxed) as usize { return Err(invalid("ATT MTU cannot carry native command")); }
        let writer = DataWriter::new()?;
        writer.WriteBytes(value)?;
        let buffer = writer.DetachBuffer()?;
        let result = complete!(characteristic.value.WriteValueWithResultAndOptionAsync(&buffer, characteristic.option), self.timeout, self.stopped, ignore_stop)?;
        gatt_status(result.Status()?, result.ProtocolError().ok().and_then(|v| v.Value().ok()), "write")
    }
    pub(super) fn notification(&mut self, timeout: Duration) -> Result<Option<Packet>> {
        self.notification_with_stop(timeout, false)
    }
    pub(super) fn notification_with_stop(&mut self, timeout: Duration, ignore_stop: bool) -> Result<Option<Packet>> {
        let deadline = Instant::now() + timeout;
        loop {
            if !ignore_stop { self.check_stop()?; }
            if self.closed || self.disconnected.load(Ordering::Relaxed) { return Err(invalid("Bluetooth configuration connection closed")); }
            if let Ok(packet) = self.receiver.try_recv() { return packet.map(Some); }
            if Instant::now() >= deadline { return Ok(None); }
            match self.receiver.recv_timeout(Duration::from_millis(50).min(deadline.saturating_duration_since(Instant::now()))) {
                Ok(packet) => return packet.map(Some),
                Err(RecvTimeoutError::Timeout) => (),
                Err(RecvTimeoutError::Disconnected) => return Err(invalid("Bluetooth notification channel closed")),
            }
        }
    }
    fn close(&mut self) -> Result<()> {
        if self.closed { return Ok(()); }
        self.closed = true;
        let mut error = None;
        if let Some((characteristic, token)) = self.subscription.take() {
            if !self.disconnected.load(Ordering::Relaxed) {
                let result = (|| {
                    let result = complete!(characteristic.WriteClientCharacteristicConfigurationDescriptorWithResultAsync(Cccd::None), self.timeout, self.stopped, true)?;
                    gatt_status(result.Status()?, result.ProtocolError().ok().and_then(|v| v.Value().ok()), "notification unsubscription")
                })();
                if let Err(e) = result { error = Some(e); }
            }
            if let Err(e) = characteristic.RemoveValueChanged(token) { error.get_or_insert_with(|| e.into()); }
        }
        if let Some(session) = self.session.take() {
            if let Some(token) = self.status_token.take() {
                if let Err(e) = session.RemoveSessionStatusChanged(token) { error.get_or_insert_with(|| e.into()); }
            }
            if let Some(token) = self.mtu_token.take() {
                if let Err(e) = session.RemoveMaxPduSizeChanged(token) { error.get_or_insert_with(|| e.into()); }
            }
            if let Err(e) = session.SetMaintainConnection(false) { error.get_or_insert_with(|| e.into()); }
            self.service.take();
            if let Err(e) = session.Close() { error.get_or_insert_with(|| e.into()); }
        }
        if let Err(e) = self.device.Close() { error.get_or_insert_with(|| e.into()); }
        match error { Some(error) => Err(error), None => Ok(()) }
    }
}
impl Drop for AttClient {
    fn drop(&mut self) { let _ = self.close(); }
}
impl Connection {
    pub fn open(address_filter: Option<&str>, timeout: Duration, stopped: Arc<AtomicBool>) -> Result<Self> {
        if let Some(value) = address_filter { address(value)?; }
        let mut radios = scan(timeout, &stopped)?;
        if let Some(address) = address_filter { radios.retain(|radio| radio.address.eq_ignore_ascii_case(address)); }
        if radios.len() != 1 {
            return Err(invalid(if radios.is_empty() {
                "No 81HF01 configuration advertisement. Hold the front pixel-logo/profile button for three seconds until configuration LEDs blink; close the mobile app."
            } else { "Multiple configuration radios; select --address from scan" }));
        }
        let radio = radios.pop().unwrap();
        let mut client = AttClient::connect(&radio, timeout, stopped)?;
        let (characteristic, crc) = client.characteristic()?;
        client.subscribe(&characteristic)?;
        Ok(Self { address: radio.address, client, characteristic, crc })
    }
    pub fn close(&mut self) -> Result<()> { self.client.close() }
}

pub fn run_controller(_device: Option<&Path>, _profile: &crate::input::HostProfile, _slot_map: Option<crate::profile::KeyMap>, _address: Option<&str>, _timeout: Duration, _stopped: Arc<AtomicBool>) -> Result<()> {
    Err(invalid("Bluetooth host mapper not yet supported on Windows; Bluetooth profile configuration and physical notifications are available"))
}
