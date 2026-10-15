//! PZMM's settings: `settings.json` in the OS config folder (`%APPDATA%\pzmm` on Windows),
//! read by the app and the CLI alike. The GitHub token lives in the OS credential store, never here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// settings.json couldn't be read: an empty stand-in, never saved over it.
    #[serde(skip)]
    pub unread: bool,
    /// Folders set by hand, by key (see `pz::folders`); a missing key means autodetect.
    pub folders: BTreeMap<String, PathBuf>,
    /// When to look for mod updates: "off", "start", "1h" or "3h" (on start, then every N hours).
    pub update_check: String,
    /// What to do with updates found: "notify" or "install".
    pub update_action: String,
    /// Look for a new PZMM on start.
    pub app_updates: bool,
    /// Workshop pages open in "steam" (the Steam client, when it's running) or the "browser".
    pub workshop_links: String,
    /// Removed mods go to the Recycle Bin instead of being deleted.
    pub recycle: bool,
    /// Removing a mod also removes required mods nothing else uses: "ask", "always" or "never".
    pub remove_requirements: String,
    /// Backups (previous versions) may use at most this many MB in total, oldest pruned first; 0 = no cap.
    pub backup_cap_mb: u64,
    /// Extra arguments for the game, e.g. `-debug -nosteam`.
    pub launch_args: String,
    /// Autosort a new profile's library the first time it's used.
    pub autosort_new: bool,
    /// Favourites go on and hidden mods off in every profile made or switched to.
    pub keep_marks: bool,
    /// Back up the last played save when PZMM starts the game.
    pub save_backup_launch: bool,
    /// Back up the saves played when a game PZMM started closes.
    pub save_backup_exit: bool,
    /// Automatic backups kept per save (named slots are never removed).
    pub save_backup_keep: u32,
    /// Closing the window keeps PZMM running in the tray, so server schedules, update checks and
    /// crash watch go on; the tray's Quit ends it.
    pub tray: bool,
    /// Closing the window asks whether to quit or go to the tray (false: `tray` decides). Unset (a
    /// settings file from before it): asks, unless `tray` already said closing goes to the tray.
    pub close_ask: Option<bool>,
    /// While a game PZMM started runs, back up the saves played every this many minutes (0: off).
    pub save_backup_every: u32,
    /// The server browser shows countries, asking api.country.is with the servers' addresses (see `pz::geoip`).
    pub countries: bool,
    /// Start PZMM, minimised to the tray, when the player signs in (Windows) or logs in (Linux).
    pub autostart: bool,
    /// Desktop notifications while PZMM's window isn't in front: "crash" (a server crashed),
    /// "restart" (a server's mods updated: it needs a restart), "downloads" (installs and
    /// updates finished) and "favourites" (a favourite server came online with room).
    pub notify: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            unread: false,
            folders: BTreeMap::new(),
            update_check: "start".into(),
            update_action: "notify".into(),
            app_updates: true,
            workshop_links: "steam".into(),
            recycle: true,
            remove_requirements: "ask".into(),
            backup_cap_mb: 0,
            launch_args: String::new(),
            autosort_new: true,
            keep_marks: false,
            save_backup_launch: false,
            save_backup_exit: true,
            save_backup_keep: 5,
            tray: false,
            close_ask: None,
            save_backup_every: 0,
            countries: false,
            autostart: false,
            notify: ["crash", "restart", "downloads"].map(String::from).to_vec(),
        }
    }
}

/// `<config>/pzmm/settings.json`. `APPDATA` comes first so a scratch config can stand in for testing.
pub fn path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).or_else(dirs::config_dir).unwrap_or_default().join("pzmm").join("settings.json")
}

/// The saved settings, or the defaults. Unit tests always get the defaults, never the real file.
pub fn load() -> Settings {
    if cfg!(test) { Settings::default() } else { load_from(&path()) }
}

fn load_from(path: &Path) -> Settings {
    // a damaged file is kept: the next save would otherwise replace it with defaults unseen. One that can't be
    // read now: the last read stands in (or the defaults, marked so they're never saved over it)
    static LAST: std::sync::Mutex<Option<Settings>> = std::sync::Mutex::new(None);
    match crate::read_json::<Settings>(path) {
        Ok(s) => {
            *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(s.clone());
            s
        }
        Err(e) => {
            crate::log!("error: {e:#}");
            LAST.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or(Settings { unread: true, ..Default::default() })
        }
    }
}

pub fn save(s: &Settings) -> Result<()> {
    save_to(&path(), s)
}

fn save_to(path: &Path, s: &Settings) -> Result<()> {
    if s.unread {
        anyhow::bail!("couldn't read {}, so it wasn't saved over: try again", path.display());
    }
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    crate::write_atomic(path, serde_json::to_string_pretty(s)?).with_context(|| format!("saving {}", path.display()))
}

// --- secrets (the GitHub token, the Steam Web API key, server passwords), kept in the OS credential store ---

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new("pzmm", name).map_err(|e| anyhow::anyhow!("the system's credential store isn't available: {e}"))
}

/// The Steam Web API key (it lists servers).
pub const STEAM_KEY: &str = "steam-web-api-key";

/// A server's remembered join password, by its `ip:port`.
pub fn server_password(addr: &str) -> String {
    format!("server-password:{addr}")
}

/// A saved secret: the credential store's, else (only when the store has none) the one in
/// [`secrets_file`]. Unit tests never read the credential store.
pub fn secret(name: &str) -> Option<String> {
    if cfg!(test) {
        return None;
    }
    entry(name).ok().and_then(|e| e.get_password().ok()).or_else(|| file_secret(&secrets_file(), name)).filter(|t| !t.trim().is_empty())
}

