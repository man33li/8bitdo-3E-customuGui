use super::{assets::{self,Artwork},clay::{self,Clay,Size,Style,Texture},coordinates::Coordinates,ffi as f,guide,state::*,widgets::*};
use crate::{Result,input::{self,Stick,StickMouse,TriggerButton},profile::*};
use std::{fs::OpenOptions,io::Write,path::PathBuf,time::Instant};
unsafe extern "C" { fn GetCharPressed()->i32; }

#[derive(Clone,Default)]
pub struct Options { pub bench:bool,pub screenshot:Option<PathBuf>,pub page:Option<String>,pub debug_pointer:bool,pub assets:Option<PathBuf> }
const PAGES:[&str;14]=["Home","Mapping","Sticks","Triggers","Vibration","Lighting","Motion","Profiles","Settings","Test","Remap","Host profiles","Guide","Gyro guide"];
const GYRO_GUIDE:usize=13;
struct App { page:usize,side:usize,rear:bool,color:usize,modal:Option<Action>,diagram:Texture,artwork:Artwork,asset_path:String,assets_cli:Option<PathBuf>,error:Option<String>,rename:String,rename_slot:usize,rename_original:[u8;32],remap_source:usize,remap_layer:usize,host_name:String,host_selected:String,rule_index:usize,rule_process:String,rule_profile:usize,rule_original:Option<crate::profiles::Rule> }
impl App {
    fn draw(&mut self,ui:&mut Ui<'_,'_>,model:&Model)->Option<Action>{
        let mut action=None;let old_page=self.page;let slot=model.active_slot.min(2);
        let profiles=model.profiles.as_ref();let profile=profiles.map(|p|&p.slots[slot]);
        ui.element("app",0,Style::row(Size::Grow,Size::Grow).background(BG),|ui|{
            if let Some(page)=ui.nav_sidebar(&PAGES,self.page,model.demo,profiles.is_some_and(|p|p.profile_bits.tournament())){self.page=page;}
            ui.element("workspace",0,Style::column(Size::Grow,Size::Grow).padding(24).gap(14),|ui|{
                ui.page_header(PAGES[self.page],if model.demo{"Demo — no device I/O"}else if model.device.as_ref().is_some_and(|d|d.link==Link::Bluetooth)&&!BLE_WRITE_VERIFIED{"Bluetooth — read-only; edits staged locally"}else if model.device.as_ref().is_some_and(|d|d.link==Link::Wired){"Wired — gamepad input pauses briefly during reads/saves"}else if model.device.is_some(){"Connected — edits staged until Save"}else{"Connect receiver, wired USB or Bluetooth"});
                if let Some(message)=self.error.as_deref().or(model.notice.as_deref()){if ui.toast(message){self.error=None;action=Some(Action::DismissNotice);}}
                if let Some(operation)=&model.operation{ui.progress("operation-progress",None,&operation.title);ui.text(&operation.detail,14,MUTED);}
                let enabled=ui.enabled;ui.enabled&=model.operation.is_none();
                ui.scroll("page-scroll",Style::column(Size::Grow,Size::Grow).gap(12),|ui|{
                    match self.page {
                        0=>{
                            self.diagram(ui);
                            if let Some(d)=&model.device{
                                ui.text(&d.name,24,TEXT);ui.text(format_args!("Link: {:?}   Logical PID: {:04x}",d.link,d.pid),16,MUTED);
                                if let Some(battery)=model.battery(){ui.text(format_args!("Battery: {} ({})",battery.level,battery.status),16,MUTED);}
                                ui.text("Firmware version: decoder not established for this link.",15,MUTED);
                                if let Some(record)=&d.version_record{ui.text(format_args!("Version record: {record}"),14,MUTED);}
                            }
                            else{ui.text("Connect an awake receiver or one wired 2dc8:202f device (including an Xbox-mode receiver), or use Bluetooth configuration. Wired reads/saves pause gamepad input briefly.",18,TEXT);}
                            if model.device.is_none()&&!model.demo{
                                ui.text("Bluetooth: hold the front pixel-logo/profile button for 3 seconds until configuration LEDs blink, then connect. Close the mobile app.",16,TEXT);
                                let enabled=ui.enabled;ui.enabled&=model.can_connect_bluetooth();
                                if ui.button("connect-bluetooth",0,"Connect Bluetooth",false){action=Some(Action::ConnectBluetooth);}
                                ui.enabled=enabled;
                            }
                            if let Some(p)=profiles{
                                if p.current_slot.get()<3{ui.text(format_args!("Active slot: {}",p.current_slot.get()+1),17,TEXT);}else{ui.text(format_args!("Active slot: none / raw 0x{:02x}",p.current_slot.get()),17,TEXT);}
                                self.header_settings(ui,p);
                            }
                            if let Some(link)=model.receiver_mode_link(){
                                let enabled=ui.enabled;
                                ui.enabled&=!model.mapper_running&&!model.mapper_pending&&!model.dirty();
                                let label=if link==Link::Receiver{"Switch receiver to Xbox mode"}else{"Switch receiver to PC (XInput) mode"};
                                if ui.button("receiver-mode",0,label,false){self.modal=Some(Action::SwitchReceiverMode(link));}
                                ui.enabled=enabled;
                                if model.dirty(){ui.text("Save or revert unsaved edits before switching receiver mode.",15,MUTED);}
                                if model.mapper_running||model.mapper_pending{ui.text("Stop the host input mapper before switching receiver mode.",15,MUTED);}
                            }
                            let enabled=ui.enabled;
                            ui.enabled&=!model.demo&&!model.mapper_pending&&(model.mapper_running||model.mapper_device().is_ok());
                            if let Some(value)=ui.toggle("host-mapper",0,"Host input mapper",model.mapper_running){action=Some(Action::SetMapper(value));}
                            ui.enabled=enabled;
                            if model.mapper_pending{ui.text("Host mapper starting / stopping…",15,MUTED);}
                            self.host_status(ui,model);
                            ui.text("Remap edits host-only controls. Starting grabs physical input and creates virtual gamepad / keyboard / mouse devices; stopping releases them.",14,MUTED);
                            if ui.button("refresh",0,"Re-read controller",false){action=Some(Action::Refresh);}
                        },
                        1=>if let Some(p)=profile{
                            self.diagram(ui);
                            if let Some(face)=ui.segmented("mapping-face",&["Front","Back"],usize::from(self.rear)){self.rear=face==1;}
                            ui.text(if self.rear{"Rear/edge inputs — P1/P2 retain numeric identity"}else{"Front inputs"},15,MUTED);
                            for source in PhysicalButton::ALL{
                                let rear=matches!(source,PhysicalButton::L4|PhysicalButton::R4|PhysicalButton::P1|PhysicalButton::P2|PhysicalButton::Lb|PhysicalButton::Rb|PhysicalButton::Lt|PhysicalButton::Rt);
                                if rear!=self.rear{continue;}let target=p.key_map.get(source);
                                ui.element("mapping-row",source.index() as u32,Style::row(Size::Grow,Size::Fit).gap(16),|ui|{
                                    let source_art=&assets::MAPPING[source.index()];let target_art=assets::destination(target);
                                    self.artwork.image(ui,source_art.normal,source.index()as u32*2,36.0,36.0);
                                    ui.element("mapping-label",source.index() as u32,Style::column(Size::Fixed(120.0),Size::Fixed(40.0)).padding(10),|ui|ui.text(source.label(),16,TEXT));
                                    if let Some(i)=ui.dropdown("mapping",source.index() as u32,&TARGET_LABELS,MappingTarget::ALL.iter().position(|v|*v==target).unwrap_or(usize::MAX)){action=Some(Action::SetMapping{slot,source,target:MappingTarget::ALL[i]});}
                                    self.artwork.image(ui,if source_art.normal==target_art.normal{target_art.default}else{target_art.change},source.index()as u32*2+1,36.0,36.0);
                                    if matches!(target,MappingTarget::Unknown(_)){ui.text(format_args!("raw {:08x}",target.raw()),13,MUTED);}
                                });
                            }
                        }else{unavailable(ui);},
                        2=>if let Some(p)=profile{
                            self.illustration(ui,assets::STICKS);
                            if let Some(side)=ui.segmented("stick-side",&["L Stick","R Stick"],self.side){self.side=side;}
                            let side=if self.side==0{Side::Left}else{Side::Right};let curve=&p.dead_zone.curves[self.side];
                            ui.element("stick-plots",0,Style::row(Size::Grow,Size::Fit).gap(24),|ui|{
                                ui.curve(curve);let (x,y)=model.pad.map(|pad|if self.side==0{(pad.left_x,pad.left_y)}else{(pad.right_x,pad.right_y)}).unwrap_or((0,0));ui.plot("stick-live",0,f32::from(x)/32768.0,f32::from(y)/32768.0);
                            });
                            ui.text("Stick Curve — input / output (%)",16,TEXT);
                            if let Some(i)=ui.dropdown("curve-preset",0,&["Custom","Default","Dynamic","Precision","Fast","Digital","Stable"],curve_preset_index(curve)){if i>0{action=Some(Action::SetPreset{slot,side,preset:[CurvePreset::Default,CurvePreset::Dynamic,CurvePreset::Precision,CurvePreset::Fast,CurvePreset::Digital,CurvePreset::Stable][i-1]});}}
                            for(point,labels)in [(0,["Deadzone — inner","Anti-Dead Zone — inner"]),(3,["Deadzone — outer","Anti-Dead Zone — outer"]),(1,["P1 input","P1 output"]),(2,["P2 input","P2 output"])]{
                                ui.element("curve-controls",point as u32,Style::row(Size::Grow,Size::Fit).gap(12),|ui|{
                                    for(axis,label,value)in [(Axis::X,labels[0],curve.points[point].input.get()),(Axis::Y,labels[1],curve.points[point].output.get())]{
                                        if let Some(value)=ui.range("curve-value",(point*2+usize::from(axis==Axis::Y))as u32,label,value,0,100,true,0){action=Some(Action::SetCurve{slot,side,point,axis,value});}
                                    }
                                });
                            }
                            if let Some(value)=ui.range("jitter",0,"Stick Sensitivity / Anti-Jitter",p.dead_zone.jitter[self.side].get(),0,5,false,0){action=Some(Action::SetJitter{slot,side,value});}
                            for(axis,label)in [(Axis::X,"Invert X"),(Axis::Y,"Invert Y")]{if let Some(value)=ui.toggle("invert",usize::from(axis==Axis::Y)as u32,label,p.feature.inverted(side,axis)){action=Some(Action::SetInvert{slot,side,axis,value});}}
                            ui.text("Stick Shape Settings",16,TEXT);
                            if let Some(i)=ui.dropdown("shape",0,&["Circular","Square","Default"],match p.feature.shape().raw(){0=>0,2=>2,_=>1}){action=Some(Action::SetShape{slot,value:[StickShape::Circular,StickShape::Square,StickShape::Default][i]});}
                            for(i,label,value,kind)in [(0,"Swap LS/RS",p.feature.swaps_sticks(),StickSwap::LeftStickRightStick),(1,"Swap LS/D-pad",p.feature.swaps_left_dpad(),StickSwap::LeftStickDpad)]{if let Some(value)=ui.toggle("stick-swap",i,label,value){action=Some(Action::SetSwap{slot,value:if value{kind}else{StickSwap::None}});}}
                            ui.text("A lower deadzone improves precision but may prevent returning to center. Interior points must stay strictly between neighbors.",14,MUTED);
                        }else{unavailable(ui);},
                        3=>if let Some(p)=profile{
                            self.illustration(ui,assets::TRIGGERS);
                            for(side,label)in [(Side::Left,"Left Trigger"),(Side::Right,"Right Trigger")]{
                                ui.text(label,20,TEXT);let live=model.pad.map(|pad|if side==Side::Left{pad.left_trigger}else{pad.right_trigger});ui.progress(if side==Side::Left{"lt-live"}else{"rt-live"},live.map(|v|f32::from(v)/255.0),"Live input");
                                for(upper,name,value)in [(false,"Lower threshold",p.dead_zone.trigger_lower[side.index()].get()),(true,"Upper threshold",p.dead_zone.trigger_upper[side.index()].get())]{
                                    if let Some(value)=ui.range("trigger-value",(side.index()*2+usize::from(upper))as u32,name,value,0,255,true,0){action=Some(Action::SetTrigger{slot,side,upper,value});}
                                }
                            }
                            if let Some(value)=ui.toggle("swap-triggers",0,"Swap Triggers",p.feature.swap_triggers()){action=Some(Action::SwapTriggers{slot,value});}
                        }else{unavailable(ui);},
                        4=>{
                            self.illustration(ui,assets::VIBRATION);
                            for(motor,label)in [(VibrationMotor::LeftGrip,"Grip Vibration - Left"),(VibrationMotor::RightGrip,"Grip Vibration - Right"),(VibrationMotor::LeftTrigger,"Trigger Vibration - Left"),(VibrationMotor::RightTrigger,"Trigger Vibration - Right")]{
                                if profile.is_none()&&motor.index()>1{continue;}
                                let value=profile.map(|p|p.feature.vibration(motor).get()).unwrap_or(model.test_levels[motor.index()]);
                                ui.element("motor-row",motor.index()as u32,Style::row(Size::Grow,Size::Fit).gap(12),|ui|{
                                    if let Some(value)=ui.range("vibration",motor.index()as u32,label,value,0,5,false,0){
                                        action=Some(if profile.is_some(){Action::SetVibration{slot,motor,value}}else{Action::SetTestLevel{motor,value}});
                                    }
                                });
                            }
                            if let Some(p)=profile{if let Some(value)=ui.toggle("rumble-sync",0,"Impulse Trigger Sync with Rumble",p.feature.impulse_sync()){action=Some(Action::SetSync{slot,value});}}
                            if model.can_test_rumble(){
                                let enabled=ui.enabled;ui.enabled&=!model.rumble_running;
                                if ui.button("test-rumble",0,if model.rumble_running{"Testing…"}else{"Test"},false){action=Some(Action::TestRumble);}
                                ui.enabled=enabled;
                            }
                            ui.text("Kernel FF_RUMBLE test: left grip = strong, right grip = weak, 600 ms. Trigger motors cannot be addressed independently by this API. Test does not save controller settings.",14,MUTED);
                            if profile.is_none(){ui.text("No controller profiles loaded: these grip levels apply only to the host vibration test.",14,MUTED);}
                        },
                        5=>if let Some(p)=profile{
                            self.illustration(ui,assets::FRONT);
                            let light=&p.lighting;ui.text("Effect",18,TEXT);
                            if let Some(i)=ui.dropdown("light-effect",0,&["Close","Light-tracing","Fire Ring","Fixed"],light.effect.raw().checked_sub(1).map(usize::from).unwrap_or(usize::MAX)){action=Some(Action::LightEffect{slot,value:LightEffect::ALL[i]});}
                            if matches!(light.effect,LightEffect::LightTracing|LightEffect::FireRing){
                                if let Some(value)=ui.range("light-speed",0,"Speed",light.speed.get()as u8,1,10,false,0){action=Some(Action::LightSpeed{slot,value});}
                                if let Some(i)=ui.segmented("light-color-index",&["Background color","Foreground Color"],self.color){self.color=i;}
                            }
                            let color=match light.effect{LightEffect::Fixed=>Some(light.fixed[0]),LightEffect::FireRing=>Some(light.fire_ring[self.color]),LightEffect::LightTracing=>Some(light.light_tracing[self.color]),_=>None};
                            if let Some(color)=color{
                                ui.element("color-swatch",0,Style::row(Size::Grow,Size::Fixed(38.0)).rounded(6.0).background(clay::rgb(color.red.get(),color.green.get(),color.blue.get())),|_|{});
                                let(hue,saturation)=color_axes(color);
                                for(i,label,current,percent)in [(0,"Hue",hue,false),(1,"Saturation",saturation,true),(2,"Brightness",color.brightness.get(),true)]{
                                    if let Some(value)=ui.range("light-color",i,label,current,0,255,percent,0){
                                        let next=if i==2{Color::new(u16::from(value),u16::from(color.red.get()),u16::from(color.green.get()),u16::from(color.blue.get())).expect("byte values")}else{hue_color(if i==0{value}else{hue},if i==1{value}else{saturation},color.brightness.get())};
                                        action=Some(Action::LightColor{slot,index:self.color,value:next});
                                    }
                                }
                                ui.text("Saturation is Windows' max(R,G,B) amplitude, not HSV saturation. Fixed edits fill all 24 stored colors.",14,MUTED);
                            }
                        }else{unavailable(ui);},
                        6=>if let Some(p)=profile{
                            let motion=&p.motion;
                            ui.text(format_args!("Editing slot {}: {}",slot+1,SlotLabel(p)),16,MUTED);
                            if !profiles.unwrap().profile_bits.tournament(){ui.text("Motion settings are shown only in PC (XInput) mode, as in Windows. The controller is in Xbox mode (profile bit 0x10 clear, verified live); imported motion settings are preserved.",16,MUTED);}else{
                            ui.text("Map motion sensing to:",18,TEXT);
                            if let Some(i)=ui.dropdown("motion-map",0,&["Left Stick (recommended for racing games)","Right Stick (recommended for shooter games)","Mouse"],motion.feature.target().raw()as usize){action=Some(Action::MotionTarget{slot,value:MotionTarget::ALL[i]});}
                            ui.text("Enable method",18,TEXT);
                            if let Some(i)=ui.dropdown("motion-method",0,&["Press and hold to enable","Press once to enable/disable"],usize::from(motion.feature.enable_method()==EnableMethod::Toggle)){action=Some(Action::MotionMethod{slot,value:if i==0{EnableMethod::Hold}else{EnableMethod::Toggle}});}
                            ui.text("Enable button",18,TEXT);
                            if let Some(i)=ui.dropdown("motion-button",0,&ENABLE_LABELS,ENABLE_BUTTONS.iter().position(|v|*v==motion.enable_button).unwrap_or(usize::MAX)){action=Some(Action::MotionButton{slot,value:ENABLE_BUTTONS[i]});}
                            if let Some(value)=ui.range("motion-sensitivity",0,"Sensivity",motion.sensitivity.get(),0,5,false,1){action=Some(Action::MotionSensitivity{slot,value});}
                            if let Some(value)=ui.range("motion-compensation",0,"Deadzone Compensation",motion.compensation.get(),0,255,true,0){action=Some(Action::MotionCompensation{slot,value});}
                            ui.text("Imported axis/mode and reserved bits are preserved; modern 3E has no controls for them.",14,MUTED);
                            }
                        }else{unavailable(ui);},
                        7=>if let Some(p)=profiles{
                            for(i,s)in p.slots.iter().enumerate(){
                                if ui.list_row(i as u32,SlotLabel(s),format_args!("Slot {}{}{}",i+1,if model.slot_dirty(i){" / unsaved"}else{""},if p.current_slot.get()==i as u8{" / active"}else{""}),i==slot){action=Some(Action::SelectProfile(i));}
                            }
                            if self.rename_slot!=slot||old_page!=7||self.rename_original!=*p.slots[slot].name.raw(){self.rename=p.slots[slot].name.display();self.rename_slot=slot;self.rename_original=*p.slots[slot].name.raw();}
                            ui.text("Name (up to 16 UTF-16 code units)",16,TEXT);ui.text_input("profile-name",&mut self.rename);
                            if ui.button("rename",0,"Rename",false){action=Some(Action::RenameProfile{slot,name:self.rename.clone()});}
                            ui.text("Copy selected profile to",16,TEXT);
                            for to in 0..3{if to!=slot&&ui.button("copy-profile",to as u32,format_args!("Slot {}",to+1),false){self.modal=Some(Action::CopyProfile{from:slot,to});}}
                            if ui.button("reset-profile",0,"Reset selected slot to structural defaults",false){self.modal=Some(Action::ResetProfile(slot));}
                            if ui.button("delete-profile",0,"Delete selected slot",false){self.modal=Some(Action::DeleteProfile(slot));}
                            ui.text("Slot selection, rename, copy, delete and reset are staged. Save transfers all slots. Reset does not erase pairing.",14,MUTED);
                        }else{unavailable(ui);},
                        8=>{
                            ui.text("Original artwork",21,TEXT);
                            if let Some(root)=&self.artwork.root{ui.text(format_args!("Resolved: {}",root.display()),15,TEXT);}else{ui.text("Resolved: none — using generic artwork",15,MUTED);}
                            ui.text("Assets folder (contains Xbox3E/ and XboxWired/Test/); blank resets automatic discovery.",14,MUTED);
                            ui.text_input_limit("assets-root",&mut self.asset_path,4096);
                            if ui.button("assets-save",0,"Save artwork folder",false){
                                match self.artwork.save_root(&self.asset_path,self.assets_cli.as_deref()){
                                    Ok(())=>self.error=Some("Artwork setting saved. --assets and ULTIMATE_ASSETS take precedence over this setting.".into()),
                                    Err(error)=>self.error=Some(error.to_string()),
                                }
                            }
                            ui.text("8BitDo copyright: use artwork from your own installed copy. Not bundled or redistributed.",14,MUTED);
                            if let Some(p)=profiles{self.header_settings(ui,p);}
                            if profiles.is_some()&&ui.button("restore-factory",0,"Restore factory profiles",false){self.modal=Some(Action::RestoreFactoryProfiles{backup:false});}
                            ui.text("Calibration / Firmware",21,TEXT);
                            ui.text("Not exposed: receiver calibration transport and firmware boot/flash flow are not proven. No guessed maintenance commands.",16,MUTED);
                            ui.text("Auto Reconnect: dock placement powers off/charges; removal powers on/reconnects (Windows description, not hardware-verified).",15,MUTED);
                            ui.text("PC (XInput) mode / Tournament: one flag (bit 0x10). Windows labels it both XInput Mode and Tournament (1000 Hz); live, its command switches the receiver between PC (2dc8:3024) and Xbox (2dc8:202f).",15,MUTED);
                            ui.text("Native Clay + raylib; Inter 4.1 / SIL OFL 1.1.",13,MUTED);
                        },
                        10=>self.host_remap(ui,model,&mut action),
                        11=>self.host_profiles(ui,model,&mut action),
                        12=>guide::draw(ui,guide::APP),
                        GYRO_GUIDE=>guide::draw(ui,guide::GYRO),
                        _=>self.input_test(ui,model),
                    }
                });
                if profiles.is_some()&&self.page<12{ui.element("save-bar",0,Style::row(Size::Grow,Size::Fit).gap(12),|ui|{
                    ui.text(format_args!("Editing slot {} / {}",slot+1,if model.dirty(){"unsaved changes"}else{"unchanged"}),15,MUTED);
                    let enabled=ui.enabled;ui.enabled&=model.can_save()&&model.dirty();
                    if model.can_save()&&ui.button("save",0,"Save + verify",true){self.modal=Some(Action::Save);}
                    ui.enabled=enabled&&(model.demo||model.device.is_some());
                    if ui.button("revert",0,"Revert / re-read",false){self.modal=Some(Action::Revert);}ui.enabled=enabled;
                });}
                if model.device.as_ref().is_some_and(|d|d.link==Link::Bluetooth)&&!BLE_WRITE_VERIFIED{
                    ui.text("Bluetooth saves are disabled pending hardware verification. All profile edits remain local; use the verified 2.4 GHz receiver to save.",15,MUTED);
                }
                ui.enabled=enabled;
            });
        });
        if self.page!=old_page{ui.focus.reset();}
        if let Some(pending)=self.modal.clone(){
            ui.enabled=true;ui.focus.begin(Input::default());
            if !["modal-cancel","modal-confirm","modal-backup"].iter().any(|name|ui.focus.selected==clay::id(name,0).id){ui.focus.reset();}
            let(title,message)=match pending{
                Action::Save=>("Write profiles to controller?","Persistently replaces all three device slots and header settings. Success requires an identical 868-byte read-back."),
                Action::Revert=>("Discard unsaved changes?","Re-read all profiles from the controller; local staged changes will be lost."),
                Action::SwitchReceiverMode(Link::Receiver)=>("Switch receiver to Xbox mode?","This persistent change is for the 2.4 GHz receiver. It re-enumerates from PC (XInput, 2dc8:3024) to Xbox (GIP, 2dc8:202f), and input pauses briefly. The Motion page is hidden in Xbox mode. The app reconnects automatically. Save or revert edits and stop the host mapper first."),
                Action::SwitchReceiverMode(_)=>("Switch receiver to PC (XInput) mode?","This persistent change is intended for the 2.4 GHz receiver in Xbox mode, not a directly cabled controller. It re-enumerates from Xbox (GIP, 2dc8:202f) to PC (XInput, 2dc8:3024), and input pauses briefly. The Motion page is hidden in Xbox mode and returns in PC mode. The app reconnects automatically. Save or revert edits and stop the host mapper first."),
                Action::CopyProfile{..}=>("Replace destination profile?","Replaces the destination slot locally. Save is required to write the controller."),
                Action::DeleteProfile(_)=>("Delete profile?","Marks this slot empty locally. Save is required to write the controller."),
                Action::DeleteHostProfile=>("Delete named host profile?","Permanently removes this host JSON file. Controller slots and host.json are unchanged. The default profile cannot be deleted; remove any rules referencing this profile first."),
                Action::RestoreFactoryProfiles{..}=>("Restore factory profiles?","Overwrites all 3 slots with Windows factory defaults: names and slot markers cleared, motion enable button None. Current polling/reconnect flags are preserved. Save + verify is required to persist them; no pairing-erase command is sent. Save backup first saves the last controller read to ~/ultimate-backup-<timestamp>.bin, then stages defaults. Any unsaved edits are discarded."),
                _=>("Reset profile to defaults?","Replaces this slot with structural defaults locally, not a full device reset."),
            };
            let backup=matches!(pending,Action::RestoreFactoryProfiles{..})&&!model.demo;
            if let Some(choice)=ui.modal(title,message,backup){
                if !matches!(choice,ModalChoice::Cancel){
                    if matches!(pending,Action::Save){eprintln!("GUI Save confirmed: page_index={} page={} editor_slot_index={} (slot {})",self.page,PAGES[self.page],slot,slot+1);}
                    self.rename_slot=usize::MAX;
                    action=Some(if matches!(choice,ModalChoice::Backup){Action::RestoreFactoryProfiles{backup:true}}else{pending});
                }
                self.modal=None;ui.focus.reset();
            }
        }action
    }
    fn host_profiles(&mut self,ui:&mut Ui<'_,'_>,model:&Model,action:&mut Option<Action>){
        ui.text("Named host profiles are separate from the controller's three device slots.",15,MUTED);
        if let Some(enabled)=ui.toggle("auto-profiles",0,"Auto-switch while host mapper runs",model.auto_enabled){*action=Some(Action::SetAutoEnabled(enabled));}
        ui.text("First matching rule wins. Only your processes count; comm / executable names match exactly, command lines match substrings, ignoring case. Changes must remain stable for 2 seconds.",14,MUTED);
        let labels:Vec<&str>=model.host_profile_names.iter().map(String::as_str).collect();
        if !labels.is_empty(){
            ui.text("Default (used when no process matches)",16,TEXT);
            let selected=labels.iter().position(|name|*name==model.auto_config.default).unwrap_or(0);
            if let Some(index)=ui.dropdown("auto-default",0,&labels,selected){let mut config=model.auto_config.clone();config.default=labels[index].into();*action=Some(Action::SetAutoConfig(config));}
        }
        let enabled=ui.enabled;ui.enabled&=!model.mapper_running&&!model.mapper_pending;
        ui.text("Select a profile to edit on Remap (save before switching)",18,TEXT);
        for(index,name)in model.host_profile_names.iter().enumerate(){
            if ui.list_row(index as u32,name,if *name==model.auto_config.default{"default"}else{"host-only"},*name==model.host_profile_name){*action=Some(Action::SelectHostProfile(name.clone()));}
        }
        if self.host_selected!=model.host_profile_name{self.host_selected=model.host_profile_name.clone();self.host_name=model.host_profile_name.clone();}
        ui.text("Profile name (letters, numbers, spaces, '-' or '_')",15,TEXT);ui.text_input("host-profile-name",&mut self.host_name);
        if ui.button("host-profile-new",0,"New from current",false){*action=Some(Action::NewHostProfile(self.host_name.clone()));}
        if ui.button("host-profile-rename",0,"Rename selected",false){*action=Some(Action::RenameHostProfile(self.host_name.clone()));}
        let editable=ui.enabled;ui.enabled&=model.host_profile_name!=model.auto_config.default;
        if ui.button("host-profile-delete",0,"Delete selected",false){self.modal=Some(Action::DeleteHostProfile);}
        ui.enabled=editable;
        if ui.button("host-profile-save",0,"Save current host profile",true){*action=Some(Action::SaveHostProfile);}
        ui.enabled=enabled;
        ui.text("Process rules (ordered)",20,TEXT);
        for(index,rule)in model.auto_config.rules.iter().enumerate(){
            if ui.list_row(1000+index as u32,&rule.process,&rule.profile,index==self.rule_index){self.rule_index=index;self.rule_original=None;}
        }
        let selected=model.auto_config.rules.get(self.rule_index);
        if selected!=self.rule_original.as_ref(){
            self.rule_original=selected.cloned();
            if let Some(rule)=selected{self.rule_process=rule.process.clone();self.rule_profile=labels.iter().position(|name|*name==rule.profile).unwrap_or(0);}
        }
        ui.text("Process executable / command-line substring",15,TEXT);ui.text_input("rule-process",&mut self.rule_process);
        if !labels.is_empty(){
            self.rule_profile=self.rule_profile.min(labels.len()-1);
            if let Some(index)=ui.dropdown("rule-profile",0,&labels,self.rule_profile){self.rule_profile=index;}
            let rule=crate::profiles::Rule{profile:labels[self.rule_profile].into(),process:self.rule_process.clone()};
            if ui.button("rule-add",0,"Add rule",false){let mut config=model.auto_config.clone();config.rules.push(rule.clone());self.rule_index=config.rules.len()-1;*action=Some(Action::SetAutoConfig(config));}
            if selected.is_some(){
                if ui.button("rule-update",0,"Update selected rule",false){let mut config=model.auto_config.clone();config.rules[self.rule_index]=rule;*action=Some(Action::SetAutoConfig(config));}
                if ui.button("rule-delete",0,"Remove selected rule",false){let mut config=model.auto_config.clone();config.rules.remove(self.rule_index);*action=Some(Action::SetAutoConfig(config));}
                for(up,label)in [(true,"Move rule up"),(false,"Move rule down")]{
                    let destination=if up{self.rule_index.checked_sub(1)}else{self.rule_index.checked_add(1).filter(|index|*index<model.auto_config.rules.len())};
                    if let Some(destination)=destination{if ui.button("rule-move",u32::from(!up),label,false){let mut config=model.auto_config.clone();config.rules.swap(self.rule_index,destination);self.rule_index=destination;*action=Some(Action::SetAutoConfig(config));}}
                }
            }
        }
    }
    fn host_status(&self,ui:&mut Ui<'_,'_>,model:&Model){
        if let Ok(device)=model.mapper_device(){ui.text(format_args!("Host input: {:?} / {} / {}",device.link,device.name,device.path.display()),14,MUTED);}
        match model.host_notice(){Ok(Some(notice))=>ui.text(notice,14,MUTED),Err(error)=>ui.text(error,14,MUTED),_=>{}}
    }
    fn host_remap(&mut self,ui:&mut Ui<'_,'_>,model:&Model,action:&mut Option<Action>){
        self.host_status(ui,model);
        ui.text(format_args!("Host profile: {}",model.host_profile_name),18,TEXT);
        ui.text("Host-only profile; independent of the controller's three stored slots. Stop the mapper before editing, then Save host profile. Start from Home.",15,MUTED);
        if ui.button("open-gyro-guide",0,"Gyro guide: what every setting does",false){self.page=GYRO_GUIDE;}
        let enabled=ui.enabled;ui.enabled&=!model.mapper_running&&!model.mapper_pending;
        let mut profile=std::borrow::Cow::Borrowed(&model.host_profile);let mut changed=false;
        if cfg!(target_os="linux"){
            if let Some(value)=ui.toggle("host-steam",0,"Steam Input (virtual DualSense Edge)",profile.steam){profile.to_mut().steam=value;changed=true;}
            if profile.steam{ui.text("Steam does remapping and gyro: host remaps, layers and host gyro output below are ignored; the controller slot's key map still applies. L4/R4 → Edge Fn buttons, PL/PR → back paddles. 2.4 GHz gyro needs \"2.4 GHz gyro clutch\" on (Motion target Mouse, level 6). Needs /dev/uhid access.",14,MUTED);}
        }
        let sources:[&str;input::SOURCE_KEYS.len()]=std::array::from_fn(|index|input::SOURCE_KEYS[index].0);
        self.remap_layer=self.remap_layer.min(profile.layers.len());
        let names:Vec<&str>=std::iter::once("Base profile").chain(profile.layers.iter().map(|layer|layer.name.as_str())).collect();
        if let Some(index)=ui.dropdown("host-layer",0,&names,self.remap_layer){self.remap_layer=index;}
        let clutch=match profile.button.as_str(){"R4"=>"BTN_TRIGGER_HAPPY2","L4"=>"BTN_TRIGGER_HAPPY1","PL"=>"BTN_Z","PR"=>"BTN_C",button=>button};
        let available=sources.iter().copied().find(|source|*source!=clutch&&!profile.layers.iter().any(|layer|layer.hold==*source));
        let controls_enabled=ui.enabled;ui.enabled&=available.is_some();
        if ui.button("host-new-layer",0,"Create layer",false){
            let name=format!("Layer {}",profile.layers.len()+1);
            profile.to_mut().layers.push(input::Layer{name,hold:available.unwrap().into(),..input::Layer::default()});
            self.remap_layer=profile.layers.len();changed=true;
        }
        ui.enabled=controls_enabled;
        if self.remap_layer>0{
            if ui.button("host-delete-layer",0,"Delete layer",false){profile.to_mut().layers.remove(self.remap_layer-1);self.remap_layer=0;changed=true;}
            else{
                let layer=&profile.layers[self.remap_layer-1];
                let mut name=layer.name.clone();let previous=name.clone();
                ui.text_input_limit("host-layer-name",&mut name,48);
                if name!=previous{profile.to_mut().layers[self.remap_layer-1].name=name;changed=true;}
                let hold=sources.iter().position(|source|*source==profile.layers[self.remap_layer-1].hold).unwrap_or(usize::MAX);
                ui.text("Hold button (consumed, never emitted)",15,MUTED);
                if let Some(index)=ui.dropdown("host-layer-hold",0,&sources,hold){profile.to_mut().layers[self.remap_layer-1].hold=sources[index].into();changed=true;}
                ui.text("Nonempty layer fields replace base fields; empty fields inherit. First held layer wins. Latches clear on exit.",14,MUTED);
            }
        }
        let selected=self.remap_layer;
        // Keep the existing widgets shared without cloning an editable profile every frame.
        macro_rules! controls {($field:ident)=>{if selected==0{&profile.$field}else{&profile.layers[selected-1].$field}};}
        macro_rules! controls_mut {($field:ident)=>{if selected==0{&mut profile.to_mut().$field}else{&mut profile.to_mut().layers[selected-1].$field}};}
        if let Some(index)=ui.dropdown("host-source",0,&sources,self.remap_source){self.remap_source=index;}
        let source=sources[self.remap_source];
        let mapping_enabled=ui.enabled;
        if selected>0&&profile.layers[selected-1].hold==source{ui.text("This source is the layer hold; choose another source to map.",15,MUTED);ui.enabled=false;}
        ui.text("Destination: Linux key name (BTN_SOUTH, KEY_SPACE, BTN_LEFT…). Unmapped sources pass through.",15,MUTED);
        let mut destination=controls!(mappings).get(source).cloned().unwrap_or_else(||source.to_owned());let previous=destination.clone();
        ui.text_input_limit("host-destination",&mut destination,32);
        if destination!=previous{controls_mut!(mappings).insert(source.into(),destination);changed=true;}
        if controls!(mappings).contains_key(source)&&ui.button("host-unmap",0,"Restore passthrough / inherit",false){controls_mut!(mappings).remove(source);changed=true;}
        let turbo=controls!(turbo).get(source).copied();
        if let Some(value)=ui.toggle("host-turbo",0,"Turbo",turbo.is_some()){if value{controls_mut!(turbo).insert(source.into(),10.0);}else{controls_mut!(turbo).remove(source);}changed=true;}
        if let Some(rate)=turbo{
            if let Some(value)=ui.range("host-turbo-rate",0,"Turbo cycles / second",rate.round().clamp(1.0,100.0)as u8,1,100,false,0){controls_mut!(turbo).insert(source.into(),f64::from(value));changed=true;}
        }
        let toggle=controls!(toggle).iter().any(|name|name==source);
        if let Some(value)=ui.toggle("host-toggle",0,"Toggle / latch",toggle){controls_mut!(toggle).retain(|name|name!=source);if value{controls_mut!(toggle).push(source.into());}changed=true;}
        ui.enabled=mapping_enabled;
        ui.text("Stick to mouse",20,TEXT);
        if let Some(value)=ui.toggle("host-stick-mouse",0,"Enable stick mouse",controls!(stick_mouse).is_some()){
            *controls_mut!(stick_mouse)=if value{Some(StickMouse{stick:Stick::Right,sensitivity:1000.0,deadzone:0.15})}else{None};changed=true;
        }
        if let Some(mut mouse)=*controls!(stick_mouse){
            let mut mouse_changed=false;
            if let Some(value)=ui.segmented("host-mouse-stick",&["Left stick","Right stick"],usize::from(mouse.stick==Stick::Right)){mouse.stick=if value==0{Stick::Left}else{Stick::Right};mouse_changed=true;}
            if let Some(value)=ui.range("host-mouse-speed",0,"Mouse speed (hundreds of pixels / second)",(mouse.sensitivity/100.0).round().clamp(1.0,100.0)as u8,1,100,false,0){mouse.sensitivity=f64::from(value)*100.0;mouse_changed=true;}
            if let Some(value)=ui.range("host-mouse-deadzone",0,"Mouse deadzone (%)",(mouse.deadzone*100.0).round().clamp(0.0,99.0)as u8,0,99,false,0){mouse.deadzone=f64::from(value)/100.0;mouse_changed=true;}
            if mouse_changed{*controls_mut!(stick_mouse)=Some(mouse);changed=true;}
        }
        ui.text("Trigger to button",20,TEXT);
        for (index,label) in [(0,"Left trigger"),(1,"Right trigger")]{
            let mut trigger=if index==0{controls!(trigger_buttons).left.clone()}else{controls!(trigger_buttons).right.clone()};
            let mut trigger_changed=false;
            if let Some(value)=ui.toggle("host-trigger",index,label,trigger.is_some()){trigger=if value{Some(TriggerButton{destination:"BTN_SOUTH".into(),threshold:0.5})}else{None};trigger_changed=true;}
            if let Some(trigger)=&mut trigger{
                let previous=trigger.destination.clone();
                ui.text_input_limit(if index==0{"host-left-trigger-key"}else{"host-right-trigger-key"},&mut trigger.destination,32);
                trigger_changed|=previous!=trigger.destination;
                if let Some(value)=ui.range("host-trigger-threshold",index,"Trigger threshold (%)",(trigger.threshold*100.0).round().clamp(1.0,100.0)as u8,1,100,false,0){trigger.threshold=f64::from(value)/100.0;trigger_changed=true;}
            }
            if trigger_changed{if index==0{controls_mut!(trigger_buttons).left=trigger;}else{controls_mut!(trigger_buttons).right=trigger;}changed=true;}
        }
        let receiver=match model.device.as_ref(){Some(device)=>device.link==Link::Receiver,None=>model.mapper_device().is_ok_and(|device|device.link==input::LinkKind::Receiver)};
        ui.text(if receiver{"Receiver gyro (all layers)"}else{"Bluetooth gyro (all layers)"},20,TEXT);
        let receiver_buttons:[&str;input::RECEIVER_CLUTCH_KEYS.len()]=std::array::from_fn(|index|input::RECEIVER_CLUTCH_KEYS[index].0);
        if receiver{
            if let Some(value)=ui.toggle("host-receiver-gyro",0,"2.4 GHz gyro clutch",profile.receiver_gyro){
                let profile=profile.to_mut();profile.receiver_gyro=value;
                if value&&!receiver_buttons.contains(&profile.button.as_str()){profile.button="R4".into();}
                changed=true;
            }
            ui.text("Motion target must be Mouse: enable it once with press-toggle, or hold its enable button. The consumed clutch suspends firmware mouse input while held.",14,MUTED);
            ui.text("Physical L4/R4/PL/PR work directly, without firmware remapping. With the mapper stopped, enabled firmware motion moves the desktop cursor.",14,MUTED);
        }else{
            ui.text("Holding the clutch disables Bluetooth gyro. Wired has no raw IMU or host gyro clutch.",14,MUTED);
        }
        let gyro_enabled=ui.enabled;ui.enabled&=!receiver||profile.receiver_gyro;
        if let Some(value)=ui.segmented("host-gyro-output",&["Right stick","Mouse"],usize::from(profile.gyro_mouse.is_some())){
            profile.to_mut().gyro_mouse=if value==1{Some(input::GyroMouse{sensitivity:1.0,deadzone:profile.deadzone,invert_x:profile.invert_x,invert_y:profile.invert_y})}else{None};changed=true;
        }
        let buttons:&[&str]=if receiver{&receiver_buttons}else{&["R4","L4","PL","PR"]};
        if let Some(index)=ui.dropdown("host-gyro-clutch",0,buttons,buttons.iter().position(|button|*button==profile.button).unwrap_or(usize::MAX)){profile.to_mut().button=buttons[index].into();changed=true;}
        if let Some(mut mouse)=profile.gyro_mouse{
            let mut mouse_changed=false;
            if let Some(value)=ui.range("host-gyro-mouse-speed",0,"Mouse gain (0.1 px/degree)",(mouse.sensitivity*10.0).round().clamp(1.0,100.0)as u8,1,100,false,0){mouse.sensitivity=f64::from(value)/10.0;mouse_changed=true;}
            if let Some(value)=ui.range("host-gyro-mouse-deadzone",0,"Tightening (0.1 deg/s)",(mouse.deadzone*10.0).round().clamp(0.0,100.0)as u8,0,100,false,0){mouse.deadzone=f64::from(value)/10.0;mouse_changed=true;}
            if let Some(value)=ui.toggle("host-gyro-invert-x",0,"Invert gyro X",mouse.invert_x){mouse.invert_x=value;mouse_changed=true;}
            if let Some(value)=ui.toggle("host-gyro-invert-y",0,"Invert gyro Y",mouse.invert_y){mouse.invert_y=value;mouse_changed=true;}
            if mouse_changed{profile.to_mut().gyro_mouse=Some(mouse);changed=true;}
        }else{
            ui.text("Velocity → game stick. Match anti-deadzone to the game's inner deadzone; mouse output avoids game stick curves and turn-speed limits.",14,MUTED);
            if let Some(value)=ui.range("host-gyro-stick-speed",0,"Stick gain (0.1x)",(profile.sensitivity*10.0).round().clamp(1.0,100.0)as u8,1,100,false,0){profile.to_mut().sensitivity=f64::from(value)/10.0;changed=true;}
            if let Some(value)=ui.range("host-gyro-stick-tightening",0,"Tightening (0.1 deg/s)",(profile.deadzone*10.0).round().clamp(0.0,100.0)as u8,0,100,false,0){profile.to_mut().deadzone=f64::from(value)/10.0;changed=true;}
            if let Some(value)=ui.toggle("host-gyro-invert-x",0,"Invert gyro X",profile.invert_x){profile.to_mut().invert_x=value;changed=true;}
            if let Some(value)=ui.toggle("host-gyro-invert-y",0,"Invert gyro Y",profile.invert_y){profile.to_mut().invert_y=value;changed=true;}
            let mut gyro=profile.gyro;
            let mut edited=false;
            if let Some(value)=ui.range("host-gyro-anti-deadzone",0,"Anti-deadzone (%)",(gyro.stick_deadzone*100.0).round()as u8,0,99,false,0){gyro.stick_deadzone=f64::from(value)/100.0;edited=true;}
            if let Some(value)=ui.range("host-gyro-saturation",0,"Saturation (10 deg/s)",(gyro.stick_saturation/10.0).round().clamp(1.0,200.0)as u8,1,200,false,0){gyro.stick_saturation=f64::from(value)*10.0;edited=true;}
            if let Some(value)=ui.range("host-gyro-power",0,"Stick power (0.1)",(gyro.stick_power*10.0).round()as u8,1,80,false,0){gyro.stick_power=f64::from(value)/10.0;edited=true;}
            if let Some(value)=ui.range("host-gyro-stick-smooth",0,"Stick smoothing (ms)",gyro.stick_smoothing_ms.round()as u8,0,100,false,0){gyro.stick_smoothing_ms=f64::from(value);edited=true;}
            if edited{profile.to_mut().gyro=gyro;changed=true;}
        }
        let mut gyro=profile.gyro;
        let mut edited=false;
        if let Some(value)=ui.range("host-gyro-min-sens",0,"Slow sensitivity (0.1x)",(gyro.min_sensitivity*10.0).round().clamp(0.0,100.0)as u8,0,100,false,0){gyro.min_sensitivity=f64::from(value)/10.0;edited=true;}
        if let Some(value)=ui.range("host-gyro-max-sens",0,"Fast sensitivity (0.1x)",(gyro.max_sensitivity*10.0).round().clamp(0.0,100.0)as u8,0,100,false,0){gyro.max_sensitivity=f64::from(value)/10.0;edited=true;}
        if let Some(value)=ui.range("host-gyro-min-threshold",0,"Slow threshold (deg/s)",gyro.min_threshold.round().clamp(0.0,200.0)as u8,0,200,false,0){gyro.min_threshold=f64::from(value);gyro.max_threshold=gyro.max_threshold.max(gyro.min_threshold);edited=true;}
        if let Some(value)=ui.range("host-gyro-max-threshold",0,"Fast threshold (deg/s)",gyro.max_threshold.round().clamp(0.0,200.0)as u8,0,200,false,0){gyro.max_threshold=f64::from(value);gyro.min_threshold=gyro.min_threshold.min(gyro.max_threshold);edited=true;}
        if let Some(value)=ui.range("host-gyro-smooth-threshold",0,"Smoothing threshold (deg/s)",gyro.smooth_threshold.round().clamp(0.0,100.0)as u8,0,100,false,0){gyro.smooth_threshold=f64::from(value);edited=true;}
        if let Some(value)=ui.range("host-gyro-smooth-time",0,"Smoothing window (ms)",gyro.smooth_time_ms.round()as u8,0,250,false,0){gyro.smooth_time_ms=f64::from(value);edited=true;}
        if edited{profile.to_mut().gyro=gyro;changed=true;}
        ui.text("Slot mappings apply before host mappings. Profile-button slot changes are invisible: stop, Refresh, then restart mapping.",14,MUTED);
        ui.enabled=gyro_enabled;
        if ui.button("save-host-profile",0,"Save host profile",true){*action=Some(Action::SaveHostProfile);}
        if changed{*action=Some(Action::SetHostProfile(Box::new(profile.into_owned())));}
        ui.enabled=enabled;
    }
    fn header_settings(&self,ui:&mut Ui<'_,'_>,p:&Profiles){
        ui.text(format_args!("Polling rate: {} Hz",p.profile_bits.polling_hz()),18,TEXT);
        let enabled=ui.enabled;ui.enabled=false;
        ui.text(format_args!("PC (XInput) profile flag 0x10: {} (read-only)",if p.profile_bits.tournament(){"on"}else{"off"}),16,MUTED);
        let _=ui.toggle("auto-reconnect",0,"Auto Reconnect",p.profile_bits.auto_reconnect());
        ui.enabled=enabled;
        ui.text("Header flags are read-only. Use Home to switch receiver PC / Xbox mode; Auto Reconnect changes are not supported.",15,MUTED);
    }
    fn illustration(&mut self,ui:&mut Ui<'_,'_>,name:&'static str){
        if !self.artwork.image(ui,name,100,480.0,230.0){
            let [width,height]=self.diagram.fit(480.0,230.0);
            ui.frame.image("generic-page-image",Style::row(Size::Fixed(width),Size::Fixed(height)),&self.diagram);
        }
    }
    fn diagram(&mut self,ui:&mut Ui<'_,'_>){
        let name=if self.page==1&&self.rear{assets::BACK}else{assets::FRONT};
        let texture=self.artwork.texture(name).unwrap_or(&self.diagram);
        if let Some(i)=ui.controller_diagram(texture,&[("Left stick",0.0,0.0),("Mapping",0.0,0.0),("Right stick",0.0,0.0)]){self.page=if i==1{1}else{2};self.side=usize::from(i==2);}
    }
    fn test_artwork(&mut self,ui:&mut Ui<'_,'_>,model:&Model){
        let buttons=model.pad.map_or(0,|pad|pad.buttons);
        let lt=model.pad.is_some_and(|pad|pad.left_trigger>0);
        let rt=model.pad.is_some_and(|pad|pad.right_trigger>0);
        // Artwork-to-canvas coordinates were not recovered from available
        // metadata/IR. Labelled cells deliberately avoid guessed composites.
        macro_rules! paired {($label:literal,$active:expr)=>{(
            $label,concat!("XboxWired/Test/Test_",$label,"Normal.png"),
            concat!("XboxWired/Test/Test_",$label,"Enter.png"),$active
        )};}
        let pairs=[
            paired!("LB",buttons&256!=0),paired!("RB",buttons&512!=0),
            paired!("LT",lt),paired!("RT",rt),
            paired!("L3",buttons&64!=0),paired!("R3",buttons&128!=0),
            paired!("L4",model.paddles&PhysicalButtons::L4.bits()!=0),
            paired!("R4",model.paddles&PhysicalButtons::R4.bits()!=0),
            paired!("PL",model.paddles&PhysicalButtons::P1.bits()!=0),
            paired!("PR",model.paddles&PhysicalButtons::P2.bits()!=0),
        ];
        ui.element("test-original",0,Style::row(Size::Grow,Size::Fit).gap(16),|ui|{
            if !self.artwork.image(ui,assets::TEST,100,340.0,250.0){
                let [width,height]=self.diagram.fit(340.0,250.0);
                ui.frame.image("test-generic-image",Style::row(Size::Fixed(width),Size::Fixed(height)),&self.diagram);
            }
            ui.element("test-original-pairs",0,Style::column(Size::Grow,Size::Fit).gap(8),|ui|{
                for (row,controls) in pairs.chunks(5).enumerate(){
                    ui.element("test-art-row",row as u32,Style::row(Size::Grow,Size::Fit).gap(6),|ui|{
                        for (column,&(label,normal,pressed,active)) in controls.iter().enumerate(){
                            let index=(row*5+column)as u32;
                            self.test_art_cell(ui,index,label,if active{pressed}else{normal},active);
                        }
                    });
                }
            });
        });
        let controls=[
            ("Up","XboxWired/Test/Test_Up.png",4,1u16),
            ("Down","XboxWired/Test/Test_Down.png",5,2),
            ("Left","XboxWired/Test/Test_Left.png",6,4),
            ("Right","XboxWired/Test/Test_Right.png",7,8),
            ("A","XboxWired/Test/Test_A.png",8,4096),
            ("B","XboxWired/Test/Test_B.png",9,8192),
            ("X","XboxWired/Test/Test_X.png",10,16384),
            ("Y","XboxWired/Test/Test_Y.png",11,32768),
            ("View","XboxWired/Test/Test_Select.png",18,32),
            ("Menu","XboxWired/Test/Test_Start.png",19,16),
            ("Guide","XboxWired/Test/Test_Xbox.png",usize::MAX,1024),
        ];
        for (row,controls) in controls.chunks(6).enumerate(){
            ui.element("test-art-face-row",row as u32,Style::row(Size::Grow,Size::Fit).gap(6),|ui|{
                for (column,&(label,pressed,source,mask)) in controls.iter().enumerate(){
                    let active=buttons&mask!=0;
                    self.test_art_cell(ui,(10+row*6+column)as u32,label,if active||source==usize::MAX{pressed}else{assets::MAPPING[source].normal},active);
                }
            });
        }
    }
    fn test_art_cell(&mut self,ui:&mut Ui<'_,'_>,index:u32,label:&str,image:&'static str,active:bool){
        ui.element("test-art-cell",index,Style::column(Size::Grow,Size::Fixed(70.0)).padding(6).gap(4).centered().background(if active{clay::rgb(28,81,75)}else{PANEL}).rounded(6.0),|ui|{
            self.artwork.image(ui,image,index,44.0,34.0);
            ui.text(label,13,if active{ACCENT}else{MUTED});
        });
    }
    fn input_test(&mut self,ui:&mut Ui<'_,'_>,model:&Model){
        let link=model.device.as_ref().map(|device|device.link);
        if link==Some(Link::Wired)&&(model.mapper_running||model.mapper_pending){
            ui.text("The host mapper owns input. Stop the mapper to test wired controls.",20,TEXT);return;
        }
        self.test_artwork(ui,model);
        let (lx,ly,rx,ry)=model.pad.map(|p|(p.left_x,p.left_y,p.right_x,p.right_y)).unwrap_or((0,0,0,0));
        let marker=self.artwork.texture("XboxWired/Test/Test_Sticks.png");
        ui.element("live-sticks",0,Style::row(Size::Grow,Size::Fit).gap(28),|ui|{ui.plot_image("left-input",0,f32::from(lx)/32768.0,f32::from(ly)/32768.0,marker);ui.plot_image("right-input",0,f32::from(rx)/32768.0,f32::from(ry)/32768.0,marker);});
        if let Some(p)=model.pad{
            ui.text(format_args!("LS: {}, {}    RS: {}, {}",p.left_x,p.left_y,p.right_x,p.right_y),16,TEXT);
            ui.progress("test-lt",Some(f32::from(p.left_trigger)/255.0),"LT");ui.progress("test-rt",Some(f32::from(p.right_trigger)/255.0),"RT");
            for(row,chunk)in [(0,&[("Up",1),("Down",2),("Left",4),("Right",8),("Menu",16),("View",32),("L3",64),("R3",128)][..]),(1,&[("LB",256),("RB",512),("Guide",1024),("A",4096),("B",8192),("X",16384),("Y",32768)][..])]{
                ui.element("test-buttons",row,Style::row(Size::Grow,Size::Fit).gap(5),|ui|{let enabled=ui.enabled;ui.enabled=false;for(i,(label,mask))in chunk.iter().enumerate(){ui.button("test-button",row*8+i as u32,label,p.buttons&mask!=0);}ui.enabled=enabled;});
            }
        }else{ui.text(if link==Some(Link::Bluetooth){"Waiting for input — press a button or move a control…"}else if link==Some(Link::Wired){"Waiting for wired input…"}else{"Waiting for receiver input reports…"},20,TEXT);}
        if matches!(link,Some(Link::Bluetooth|Link::Receiver)){
            ui.element("test-paddles",0,Style::row(Size::Grow,Size::Fit).gap(5),|ui|{
                let enabled=ui.enabled;ui.enabled=false;
                for (index,(label,key)) in [("L4",PhysicalButtons::L4),("R4",PhysicalButtons::R4),("PL (P1)",PhysicalButtons::P1),("PR (P2)",PhysicalButtons::P2)].into_iter().enumerate(){ui.button("test-paddle",index as u32,label,model.paddles&key.bits()!=0);}
                ui.enabled=enabled;
            });
            ui.text(if link==Some(Link::Receiver){"Receiver paddles use physical reports enabled by the Windows 400-ms heartbeat, not XInput residue. Live GUI check pending."}else{"Bluetooth physical input is change-driven; silence is normal, not a disconnect. Paddles are independent of stored mappings."},15,MUTED);
        }else{
            ui.text("Verified XInput bits only. P1/P2/L4/R4/Profile and raw IMU are unproven in this stream; states are not fabricated.",15,MUTED);
        }
    }
}
fn unavailable(ui:&mut Ui<'_,'_>){ui.text("Profile data unavailable",23,TEXT);ui.text("Connect and read a controller profile before editing.",17,MUTED);}
struct SlotLabel<'a>(&'a Slot);
impl std::fmt::Display for SlotLabel<'_>{
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{
        if !self.0.is_valid(){return f.write_str("Empty");}
        let units=self.0.name.raw().chunks_exact(2).map(|b|u16::from_be_bytes([b[0],b[1]])).take_while(|&u|u!=0);
        for c in char::decode_utf16(units){std::fmt::Write::write_char(f,c.unwrap_or(char::REPLACEMENT_CHARACTER))?;}
        Ok(())
    }
}
fn curve_preset_index(curve:&StickCurve)->usize{
    let points=[curve.points[1].input.get(),curve.points[1].output.get(),curve.points[2].input.get(),curve.points[2].output.get()];
    [[36,33,65,66],[36,16,65,83],[36,16,73,66],[15,16,36,66],[29,16,29,66],[51,50,73,66]].iter().position(|p|*p==points).map_or(0,|i|i+1)
}
fn color_axes(c:Color)->(u8,u8){
    let(r,g,b)=(f32::from(c.red.get()),f32::from(c.green.get()),f32::from(c.blue.get()));let max=r.max(g).max(b);let delta=max-r.min(g).min(b);
    let h=if delta==0.0{0.0}else if max==r{((g-b)/delta).rem_euclid(6.0)}else if max==g{(b-r)/delta+2.0}else{(r-g)/delta+4.0};((h*42.5)as u8,max as u8)
}
fn hue_color(h:u8,s:u8,brightness:u8)->Color{
    let h=f32::from(h)/42.5;let x=1.0-(h%2.0-1.0).abs();let(r,g,b)=match h as u8{0=>(1.0,x,0.0),1=>(x,1.0,0.0),2=>(0.0,1.0,x),3=>(0.0,x,1.0),4=>(x,0.0,1.0),_=>(1.0,0.0,x)};
    Color::new(u16::from(brightness),(r*f32::from(s))as u16,(g*f32::from(s))as u16,(b*f32::from(s))as u16).expect("bounded color")
}
struct Window;
impl Drop for Window{fn drop(&mut self){unsafe{f::Ultimate_Close()}}}
struct Font(f::Font);
impl Drop for Font{fn drop(&mut self){unsafe{f::UnloadFont(self.0)}}}

