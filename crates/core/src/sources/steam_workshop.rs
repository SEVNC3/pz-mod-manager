//! Steam Workshop: anonymous downloads through SteamCMD, and the folder where the
//! Steam client keeps the user's own subscriptions.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::pz::Paths;

const STEAMCMD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/";

/// `steamapps/workshop/content/<app>` in whichever Steam library has it.
pub fn content_dir(app: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    #[allow(unused_mut)]
    let mut roots = vec![
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        if let Ok(p) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam").and_then(|k| k.get_value::<String, _>("SteamPath")) {
            roots.insert(0, PathBuf::from(p));
        }
    }
    for root in roots {
        let vdf = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
        let libs = vdf.lines().filter_map(|l| {
            let v = l.trim().strip_prefix("\"path\"")?.trim().trim_matches('"');
            Some(PathBuf::from(v.replace("\\\\", "\\")))
        });
        for lib in std::iter::once(root.clone()).chain(libs) {
            let d = lib.join("steamapps/workshop/content").join(app);
            if d.is_dir() {
                return Some(d);
            }
        }
    }
    None
}

fn steamcmd(paths: &Paths) -> Result<PathBuf> {
    let dir = paths.data.join("steamcmd");
    let exe = dir.join(if cfg!(windows) { "steamcmd.exe" } else { "steamcmd.sh" });
    if !exe.exists() {
        crate::log!("downloading steamcmd...");
        std::fs::create_dir_all(&dir)?;
        if cfg!(windows) {
            super::unzip(&super::http_get(&format!("{STEAMCMD_URL}steamcmd.zip"))?, &dir)?;
        } else {
            let gz = super::http_get(&format!("{STEAMCMD_URL}steamcmd_linux.tar.gz"))?;
            tar::Archive::new(flate2::read::GzDecoder::new(&gz[..])).unpack(&dir)?;
        }
    }
    Ok(exe)
}

/// Download items anonymously. Returns each id with its content folder, or None if
/// SteamCMD didn't produce one (its exit code is unreliable, so we check the folder).
pub fn download(paths: &Paths, app: &str, ids: &[&str]) -> Result<Vec<(String, Option<PathBuf>)>> {
    let exe = steamcmd(paths)?;
    let mut cmd = Command::new(&exe);
    cmd.arg("+login").arg("anonymous");
    for id in ids {
        cmd.args(["+workshop_download_item", app, id, "validate"]);
    }
    cmd.arg("+quit").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    crate::log!("downloading {} workshop item(s)...", ids.len());
    let mut child = cmd.spawn().with_context(|| format!("running {}", exe.display()))?;
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let l = line.to_lowercase();
            if l.contains("workshop") || l.contains("error") {
                crate::log!("{}", line.trim());
            }
        }
    }
    child.wait()?;
    let content: &Path = &exe.parent().unwrap().join("steamapps/workshop/content").join(app);
    Ok(ids.iter().map(|id| (id.to_string(), Some(content.join(id)).filter(|d| d.is_dir()))).collect())
}
