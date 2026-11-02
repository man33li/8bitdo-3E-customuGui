//! Test-page adapters. Wired input is observed without taking an evdev grab.
use crate::{Result, invalid, input::{AxisInfo, Event}, profile::PhysicalButtons as Keys, protocol::PhysicalInput, receiver::Pad};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::{fs::{File, OpenOptions}, io::{self, Read}, mem::size_of, os::{fd::AsRawFd, unix::fs::{FileTypeExt, OpenOptionsExt}}};

pub(crate) fn bluetooth(state:PhysicalInput)->(Pad,u32){
    let mut buttons=0;
    for (physical,xinput) in [(Keys::UP,1),(Keys::DOWN,2),(Keys::LEFT,4),(Keys::RIGHT,8),(Keys::MENU,16),(Keys::VIEW,32),(Keys::L3,64),(Keys::R3,128),(Keys::LB,256),(Keys::RB,512),(Keys::A,4096),(Keys::B,8192),(Keys::X,16384),(Keys::Y,32768)]{
        if state.buttons&physical.bits()!=0{buttons|=xinput;}
    }
    // Physical BLE axes are unsigned, Y down; the receiver display is Y up.
    let axis=|value:u8|->i16{if value<128{((i32::from(value)-128)*256)as i16}else{((i32::from(value)-128)*32767/127)as i16}};
    let invert=|value:i16|value.saturating_neg();
    (Pad{buttons,left_trigger:state.left_trigger,right_trigger:state.right_trigger,left_x:axis(state.left_x),left_y:invert(axis(state.left_y)),right_x:axis(state.right_x),right_y:invert(axis(state.right_y))},state.buttons&(Keys::L4.bits()|Keys::R4.bits()|Keys::P1.bits()|Keys::P2.bits()))
}

#[derive(Default)]
pub(crate) struct ReceiverTest {
    pad: Option<Pad>,
    paddles: crate::receiver::Paddles,
}
impl ReceiverTest {
    pub(crate) fn sample(&mut self, input:(Option<Pad>,Option<PhysicalInput>), _now:std::time::Duration, visible:bool)->Option<(Pad,u32)>{
        if let Some(pad)=input.0{self.pad=Some(pad);}
        if !visible{self.paddles=crate::receiver::Paddles::default();}
        else if let Some(state)=input.1{self.paddles.update(state);}
        self.pad.map(|pad|(pad,self.paddles.buttons()))
    }
}

const AXES:[u16;8]=[0,1,3,4,2,5,16,17]; // xone: X/Y/RX/RY, Z/RZ, HAT0X/Y.
const BUTTONS:[(u16,u16);11]=[(304,4096),(305,8192),(307,16384),(308,32768),(310,256),(311,512),(314,32),(315,16),(316,1024),(317,64),(318,128)];
#[cfg(target_os = "linux")]
const fn request(number:u8,size:usize)->libc::c_ulong{((2u32<<30)|((size as u32)<<16)|((b'E' as u32)<<8)|number as u32)as libc::c_ulong}
#[cfg(target_os = "linux")]
fn query<T>(file:&File,number:u8,value:&mut T)->Result<()>{
    // SAFETY: each caller supplies the Linux UAPI structure/bitmap matching number.
    if unsafe{libc::ioctl(file.as_raw_fd(),request(number,size_of::<T>()),value as *mut T)}<0{return Err(io::Error::last_os_error().into());}
    Ok(())
}

