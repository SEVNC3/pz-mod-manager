//! PZMM core. `pz` holds every Project Zomboid specific; the rest is game-neutral.

pub mod profile;
pub mod pz;
pub mod sources;
pub mod store;

use std::sync::OnceLock;

type Sink = Box<dyn Fn(&str) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Send progress/error lines somewhere other than stdout (the desktop app's activity log).
pub fn set_log_sink(f: impl Fn(&str) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(f));
}

pub fn log_line(msg: &str) {
    match SINK.get() {
        Some(f) => f(msg),
        None => println!("{msg}"),
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::log_line(&format!($($t)*)) };
}

/// Write via a temp file + rename, so a crash never leaves a half-written file.
pub fn write_atomic(path: &std::path::Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".pzmm-tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Keep the file as it was before PZMM first changed it (`<file>.bak`); later writes
/// don't overwrite that original.
pub fn backup_once(path: &std::path::Path) -> std::io::Result<()> {
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    let bak = std::path::PathBuf::from(bak);
    if path.exists() && !bak.exists() {
        std::fs::copy(path, bak)?;
    }
    Ok(())
}

/// Usable as a single folder/file name on Windows and Linux: not empty, `.` or `..`, no
/// path separators or reserved characters, no trailing dot/space, not a DOS device name.
pub fn safe_file_name(name: &str) -> bool {
    let reserved = |b: &str| {
        let b = b.to_ascii_uppercase();
        ["CON", "PRN", "AUX", "NUL"].contains(&b.as_str())
            || ((b.starts_with("COM") || b.starts_with("LPT")) && b.len() == 4 && b.as_bytes()[3].is_ascii_digit())
    };
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.chars().any(|c| r#"<>:"/\|?*"#.contains(c) || c.is_control())
        && !name.ends_with(['.', ' '])
        && !reserved(name.split('.').next().unwrap_or(name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn safe_file_names() {
        for ok in ["ModOptions", "Trash and Corpses", "TLOG_Remaster Flamethrower", "mod.v2", "Comic"] {
            assert!(super::safe_file_name(ok), "{ok}");
        }
        for bad in ["", ".", "..", "a/b", r"a\b", "x:", "trail.", "trail ", "CON", "con.txt", "COM1", "lpt9", "tab\t"] {
            assert!(!super::safe_file_name(bad), "{bad:?}");
        }
    }
}