/// Save a secret as it is (spaces and all), or forget it when `value` is blank. When the
/// credential store can't take it (a Linux host with no Secret Service) it goes in [`secrets_file`].
pub fn set_secret(name: &str, value: &str) -> Result<()> {
    let value = if value.trim().is_empty() { "" } else { value };
    let stored = entry(name).and_then(|e| match value {
        "" => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(anyhow::anyhow!("forgetting it in the credential store: {err}")),
        },
        v => e.set_password(v).map_err(|err| anyhow::anyhow!("saving it in the credential store: {err}")),
    });
    match stored {
        // in the store: a copy the file kept from when the store failed goes (a forgotten one too)
        Ok(()) => set_file_secret(&secrets_file(), name, "").context("forgetting it in secrets.json"),
        Err(e) => {
            static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !value.is_empty() && !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                crate::say!("secretsFile", "The system's credential store isn't available ({error}), so PZMM keeps passwords and keys in {file}", error = format!("{e:#}"), file = secrets_file().display());
            }
            set_file_secret(&secrets_file(), name, value).with_context(|| format!("{e:#}"))
        }
    }
}

/// Where secrets go when the credential store isn't there: `<PZMM data>/secrets.json`, readable by
/// this user only.
fn secrets_file() -> PathBuf {
    crate::pz::Paths::detect().data.join("secrets.json")
}

fn file_secret(file: &Path, name: &str) -> Option<String> {
    crate::read_json::<BTreeMap<String, String>>(file).ok()?.remove(name)
}

fn set_file_secret(file: &Path, name: &str, value: &str) -> Result<()> {
    // nothing to forget: no data lock taken (a caller may hold it)
    if value.is_empty() && file_secret(file, name).is_none() {
        return Ok(());
    }
    let dir = file.parent().filter(|d| d.is_absolute()).context("PZMM has no data folder")?;
    let _lock = crate::lock_dir(dir)?;
    let mut all: BTreeMap<String, String> = crate::read_json(file)?;
    let changed = if value.is_empty() { all.remove(name).is_some() } else { all.insert(name.into(), value.into()).as_deref() != Some(value) };
    if !changed {
        return Ok(());
    }
    // made for this user alone before anything is written to it, then swapped in
    let tmp = file.with_extension("json.pzmm-new");
    let _ = fs::remove_file(&tmp);
    let mut o = fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    std::io::Write::write_all(&mut o.open(&tmp)?, serde_json::to_string_pretty(&all)?.as_bytes())?;
    fs::rename(&tmp, file)?;
    Ok(())
}

/// `GITHUB_TOKEN`, else the saved token (it raises GitHub's rate limit).
pub fn github_token() -> Option<String> {
    pick_token(std::env::var("GITHUB_TOKEN").ok(), || secret("github-token"))
}

fn pick_token(env: Option<String>, saved: impl FnOnce() -> Option<String>) -> Option<String> {
    env.filter(|t| !t.trim().is_empty()).or_else(saved).filter(|t| !t.trim().is_empty())
}

/// Whether a GitHub token is saved in the credential store.
pub fn token_saved() -> bool {
    secret("github-token").is_some()
}

/// Save the GitHub token, or forget it when `token` is empty.
pub fn set_github_token(token: &str) -> Result<()> {
    set_secret("github-token", token.trim()).context("the GitHub token")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_missing_and_corrupt_files() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("pzmm/settings.json");
        assert_eq!(load_from(&p), Settings::default(), "missing file: defaults");

        let mut s = Settings { recycle: false, launch_args: "-debug".into(), ..Default::default() };
        s.folders.insert("zomboid".into(), "D:/Zomboid".into());
        save_to(&p, &s).unwrap();
        assert_eq!(load_from(&p), s);

        // fields added later fall back to their defaults
        fs::write(&p, r#"{"recycle": false}"#).unwrap();
        assert_eq!(load_from(&p), Settings { recycle: false, ..Default::default() });

        fs::write(&p, "{ not json").unwrap();
        assert_eq!(load_from(&p), Settings::default());
        assert_eq!(fs::read_to_string(p.with_extension("json.bad")).unwrap(), "{ not json", "the damaged file is kept");
        assert_eq!(fs::read_to_string(&p).unwrap(), "{ not json", "and not overwritten by reading");
    }

    #[test]
    fn secrets_fall_back_to_a_private_file() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("secrets.json");
        assert_eq!(file_secret(&f, "a"), None);
        set_file_secret(&f, "a", " pw with spaces ").unwrap();
        set_file_secret(&f, "b", "x").unwrap();
        assert_eq!(file_secret(&f, "a").as_deref(), Some(" pw with spaces "), "kept as typed");
        set_file_secret(&f, "a", "").unwrap();
        assert_eq!((file_secret(&f, "a"), file_secret(&f, "b").as_deref()), (None, Some("x")));
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&f).unwrap().permissions()) & 0o777, 0o600);
        assert!(set_file_secret(Path::new("secrets.json"), "a", "x").is_err(), "never next to wherever PZMM runs");
        assert!(set_file_secret(Path::new("secrets.json"), "a", "").is_ok(), "nothing to forget: nothing locked or written");
    }

    #[test]
    fn env_token_wins() {
        let saved = || Some("saved".to_string());
        assert_eq!(pick_token(Some("env".into()), saved).as_deref(), Some("env"));
        assert_eq!(pick_token(None, saved).as_deref(), Some("saved"));
        assert_eq!(pick_token(Some(" ".into()), saved).as_deref(), Some("saved"));
        assert_eq!(pick_token(None, || Some(String::new())), None);
    }
}
