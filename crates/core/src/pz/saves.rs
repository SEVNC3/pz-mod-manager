//! Saved games: each keeps the mods it was played with in `Saves/<mode>/<save>/mods.txt`,
//! in the same format as `mods/default.txt`. PZMM backs a save's folder up into
//! `<save backups>/<mode>/<save>/<unix ms>-<reason>[-<slot name>].zip` (the "saves" folder in
//! Settings, else `<PZMM data>/save-backups`), restores it, finds broken map files, resets parts
//! the game rebuilds, copies a save with other mods and reads its `players.db`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use super::server_backup::{self, Backup};
use super::{Paths, read_default};

#[derive(Debug, serde::Serialize)]
pub struct Save {
    pub mode: String,
    pub name: String,
    pub mods: Vec<String>,
    /// Its `Map=` folders, in order.
    pub maps: Vec<String>,
    /// Last played (seconds since 1970), for sorting.
    pub modified: u64,
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// When the save was last written: the newest of its folder and the files directly in it (the game
/// rewrites map_t.bin, players.db... in place, which leaves the folder's own time alone).
fn last_played(dir: &Path) -> u64 {
    let own = fs::metadata(dir).and_then(|m| m.modified()).map_or(0, secs);
    fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()?.modified().ok()).map(secs).fold(own, u64::max)
}

/// Every save that records its mods, most recently played first.
pub fn list(paths: &Paths) -> Vec<Save> {
    let mut out = vec![];
    for mode in fs::read_dir(paths.zomboid.join("Saves")).into_iter().flatten().flatten() {
        for save in fs::read_dir(mode.path()).into_iter().flatten().flatten() {
            let file = save.path().join("mods.txt");
            if !file.is_file() {
                continue;
            }
            let list = read_default(&file);
            out.push(Save {
                mode: mode.file_name().to_string_lossy().into_owned(),
                name: save.file_name().to_string_lossy().into_owned(),
                maps: list.map_folders(),
                mods: list.mods,
                modified: last_played(&save.path()),
            });
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.modified));
    out
}

/// A save's folder (it may not exist: a deleted save that has backups). `mode` and `name` are plain folder names.
/// The in-game date and time when a world last saved (its `map_t.bin`): year, month (1-12), day,
/// hour, minute. None when it isn't there or doesn't read as one.
pub fn game_time(save: &Path) -> Option<[u32; 5]> {
    let b = fs::read(save.join("map_t.bin")).ok()?;
    if b.get(..4)? != b"GMTM" {
        return None;
    }
    // big-endian: version, speed, nights survived, target zombies, last time, time of day, day, month, year
    let word = |at: usize| -> Option<[u8; 4]> { b.get(at..at + 4)?.try_into().ok() };
    let hours = f32::from_be_bytes(word(24)?);
    let [day, month, year] = [28, 32, 36].map(|at| word(at).map(i32::from_be_bytes));
    let (day, month, year) = (day?, month?, year?);
    if !(0.0..24.0).contains(&hours) || !(0..12).contains(&month) || !(0..31).contains(&day) || !(1900..3000).contains(&year) {
        return None;
    }
    Some([year as u32, month as u32 + 1, day as u32 + 1, hours as u32, (hours.fract() * 60.0) as u32])
}

pub fn dir(paths: &Paths, mode: &str, name: &str) -> Result<PathBuf> {
    if !crate::safe_file_name(mode) || !crate::safe_file_name(name) {
        bail!("invalid save {mode}/{name}");
    }
    Ok(paths.zomboid.join("Saves").join(mode).join(name))
}

/// Where save backups go: the "saves" folder set in Settings, else `<PZMM data>/save-backups`.
fn root(paths: &Paths) -> PathBuf {
    crate::settings::load().folders.get("saves").cloned().unwrap_or_else(|| paths.data.join("save-backups"))
}

/// What keeps each Zomboid folder's save backups apart (two folders can each have a `Sandbox/<name>`):
/// `-` and a short hash of the folder, after the save's name.
fn folder_tag(paths: &Paths) -> String {
    let z = paths.zomboid.to_string_lossy();
    let z = if cfg!(windows) { z.to_lowercase() } else { z.into_owned() };
    format!("-{}", &super::javamods::sha256(z.as_bytes())[..8])
}

fn backup_dir(paths: &Paths, mode: &str, name: &str) -> Result<PathBuf> {
    dir(paths, mode, name)?;
    let parent = root(paths).join(mode);
    let to = parent.join(format!("{name}{}", folder_tag(paths)));
    // backups from before the tag are taken to be the folder's in use now
    let old = parent.join(name);
    if !to.exists()
        && old.is_dir()
        && let Err(e) = fs::rename(&old, &to)
    {
        crate::log!("couldn't move the backups in {} to {}: {e}", old.display(), to.display());
        return Ok(old);
    }
    Ok(to)
}

/// A save's backups, newest first.
pub fn backups(paths: &Paths, mode: &str, name: &str) -> Result<Vec<Backup>> {
    Ok(server_backup::list_in(&backup_dir(paths, mode, name)?))
}

