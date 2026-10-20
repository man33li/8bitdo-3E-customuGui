//! Named host profiles and deterministic, user-scoped process selection.
use crate::{Result, input::HostProfile, invalid};
use serde::{Deserialize, Serialize};
use std::{fs::{self, File, OpenOptions}, io::{Read,Write}, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}, time::Duration};
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule { pub profile:String, pub process:String }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutoConfig { pub default:String, pub rules:Vec<Rule> }
impl Default for AutoConfig { fn default()->Self{Self{default:"default".into(),rules:Vec::new()}} }
impl AutoConfig {
    pub fn validate(&self)->Result<()> {
        validate_name(&self.default)?;
        for rule in &self.rules {
            validate_name(&rule.profile)?;
            if rule.process.trim().is_empty(){return Err(invalid("Process matcher cannot be empty"));}
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct Process { pub comm:String, pub exe:String, pub cmdline:String }
fn matches(rule:&Rule,process:&Process)->bool {
    let needle=rule.process.as_bytes();
    if rule.process.is_ascii()&&process.comm.is_ascii()&&process.exe.is_ascii()&&process.cmdline.is_ascii(){
        process.comm.eq_ignore_ascii_case(&rule.process)||process.exe.eq_ignore_ascii_case(&rule.process)
            ||process.cmdline.as_bytes().windows(needle.len().max(1)).any(|window|window.eq_ignore_ascii_case(needle))
    }else{
        let needle=rule.process.to_lowercase();
        process.comm.to_lowercase()==needle||process.exe.to_lowercase()==needle||process.cmdline.to_lowercase().contains(&needle)
    }
}
pub fn matching_rule<'a>(rules:&'a [Rule],processes:&[Process])->Option<&'a Rule>{
    rules.iter().find(|rule|!rule.process.trim().is_empty()&&processes.iter().any(|process|matches(rule,process)))
}
pub fn select<'a>(rules:&'a [Rule],processes:&[Process])->Option<&'a str>{matching_rule(rules,processes).map(|rule|rule.profile.as_str())}
/// Read only /proc; an absent/unreadable UID never counts as the current user.
#[cfg(target_os = "linux")]
pub fn collect_processes(root:&Path,uid:u32)->Vec<Process>{
    let Ok(entries)=fs::read_dir(root)else{return Vec::new()};
    entries.filter_map(|entry|{
        let entry=entry.ok()?;
        if !entry.file_name().to_string_lossy().bytes().all(|byte|byte.is_ascii_digit()){return None;}
        let path=entry.path();let status=fs::read_to_string(path.join("status")).ok()?;
        let owner=status.lines().find_map(|line|line.strip_prefix("Uid:").and_then(|value|value.split_whitespace().next()?.parse::<u32>().ok()))?;
        if owner!=uid{return None;}
        let comm=fs::read_to_string(path.join("comm")).unwrap_or_default().trim_end_matches('\n').to_owned();
        let exe=fs::read_link(path.join("exe")).ok().and_then(|path|path.file_name().map(|name|name.to_string_lossy().into_owned())).unwrap_or_default();
        let cmdline=fs::read(path.join("cmdline")).map(|bytes|String::from_utf8_lossy(&bytes).replace('\0'," ")).unwrap_or_default();
        Some(Process{comm,exe,cmdline})
    }).collect()
}
/// A new target must remain selected for two seconds; unchanged targets never restart.
#[derive(Default)]
pub struct Debounce { pending:Option<(String,Duration)> }
impl Debounce {
    pub fn changed(&mut self,current:&str,target:&str,now:Duration)->bool{
        if target==current{self.pending=None;return false;}
        match &self.pending {
            Some((name,since)) if name==target=>now.saturating_sub(*since)>=Duration::from_secs(2),
            _=>{self.pending=Some((target.into(),now));false},
        }
    }
}
#[derive(Clone, Debug)]
pub struct Store { root:PathBuf }
impl Default for Store { fn default()->Self{Self::new(crate::ui::default_profile_path().parent().expect("config parent").to_owned())} }
impl Store {
    pub fn new(root:PathBuf)->Self{Self{root}}
    pub fn path(&self,name:&str)->Result<PathBuf>{validate_name(name)?;Ok(self.root.join("profiles").join(format!("{name}.json")))}
    pub fn initialize(&self)->Result<()> {
        let directory=self.root.join("profiles");
        if !directory.exists(){
            let legacy=self.root.join("host.json");
            if legacy.exists(){
                let mut bytes=Vec::new();File::open(legacy)?.take(65537).read_to_end(&mut bytes)?;
                if bytes.len()>65536{return Err(invalid("Host profile exceeds 64KiB"));}
                serde_json::from_slice::<HostProfile>(&bytes)?.validate_structure()?;
                fs::create_dir_all(&directory)?;
                if let Err(error)=atomic_write(&self.path("default")?,&bytes,false){let _=fs::remove_dir(&directory);return Err(error);}
            }else{self.save("default",&HostProfile::default())?;}
        }
        Ok(())
    }
    pub fn names(&self)->Result<Vec<String>>{
        let mut names=Vec::new();
        for entry in fs::read_dir(self.root.join("profiles"))?{
            let entry=entry?;let path=entry.path();
            if path.extension().is_some_and(|ext|ext=="json")&&entry.file_type()?.is_file(){
                if let Some(name)=path.file_stem().and_then(|name|name.to_str()).filter(|name|validate_name(name).is_ok()){names.push(name.to_owned());}
            }
        }
        names.sort();Ok(names)
    }
    pub fn load(&self,name:&str)->Result<HostProfile>{crate::ui::load_profile(&self.path(name)?)}
    pub fn save(&self,name:&str,profile:&HostProfile)->Result<()>{crate::ui::save_profile(&self.path(name)?,profile,true)}
    pub fn create(&self,name:&str,profile:&HostProfile)->Result<()>{
        profile.validate_structure()?;
        atomic_write(&self.path(name)?,&serde_json::to_vec_pretty(profile)?,false)
    }
    pub fn load_auto(&self)->Result<AutoConfig>{
        let config=match fs::read(self.root.join("auto.json")){
            Ok(bytes)=>serde_json::from_slice::<AutoConfig>(&bytes)?,
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>AutoConfig::default(),
            Err(error)=>return Err(error.into()),
        };
        config.validate()?;Ok(config)
    }
    pub fn save_auto(&self,config:&AutoConfig)->Result<()>{
        config.validate()?;self.load(&config.default)?;
        for rule in &config.rules{self.load(&rule.profile)?;}
        atomic_write(&self.root.join("auto.json"),&serde_json::to_vec_pretty(config)?,true)
    }
    pub fn rename(&self,old:&str,new:&str,config:&mut AutoConfig)->Result<()>{
        if old==new{return Ok(());}
        let profile=self.load(old)?;self.create(new,&profile)?;
        let mut updated=config.clone();
        if updated.default==old{updated.default=new.into();}
        for rule in &mut updated.rules{if rule.profile==old{rule.profile=new.into();}}
        if let Err(error)=self.save_auto(&updated){let _=fs::remove_file(self.path(new)?);return Err(error);}
        fs::remove_file(self.path(old)?)?;*config=updated;Ok(())
    }
    pub fn delete(&self,name:&str,config:&AutoConfig)->Result<()>{
        if name==config.default{return Err(invalid("Cannot delete the default host profile"));}
        if config.rules.iter().any(|rule|rule.profile==name){return Err(invalid("Remove auto-switch rules for this profile before deleting it"));}
        fs::remove_file(self.path(name)?)?;Ok(())
    }
}
fn validate_name(name:&str)->Result<()>{
    if name.is_empty()||name.len()>64||name.trim()!=name||!name.chars().all(|c|c.is_alphanumeric()||matches!(c,' '|'-'|'_')){
        return Err(invalid("Profile name must be 1–64 bytes: letters, numbers, spaces, '-' or '_'"));
    }
    Ok(())
}
static NEXT_TEMP:AtomicU64=AtomicU64::new(1);
fn atomic_write(path:&Path,bytes:&[u8],replace:bool)->Result<()>{
    let parent=path.parent().ok_or_else(||invalid("Missing configuration directory"))?;fs::create_dir_all(parent)?;
    if fs::symlink_metadata(path).is_ok_and(|meta|meta.file_type().is_symlink()){return Err(invalid("Refusing to replace symlink configuration"));}
    let temporary=parent.join(format!(".profiles-{}-{}.tmp",std::process::id(),NEXT_TEMP.fetch_add(1,Ordering::Relaxed)));
    let mut options=OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "linux")]
    options.mode(0o600);
    let mut file=options.open(&temporary)?;
    let result=(||->Result<()>{
        file.write_all(bytes)?;file.sync_all()?;
        if replace{fs::rename(&temporary,path)?;}else{fs::hard_link(&temporary,path)?;fs::remove_file(&temporary)?;}
        #[cfg(target_os = "linux")]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_file(temporary);}result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp{fn new()->Self{let path=std::env::temp_dir().join(format!("ultimate-profiles-{}-{}",std::process::id(),NEXT_TEMP.fetch_add(1,Ordering::Relaxed)));fs::create_dir_all(&path).unwrap();Self(path)}}
    impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    fn rule(profile:&str,process:&str)->Rule{Rule{profile:profile.into(),process:process.into()}}
    #[test]
    fn matcher_precedence_case_and_exact_names(){
        let processes=[Process{comm:"Game".into(),exe:"launcher".into(),cmdline:"/bin/game --MODE Arena".into()}];
        assert_eq!(select(&[rule("first","ARENA"),rule("second","game")],&processes),Some("first"));
        assert_eq!(select(&[rule("exe","LAUNCHER")],&processes),Some("exe"));
        assert_eq!(select(&[rule("comm","gAmE")],&processes),Some("comm"));
        assert_eq!(select(&[rule("not-substring","aunch")],&processes),None);
        assert_eq!(select(&[rule("empty","")],&processes),None);
    }
    #[test]
    fn fake_proc_filters_uid_and_skips_unreadable_entries(){
        let temp=Temp::new();
        for(pid,uid)in [("1",1000),("2",2000)]{
            let path=temp.0.join(pid);fs::create_dir(&path).unwrap();fs::write(path.join("status"),format!("Name:\tgame\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\n")).unwrap();
            fs::write(path.join("comm"),format!("game{pid}\n")).unwrap();fs::write(path.join("cmdline"),b"/bin/game\0--arena\0").unwrap();
            std::os::unix::fs::symlink("/bin/executable",path.join("exe")).unwrap();
        }
        fs::create_dir(temp.0.join("3")).unwrap();fs::create_dir(temp.0.join("self")).unwrap();
        let processes=collect_processes(&temp.0,1000);assert_eq!(processes.len(),1);assert_eq!(processes[0].comm,"game1");assert_eq!(processes[0].exe,"executable");
        assert_eq!(select(&[rule("other-user","game2")],&processes),None);
    }
    #[test]
    fn migration_preserves_legacy_and_never_overwrites_named_profile(){
        let temp=Temp::new();let store=Store::new(temp.0.clone());let legacy=temp.0.join("host.json");
        fs::write(&legacy,b"{\"sensitivity\":23.0}\n").unwrap();store.initialize().unwrap();
        assert_eq!(fs::read(&legacy).unwrap(),fs::read(store.path("default").unwrap()).unwrap());
        fs::write(&legacy,b"{}\n").unwrap();store.initialize().unwrap();assert_eq!(store.load("default").unwrap().sensitivity,23.0);
    }
    #[test]
    fn default_cannot_be_deleted_and_rename_updates_rules(){
        let temp=Temp::new();let store=Store::new(temp.0.clone());store.initialize().unwrap();let mut config=AutoConfig::default();
        assert!(store.delete("default",&config).is_err());assert!(store.load("default").is_ok());
        config.rules.push(rule("default","game"));store.rename("default","renamed",&mut config).unwrap();assert_eq!(config.default,"renamed");assert_eq!(config.rules[0].profile,"renamed");assert!(store.load("renamed").is_ok());
        assert!(store.path("../escape").is_err());
    }
    #[test]
    fn debounce_requires_stable_change_and_ignores_unchanged(){
        let mut debounce=Debounce::default();let at=Duration::from_secs;
        assert!(!debounce.changed("a","a",at(0)));assert!(!debounce.changed("a","b",at(1)));assert!(!debounce.changed("a","b",at(2)));
        assert!(!debounce.changed("a","c",at(3)));assert!(!debounce.changed("a","b",at(4)));assert!(debounce.changed("a","b",at(6)));
        assert!(!debounce.changed("b","b",at(7)));assert!(!debounce.changed("b","a",at(8)));assert!(!debounce.changed("b","b",at(9)));
        assert!(!debounce.changed("b","a",at(10)));assert!(!debounce.changed("b","a",at(11)));assert!(debounce.changed("b","a",at(12)));
    }
}
