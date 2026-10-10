//! PZMM core. `pz` holds every Project Zomboid specific; the rest is game-neutral.

pub mod profile;
pub mod pz;
pub mod settings;
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
    // unique per write: two writers of one file (threads or processes) never share a temp file
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}-{}.pzmm-tmp", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let tmp = std::path::PathBuf::from(tmp);
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes.as_ref())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A `write_atomic` temp file's name: `<file>.<pid>-<n>.pzmm-tmp`.
fn is_temp(name: &str) -> bool {
    let tag = name.strip_suffix(".pzmm-tmp").and_then(|n| n.rsplit_once('.')).map_or("", |(_, t)| t);
    tag.split_once('-').is_some_and(|(pid, n)| [pid, n].iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
}

/// Delete the temp files a crash mid-`write_atomic` left in the folders PZMM writes to (an
/// hour old, so another process's write in progress is never touched). Run once at startup.
pub fn sweep_temp(paths: &pz::Paths) {
    let profiles = std::fs::read_dir(paths.data.join("profiles")).into_iter().flatten().flatten().map(|e| e.path());
    let dirs = [paths.data.clone(), paths.mods(), pz::server::dir(paths)].into_iter().chain(profiles);
    for e in dirs.filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age.as_secs() > 3600));
        if old && e.file_type().is_ok_and(|t| t.is_file()) && is_temp(&e.file_name().to_string_lossy()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
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

/// Hold this while changing PZMM's data or the game's mod list. It's an OS file lock on
/// `<data>/pzmm.lock`, so the CLI and the app (and two app windows) never interleave
/// read-modify-write cycles. Released when the returned file is dropped.
pub fn lock_data(paths: &pz::Paths) -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(&paths.data)?;
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(paths.data.join("pzmm.lock"))?;
    f.lock()?;
    Ok(f)
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

    #[test]
    fn overlapping_writes_never_truncate() {
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("cache.json");
        let payload = |i: usize| vec![b'a' + i as u8; 100_000];
        std::thread::scope(|s| {
            for i in 0..8 {
                let (file, data) = (&file, payload(i));
                s.spawn(move || {
                    for _ in 0..10 {
                        let _ = super::write_atomic(file, &data); // a rename may lose the race; it never leaves half a file
                    }
                });
            }
        });
        let got = std::fs::read(&file).unwrap();
        assert!((0..8).any(|i| got == payload(i)), "whole file from one writer");
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1, "no temp files left");
    }

    #[test]
    fn sweeps_only_old_temp_files() {
        let t = tempfile::tempdir().unwrap();
        let paths = crate::pz::Paths { zomboid: t.path().into(), data: t.path().join("data") };
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        let dirs = [paths.data.clone(), paths.mods(), crate::pz::server::dir(&paths), paths.data.join("profiles/Default")];
        let mut keep = vec![];
        for d in &dirs {
            std::fs::create_dir_all(d).unwrap();
            for (name, old) in [("default.txt.123-4.pzmm-tmp", true), ("default.txt.123-5.pzmm-tmp", false), ("x.pzmm-tmp", true), ("a.1-.pzmm-tmp", true), ("default.txt", true)] {
                let f = std::fs::File::create(d.join(name)).unwrap();
                if old {
                    f.set_modified(hour_ago).unwrap();
                }
                if name != "default.txt.123-4.pzmm-tmp" {
                    keep.push(d.join(name));
                }
            }
        }
        super::sweep_temp(&paths);
        for d in &dirs {
            assert!(!d.join("default.txt.123-4.pzmm-tmp").exists(), "{}", d.display());
        }
        assert!(keep.iter().all(|f| f.exists()), "new temp files and other names stay");
    }

    #[test]
    fn data_lock_is_exclusive() {
        let t = tempfile::tempdir().unwrap();
        let paths = crate::pz::Paths { zomboid: t.path().into(), data: t.path().join("data") };
        let held = super::lock_data(&paths).unwrap();
        let other = std::fs::OpenOptions::new().write(true).open(paths.data.join("pzmm.lock")).unwrap();
        assert!(matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
        drop(held);
        assert!(other.try_lock().is_ok());
    }
}