/// Saves that are gone but have backups: (mode, name).
pub fn deleted_with_backups(paths: &Paths) -> Vec<(String, String)> {
    let (mut out, tag) = (vec![], folder_tag(paths));
    // another folder's tag: `-` and 8 hex digits
    let tagged = |n: &str| n.rsplit_once('-').is_some_and(|(_, h)| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    for mode in fs::read_dir(root(paths)).into_iter().flatten().flatten() {
        for save in fs::read_dir(mode.path()).into_iter().flatten().flatten() {
            let (m, n) = (mode.file_name().to_string_lossy().into_owned(), save.file_name().to_string_lossy().into_owned());
            // this folder's, or from before the tag
            let n = match n.strip_suffix(&tag) {
                Some(n) => n.to_string(),
                None if !tagged(&n) => n,
                None => continue,
            };
            if dir(paths, &m, &n).is_ok_and(|d| !d.exists()) && !server_backup::list_in(&save.path()).is_empty() {
                out.push((m, n));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Zip the save's folder (no retention). None when there's nothing in it.
fn take(paths: &Paths, mode: &str, name: &str, tag: &str) -> Result<Option<Backup>> {
    let save = dir(paths, mode, name)?;
    let mut items = vec![];
    server_backup::walk(&save, "", &mut items);
    let to = server_backup::new_file(&backup_dir(paths, mode, name)?, tag);
    if !server_backup::zip_items(&items, &to)? {
        return Ok(None);
    }
    Ok(backups(paths, mode, name)?.into_iter().find(|b| b.file == to))
}

/// Crash backups kept per save.
const CRASHES: u32 = 3;

/// The automatic backups beyond `keep` (0: no limit) and crash backups beyond [`CRASHES`] go; slots stay.
fn prune(paths: &Paths, mode: &str, name: &str, keep: u32) -> Result<()> {
    let (crash, auto): (Vec<Backup>, Vec<Backup>) = backups(paths, mode, name)?.into_iter().filter(|b| b.reason != "slot").partition(|b| b.reason == "crash");
    for f in server_backup::prune(&auto, keep, 0).into_iter().chain(server_backup::prune(&crash, CRASHES, 0)) {
        let _ = fs::remove_file(f);
    }
    Ok(())
}

/// Back a save up now. With a `slot` name it's kept until the player deletes it; otherwise the
/// oldest automatic backups beyond `keep` go.
pub fn back_up(paths: &Paths, mode: &str, name: &str, reason: &str, slot: &str, keep: u32) -> Result<Backup> {
    let slot = slot.trim();
    let tag = if slot.is_empty() { reason.to_string() } else { slot_tag(slot)? };
    let b = take(paths, mode, name, &tag)?.with_context(|| format!("the save {name} has nothing to back up"))?;
    crate::say!("saveBackedUp", "Backed up save {name} ({size} MB)", name = name, size = format!("{:.1}", b.size as f64 / 1048576.0));
    prune(paths, mode, name, keep)?;
    Ok(b)
}

/// `slot-<name>`, a slot's part of its file name.
fn slot_tag(slot: &str) -> Result<String> {
    let slot = slot.trim();
    if !crate::safe_file_name(slot) || slot.chars().count() > 60 {
        bail!("a slot name can't have \\ / : * ? \" < > | in it, or end with a dot");
    }
    Ok(format!("slot-{slot}"))
}

/// Name backup `file` `slot` (an automatic backup becomes a slot, kept until deleted), or with
/// `copy` keep it and add a copy under that name. Its time stays the time it was taken.
pub fn name_backup(paths: &Paths, mode: &str, name: &str, file: &Path, slot: &str, copy: bool) -> Result<Backup> {
    let b = backups(paths, mode, name)?.into_iter().find(|b| b.file == file).with_context(|| format!("{} isn't one of {name}'s backups", file.display()))?;
    let stamp = file.file_name().and_then(|f| f.to_str()).and_then(|f| f.split_once('-')).map(|(ms, _)| ms.to_string()).context("backup name")?;
    let to = backup_dir(paths, mode, name)?.join(format!("{stamp}-{}.zip", slot_tag(slot)?));
    if to.exists() {
        bail!("this backup already has a slot named {}", slot.trim());
    }
    if copy {
        fs::copy(&b.file, &to).with_context(|| format!("copying {}", b.file.display()))?;
    } else {
        fs::rename(&b.file, &to).with_context(|| format!("renaming {}", b.file.display()))?;
    }
    backups(paths, mode, name)?.into_iter().find(|b| b.file == to).context("the named backup")
}

/// Put backup `file` back (the game must be closed: the caller checks). The save as it is now is
/// backed up first ("restore"); retention runs after, so it never takes the backup being restored.
pub fn restore(paths: &Paths, mode: &str, name: &str, file: &Path, keep: u32) -> Result<()> {
    if file.parent() != Some(backup_dir(paths, mode, name)?.as_path()) {
        bail!("{} isn't one of {name}'s backups", file.display());
    }
    let mut z = zip::ZipArchive::new(fs::File::open(file).with_context(|| format!("opening {}", file.display()))?)?;
    for i in 0..z.len() {
        let e = z.by_index(i)?;
        if e.enclosed_name().is_none() {
            bail!("{} holds {}, outside the save: nothing was changed", file.display(), e.name()?);
        }
    }
    let save = dir(paths, mode, name)?;
    // the save as it was before an earlier restore that didn't finish: never deleted here
    let old = save.with_file_name(format!("{name}.pzmm-old"));
    if old.exists() {
        bail!("{} is left from an earlier restore (the save as it was then): move it out of the Saves folder or delete it, then try again", old.display());
    }
    take(paths, mode, name, "restore")?;
    // unpacked, then swapped in: a failure part way leaves the save as it was. In PZMM's data when
    // that's on the save's drive (the game would list a folder beside the save as a save), else beside it.
    let new = if same_drive(&paths.data, &save) { paths.data.join("restoring").join(mode).join(name) } else { save.with_file_name(format!("{name}.pzmm-restoring")) };
    if new.exists() {
        fs::remove_dir_all(&new).with_context(|| format!("removing {}", new.display()))?;
    }
    let unpacked = (|| -> Result<()> {
        fs::create_dir_all(&new)?;
        for i in 0..z.len() {
            let mut e = z.by_index(i)?;
            if e.is_dir() {
                continue;
            }
            let to = new.join(e.enclosed_name().context("entry path")?);
            fs::create_dir_all(to.parent().context("entry folder")?)?;
            std::io::copy(&mut e, &mut fs::File::create(&to)?)?;
        }
        Ok(())
    })();
    if let Err(e) = unpacked {
        let _ = fs::remove_dir_all(&new);
        return Err(e.context(format!("unpacking {}: the save wasn't changed", file.display())));
    }
    if save.exists()
        && let Err(e) = fs::rename(&save, &old)
    {
        let _ = fs::remove_dir_all(&new);
        return Err(e).with_context(|| format!("moving {} aside (is the game running?): it wasn't changed", save.display()));
    }
    fs::rename(&new, &save).with_context(|| format!("moving the restored save into place: the save as it was is in {}", old.display()))?;
    let _ = fs::remove_dir_all(&old);
    crate::say!("saveRestored", "Restored save {name} from {file}", name = name, file = file.display());
    prune(paths, mode, name, keep)
}

/// Whether `a` and `b` are on one drive (so a folder renames from one to the other).
fn same_drive(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        let drive = |p: &Path| p.components().next().map(|c| c.as_os_str().to_ascii_uppercase());
        drive(a).is_some() && drive(a) == drive(b)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        let dev = |p: &Path| p.ancestors().find_map(|d| fs::metadata(d).ok()).map(|m| m.dev());
        dev(a).is_some() && dev(a) == dev(b)
    }
}

/// At startup: what restores cut short (PZMM closed part way) left, an hour old at least so none is in
/// progress. Unpacked copies go; the save as it was goes only once the restored save is in its
/// place (it's in the "restore" backup too).
pub fn sweep(paths: &Paths) {
    let stale = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age.as_secs() > 3600));
    let sub = |d: PathBuf| fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path());
    let unpacked = sub(paths.data.join("restoring")).flat_map(sub);
    let beside = sub(paths.zomboid.join("Saves")).flat_map(sub).filter(|p| {
        let p = p.to_string_lossy();
        p.ends_with(".pzmm-restoring") || p.strip_suffix(".pzmm-old").is_some_and(|save| Path::new(save).is_dir())
    });
    for d in unpacked.chain(beside).filter(|d| stale(d)) {
        let _ = fs::remove_dir_all(d);
    }
}

pub fn delete_backup(paths: &Paths, mode: &str, name: &str, file: &Path) -> Result<()> {
    if file.parent() != Some(backup_dir(paths, mode, name)?.as_path()) || file.extension().is_none_or(|x| x != "zip") {
        bail!("{} isn't one of {name}'s backups", file.display());
    }
    fs::remove_file(file).with_context(|| format!("deleting {}", file.display()))
}

/// A map file the game rebuilds from the map when it's missing: named by its cell or chunk
/// coordinates (`map_12_34.bin`, `chunkdata_1_2.bin`, `map/12/34.bin`).
fn regenerable(rel: &str) -> bool {
    let p = Path::new(rel);
    let num = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parts: Vec<&str> = stem.rsplitn(3, '_').collect();
    (parts.len() == 3 && num(parts[0]) && num(parts[1])) || (num(stem) && p.parent().and_then(|d| d.file_name()).and_then(|d| d.to_str()).is_some_and(num))
}

/// Zero-byte map files in a save (a crash while saving leaves them): cells and chunks the game
/// rebuilds once they're removed, and the rest (world files only a backup brings back).
#[derive(Debug, Default, serde::Serialize)]
pub struct Broken {
    pub cells: Vec<String>,
    pub other: Vec<String>,
}

pub fn broken(save: &Path) -> Broken {
    let (cells, other) = server_backup::empty_cells(save).into_iter().partition(|r| regenerable(r));
    Broken { cells, other }
}

/// Back the save up ("repair"), then remove its zero-byte cell and chunk files. How many went.
pub fn repair(paths: &Paths, mode: &str, name: &str, keep: u32) -> Result<usize> {
    let save = dir(paths, mode, name)?;
    let cells = broken(&save).cells;
    if cells.is_empty() {
        return Ok(0);
    }
    back_up(paths, mode, name, "repair", "", keep)?;
    for rel in &cells {
        fs::remove_file(save.join(rel)).with_context(|| format!("removing {rel}"))?;
    }
    Ok(cells.len())
}

/// What a reset can start afresh. The files come from the game's own list of savefile files it
/// rebuilds when they're missing (the debug "delete selected files" list on its Load screen).
pub const RESETS: [&str; 3] = ["zombies", "time", "animals"];

fn reset_files(save: &Path, what: &str) -> Vec<String> {
    let num = |s: &str| {
        let d = s.strip_prefix('-').unwrap_or(s);
        !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())
    };
    // `<prefix>_<x>_<y>.bin`, in the save (B41) or its `<prefix>` folder (B42)
    let cell = |rel: &str, prefix: &str| {
        let name = rel.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')).unwrap_or(rel);
        !name.contains('/')
            && name.strip_prefix(prefix).and_then(|r| r.strip_prefix('_')).and_then(|r| r.strip_suffix(".bin")).and_then(|r| r.split_once('_')).is_some_and(|(x, y)| num(x) && num(y))
    };
    let mut items = vec![];
    server_backup::walk(save, "", &mut items);
    let mut out: Vec<String> = items
        .into_iter()
        .map(|(_, rel)| rel)
        .filter(|rel| match what {
            "zombies" => rel == "zpop_virtual.bin" || cell(rel, "zpop"),
            "time" => rel == "map_t.bin",
            "animals" => rel == "map_animals.bin" || cell(rel, "apop"),
            _ => false,
        })
        .collect();
    out.sort();
    out
}

