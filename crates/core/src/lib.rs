//! PZMM core. `pz` holds every Project Zomboid specific; the rest is game-neutral.

/// How PZMM introduces itself to web services (Steam limits requests per user agent).
pub const USER_AGENT: &str = concat!("PZMM/", env!("CARGO_PKG_VERSION"), " (+https://github.com/SEVNC3/pz-mod-manager)");

pub mod history;
pub mod profile;
pub mod pz;
pub mod settings;
pub mod sources;
pub mod store;

use std::sync::OnceLock;

/// An activity line: its English `text` and, for lines worded by PZMM, the locale key
/// (`backend.<key>` in the app's locale files) and the values for its `{placeholders}`.
#[derive(serde::Serialize)]
pub struct LogLine {
    pub text: String,
    pub key: Option<&'static str>,
    pub vars: std::collections::BTreeMap<&'static str, String>,
}

type Sink = Box<dyn Fn(&LogLine) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Send progress/error lines somewhere other than stdout (the desktop app's activity log).
pub fn set_log_sink(f: impl Fn(&LogLine) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(f));
}

fn emit(line: LogLine) {
    match SINK.get() {
        Some(f) => f(&line),
        None => println!("{}", line.text),
    }
}

/// A line as it is (tool output, error details): it reads the same in every language.
pub fn log_line(msg: &str) {
    emit(LogLine { text: msg.into(), key: None, vars: Default::default() });
}

/// `template` with each `{name}` replaced by its value in `vars`.
pub fn fill(template: &str, vars: &[(&'static str, String)]) -> String {
    vars.iter().fold(template.to_string(), |s, (k, v)| s.replace(&format!("{{{k}}}"), v))
}

/// A line PZMM words itself: English from `template` (the CLI prints it), and the key the app translates.
pub fn log_keyed(key: &'static str, template: &str, vars: &[(&'static str, String)]) {
    emit(LogLine { text: fill(template, vars), key: Some(key), vars: vars.iter().cloned().collect() });
}

type ProgressSink = Box<dyn Fn(usize, usize) + Send + Sync>;
static PROGRESS: OnceLock<ProgressSink> = OnceLock::new();

/// Hear download progress (`done` of `total` Workshop items); without a sink it's dropped.
pub fn set_progress_sink(f: impl Fn(usize, usize) + Send + Sync + 'static) {
    let _ = PROGRESS.set(Box::new(f));
}

pub fn progress(done: usize, total: usize) {
    if let Some(f) = PROGRESS.get() {
        f(done, total);
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::log_line(&format!($($t)*)) };
}

/// `say!(key, template, name = value, ...)` with literal key and template: a translatable activity line (see `log_keyed`).
/// The English must match `backend.<key>` in app/src/lib/locales/en.json (scripts/i18n-check.ts checks it);
/// a template starting with "error: " is an error line (en.json leaves that prefix out).
#[macro_export]
macro_rules! say {
    ($key:literal, $template:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::log_keyed($key, $template, &[$((stringify!($name), $value.to_string())),*])
    };
}

/// `keyed!(key, template, name = value, ...)` with literal key and template: an error the app shows in
/// the player's language (`keyedError` in i18n.svelte.ts): `errors.<key>`, its values as JSON when it
/// has any, then ": " and the English (as `say!`; en.json's `errors.<key>` matches it).
#[macro_export]
macro_rules! keyed {
    ($key:literal, $template:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::keyed_text($key, $template, &[$((stringify!($name), $value.to_string())),*])
    };
}

pub fn keyed_text(key: &str, template: &str, vars: &[(&'static str, String)]) -> String {
    let map: std::collections::BTreeMap<_, _> = vars.iter().cloned().collect();
    // a `}` in a value is escaped: keyedError takes the first `}: ` as the end of the values
    let json = if vars.is_empty() { String::new() } else { serde_json::to_string(&map).map(|j| format!("{}}}", j[..j.len() - 1].replace('}', "\\u007d"))).unwrap_or_default() };
    format!("errors.{key}{json}: {}", fill(template, vars))
}

/// A `keyed!` error's English (anything else as it is).
pub fn unkeyed(s: &str) -> &str {
    let english = || {
        let rest = s.strip_prefix("errors.")?.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
        if rest.starts_with('{') { rest.split_once("}: ").map(|x| x.1) } else { rest.strip_prefix(": ") }
    };
    english().unwrap_or(s)
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

/// Delete the temp files a crash mid-`write_atomic` (or mid server backup: `.pzmm-part`) left in
/// the folders PZMM writes to (an hour old, so another process's write in progress is never
/// touched), and what restores cut short left ([`pz::saves::sweep`]). Run once at startup.
pub fn sweep_temp(paths: &pz::Paths) {
    let sub = |d: std::path::PathBuf| std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path());
    let backups = sub(paths.data.join("server-backups")).flat_map(|d| [d.join("zones"), d]);
    let dirs = [paths.data.clone(), paths.mods(), pz::server::dir(paths)].into_iter().chain(sub(paths.data.join("profiles"))).chain(backups);
    for e in dirs.filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age.as_secs() > 3600));
        let name = e.file_name().to_string_lossy().into_owned();
        if old && e.file_type().is_ok_and(|t| t.is_file()) && (is_temp(&name) || name.ends_with(".pzmm-part")) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    pz::saves::sweep(paths);
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
/// `name` with the characters a file name can't hold as spaces (runs collapsed), its ends trimmed
/// of dots and spaces: a title or server name made usable as a folder name.
pub fn clean_file_name(name: &str) -> String {
    let clean: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { ' ' } else { c }).collect();
    clean.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches(['.', ' ']).to_string()
}

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