#[derive(Default)]
struct PadLatency { seen:Option<Instant>,samples:u64,total:f64,max:f64 }
impl PadLatency {
    fn presented(&mut self,received:Option<Instant>){
        if received==self.seen{return}
        self.seen=received;
        if let Some(received)=received{
            let ms=received.elapsed().as_secs_f64()*1000.0;
            self.samples+=1;self.total+=ms;self.max=self.max.max(ms);
        }
    }
    fn report(&self){if self.samples>0{eprintln!("gui bench: pad worker-receipt to swap mean {:.3} max {:.3} ms ({} changed states; latest-only mailbox, no pad queue)",self.total/self.samples as f64,self.max,self.samples);}}
}

fn debug_pointer(ui:&mut Ui<'_,'_>,metrics:f::Ultimate_WindowMetrics,coordinates:Coordinates){
    let overlay=|style:Style|{
        let mut style=style;style.0.floating.pointerCaptureMode=1;style
    };
    let [x,y]=coordinates.pointer;
    ui.element("pointer-debug-horizontal",0,overlay(Style::row(Size::Fixed(25.0),Size::Fixed(2.0)).background(FOCUS).floating(None,x-12.0,y,201)),|_|{});
    ui.element("pointer-debug-vertical",0,overlay(Style::column(Size::Fixed(2.0),Size::Fixed(25.0)).background(FOCUS).floating(None,x,y-12.0,201)),|_|{});
    ui.element("pointer-debug-panel",0,overlay(Style::column(Size::Fixed(630.0),Size::Fit).padding(8).gap(3).background(PANEL).border(FOCUS,1).floating(None,(coordinates.layout[0]-640.0).max(0.0),2.0,200)),|ui|{
        ui.text(format_args!("Native GLFW raw/logical: ({:.2}, {:.2})   Clay: ({x:.2}, {y:.2})",metrics.pointer.x,metrics.pointer.y),14,FOCUS);
        ui.text(format_args!("Raylib scaled: ({:.2}, {:.2})   camera scale: {:.4}",metrics.raylibPointer.x,metrics.raylibPointer.y,coordinates.scale),14,TEXT);
        ui.text(format_args!("Window: {:.0}x{:.0}   framebuffer: {:.0}x{:.0}   DPI: {:.4}/{:.4}   HIGHDPI: {}",metrics.window.x,metrics.window.y,metrics.framebuffer.x,metrics.framebuffer.y,metrics.dpi.x,metrics.dpi.y,metrics.highDpi),13,MUTED);
    });
}

