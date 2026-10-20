//! Optional, user-owned artwork. No original artwork is embedded or redistributed.
use super::{clay::{Size, Style, Texture}, widgets::Ui};
use crate::{Result, invalid};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, io::Write, path::{Path, PathBuf}};

pub(super) const FRONT: &str = "XboxWired/mapping/Mapping_bg_L7.png";
pub(super) const BACK: &str = "XboxWired/mapping/Mapping_back_L7.png";
pub(super) const TEST: &str = "XboxWired/Test/Test_L7.png";
pub(super) const STICKS: &str = "XboxWired/sticks/Sticks_bg_L7.png";
pub(super) const TRIGGERS: &str = "XboxWired/Triggers/Triggers_bg_L7.png";
pub(super) const VIBRATION: &str = "XboxWired/vibration/Vibration_bg_L7.png";

fn valid(root: &Path) -> bool {
    root.join("Xbox3E").is_dir() && root.join("XboxWired/Test").is_dir()
}

fn resolve(cli: Option<&Path>, environment: Option<&Path>, saved: Option<&Path>, probes: &[PathBuf]) -> Option<PathBuf> {
    cli.into_iter().chain(environment).chain(saved).chain(probes.iter().map(PathBuf::as_path))
        .find(|root| valid(root)).map(Path::to_path_buf)
}

fn probes() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(executable) = std::env::current_exe() {
            for ancestor in executable.ancestors().skip(1) {
                paths.push(ancestor.join("reconstruction/original/Assets"));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".local/share/ultimate-control/Assets"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        if let Ok(entries) = fs::read_dir(program_files.join("WindowsApps")) {
            let mut packages: Vec<_> = entries.flatten().filter(|entry| {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                name.contains("8bitdo") && name.contains("ultimatesoftwarex")
            }).map(|entry| entry.path().join("Assets")).collect();
            packages.sort();
            paths.extend(packages);
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            paths.push(PathBuf::from(local).join("ultimate-control/Assets"));
        }
    }
    paths
}