struct WiredState { axes:[AxisInfo;8], pad:Pad, dirty:bool }
impl WiredState {
    fn new(axes:[AxisInfo;8])->Result<Self>{
        for (i,axis) in axes.iter().enumerate(){
            if axis.minimum>=axis.maximum||!(axis.minimum..=axis.maximum).contains(&axis.value)||(i<4&&(axis.minimum>=0||axis.maximum<=0))||(i>=4&&i<6&&axis.minimum!=0){return Err(invalid("Invalid wired evdev axis range"));}
        }
        let mut state=Self{axes,pad:Pad::default(),dirty:false};
        for (index,code) in AXES.into_iter().enumerate(){state.axis(code,axes[index].value);}
        state.dirty=false;Ok(state)
    }
    fn axis(&mut self,code:u16,value:i32){
        let Some(index)=AXES.iter().position(|&axis|axis==code)else{return;};
        let info=&mut self.axes[index];info.value=value.clamp(info.minimum,info.maximum);
        let axis=|invert:bool|{let value=i64::from(info.value);let value=if invert{-value}else{value};let maximum=if value<0{if invert{i64::from(info.maximum)}else{-i64::from(info.minimum)}}else if invert{-i64::from(info.minimum)}else{i64::from(info.maximum)};(value*32767/maximum)as i16};
        match index{
            0=>self.pad.left_x=axis(false),1=>self.pad.left_y=axis(true),2=>self.pad.right_x=axis(false),3=>self.pad.right_y=axis(true),
            4=>self.pad.left_trigger=(i64::from(info.value)*255/i64::from(info.maximum))as u8,
            5=>self.pad.right_trigger=(i64::from(info.value)*255/i64::from(info.maximum))as u8,
            6=>self.pad.buttons=(self.pad.buttons&!12)|if info.value<0{4}else if info.value>0{8}else{0},
            7=>self.pad.buttons=(self.pad.buttons&!3)|if info.value<0{1}else if info.value>0{2}else{0},_=>unreachable!(),
        }
        self.dirty=true;
    }
    fn key(&mut self,code:u16,value:i32){
        if let Some(&(_,mask))=BUTTONS.iter().find(|&&(key,_)|key==code){
            if value==0{self.pad.buttons&=!mask;}else{self.pad.buttons|=mask;}
            self.dirty=true;
        }
    }
    fn event(&mut self,event:Event)->Result<Option<Pad>>{
        match (event.kind,event.code){
            (0,3)=>return Err(invalid("Wired input events dropped; reopening input to recover held controls")),
            (0,0)=>{if std::mem::take(&mut self.dirty){return Ok(Some(self.pad));}},
            (1,_)=>self.key(event.code,event.value),(3,_)=>self.axis(event.code,event.value),_=>{},
        }
        Ok(None)
    }
}
#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone,Copy)]
struct RawEvent { time:libc::timeval, kind:u16, code:u16, value:i32 }
#[cfg(target_os = "linux")]
pub(crate) struct WiredReader { file:File, state:WiredState }
#[cfg(target_os = "linux")]
impl WiredReader {
    pub(crate) fn open(path:&Path)->Result<Self>{
        let file=OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK|libc::O_CLOEXEC|libc::O_NOFOLLOW).open(path)?;
        if !file.metadata()?.file_type().is_char_device(){return Err(invalid("Wired input is not an evdev character device"));}
        let mut axes=[AxisInfo::default();8];
        for (index,code) in AXES.into_iter().enumerate(){query(&file,0x40+code as u8,&mut axes[index])?;}
        let mut state=WiredState::new(axes)?;
        let mut keys=[0u8;96];query(&file,0x18,&mut keys)?;
        for (code,_) in BUTTONS{state.key(code,i32::from(keys[usize::from(code)/8]&(1<<(code%8))!=0));}
        state.dirty=false;
        Ok(Self{file,state})
    }
    pub(crate) fn state(&self)->Pad{self.state.pad}
    pub(crate) fn poll(&mut self)->Result<Option<Pad>>{
        let mut bytes=[0;size_of::<RawEvent>()*64];let mut newest=None;
        // Bound draining so commands (notably USB takeover) cannot starve.
        for _ in 0..16{
            let count=match self.file.read(&mut bytes){
                Ok(0)=>return Err(invalid("Wired input disconnected")),Ok(count)=>count,
                Err(error) if error.kind()==io::ErrorKind::WouldBlock=>break,
                Err(error) if error.kind()==io::ErrorKind::Interrupted=>continue,
                Err(error)=>return Err(error.into()),
            };
            if count%size_of::<RawEvent>()!=0{return Err(invalid("Partial wired evdev event"));}
            for bytes in bytes[..count].chunks_exact(size_of::<RawEvent>()){
                // SAFETY: complete repr(C) Linux input_event; byte buffer may be unaligned.
                let raw=unsafe{std::ptr::read_unaligned(bytes.as_ptr().cast::<RawEvent>())};
                if let Some(pad)=self.state.event(Event{kind:raw.kind,code:raw.code,value:raw.value})?{newest=Some(pad);}
            }
        }
        Ok(newest)
    }
    #[cfg(test)]
    pub(crate) fn test_file(file:File)->Self{Self{file,state:WiredState::new(test_axes()).unwrap()}}
}
#[cfg(target_os = "windows")]
pub(crate) enum WiredReader {}
#[cfg(target_os = "windows")]
impl WiredReader {
    pub(crate) fn open(_path:&Path)->Result<Self>{
        Err(invalid("Wired input testing is not yet supported on Windows"))
    }
    pub(crate) fn state(&self)->Pad { match *self {} }
    pub(crate) fn poll(&mut self)->Result<Option<Pad>> { match *self {} }
}
#[cfg(test)]
fn test_axes()->[AxisInfo;8]{std::array::from_fn(|i|AxisInfo{minimum:if i<4{-32768}else if i<6{0}else{-1},maximum:if i<4{32767}else if i==4{1023}else if i==5{2047}else{1},..AxisInfo::default()})}