pub fn run(state:&mut impl GuiState,options:Options)->Result<()> {
    let initial_page=if let Some(page)=&options.page{
        PAGES.iter().position(|p|p.eq_ignore_ascii_case(page)||(*p=="Lighting"&&page.eq_ignore_ascii_case("RGB"))).ok_or_else(||crate::invalid(format!("Unknown GUI page {page:?}; choose {}",PAGES.join(", "))))?
    }else{0};
    if let Some(path)=&options.screenshot{
        if !path.is_absolute(){return Err(crate::invalid("--screenshot requires an absolute PNG path"));}
        if path.extension().and_then(|extension|extension.to_str()).is_none_or(|extension|!extension.eq_ignore_ascii_case("png")){return Err(crate::invalid("--screenshot path must end in .png"));}
        if path.try_exists()?{return Err(crate::invalid(format!("Refusing to overwrite {}",path.display())));}
    }
    unsafe{f::SetTraceLogLevel(f::LOG_WARNING as i32);f::Ultimate_Initialize(1180,820,c"Ultimate Control".as_ptr());}
    let _window=Window;
    if !unsafe{f::IsWindowReady()}{return Err(crate::invalid("Raylib window initialization failed"))}
    unsafe{f::SetWindowMinSize(880,640);f::SetExitKey(f::KEY_NULL as i32);}
    let bytes=include_bytes!("../../assets/fonts/InterVariable.ttf");
    // Non-ASCII glyphs used by UI/guide text; anything else renders as '?'.
    const EXTRA:[i32;7]=[0x2013,0x2014,0x2026,0x2192,0x00b0,0x00d7,0x2265];
    let mut codepoints:[i32;95+EXTRA.len()]=std::array::from_fn(|i|if i<95{32+i as i32}else{EXTRA[i-95]});
    let mut font=Font(unsafe{f::LoadFontFromMemory(c".ttf".as_ptr(),bytes.as_ptr(),bytes.len() as i32,32,codepoints.as_mut_ptr(),codepoints.len()as i32)});
    if font.0.texture.id==0{return Err(crate::invalid("Bundled Inter font failed to load"))}
    unsafe{f::SetTextureFilter(font.0.texture,f::TEXTURE_FILTER_BILINEAR as i32);}
    let mut clay=Clay::new(1180.0,820.0)?;
    unsafe{f::Ultimate_MeasureFont(&mut font.0)};
    let diagram=Texture::png(include_bytes!("../../assets/controller-diagram.png"))?;
    let artwork=Artwork::discover(options.assets.as_deref());
    let asset_path=artwork.root.as_ref().map(|path|path.to_string_lossy().into_owned()).unwrap_or_default();
    let mut app=App{page:initial_page,side:0,rear:false,color:0,modal:None,diagram,artwork,asset_path,assets_cli:options.assets.clone(),error:None,rename:String::new(),rename_slot:usize::MAX,rename_original:[0;32],remap_source:0,remap_layer:0,host_name:String::new(),host_selected:String::new(),rule_index:0,rule_process:String::new(),rule_profile:0,rule_original:None};
    let mut focus=Focus::default();
    focus.selected=clay::id("nav",initial_page as u32).id;
    let mut dirty=true;let mut last_pointer=[-1.0,-1.0];let mut last_size=[0,0];let mut last_scale=0.0;
    let mut last_pointer_down=false;
    let mut previous_axis=[0i8;2];let mut previous_time=unsafe{f::GetTime()};
    let mut samples=0u32;let(mut layout_total,mut render_total,mut layout_max,mut render_max)=(0.0f64,0.0f64,0.0f64,0.0f64);
    let mut capture_frames=0u32;
    let capture_started=Instant::now();
    let benchmark=options.bench||options.screenshot.is_some();
    let mut pad_latency=PadLatency::default();
    while !unsafe{f::WindowShouldClose()}&&!crate::interrupted(){
        state.set_test_visible(app.page==9)?;
        dirty|=state.poll()?;
        dirty|=options.screenshot.is_some();
        if options.screenshot.is_some()&&capture_started.elapsed().as_secs()>30{return Err(crate::invalid(format!("Screenshot timed out: {}; {}",state.model().operation.as_ref().map_or("profiles unavailable",|op|op.title.as_str()),state.model().notice.as_deref().unwrap_or("Press a controller button, then Refresh"))));}
        // Query native GLFW metrics every frame: raylib's non-Apple HIGHDPI
        // callback applies an extra inverse-DPI scale to GetMousePosition.
        let metrics=unsafe{f::Ultimate_GetWindowMetrics()};
        let coordinates=Coordinates::new([metrics.pointer.x,metrics.pointer.y],[metrics.window.x,metrics.window.y],[metrics.framebuffer.x,metrics.framebuffer.y],metrics.dpi.x,metrics.highDpi);
        let scale=coordinates.scale;
        let size=[metrics.framebuffer.x as i32,metrics.framebuffer.y as i32];
        let pointer=coordinates.pointer;
        let pressed=|key:u32|unsafe{f::IsKeyPressed(key as i32)||f::IsKeyPressedRepeat(key as i32)};
        let gamepad=|button:u32|unsafe{f::IsGamepadAvailable(0)&&f::IsGamepadButtonPressed(0,button as i32)};
        let axis=if unsafe{f::IsGamepadAvailable(0)}{unsafe{[f::GetGamepadAxisMovement(0,0),f::GetGamepadAxisMovement(0,1)]}.map(|v|if v>0.55{1}else if v< -0.55{-1}else{0})}else{[0,0]};
        let mut chars=[0;16];for code in &mut chars{*code=unsafe{GetCharPressed()}as u32;if *code==0{break;}}
        let mut input=Input{pointer,click:unsafe{f::IsMouseButtonPressed(0)},down:unsafe{f::IsMouseButtonDown(0)},activate:pressed(f::KEY_ENTER)||pressed(f::KEY_SPACE)||gamepad(f::GAMEPAD_BUTTON_RIGHT_FACE_DOWN),back:pressed(f::KEY_ESCAPE)||gamepad(f::GAMEPAD_BUTTON_RIGHT_FACE_RIGHT),left:pressed(f::KEY_LEFT)||gamepad(f::GAMEPAD_BUTTON_LEFT_FACE_LEFT)||(axis[0]== -1&&previous_axis[0]!= -1),right:pressed(f::KEY_RIGHT)||gamepad(f::GAMEPAD_BUTTON_LEFT_FACE_RIGHT)||(axis[0]==1&&previous_axis[0]!=1),up:pressed(f::KEY_UP)||gamepad(f::GAMEPAD_BUTTON_LEFT_FACE_UP)||(axis[1]== -1&&previous_axis[1]!= -1),next:pressed(f::KEY_DOWN)||gamepad(f::GAMEPAD_BUTTON_LEFT_FACE_DOWN)||(axis[1]==1&&previous_axis[1]!=1),previous:false,chars,backspace:pressed(f::KEY_BACKSPACE)};
        if pressed(f::KEY_TAB){if unsafe{f::IsKeyDown(f::KEY_LEFT_SHIFT as i32)||f::IsKeyDown(f::KEY_RIGHT_SHIFT as i32)}{input.previous=true}else{input.next=true}}
        previous_axis=axis;
        let wheel=unsafe{f::GetMouseWheelMoveV()};
        let now=unsafe{f::GetTime()};let dt=(now-previous_time).min(0.1) as f32;previous_time=now;
        dirty|=pointer!=last_pointer||size!=last_size||scale!=last_scale||input.click||input.down||input.activate||input.back||input.left||input.right||input.up||input.next||input.previous||wheel.x!=0.0||wheel.y!=0.0||unsafe{f::IsWindowResized()};
        dirty|=input.chars[0]!=0||input.backspace;
        dirty|=input.down!=last_pointer_down;last_pointer_down=input.down;
        last_pointer=pointer;last_size=size;last_scale=scale;
        // Sample again just before layout so an arrival during input processing
        // is included in this frame, rather than waiting for another swap.
        dirty|=state.poll()?;
        if dirty {
            focus.begin(input);
            let started=Instant::now();
            let frame=clay.begin(coordinates.layout[0],coordinates.layout[1],pointer,input.down,[wheel.x,wheel.y],dt);
            let mut frame=frame;
            let mut ui=Ui{frame:&mut frame,focus:&mut focus,input,enabled:app.modal.is_none()};
            let action=app.draw(&mut ui,state.model());
            if options.debug_pointer{debug_pointer(&mut ui,metrics,coordinates);}
            let pad_received_at=state.model().pad_received_at;
            let mut layout_ms=started.elapsed().as_secs_f64()*1000.0;
            let render_started=Instant::now();
            unsafe{f::BeginDrawing();f::ClearBackground(f::Color{r:17,g:21,b:29,a:255});f::BeginMode2D(f::Camera2D{offset:f::Vector2{x:0.0,y:0.0},target:f::Vector2{x:0.0,y:0.0},rotation:0.0,zoom:scale});}
            let layout_extra=frame.render(dt,&mut font.0,scale);layout_ms+=layout_extra;
            unsafe{f::EndMode2D();}
            let render_ms=(render_started.elapsed().as_secs_f64()*1000.0-layout_extra).max(0.0);
            unsafe{f::EndDrawing();}
            if benchmark{pad_latency.presented(pad_received_at);}
            dirty=false;
            if let Some(action)=action {if let Err(error)=state.dispatch(action){app.error=Some(error.to_string());}dirty=true;}
            state.set_test_visible(app.page==9)?;
            // Focus/page/dropdown/modal changes need one fresh layout even with
            // a stationary pointer. There is no continuous redraw while idle.
            dirty|=input.activate||input.back||input.click||input.left||input.right||input.up||input.next||input.previous||wheel.y!=0.0;
            if benchmark{samples+=1;layout_total+=layout_ms;render_total+=render_ms;layout_max=layout_max.max(layout_ms);render_max=render_max.max(render_ms);if samples==120{eprintln!("gui bench (120 drawn frames; vsync excluded): layout mean {:.3} max {:.3} ms; render mean {:.3} max {:.3} ms",layout_total/120.0,layout_max,render_total/120.0,render_max);pad_latency.report();samples=0;layout_total=0.0;render_total=0.0;layout_max=0.0;render_max=0.0;}}
            if let Some(path)=&options.screenshot{
                if state.model().profiles.is_some()&&state.model().operation.is_none()&&(app.page!=9||state.model().demo||state.model().pad.is_some()){capture_frames+=1;}else{capture_frames=0;}
                if capture_frames==10{capture(path)?;println!("GUI screenshot: {}",path.display());break;}
            }
        } else if app.page==9||focus.dragging!=0 {
            unsafe{f::WaitTime(0.001);f::PollInputEvents();}
        } else {
            // Reset edge state, then dispatch pending callbacks BEFORE waiting.
            // Never poll again after the native wait, or fresh key edges vanish.
            unsafe{f::Ultimate_PrepareWait();}
            dirty|=unsafe{f::Ultimate_PollInputEvents()};
            dirty|=state.poll()?;
            if !dirty{unsafe{f::Ultimate_WaitEventsTimeout(0.25);}}
        }
    }
    if options.screenshot.is_some()&&capture_frames<10{return Err(crate::invalid("Window closed before screenshot capture completed"));}
    if benchmark&&samples>0{eprintln!("gui bench ({samples} drawn frames; vsync excluded): layout mean {:.3} max {:.3} ms; render mean {:.3} max {:.3} ms",layout_total/samples as f64,layout_max,render_total/samples as f64,render_max);}
    if benchmark{pad_latency.report();}
    Ok(())
}