/// How many files each reset would remove.
pub fn resets(save: &Path) -> std::collections::BTreeMap<&'static str, usize> {
    RESETS.iter().map(|w| (*w, reset_files(save, w).len())).collect()
}

/// Back the save up ("reset"), then remove the files of one of `RESETS` so the game builds them
/// afresh: "zombies" (the population, spread again from the map), "time" (map_t.bin: the clock,
/// calendar and weather), "animals" (B42's animal population). How many files went.
pub fn reset(paths: &Paths, mode: &str, name: &str, what: &str, keep: u32) -> Result<usize> {
    if !RESETS.contains(&what) {
        bail!("unknown reset {what:?}");
    }
    let save = dir(paths, mode, name)?;
    let files = reset_files(&save, what);
    if files.is_empty() {
        return Ok(0);
    }
    back_up(paths, mode, name, "reset", "", keep)?;
    for rel in &files {
        fs::remove_file(save.join(rel)).with_context(|| format!("removing {rel}"))?;
    }
    Ok(files.len())
}

/// Copy the save as `<name>_modded` (`_modded 2`... when taken) and give the copy these mods and
/// maps (see `set_mods`); the original stays as it is. The copy's name.
pub fn clone_with_mods(paths: &Paths, mode: &str, name: &str, ids: &[String], maps: &[String]) -> Result<String> {
    let from = dir(paths, mode, name)?;
    if !from.join("mods.txt").is_file() {
        bail!("save {mode}/{name} not found");
    }
    let base = format!("{name}_modded");
    let to = std::iter::once(base.clone()).chain((2..).map(|i| format!("{base} {i}"))).find(|n| dir(paths, mode, n).is_ok_and(|d| !d.exists())).context("a free name")?;
    crate::store::copy_dir(&from, &dir(paths, mode, &to)?).with_context(|| format!("copying {name} (is the game running?)"))?;
    set_mods(paths, mode, &to, ids, maps)?;
    Ok(to)
}

/// The save's screenshot (`thumb.png`, taken by the game when it saves), or the one in backup
/// `file`; empty when there's none.
pub fn thumb(paths: &Paths, mode: &str, name: &str, file: Option<&Path>) -> Result<Vec<u8>> {
    use std::io::Read;
    let Some(file) = file else { return Ok(fs::read(dir(paths, mode, name)?.join("thumb.png")).unwrap_or_default()) };
    if file.parent() != Some(backup_dir(paths, mode, name)?.as_path()) {
        bail!("{} isn't one of {name}'s backups", file.display());
    }
    let mut z = zip::ZipArchive::new(fs::File::open(file).with_context(|| format!("opening {}", file.display()))?)?;
    let mut out = vec![];
    if let Ok(mut e) = z.by_name("thumb.png") {
        e.read_to_end(&mut out)?;
    }
    Ok(out)
}

