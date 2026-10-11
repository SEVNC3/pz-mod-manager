//! The folders PZMM works with: where each one is, whether it looks right, and moving PZMM's
//! data. Each comes from its environment variable, else the setting, else autodetect.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::{Paths, STEAM_APP};
use crate::settings::{self, Settings};
use crate::sources::steam_workshop;

/// (key, label, environment variable)
pub const FOLDERS: [(&str, &str, Option<&str>); 5] = [
    ("game", "Project Zomboid install", Some("PZ_GAME_DIR")),
    ("zomboid", "Zomboid user folder", Some("PZ_HOME")),
    ("server", "Dedicated server install", Some("PZ_SERVER_DIR")),
    ("workshop", "Steam Workshop folder", None),
    ("data", "PZMM data", Some("PZMM_DATA")),
];

fn env_var(key: &str) -> Option<&'static str> {
    FOLDERS.iter().find(|f| f.0 == key).and_then(|f| f.2)
}

/// Where the folder is with no environment variable or setting.
pub fn detect(key: &str) -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    match key {
        "game" => steam_workshop::library_path("steamapps/common/ProjectZomboid"),
        "zomboid" => Some(home.join("Zomboid")),
        "server" => steam_workshop::library_path("steamapps/common/Project Zomboid Dedicated Server"),
        "workshop" => steam_workshop::content_dir(STEAM_APP),
        "data" => Some(dirs::data_local_dir().unwrap_or(home).join("pzmm")),
        _ => None,
    }
}

/// The environment variable, then the setting, then autodetect.
pub fn resolve(env: Option<OsString>, setting: Option<&PathBuf>, detect: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    env.filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| setting.cloned()).or_else(detect)
}

/// Where PZMM looks for folder `key` now.
pub fn path(key: &str, s: &Settings) -> Option<PathBuf> {
    resolve(env_var(key).and_then(std::env::var_os), s.folders.get(key), || detect(key))
}

/// "found", "missing", or "not_pz" when the folder exists but isn't what it should be.
pub fn status(key: &str, p: &Path) -> &'static str {
    if !p.is_dir() {
        return "missing";
    }
    let has = |names: &[&str]| names.iter().any(|n| p.join(n).exists());
    let ok = match key {
        "game" => super::zombiebuddy::launcher_dir(p).is_some(),
        "zomboid" => has(&["mods", "Saves", "Server", "console.txt", "options.ini"]),
        "server" => has(&["StartServer64.bat", "ProjectZomboidServer.bat", "start-server.sh"]),
        "workshop" => {
            p.ends_with(STEAM_APP)
                || fs::read_dir(p).into_iter().flatten().flatten().any(|e| e.file_name().to_str().is_some_and(steam_workshop::valid_id))
        }
        _ => true,
    };
    if ok { "found" } else { "not_pz" }
}

/// Why `p` can't hold PZMM's data, if it can't.
pub fn refuse_data(p: &Path, game: Option<&Path>) -> Option<String> {
    if p.parent().is_none() {
        return Some("a drive root can't hold PZMM's data: pick a folder on it".into());
    }
    if game.is_some_and(|g| p.starts_with(g)) {
        return Some("PZMM's data can't go in the game folder: Steam deletes it when it verifies or reinstalls the game".into());
    }
    None
}

/// Free bytes on the drive holding `p`.
#[cfg(windows)]
fn free_space(p: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, free: *mut u64, total: *mut u64, total_free: *mut u64) -> i32;
    }
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0u64;
    // SAFETY: a NUL-terminated path and a valid out pointer; the other outputs are optional
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

// ponytail: low-space warning on Windows only; add statvfs (rustix is already in the tree) if Linux needs it
#[cfg(not(windows))]
fn free_space(_: &Path) -> Option<u64> {
    None
}

/// Problems with folder `key` at `p`: a refused data folder, OneDrive, no write permission, low disk space.
pub fn warnings(key: &str, p: &Path, s: &Settings) -> Vec<String> {
    let mut out = vec![];
    if key == "data"
        && let Some(why) = refuse_data(p, path("game", s).as_deref())
    {
        out.push(why);
    }
    if !p.is_dir() {
        return out;
    }
    if p.components().any(|c| c.as_os_str().to_string_lossy().to_lowercase().starts_with("onedrive")) {
        out.push("it's in OneDrive, which can lock or swap files while the game or PZMM uses them".into());
    }
    // the folders PZMM writes to (the game folder for ZombieBuddy)
    if matches!(key, "game" | "zomboid" | "data") && tempfile::tempfile_in(p).is_err() {
        out.push("PZMM can't write here (no permission)".into());
    }
    if matches!(key, "zomboid" | "data")
        && let Some(free) = free_space(p).filter(|f| *f < 2 << 30)
    {
        out.push(format!("only {:.1} GB free on this drive", free as f64 / f64::from(1 << 30)));
    }
    out
}

#[derive(Debug, serde::Serialize)]
pub struct Folder {
    pub key: &'static str,
    pub label: &'static str,
    pub env: Option<&'static str>,
    /// Where PZMM looks now.
    pub path: Option<PathBuf>,
    /// The environment variable is set, so it wins over the setting.
    pub overridden: bool,
    /// Set by hand in settings.json (else autodetected).
    pub custom: bool,
    pub detected: Option<PathBuf>,
    /// "found", "missing" or "not_pz".
    pub status: &'static str,
    pub warnings: Vec<String>,
}