fn capture(path:&std::path::Path)->Result<()> {
    let mut size=0i32;
    let bytes=unsafe{
        let image=f::LoadImageFromScreen();
        if image.data.is_null(){return Err(crate::invalid("Screenshot framebuffer read failed"));}
        let bytes=f::ExportImageToMemory(image,c".png".as_ptr(),&mut size);
        f::UnloadImage(image);
        bytes
    };
    if bytes.is_null()||size<=0{
        if !bytes.is_null(){unsafe{f::MemFree(bytes.cast())};}
        return Err(crate::invalid("Screenshot PNG encoding failed"));
    }
    let result=(||{
        let mut output=OpenOptions::new().write(true).create_new(true).open(path)?;
        let result=output.write_all(unsafe{std::slice::from_raw_parts(bytes,size as usize)}).and_then(|_|output.sync_all());
        if result.is_err(){let _=std::fs::remove_file(path);}
        result
    })();
    unsafe{f::MemFree(bytes.cast())};
    result?;
    Ok(())
}

#[cfg(test)]
static GUI_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Exercises the real Clay widget, page action selection, and confirmation
/// dialog without a window, transport, or GPU. Called by the live save test.
#[cfg(test)]
pub(crate) fn exercise_motion_save(state:&mut impl GuiState,pointer:bool,target:u8){
    let _guard=GUI_TEST_LOCK.lock();
    unsafe extern "C" fn measure(text:f::Clay_StringSlice,config:*mut f::Clay_TextElementConfig,_:*mut std::ffi::c_void)->f::Clay_Dimensions{
        f::Clay_Dimensions{width:text.length as f32*8.0,height:unsafe{(*config).fontSize}as f32}
    }
    fn bounds(name:&'static str)->f::Clay_BoundingBox{
        let data=unsafe{f::Clay_GetElementData(clay::id(name,0))};assert!(data.found,"{name} not laid out");data.boundingBox
    }
    fn frame(app:&mut App,clay:&mut Clay,focus:&mut Focus,state:&mut impl GuiState,input:Input){
        focus.begin(input);
        let mut frame=clay.begin(1180.0,820.0,input.pointer,input.down,[0.0,0.0],0.016);
        let action=app.draw(&mut Ui{frame:&mut frame,focus,input,enabled:app.modal.is_none()},state.model());
        drop(frame);
        if let Some(action)=action{state.dispatch(action).unwrap();}
    }
    let mut clay=Clay::new(1180.0,820.0).unwrap();
    unsafe{f::Clay_SetMeasureTextFunction(Some(measure),std::ptr::null_mut());}
    let mut app=App{page:6,side:0,rear:false,color:0,modal:None,diagram:Texture::layout_only(),artwork:Artwork::default(),asset_path:String::new(),assets_cli:None,error:None,rename:String::new(),rename_slot:usize::MAX,rename_original:[0;32],remap_source:0,remap_layer:0,host_name:String::new(),host_selected:String::new(),rule_index:0,rule_process:String::new(),rule_profile:0,rule_original:None};
    let mut focus=Focus::default();
    frame(&mut app,&mut clay,&mut focus,state,Input::default());
    assert_eq!(state.model().active_slot,0,"raw 0xff must edit slot 1, not slot 3");
    if pointer{
        let b=bounds("motion-sensitivity");
        let start=state.model().profiles.as_ref().unwrap().slots[0].motion.sensitivity.get();
        let start_pointer=[b.x+14.0+(b.width-28.0)*f32::from(start)/5.0,b.y+b.height/2.0];
        frame(&mut app,&mut clay,&mut focus,state,Input{pointer:start_pointer,click:true,down:true,..Input::default()});
        let pointer=[b.x+14.0+(b.width-28.0)*f32::from(target)/5.0,b.y+b.height/2.0];
        frame(&mut app,&mut clay,&mut focus,state,Input{pointer,down:true,..Input::default()});
        frame(&mut app,&mut clay,&mut focus,state,Input{pointer,..Input::default()});
    }else{
        focus.selected=clay::id("motion-sensitivity",0).id;
        for _ in 0..5{
            let current=state.model().profiles.as_ref().unwrap().slots[0].motion.sensitivity.get();if current==target{break;}
            frame(&mut app,&mut clay,&mut focus,state,Input{left:current>target,right:current<target,..Input::default()});
        }
        // Both directions must dispatch through the widget, not a direct edit.
        frame(&mut app,&mut clay,&mut focus,state,Input{left:true,..Input::default()});
        frame(&mut app,&mut clay,&mut focus,state,Input{right:true,..Input::default()});
    }
    assert_eq!(state.model().profiles.as_ref().unwrap().slots[0].motion.sensitivity.get(),target);
    let b=bounds("save");let pointer=[b.x+b.width/2.0,b.y+b.height/2.0];
    frame(&mut app,&mut clay,&mut focus,state,Input{pointer,click:true,down:true,..Input::default()});
    assert!(matches!(app.modal,Some(Action::Save)));
    frame(&mut app,&mut clay,&mut focus,state,Input{pointer,..Input::default()});
    let b=bounds("modal-confirm");let pointer=[b.x+b.width/2.0,b.y+b.height/2.0];
    frame(&mut app,&mut clay,&mut focus,state,Input{pointer,click:true,down:true,..Input::default()});
    assert!(app.modal.is_none());
    assert!(state.model().operation.is_some(),"confirmed Save must reach LiveState");
}