#[derive(Default, Deserialize, Serialize)]
struct Setting { root: Option<PathBuf> }
fn setting_path() -> PathBuf {
    crate::ui::default_profile_path().with_file_name("assets.json")
}
fn saved(path: &Path) -> Option<PathBuf> {
    serde_json::from_slice::<Setting>(&fs::read(path).ok()?).ok()?.root
}
fn save(path: &Path, root: Option<PathBuf>) -> Result<()> {
    if root.as_deref().is_some_and(|root| !valid(root)) {
        return Err(invalid("Artwork root must contain Xbox3E/ and XboxWired/Test/"));
    }
    let root = root.map(fs::canonicalize).transpose()?;
    let parent = path.parent().ok_or_else(|| invalid("Missing artwork settings directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".assets-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&serde_json::to_vec_pretty(&Setting { root })?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(temporary); }
    result
}

#[derive(Default)]
pub(super) struct Artwork {
    pub root: Option<PathBuf>,
    // Failed reads/decodes are cached too: missing artwork never causes per-frame I/O.
    textures: HashMap<&'static str, Option<Texture>>,
}
impl Artwork {
    pub fn discover(cli: Option<&Path>) -> Self {
        let environment = std::env::var_os("ULTIMATE_ASSETS").map(PathBuf::from);
        Self { root: resolve(cli, environment.as_deref(), saved(&setting_path()).as_deref(), &probes()), ..Self::default() }
    }
    pub fn save_root(&mut self, value: &str, cli: Option<&Path>) -> Result<()> {
        let root = (!value.trim().is_empty()).then(|| PathBuf::from(value));
        save(&setting_path(), root)?;
        let next = Self::discover(cli);
        if self.root != next.root { *self = next; }
        Ok(())
    }
    pub fn texture(&mut self, name: &'static str) -> Option<&Texture> {
        let root = self.root.as_ref()?;
        self.textures.entry(name).or_insert_with(|| {
            let bytes = fs::read(root.join(name)).ok()?;
            Texture::png(&bytes).ok()
        }).as_ref()
    }
    pub fn image(&mut self, ui: &mut Ui<'_, '_>, name: &'static str, index: u32, width: f32, height: f32) -> bool {
        let Some(texture) = self.texture(name) else { return false };
        let [width, height] = texture.fit(width, height);
        ui.frame.image_index("original-art", index, Style::row(Size::Fixed(width), Size::Fixed(height)), texture);
        true
    }
}

pub(super) struct MappingArt { pub normal: &'static str, pub change: &'static str, pub default: &'static str }
macro_rules! mapping {
    ($name:literal) => { MappingArt {
        normal: concat!("Xbox3E/Mapping_", $name, ".png"),
        change: concat!("Xbox3E/Mapping_", $name, "_Change.png"),
        default: concat!("Xbox3E/Mapping_", $name, "_Default.png"),
    } };
}
// PhysicalButton::ALL order; P1/P2 are the original app's PL/PR labels.
pub(super) const MAPPING: [MappingArt; 20] = [
    mapping!("L3"), mapping!("R3"), mapping!("L4"), mapping!("R4"),
    mapping!("Up"), mapping!("Down"), mapping!("Left"), mapping!("Right"),
    mapping!("A"), mapping!("B"), mapping!("X"), mapping!("Y"),
    mapping!("L1"), mapping!("R1"), mapping!("L2"), mapping!("R2"),
    mapping!("PL"), mapping!("PR"), mapping!("Select"), mapping!("Start"),
];
pub(super) const NULL_MAPPING: MappingArt = mapping!("Null");
pub(super) fn destination(target: crate::profile::MappingTarget) -> &'static MappingArt {
    use crate::profile::{MappingTarget as T, PhysicalButton as B};
    let source = match target {
        T::A => B::A, T::B => B::B, T::X => B::X, T::Y => B::Y,
        T::Lb => B::Lb, T::Rb => B::Rb, T::Lt => B::Lt, T::Rt => B::Rt,
        T::L3 => B::L3, T::R3 => B::R3, T::View => B::View, T::Menu => B::Menu,
        T::Up => B::Up, T::Down => B::Down, T::Left => B::Left, T::Right => B::Right,
        _ => return &NULL_MAPPING,
    };
    &MAPPING[source.index()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("ultimate-art-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn art(&self, name: &str) -> PathBuf {
            let root = self.0.join(name);
            fs::create_dir_all(root.join("Xbox3E")).unwrap();
            fs::create_dir_all(root.join("XboxWired/Test")).unwrap();
            root
        }
    }
    impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    #[test]
    fn resolution_precedence_and_validation() {
        let temp = Temp::new();
        let cli = temp.art("cli"); let env = temp.art("env");
        let saved = temp.art("saved"); let probe = temp.art("probe");
        let probes = [temp.0.join("missing"), probe.clone()];
        assert_eq!(resolve(Some(&cli), Some(&env), Some(&saved), &probes), Some(cli));
        assert_eq!(resolve(Some(&probes[0]), Some(&env), Some(&saved), &probes), Some(env.clone()));
        assert_eq!(resolve(None, Some(&probes[0]), Some(&saved), &probes), Some(saved.clone()));
        assert_eq!(resolve(None, Some(&env), Some(&saved), &probes), Some(env));
        assert_eq!(resolve(None, None, Some(&saved), &probes), Some(saved));
        assert_eq!(resolve(None, None, None, &probes), Some(probe));
        assert!(!valid(&temp.0));
        fs::create_dir_all(temp.0.join("partial/Xbox3E")).unwrap();
        assert!(!valid(&temp.0.join("partial")));
        assert_eq!(resolve(Some(&temp.0.join("partial")), None, None, &[]), None);
        assert_eq!(resolve(Some(&temp.0), None, None, &probes), Some(probes[1].clone()));
    }
    #[test]
    fn settings_round_trip_and_missing_artwork_fallback() {
        let temp = Temp::new(); let root = fs::canonicalize(temp.art("art")).unwrap(); let config = temp.0.join("config/assets.json");
        assert_eq!(saved(&config), None);
        save(&config, Some(root.clone())).unwrap();
        assert_eq!(saved(&config), Some(root.clone()));
        assert!(save(&config, Some(temp.0.join("missing"))).is_err());
        assert_eq!(saved(&config), Some(root.clone()));
        save(&config, None).unwrap(); assert_eq!(saved(&config), None);
        let mut art = Artwork { root: Some(root), ..Artwork::default() };
        assert!(art.texture(FRONT).is_none()); assert!(art.texture(FRONT).is_none());
        assert_eq!(art.textures.len(), 1);
        assert!(Artwork::default().texture(FRONT).is_none());
        fs::write(&config, b"not json").unwrap(); assert_eq!(saved(&config), None);
    }
}