/// Live motion for the Test page. Bluetooth: raw IMU from the gaming HID report (ID 1,
/// 32 bytes; accel i16×3 at 15, gyro i16×3 at 21, unchanged on firmware 1.38, live
/// 2026-10-25). Receiver/cable: only the firmware's gyro-as-mouse output, if enabled.
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct Motion { pub gyro_dps:[f32;3], pub accel_g:Option<[f32;3]> }

const GYRO_DPS_PER_UNIT:f32=2000.0/32767.0;
// Inferred from ≈8031 counts on the vertical axis at rest (1 g); not a published scale.
const ACCEL_UNITS_PER_G:f32=8192.0;

pub(crate) fn bluetooth_motion(report:&[u8])->Option<Motion>{
    if report.len()!=32||report[0]!=1{return None;}
    let axis=|at:usize|f32::from(i16::from_le_bytes([report[at],report[at+1]]));
    Some(Motion{gyro_dps:[21,23,25].map(|at|axis(at)*GYRO_DPS_PER_UNIT),accel_g:Some([15,17,19].map(|at|axis(at)/ACCEL_UNITS_PER_G))})
}

#[cfg(target_os = "linux")]
enum MotionSource { Bluetooth(File), ReceiverMouse{file:File,counts:[i32;2],since:std::time::Instant} }
#[cfg(target_os = "linux")]
pub(crate) struct MotionReader(MotionSource);
#[cfg(target_os = "linux")]
impl MotionReader {
    /// Passive and non-grabbing; None when the link carries no motion data.
    pub(crate) fn open(devices:&[crate::input::DeviceInfo])->Option<Self>{
        use crate::input::LinkKind;
        let open=|path:&Path|OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK|libc::O_CLOEXEC|libc::O_NOFOLLOW).open(path).ok();
        if let Some(path)=devices.iter().find(|d|d.link==LinkKind::Bluetooth).and_then(|d|d.hidraw.as_deref()){
            return open(path).map(|file|Self(MotionSource::Bluetooth(file)));
        }
        let receiver=devices.iter().find(|d|d.link==LinkKind::Receiver)?;
        let profile=crate::input::HostProfile{receiver_gyro:true,..crate::input::HostProfile::default()};
        let path=crate::input::receiver_mouse_path(&profile,receiver,Path::new("/sys/class/input")).ok()??;
        open(&path).map(|file|Self(MotionSource::ReceiverMouse{file,counts:[0;2],since:std::time::Instant::now()}))
    }
    /// Newest sample; None while nothing new is available.
    pub(crate) fn poll(&mut self)->Option<Motion>{
        match &mut self.0 {
            MotionSource::Bluetooth(file)=>{
                let mut newest=None;let mut report=[0u8;64];
                for _ in 0..64{match file.read(&mut report){Ok(n)=>{if let Some(m)=bluetooth_motion(&report[..n]){newest=Some(m);}},Err(_)=>break}}
                newest
            }
            MotionSource::ReceiverMouse{file,counts,since}=>{
                let mut bytes=[0;size_of::<RawEvent>()*64];
                while let Ok(n)=file.read(&mut bytes){
                    if n==0{break;}
                    for chunk in bytes[..n-n%size_of::<RawEvent>()].chunks_exact(size_of::<RawEvent>()){
                        // SAFETY: complete repr(C) Linux input_event; byte buffer may be unaligned.
                        let raw=unsafe{std::ptr::read_unaligned(chunk.as_ptr().cast::<RawEvent>())};
                        if raw.kind==2&&raw.code<2{counts[usize::from(raw.code)]+=raw.value;}
                    }
                }
                // Rate over ≥100 ms windows: single 4-ms reports are too coarse to read.
                let elapsed=since.elapsed().as_secs_f32();
                if elapsed<0.1{return None;}
                let rate=|c:i32|c as f32/(elapsed*crate::input::RECEIVER_COUNTS_PER_DEGREE as f32);
                // Mouse X = yaw, mouse Y = pitch (screen down = tilt down).
                let motion=Motion{gyro_dps:[-rate(counts[1]),rate(counts[0]),0.0],accel_g:None};
                *counts=[0;2];*since=std::time::Instant::now();Some(motion)
            }
        }
    }
    pub(crate) fn is_receiver_mouse(&self)->bool{matches!(self.0,MotionSource::ReceiverMouse{..})}
}
#[cfg(target_os = "windows")]
pub(crate) enum MotionReader {}
#[cfg(target_os = "windows")]
impl MotionReader {
    pub(crate) fn open(_devices:&[crate::input::DeviceInfo])->Option<Self>{None}
    pub(crate) fn poll(&mut self)->Option<Motion>{match *self {}}
    pub(crate) fn is_receiver_mouse(&self)->bool{match *self {}}
}