#[cfg(test)]
mod remap_tests {
    use super::*;
    #[test]
    fn receiver_opt_in_and_link_specific_clutch_controls(){
        let _guard=GUI_TEST_LOCK.lock();
        unsafe extern "C" fn measure(text:f::Clay_StringSlice,config:*mut f::Clay_TextElementConfig,_:*mut std::ffi::c_void)->f::Clay_Dimensions{
            f::Clay_Dimensions{width:text.length as f32*8.0,height:unsafe{(*config).fontSize}as f32}
        }
        fn frame(app:&mut App,clay:&mut Clay,focus:&mut Focus,model:&mut Model,input:Input)->bool{
            focus.begin(input);
            let mut frame=clay.begin(1180.0,820.0,input.pointer,input.down,[0.0,0.0],0.016);
            let action=app.draw(&mut Ui{frame:&mut frame,focus,input,enabled:true},model);
            drop(frame);
            let changed=action.is_some();
            if let Some(action)=action{assert!(matches!(action,Action::SetHostProfile(_)));model.edit(action).unwrap();}
            changed
        }
        let mut clay=Clay::new(1180.0,820.0).unwrap();
        unsafe{f::Clay_SetMeasureTextFunction(Some(measure),std::ptr::null_mut());}
        let mut app=App{page:10,side:0,rear:false,color:0,modal:None,diagram:Texture::layout_only(),artwork:Artwork::default(),asset_path:String::new(),assets_cli:None,error:None,rename:String::new(),rename_slot:usize::MAX,rename_original:[0;32],remap_source:0,remap_layer:0,host_name:String::new(),host_selected:String::new(),rule_index:0,rule_process:String::new(),rule_profile:0,rule_original:None};
        let mut model=Model{device:Some(Device{name:"test".into(),link:Link::Receiver,pid:0x3024,version_record:None}),..Model::default()};
        let mut focus=Focus::default();
        assert!(!frame(&mut app,&mut clay,&mut focus,&mut model,Input::default()));
        assert_eq!(model.host_profile.button,"R4");
        assert!(!model.host_profile.receiver_gyro);
        focus.selected=clay::id("host-receiver-gyro",0).id;
        assert!(frame(&mut app,&mut clay,&mut focus,&mut model,Input{activate:true,..Input::default()}));
        assert!(model.host_profile.receiver_gyro);
        assert_eq!(model.host_profile.button,"R4");
        focus.selected=clay::id("host-gyro-stick-speed",0).id;
        assert!(frame(&mut app,&mut clay,&mut focus,&mut model,Input{right:true,..Input::default()}));
        assert_eq!(model.host_profile.sensitivity,1.1);
        let dropdown=clay::id("host-gyro-clutch",0).id;
        for (index,(button,_)) in input::RECEIVER_CLUTCH_KEYS.iter().enumerate(){
            focus.dropdown=dropdown;
            focus.selected=clay::id("dropdown-option",dropdown.wrapping_add(index as u32)).id;
            assert!(frame(&mut app,&mut clay,&mut focus,&mut model,Input{activate:true,..Input::default()}));
            assert_eq!(model.host_profile.button,*button);
        }
        focus.selected=clay::id("host-gyro-output",2).id;
        assert!(frame(&mut app,&mut clay,&mut focus,&mut model,Input{activate:true,..Input::default()}));
        assert!(model.host_profile.gyro_mouse.is_some());
        frame(&mut app,&mut clay,&mut focus,&mut model,Input::default());
        assert!(unsafe{f::Clay_GetElementData(clay::id("host-gyro-mouse-deadzone",0))}.found);
        focus.selected=clay::id("host-gyro-mouse-speed",0).id;
        assert!(frame(&mut app,&mut clay,&mut focus,&mut model,Input{right:true,..Input::default()}));
        assert_eq!(model.host_profile.gyro_mouse.unwrap().sensitivity,1.1);
        focus.selected=clay::id("host-gyro-smooth-threshold",0).id;
        assert!(frame(&mut app,&mut clay,&mut focus,&mut model,Input{right:true,..Input::default()}));
        assert_eq!(model.host_profile.gyro.smooth_threshold,6.0);
        let button=model.host_profile.button.clone();
        focus.selected=clay::id("host-receiver-gyro",0).id;
        frame(&mut app,&mut clay,&mut focus,&mut model,Input{activate:true,..Input::default()});
        assert!(!model.host_profile.receiver_gyro);
        assert_eq!(model.host_profile.button,button);
        model.device.as_mut().unwrap().link=Link::Bluetooth;
        assert!(!frame(&mut app,&mut clay,&mut focus,&mut model,Input::default()));
        for (index,button) in ["R4","L4","PL","PR"].iter().enumerate(){
            focus.dropdown=dropdown;
            focus.selected=clay::id("dropdown-option",dropdown.wrapping_add(index as u32)).id;
            assert!(frame(&mut app,&mut clay,&mut focus,&mut model,Input{activate:true,..Input::default()}));
            assert_eq!(model.host_profile.button,*button);
        }
        model.device.as_mut().unwrap().link=Link::Receiver;
        assert!(!frame(&mut app,&mut clay,&mut focus,&mut model,Input::default()));
        assert_eq!(model.host_profile.button,"PR");
    }
}