/// Give the save these mods, and the `maps` they add (in arranged order) that it lacks: the
/// maps it has stay, in their order (its world was made with them). Its mods.txt as it was
/// before stays as `mods.txt.prev` (and before PZMM's first change, `mods.txt.bak`).
pub fn set_mods(paths: &Paths, mode: &str, name: &str, ids: &[String], maps: &[String]) -> Result<()> {
    let file = dir(paths, mode, name)?.join("mods.txt");
    let old = super::try_read_default(&file).with_context(|| format!("can't read {} (is the game running?)", file.display()))?;
    super::server_config::keep_prev(&file)?;
    let maps = super::server::merge_order(maps, &old.map_folders());
    super::write_default(&file, ids, None, Some(&maps))
}

/// A character in `players.db`.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Player {
    pub id: i64,
    pub name: String,
    /// Multiplayer: the account.
    pub username: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub dead: bool,
    /// Size of the saved character (inventory, skills...).
    pub data_size: i64,
}

/// A copy of SQLite database `db` (with its write-ahead log), opened: the game's file is never
/// opened, so a running game or server isn't disturbed. The copy lives as long as the folder.
pub(crate) fn open_copy(db: &Path) -> Result<(tempfile::TempDir, rusqlite::Connection)> {
    let tmp = tempfile::tempdir()?;
    let copy = tmp.path().join("copy.db");
    fs::copy(db, &copy).with_context(|| format!("reading {}", db.display()))?;
    let wal = db.with_extension("db-wal");
    if wal.is_file() {
        fs::copy(&wal, tmp.path().join("copy.db-wal"))?;
    }
    let c = rusqlite::Connection::open(&copy)?;
    Ok((tmp, c))
}

/// [`open_copy`] of one of a save's databases, by file name.
fn save_db(save: &Path, file: &str) -> Result<(tempfile::TempDir, rusqlite::Connection)> {
    if !file.ends_with(".db") || !crate::safe_file_name(file) || !save.join(file).is_file() {
        bail!("this save has no {file}");
    }
    open_copy(&save.join(file))
}

/// A value as the database browser shows it: text and numbers as they are, data as `{"bytes": <size>}`.
fn shown(v: rusqlite::types::ValueRef) -> serde_json::Value {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => i.into(),
        ValueRef::Real(f) => f.into(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into(),
        ValueRef::Blob(b) => serde_json::json!({ "bytes": b.len() }),
    }
}

/// Database `db` opened to read with no copy and no lock (SQLite's `immutable`): a running game or
/// server isn't disturbed, though what it's writing right now may not show. For quick looks.
fn open_unlocked(db: &Path) -> Result<rusqlite::Connection> {
    let path = db.to_string_lossy().replace('\\', "/").replace('%', "%25").replace('?', "%3f").replace('#', "%23");
    use rusqlite::OpenFlags as F;
    Ok(rusqlite::Connection::open_with_flags(format!("file:{path}?immutable=1"), F::SQLITE_OPEN_READ_ONLY | F::SQLITE_OPEN_URI | F::SQLITE_OPEN_NO_MUTEX)?)
}

/// The save's databases (`players.db`, `vehicles.db`...) and their tables with how many rows each
/// has, read in place (copying every database to count its rows took long on a big save).
pub fn databases(save: &Path) -> Vec<(String, Vec<(String, u64)>)> {
    let mut files: Vec<String> = fs::read_dir(save).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str().filter(|n| n.ends_with(".db")).map(String::from)).collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|f| {
            let path = save.join(&f);
            // a write-ahead log holds what the file doesn't yet (the game's databases keep none): a copy reads both
            let (_tmp, c) = if path.with_extension("db-wal").is_file() { open_copy(&path).map(|(t, c)| (Some(t), c)).ok()? } else { (None, open_unlocked(&path).ok()?) };
            let names: Vec<String> = c.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").ok()?.query_map([], |r| r.get(0)).ok()?.flatten().collect();
            let count = |t: &str| c.query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64;
            let tables = names.into_iter().map(|t| {
                let n = count(&t);
                (t, n)
            }).collect();
            Some((f, tables))
        })
        .collect()
}

/// Rows `offset..offset + limit` of a table in one of the save's databases, values as `shown`.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

pub fn table(save: &Path, file: &str, table: &str, offset: u64, limit: u64) -> Result<Table> {
    let (_tmp, c) = save_db(save, file)?;
    let known: bool = c.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get::<_, i64>(0))? > 0;
    if !known || table.contains('"') {
        bail!("{file} has no table {table}");
    }
    let mut q = c.prepare(&format!("SELECT * FROM \"{table}\" LIMIT ?1 OFFSET ?2"))?;
    let columns: Vec<String> = q.column_names().into_iter().map(String::from).collect();
    let n = columns.len();
    let rows = q.query_map([limit.min(500) as i64, offset as i64], |r| (0..n).map(|i| r.get_ref(i).map(shown)).collect::<rusqlite::Result<Vec<_>>>())?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Table { columns, rows })
}

fn text(v: rusqlite::types::ValueRef) -> String {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        _ => String::new(),
    }
}

fn num(v: rusqlite::types::ValueRef) -> f64 {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Integer(i) => i as f64,
        ValueRef::Real(f) => f,
        ValueRef::Text(t) => String::from_utf8_lossy(t).trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn has_table(c: &rusqlite::Connection, table: &str) -> Result<bool> {
    Ok(c.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get::<_, i64>(0))? > 0)
}