#[cfg(test)]
mod tests {
    #[test]
    fn bluetooth_motion_decodes_live_firmware_138_report(){
        // Captured 2026-10-25 at rest (firmware 1.38): accel ≈(-0.01,-0.002,0.98) g, gyro ≈0.
        let report=decode("0108807f7e7f000000000000000064c1ffe4ff5d1f0100ffff0000c6167d0f00");
        let m=bluetooth_motion(&report).unwrap();
        assert_eq!(m.gyro_dps,[1.0,-1.0,0.0].map(|v:f32|v*GYRO_DPS_PER_UNIT));
        let [ax,ay,az]=m.accel_g.unwrap();
        assert_eq!([ax,ay,az],[-63.0/8192.0,-28.0/8192.0,8029.0/8192.0]);
        assert!(bluetooth_motion(&report[..31]).is_none(),"length is checked");
        let mut other=report;other[0]=2;assert!(bluetooth_motion(&other).is_none(),"report id is checked");
    }
    fn decode(hex:&str)->[u8;32]{std::array::from_fn(|i|u8::from_str_radix(&hex[i*2..i*2+2],16).unwrap())}
    use super::*;
    #[test]
    fn ble_physical_buttons_axes_and_paddles_are_not_xinput_bits(){
        let keys=[(Keys::UP,1),(Keys::DOWN,2),(Keys::LEFT,4),(Keys::RIGHT,8),(Keys::MENU,16),(Keys::VIEW,32),(Keys::L3,64),(Keys::R3,128),(Keys::LB,256),(Keys::RB,512),(Keys::A,4096),(Keys::B,8192),(Keys::X,16384),(Keys::Y,32768)];
        let sample=PhysicalInput{buttons:0,left_x:0,left_y:255,right_x:255,right_y:128,left_trigger:17,right_trigger:255};
        for (physical,xinput) in keys{let(pad,paddles)=bluetooth(PhysicalInput{buttons:physical.bits(),..sample});assert_eq!(pad.buttons,xinput);assert_eq!(paddles,0);}
        for paddle in [Keys::L4,Keys::R4,Keys::P1,Keys::P2]{let(pad,paddles)=bluetooth(PhysicalInput{buttons:paddle.bits(),..sample});assert_eq!(pad.buttons,0);assert_eq!(paddles,paddle.bits());}
        let(pad,_)=bluetooth(sample);assert_eq!((pad.left_x,pad.left_y,pad.right_x,pad.right_y),(-32768,-32767,32767,0));assert_eq!((pad.left_trigger,pad.right_trigger),(17,255));
    }
    #[test]
    fn wired_events_scale_runtime_ranges_and_publish_only_complete_frames(){
        let mut state=WiredState::new(test_axes()).unwrap();
        for (code,value) in [(0,-32768),(1,-32768),(3,32767),(4,32767),(2,1023),(5,1024),(16,-1),(17,1)]{assert!(state.event(Event{kind:3,code,value}).unwrap().is_none());}
        for code in [304,305,307,308,310,311,314,315,316,317,318]{assert!(state.event(Event{kind:1,code,value:1}).unwrap().is_none());}
        let pad=state.event(Event::default()).unwrap().unwrap();
        assert_eq!((pad.left_x,pad.left_y,pad.right_x,pad.right_y),(-32767,32767,32767,-32767));assert_eq!((pad.left_trigger,pad.right_trigger),(255,127));assert_eq!(pad.buttons,0xf7f6);
        for (code,value) in [(16,1),(17,-1),(2,0),(5,2047)]{state.event(Event{kind:3,code,value}).unwrap();}
        state.event(Event{kind:1,code:304,value:0}).unwrap();let pad=state.event(Event::default()).unwrap().unwrap();assert_eq!(pad.buttons&15,9);assert_eq!(pad.buttons&4096,0);assert_eq!((pad.left_trigger,pad.right_trigger),(0,255));
        assert!(state.event(Event{kind:0,code:3,value:0}).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn wired_reader_consumes_linux_events_and_coalesces_frames(){
        use std::{io::Write,os::{fd::OwnedFd,unix::net::UnixStream}};
        let(source,mut writer)=UnixStream::pair().unwrap();source.set_nonblocking(true).unwrap();
        let mut reader=WiredReader::test_file(File::from(OwnedFd::from(source)));
        let send=|writer:&mut UnixStream,kind,code,value|{
            let event=RawEvent{time:libc::timeval{tv_sec:0,tv_usec:0},kind,code,value};
            // SAFETY: initialized, padding-free input_event on supported x86_64 Linux.
            let bytes=unsafe{std::slice::from_raw_parts((&event as *const RawEvent).cast::<u8>(),size_of::<RawEvent>())};
            writer.write_all(bytes).unwrap();
        };
        send(&mut writer,3,0,12000);send(&mut writer,1,304,1);
        assert!(reader.poll().unwrap().is_none(),"partial SYN frame must not be displayed");
        send(&mut writer,0,0,0);send(&mut writer,3,5,2047);send(&mut writer,0,0,0);
        let pad=reader.poll().unwrap().unwrap();assert_eq!(pad.left_x,12000);assert_eq!(pad.buttons,4096);assert_eq!(pad.right_trigger,255);
        assert!(reader.poll().unwrap().is_none());
    }
}
