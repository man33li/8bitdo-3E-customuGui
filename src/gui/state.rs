//! Lossless GUI editing: the working model is the codec, never a parallel profile.
use crate::{Result, invalid, profile::*, receiver::Pad};
use crate::input::{DeviceInfo,HostProfile,LinkKind};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link { Receiver, Wired, Bluetooth }
/// Verified live 2026-10-15: identity write, real edit (slot-1 sensitivity) read back over
/// Bluetooth and again over the receiver after a mode switch; reporting restored.
pub const BLE_WRITE_VERIFIED: bool = true;
#[derive(Clone, Debug)]
pub struct Device { pub name:String, pub link:Link, pub pid:u16, pub version_record:Option<String> }
#[derive(Clone, Debug)]
pub struct Operation { pub title:String, pub detail:String }
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub demo:bool, pub device:Option<Device>, pub profiles:Option<Profiles>,
    /// Passive xone sysfs level/status, refreshed by the worker at most every 10 s.
    pub battery:Option<crate::gip::Battery>,
    pub baseline:Option<Profiles>, pub active_slot:usize, pub pad:Option<Pad>,
    /// Worker receipt time of the current displayed pad state; not device time.
    pub pad_received_at:Option<std::time::Instant>,
    /// Physical BLE/receiver paddle masks; not derived from XInput reports.
    pub paddles:u32,
    pub test_visible:bool,
    pub operation:Option<Operation>, pub notice:Option<String>,
    pub host_profile:HostProfile, pub mapper_running:bool, pub mapper_pending:bool,
    pub host_profile_name:String, pub host_profile_names:Vec<String>,
    pub auto_config:crate::profiles::AutoConfig, pub auto_enabled:bool,
    pub input_devices:Vec<DeviceInfo>, pub rumble_nodes:Vec<PathBuf>, pub rumble_running:bool,
    pub test_levels:[u8;4],
}
impl Model {
    pub fn battery(&self)->Option<&crate::gip::Battery>{
        self.device.as_ref().filter(|device|device.link==Link::Wired)?;
        self.battery.as_ref()
    }
    pub fn can_save(&self)->bool{
        self.demo||self.device.as_ref().is_some_and(|d|d.link!=Link::Bluetooth||BLE_WRITE_VERIFIED)
    }
    pub fn can_connect_bluetooth(&self)->bool{
        !self.demo&&self.device.is_none()&&!self.mapper_running&&!self.mapper_pending&&self.operation.is_none()
    }
    pub fn receiver_mode_link(&self)->Option<Link>{
        if self.demo{return None;}
        self.device.as_ref().and_then(|device|match device.link{
            // Windows can't switch back yet (GIP backend gated), so never offer PC→Xbox there.
            Link::Receiver if !cfg!(windows)=>Some(Link::Receiver),
            Link::Wired=>Some(Link::Wired),
            _=>None,
        })
    }
    pub fn mapper_device(&self)->Result<&DeviceInfo>{
        let link=self.device.as_ref().map(|d|match d.link{Link::Receiver=>LinkKind::Receiver,Link::Wired=>LinkKind::Wired,Link::Bluetooth=>LinkKind::Bluetooth});
        let mut candidates=self.input_devices.iter().filter(|d|link.is_none_or(|link|d.link==link));
        let selected=candidates.next().ok_or_else(||invalid("No supported receiver, Bluetooth or wired input node detected"))?;
        if candidates.next().is_some(){return Err(invalid("Multiple controllers on this link; disconnect all but the intended controller"));}
        Ok(selected)
    }
    pub fn host_notice(&self)->Result<Option<&'static str>>{
        self.host_profile.validate_for_link(self.mapper_device()?.link)
    }
    pub fn rumble_levels(&self)->[u8;2]{
        self.profiles.as_ref().map(|p|{
            let feature=p.slots[self.active_slot.min(2)].feature;
            [feature.vibration(VibrationMotor::LeftGrip).get(),feature.vibration(VibrationMotor::RightGrip).get()]
        }).unwrap_or([self.test_levels[0],self.test_levels[1]])
    }
    pub fn can_test_rumble(&self)->bool{!self.demo&&!self.rumble_nodes.is_empty()}
    pub fn dirty(&self)->bool { self.profiles != self.baseline }
    pub fn slot_dirty(&self,slot:usize)->bool {
        match (&self.profiles,&self.baseline) { (Some(a),Some(b))=>a.slots[slot]!=b.slots[slot],_=>false }
    }
    /// Only the last read/save baseline describes controller state; edits are staged.
    pub fn mapper_key_map(&self)->Option<KeyMap>{
        self.baseline.as_ref()?.selected_slot().map(|slot|slot.key_map.clone())
    }
    pub fn load(&mut self,profiles:Profiles) {
        self.active_slot=if profiles.current_slot.get()<3{usize::from(profiles.current_slot.get())}else{0};
        self.baseline=Some(profiles.clone()); self.profiles=Some(profiles);
    }
    pub fn edit(&mut self,action:Action)->Result<()> {
        if self.operation.is_some(){return Err(invalid("Wait for the device operation to finish"));}
        match action {
            Action::SetHostProfile(profile)=>{
                if self.mapper_running||self.mapper_pending{return Err(invalid("Stop the host mapper before editing its profile"));}
                self.host_profile=*profile;return Ok(());
            },
            Action::SetTestLevel{motor,value}=>{
                if value>5{return Err(invalid("Vibration test level must be in 0..=5"));}
                self.test_levels[motor.index()]=value;return Ok(());
            },
            Action::RestoreFactoryProfiles{backup}=>{
                let defaults=self.profiles.as_ref().ok_or_else(||invalid("Read controller profiles before restoring defaults"))?.factory_defaults();
                let saved=if backup{
                    if self.demo{return Err(invalid("Demo does not write backup files"));}
                    let image=self.baseline.as_ref().ok_or_else(||invalid("Read controller profiles before saving a backup"))?.encode();
                    let home=std::env::var_os("HOME").ok_or_else(||invalid("HOME is not set; cannot save backup"))?;
                    let timestamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
                    let path=PathBuf::from(home).join(format!("ultimate-backup-{timestamp}.bin"));
                    crate::protocol::save_backup(&path,&image)?;
                    Some(path)
                }else{None};
                self.profiles=Some(defaults);
                self.notice=Some(match saved{
                    Some(path)=>format!("Backup saved to {}. Factory profiles staged; Save + verify is required to persist them.",path.display()),
                    None=>"Factory profiles staged; Save + verify is required to persist them.".into(),
                });
                return Ok(());
            },
            _=>{},
        }
        let p=self.profiles.as_mut().ok_or_else(||invalid("Connect and read profiles before editing"))?;
        match action {
            Action::SelectProfile(slot) if slot<3=>{p.set_current_slot(slot as u16)?;self.active_slot=slot;},
            Action::SetTournament(value)=>p.profile_bits.set_tournament(value),
            Action::SetReconnect(value)=>p.profile_bits.set_auto_reconnect(value),
            Action::CopyProfile{from,to} if from<3&&to<3=>p.slots[to]=p.slots[from].clone(),
            Action::ResetProfile(slot) if slot<3=>p.slots[slot]=defaults()?.slots[slot].clone(),
            Action::DeleteProfile(slot) if slot<3=>p.slots[slot].flag=0,
            Action::RenameProfile{slot,name} if slot<3=>p.slots[slot].name.set(&name)?,
            Action::SetMapping{slot,source,target}=>slot_mut(p,slot)?.key_map.set(source,target)?,
            Action::SetCurve{slot,side,point,axis,value}=>{
                let c=&mut slot_mut(p,slot)?.dead_zone.curves[side.index()];
                let old=*c.points.get(point).ok_or_else(||invalid("Invalid curve point"))?;
                let (x,y)=if axis==Axis::X{(u16::from(value),u16::from(old.output.get()))}else{(u16::from(old.input.get()),u16::from(value))};
                if point==0||point==3 { c.set_endpoint(point==3,x,y)?; } else { c.set_custom_point(point,x,y)?; }
            },
            Action::SetPreset{slot,side,preset}=>slot_mut(p,slot)?.dead_zone.curves[side.index()].apply_preset(preset),
            Action::SetJitter{slot,side,value}=>slot_mut(p,slot)?.dead_zone.set_jitter(side,u16::from(value))?,
            Action::SetInvert{slot,side,axis,value}=>slot_mut(p,slot)?.feature.set_inverted(side,axis,value),
            Action::SetSwap{slot,value}=>slot_mut(p,slot)?.feature.set_stick_swap(value),
            Action::SetShape{slot,value}=>slot_mut(p,slot)?.feature.set_shape(value)?,
            Action::SetTrigger{slot,side,upper,value}=>{let z=&mut slot_mut(p,slot)?.dead_zone;if upper{z.set_trigger_upper(side,u16::from(value))?}else{z.set_trigger_lower(side,u16::from(value))?}},
            Action::SwapTriggers{slot,value}=>slot_mut(p,slot)?.feature.set_swap_triggers(value),
            Action::SetVibration{slot,motor,value}=>slot_mut(p,slot)?.feature.set_vibration(motor,u16::from(value))?,
            Action::SetSync{slot,value}=>slot_mut(p,slot)?.feature.set_impulse_sync(value),
            Action::MotionTarget{slot,value}=>slot_mut(p,slot)?.motion.feature.set_target(value)?,
            Action::MotionMethod{slot,value}=>slot_mut(p,slot)?.motion.feature.set_enable_method(value),
            Action::MotionButton{slot,value}=>slot_mut(p,slot)?.motion.set_enable_button(value)?,
            Action::MotionSensitivity{slot,value}=>slot_mut(p,slot)?.motion.set_sensitivity(u16::from(value))?,
            Action::MotionCompensation{slot,value}=>slot_mut(p,slot)?.motion.set_compensation(u16::from(value))?,
            Action::LightEffect{slot,value}=>slot_mut(p,slot)?.lighting.set_effect(value)?,
            Action::LightSpeed{slot,value}=>slot_mut(p,slot)?.lighting.set_speed(u16::from(value))?,
            Action::LightColor{slot,index,value} if index<2=>{let l=&mut slot_mut(p,slot)?.lighting;match l.effect{LightEffect::Fixed=>l.set_fixed_color(value),LightEffect::FireRing=>l.fire_ring[index]=value,LightEffect::LightTracing=>l.light_tracing[index]=value,_=>return Err(invalid("Select a lighting effect before editing its color"))}},
            _=>return Err(invalid("Invalid profile edit")),
        }
        Ok(())
    }
}
fn slot_mut(p:&mut Profiles,slot:usize)->Result<&mut Slot>{p.slots.get_mut(slot).ok_or_else(||invalid("Invalid profile slot"))}