/// The characters in a save's `players.db`, read from a copy (the save is never opened for writing).
pub fn players(save: &Path) -> Result<Vec<Player>> {
    let db = save.join("players.db");
    if !db.is_file() {
        bail!("this save has no players.db");
    }
    let (_tmp, c) = open_copy(&db)?;
    let mut out = vec![];
    for (table, user) in [("localPlayers", "''"), ("networkPlayers", "username")] {
        if !has_table(&c, table)? {
            continue;
        }
        let mut q = c.prepare(&format!("SELECT id, name, {user}, x, y, z, isDead, length(data) FROM {table} ORDER BY id"))?;
        let rows = q.query_map([], |r| {
            Ok(Player {
                id: num(r.get_ref(0)?) as i64,
                name: text(r.get_ref(1)?),
                username: text(r.get_ref(2)?),
                x: num(r.get_ref(3)?),
                y: num(r.get_ref(4)?),
                z: num(r.get_ref(5)?),
                dead: num(r.get_ref(6)?) != 0.0 || text(r.get_ref(6)?).eq_ignore_ascii_case("true"),
                data_size: num(r.get_ref(7)?) as i64,
            })
        })?;
        out.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    Ok(out)
}

/// An account on a dedicated server (its `db/<server>.db` whitelist).
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Account {
    pub username: String,
    /// Its role (Build 42) or access level (Build 41): "admin", "user"...
    pub role: String,
    pub last: String,
    pub steamid: String,
    pub banned: bool,
}

/// A server's accounts and bans.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Accounts {
    pub users: Vec<Account>,
    /// (SteamID, reason)
    pub banned_ids: Vec<(String, String)>,
    /// (IP, username, reason)
    pub banned_ips: Vec<(String, String, String)>,
}

/// The accounts and bans in a dedicated server's database (`db/<server>.db`), read from a copy.
/// Build 41 marks a ban on the account (`banned`); Build 42 gives it the role set for bans.
pub fn accounts(db: &Path) -> Result<Accounts> {
    if !db.is_file() {
        bail!("{} doesn't exist yet (the server makes it on its first start)", db.display());
    }
    let (_tmp, c) = open_copy(db)?;
    let mut out = Accounts::default();
    if has_table(&c, "whitelist")? {
        let roles: std::collections::HashMap<i64, String> = if has_table(&c, "role")? {
            let mut q = c.prepare("SELECT id, name FROM role")?;
            q.query_map([], |r| Ok((num(r.get_ref(0)?) as i64, text(r.get_ref(1)?))))?.collect::<rusqlite::Result<_>>()?
        } else {
            Default::default()
        };
        let banned_role: Option<i64> = if has_table(&c, "defaultRoles")? { c.query_row("SELECT role FROM defaultRoles WHERE name = 'banned'", [], |r| r.get(0)).ok() } else { None };
        let mut q = c.prepare("SELECT * FROM whitelist")?;
        let cols: Vec<String> = q.column_names().iter().map(|n| n.to_ascii_lowercase()).collect();
        let at = |name: &str| cols.iter().position(|c| c == name);
        let rows = q.query_map([], |r| {
            let get = |name: &str| at(name).map(|i| r.get_ref(i).map(text)).transpose().map(Option::unwrap_or_default);
            let role_id = at("role").map(|i| r.get_ref(i).map(num)).transpose()?.map(|n| n as i64);
            let role = match role_id {
                Some(id) => roles.get(&id).cloned().unwrap_or_default(),
                None => get("accesslevel")?,
            };
            let flag = |v: String| v == "1" || v.eq_ignore_ascii_case("true");
            Ok(Account {
                banned: flag(get("banned")?) || role_id.is_some_and(|id| Some(id) == banned_role) || role.eq_ignore_ascii_case("banned"),
                username: get("username")?,
                role,
                last: get("lastconnection")?,
                steamid: get("steamid")?,
            })
        })?;
        out.users = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    }
    if has_table(&c, "bannedid")? {
        let mut q = c.prepare("SELECT steamid, reason FROM bannedid")?;
        out.banned_ids = q.query_map([], |r| Ok((text(r.get_ref(0)?), text(r.get_ref(1)?))))?.collect::<rusqlite::Result<_>>()?;
    }
    if has_table(&c, "bannedip")? {
        let mut q = c.prepare("SELECT ip, username, reason FROM bannedip")?;
        out.banned_ips = q.query_map([], |r| Ok((text(r.get_ref(0)?), text(r.get_ref(1)?), text(r.get_ref(2)?))))?.collect::<rusqlite::Result<_>>()?;
    }
    Ok(out)
}

/// Whether the game started at `started` (Unix seconds) ended without closing cleanly: its
/// console.txt doesn't end with the line the game writes on a clean exit, or Java left a crash
/// report (`hs_err_pid*.log`, in the game's folder or the Zomboid folder).
pub fn crashed(zomboid: &Path, game: Option<&Path>, started: u64) -> bool {
    let console = zomboid.join("console.txt");
    // ponytail: the clean-exit line is RenderThread's "GameThread exited." (read in the 42 jar);
    // a console the game didn't write this time says nothing
    let written = fs::metadata(&console).and_then(|m| m.modified()).is_ok_and(|t| secs(t) >= started);
    let tail = fs::read(&console).map(|b| String::from_utf8_lossy(&b[b.len().saturating_sub(4096)..]).into_owned()).unwrap_or_default();
    let report = game.into_iter().chain([zomboid]).flat_map(|d| fs::read_dir(d).into_iter().flatten().flatten()).any(|e| {
        e.file_name().to_str().is_some_and(|n| n.starts_with("hs_err_pid") && n.ends_with(".log")) && e.metadata().and_then(|m| m.modified()).is_ok_and(|t| secs(t) >= started)
    });
    // ponytail: Build 41 has no clean-exit line known here, so only a crash report tells there; read its classes to add one
    let b41 = super::logs::game_version(zomboid).is_some_and(|v| v.starts_with("41."));
    (written && !b41 && !tail.contains("GameThread exited.")) || report
}

