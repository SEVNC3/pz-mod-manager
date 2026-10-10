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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
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

pub fn load_from(path: &Path) -> Settings {
    let Ok(text) = fs::read_to_string(path) else { return Settings::default() };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        // keep the broken file: the next save would otherwise replace it with defaults unseen
        let bad = path.with_extension("json.bad");
        if fs::read(&bad).ok().as_deref() != Some(text.as_bytes()) {
            let _ = fs::copy(path, &bad);
            crate::log!("error: {} is damaged ({e}); kept a copy as {} and used the default settings", path.display(), bad.display());
        }
        Settings::default()
    })
}

pub fn save(s: &Settings) -> Result<()> {
    save_to(&path(), s)
}

pub fn save_to(path: &Path, s: &Settings) -> Result<()> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    crate::write_atomic(path, serde_json::to_string_pretty(s)?).with_context(|| format!("saving {}", path.display()))
}

// --- GitHub token (raises GitHub's rate limit), kept in the OS credential store ---

fn token_entry() -> Result<keyring::Entry> {
    keyring::Entry::new("pzmm", "github-token").map_err(|e| anyhow::anyhow!("the system's credential store isn't available: {e}"))
}

/// `GITHUB_TOKEN`, else the saved token. Unit tests never read the credential store.
pub fn github_token() -> Option<String> {
    pick_token(std::env::var("GITHUB_TOKEN").ok(), || if cfg!(test) { None } else { token_entry().ok()?.get_password().ok() })
}

fn pick_token(env: Option<String>, saved: impl FnOnce() -> Option<String>) -> Option<String> {
    env.filter(|t| !t.trim().is_empty()).or_else(saved).filter(|t| !t.trim().is_empty())
}

/// Whether a token is saved in the credential store.
pub fn token_saved() -> bool {
    token_entry().is_ok_and(|e| e.get_password().is_ok())
}

/// Save the token, or forget it when `token` is empty.
pub fn set_github_token(token: &str) -> Result<()> {
    let e = token_entry()?;
    let token = token.trim();
    if token.is_empty() {
        return match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(anyhow::anyhow!("forgetting the GitHub token: {err}")),
        };
    }
    e.set_password(token).map_err(|err| anyhow::anyhow!("saving the GitHub token: {err}"))
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
    fn env_token_wins() {
        let saved = || Some("saved".to_string());
        assert_eq!(pick_token(Some("env".into()), saved).as_deref(), Some("env"));
        assert_eq!(pick_token(None, saved).as_deref(), Some("saved"));
        assert_eq!(pick_token(Some(" ".into()), saved).as_deref(), Some("saved"));
        assert_eq!(pick_token(None, || Some(String::new())), None);
    }
}