/// One of PZMM's JSON files: missing is empty; one that doesn't parse (damaged, or an older
/// format) is kept as `<file>.bad` and starts over empty; one that can't be read is an error
/// (saving over it would wipe it).
pub(crate) fn read_json<T: serde::de::DeserializeOwned + Default>(file: &std::path::Path) -> anyhow::Result<T> {
    use anyhow::Context;
    let mut tries = 0;
    let bytes = loop {
        match std::fs::read(file) {
            Ok(b) => break b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
            // a scanner on Windows holds a file a moment: read again before it counts as unreadable
            // ERROR_SHARING_VIOLATION (32) and ERROR_LOCK_VIOLATION (33), or access denied while a scanner has it
            Err(e) if tries < 3 && (e.kind() == std::io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(32 | 33))) => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
        }
    };
    Ok(serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        let bad = file.with_extension("json.bad");
        if std::fs::read(&bad).ok().as_deref() != Some(bytes.as_slice()) {
            let _ = std::fs::copy(file, &bad);
            say!("fileDamaged", "error: {file} is damaged ({error}); kept a copy as {copy} and started over", file = file.display(), error = e, copy = bad.display());
        }
        T::default()
    }))
}

/// Hold this while changing PZMM's data or the game's mod list. It's an OS file lock on
/// `<data>/pzmm.lock`, so the CLI and the app (and two app windows) never interleave
/// read-modify-write cycles. Released when the returned file is dropped.
pub fn lock_data(paths: &pz::Paths) -> std::io::Result<std::fs::File> {
    lock_dir(&paths.data)
}

fn lock_dir(dir: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(dir)?;
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("pzmm.lock"))?;
    f.lock()?;
    Ok(f)
}

#[cfg(test)]
mod tests {
    #[test]
    fn keyed_values_never_end_early() {
        let e = keyed!("stopFirst", "Stop {name} first.", name = "a}: b");
        // as the UI's keyedError reads it: the values end at the first `}: `
        let (head, english) = e.split_once("}: ").unwrap();
        assert_eq!((english, super::unkeyed(&e)), ("Stop a}: b first.", "Stop a}: b first."));
        let vars: std::collections::BTreeMap<String, String> = serde_json::from_str(&format!("{}}}", head.strip_prefix("errors.stopFirst").unwrap())).unwrap();
        assert_eq!(vars["name"], "a}: b");
        assert_eq!(super::unkeyed("plain: text"), "plain: text");
    }

    #[test]
    fn fills_placeholders() {
        let vars = [("id", "A".to_string()), ("other", "B".to_string())];
        assert_eq!(super::fill("{id} left off: it's incompatible with {other}. Enable it instead of {other}", &vars), "A left off: it's incompatible with B. Enable it instead of B");
        assert_eq!(super::fill("{missing} stays", &vars), "{missing} stays");
    }

    #[test]
    fn safe_file_names() {
        for ok in ["ModOptions", "Trash and Corpses", "TLOG_Remaster Flamethrower", "mod.v2", "Comic"] {
            assert!(super::safe_file_name(ok), "{ok}");
        }
        for bad in ["", ".", "..", "a/b", r"a\b", "x:", "trail.", "trail ", "CON", "con.txt", "COM1", "lpt9", "tab\t"] {
            assert!(!super::safe_file_name(bad), "{bad:?}");
        }
        assert_eq!(super::clean_file_name(" Mods: \"Best\" | 2024? / B42... "), "Mods Best 2024 B42");
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
        let dirs = [paths.data.clone(), paths.mods(), crate::pz::server::dir(&paths), paths.data.join("profiles/Default"), paths.data.join("server-backups/A"), paths.data.join("server-backups/A/zones")];
        let mut keep = vec![];
        let gone = ["default.txt.123-4.pzmm-tmp", "1-manual.zip.pzmm-part"];
        for d in &dirs {
            std::fs::create_dir_all(d).unwrap();
            for (name, old) in [("default.txt.123-4.pzmm-tmp", true), ("default.txt.123-5.pzmm-tmp", false), ("x.pzmm-tmp", true), ("a.1-.pzmm-tmp", true), ("default.txt", true), ("1-manual.zip.pzmm-part", true), ("2-manual.zip.pzmm-part", false)] {
                let f = std::fs::File::create(d.join(name)).unwrap();
                if old {
                    f.set_modified(hour_ago).unwrap();
                }
                if !gone.contains(&name) {
                    keep.push(d.join(name));
                }
            }
        }
        super::sweep_temp(&paths);
        for d in &dirs {
            assert!(gone.iter().all(|g| !d.join(g).exists()), "{}", d.display());
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
