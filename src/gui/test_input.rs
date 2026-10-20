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

#[cfg(test)]
mod tests {
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
