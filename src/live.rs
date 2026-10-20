//! Configuration transactions, explicit force feedback, and mapper lifetime belong to the worker.
use crate::{Result, invalid, att, gip::{self,Gip}, gui::state::*, input::{self,HostProfile,DeviceInfo,LinkKind}, profile::{Profiles,IMAGE_SIZE}, receiver::{self,Receiver,Pad}};
use std::{sync::{mpsc::{self,Receiver as Rx,Sender},Arc,atomic::{AtomicBool,Ordering}},thread,time::{Duration,Instant},path::{Path,PathBuf}};
use parking_lot::Mutex;
use crate::profiles::{self,Store,Debounce};
use crate::gui::test_input::{self,WiredReader};

enum Command { Read, ConnectBluetooth, SwitchReceiverMode(Link), Save(Box<[u8;IMAGE_SIZE]>), TestVisible(bool), Mapper(Option<(DeviceInfo,HostProfile,Option<crate::profile::KeyMap>)>), Auto(Option<(profiles::AutoConfig,String)>), Rumble(PathBuf,[u8;2]), Shutdown }
enum Event { Connected(PathBuf,Profiles,bool), WiredConnected(PathBuf,Profiles,bool), WiredStarting, BluetoothConnected(String,Profiles), ModeSwitched(String), Saved(Profiles), Read(Profiles), Version(String), Error(String), Disconnected(String), Inventory(Vec<DeviceInfo>,Vec<PathBuf>), Battery(Option<gip::Battery>), Mapper(bool,String), AutoSwitched{name:String,profile:HostProfile,notice:String}, Rumble(bool,Option<String>) }
#[derive(Clone,Copy)]
struct PadSample { state:Pad,paddles:u32,received_at:Instant }
pub struct LiveState { model:Model, commands:Sender<Command>, events:Rx<Event>, pad:Arc<Mutex<Option<PadSample>>>, worker:Option<thread::JoinHandle<()>>, stop:Arc<AtomicBool>, store:Store }
impl Default for LiveState{fn default()->Self{Self::new()}}
impl LiveState {
    pub fn new()->Self {
        let(tx,commands)=mpsc::channel();let(events,rx)=mpsc::channel();let pad=Arc::new(Mutex::new(None));let worker_pad=Arc::clone(&pad);
        let stop=Arc::new(AtomicBool::new(false));let worker_stop=Arc::clone(&stop);
        let worker=thread::spawn(move||worker(commands,events,worker_pad,worker_stop));
        let mut model=Model{notice:Some("Looking for an awake Ultimate 3E receiver or wired USB controller. Wired configuration pauses gamepad input briefly.".into()),..Model::default()};
        let store=Store::default();
        match (||->Result<()>{store.initialize()?;model.host_profile_names=store.names()?;model.auto_config=store.load_auto()?;model.host_profile_name=model.auto_config.default.clone();model.host_profile=store.load(&model.host_profile_name)?;Ok(())})(){
            Ok(())=>{},
            Err(error)=>{
                // A broken named configuration must not replace a usable legacy mapping.
                if let Ok(profile)=crate::ui::load_profile(&crate::ui::default_profile_path()){model.host_profile=profile;}
                model.notice=Some(format!("Named host profiles could not load: {error}"));
            },
        }
        Self{model,commands:tx,events:rx,pad,worker:Some(worker),stop,store}
    }
    fn start(&mut self,command:Command,title:&str)->Result<()> {
        if self.model.operation.is_some(){return Err(invalid("A device operation is already running"));}
        if self.model.device.is_none()&&!matches!(command,Command::Read|Command::ConnectBluetooth){return Err(invalid("No connected controller"));}
        let wired=self.model.device.as_ref().is_some_and(|device|device.link==Link::Wired)&&matches!(command,Command::Read|Command::Save(_));
        if wired&&(self.model.mapper_running||self.model.mapper_pending){
            let notice="Stop the host input mapper before wired configuration; gamepad input pauses briefly.";
            self.model.notice=Some(notice.into());return Err(invalid(notice));
        }
        self.model.operation=Some(Operation{title:title.into(),detail:if wired{"Gamepad input pauses briefly. Do not disconnect the controller."}else{"Do not disconnect the controller."}.into()});
        if self.commands.send(command).is_err(){self.model.operation=None;return Err(invalid("Device worker stopped"));}
        Ok(())
    }
    fn receive_profiles(&mut self,p:Profiles){self.model.load(p);self.model.operation=None;}
    fn host_editable(&self)->Result<()>{
        if self.model.mapper_running||self.model.mapper_pending{return Err(invalid("Stop the host mapper before editing named profiles"));}
        Ok(())
    }
    fn sync_auto(&self)->Result<()>{
        let selection=self.model.auto_enabled.then(||(self.model.auto_config.clone(),self.model.host_profile_name.clone()));
        self.commands.send(Command::Auto(selection)).map_err(|_|invalid("Device worker stopped"))
    }
}
impl GuiState for LiveState {
    fn model(&self)->&Model{&self.model}
    fn set_test_visible(&mut self,visible:bool)->Result<()>{
        if visible!=self.model.test_visible{
            self.commands.send(Command::TestVisible(visible)).map_err(|_|invalid("Device worker stopped"))?;
            self.model.test_visible=visible;
        }
        Ok(())
    }
    fn poll(&mut self)->Result<bool> {
        let mut changed=false;
        while let Ok(event)=self.events.try_recv(){changed=true;match event{
            Event::Connected(path,p,replace)=>{
                self.model.device=Some(Device{name:"Ultimate 3E Controller for XBOX".into(),link:Link::Receiver,pid:0x202f,version_record:None});
                self.model.battery=None;
                if !replace&&self.model.profiles.is_some()&&self.model.dirty(){self.model.baseline=Some(p);self.model.notice=Some(format!("Reconnected {}. Unsaved edits retained; Revert reloads the controller.",path.display()));}
                else{self.model.load(p);self.model.notice=Some(format!("Connected {}. Edits remain staged until Save.",path.display()));}
                self.model.operation=Some(Operation{title:"Reading version record…".into(),detail:"Profiles loaded; finishing read-only discovery.".into()});
            },
            Event::WiredStarting=>{
                self.model.notice=Some("Wired configuration: gamepad input pauses briefly.".into());
                self.model.operation=Some(Operation{title:"Reading wired profiles and version…".into(),detail:"Gamepad input pauses briefly. Do not disconnect the controller.".into()});
            },
            Event::WiredConnected(path,p,replace)=>{
                self.model.device=Some(Device{name:"Ultimate 3E Controller for XBOX".into(),link:Link::Wired,pid:0x202f,version_record:None});
                self.model.battery=None;
                if !replace&&self.model.profiles.is_some()&&self.model.dirty(){self.model.baseline=Some(p);self.model.notice=Some(format!("Reconnected wired {}. Unsaved edits retained; Revert reloads the controller.",path.display()));}
                else{self.model.load(p);self.model.notice=Some(format!("Connected wired {}. Reads/saves pause gamepad input briefly; edits stay staged until Save.",path.display()));}
                self.model.operation=None;self.model.pad=None;self.model.pad_received_at=None;
            },
            Event::BluetoothConnected(address,p)=>{
                self.model.device=Some(Device{name:format!("Ultimate 3E — {address}"),link:Link::Bluetooth,pid:0x202f,version_record:None});
                self.model.battery=None;
                if self.model.profiles.is_some()&&self.model.dirty(){
                    self.model.baseline=Some(p);self.model.notice=Some("Bluetooth connected. Unsaved edits retained; Revert reloads the controller. Bluetooth saves disabled pending hardware verification.".into());
                }else{
                    self.model.load(p);self.model.notice=Some("Bluetooth profiles loaded. Saves disabled pending hardware verification; edits remain local.".into());
                }
                self.model.operation=None;self.model.pad=None;self.model.pad_received_at=None;
            },
            Event::Saved(p)=>{
                let notice=self.model.baseline.as_ref().map(|baseline|saved_notice(&baseline.encode(),&p.encode())).unwrap_or_else(||"Read-back identical; previous image unavailable.".into());
                self.receive_profiles(p);self.model.notice=Some(notice);
            },
            Event::Read(p)=>{self.receive_profiles(p);self.model.notice=Some("Profiles reloaded from the controller.".into());},
            Event::Version(version)=>{if let Some(device)=&mut self.model.device{device.version_record=Some(version);}self.model.operation=None;},
            Event::ModeSwitched(notice)=>{
                self.model.device=None;self.model.pad=None;self.model.pad_received_at=None;
                self.model.battery=None;
                self.model.operation=None;
                self.model.notice=Some(notice);
            },
            Event::Error(error)=>{self.model.operation=None;self.model.notice=Some(error);},
            Event::Disconnected(error)=>{self.model.device=None;self.model.battery=None;self.model.pad=None;self.model.pad_received_at=None;self.model.operation=None;self.model.notice=Some(error);},
            Event::Inventory(devices,nodes)=>{self.model.input_devices=devices;self.model.rumble_nodes=nodes;},
            Event::Battery(battery)=>self.model.battery=battery,
            Event::Mapper(running,notice)=>{
                self.model.mapper_running=running;self.model.mapper_pending=false;
                self.model.notice=Some(notice);
            },
            Event::AutoSwitched{name,profile,notice}=>{
                self.model.host_profile_name=name;self.model.host_profile=profile;self.model.notice=Some(notice);
            },
            Event::Rumble(running,notice)=>{self.model.rumble_running=running;if let Some(notice)=notice{self.model.notice=Some(notice);}},
        }}
        if let Some(latest)=self.pad.try_lock(){
            let pad=latest.map(|sample|sample.state);
            let paddles=latest.map_or(0,|sample|sample.paddles);
            if self.model.device.is_some()&&(pad!=self.model.pad||paddles!=self.model.paddles){
                self.model.pad=pad;self.model.paddles=paddles;self.model.pad_received_at=latest.map(|sample|sample.received_at);changed=true;
            }
        }
        Ok(changed)
    }
    fn dispatch(&mut self,action:Action)->Result<()> {
        match action {
            Action::DismissNotice=>self.model.notice=None,
            Action::SetMapper(enabled)=>{
                if enabled&&self.model.operation.is_some(){return Err(invalid("Wait for the device operation to finish before starting the host mapper"));}
                if self.model.mapper_pending{return Err(invalid("Wait for the host mapper to start or stop"));}
                if enabled==self.model.mapper_running{return Ok(());}
                let selection=if enabled{
                    let device=self.model.mapper_device()?.clone();
                    self.model.host_profile.validate_for_link(device.link)?;
                    let slot_map=self.model.mapper_key_map();
                    Some((device,self.model.host_profile.clone(),slot_map))
                }else{None};
                self.commands.send(Command::Mapper(selection)).map_err(|_|invalid("Device worker stopped"))?;
                self.model.mapper_pending=true;
                if enabled{self.sync_auto()?;}
            },
            Action::SaveHostProfile=>{
                self.host_editable()?;
                self.store.save(&self.model.host_profile_name,&self.model.host_profile)?;
                self.model.notice=Some(format!("Host profile saved to {}",self.store.path(&self.model.host_profile_name)?.display()));
            },
            Action::SelectHostProfile(name)=>{
                self.host_editable()?;let profile=self.store.load(&name)?;
                self.model.host_profile=profile;self.model.host_profile_name=name;
            },
            Action::NewHostProfile(name)=>{
                self.host_editable()?;self.store.create(&name,&self.model.host_profile)?;
                self.model.host_profile_names=self.store.names()?;self.model.host_profile_name=name;
            },
            Action::RenameHostProfile(name)=>{
                self.host_editable()?;
                // Preserve current Remap edits when renaming.
                self.store.save(&self.model.host_profile_name,&self.model.host_profile)?;
                self.store.rename(&self.model.host_profile_name,&name,&mut self.model.auto_config)?;
                self.model.host_profile_name=name;self.model.host_profile_names=self.store.names()?;
            },
            Action::DeleteHostProfile=>{
                self.host_editable()?;self.store.delete(&self.model.host_profile_name,&self.model.auto_config)?;
                self.model.host_profile_name=self.model.auto_config.default.clone();
                self.model.host_profile=self.store.load(&self.model.host_profile_name)?;self.model.host_profile_names=self.store.names()?;
            },
            Action::SetAutoEnabled(enabled)=>{
                self.model.auto_enabled=enabled;self.sync_auto()?;
                self.model.notice=Some(if enabled{"Automatic host profiles enabled while the mapper runs."}else{"Automatic host profiles disabled."}.into());
            },
            Action::SetAutoConfig(config)=>{
                self.store.save_auto(&config)?;self.model.auto_config=config;self.sync_auto()?;
            },
            Action::TestRumble=>{
                if !self.model.can_test_rumble(){return Err(invalid("No FF_RUMBLE-capable controller input node"));}
                if self.model.rumble_nodes.len()!=1{return Err(invalid("Multiple FF-capable controllers; disconnect all but the intended controller"));}
                if self.model.rumble_running{return Err(invalid("Vibration test already running"));}
                self.commands.send(Command::Rumble(self.model.rumble_nodes[0].clone(),self.model.rumble_levels())).map_err(|_|invalid("Device worker stopped"))?;
                self.model.rumble_running=true;
            },
            Action::ConnectBluetooth=>{
                if !self.model.can_connect_bluetooth(){return Err(invalid("Disconnect the receiver and stop the host mapper before connecting Bluetooth"));}
                self.start(Command::ConnectBluetooth,"Scanning for 81HF01…")?;
                self.model.operation.as_mut().unwrap().detail="Hold the front pixel-logo/profile button for 3 seconds until configuration LEDs blink. Close the mobile app.".into();
            },
            Action::SwitchReceiverMode(link)=>{
                if self.model.operation.is_some(){return Err(invalid("Wait for the pending device transfer before switching receiver mode"));}
                if self.model.mapper_running||self.model.mapper_pending{return Err(invalid("Stop the host input mapper before switching receiver mode"));}
                if self.model.dirty(){return Err(invalid("Save or revert unsaved edits before switching receiver mode"));}
                if self.model.receiver_mode_link()!=Some(link){return Err(invalid("Receiver link changed; reopen the mode-switch confirmation"));}
                self.start(Command::SwitchReceiverMode(link),"Switching receiver mode…")?;
                self.model.operation.as_mut().unwrap().detail="Persistent mode change: input pauses while the receiver re-enumerates. Checking the USB identities for up to 10 seconds; not retried.".into();
            },
            Action::Refresh|Action::Revert=>self.start(Command::Read,"Reading profiles…")?,
            Action::Save=>{
                if self.model.operation.is_some(){return Err(invalid("A device operation is already running"));}
                if self.model.device.is_none(){return Err(invalid("No connected controller"));}
                if !self.model.can_save(){return Err(invalid("Bluetooth saves disabled pending hardware verification; use the 2.4 GHz receiver"));}
                let mut profiles=self.model.profiles.clone().ok_or_else(||invalid("No profiles loaded"))?;
                let baseline=self.model.baseline.as_ref().ok_or_else(||invalid("Read controller profiles before saving"))?;
                // Full-profile writes do not change runtime/header flags on the controller.
                profiles.profile_bits=baseline.profile_bits;
                profiles.fix_crc();let image=profiles.encode();let before=baseline.encode();
                let changed=before.iter().zip(&image).filter(|(old,new)|old!=new).count();
                eprintln!("Save image: editor_slot_index={} (slot {}), runtime_slot=0x{:02x}, changed_bytes={changed}",self.model.active_slot,self.model.active_slot+1,image[2]);
                if changed==0{eprintln!("Save skipped: Nothing to save");self.model.profiles=Some(profiles);self.model.notice=Some("Nothing to save".into());return Ok(());}
                for(offset,(&old,&new))in before.iter().zip(&image).enumerate(){if old!=new{eprintln!("Save byte 0x{offset:03x}: 0x{old:02x} -> 0x{new:02x}");}}
                self.start(Command::Save(Box::new(image)),"Saving and verifying profiles…")?;
            },
            edit=>self.model.edit(edit)?,
        }Ok(())
    }
}
impl Drop for LiveState{
    fn drop(&mut self){
        self.stop.store(true,Ordering::Relaxed);
        let _=self.commands.send(Command::Shutdown);
        // Cancel waits; transaction cleanup restores reporting.
        if let Some(worker)=self.worker.take(){let _=worker.join();}
    }
}
fn saved_notice(before:&[u8;IMAGE_SIZE],after:&[u8;IMAGE_SIZE])->String{
    use std::fmt::Write;
    let changed=before.iter().zip(after).filter(|(a,b)|a!=b).count();
    let mut text=format!("Saved: {changed} bytes changed, read-back identical");
    if changed>0{
        text.push_str(" (");let mut first=true;let mut i=0;
        while i<IMAGE_SIZE{
            if before[i]==after[i]{i+=1;continue;}
            let start=i;while i+1<IMAGE_SIZE&&before[i+1]!=after[i+1]{i+=1;}
            if !first{text.push_str(", ");}first=false;
            let _=write!(text,"0x{start:03x}");if i!=start{let _=write!(text,"-0x{i:03x}");}i+=1;
        }
        text.push(')');
    }
    text
}
fn read(receiver:&mut Receiver)->Result<Profiles>{let bytes: [u8;IMAGE_SIZE]=receiver.read_profiles()?.try_into().map_err(|_|invalid("Receiver returned an invalid profile length"))?;Profiles::decode(&bytes)}
fn save(receiver:&mut Receiver,image:&[u8;IMAGE_SIZE])->Result<Profiles>{receiver.write_profiles(image)?;verified(image,&receiver.read_profiles()?)}
fn read_bluetooth(connection:&mut att::Connection)->Result<Profiles>{
    // read_profiles restores op7/0x110 on success and failure. Do not call
    // enable_physical: it waits for a change-driven report and fails on idle.
    let bytes:[u8;IMAGE_SIZE]=connection.read_profiles()?.try_into().map_err(|_|invalid("Bluetooth returned an invalid profile length"))?;
    Profiles::decode(&bytes)
}
fn save_bluetooth(connection:&mut att::Connection,image:&[u8;IMAGE_SIZE])->Result<Profiles>{
    if !BLE_WRITE_VERIFIED{return Err(invalid("Bluetooth writes have not passed hardware verification"));}
    connection.write_profiles(image)?;verified(image,&connection.read_profiles()?)
}
fn wired_failure(error:&crate::Error)->String{
    #[cfg(target_os = "linux")]
    if error.downcast_ref::<std::io::Error>().is_some_and(|error|error.raw_os_error()==Some(libc::EACCES)){
        return "Wired USB permission denied (EACCES). Install the README udev rules as an early (70-) rules file; NixOS extraRules (99-local) is too late for uaccess. Replug, then Refresh. Staged edits retained.".into();
    }
    failure(error)
}
fn finish_wired<T>(result:Result<T>,closed:Result<gip::Rebind>)->Result<T>{
    match (result,closed){
        (Ok(value),Ok(_))=>Ok(value),
        (Err(error),Ok(_))=>Err(error),
        (result,Err(error))=>Err(invalid(match result{
            Ok(_)=>format!("Wired transfer completed, but driver handback failed: {error}; physical replug may be required"),
            Err(transfer)=>format!("Wired transfer failed: {transfer}; driver handback failed: {error}; physical replug may be required"),
        })),
    }
}
fn wired_transaction<T>(path:&Path,operation:impl FnOnce(&mut Gip)->Result<T>)->Result<T>{
    let mut connection=Gip::takeover(path)?;
    let result=operation(&mut connection);
    finish_wired(result,connection.close())
}
fn mode_changed(source:Link,receiver_present:bool,gip_present:bool)->bool{
    match source{
        Link::Receiver=>gip_present&&!receiver_present,
        Link::Wired=>receiver_present&&!gip_present,
        Link::Bluetooth=>false,
    }
}
fn mode_switch_outcome(source:Link,changed:bool,sent:Result<()>,handback:Result<()>)->Result<String>{
    let target=if source==Link::Receiver{"Xbox mode (2dc8:202f)"}else{"PC (XInput) mode (2dc8:3024)"};
    if changed{
        // Live: disconnect can precede the acknowledgment and xone handback.
        if let Err(error)=sent{eprintln!("Receiver switched despite send/acknowledgment error: {error}");}
        if let Err(error)=handback{eprintln!("Receiver switched despite driver handback error: {error}");}
        Ok(format!("Receiver switched to {target}. Reconnecting…"))
    }else{
        let mut message=format!("Receiver did not switch to {target} within 10 seconds (target must appear and old identity must disappear). Not retried.");
        if let Err(error)=sent{message.push_str(&format!(" Send/acknowledgment: {error}."));}
        if let Err(error)=handback{message.push_str(&format!(" Driver handback: {error}."));}
        Err(invalid(message))
    }
}
fn wait_mode_change(source:Link,stop:&AtomicBool)->bool{
    let deadline=Instant::now()+Duration::from_secs(10);
    loop{
        let receiver=receiver::find();
        let receiver_present=match receiver{
            Ok(_)=>Some(true),
            Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|error|error.kind()==std::io::ErrorKind::NotFound)=>Some(false),
            Err(_)=>None,
        };
        // A failed discovery is unknown, never proof that the old identity is gone.
        if let (Some(receiver),Ok(gip))=(receiver_present,gip::discover()){
            if mode_changed(source,receiver,!gip.is_empty()){return true;}
        }
        if Instant::now()>=deadline||stop.load(Ordering::Relaxed){return false;}
        thread::sleep(Duration::from_millis(200));
    }
}
fn read_wired(path:&Path)->Result<Profiles>{
    let bytes:[u8;IMAGE_SIZE]=wired_transaction(path,|connection|connection.read_profiles())?.try_into().map_err(|_|invalid("Wired returned an invalid profile length"))?;
    Profiles::decode(&bytes)
}
fn save_wired(path:&Path,image:&[u8;IMAGE_SIZE])->Result<Profiles>{
    let back=wired_transaction(path,|connection|{connection.write_profiles(image)?;connection.read_profiles()})?;
    verified(image,&back)
}
// USB bus addresses can change during xone handback; sysfs topology is the attempt key.
fn select_link(receiver:Option<PathBuf>,wired:&[gip::Device])->Result<Option<(Link,PathBuf,PathBuf)>>{
    if let Some(path)=receiver{return Ok(Some((Link::Receiver,path.clone(),path)));}
    match wired{
        []=>Ok(None),
        [device]=>Ok(Some((Link::Wired,device.node.clone(),device.sysfs.clone()))),
        _=>Err(invalid("Multiple wired 2dc8:202f devices detected (controller or Xbox-mode receiver); disconnect all but the intended device, then Refresh. No device selected.")),
    }
}
fn version_label(bytes:&[u8])->String{
    use std::fmt::Write;
    let mut label=String::with_capacity(bytes.len()*3);
    for(i,byte)in bytes.iter().enumerate(){if i>0{label.push(' ');}let _=write!(label,"{byte:02x}");}
    label
}
fn connect_wired(path:&Path,events:&Sender<Event>,replace:bool,mapper_active:bool)->Result<()>{
    if mapper_active{return Err(invalid("Stop the host input mapper before wired configuration; gamepad input pauses briefly"));}
    send_event(events,Event::WiredStarting);
    let(bytes,version)=wired_transaction(path,|connection|Ok((connection.read_profiles()?,connection.read_version())))?;
    let bytes:[u8;IMAGE_SIZE]=bytes.try_into().map_err(|_|invalid("Wired returned an invalid profile length"))?;
    send_event(events,Event::WiredConnected(path.to_owned(),Profiles::decode(&bytes)?,replace));
    match version{
        Ok(bytes)=>send_event(events,Event::Version(version_label(&bytes))),
        Err(error)=>send_event(events,Event::Error(format!("Wired version record unavailable: {error}"))),
    }
    Ok(())
}
/// CRC + body must match; bytes 2 (runtime slot) and 3 (profile bits) are device-managed
/// and not changed by opcode-1 writes (verified live), so the device's values are kept.
fn verified(image:&[u8;IMAGE_SIZE],back:&[u8])->Result<Profiles>{
    let back:&[u8;IMAGE_SIZE]=back.try_into().map_err(|_|invalid("Save verification failed: incomplete read-back; staged edits retained"))?;
    if back[..2]!=image[..2]||back[4..]!=image[4..]{return Err(invalid("Save verification failed: controller read-back differs; staged edits retained"));}
    Profiles::decode(back)
}
fn failure(error:&crate::Error)->String{
    if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::TimedOut){
        "Controller asleep or out of range — press any button, then Refresh. Staged edits retained.".into()
    }else{format!("{error}. Staged edits retained; press Refresh to retry.")}
}
fn send_event(events:&Sender<Event>,event:Event){
    if events.send(event).is_ok(){crate::gui::wake();}
}
fn connect(path:&Path,stop:&Arc<AtomicBool>,events:&Sender<Event>,replace:bool)->Result<Receiver>{
    let mut receiver=Receiver::open(path)?.with_stop(Arc::clone(stop));
    let profiles=read(&mut receiver)?;
    send_event(events,Event::Connected(path.to_owned(),profiles,replace));
    // Live receiver repeats one 13-byte response regardless of offset;
    // Windows' 40-byte component-version decoder is inapplicable.
    match receiver.read_version(){
        Ok(bytes)=>{
            send_event(events,Event::Version(version_label(&bytes[..13])));
        },
        Err(error)=>{send_event(events,Event::Error(format!("Version record unavailable: {error}")));},
    }
    Ok(receiver)
}
// Once connected, the HID poll is the wait. Waiting for commands first halves
// service headroom and lets old reports accumulate before they reach the GUI.
fn next_command(commands:&Rx<Command>,connected:bool)->Result<Command,mpsc::RecvTimeoutError>{
    if !connected{return commands.recv_timeout(Duration::from_millis(8));}
    commands.try_recv().map_err(|error|match error{
        mpsc::TryRecvError::Empty=>mpsc::RecvTimeoutError::Timeout,
        mpsc::TryRecvError::Disconnected=>mpsc::RecvTimeoutError::Disconnected,
    })
}
struct AutoState { config:profiles::AutoConfig, current:String, debounce:Debounce }
impl AutoState {
    fn new(config:profiles::AutoConfig,current:String)->Self{Self{config,current,debounce:Debounce::default()}}
}
struct AutoTarget { name:String, notice:String }
/// Selection and debounce are independent of GUI frames, threads and the filesystem.
fn auto_step(auto:&mut Option<AutoState>,running:bool,now:Duration,processes:&[profiles::Process])->Option<AutoTarget>{
    let state=auto.as_mut()?;
    if !running{state.debounce=Debounce::default();return None;}
    let rule=profiles::matching_rule(&state.config.rules,processes);
    let target=rule.map_or(state.config.default.as_str(),|rule|rule.profile.as_str());
    if !state.debounce.changed(&state.current,target,now){return None;}
    let matched=rule.map_or("default",|rule|rule.process.as_str());
    Some(AutoTarget{name:target.to_owned(),notice:format!("Profile: {target} (matched {matched})")})
}
fn prepare_auto_switch(auto:&mut Option<AutoState>,link:Option<LinkKind>,store:&Store,now:Duration,processes:&[profiles::Process])->Result<Option<(AutoTarget,HostProfile)>>{
    let Some(target)=auto_step(auto,link.is_some(),now,processes)else{return Ok(None);};
    let profile=store.load(&target.name)?;
    profile.validate_for_link(link.expect("running mapper link"))?;
    Ok(Some((target,profile)))
}
struct MapperWorker { stop:Arc<AtomicBool>, handle:Option<thread::JoinHandle<Result<()>>> }
impl MapperWorker {
    fn start(device:DeviceInfo,profile:HostProfile,slot_map:Option<crate::profile::KeyMap>)->Self{
        let stop=Arc::new(AtomicBool::new(false));let flag=Arc::clone(&stop);
        let handle=thread::spawn(move||match device.link{
            LinkKind::Receiver=>input::run_receiver(Some(&device.path),&profile,slot_map,flag),
            LinkKind::Bluetooth=>crate::att::run_controller(Some(&device.path),&profile,slot_map,None,Duration::from_secs(10),flag),
            LinkKind::Wired=>input::run_wired(Some(&device.path),&profile,flag),
        });
        Self{stop,handle:Some(handle)}
    }
    fn close(&mut self)->Result<()>{
        self.stop.store(true,Ordering::Release);
        if let Some(handle)=self.handle.take(){handle.join().map_err(|_|invalid("Host mapper thread panicked"))??;}
        Ok(())
    }
}
impl Drop for MapperWorker { fn drop(&mut self){if let Err(error)=self.close(){eprintln!("Host mapper shutdown: {error}");}} }
fn publish_pad(mailbox:&Mutex<Option<PadSample>>,state:Pad,paddles:u32){
    let changed={
        let mut latest=mailbox.lock();
        let changed=latest.is_none_or(|sample|sample.state!=state||sample.paddles!=paddles);
        *latest=Some(PadSample{state,paddles,received_at:Instant::now()});changed
    };
    if changed{crate::gui::wake();}
}
fn wired_test_enabled(visible:bool,mapper_active:bool,wired_connected:bool)->bool{visible&&!mapper_active&&wired_connected}
fn release_wired_test(command:&Command,reader:&mut Option<WiredReader>,pad:&Mutex<Option<PadSample>>){
    if matches!(command,Command::Read|Command::Save(_)|Command::SwitchReceiverMode(_)|Command::Mapper(_)|Command::TestVisible(false)|Command::ConnectBluetooth|Command::Shutdown)&&reader.take().is_some(){*pad.lock()=None;}
}
fn worker(commands:Rx<Command>,events:Sender<Event>,pad:Arc<Mutex<Option<PadSample>>>,stop:Arc<AtomicBool>) {
    let mut connected:Option<Receiver>=None;
    let mut bluetooth:Option<att::Connection>=None;
    let mut wired:Option<PathBuf>=None;
    let mut selection_error:Option<String>=None;
    let mut attempted:Option<PathBuf>=None;
    let mut next_find=Instant::now();let mut reload=false;
    let mut mapper:Option<MapperWorker>=None;
    let mut mapper_device:Option<DeviceInfo>=None;
    let mut mapper_slot_map=None;
    let mut auto:Option<AutoState>=None;
    let auto_epoch=Instant::now();let mut auto_next=auto_epoch;let store=Store::default();
    let mut rumble:Option<(input::rumble::Effect,Instant)>=None;
    let mut next_inventory=Instant::now();
    let mut next_battery=Instant::now();
    let mut wired_input:Option<PathBuf>=None;
    let mut test_visible=false;let mut test_reader:Option<WiredReader>=None;
    let mut next_test_open=Instant::now();let mut test_error=None;
    let mut receiver_test=test_input::ReceiverTest::default();
    loop {
        if crate::interrupted()||stop.load(Ordering::Relaxed){break;}
        if mapper.as_ref().is_some_and(|worker|worker.handle.as_ref().is_some_and(|handle|handle.is_finished())){
            let result=mapper.take().unwrap().close();
            mapper_device=None;auto=None;
            send_event(&events,Event::Mapper(false,result.err().map_or_else(||"Host mapper stopped.".into(),|error|format!("Host mapper stopped: {error}"))));
        }
        let now=Instant::now();
        #[cfg(target_os = "linux")]
        if auto.is_some()&&mapper.is_some()&&now>=auto_next{
            auto_next=now+Duration::from_secs(1);
            let processes=profiles::collect_processes(Path::new("/proc"),unsafe{libc::getuid()});
            match prepare_auto_switch(&mut auto,mapper_device.as_ref().map(|device|device.link),&store,now.saturating_duration_since(auto_epoch),&processes){
                Ok(Some((target,profile)))=>{
                    let mut previous=mapper.take().expect("running mapper");
                    if let Err(error)=previous.close(){
                        mapper_device=None;auto=None;
                        send_event(&events,Event::Mapper(false,format!("Host mapper stopped: {error}")));
                    }else{
                        mapper=Some(MapperWorker::start(mapper_device.as_ref().expect("running mapper device").clone(),profile.clone(),mapper_slot_map.clone()));
                        let state=auto.as_mut().expect("enabled auto watcher");
                        state.current=target.name.clone();state.debounce=Debounce::default();
                        send_event(&events,Event::AutoSwitched{name:target.name,profile,notice:target.notice});
                    }
                },
                Ok(None)=>{},
                Err(error)=>send_event(&events,Event::Error(format!("Auto profile switch failed: {error}"))),
            }
        }
        if rumble.as_ref().is_some_and(|(_,deadline)|Instant::now()>=*deadline){
            rumble=None;send_event(&events,Event::Rumble(false,None));
        }
        if Instant::now()>=next_inventory{
            next_inventory=Instant::now()+Duration::from_secs(1);
            let devices=input::devices().unwrap_or_default();
            let mut nodes=devices.iter().filter(|device|device.link==LinkKind::Wired);
            wired_input=nodes.next().filter(|_|nodes.next().is_none()).map(|device|device.path.clone());
            send_event(&events,Event::Inventory(devices,input::rumble::devices().unwrap_or_default()));
        }
        let command=next_command(&commands,connected.is_some()||bluetooth.is_some());
        if let Ok(command)=&command{release_wired_test(command,&mut test_reader,&pad);}
        if matches!(&command,Ok(Command::Read|Command::Save(_)|Command::SwitchReceiverMode(_)|Command::TestVisible(false)|Command::Shutdown)){
            receiver_test=test_input::ReceiverTest::default();*pad.lock()=None;
        }
        match command{
            Ok(Command::Shutdown)|Err(mpsc::RecvTimeoutError::Disconnected)=>break,
            Ok(Command::Auto(selection))=>{
                #[cfg(target_os = "linux")]
                { auto=selection.map(|(config,current)|AutoState::new(config,current));auto_next=Instant::now(); }
                #[cfg(target_os = "windows")]
                if selection.is_some(){send_event(&events,Event::Error("Process-based profile switching is not yet supported on Windows".into()));}
            },
            Ok(Command::TestVisible(visible))=>{
                test_visible=visible;next_test_open=Instant::now();test_error=None;
            },
            Ok(Command::Mapper(selection))=>{
                auto=None;mapper_device=None;
                if selection.as_ref().is_some_and(|(device,_,_)|device.link==LinkKind::Bluetooth){
                    if let Some(mut connection)=bluetooth.take(){
                        if let Err(error)=connection.close(){send_event(&events,Event::Error(format!("Bluetooth configuration close failed: {error}")));}
                        send_event(&events,Event::Disconnected("Host mapper owns configuration BLE; stop mapping and Connect Bluetooth to edit device profiles.".into()));
                    }
                }
                if let Some(mut worker)=mapper.take(){
                    if let Err(error)=worker.close(){send_event(&events,Event::Mapper(false,format!("Host mapper stopped: {error}")));continue;}
                }
                if let Some((device,profile,slot_map))=selection{
                    match profile.validate_for_link(device.link){
                        Ok(notice)=>{
                            let notice=notice.unwrap_or("Bluetooth host mapper starting; waiting for configuration BLE.").to_owned();
                            mapper_device=Some(device.clone());
                            mapper_slot_map=slot_map.clone();
                            mapper=Some(MapperWorker::start(device,profile,slot_map));
                            send_event(&events,Event::Mapper(true,notice));
                        },
                        Err(error)=>send_event(&events,Event::Mapper(false,error.to_string())),
                    }
                }else{send_event(&events,Event::Mapper(false,"Host mapper stopped; physical input released.".into()));}
            },
            Ok(Command::Rumble(path,levels))=>{
                rumble=None;
                let duration=Duration::from_millis(600);
                let magnitude=|level:u8|((u32::from(level.min(5))*u32::from(u16::MAX))/5)as u16;
                match input::rumble::Effect::play(&path,magnitude(levels[0]),magnitude(levels[1]),duration){
                    Ok(effect)=>{rumble=Some((effect,Instant::now()+duration));send_event(&events,Event::Rumble(true,None));},
                    Err(error)=>send_event(&events,Event::Rumble(false,Some(format!("Vibration test failed: {error}")))),
                }
            },
            Ok(Command::SwitchReceiverMode(source))=>{
                if mapper.is_some(){send_event(&events,Event::Error("Stop the host input mapper before switching receiver mode.".into()));continue;}
                let result=(||->Result<String>{
                    let(sent,handback)=match source{
                        Link::Receiver=>{
                            let mut receiver=connected.take().ok_or_else(||invalid("Receiver disconnected; mode command not sent"))?;
                            (receiver.enter_xbox_mode(),Ok(()))
                        },
                        Link::Wired=>{
                            let path=wired.as_ref().ok_or_else(||invalid("Wired receiver disconnected; mode command not sent"))?;
                            let mut connection=Gip::takeover(path)?;
                            let sent=connection.enable_xinput_mode();
                            (sent,connection.close().map(|_|()))
                        },
                        Link::Bluetooth=>return Err(invalid("Receiver mode switching is not available over Bluetooth")),
                    };
                    mode_switch_outcome(source,wait_mode_change(source,&stop),sent,handback)
                })();
                connected=None;wired=None;attempted=None;reload=false;*pad.lock()=None;
                next_find=Instant::now();next_inventory=Instant::now();next_test_open=Instant::now();
                match result{
                    Ok(notice)=>send_event(&events,Event::ModeSwitched(notice)),
                    Err(error)=>send_event(&events,Event::Disconnected(format!("{error} Staged edits retained; rediscovering."))),
                }
            },
            Ok(Command::ConnectBluetooth)=>{
                let result=(||->Result<()>{
                    if receiver::find().is_ok(){return Err(invalid("A receiver is present; disconnect it before using Bluetooth configuration"));}
                    if mapper.is_some(){return Err(invalid("Stop the host mapper before using Bluetooth configuration"));}
                    connected=None;wired=None;attempted=None;*pad.lock()=None;
                    let mut connection=att::Connection::open(None,Duration::from_secs(20),Arc::clone(&stop))?;
                    let profiles=read_bluetooth(&mut connection)?;
                    send_event(&events,Event::BluetoothConnected(connection.address.clone(),profiles));
                    bluetooth=Some(connection);Ok(())
                })();
                if let Err(error)=result{send_event(&events,Event::Error(format!("Bluetooth connection failed: {error}. Hold the profile button for 3 seconds, then Connect Bluetooth.")));}
            },
            Ok(command)=>{
                if let Some(receiver)=&mut connected{
                    let result:Result<Option<Event>>=(||match command{
                        Command::Read=>Ok(Some(Event::Read(read(receiver)?))),
                        Command::Save(image)=>Ok(Some(Event::Saved(save(receiver,&image)?))),
                        Command::Shutdown|Command::ConnectBluetooth|Command::SwitchReceiverMode(_)|Command::TestVisible(_)|Command::Mapper(_)|Command::Auto(_)|Command::Rumble(_,_)=>Ok(None),
                    })();
                    match result{Ok(Some(event))=>send_event(&events,event),Ok(None)=>{},Err(error)=>send_event(&events,Event::Error(failure(&error)))}
                }else if let Some(path)=&wired{
                    let result=if mapper.is_some(){Err(invalid("Stop the host input mapper before wired configuration; gamepad input pauses briefly"))}else{match command{
                        Command::Read=>read_wired(path).map(Event::Read),
                        Command::Save(image)=>save_wired(path,&image).map(Event::Saved),
                        _=>continue,
                    }};
                    match result{Ok(event)=>send_event(&events,event),Err(error)=>send_event(&events,Event::Error(wired_failure(&error)))}
                    next_inventory=Instant::now();next_test_open=Instant::now();
                    continue; // Re-inventory xone's new evdev node after driver handback.
                }else if let Some(connection)=&mut bluetooth{
                    let result=match command{
                        Command::Read=>read_bluetooth(connection).map(Event::Read),
                        Command::Save(image)=>save_bluetooth(connection,&image).map(Event::Saved),
                        _=>continue,
                    };
                    match result{
                        Ok(event)=>send_event(&events,event),
                        Err(error)=>{
                            let _=connection.close();bluetooth=None;*pad.lock()=None;
                            send_event(&events,Event::Disconnected(format!("Bluetooth operation failed: {error}. Staged edits retained; hold the profile button 3 seconds, then Connect Bluetooth.")));
                        },
                    }
                }else if matches!(command,Command::Read){
                    attempted=None;reload=true;next_find=Instant::now();
                }else{send_event(&events,Event::Error("Controller disconnected; operation not sent.".into()));}
            },
            Err(mpsc::RecvTimeoutError::Timeout)=>{},
        }
        if stop.load(Ordering::Relaxed){break;}
        if bluetooth.is_none()&&Instant::now()>=next_find{
            next_find=Instant::now()+Duration::from_secs(1);
            let selection=(||{
                let receiver=receiver::find().ok();
                if receiver.is_some(){select_link(receiver,&[])}else{select_link(None,&gip::discover()?)}
            })();
            match selection{
                Ok(Some((link,path,key)))=>{
                    selection_error=None;
                    if attempted.as_ref()!=Some(&key){
                        // One attempt per appearance; Refresh explicitly retries.
                        attempted=Some(key);connected=None;wired=None;test_reader=None;*pad.lock()=None;
                        receiver_test=test_input::ReceiverTest::default();
                        match link{
                            Link::Receiver=>match connect(&path,&stop,&events,reload){Ok(receiver)=>connected=Some(receiver),Err(error)=>send_event(&events,Event::Error(failure(&error)))},
                            Link::Wired=>{
                                match connect_wired(&path,&events,reload,mapper.is_some()){
                                    Ok(())=>{
                                        wired=Some(path);next_inventory=Instant::now();
                                        next_test_open=Instant::now()+Duration::from_millis(16);
                                    },
                                    Err(error)=>send_event(&events,Event::Error(wired_failure(&error))),
                                }
                            },
                            Link::Bluetooth=>unreachable!(),
                        }
                        reload=false;
                    }
                    else if link==Link::Wired&&wired.is_some(){wired=Some(path);}
                },
                Ok(None)=>{
                    if connected.take().is_some()||wired.take().is_some()||attempted.take().is_some()||reload{send_event(&events,Event::Disconnected("Controller disconnected. Staged edits retained.".into()));}
                    attempted=None;reload=false;selection_error=None;*pad.lock()=None;
                    receiver_test=test_input::ReceiverTest::default();
                },
                Err(error)=>{
                    connected=None;wired=None;attempted=None;reload=false;*pad.lock()=None;
                    receiver_test=test_input::ReceiverTest::default();
                    let notice=error.to_string();
                    if selection_error.as_ref()!=Some(&notice){send_event(&events,Event::Disconnected(notice.clone()));selection_error=Some(notice);}
                },
            }
        }
        if wired.is_some()&&Instant::now()>=next_battery{
            next_battery=Instant::now()+Duration::from_secs(10);
            let battery=gip::discover().ok().and_then(|devices|devices.into_iter()
                .find(|device|Some(&device.node)==wired.as_ref())
                .and_then(|device|gip::battery(&device)));
            send_event(&events,Event::Battery(battery));
        }
        if let Some(receiver)=&mut connected{
            let result=receiver.heartbeat(test_visible,Instant::now()).and_then(|()|receiver.input(Duration::from_millis(4)));
            match result{
                Ok(input)=>{
                    if let Some((state,paddles))=receiver_test.sample(input,auto_epoch.elapsed(),test_visible){
                        publish_pad(&pad,state,paddles);
                    }
                },
                Err(error)=>{connected=None;*pad.lock()=None;send_event(&events,Event::Disconnected(format!("{}. Press any button, then Refresh.",failure(&error))));},
            }
        }
        if let Some(connection)=&mut bluetooth{
            match connection.next_physical(Duration::from_millis(4)){
                Ok(Some(state))=>{let(state,paddles)=test_input::bluetooth(state);publish_pad(&pad,state,paddles);},
                Ok(None)=>{}, // Change-driven BLE silence is not a disconnect.
                Err(error)=>{
                    let _=connection.close();bluetooth=None;*pad.lock()=None;
                    send_event(&events,Event::Disconnected(format!("Bluetooth disconnected: {error}. Staged edits retained; hold the profile button 3 seconds, then Connect Bluetooth.")));
                },
            }
        }
        if !wired_test_enabled(test_visible,mapper.is_some(),wired.is_some()){
            if test_reader.take().is_some(){*pad.lock()=None;}
        }else{
            if test_reader.is_none()&&Instant::now()>=next_test_open{
                next_test_open=Instant::now()+Duration::from_secs(1);
                if let Some(path)=&wired_input{
                    match WiredReader::open(path){
                        Ok(reader)=>{publish_pad(&pad,reader.state(),0);test_reader=Some(reader);test_error=None;},
                        Err(error)=>{
                            let message=format!("Wired input unavailable: {error}. Check evdev read access; no input grab is used.");
                            if test_error.as_ref()!=Some(&message){send_event(&events,Event::Error(message.clone()));test_error=Some(message);}
                        },
                    }
                }
            }
            if let Some(reader)=&mut test_reader{
                match reader.poll(){
                    Ok(Some(state))=>publish_pad(&pad,state,0),Ok(None)=>{},
                    Err(error)=>{
                        test_reader=None;*pad.lock()=None;
                        send_event(&events,Event::Error(format!("Wired input stopped: {error}")));
                    },
                }
            }
        }
    }
    if let Some(mut connection)=bluetooth{let _=connection.close();}
    crate::gui::wake();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state()->(LiveState,Rx<Command>,Sender<Event>){
        let(commands,rx)=mpsc::channel();let(tx,events)=mpsc::channel();
        let mut model=Model{device:Some(Device{name:"test receiver".into(),link:Link::Receiver,pid:0x202f,version_record:None}),..Model::default()};
        model.load(crate::gui::state::defaults().unwrap());
        (LiveState{model,commands,events,pad:Arc::new(Mutex::new(None)),worker:None,stop:Arc::new(AtomicBool::new(false)),store:Store::default()},rx,tx)
    }
    fn input_device(link:LinkKind)->DeviceInfo{
        DeviceInfo{path:PathBuf::from("/not-a-device"),name:"test controller".into(),bus:5,vendor:0x2dc8,product:0x202f,physical:"test/input0".into(),hidraw:None,identity:"test".into(),input_access:true,hidraw_access:false,link}
    }
    #[test]
    fn battery_worker_events_update_clear_and_do_not_leak_across_links(){
        let(mut state,_commands,events)=state();
        let battery=gip::Battery{level:"Low".into(),status:"Discharging".into()};
        events.send(Event::Battery(Some(battery.clone()))).unwrap();
        state.poll().unwrap();assert_eq!(state.model.battery(),None);
        events.send(Event::WiredConnected(PathBuf::from("/not-opened"),crate::gui::state::defaults().unwrap(),false)).unwrap();
        state.poll().unwrap();assert_eq!(state.model.battery(),None);
        events.send(Event::Battery(Some(battery.clone()))).unwrap();
        state.poll().unwrap();assert_eq!(state.model.battery(),Some(&battery));
        events.send(Event::Battery(None)).unwrap();
        state.poll().unwrap();assert_eq!(state.model.battery(),None);
        events.send(Event::Battery(Some(battery))).unwrap();
        events.send(Event::Disconnected("unplugged".into())).unwrap();
        state.poll().unwrap();assert_eq!(state.model.battery,None);
    }
    #[test]
    fn receiver_mode_dispatch_routes_each_usb_link_and_freezes_mapper_start(){
        for link in [Link::Receiver,Link::Wired]{
            let(mut state,commands,_events)=state();
            state.model.device.as_mut().unwrap().link=link;
            state.dispatch(Action::SwitchReceiverMode(link)).unwrap();
            assert!(matches!(commands.try_recv().unwrap(),Command::SwitchReceiverMode(source) if source==link));
            assert!(state.model.operation.is_some());
            assert!(state.dispatch(Action::SetMapper(true)).is_err());
            assert!(commands.try_recv().is_err());
        }
    }
    #[test]
    fn receiver_mode_refuses_dirty_mapper_pending_transfer_demo_and_wrong_links(){
        for case in 0..8{
            let(mut state,commands,_events)=state();
            match case{
                0=>state.model.mapper_running=true,
                1=>state.model.mapper_pending=true,
                2=>state.model.operation=Some(Operation{title:"Saving".into(),detail:String::new()}),
                3=>state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap(),
                4=>state.model.device=None,
                5=>state.model.device.as_mut().unwrap().link=Link::Bluetooth,
                6=>state.model.demo=true,
                7=>state.model.device.as_mut().unwrap().link=Link::Wired,
                _=>unreachable!(),
            }
            let profiles=state.model.profiles.clone();let baseline=state.model.baseline.clone();
            let error=state.dispatch(Action::SwitchReceiverMode(Link::Receiver)).unwrap_err().to_string();
            if case==3{assert!(error.contains("Save or revert"));}
            assert!(commands.try_recv().is_err(),"case {case} must not send");
            assert_eq!(state.model.profiles,profiles);assert_eq!(state.model.baseline,baseline);
        }
    }
    #[test]
    fn receiver_mode_success_requires_new_identity_and_old_identity_gone(){
        for source in [Link::Receiver,Link::Wired]{
            for receiver in [false,true]{for gip in [false,true]{
                let changed=mode_changed(source,receiver,gip);
                let expected=match source{Link::Receiver=>gip&&!receiver,Link::Wired=>receiver&&!gip,_=>false};
                assert_eq!(changed,expected);
                assert_eq!(mode_switch_outcome(source,changed,Ok(()),Ok(())).is_ok(),expected);
                // ESHUTDOWN / ENODEV are normal once re-enumeration proves success.
                let sent=Err(std::io::Error::from_raw_os_error(libc::ESHUTDOWN).into());
                let handback=Err(std::io::Error::from_raw_os_error(libc::ENODEV).into());
                assert_eq!(mode_switch_outcome(source,changed,sent,handback).is_ok(),expected);
            }}
        }
        assert!(!mode_changed(Link::Bluetooth,true,false));
    }
    #[test]
    fn receiver_mode_event_reconnects_without_clearing_staged_edits(){
        let(mut state,_commands,events)=state();
        let baseline=state.model.baseline.clone();
        state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
        let staged=state.model.profiles.clone();
        events.send(Event::ModeSwitched("Receiver switched; reconnecting".into())).unwrap();
        state.poll().unwrap();
        assert!(state.model.device.is_none());assert!(state.model.operation.is_none());
        assert_eq!(state.model.profiles,staged);assert_eq!(state.model.baseline,baseline);
        events.send(Event::WiredConnected(PathBuf::from("/not-a-device"),baseline.unwrap(),false)).unwrap();
        state.poll().unwrap();
        assert_eq!(state.model.device.as_ref().unwrap().link,Link::Wired);
        assert_eq!(state.model.profiles,staged);assert!(state.model.dirty());
        assert!(state.model.operation.is_none());
    }
    #[test]
    fn worker_auto_step_debounces_changes_and_ignores_unchanged_or_off(){
        let at=Duration::from_secs;
        let processes=[profiles::Process{comm:"arena".into(),..Default::default()}];
        let mut auto=None;
        assert!(auto_step(&mut auto,true,at(0),&processes).is_none());
        let config=profiles::AutoConfig{default:"default".into(),rules:vec![profiles::Rule{profile:"game".into(),process:"arena".into()}]};
        auto=Some(AutoState::new(config,"default".into()));
        assert!(auto_step(&mut auto,false,at(1),&processes).is_none());
        assert!(auto_step(&mut auto,false,at(4),&processes).is_none());
        assert!(auto_step(&mut auto,true,at(5),&processes).is_none());
        assert!(auto_step(&mut auto,true,at(6),&processes).is_none());
        assert!(auto_step(&mut auto,true,at(7),&[]).is_none(),"flapping cancels the candidate");
        assert!(auto_step(&mut auto,true,at(8),&processes).is_none());
        let target=auto_step(&mut auto,true,at(10),&processes).expect("stable process must switch without GUI polling");
        assert_eq!(target.name,"game");assert_eq!(target.notice,"Profile: game (matched arena)");
        assert_eq!(auto.as_ref().unwrap().current,"default","selection is committed only after restart");
        auto.as_mut().unwrap().current=target.name;
        assert!(auto_step(&mut auto,true,at(11),&processes).is_none());
        assert!(auto_step(&mut auto,true,at(12),&[]).is_none());
        let target=auto_step(&mut auto,true,at(14),&[]).expect("stable exit must restore default");
        assert_eq!(target.name,"default");assert_eq!(target.notice,"Profile: default (matched default)");
        auto=None;assert!(auto_step(&mut auto,true,at(20),&processes).is_none());
    }
    #[test]
    fn auto_profile_load_error_keeps_current_mapping(){
        let root=std::env::temp_dir().join(format!("ultimate-auto-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
        let _cleanup=Cleanup(root.clone());let store=Store::new(root);store.initialize().unwrap();
        let config=profiles::AutoConfig{default:"default".into(),rules:vec![profiles::Rule{profile:"missing".into(),process:"arena".into()}]};
        let mut auto=Some(AutoState::new(config,"default".into()));
        let processes=[profiles::Process{comm:"arena".into(),..Default::default()}];
        assert!(prepare_auto_switch(&mut auto,Some(LinkKind::Receiver),&store,Duration::ZERO,&processes).unwrap().is_none());
        assert!(prepare_auto_switch(&mut auto,Some(LinkKind::Receiver),&store,Duration::from_secs(2),&processes).is_err());
        assert_eq!(auto.as_ref().unwrap().current,"default");
        let mut game=HostProfile::default();game.mappings.insert("BTN_SOUTH".into(),"BTN_EAST".into());store.create("missing",&game).unwrap();
        let(target,profile)=prepare_auto_switch(&mut auto,Some(LinkKind::Receiver),&store,Duration::from_secs(3),&processes).unwrap().unwrap();
        assert_eq!(target.name,"missing");assert_eq!(profile.mappings["BTN_SOUTH"],"BTN_EAST");
        assert_eq!(auto.as_ref().unwrap().current,"default","preparation must not commit before worker restart");
    }
    #[test]
    fn auto_commands_and_notifications_cross_the_worker_boundary(){
        let(mut state,commands,events)=state();state.model.host_profile_name="default".into();
        state.model.input_devices=vec![input_device(LinkKind::Receiver)];
        state.dispatch(Action::SetAutoEnabled(true)).unwrap();
        let Command::Auto(Some((config,name)))=commands.try_recv().unwrap()else{panic!("expected watcher enable");};
        assert_eq!(name,"default");assert_eq!(config.default,"default");
        state.dispatch(Action::SetMapper(true)).unwrap();
        assert!(matches!(commands.try_recv().unwrap(),Command::Mapper(Some(_))));
        assert!(matches!(commands.try_recv().unwrap(),Command::Auto(Some((_,name))) if name=="default"));
        events.send(Event::Mapper(true,"starting".into())).unwrap();state.poll().unwrap();
        let mut profile=HostProfile::default();profile.mappings.insert("BTN_SOUTH".into(),"BTN_EAST".into());
        events.send(Event::AutoSwitched{name:"game".into(),profile,notice:"Profile: game (matched arena)".into()}).unwrap();
        assert_eq!(state.model.host_profile_name,"default");
        state.poll().unwrap();
        assert_eq!(state.model.host_profile_name,"game");assert_eq!(state.model.host_profile.mappings["BTN_SOUTH"],"BTN_EAST");
        assert_eq!(state.model.notice.as_deref(),Some("Profile: game (matched arena)"));
        state.dispatch(Action::SetAutoEnabled(false)).unwrap();assert!(matches!(commands.try_recv().unwrap(),Command::Auto(None)));
        state.dispatch(Action::SetMapper(false)).unwrap();assert!(matches!(commands.try_recv().unwrap(),Command::Mapper(None)));
    }
    #[test]
    fn configuration_selection_prefers_receiver_then_one_wired_device(){
        let receiver=PathBuf::from("/not-a-receiver");
        let device=|number|gip::Device{
            sysfs:PathBuf::from(format!("/sys/not-a-device/{number}")),
            node:PathBuf::from(format!("/dev/not-a-device/{number}")),
            bus:1,address:number,writable:false,interfaces:vec![],input_nodes:vec![],
        };
        assert!(select_link(None,&[]).unwrap().is_none());
        let wired=[device(1),device(2)];
        let selected=select_link(Some(receiver.clone()),&wired).unwrap().unwrap();
        assert_eq!(selected,(Link::Receiver,receiver.clone(),receiver));
        let selected=select_link(None,&wired[..1]).unwrap().unwrap();
        assert_eq!(selected,(Link::Wired,wired[0].node.clone(),wired[0].sysfs.clone()));
        assert!(select_link(None,&wired).unwrap_err().to_string().contains("Multiple wired"));
    }
    #[test]
    fn wired_save_dispatch_warns_and_waits_for_verified_result(){
        let(mut state,commands,events)=state();
        events.send(Event::WiredConnected(PathBuf::from("/not-a-device"),crate::gui::state::defaults().unwrap(),false)).unwrap();
        events.send(Event::Version(version_label(&[0x03,0x3a,0x01,0x82]))).unwrap();
        state.poll().unwrap();
        assert_eq!(state.model.device.as_ref().unwrap().link,Link::Wired);
        assert_eq!(state.model.device.as_ref().unwrap().version_record.as_deref(),Some("03 3a 01 82"));
        state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
        state.dispatch(Action::Save).unwrap();
        assert!(state.model.operation.as_ref().unwrap().detail.contains("Gamepad input pauses briefly"));
        assert!(state.model.dirty());
        let Command::Save(image)=commands.try_recv().unwrap()else{panic!("expected wired Save")};
        events.send(Event::Saved(verified(&image,&image[..]).unwrap())).unwrap();state.poll().unwrap();
        assert!(!state.model.dirty());assert!(state.model.operation.is_none());
        state.dispatch(Action::Refresh).unwrap();
        assert!(matches!(commands.try_recv().unwrap(),Command::Read));
        assert!(state.model.operation.as_ref().unwrap().detail.contains("Gamepad input pauses briefly"));
    }
    #[test]
    fn wired_transactions_refuse_active_or_pending_mapper(){
        for pending in [false,true]{
            let(mut state,commands,_events)=state();
            state.model.device.as_mut().unwrap().link=Link::Wired;
            state.model.mapper_running=!pending;state.model.mapper_pending=pending;
            state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
            for action in [Action::Save,Action::Refresh,Action::Revert]{
                assert!(state.dispatch(action).unwrap_err().to_string().contains("Stop the host input mapper"));
                assert!(state.model.notice.as_deref().unwrap().contains("gamepad input pauses briefly"));
                assert!(commands.try_recv().is_err());assert!(state.model.operation.is_none());
            }
        }
        let(events,_rx)=mpsc::channel();
        assert!(connect_wired(Path::new("/not-a-device"),&events,false,true).unwrap_err().to_string().contains("Stop the host input mapper"));
    }
    #[test]
    fn wired_permission_error_gets_access_notice_not_generic_failure(){
        let error:crate::Error=std::io::Error::from_raw_os_error(libc::EACCES).into();
        assert!(wired_failure(&error).contains("EACCES"));
        let other:crate::Error=std::io::Error::from_raw_os_error(libc::EIO).into();
        assert!(!wired_failure(&other).contains("EACCES"));
    }
    #[test]
    fn wired_handback_failure_overrides_success_and_keeps_transfer_error(){
        let error=finish_wired(Ok(()),Err(invalid("binding missing"))).unwrap_err().to_string();
        assert!(error.contains("transfer completed"));assert!(error.contains("physical replug may be required"));
        let error=finish_wired::<()>(Err(invalid("write failed")),Err(invalid("binding missing"))).unwrap_err().to_string();
        assert!(error.contains("write failed"));assert!(error.contains("binding missing"));
        let binding=gip::Rebind{connect_result:0,interface0_driver:Some("xone-wired".into()),interface1_driver:None};
        assert!(finish_wired(Ok(()),Ok(binding)).is_ok());
    }
    #[test]
    fn bluetooth_connect_is_explicit_and_save_uses_verified_path(){
        let(mut state,commands,events)=state();
        assert!(state.dispatch(Action::ConnectBluetooth).is_err());
        state.model.device=None;
        state.dispatch(Action::ConnectBluetooth).unwrap();
        assert!(matches!(commands.try_recv().unwrap(),Command::ConnectBluetooth));
        assert!(state.model.operation.as_ref().unwrap().detail.contains("3 seconds"));
        assert!(state.dispatch(Action::ConnectBluetooth).is_err());
        events.send(Event::BluetoothConnected("test-radio".into(),crate::gui::state::defaults().unwrap())).unwrap();
        state.poll().unwrap();
        assert_eq!(state.model.device.as_ref().unwrap().link,Link::Bluetooth);
        assert!(state.model.operation.is_none());assert!(state.model.profiles.is_some());
        assert!(!state.model.can_connect_bluetooth());
        state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
        assert!(state.model.dirty());state.dispatch(Action::Save).unwrap();
        assert!(matches!(commands.try_recv().unwrap(),Command::Save(_)),"verified BLE save goes through the normal Save command");
        events.send(Event::Saved(crate::gui::state::defaults().unwrap())).unwrap();state.poll().unwrap();
        assert!(state.model.operation.is_none());
        state.dispatch(Action::Revert).unwrap();
        assert!(matches!(commands.try_recv().unwrap(),Command::Read));
        events.send(Event::Read(crate::gui::state::defaults().unwrap())).unwrap();state.poll().unwrap();
        assert!(!state.model.dirty());
    }
    #[test]
    fn bluetooth_reconnect_preserves_edits_and_disconnect_allows_explicit_retry(){
        let(mut state,commands,events)=state();
        state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
        events.send(Event::Disconnected("link lost".into())).unwrap();state.poll().unwrap();
        events.send(Event::BluetoothConnected("test-radio".into(),crate::gui::state::defaults().unwrap())).unwrap();state.poll().unwrap();
        assert_eq!(state.model.profiles.as_ref().unwrap().slots[0].motion.sensitivity.get(),3);
        assert!(state.model.dirty());assert!(state.model.pad.is_none());
        events.send(Event::Disconnected("BLE lost".into())).unwrap();state.poll().unwrap();
        state.dispatch(Action::ConnectBluetooth).unwrap();
        assert!(matches!(commands.try_recv().unwrap(),Command::ConnectBluetooth));
    }
    #[test]
    fn mapper_dispatch_selects_detected_link_and_waits_for_worker_events(){
        for link in [LinkKind::Receiver,LinkKind::Bluetooth,LinkKind::Wired]{
            let(mut state,commands,events)=state();
            state.model.device=None;state.model.input_devices=vec![input_device(link)];
            state.model.host_profile.turbo.insert("BTN_SOUTH".into(),12.0);
            state.dispatch(Action::SetMapper(true)).unwrap();
            assert!(state.model.mapper_pending);assert!(!state.model.mapper_running);
            let Command::Mapper(Some((device,profile,_)))=commands.try_recv().unwrap()else{panic!("expected mapper start")};
            assert_eq!(device.link,link);assert_eq!(profile.turbo["BTN_SOUTH"],12.0);
            assert!(matches!(commands.try_recv().unwrap(),Command::Auto(None)));
            assert!(state.dispatch(Action::SetMapper(true)).is_err());
            events.send(Event::Mapper(true,"starting".into())).unwrap();state.poll().unwrap();
            assert!(state.model.mapper_running);assert!(!state.model.mapper_pending);
            assert!(state.dispatch(Action::SetHostProfile(Box::default())).is_err());
            state.dispatch(Action::SetMapper(false)).unwrap();
            assert!(matches!(commands.try_recv().unwrap(),Command::Mapper(None)));
            events.send(Event::Mapper(false,"stopped".into())).unwrap();state.poll().unwrap();
            assert!(!state.model.mapper_running);
        }
    }
    #[test]
    fn rumble_dispatch_uses_current_grip_levels_and_never_sends_without_capability(){
        let(mut state,commands,events)=state();
        assert!(state.dispatch(Action::TestRumble).is_err());assert!(commands.try_recv().is_err());
        state.model.rumble_nodes=vec![PathBuf::from("/not-a-device")];
        state.dispatch(Action::SetVibration{slot:0,motor:crate::profile::VibrationMotor::LeftGrip,value:4}).unwrap();
        state.dispatch(Action::SetVibration{slot:0,motor:crate::profile::VibrationMotor::RightGrip,value:2}).unwrap();
        state.dispatch(Action::TestRumble).unwrap();
        let Command::Rumble(path,levels)=commands.try_recv().unwrap()else{panic!("expected rumble")};
        assert_eq!(path,PathBuf::from("/not-a-device"));assert_eq!(levels,[4,2]);
        assert!(state.dispatch(Action::TestRumble).is_err());
        events.send(Event::Rumble(false,None)).unwrap();state.poll().unwrap();assert!(!state.model.rumble_running);
    }
    #[test]
    fn mapper_guard_stops_and_joins_thread_on_drop(){
        let stop=Arc::new(AtomicBool::new(false));let flag=Arc::clone(&stop);
        let exited=Arc::new(AtomicBool::new(false));let exit_flag=Arc::clone(&exited);
        let handle=thread::spawn(move||{while !flag.load(Ordering::Acquire){thread::yield_now();}exit_flag.store(true,Ordering::Release);Ok(())});
        drop(MapperWorker{stop:Arc::clone(&stop),handle:Some(handle)});
        assert!(stop.load(Ordering::Acquire));assert!(exited.load(Ordering::Acquire));
    }
    #[test]
    fn factory_restore_is_staged_and_uses_the_ordinary_verified_save(){
        let(mut state,commands,events)=state();
        let baseline=state.model.baseline.clone();
        let expected=state.model.profiles.as_ref().unwrap().factory_defaults();
        state.dispatch(Action::RestoreFactoryProfiles{backup:false}).unwrap();
        assert!(commands.try_recv().is_err(),"staging must not contact the controller");
        assert_eq!(state.model.baseline,baseline);assert!(state.model.dirty());
        state.dispatch(Action::Save).unwrap();
        let Command::Save(image)=commands.try_recv().unwrap()else{panic!("expected normal save")};
        assert_eq!(*image,expected.encode());
        events.send(Event::Saved(verified(&image,&image[..]).unwrap())).unwrap();
        state.poll().unwrap();assert!(!state.model.dirty());
    }
    #[test]
    fn save_preserves_baseline_header_flags_and_excludes_staged_flags_from_notice(){
        let(mut state,commands,events)=state();
        let header=state.model.baseline.as_ref().unwrap().encode()[3];
        state.dispatch(Action::SetTournament(true)).unwrap();
        state.dispatch(Action::SetReconnect(true)).unwrap();
        state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
        state.dispatch(Action::Save).unwrap();
        let Command::Save(image)=commands.try_recv().unwrap()else{panic!("expected profile save")};
        assert_eq!(image[3],header);
        events.send(Event::Saved(verified(&image,&image[..]).unwrap())).unwrap();state.poll().unwrap();
        let notice=state.model.notice.as_deref().unwrap();
        assert!(notice.contains("3 bytes changed"));assert!(!notice.contains("0x003"));
        assert!(!state.model.dirty());
        state.dispatch(Action::SetTournament(true)).unwrap();
        state.dispatch(Action::SetReconnect(true)).unwrap();
        state.dispatch(Action::Save).unwrap();
        assert!(commands.try_recv().is_err(),"header-only edit must not issue full write");
        assert_eq!(state.model.notice.as_deref(),Some("Nothing to save"));
        assert!(!state.model.dirty());
    }
    #[test]
    fn save_freezes_edits_and_clears_dirty_only_after_verified_event(){
        let(mut state,commands,events)=state();
        state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
        assert!(state.model.dirty());state.dispatch(Action::Save).unwrap();
        assert!(state.model.dirty());assert!(state.model.operation.is_some());
        assert!(state.dispatch(Action::SetTournament(true)).is_err());
        let Command::Save(image)=commands.try_recv().unwrap()else{panic!("expected save")};
        assert!(crate::profile::verify_crc(&image));
        let p=verified(&image,&image[..]).unwrap();events.send(Event::Saved(p)).unwrap();state.poll().unwrap();
        assert!(!state.model.dirty());assert!(state.model.operation.is_none());
        assert!(state.model.notice.as_deref().unwrap().contains("3 bytes changed, read-back identical"));
        assert!(state.model.notice.as_deref().unwrap().contains("0x102"));
    }
    #[test]
    fn readback_requires_every_byte_including_header_and_crc(){
        let image=crate::gui::state::defaults().unwrap().encode();
        assert!(verified(&image,&image).is_ok());assert!(verified(&image,&image[..867]).is_err());
        for index in 0..IMAGE_SIZE{let mut back=image;back[index]^=1;assert_eq!(verified(&image,&back).is_ok(),index==2||index==3,"byte {index}");}
    }
    #[test]
    fn disconnect_preserves_staged_edits_and_reconnect_does_not_overwrite_them(){
        let(mut state,_commands,events)=state();
        state.dispatch(Action::MotionSensitivity{slot:0,value:3}).unwrap();
        state.dispatch(Action::Save).unwrap();
        events.send(Event::Disconnected("Disconnected during save".into())).unwrap();state.poll().unwrap();
        assert!(state.model.device.is_none());assert!(state.model.operation.is_none());assert!(state.model.dirty());
        assert!(state.dispatch(Action::Save).is_err());
        events.send(Event::Connected(PathBuf::from("/not-a-device"),crate::gui::state::defaults().unwrap(),false)).unwrap();
        state.poll().unwrap();
        assert_eq!(state.model.profiles.as_ref().unwrap().slots[0].motion.sensitivity.get(),3);
        assert!(state.model.dirty());
        events.send(Event::Error("Version record unavailable".into())).unwrap();state.poll().unwrap();
        state.dispatch(Action::Revert).unwrap();events.send(Event::Read(crate::gui::state::defaults().unwrap())).unwrap();state.poll().unwrap();
        assert!(!state.model.dirty());
    }
    #[test]
    fn timeout_notice_gives_an_explicit_wake_action(){
        let error:crate::Error=std::io::Error::new(std::io::ErrorKind::TimedOut,"No acknowledgment").into();
        let notice=failure(&error);assert!(notice.contains("press any button, then Refresh"));
    }
    #[test]
    fn refresh_can_retry_after_disconnect_and_explicitly_replace_edits(){
        let(commands,rx)=mpsc::channel();let(tx,events)=mpsc::channel();
        let stop=Arc::new(AtomicBool::new(false));let mut model=Model::default();model.load(crate::gui::state::defaults().unwrap());
        model.edit(Action::SetTournament(true)).unwrap();
        let mut state=LiveState{model,commands,events,pad:Arc::new(Mutex::new(None)),worker:None,stop:Arc::clone(&stop),store:Store::default()};
        state.dispatch(Action::Revert).unwrap();assert!(matches!(rx.try_recv().unwrap(),Command::Read));
        tx.send(Event::Connected(PathBuf::from("/not-a-device"),crate::gui::state::defaults().unwrap(),true)).unwrap();state.poll().unwrap();
        assert!(!state.model.dirty());assert!(!state.model.profiles.as_ref().unwrap().profile_bits.tournament());
        drop(state);assert!(stop.load(Ordering::Relaxed));
    }
    #[test]
    fn pad_mailbox_coalesces_bursts_before_the_next_gui_frame(){
        let(mut state,_commands,_events)=state();
        let received=Instant::now();
        for value in 0..10000{
            *state.pad.lock()=Some(PadSample{
                state:Pad{left_x:value,..Pad::default()},paddles:0,
                received_at:received,
            });
        }
        assert!(state.poll().unwrap());
        assert_eq!(state.model.pad.unwrap().left_x,9999);
        assert_eq!(state.model.pad_received_at,Some(received));
        assert!(!state.poll().unwrap(),"no backlog remains after consuming newest state");
        // Identical HID samples do not force redraws or inflate latency counts.
        *state.pad.lock()=Some(PadSample{state:state.model.pad.unwrap(),paddles:0,received_at:received+Duration::from_millis(1)});
        assert!(!state.poll().unwrap());
        assert_eq!(state.model.pad_received_at,Some(received));
    }
    fn test_reader()->(Option<WiredReader>,std::os::unix::net::UnixStream){
        let(source,peer)=std::os::unix::net::UnixStream::pair().unwrap();
        peer.set_nonblocking(true).unwrap();
        (Some(WiredReader::test_file(std::fs::File::from(std::os::fd::OwnedFd::from(source)))),peer)
    }
    #[test]
    fn wired_reader_is_closed_before_read_save_mode_mapper_and_page_exit(){
        use std::io::Read;
        for command in [Command::Read,Command::Save(Box::new([0;IMAGE_SIZE])),Command::SwitchReceiverMode(Link::Wired),Command::Mapper(Some((input_device(LinkKind::Wired),HostProfile::default(),None))),Command::TestVisible(false)]{
            let(mut reader,mut peer)=test_reader();
            let pad=Mutex::new(Some(PadSample{state:Pad::default(),paddles:0,received_at:Instant::now()}));
            let mut byte=[0];assert_eq!(peer.read(&mut byte).unwrap_err().kind(),std::io::ErrorKind::WouldBlock);
            release_wired_test(&command,&mut reader,&pad);
            assert!(reader.is_none());assert!(pad.lock().is_none());
            assert_eq!(peer.read(&mut byte).unwrap(),0,"input descriptor must already be closed before executing command");
        }
    }
    #[test]
    fn wired_test_reader_is_never_enabled_while_mapper_runs_or_page_is_hidden(){
        for visible in [false,true]{for mapper in [false,true]{for wired in [false,true]{
            assert_eq!(wired_test_enabled(visible,mapper,wired),visible&&!mapper&&wired);
        }}}
        let(mut state,commands,_events)=state();
        state.set_test_visible(true).unwrap();assert!(matches!(commands.try_recv().unwrap(),Command::TestVisible(true)));
        state.set_test_visible(true).unwrap();assert!(commands.try_recv().is_err(),"unchanged visibility must not flood worker");
        state.set_test_visible(false).unwrap();assert!(matches!(commands.try_recv().unwrap(),Command::TestVisible(false)));
    }
    #[test]
    fn bluetooth_paddle_only_changes_reach_model_and_idle_keeps_last_input(){
        let(mut state,_commands,_events)=state();state.model.device.as_mut().unwrap().link=Link::Bluetooth;
        let pad=Pad::default();
        publish_pad(&state.pad,pad,0);state.poll().unwrap();
        publish_pad(&state.pad,pad,crate::profile::PhysicalButtons::R4.bits());
        assert!(state.poll().unwrap());assert_eq!(state.model.paddles,0x4000_0000);
        assert!(!state.poll().unwrap());assert_eq!(state.model.pad,Some(pad));assert!(state.model.device.is_some());
    }
    #[test]
    fn receiver_test_physical_paddles_reach_model_without_overwriting_xinput() {
        let(mut state,commands,_events)=state();
        state.set_test_visible(true).unwrap();
        assert!(matches!(commands.try_recv().unwrap(),Command::TestVisible(true)));
        let mut reader=test_input::ReceiverTest::default();
        assert!(reader.sample((None,None),Duration::ZERO,true).is_none());
        let raw=crate::protocol::decode_hex("811c040003000c00000000000000000000000020000080808080000004002000").unwrap();
        let physical=receiver::physical_input(&raw).unwrap().unwrap();
        let xinput=Pad{buttons:4096,right_x:1234,..Pad::default()};
        let(pad,paddles)=reader.sample((Some(xinput),Some(physical)),Duration::ZERO,true).unwrap();
        publish_pad(&state.pad,pad,paddles);state.poll().unwrap();
        assert_eq!(state.model.pad,Some(xinput));assert_eq!(state.model.paddles,0, "captured A is not a paddle");
        for mask in [0x00200000,0x40000000,0x02000000,0x04000000] {
            let(pad,paddles)=reader.sample((None,Some(crate::protocol::PhysicalInput{buttons:mask,..physical})),Duration::ZERO,true).unwrap();
            publish_pad(&state.pad,pad,paddles);state.poll().unwrap();
            assert_eq!(state.model.paddles,mask);assert_eq!(state.model.pad,Some(xinput));
        }
        let(pad,paddles)=reader.sample((Some(xinput),None),Duration::from_secs(10),true).unwrap();
        publish_pad(&state.pad,pad,paddles);state.poll().unwrap();assert_eq!(state.model.paddles,0x04000000,"held until the next physical report");
        assert_eq!(reader.sample((None,Some(physical)),Duration::from_secs(2),false).unwrap().1,0);
    }
    #[test]
    fn connected_worker_checks_commands_without_a_pre_read_wait(){
        let(sender,receiver)=mpsc::channel();
        assert!(matches!(next_command(&receiver,true),Err(mpsc::RecvTimeoutError::Timeout)));
        sender.send(Command::Read).unwrap();
        assert!(matches!(next_command(&receiver,true),Ok(Command::Read)));
        drop(sender);
        assert!(matches!(next_command(&receiver,true),Err(mpsc::RecvTimeoutError::Disconnected)));
    }
}

#[cfg(test)]
mod gui_save_regression{
    use super::*;
    #[test]
    fn sensitivity_pointer_and_keyboard_save_slot_one_when_runtime_slot_is_ff(){
        for(pointer,start,target)in [(false,5,2),(true,5,2),(false,2,5),(true,2,5)]{
            let(commands,rx)=mpsc::channel();let(events,events_rx)=mpsc::channel();
            let mut image=crate::gui::state::defaults().unwrap().encode();image[2]=0xff;image[3]=0x18;image[0x102]=start;
            let mut profiles=Profiles::decode(&image).unwrap();profiles.fix_crc();
            let mut model=Model{device:Some(Device{name:"test".into(),link:Link::Receiver,pid:0x202f,version_record:None}),notice:Some("Connected test receiver; edits stay staged until Save.".into()),..Model::default()};
            model.load(profiles);
            let mut state=LiveState{model,commands,events:events_rx,pad:Arc::new(Mutex::new(None)),worker:None,stop:Arc::new(AtomicBool::new(false)),store:Store::default()};
            crate::gui::app::exercise_motion_save(&mut state,pointer,target);
            let Command::Save(written)=rx.try_recv().expect("writer must receive the edited image")else{panic!("not Save")};
            assert_eq!(written[0x102],target,"pointer={pointer}: slot 1 stored sensitivity");
            assert_eq!(written[0x222],5,"slot 2 must remain unchanged");assert_eq!(written[0x342],5,"slot 3 must remain unchanged");
            assert_eq!(written[2],0xff,"editing slot 1 must not invent activation of it");
            assert!(crate::profile::verify_crc(&written));
            events.send(Event::Saved(verified(&written,&written[..]).unwrap())).unwrap();state.poll().unwrap();
            assert!(state.model.notice.as_deref().unwrap().contains("3 bytes changed, read-back identical"));
            assert!(state.model.notice.as_deref().unwrap().contains("0x102"));
        }
    }
    #[test]
    fn unchanged_save_does_not_send_a_write(){
        let(commands,rx)=mpsc::channel();let(_events,events_rx)=mpsc::channel();
        let mut model=Model{device:Some(Device{name:"test".into(),link:Link::Receiver,pid:0x202f,version_record:None}),..Model::default()};
        model.load(crate::gui::state::defaults().unwrap());
        let mut state=LiveState{model,commands,events:events_rx,pad:Arc::new(Mutex::new(None)),worker:None,stop:Arc::new(AtomicBool::new(false)),store:Store::default()};
        state.dispatch(Action::Save).unwrap();
        assert!(rx.try_recv().is_err(),"unchanged Save must not enqueue a write");
        assert_eq!(state.model.notice.as_deref(),Some("Nothing to save"));
    }
}