/// Backups around a game PZMM started: the last played save now (`save_backup_launch`), every
/// `save_backup_every` minutes the saves played since the last time ("interval"), and once the game
/// has closed, every save played since it started: "exit" (`save_backup_exit`), or "crash" (kept
/// like a slot) when it didn't close cleanly. Blocks until then; one at a time.
pub fn session(paths: &Paths, s: &crate::settings::Settings) {
    static ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let watch = s.save_backup_exit || s.save_backup_every > 0;
    if !(s.save_backup_launch || watch) || ON.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let started = crate::store::now();
    let back_up = |save: &Save, reason: &str| {
        if let Err(e) = back_up(paths, &save.mode, &save.name, reason, "", s.save_backup_keep) {
            crate::say!("saveBackupFailed", "error: couldn't back up save {name}: {error}", name = save.name, error = format!("{e:#}"));
        }
    };
    if s.save_backup_launch
        && let Some(save) = list(paths).first()
    {
        back_up(save, "launch");
    }
    // Steam takes a while to start the game; give up after 3 minutes
    let seen = || {
        let up = super::host::game_running();
        if !up {
            std::thread::sleep(Duration::from_secs(5));
        }
        up
    };
    if watch && (0..36).any(|_| seen()) {
        let every = Duration::from_secs(u64::from(s.save_backup_every) * 60);
        let (mut since, mut last) = (started, std::time::Instant::now());
        while super::host::game_running() {
            std::thread::sleep(Duration::from_secs(10));
            if s.save_backup_every > 0 && last.elapsed() >= every {
                let now = crate::store::now();
                for save in list(paths).iter().filter(|x| x.modified >= since) {
                    back_up(save, "interval");
                }
                (since, last) = (now, std::time::Instant::now());
            }
        }
        let crash = crashed(&paths.zomboid, super::zombiebuddy::game_dir().as_deref(), started);
        if s.save_backup_exit || crash {
            for save in list(paths).iter().filter(|x| x.modified >= started) {
                back_up(save, if crash { "crash" } else { "exit" });
            }
        }
    }
    ON.store(false, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_game_time() {
        let t = tempfile::tempdir().unwrap();
        // the start of a 42.21 server's map_t.bin: 9 July 1993, 09:30
        let mut b = b"GMTM".to_vec();
        for w in [249u32, 0x3f80_0000, 0, 750, 0, 9.5f32.to_bits(), 8, 6, 1993] {
            b.extend(w.to_be_bytes());
        }
        b.extend([0u8; 16]);
        fs::write(t.path().join("map_t.bin"), &b).unwrap();
        assert_eq!(game_time(t.path()), Some([1993, 7, 9, 9, 30]));
        fs::write(t.path().join("map_t.bin"), b"GMTM\0\0").unwrap();
        assert_eq!(game_time(t.path()), None);
        assert_eq!(game_time(&t.path().join("none")), None);
    }

    fn save(paths: &Paths) -> PathBuf {
        let save = paths.zomboid.join("Saves/Sandbox/2026-10-10_12-00-00");
        for (rel, text) in [("mods.txt", "VERSION = 1,\nmods\n{\n    mod = \\ModA,\n}\nmaps\n{\n    map = Muldraugh, KY,\n}\n"), ("map_t.bin", "time"), ("map/12/34.bin", "cell")] {
            let p = save.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        save
    }

    #[test]
    fn lists_saves_with_mods() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().into(), data: t.path().join("data") };
        save(&paths);
        fs::create_dir_all(t.path().join("Saves/Sandbox/no-mods-file")).unwrap();
        let saves = list(&paths);
        assert_eq!(saves.len(), 1);
        assert_eq!((saves[0].mode.as_str(), saves[0].mods.clone()), ("Sandbox", vec!["ModA".to_string()]));
        assert!(dir(&paths, "Sandbox", "..").is_err() && dir(&paths, "../x", "a").is_err());
    }

    #[test]
    fn backups_slots_and_restore() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = save(&paths);
        let (m, n) = ("Sandbox", "2026-10-10_12-00-00");
        let slot = back_up(&paths, m, n, "manual", "Before the raid", 1).unwrap();
        assert_eq!((slot.reason.as_str(), slot.label.as_str()), ("slot", "Before the raid"));
        back_up(&paths, m, n, "manual", "", 1).unwrap();
        let newest = back_up(&paths, m, n, "exit", "", 1).unwrap();
        let left = backups(&paths, m, n).unwrap();
        assert_eq!(left.iter().map(|b| b.reason.as_str()).collect::<Vec<_>>(), ["exit", "slot"], "keep 1 automatic; the slot stays");
        assert!(back_up(&paths, m, n, "manual", "a/b", 1).is_err());

        // play on: a new cell, a changed file; then go back to the slot
        fs::write(s.join("map/12/35.bin"), "new").unwrap();
        fs::write(s.join("map_t.bin"), "later").unwrap();
        restore(&paths, m, n, &slot.file, 1).unwrap();
        assert_eq!(fs::read_to_string(s.join("map_t.bin")).unwrap(), "time");
        assert!(!s.join("map/12/35.bin").exists(), "what came after the backup is gone");
        let after = backups(&paths, m, n).unwrap();
        assert_eq!(after.iter().map(|b| b.reason.as_str()).collect::<Vec<_>>(), ["restore", "slot"], "the state before the restore is kept");
        assert!(!newest.file.exists());
        assert!(restore(&paths, m, n, &s.join("map_t.bin"), 1).is_err(), "only its own backups");
        assert!(fs::read_dir(s.parent().unwrap()).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains(".pzmm-")), "nothing left beside it");

        // the save as it was, left by a restore cut short: a restore never deletes it; at startup
        // it goes once it's an hour old and the save is back in its place
        let old = s.with_file_name(format!("{n}.pzmm-old"));
        fs::create_dir_all(&old).unwrap();
        assert!(format!("{:#}", restore(&paths, m, n, &slot.file, 1).unwrap_err()).contains("earlier restore"));
        sweep(&paths);
        assert!(old.exists(), "not an hour old");
        let mut dir = fs::OpenOptions::new();
        #[cfg(windows)]
        std::os::windows::fs::OpenOptionsExt::custom_flags(dir.write(true), 0x0200_0000); // FILE_FLAG_BACKUP_SEMANTICS: a folder
        #[cfg(not(windows))]
        dir.read(true);
        dir.open(&old).unwrap().set_modified(SystemTime::now() - Duration::from_secs(7200)).unwrap();
        sweep(&paths);
        assert!(!old.exists() && s.is_dir());

        // crash backups: the last few are kept
        for _ in 0..5 {
            back_up(&paths, m, n, "crash", "", 1).unwrap();
        }
        assert_eq!(backups(&paths, m, n).unwrap().iter().filter(|b| b.reason == "crash").count(), CRASHES as usize);

        // a deleted save can come back from its backups
        fs::remove_dir_all(&s).unwrap();
        assert_eq!(deleted_with_backups(&paths), [(m.to_string(), n.to_string())]);
        restore(&paths, m, n, &slot.file, 1).unwrap();
        assert!(s.join("mods.txt").is_file());
        delete_backup(&paths, m, n, &slot.file).unwrap();
        assert!(delete_backup(&paths, m, n, &s.join("mods.txt")).is_err());
    }

    #[test]
    fn backups_are_kept_per_zomboid_folder() {
        let (_t, paths) = crate::store::tests::temp_paths();
        save(&paths);
        let (m, n) = ("Sandbox", "2026-10-10_12-00-00");
        let tagged = back_up(&paths, m, n, "manual", "", 5).unwrap().file.parent().unwrap().to_path_buf();
        assert_eq!(tagged.file_name().unwrap().to_string_lossy(), format!("{n}{}", folder_tag(&paths)));
        // from before the tag: moved to the folder in use
        let legacy = tagged.with_file_name(n);
        fs::rename(&tagged, &legacy).unwrap();
        assert_eq!(backups(&paths, m, n).unwrap().len(), 1);
        assert!(tagged.is_dir() && !legacy.exists());
        // another Zomboid folder with a save of the same name sees none of them
        let other = Paths { zomboid: paths.zomboid.with_file_name("Zomboid2"), data: paths.data.clone() };
        assert!(backups(&other, m, n).unwrap().is_empty());
        assert!(deleted_with_backups(&other).is_empty());
    }

    #[test]
    fn named_resets_copies_and_crashes() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = save(&paths);
        let (m, n) = ("Sandbox", "2026-10-10_12-00-00");
        fs::write(s.join("thumb.png"), "png").unwrap();
        // an automatic backup named becomes a slot; a copy keeps the original
        let auto = back_up(&paths, m, n, "exit", "", 5).unwrap();
        let slot = name_backup(&paths, m, n, &auto.file, "Day 3", false).unwrap();
        assert!(!auto.file.exists() && (slot.reason.as_str(), slot.label.as_str(), slot.time) == ("slot", "Day 3", auto.time));
        let copy = name_backup(&paths, m, n, &slot.file, "Day 3 again", true).unwrap();
        assert!(slot.file.exists() && copy.label == "Day 3 again");
        assert!(name_backup(&paths, m, n, &slot.file, "Day 3", true).is_err(), "taken");
        assert_eq!(thumb(&paths, m, n, Some(&copy.file)).unwrap(), b"png");
        assert_eq!(thumb(&paths, m, n, None).unwrap(), b"png");
        assert!(thumb(&paths, m, n, Some(&s.join("thumb.png"))).is_err(), "only its own backups");

        // resets remove only what the game rebuilds, after a backup
        for rel in ["zpop_virtual.bin", "zpop/zpop_10_20.bin", "zpop_3_4.bin", "apop/apop_-1_2.bin", "map_animals.bin", "zpop/notes.txt", "map_zone.bin"] {
            let p = s.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        assert_eq!(reset_files(&s, "zombies"), ["zpop/zpop_10_20.bin", "zpop_3_4.bin", "zpop_virtual.bin"]);
        assert_eq!(resets(&s), [("animals", 2), ("time", 1), ("zombies", 3)].into_iter().collect());
        assert_eq!(reset(&paths, m, n, "zombies", 5).unwrap(), 3);
        assert!(s.join("zpop/notes.txt").exists() && s.join("map_zone.bin").exists() && !s.join("zpop_virtual.bin").exists());
        assert_eq!(backups(&paths, m, n).unwrap()[0].reason, "reset");
        assert!(reset(&paths, m, n, "players", 5).is_err());

        // a copy with other mods; the original keeps its own
        assert_eq!(clone_with_mods(&paths, m, n, &["ModB".into()], &[]).unwrap(), format!("{n}_modded"));
        assert_eq!(clone_with_mods(&paths, m, n, &["ModC".into()], &[]).unwrap(), format!("{n}_modded 2"));
        assert_eq!(read_default(&s.join("mods.txt")).mods, ["ModA"]);
        assert_eq!(read_default(&dir(&paths, m, &format!("{n}_modded")).unwrap().join("mods.txt")).mods, ["ModB"]);

        // a crash: no clean-exit line, or a crash report written since the start
        let (z, g) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        fs::write(z.path().join("console.txt"), "LOG  : General      f:0> GameThread exited.\n").unwrap();
        assert!(!crashed(z.path(), Some(g.path()), 0));
        fs::write(g.path().join("hs_err_pid42.log"), "#").unwrap();
        assert!(crashed(z.path(), Some(g.path()), 0) && !crashed(z.path(), Some(g.path()), u64::MAX));
        fs::write(z.path().join("console.txt"), "LOG  : General      f:0> loading\n").unwrap();
        assert!(crashed(z.path(), None, 0));
        assert!(!crashed(z.path(), None, u64::MAX), "a console from an earlier run says nothing");
        fs::write(z.path().join("console.txt"), "LOG  : General      f:0> version=41.78.16 demo=false\nLOG  : General      f:0> loading\n").unwrap();
        assert!(!crashed(z.path(), None, 0), "Build 41: only a crash report");
    }

    #[test]
    fn broken_cells_are_repaired_after_a_backup() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = save(&paths);
        for rel in ["map/12/36.bin", "map_40_22.bin", "chunkdata/chunkdata_1_2.bin", "map_meta.bin"] {
            let p = s.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "").unwrap();
        }
        let b = broken(&s);
        assert_eq!(b.cells, ["chunkdata/chunkdata_1_2.bin", "map/12/36.bin", "map_40_22.bin"]);
        assert_eq!(b.other, ["map_meta.bin"], "only a backup brings this back");
        assert_eq!(repair(&paths, "Sandbox", "2026-10-10_12-00-00", 5).unwrap(), 3);
        assert!(broken(&s).cells.is_empty() && s.join("map_meta.bin").exists() && s.join("map/12/34.bin").exists());
        assert_eq!(backups(&paths, "Sandbox", "2026-10-10_12-00-00").unwrap()[0].reason, "repair");
    }

    #[test]
    fn changes_a_saves_mods_and_keeps_its_maps() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = save(&paths);
        set_mods(&paths, "Sandbox", "2026-10-10_12-00-00", &["ModB".into(), "ModC".into()], &[]).unwrap();
        let now = read_default(&s.join("mods.txt"));
        assert_eq!(now.mods, ["ModB", "ModC"]);
        assert_eq!(now.map_folders(), ["Muldraugh, KY"]);
        assert_eq!(read_default(&s.join("mods.txt.prev")).mods, ["ModA"]);
        // the profile's maps the save lacks join it, before Muldraugh; the ones it has keep their place
        let maps = |v: &[&str]| v.iter().map(|m| m.to_string()).collect::<Vec<_>>();
        set_mods(&paths, "Sandbox", "2026-10-10_12-00-00", &["ModB".into()], &maps(&["Town", "Fort"])).unwrap();
        set_mods(&paths, "Sandbox", "2026-10-10_12-00-00", &["ModB".into()], &maps(&["Fort", "Lake", "Town"])).unwrap();
        assert_eq!(read_default(&s.join("mods.txt")).map_folders(), ["Town", "Fort", "Lake", "Muldraugh, KY"]);
        assert!(set_mods(&paths, "Sandbox", "missing", &[], &[]).is_err());
    }

    #[test]
    fn reads_players_db() {
        let t = tempfile::tempdir().unwrap();
        let c = rusqlite::Connection::open(t.path().join("players.db")).unwrap();
        // the game's own schema (zombie/savefile/PlayerDB)
        c.execute_batch("CREATE TABLE localPlayers (id   INTEGER PRIMARY KEY NOT NULL,name STRING,wx    INTEGER,wy    INTEGER,x    FLOAT,y    FLOAT,z    FLOAT,worldversion    INTEGER,data BLOB,isDead BOOLEAN);
            INSERT INTO localPlayers VALUES (1, 'Kate Smith', 36, 30, 10850.5, 9100.25, 0, 249, x'00010203', 0);
            INSERT INTO localPlayers VALUES (2, '1234', 36, 30, 1, 2, 1, 249, NULL, 1);").unwrap();
        drop(c);
        let p = players(t.path()).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].name.as_str(), p[0].x, p[0].dead, p[0].data_size), ("Kate Smith", 10850.5, false, 4));
        assert_eq!((p[1].name.as_str(), p[1].dead), ("1234", true), "STRING columns hold numbers as numbers");
        assert!(players(&t.path().join("none")).is_err());

        // the browser: every database and table, rows as text, data as its size
        assert_eq!(databases(t.path()), [("players.db".to_string(), vec![("localPlayers".to_string(), 2)])]);
        // read in place, while something holds it open; any folder name
        let odd = t.path().join("50% #1");
        fs::create_dir_all(&odd).unwrap();
        let w = rusqlite::Connection::open(odd.join("vehicles.db")).unwrap();
        w.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE vehicles (id INTEGER); INSERT INTO vehicles VALUES (1);").unwrap();
        let want = [("vehicles.db".to_string(), vec![("vehicles".to_string(), 1)])];
        assert_eq!(databases(&odd), want, "with its write-ahead log");
        drop(w);
        assert!(!odd.join("vehicles.db-wal").exists());
        assert_eq!(databases(&odd), want, "in place");
        let rows = table(t.path(), "players.db", "localPlayers", 1, 10).unwrap();
        assert_eq!(rows.columns[..2], ["id", "name"]);
        assert_eq!(rows.rows, [serde_json::json!([2, 1234, 36, 30, 1.0, 2.0, 1.0, 249, null, 1]).as_array().unwrap().clone()]);
        assert_eq!(table(t.path(), "players.db", "localPlayers", 0, 1).unwrap().rows[0][8], serde_json::json!({ "bytes": 4 }));
        assert!(table(t.path(), "players.db", "nope", 0, 10).is_err(), "only its own tables");
        assert!(table(t.path(), "../x.db", "localPlayers", 0, 10).is_err(), "only its own files");
    }

    #[test]
    fn reads_a_servers_accounts_and_bans() {
        let t = tempfile::tempdir().unwrap();
        let db = t.path().join("servertest.db");
        let c = rusqlite::Connection::open(&db).unwrap();
        // Build 42's schema (zombie/network/ServerWorldDatabase)
        c.execute_batch("CREATE TABLE [whitelist] ([id] INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,[world] TEXT DEFAULT '' NULL,[username] TEXT  NULL, [password] TEXT  NULL, [lastConnection] TEXT NULL, [role] INTEGER NOT NULL, [authType] INTEGER NULL DEFAULT 1, [googleKey] TEXT NULL, [steamid] TEXT NULL, [ownerid] TEXT NULL, [displayName] TEXT NULL);
            CREATE TABLE [role] ([id] INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, [name] TEXT NOT NULL,[description] TEXT NULL, [colorR] REAL NOT NULL, [colorG] REAL NOT NULL, [colorB] REAL NOT NULL, [readonly] BOOLEAN NULL DEFAULT false, [position] INTEGER NOT NULL DEFAULT -1 );
            CREATE TABLE [defaultRoles] ([name] TEXT NOT NULL,[role] INTEGER);
            CREATE TABLE [bannedid] ([steamid] TEXT NOT NULL, [reason] TEXT NULL);
            CREATE TABLE [bannedip] ([ip] TEXT NOT NULL,[username] TEXT NULL, [reason] TEXT NULL);
            INSERT INTO role (id, name, colorR, colorG, colorB) VALUES (1, 'admin', 1, 0, 0), (2, 'user', 1, 1, 1), (3, 'banned', 0, 0, 0);
            INSERT INTO defaultRoles VALUES ('banned', 3), ('user', 2);
            INSERT INTO whitelist (username, lastConnection, role, steamid) VALUES ('admin', '2026-10-01 10:00:00', 1, ''), ('Kate', '', 3, '76561190000000001');
            INSERT INTO bannedid VALUES ('76561190000000002', 'griefing');
            INSERT INTO bannedip VALUES ('10.0.0.9', 'Kate', NULL);").unwrap();
        drop(c);
        let a = accounts(&db).unwrap();
        let users: Vec<(&str, &str, bool)> = a.users.iter().map(|u| (u.username.as_str(), u.role.as_str(), u.banned)).collect();
        assert_eq!(users, [("admin", "admin", false), ("Kate", "banned", true)]);
        assert_eq!(a.users[0].last, "2026-10-01 10:00:00");
        assert_eq!(a.banned_ids, [("76561190000000002".to_string(), "griefing".to_string())]);
        assert_eq!(a.banned_ips, [("10.0.0.9".to_string(), "Kate".to_string(), String::new())]);
        // Build 41: the ban is a column, the level text
        let db41 = t.path().join("old.db");
        rusqlite::Connection::open(&db41).unwrap().execute_batch("CREATE TABLE whitelist (id INTEGER PRIMARY KEY, world TEXT, username TEXT, password TEXT, admin BOOLEAN, moderator BOOLEAN, banned BOOLEAN, priority BOOLEAN, lastConnection TEXT, accesslevel TEXT, steamid TEXT);
            INSERT INTO whitelist (username, banned, accesslevel) VALUES ('Old', 'true', 'moderator'), ('Fine', 'false', '');").unwrap();
        let users: Vec<(String, String, bool)> = accounts(&db41).unwrap().users.into_iter().map(|u| (u.username, u.role, u.banned)).collect();
        assert_eq!(users, [("Old".into(), "moderator".into(), true), ("Fine".into(), String::new(), false)]);
        assert!(accounts(&t.path().join("none.db")).is_err());
    }
}
