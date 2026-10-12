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
const FOLDERS: [(&str, &str, Option<&str>); 6] = [
    ("game", "Project Zomboid install", Some("PZ_GAME_DIR")),
    ("zomboid", "Zomboid user folder", Some("PZ_HOME")),
    ("server", "Dedicated server install", Some("PZ_SERVER_DIR")),
    ("workshop", "Steam Workshop folder", None),
    ("data", "PZMM data", Some("PZMM_DATA")),
    ("saves", "Save backups", None),
];

fn env_var(key: &str) -> Option<&'static str> {
    FOLDERS.iter().find(|f| f.0 == key).and_then(|f| f.2)
}

/// Where the folder is with no environment variable or setting (none in unit tests, which must
/// never touch the real folders).
pub fn detect(key: &str) -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let home = dirs::home_dir().unwrap_or_default();
    match key {
        "game" => steam_workshop::library_path("steamapps/common/ProjectZomboid").or_else(gog_dir),
        "zomboid" => {
            let z = home.join("Zomboid");
            // the Windows game under Proton keeps it in its Wine prefix
            let proton = || steam_workshop::library_path("steamapps/compatdata/108600/pfx/drive_c/users/steamuser/Zomboid");
            Some(if cfg!(windows) || z.is_dir() { z } else { proton().unwrap_or(z) })
        }
        "server" => steam_workshop::library_path("steamapps/common/Project Zomboid Dedicated Server"),
        "workshop" => steam_workshop::content_dir(STEAM_APP),
        "data" => Some(dirs::data_local_dir().unwrap_or(home).join("pzmm")),
        "saves" => path("data", &settings::load()).map(|d| d.join("save-backups")),
        _ => None,
    }
}

/// A GOG copy of the game: where GOG Galaxy's installer recorded it, else GOG's usual folders.
fn gog_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};
        if let Ok(games) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games") {
            for key in games.enum_keys().flatten().filter_map(|k| games.open_subkey(k).ok()) {
                let name: String = key.get_value("gameName").unwrap_or_default();
                if let Ok(path) = key.get_value::<String, _>("path")
                    && name.to_ascii_lowercase().contains("project zomboid")
                    && Path::new(&path).is_dir()
                {
                    return Some(path.into());
                }
            }
        }
    }
    let home = dirs::home_dir().unwrap_or_default();
    [PathBuf::from(r"C:\GOG Games\Project Zomboid"), PathBuf::from(r"C:\Program Files (x86)\GOG Galaxy\Games\Project Zomboid"), home.join("GOG Games/Project Zomboid")]
        .into_iter()
        .find(|p| p.is_dir())
}