#[cfg(test)]
mod native_smoke {
    use super::*;
    #[test]
    fn glfw_context_metrics_are_valid_in_a_real_display_window() {
        if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_none() {
            eprintln!("Skipping native GLFW smoke test: no display environment");
            return;
        }
        unsafe {
            f::SetTraceLogLevel(f::LOG_WARNING as i32);
            f::Ultimate_Initialize(480, 320, c"Ultimate Control native GLFW smoke".as_ptr());
        }
        let window = Window;
        assert!(unsafe { f::IsWindowReady() }, "native display window must initialize");
        let metrics = unsafe { f::Ultimate_GetWindowMetrics() };
        assert!(metrics.window.x > 0.0 && metrics.window.y > 0.0);
        assert!(metrics.framebuffer.x > 0.0 && metrics.framebuffer.y > 0.0);
        assert!(metrics.pointer.x.is_finite() && metrics.pointer.y.is_finite());
        for _ in 0..3 {
            unsafe {
                f::BeginDrawing();
                f::ClearBackground(f::Color { r: 17, g: 21, b: 29, a: 255 });
                f::EndDrawing();
            }
        }
        unsafe { f::Ultimate_PrepareWait(); }
        let notify = std::thread::spawn(crate::gui::wake);
        notify.join().unwrap();
        // Covers the coalesced native notification path as well as the handle.
        unsafe { f::Ultimate_WaitEventsTimeout(0.01); f::Ultimate_PollInputEvents(); }
        drop(window);
        crate::gui::wake(); // notifications after teardown must be harmless
    }
}