/// Structural defaults from 0x180c80ce0/E-RESET, not captured user settings.
pub fn defaults()->Result<Profiles> {
    let mut p=Profiles::decode(&[0;IMAGE_SIZE])?.factory_defaults();
    for (i,s) in p.slots.iter_mut().enumerate(){
        s.flag=RECORD_MAGIC;s.name.set(["Config 1","Config 2","Config 3"][i])?;
        s.key_map.flag=RECORD_MAGIC;s.feature_flag=RECORD_MAGIC;s.lighting.flag=RECORD_MAGIC;
        s.motion.flag=RECORD_MAGIC;s.dead_zone.flag=RECORD_MAGIC;
    }
    p.fix_crc();Ok(p)
}
#[derive(Clone, Debug)]
pub enum Action {
    Refresh, Revert, Save, ConnectBluetooth, SwitchReceiverMode(Link), DismissNotice, SelectProfile(usize), SetTournament(bool),SetReconnect(bool),
    SetMapper(bool), SetHostProfile(Box<HostProfile>), SaveHostProfile, TestRumble,
    SelectHostProfile(String), NewHostProfile(String), RenameHostProfile(String), DeleteHostProfile,
    SetAutoEnabled(bool), SetAutoConfig(crate::profiles::AutoConfig),
    RestoreFactoryProfiles{backup:bool},
    SetTestLevel{motor:VibrationMotor,value:u8},
    CopyProfile{from:usize,to:usize},ResetProfile(usize),DeleteProfile(usize),RenameProfile{slot:usize,name:String},
    SetMapping{slot:usize,source:PhysicalButton,target:MappingTarget},
    SetCurve{slot:usize,side:Side,point:usize,axis:Axis,value:u8},SetPreset{slot:usize,side:Side,preset:CurvePreset},
    SetJitter{slot:usize,side:Side,value:u8},SetInvert{slot:usize,side:Side,axis:Axis,value:bool},SetSwap{slot:usize,value:StickSwap},SetShape{slot:usize,value:StickShape},
    SetTrigger{slot:usize,side:Side,upper:bool,value:u8},SwapTriggers{slot:usize,value:bool},
    SetVibration{slot:usize,motor:VibrationMotor,value:u8},SetSync{slot:usize,value:bool},
    MotionTarget{slot:usize,value:MotionTarget},MotionMethod{slot:usize,value:EnableMethod},MotionButton{slot:usize,value:PhysicalButtons},
    MotionSensitivity{slot:usize,value:u8},MotionCompensation{slot:usize,value:u8},
    LightEffect{slot:usize,value:LightEffect},LightSpeed{slot:usize,value:u8},LightColor{slot:usize,index:usize,value:Color},
}
pub const TARGET_LABELS:[&str;17]=["A","B","X","Y","LB","LT","L3","RB","RT","R3","View","Menu","Up","Down","Left","Right","Null"];
pub const ENABLE_LABELS:[&str;21]=["A","B","X","Y","LB","LT","L3","RB","RT","R3","View","Menu","Up","Down","Left","Right","P1 (PL)","P2 (PR)","L4","R4","None"];
pub const ENABLE_BUTTONS:[PhysicalButtons;21]=[PhysicalButtons::A,PhysicalButtons::B,PhysicalButtons::X,PhysicalButtons::Y,PhysicalButtons::LB,PhysicalButtons::LT,PhysicalButtons::L3,PhysicalButtons::RB,PhysicalButtons::RT,PhysicalButtons::R3,PhysicalButtons::VIEW,PhysicalButtons::MENU,PhysicalButtons::UP,PhysicalButtons::DOWN,PhysicalButtons::LEFT,PhysicalButtons::RIGHT,PhysicalButtons::P1,PhysicalButtons::P2,PhysicalButtons::L4,PhysicalButtons::R4,PhysicalButtons::NONE];
pub trait GuiState {
    fn model(&self)->&Model;
    fn poll(&mut self)->Result<bool>;
    fn dispatch(&mut self,action:Action)->Result<()>;
    /// Demo and other non-hardware states need no input-page lifecycle.
    fn set_test_visible(&mut self,_visible:bool)->Result<()>{Ok(())}
}
pub struct DemoState { model:Model, host_profiles:std::collections::BTreeMap<String,HostProfile> }
impl Default for DemoState{fn default()->Self{Self::new()}}
impl DemoState {pub fn new()->Self{
    let mut model=Model{demo:true,device:Some(Device{name:"Ultimate 3E (demo)".into(),link:Link::Receiver,pid:0x202f,version_record:None}),notice:Some("Demo: memory only; no device I/O.".into()),host_profile_name:"default".into(),host_profile_names:vec!["default".into()],..Model::default()};
    model.load(defaults().expect("structural defaults"));
    let host_profiles=std::collections::BTreeMap::from([("default".into(),model.host_profile.clone())]);Self{model,host_profiles}
}}
impl GuiState for DemoState {
    fn model(&self)->&Model{&self.model}
    fn poll(&mut self)->Result<bool>{Ok(false)}
    fn dispatch(&mut self,action:Action)->Result<()>{match action{
        Action::DismissNotice=>self.model.notice=None,
        Action::SetMapper(_)|Action::TestRumble|Action::ConnectBluetooth|Action::SwitchReceiverMode(_)=>return Err(invalid("Demo never starts hardware input, Bluetooth discovery, mode switching or rumble")),
        Action::SaveHostProfile=>{self.host_profiles.insert(self.model.host_profile_name.clone(),self.model.host_profile.clone());self.model.notice=Some("Demo host profile remains in memory.".into());},
        Action::SelectHostProfile(name)=>{self.model.host_profile=self.host_profiles.get(&name).ok_or_else(||invalid("Unknown host profile"))?.clone();self.model.host_profile_name=name;},
        Action::NewHostProfile(name)=>{
            crate::profiles::Store::default().path(&name)?;
            if self.host_profiles.contains_key(&name){return Err(invalid("Host profile already exists"));}
            self.host_profiles.insert(name.clone(),self.model.host_profile.clone());self.model.host_profile_name=name;self.model.host_profile_names=self.host_profiles.keys().cloned().collect();
        },
        Action::RenameHostProfile(name)=>{
            crate::profiles::Store::default().path(&name)?;let old=self.model.host_profile_name.clone();
            if name!=old&&self.host_profiles.contains_key(&name){return Err(invalid("Host profile already exists"));}
            self.host_profiles.remove(&old);self.host_profiles.insert(name.clone(),self.model.host_profile.clone());
            if self.model.auto_config.default==old{self.model.auto_config.default=name.clone();}
            for rule in &mut self.model.auto_config.rules{if rule.profile==old{rule.profile=name.clone();}}
            self.model.host_profile_name=name;self.model.host_profile_names=self.host_profiles.keys().cloned().collect();
        },
        Action::DeleteHostProfile=>{
            if self.model.host_profile_name==self.model.auto_config.default{return Err(invalid("Cannot delete the default host profile"));}
            if self.model.auto_config.rules.iter().any(|rule|rule.profile==self.model.host_profile_name){return Err(invalid("Remove rules for this profile first"));}
            self.host_profiles.remove(&self.model.host_profile_name);self.model.host_profile_name=self.model.auto_config.default.clone();
            self.model.host_profile=self.host_profiles[&self.model.host_profile_name].clone();self.model.host_profile_names=self.host_profiles.keys().cloned().collect();
        },
        Action::SetAutoEnabled(enabled)=>self.model.auto_enabled=enabled,
        Action::SetAutoConfig(config)=>{
            config.validate()?;
            if !self.host_profiles.contains_key(&config.default)||config.rules.iter().any(|rule|!self.host_profiles.contains_key(&rule.profile)){return Err(invalid("Unknown host profile"));}
            self.model.auto_config=config;
        },
        Action::Save=>{self.model.baseline=self.model.profiles.clone();self.model.notice=Some("Demo saved in memory, not to hardware.".into());},
        Action::Refresh|Action::Revert=>{if let Some(p)=self.model.baseline.clone(){self.model.load(p);}},
        edit=>self.model.edit(edit)?
    }Ok(())}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model()->Model{let mut m=Model::default();m.load(defaults().unwrap());m}
    #[test]
    fn battery_is_visible_only_on_wired_with_available_kernel_data(){
        let battery=crate::gip::Battery{level:"Low".into(),status:"Discharging".into()};
        let mut m=Model{battery:Some(battery.clone()),..Model::default()};
        assert_eq!(m.battery(),None);
        for link in [Link::Receiver,Link::Bluetooth,Link::Wired]{
            m.device=Some(Device{name:"test".into(),link,pid:0x202f,version_record:None});
            assert_eq!(m.battery(),if link==Link::Wired{Some(&battery)}else{None});
        }
        m.battery=None;
        assert_eq!(m.battery(),None);
    }
    pub(crate) fn input_device(link:LinkKind)->DeviceInfo{
        DeviceInfo{path:PathBuf::from("/not-a-device"),name:"test controller".into(),bus:match link{LinkKind::Bluetooth=>5,LinkKind::Wired=>6,LinkKind::Receiver=>3},vendor:0x2dc8,product:if link==LinkKind::Receiver{0x3024}else{0x202f},physical:"test/input0".into(),hidraw:None,identity:"test".into(),input_access:true,hidraw_access:false,link}
    }
    #[test]
    fn named_profiles_demo_edits_selected_profile_without_device_or_file_io(){
        let mut state=DemoState::new();
        state.dispatch(Action::NewHostProfile("Shooter".into())).unwrap();
        let mut profile=state.model().host_profile.clone();profile.mappings.insert("BTN_SOUTH".into(),"KEY_SPACE".into());
        state.dispatch(Action::SetHostProfile(Box::new(profile))).unwrap();state.dispatch(Action::SaveHostProfile).unwrap();
        state.dispatch(Action::SelectHostProfile("default".into())).unwrap();assert!(state.model().host_profile.mappings.is_empty());
        assert!(state.dispatch(Action::DeleteHostProfile).is_err());
        state.dispatch(Action::SetAutoConfig(crate::profiles::AutoConfig{default:"default".into(),rules:vec![crate::profiles::Rule{process:"game".into(),profile:"Shooter".into()}]})).unwrap();
        state.dispatch(Action::SelectHostProfile("Shooter".into())).unwrap();
        assert_eq!(state.model().host_profile.mappings["BTN_SOUTH"],"KEY_SPACE");
        state.dispatch(Action::RenameHostProfile("Arena".into())).unwrap();
        assert_eq!(state.model().auto_config.rules[0].profile,"Arena");
        assert!(state.dispatch(Action::DeleteHostProfile).is_err());
        state.dispatch(Action::SetAutoConfig(crate::profiles::AutoConfig::default())).unwrap();
        state.dispatch(Action::DeleteHostProfile).unwrap();
        assert_eq!(state.model().host_profile_name,"default");assert_eq!(state.model().host_profile_names,["default"]);
        assert!(!state.model().auto_enabled);state.dispatch(Action::SetAutoEnabled(true)).unwrap();assert!(state.model().auto_enabled);
    }
    #[test]
    fn host_edits_are_independent_and_runtime_validation_follows_link(){
        let mut m=Model{input_devices:vec![input_device(LinkKind::Receiver)],..Model::default()};
        let mut host=m.host_profile.clone();
        host.turbo.insert("BTN_TRIGGER_HAPPY2".into(),10.0);
        host.toggle.push("BTN_SOUTH".into());
        host.mappings.insert("BTN_SOUTH".into(),"KEY_SPACE".into());
        host.stick_mouse=Some(crate::input::StickMouse{stick:crate::input::Stick::Right,sensitivity:1200.0,deadzone:0.2});
        host.trigger_buttons.left=Some(crate::input::TriggerButton{destination:"BTN_LEFT".into(),threshold:0.4});
        m.edit(Action::SetHostProfile(Box::new(host))).unwrap();
        assert!(!m.dirty());assert!(m.profiles.is_none());
        assert_eq!(m.host_notice().unwrap(),Some(crate::input::RECEIVER_NOTICE));
        m.input_devices[0].link=LinkKind::Bluetooth;
        assert!(m.host_notice().unwrap_err().to_string().contains("clutch"));
        m.mapper_running=true;
        assert!(m.edit(Action::SetHostProfile(Box::default())).is_err());
    }
    #[test]
    fn factory_restore_stages_all_slots_without_replacing_baseline(){
        let mut m=model();m.edit(Action::SetTournament(true)).unwrap();
        m.edit(Action::SetReconnect(true)).unwrap();
        let baseline=m.baseline.clone();
        let expected=m.profiles.as_ref().unwrap().factory_defaults();
        m.edit(Action::RestoreFactoryProfiles{backup:false}).unwrap();
        assert_eq!(m.profiles,Some(expected));assert_eq!(m.baseline,baseline);
        assert!(m.dirty());assert!(m.profiles.as_ref().unwrap().crc_valid());
        m.operation=Some(Operation{title:"busy".into(),detail:String::new()});
        assert!(m.edit(Action::RestoreFactoryProfiles{backup:false}).is_err());
    }
    #[test]
    fn backup_failure_does_not_discard_staged_profiles(){
        let mut m=model();m.demo=true;
        m.edit(Action::SetTournament(true)).unwrap();
        let profiles=m.profiles.clone();let baseline=m.baseline.clone();
        assert!(m.edit(Action::RestoreFactoryProfiles{backup:true}).is_err());
        assert_eq!(m.profiles,profiles);assert_eq!(m.baseline,baseline);
    }
    #[test]
    fn rumble_visibility_and_levels_do_not_require_controller_profiles(){
        let mut m=Model::default();assert!(!m.can_test_rumble());
        m.rumble_nodes.push(PathBuf::from("/not-a-device"));assert!(m.can_test_rumble());
        m.edit(Action::SetTestLevel{motor:VibrationMotor::LeftGrip,value:4}).unwrap();
        assert_eq!(m.rumble_levels(),[4,0]);
        assert!(m.edit(Action::SetTestLevel{motor:VibrationMotor::RightGrip,value:6}).is_err());
        m.demo=true;assert!(!m.can_test_rumble());
        m.load(defaults().unwrap());
        m.edit(Action::SetVibration{slot:0,motor:VibrationMotor::RightGrip,value:2}).unwrap();
        assert_eq!(m.rumble_levels()[1],2);
    }
    #[test]
    fn mapper_selection_requires_one_controller_on_detected_link(){
        let mut m=Model{input_devices:vec![input_device(LinkKind::Bluetooth),input_device(LinkKind::Receiver)],..Model::default()};
        assert!(m.mapper_device().is_err());
        m.device=Some(Device{name:"receiver".into(),link:Link::Receiver,pid:0x202f,version_record:None});
        assert_eq!(m.mapper_device().unwrap().link,LinkKind::Receiver);
        m.input_devices.push(input_device(LinkKind::Receiver));assert!(m.mapper_device().is_err());
    }
    #[test]
    fn mapper_slot_map_uses_last_read_not_staged_selection_or_remaps(){
        let mut m=model();
        let saved=m.mapper_key_map().unwrap();
        m.edit(Action::SetMapping{slot:0,source:PhysicalButton::P1,target:MappingTarget::L3}).unwrap();
        m.edit(Action::SelectProfile(2)).unwrap();
        assert_eq!(m.mapper_key_map(),Some(saved));
        let mut bytes=m.profiles.as_ref().unwrap().encode();
        bytes[2]=0xff;
        m.load(Profiles::decode(&bytes).unwrap());
        assert!(m.mapper_key_map().is_none(),"firmware default slot has no map");
    }
    fn check(mut m:Model,action:Action,patch:&[(usize,u8)]){
        let before=m.profiles.as_ref().unwrap().encode();let mut expected=before;
        for &(at,value) in patch{expected[at]=value;}
        m.edit(action).unwrap();assert_eq!(m.profiles.as_ref().unwrap().encode(),expected);
        assert_eq!(m.dirty(),expected!=before);
    }
    fn feature(action:Action,mask:u32,value:u32){
        let m=model();let raw=m.profiles.as_ref().unwrap().slots[0].feature.raw();
        let bytes=((raw&!mask)|value).to_le_bytes();
        check(m,action,&std::array::from_fn::<_,4,_>(|i|(0x80+i,bytes[i])));
    }
    #[test]
    fn model_roundtrip_preserves_every_imported_byte(){
        let image=std::array::from_fn(|i|(i.wrapping_mul(73)^0xa5)as u8);
        let mut m=Model::default();m.load(Profiles::decode(&image).unwrap());
        assert_eq!(m.profiles.as_ref().unwrap().encode(),image);assert!(!m.dirty());
        assert_eq!(m.profiles.as_ref().unwrap().current_slot.get(),image[2]);
    }
    #[test]
    fn gui_controls_change_only_their_spec_bytes(){
        for slot in 0..3{
            let s=4+slot*SLOT_SIZE;
            for source in PhysicalButton::ALL{
                let target=if source==PhysicalButton::B{MappingTarget::A}else{MappingTarget::B};
                let bytes=target.raw().to_le_bytes();
                check(model(),Action::SetMapping{slot,source,target},&std::array::from_fn::<_,4,_>(|i|(s+0x28+source.index()*4+i,bytes[i])));
            }
            check(model(),Action::SelectProfile(slot),&[(2,slot as u8)]);
            for side in [Side::Left,Side::Right]{
                for point in 0..4{
                    for axis in [Axis::X,Axis::Y]{
                        let value=match(point,axis){(0,Axis::X)=>9,(0,Axis::Y)=>1,(1,Axis::X)=>38,(1,Axis::Y)=>34,(2,Axis::X)=>67,(2,Axis::Y)=>67,(3,Axis::X)=>96,_=>99};
                        check(model(),Action::SetCurve{slot,side,point,axis,value},&[(s+0x108+side.index()*8+point*2+usize::from(axis==Axis::Y),value)]);
                    }
                }
                check(model(),Action::SetJitter{slot,side,value:3},&[(s+0x118+side.index(),3)]);
                for upper in [false,true]{check(model(),Action::SetTrigger{slot,side,upper,value:123},&[(s+if upper{0x11c}else{0x11a}+side.index(),123)]);}
                for(preset,values)in [(CurvePreset::Default,[36,33,65,66]),(CurvePreset::Dynamic,[36,16,65,83]),(CurvePreset::Precision,[36,16,73,66]),(CurvePreset::Fast,[15,16,36,66]),(CurvePreset::Digital,[29,16,29,66]),(CurvePreset::Stable,[51,50,73,66])]{
                    check(model(),Action::SetPreset{slot,side,preset},&std::array::from_fn::<_,4,_>(|i|(s+0x10a+side.index()*8+i,values[i])));
                }
            }
            check(model(),Action::MotionTarget{slot,value:MotionTarget::Mouse},&[(s+0xfc,0x1a)]);
            check(model(),Action::MotionMethod{slot,value:EnableMethod::Toggle},&[(s+0xfc,0x1c)]);
            check(model(),Action::MotionSensitivity{slot,value:2},&[(s+0xfe,2)]);
            check(model(),Action::MotionCompensation{slot,value:127},&[(s+0xff,127)]);
            for value in ENABLE_BUTTONS{let bytes=value.bits().to_le_bytes();check(model(),Action::MotionButton{slot,value},&std::array::from_fn::<_,4,_>(|i|(s+0x100+i,bytes[i])));}
            check(model(),Action::LightEffect{slot,value:LightEffect::FireRing},&[(s+0x84,3)]);
            check(model(),Action::LightSpeed{slot,value:7},&[(s+0x86,7)]);
            let color=Color::new(71,21,31,41).unwrap();let bytes=color.packed().to_le_bytes();
            let patch=(0..24).flat_map(|n|(0..4).map(move|i|(s+0x98+n*4+i,bytes[i]))).collect::<Vec<_>>();
            check(model(),Action::LightColor{slot,index:0,value:color},&patch);
            for(effect,base)in [(LightEffect::FireRing,0x88),(LightEffect::LightTracing,0x90)]{for index in 0..2{
                let mut m=model();m.profiles.as_mut().unwrap().slots[slot].lighting.set_effect(effect).unwrap();m.baseline=m.profiles.clone();
                check(m,Action::LightColor{slot,index,value:color},&std::array::from_fn::<_,4,_>(|i|(s+base+index*4+i,bytes[i])));
            }}
        }
        check(model(),Action::SetTournament(true),&[(3,0x10)]);
        check(model(),Action::SetReconnect(true),&[(3,0x08)]);
        for side in [Side::Left,Side::Right]{for axis in [Axis::X,Axis::Y]{let bit=1<<(side.index()*2+usize::from(axis==Axis::Y));feature(Action::SetInvert{slot:0,side,axis,value:true},bit,bit);}}
        feature(Action::SetSwap{slot:0,value:StickSwap::LeftStickDpad},0x30,0x10);
        feature(Action::SetSwap{slot:0,value:StickSwap::LeftStickRightStick},0x30,0x20);
        feature(Action::SetShape{slot:0,value:StickShape::Square},0x300000,0x100000);
        feature(Action::SwapTriggers{slot:0,value:true},0x40,0x40);
        feature(Action::SetSync{slot:0,value:true},0x80,0x80);
        for(i,motor)in [VibrationMotor::LeftGrip,VibrationMotor::RightGrip,VibrationMotor::LeftTrigger,VibrationMotor::RightTrigger].into_iter().enumerate(){let shift=8+i*3;feature(Action::SetVibration{slot:0,motor,value:4},7<<shift,4<<shift);}
    }
    #[test]
    fn slot_management_is_local_lossless_and_bounded(){
        let mut m=model();let before=m.profiles.as_ref().unwrap().encode();
        m.edit(Action::RenameProfile{slot:0,name:"🚀 Racing".into()}).unwrap();
        let named=m.profiles.as_ref().unwrap().encode();assert_eq!(&before[..8],&named[..8]);assert_eq!(&before[0x28..],&named[0x28..]);
        assert!(m.edit(Action::RenameProfile{slot:0,name:"🚀".repeat(9)}).is_err());
        assert!(m.edit(Action::SelectProfile(65536)).is_err());
        let source=m.profiles.as_ref().unwrap().slots[0].clone();
        m.edit(Action::CopyProfile{from:0,to:1}).unwrap();assert_eq!(m.profiles.as_ref().unwrap().slots[1],source);
        m.edit(Action::DeleteProfile(1)).unwrap();assert!(!m.profiles.as_ref().unwrap().slots[1].is_valid());
        m.edit(Action::ResetProfile(1)).unwrap();assert_eq!(m.profiles.as_ref().unwrap().slots[1],defaults().unwrap().slots[1]);
    }
    #[test]
    fn edits_preserve_unknown_flags_and_reject_invalid_domains(){
        let mut image=defaults().unwrap().encode();image[3]=0xe7;image[0x83]=0xc0;image[0x100]|=0xc8;image[0x101]=0xad;image[0x122]=0xab;image[0x123]=0xcd;
        let mut m=Model::default();m.load(Profiles::decode(&image).unwrap());
        m.edit(Action::SetReconnect(true)).unwrap();m.edit(Action::MotionTarget{slot:0,value:MotionTarget::Mouse}).unwrap();
        let bytes=m.profiles.as_ref().unwrap().encode();assert_eq!(bytes[3],0xef);assert_eq!(bytes[0x100],image[0x100]|2);
        for i in [0x83,0x101,0x122,0x123]{assert_eq!(bytes[i],image[i]);}
        for action in [Action::SetJitter{slot:0,side:Side::Left,value:6},Action::MotionSensitivity{slot:0,value:6},Action::SetVibration{slot:0,motor:VibrationMotor::LeftGrip,value:6},Action::LightSpeed{slot:0,value:0}]{
            let before=m.profiles.as_ref().unwrap().encode();assert!(m.edit(action).is_err());assert_eq!(m.profiles.as_ref().unwrap().encode(),before);
        }
    }
}