/// Whether the game folder is Steam's copy (started through Steam), not GOG's or another one
/// started by its own launcher.
pub fn steam_copy(game: &Path) -> bool {
    game.components().any(|c| c.as_os_str().eq_ignore_ascii_case("steamapps")) || game.join("steam_appid.txt").is_file()
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
    // the save backups folder is made with the first backup
    if !p.is_dir() && key != "saves" {
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

/// Why `p` can't hold PZMM's data, if it can't (a `keyed!` error).
fn refuse_data(p: &Path, game: Option<&Path>) -> Option<String> {
    if p.parent().is_none() {
        return Some(crate::keyed!("dataDriveRoot", "a drive root can't hold PZMM's data: pick a folder on it"));
    }
    if game.is_some_and(|g| p.starts_with(g)) {
        return Some(crate::keyed!("dataInGame", "PZMM's data can't go in the game folder: Steam deletes it when it verifies or reinstalls the game"));
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
        out.push(crate::unkeyed(&why).to_string());
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
            bail!(crate::keyed!("notFullPath", "{path} isn't a full path (like D:\\Games\\Zomboid)", path = p.display()));
        }
        if key == "data"
            && let Some(why) = refuse_data(p, path("game", &s).as_deref())
        {
            bail!("{why}");
        }
    }
    let target = if key == "data" && move_data { to.clone().or_else(|| detect("data")) } else { None };
    let save = move || {
        match to {
            Some(p) => s.folders.insert(key.into(), p),
            None => s.folders.remove(key),
        };
        settings::save(&s)
    };
    let paths = Paths::detect();
    if let Some(target) = target
        && target != paths.data
        && paths.data.exists()
    {
        // a running server's files (its install, its own data folder) may be in there
        if let Some(s) = super::host::load(&paths).iter().find(|s| !matches!(super::host::state(s).0, super::host::Status::Stopped | super::host::Status::Crashed)) {
            bail!(crate::keyed!("dataServerRuns", "stop {name} first: PZMM's data can't move while a server it manages runs", name = s.name));
        }
        return move_data_dir(&paths.data, &target, save);
    }
    save()
}

/// Move PZMM's data folder to `to`, which must be missing or empty, then `switch` PZMM over to it.
/// Across drives it's copied under the old folder's data lock (so no write lands in the old
/// folder mid-copy), PZMM switches before the lock is let go, and the old folder is deleted only
/// once the copy is complete.
fn move_data_dir(from: &Path, to: &Path, switch: impl FnOnce() -> Result<()>) -> Result<()> {
    if to.starts_with(from) {
        bail!("can't move PZMM's data into a folder inside it");
    }
    // a server's automation round under way would write to the old folder (the app starts no new ones meanwhile)
    super::autopilot::wait_rounds();
    if to.exists() {
        if fs::read_dir(to)?.next().is_some() {
            bail!("{} isn't empty: pick an empty or new folder", to.display());
        }
        fs::remove_dir(to)?;
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    // a profile's own game folder is kept by its short name when its path has spaces: read while it exists
    let short_from = crate::profile::path_form(from, false).filter(|s| s != from);
    match fs::rename(from, to) {
        Ok(()) => switch()?,
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            let lock = crate::lock_dir(from)?;
            if let Err(e) = crate::store::copy_dir(from, to).and_then(|()| switch()) {
                let _ = fs::remove_dir_all(to); // only the partial copy: `to` was empty
                return Err(e).with_context(|| format!("copying PZMM's data to {} (nothing was moved)", to.display()));
            }
            drop(lock);
            if let Err(e) = fs::remove_dir_all(from) {
                crate::say!("dataCopied", "Copied PZMM's data to {to}, but couldn't delete the old folder {from} ({error}). Delete it yourself.", to = to.display(), from = from.display(), error = e);
            }
        }
        Err(e) => return Err(e).with_context(|| format!("moving {} (close the game and any PZMM command line, then try again)", from.display())),
    }
    if let Err(e) = repoint(&[Some(from), short_from.as_deref()].into_iter().flatten().collect::<Vec<_>>(), to) {
        crate::say!("dataRepointFailed", "error: couldn't point the servers and profiles at {to} ({error}): check their folders in Servers and the profiles' launch options", to = to.display(), error = format!("{e:#}"));
    }
    crate::say!("dataMoved", "Moved PZMM's data to {to}", to = to.display());
    Ok(())
}

/// After a move: the paths PZMM keeps that pointed into the old data folder (`from`: its long and
/// short forms) point into the new one: a server's install and own data folder in servers.json, a
/// profile's own game folder in its launch.json.
fn repoint(from: &[&Path], to: &Path) -> Result<()> {
    let _lock = crate::lock_dir(to)?;
    let profiles = fs::read_dir(to.join("profiles")).into_iter().flatten().flatten().map(|e| e.path().join("launch.json"));
    for file in std::iter::once(to.join("servers.json")).chain(profiles) {
        let text = match fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
        };
        let mut v: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("reading {}", file.display()))?;
        if repoint_json(&mut v, from, to) {
            crate::write_atomic(&file, serde_json::to_string_pretty(&v)?)?;
        }
    }
    Ok(())
}