pub fn list(s: &Settings) -> Vec<Folder> {
    FOLDERS
        .iter()
        .map(|&(key, label, env)| {
            let p = path(key, s);
            Folder {
                key,
                label,
                env,
                overridden: env.and_then(std::env::var_os).is_some_and(|v| !v.is_empty()),
                custom: s.folders.contains_key(key),
                detected: detect(key),
                status: p.as_deref().map_or("missing", |p| status(key, p)),
                warnings: p.as_deref().map(|p| warnings(key, p, s)).unwrap_or_default(),
                path: p,
            }
        })
        .collect()
}

/// Save folder `key` (`None`: back to autodetect). For PZMM data with `move_data`, the current
/// data folder moves there first.
pub fn set(key: &str, to: Option<PathBuf>, move_data: bool) -> Result<()> {
    if !FOLDERS.iter().any(|f| f.0 == key) {
        bail!("unknown folder {key:?}");
    }
    let mut s = settings::load();
    if let Some(p) = &to {
        if !p.is_absolute() {
            bail!("{} isn't a full path (like D:\\Games\\Zomboid)", p.display());
        }
        if key == "data"
            && let Some(why) = refuse_data(p, path("game", &s).as_deref())
        {
            bail!("{why}");
        }
    }
    if key == "data" && move_data {
        let from = Paths::detect().data;
        if let Some(target) = to.clone().or_else(|| detect("data"))
            && target != from
            && from.exists()
        {
            move_data_dir(&from, &target)?;
        }
    }
    match to {
        Some(p) => s.folders.insert(key.into(), p),
        None => s.folders.remove(key),
    };
    settings::save(&s)
}

/// Move PZMM's data folder to `to`, which must be missing or empty. Across drives it's copied
/// first and the old folder is deleted only once the copy is complete.
pub fn move_data_dir(from: &Path, to: &Path) -> Result<()> {
    if to.starts_with(from) {
        bail!("can't move PZMM's data into a folder inside it");
    }
    if to.exists() {
        if fs::read_dir(to)?.next().is_some() {
            bail!("{} isn't empty: pick an empty or new folder", to.display());
        }
        fs::remove_dir(to)?;
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            if let Err(e) = crate::store::copy_dir(from, to) {
                let _ = fs::remove_dir_all(to); // only the partial copy: `to` was empty
                return Err(e).with_context(|| format!("copying PZMM's data to {} (nothing was moved)", to.display()));
            }
            if let Err(e) = fs::remove_dir_all(from) {
                crate::say!("dataCopied", "Copied PZMM's data to {to}, but couldn't delete the old folder {from} ({error}). Delete it yourself.", to = to.display(), from = from.display(), error = e);
            }
        }
        Err(e) => return Err(e).with_context(|| format!("moving {} (close the game and any PZMM command line, then try again)", from.display())),
    }
    crate::say!("dataMoved", "Moved PZMM's data to {to}", to = to.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_beats_setting_beats_detect() {
        let set = PathBuf::from("D:/set");
        let detected = || Some(PathBuf::from("C:/detected"));
        assert_eq!(resolve(Some("E:/env".into()), Some(&set), detected), Some("E:/env".into()));
        assert_eq!(resolve(None, Some(&set), detected), Some(set.clone()));
        assert_eq!(resolve(Some("".into()), Some(&set), detected), Some(set), "an empty variable doesn't count");
        assert_eq!(resolve(None, None, detected), Some("C:/detected".into()));
        assert_eq!(resolve(None, None, || None), None);
    }

    #[test]
    fn validates_paths() {
        let t = tempfile::tempdir().unwrap();
        let game = t.path().join("ProjectZomboid");
        fs::create_dir_all(&game).unwrap();
        assert_eq!(status("game", &game), "not_pz");
        fs::write(game.join("ProjectZomboid64.json"), "{}").unwrap();
        assert_eq!(status("game", &game), "found");
        assert_eq!(status("zomboid", &t.path().join("nope")), "missing");
        assert_eq!(status("zomboid", &game), "not_pz");
        fs::create_dir_all(t.path().join("ws/3619862853")).unwrap();
        assert_eq!(status("workshop", &t.path().join("ws")), "found");

        let root = if cfg!(windows) { Path::new("C:\\") } else { Path::new("/") };
        assert!(refuse_data(root, None).is_some());
        assert!(refuse_data(&game.join("pzmm"), Some(&game)).is_some());
        assert!(refuse_data(&t.path().join("pzmm"), Some(&game)).is_none());

        let od = t.path().join("OneDrive - Work");
        fs::create_dir_all(&od).unwrap();
        let w = warnings("zomboid", &od, &Settings::default());
        assert!(w.iter().any(|w| w.contains("OneDrive")), "{w:?}");
        let w = warnings("zomboid", &game, &Settings::default());
        assert!(!w.iter().any(|w| w.contains("OneDrive") || w.contains("permission")), "{w:?}"); // free space depends on the machine
    }

    #[test]
    fn moves_data_only_into_an_empty_folder() {
        let t = tempfile::tempdir().unwrap();
        let from = t.path().join("old");
        fs::create_dir_all(from.join("profiles/Default")).unwrap();
        fs::write(from.join("installed.json"), "{}").unwrap();

        let busy = t.path().join("busy");
        fs::create_dir_all(&busy).unwrap();
        fs::write(busy.join("x"), "").unwrap();
        assert!(move_data_dir(&from, &busy).is_err());
        assert!(move_data_dir(&from, &from.join("inner")).is_err());
        assert!(from.join("installed.json").exists(), "nothing moved");

        let empty = t.path().join("new");
        fs::create_dir_all(&empty).unwrap();
        move_data_dir(&from, &empty).unwrap();
        assert!(empty.join("installed.json").exists() && empty.join("profiles/Default").is_dir() && !from.exists());
    }
}