/// Point the `install_dir`, `cachedir` and `cache_dir` paths in `v` that are under one of `from`
/// under `to` instead; whether any changed.
fn repoint_json(v: &mut serde_json::Value, from: &[&Path], to: &Path) -> bool {
    match v {
        serde_json::Value::Array(a) => a.iter_mut().fold(false, |changed, v| repoint_json(v, from, to) | changed),
        serde_json::Value::Object(o) => {
            let mut changed = false;
            for (k, v) in o.iter_mut().filter(|(k, _)| ["install_dir", "cachedir", "cache_dir"].contains(&k.as_str())) {
                let Some(mut new) = v.as_str().and_then(|p| from.iter().find_map(|f| Path::new(p).strip_prefix(f).ok())).map(|rest| to.join(rest)) else { continue };
                if k == "cache_dir" {
                    // a short name from the old place: long again, then short if it has spaces (as profile::set_launch keeps it)
                    new = crate::profile::path_form(&new, true).unwrap_or(new);
                    let spaced = |p: &Path| p.to_string_lossy().contains(char::is_whitespace);
                    if let Some(short) = crate::profile::path_form(&new, false).filter(|s| spaced(&new) && !spaced(s)) {
                        new = short;
                    }
                }
                *v = new.to_string_lossy().into_owned().into();
                changed = true;
            }
            changed
        }
        _ => false,
    }
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
    fn tests_never_find_the_real_folders() {
        for f in list(&Settings::default()) {
            assert_eq!(f.detected, None, "{}", f.key);
            if !f.overridden {
                assert_eq!(f.path, None, "{} would be probed", f.key);
            }
        }
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
        assert!(move_data_dir(&from, &busy, || Ok(())).is_err());
        assert!(move_data_dir(&from, &from.join("inner"), || Ok(())).is_err());
        assert!(from.join("installed.json").exists(), "nothing moved");

        let empty = t.path().join("new");
        fs::create_dir_all(&empty).unwrap();
        move_data_dir(&from, &empty, || Ok(())).unwrap();
        assert!(empty.join("installed.json").exists() && empty.join("profiles/Default").is_dir() && !from.exists());
    }

    #[test]
    fn a_move_repoints_servers_and_profiles() {
        let t = tempfile::tempdir().unwrap();
        let (from, to, elsewhere) = (t.path().join("old"), t.path().join("new"), t.path().join("D").join("srv"));
        fs::create_dir_all(from.join("profiles/Own")).unwrap();
        let servers = serde_json::json!([
            { "name": "A", "install_dir": from.join("servers/b42"), "cachedir": from.join("server-data/A"), "auto": { "backup_keep": 3 } },
            { "name": "B", "install_dir": elsewhere, "cachedir": null },
        ]);
        fs::write(from.join("servers.json"), servers.to_string()).unwrap();
        fs::write(from.join("profiles/Own/launch.json"), serde_json::json!({ "args": "-debug", "cache_dir": from.join("game-folders/Own") }).to_string()).unwrap();
        move_data_dir(&from, &to, || Ok(())).unwrap();
        let read = |f: &str| serde_json::from_str::<serde_json::Value>(&fs::read_to_string(to.join(f)).unwrap()).unwrap();
        let s = read("servers.json");
        let path = |v: &serde_json::Value| PathBuf::from(v.as_str().unwrap());
        assert_eq!((path(&s[0]["install_dir"]), path(&s[0]["cachedir"])), (to.join("servers/b42"), to.join("server-data/A")));
        assert_eq!((path(&s[1]["install_dir"]), &s[1]["cachedir"], &s[0]["auto"]["backup_keep"]), (elsewhere, &serde_json::Value::Null, &serde_json::json!(3)), "the rest stays");
        let l = read("profiles/Own/launch.json");
        assert_eq!((path(&l["cache_dir"]), l["args"].as_str()), (to.join("game-folders/Own"), Some("-debug")));
    }

    #[test]
    fn a_move_repoints_a_game_folder_kept_by_its_short_name() {
        // a user name with a space: the profile's own game folder is saved in its 8.3 form
        let t = tempfile::tempdir().unwrap();
        let (from, to) = (t.path().join("old data"), t.path().join("new data"));
        let own = from.join("game-folders/Own");
        fs::create_dir_all(&own).unwrap();
        fs::create_dir_all(from.join("profiles/Own")).unwrap();
        let short = |p: &Path| crate::profile::path_form(p, false).unwrap_or_else(|| p.to_path_buf());
        fs::write(from.join("profiles/Own/launch.json"), serde_json::json!({ "cache_dir": short(&own) }).to_string()).unwrap();
        move_data_dir(&from, &to, || Ok(())).unwrap();
        let l: serde_json::Value = serde_json::from_str(&fs::read_to_string(to.join("profiles/Own/launch.json")).unwrap()).unwrap();
        let moved = PathBuf::from(l["cache_dir"].as_str().unwrap());
        let long = |p: PathBuf| crate::profile::path_form(&p, true).unwrap_or(p); // the temp dir may itself be an 8.3 path (CI)
        assert_eq!(long(moved), long(to.join("game-folders").join("Own")));
    }

    #[test]
    fn warnings_keep_the_wording_settings_matches() {
        // Settings.svelte's WARNINGS finds these by their English: a rewording here must change it there too
        let refused = |p: &Path, game: Option<&Path>| crate::unkeyed(&refuse_data(p, game).unwrap()).to_string();
        assert!(refused(Path::new("/"), None).starts_with("a drive root"));
        let t = tempfile::tempdir().unwrap();
        assert!(refused(&t.path().join("game/data"), Some(&t.path().join("game"))).contains("in the game folder"));
        let one = t.path().join("OneDrive");
        fs::create_dir_all(&one).unwrap();
        assert!(warnings("saves", &one, &Settings::default()).iter().any(|w| w.contains("OneDrive")));
        // the two this test can't bring about, as written
        let src = include_str!("folders.rs");
        assert!(src.contains(concat!("\"PZMM can't write here (", "no permission)\"")) && src.contains(concat!("format!(\"only {:.1} GB", " free")));
    }

    #[test]
    fn copies_data_while_its_lock_is_held() {
        // a move across drives copies the old folder with its pzmm.lock held
        let t = tempfile::tempdir().unwrap();
        let from = t.path().join("old");
        fs::create_dir_all(&from).unwrap();
        fs::write(from.join("servers.json"), "[]").unwrap();
        let _lock = crate::lock_dir(&from).unwrap();
        crate::store::copy_dir(&from, &t.path().join("new")).unwrap();
        assert!(t.path().join("new/servers.json").exists());
    }
}
