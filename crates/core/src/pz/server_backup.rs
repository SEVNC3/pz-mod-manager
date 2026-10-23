//! Server backups PZMM takes: the save (`Saves/Multiplayer/<name>`), its database (`db/<name>.db`)
//! and its configs (`Server/<name>.ini`, `_SandboxVars.lua`, `_spawnregions.lua`, `_spawnpoints.lua`),
//! zipped into `<PZMM data>/server-backups/<name>/<unix ms>-<reason>.zip`, paths relative to the
//! server's data folder. The server's own `BackupsOnStart` backups (in `<data folder>/backups`) are separate.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::Paths;
use super::host::Server;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Backup {
    pub file: PathBuf,
    /// Unix seconds.
    pub time: u64,
    pub size: u64,
    /// start, update, scheduled, manual, restore (before a restore)...
    pub reason: String,
    /// A name the player gave it (save slots); "" for none.
    pub label: String,
}

pub fn dir(paths: &Paths, name: &str) -> PathBuf {
    paths.data.join("server-backups").join(name)
}

/// A server's backups, newest first.
pub fn list(paths: &Paths, name: &str) -> Vec<Backup> {
    list_in(&dir(paths, name))
}

/// The `<unix ms>-<reason>[-<label>].zip` backups in `dir`, newest first.
pub(crate) fn list_in(dir: &Path) -> Vec<Backup> {
    let mut out: Vec<Backup> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let file = e.path();
            let (ms, rest) = file.file_name()?.to_str()?.strip_suffix(".zip")?.split_once('-')?;
            let ms: u64 = ms.parse().ok()?;
            let (reason, label) = rest.split_once('-').unwrap_or((rest, ""));
            Some(Backup { time: ms / 1000, size: e.metadata().ok()?.len(), reason: reason.into(), label: label.into(), file })
        })
        .collect();
    out.sort_by(|a, b| b.file.cmp(&a.file));
    out
}

/// What a backup holds, relative to the data folder: folders, then files.
fn parts(name: &str) -> (Vec<String>, Vec<String>) {
    let dirs = vec![format!("Saves/Multiplayer/{name}"), format!("Saves/Multiplayer/{name}_player")];
    let files = ["{}.ini", "{}_SandboxVars.lua", "{}_spawnregions.lua", "{}_spawnpoints.lua"].iter().map(|f| format!("Server/{}", f.replace("{}", name))).chain([format!("db/{name}.db")]).collect();
    (dirs, files)
}

/// Whether a zip entry is something a backup of `name` may hold (anything else is refused on restore).
fn covered(name: &str, rel: &str) -> bool {
    // Checked on the raw name, while extracting follows `enclosed_name`, which resolves `..`: refuse
    // any `..`, `.` or backslash so the two can't disagree.
    if rel.contains('\\') ||rel.split('/').any(|c| c == ".." || c == "." || c.is_empty()) {
        return false;
    }
    let (dirs, files) = parts(name);
    files.iter().any(|f| f == rel) || dirs.iter().any(|d| rel.strip_prefix(d.as_str()).is_some_and(|r| r.starts_with('/')))
}

pub(crate) fn walk(root: &Path, rel: &str, out: &mut Vec<(PathBuf, String)>) {
    for e in fs::read_dir(root.join(rel)).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let r = if rel.is_empty() { n } else { format!("{rel}/{n}") };
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(root, &r, out),
            Ok(t) if t.is_file() => out.push((e.path(), r)),
            _ => {}
        }
    }
}

/// Zip the server's save, database and configs into `to`. False when there was nothing to back up.
fn zip_state(cachedir: &Path, name: &str, to: &Path) -> Result<bool> {
    let (dirs, files) = parts(name);
    let mut items = vec![];
    for d in &dirs {
        walk(cachedir, d, &mut items);
    }
    items.extend(files.iter().map(|f| (cachedir.join(f), f.clone())).filter(|(p, _)| p.is_file()));
    zip_items(&items, to)
}

/// Zip `items` (file, path in the zip) into `to`, via a temp file. False when there's nothing.
pub(crate) fn zip_items(items: &[(PathBuf, String)], to: &Path) -> Result<bool> {
    if items.is_empty() {
        return Ok(false);
    }
    fs::create_dir_all(to.parent().context("backup folder")?)?;
    let tmp = to.with_extension("zip.pzmm-part");
    let result = (|| -> Result<()> {
        let mut z = zip::ZipWriter::new(fs::File::create(&tmp)?);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
        for (path, rel) in items {
            // a file the server holds open with no sharing (rare) fails the backup rather than leaving a hole
            let mut f = fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
            z.start_file(rel.as_str(), opts)?;
            std::io::copy(&mut f, &mut z)?;
        }
        z.finish()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, to)?;
    Ok(true)
}

fn stamp() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

/// A new backup's file in `dir`: `<unix ms>-<tag>.zip`.
pub(crate) fn new_file(dir: &Path, tag: &str) -> PathBuf {
    // a millisecond of its own, whatever the tag: two in the same one would sort by tag, not age
    let taken = |ms: u128| fs::read_dir(dir).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().starts_with(&format!("{ms}-")));
    let mut ms = stamp();
    while taken(ms) {
        std::thread::sleep(std::time::Duration::from_millis(2));
        ms = stamp();
    }
    dir.join(format!("{ms}-{tag}.zip"))
}

fn take(paths: &Paths, s: &Server, reason: &str) -> Result<Option<Backup>> {
    let file = new_file(&dir(paths, &s.name), reason);
    if !zip_state(&s.cachedir(paths), &s.name, &file)? {
        return Ok(None);
    }
    Ok(list(paths, &s.name).into_iter().find(|b| b.file == file))
}

/// Back the server up now, then apply its retention.
pub fn create(paths: &Paths, s: &Server, reason: &str) -> Result<Backup> {
    let b = take(paths, s, reason)?.with_context(|| format!("{} has no save or configs to back up yet", s.name))?;
    crate::say!("serverBackedUp", "Backed up {name} ({size} MB)", name = s.name, size = format!("{:.1}", b.size as f64 / 1048576.0));
    prune_now(paths, s, None);
    Ok(b)
}

/// Retention: the automatic backups beyond the server's limits go. The player's own ("manual",
/// "import") never do, nor `keep` (the backup just restored).
fn prune_now(paths: &Paths, s: &Server, keep: Option<&Path>) {
    let auto: Vec<Backup> = list(paths, &s.name).into_iter().filter(|b| !["manual", "import"].contains(&b.reason.as_str()) && Some(b.file.as_path()) != keep).collect();
    for f in prune(&auto, s.auto.backup_keep, s.auto.backup_max_mb << 20) {
        let _ = fs::remove_file(f);
    }
}

/// The backups (newest first) to delete: beyond `keep` (0: no count limit), then the oldest while
/// they take more than `max_bytes` (0: no cap). The newest always stays.
pub fn prune(list: &[Backup], keep: u32, max_bytes: u64) -> Vec<PathBuf> {
    let mut kept = list.len();
    if keep > 0 {
        kept = kept.min(keep as usize);
    }
    if max_bytes > 0 {
        while kept > 1 && list[..kept].iter().map(|b| b.size).sum::<u64>() > max_bytes {
            kept -= 1;
        }
    }
    list[kept.max(1).min(list.len())..].iter().map(|b| b.file.clone()).collect()
}

/// Put backup `file` back. The server must be stopped (the caller stops it). The current state is
/// backed up first ("restore"); retention never takes the backup being restored.
pub fn restore(paths: &Paths, s: &Server, file: &Path) -> Result<()> {
    if file.parent() != Some(dir(paths, &s.name).as_path()) {
        bail!("{} isn't one of {}'s backups", file.display(), s.name);
    }
    let mut z = zip::ZipArchive::new(fs::File::open(file).with_context(|| format!("opening {}", file.display()))?)?;
    // check every entry before anything changes
    for i in 0..z.len() {
        let e = z.by_index(i)?;
        let name = e.name()?.to_string();
        let ok = e.enclosed_name().is_some() && covered(&s.name, name.trim_end_matches('/'));
        if !ok && (!e.is_dir() || name.split(['/', '\\']).any(|c| c == "..")) {
            bail!("{} holds {name}, which isn't part of a server backup: nothing was changed", file.display());
        }
    }
    super::host::check_stopped(paths, s)?;
    let _held = super::host::reserve(&s.name)?;
    let cachedir = s.cachedir(paths);
    // the server's files as they were before an earlier restore that didn't finish: never deleted here
    let old = cachedir.join(".pzmm-old");
    if old.exists() {
        bail!("{} is left from an earlier restore (the server's files as they were then): move it out or delete it, then try again", old.display());
    }
    take(paths, s, "restore")?;
    // unpacked beside the files (one drive, so they rename), then swapped in: a failure part way
    // leaves the server as it was
    let new = cachedir.join(".pzmm-restoring");
    let remove = |p: &Path| if p.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) };
    if new.exists() {
        remove(&new).with_context(|| format!("removing {}", new.display()))?;
    }
    let unpacked = (|| -> Result<()> {
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
        let _ = remove(&new);
        return Err(e.context(format!("unpacking {}: the server wasn't changed", file.display())));
    }
    let (dirs, files) = parts(&s.name);
    let (mut aside, mut placed) = (vec![], vec![]);
    let swapped = dirs.iter().chain(&files).try_for_each(|part| -> std::io::Result<()> {
        let (now, was, restored) = (cachedir.join(part), old.join(part), new.join(part));
        if now.exists() {
            fs::create_dir_all(was.parent().unwrap_or(&old))?;
            fs::rename(&now, &was)?;
            aside.push(part);
        }
        if restored.exists() {
            fs::create_dir_all(now.parent().unwrap_or(&cachedir))?;
            fs::rename(&restored, &now)?;
            placed.push(part);
        }
        Ok(())
    });
    if let Err(e) = swapped {
        // back as it was
        for part in placed {
            let _ = remove(&cachedir.join(part));
        }
        let back = aside.iter().all(|part| fs::rename(old.join(part), cachedir.join(part)).is_ok());
        let _ = remove(&new);
        if !back {
            return Err(e).with_context(|| format!("putting {}'s files in place (is something using them?): the files as they were are in {}", s.name, old.display()));
        }
        let _ = fs::remove_dir_all(&old);
        return Err(e).with_context(|| format!("putting {}'s files in place (is something using them?): they weren't changed", s.name));
    }
    let _ = remove(&new);
    let _ = fs::remove_dir_all(&old);
    crate::say!("serverRestored", "Restored {name} from {file}", name = s.name, file = file.display());
    prune_now(paths, s, Some(file));
    Ok(())
}

/// Copy backup `file` of server `name` into `dir` as `<name>-<its file name>`; returns the copy.
pub fn export(paths: &Paths, name: &str, file: &Path, dir: &Path) -> Result<PathBuf> {
    if file.parent() != Some(self::dir(paths, name).as_path()) {
        bail!("{} isn't one of {name}'s backups", file.display());
    }
    let to = dir.join(format!("{name}-{}", file.file_name().context("backup file")?.to_string_lossy()));
    fs::copy(file, &to).with_context(|| format!("copying to {}", to.display()))?;
    Ok(to)
}

/// Bring in a server backup zip from elsewhere (an exported backup of this or another server):
/// its entries are renamed to this server, and anything a server backup doesn't hold refuses the
/// whole file. It joins the list as an "import" backup, ready to restore.
pub fn import(paths: &Paths, s: &Server, zip_file: &Path) -> Result<Backup> {
    let mut z = zip::ZipArchive::new(fs::File::open(zip_file).with_context(|| format!("opening {}", zip_file.display()))?)?;
    let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).map(|e| e.name().unwrap_or_default().to_string())).collect::<Result<_, _>>()?;
    // the server it was taken from: by its ini, else its world
    let from = names
        .iter()
        .find_map(|n| n.strip_prefix("Server/")?.strip_suffix(".ini").map(String::from))
        .or_else(|| names.iter().find_map(|n| n.strip_prefix("Saves/Multiplayer/")?.split('/').next().map(|w| w.trim_end_matches("_player").to_string())))
        .with_context(|| format!("{} isn't a server backup (no Server/<name>.ini or Saves/Multiplayer/<name>)", zip_file.display()))?;
    let rename = |n: &str| -> Option<String> {
        let to = &s.name;
        for (a, b) in [(format!("Saves/Multiplayer/{from}_player/"), format!("Saves/Multiplayer/{to}_player/")), (format!("Saves/Multiplayer/{from}/"), format!("Saves/Multiplayer/{to}/")), (format!("Server/{from}"), format!("Server/{to}")), (format!("db/{from}."), format!("db/{to}."))] {
            if let Some(rest) = n.strip_prefix(a.as_str()) {
                return Some(format!("{b}{rest}"));
            }
        }
        None
    };
    let mut items = vec![];
    for (i, n) in names.iter().enumerate() {
        if n.ends_with('/') {
            continue;
        }
        let rel = rename(n).filter(|r| z.by_index(i).is_ok_and(|e| e.enclosed_name().is_some()) && covered(&s.name, r));
        items.push((i, rel.with_context(|| format!("{} holds {n}, which isn't part of a server backup: nothing was imported", zip_file.display()))?));
    }
    if items.is_empty() {
        bail!("{} is empty", zip_file.display());
    }
    let to = new_file(&dir(paths, &s.name), "import");
    fs::create_dir_all(to.parent().context("backup folder")?)?;
    let tmp = to.with_extension("zip.pzmm-part");
    let result = (|| -> Result<()> {
        let mut w = zip::ZipWriter::new(fs::File::create(&tmp)?);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
        let (ini, regions) = (format!("Server/{}.ini", s.name), format!("Server/{}_spawnregions.lua", s.name));
        for (i, rel) in &items {
            w.start_file(rel.as_str(), opts)?;
            if from != s.name && (*rel == ini || *rel == regions) {
                let mut text = String::new();
                std::io::Read::read_to_string(&mut z.by_index(*i)?, &mut text)?;
                std::io::Write::write_all(&mut w, own_settings(paths, s, &from, rel == &ini, &text).as_bytes())?;
            } else {
                std::io::copy(&mut z.by_index(*i)?, &mut w)?;
            }
        }
        w.finish()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, &to)?;
    crate::say!("serverBackupImported", "Imported a backup for {name} from {file}", name = s.name, file = zip_file.display());
    list(paths, &s.name).into_iter().find(|b| b.file == to).context("the imported backup")
}

/// Another server's ini or spawn regions, made `s`'s: its spawn points file by `s`'s name, and its RCON port and
/// password `s`'s own (else new ones): shared ones would send PZMM's commands for one server to the other.
fn own_settings(paths: &Paths, s: &Server, from: &str, ini: bool, text: &str) -> String {
    if !ini {
        return text.replace(&format!("\"{from}_spawnpoints.lua\""), &format!("\"{}_spawnpoints.lua\"", s.name));
    }
    let mine = fs::read_to_string(s.ini(paths)).unwrap_or_default();
    let keep = |key: &str, new: &dyn Fn() -> String| Some(super::server::value(&mine, key)).filter(|v| !v.is_empty()).map(String::from).unwrap_or_else(new);
    let port = keep("RCONPort", &|| super::host::free_rcon_port(paths).to_string());
    let password = if super::server::value(text, "RCONPassword").is_empty() { String::new() } else { keep("RCONPassword", &super::host::generate_password) };
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    text.lines()
        .map(|l| match l.split_once('=').map(|(k, _)| k) {
            Some("RCONPort") => format!("RCONPort={port}"),
            Some("RCONPassword") => format!("RCONPassword={password}"),
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join(eol)
        + eol
}

/// What a wipe leaves out of a save: the map (chunks, vehicles, zombies, animals) and, unless kept,
/// the characters. `keep_zones` keeps everything else, so the safehouses, safe zones, non-PvP zones
/// and factions in `map_meta.bin` stay; without it only the characters can stay.
fn wiped(rel: &str, keep_characters: bool, keep_zones: bool) -> bool {
    let top = rel.split('/').next().unwrap_or(rel);
    let numbered = |prefix: &str| top.strip_prefix(prefix).and_then(|r| r.strip_suffix(".bin")).is_some_and(|xy| xy.split('_').all(|n| n.parse::<i64>().is_ok()));
    let characters = top == "players.db";
    // inventories name items by the numbers in the world dictionary
    let dictionary = top.starts_with("WorldDictionary");
    let map = ["map", "chunkdata", "zpop", "apop", "isoregiondata", "map_visited_server"].contains(&if top != rel { top } else { "" })
        || ["vehicles.db", "map_animals.bin", "reanimated.bin"].contains(&top)
        || numbered("map_")
        || numbered("zpop_")
        || numbered("chunkdata_");
    if characters {
        return !keep_characters;
    }
    if keep_zones { map } else { !(keep_characters && dictionary) }
}

/// Reset a stopped server's world after a backup ("wipe"): see [`wiped`]. The host's co-op
/// character folder goes unless characters are kept; accounts (`db/`) and settings stay. Returns
/// the files removed.
pub fn wipe(paths: &Paths, s: &Server, keep_characters: bool, keep_zones: bool) -> Result<usize> {
    super::host::check_stopped(paths, s)?;
    let _held = super::host::reserve(&s.name)?;
    let saves = s.cachedir(paths).join("Saves/Multiplayer");
    let save = saves.join(&s.name);
    let player = saves.join(format!("{}_player", s.name));
    if save.exists() || player.exists() {
        create(paths, s, "wipe")?;
    }
    let mut all = vec![];
    walk(&save, "", &mut all);
    let mut n = 0;
    for (p, rel) in all.iter().filter(|(_, r)| wiped(r, keep_characters, keep_zones)) {
        fs::remove_file(p).with_context(|| format!("removing {rel}"))?;
        n += 1;
    }
    // the folders left empty
    for d in fs::read_dir(&save).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        let mut left = vec![];
        walk(&save, &d.file_name().to_string_lossy(), &mut left);
        if left.is_empty() {
            let _ = fs::remove_dir_all(d.path());
        }
    }
    if !keep_characters && player.exists() {
        fs::remove_dir_all(&player).with_context(|| format!("removing {}", player.display()))?;
    }
    crate::say!("serverWiped", "Wiped {name}'s world ({count} files)", name = s.name, count = n);
    Ok(n)
}

/// The save file holding the safehouses, safe zones, non-PvP zones and factions (the game's
/// `IsoMetaGrid` writes them there), together with every building's and room's flags (alarm,
/// visited, looted, lights) and, in Build 42, animal designation zones and stashes.
const ZONES: &str = "map_meta.bin";

/// The zone backups (`map_meta.bin` alone), apart from the full ones.
fn zones_dir(paths: &Paths, name: &str) -> PathBuf {
    dir(paths, name).join("zones")
}

pub fn zones_list(paths: &Paths, name: &str) -> Vec<Backup> {
    list_in(&zones_dir(paths, name))
}

/// Back up the save's [`ZONES`] file alone (it may run: the file as the last world save left it).
pub fn zones_save(paths: &Paths, s: &Server, reason: &str) -> Result<Backup> {
    let file = s.cachedir(paths).join("Saves/Multiplayer").join(&s.name).join(ZONES);
    if !file.is_file() {
        bail!("{} has no {ZONES} yet: the world makes it on its first save", s.name);
    }
    let to = new_file(&zones_dir(paths, &s.name), reason);
    zip_items(&[(file, ZONES.to_string())], &to)?;
    zones_list(paths, &s.name).into_iter().find(|b| b.file == to).context("the zone backup")
}

/// Put a zone backup's [`ZONES`] back into the stopped server's save, backing up the current one
/// first ("restore"); the rest of the world stays as it is.
pub fn zones_restore(paths: &Paths, s: &Server, file: &Path) -> Result<()> {
    if file.parent() != Some(zones_dir(paths, &s.name).as_path()) {
        bail!("{} isn't one of {}'s zone backups", file.display(), s.name);
    }
    super::host::check_stopped(paths, s)?;
    let _held = super::host::reserve(&s.name)?;
    let mut z = zip::ZipArchive::new(fs::File::open(file).with_context(|| format!("opening {}", file.display()))?)?;
    if z.len() != 1 {
        bail!("{} isn't a zone backup: nothing was changed", file.display());
    }
    let mut bytes = vec![];
    std::io::Read::read_to_end(&mut z.by_name(ZONES).with_context(|| format!("{} holds no {ZONES}: nothing was changed", file.display()))?, &mut bytes)?;
    let save = s.cachedir(paths).join("Saves/Multiplayer").join(&s.name);
    if save.join(ZONES).is_file() {
        zones_save(paths, s, "restore")?;
    }
    fs::create_dir_all(&save)?;
    crate::write_atomic(&save.join(ZONES), bytes)?;
    crate::say!("serverZonesRestored", "Restored {name}'s safehouses, zones and factions from {file}", name = s.name, file = file.display());
    Ok(())
}

/// A map chunk file in a save and the square of tiles it holds: Build 41's `map_X_Y.bin`
/// (10 tiles a side), Build 42's `map/X/Y.bin` (8).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Chunk {
    pub rel: String,
    pub x: i64,
    pub y: i64,
    pub size: i64,
}

pub fn chunks(save: &Path) -> Vec<Chunk> {
    let mut all = vec![];
    walk(save, "", &mut all);
    let mut out: Vec<Chunk> = all
        .into_iter()
        .filter_map(|(_, rel)| {
            let parts: Vec<&str> = rel.strip_suffix(".bin")?.split(['/', '_']).collect();
            let (x, y, size) = match parts.as_slice() {
                ["map", x, y] if rel.starts_with("map/") => (x, y, 8),
                ["map", x, y] => (x, y, 10),
                _ => return None,
            };
            Some(Chunk { x: x.parse::<i64>().ok()? * size, y: y.parse::<i64>().ok()? * size, size, rel })
        })
        .collect();
    out.sort_by_key(|c| (c.x, c.y));
    out
}

/// A rectangle of tiles (corners included, in either order), as the game's coordinates show them.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Area {
    pub x1: i64,
    pub y1: i64,
    pub x2: i64,
    pub y2: i64,
}

impl Area {
    fn touches(&self, c: &Chunk) -> bool {
        let (x1, x2, y1, y2) = (self.x1.min(self.x2), self.x1.max(self.x2), self.y1.min(self.y2), self.y1.max(self.y2));
        c.x <= x2 && c.x + c.size > x1 && c.y <= y2 && c.y + c.size > y1
    }
}

/// The chunks a reset takes: those touching any of `areas` (`inside`), else those touching none.
pub fn to_reset<'a>(all: &'a [Chunk], areas: &[Area], inside: bool) -> Vec<&'a Chunk> {
    all.iter().filter(|c| areas.iter().any(|a| a.touches(c)) == inside).collect()
}

/// Back up, then delete chunk files of a stopped server's world (see [`to_reset`]): the game makes
/// them fresh from the map when they next load. Returns how many went.
pub fn reset_chunks(paths: &Paths, s: &Server, areas: &[Area], inside: bool) -> Result<usize> {
    super::host::check_stopped(paths, s)?;
    let _held = super::host::reserve(&s.name)?;
    if inside && areas.is_empty() {
        return Ok(0);
    }
    let save = s.cachedir(paths).join("Saves/Multiplayer").join(&s.name);
    let all = chunks(&save);
    let gone = to_reset(&all, areas, inside);
    if gone.is_empty() {
        return Ok(0);
    }
    create(paths, s, "chunks")?;
    for c in &gone {
        fs::remove_file(save.join(&c.rel)).with_context(|| format!("removing {}", c.rel))?;
    }
    crate::say!("serverChunksReset", "Reset {count} map chunks of {name}", count = gone.len(), name = s.name);
    Ok(gone.len())
}

/// Zero-byte map files in a save (`.bin`: cells, chunks, zombie population), which the game can't
/// load; relative paths. A crash while saving leaves them.
pub fn empty_cells(save: &Path) -> Vec<String> {
    let mut all = vec![];
    walk(save, "", &mut all);
    let mut out: Vec<String> = all
        .into_iter()
        .filter(|(p, _)| p.extension().is_some_and(|x| x == "bin") && p.metadata().is_ok_and(|m| m.len() == 0))
        .map(|(_, r)| r)
        .collect();
    out.sort();
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The tests that change a server's files take turns: host::reserve holds a server by its name,
    /// and they all use "Friends".
    static FRIENDS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub(crate) fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        FRIENDS.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn server(paths: &Paths) -> Server {
        let s = Server { name: "Friends".into(), cachedir: Some(paths.zomboid.clone()), ..Default::default() };
        let w = |rel: &str, text: &str| {
            let p = paths.zomboid.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        };
        w("Server/Friends.ini", "PVP=true\n");
        w("Server/Friends_SandboxVars.lua", "SandboxVars = {\n}\n");
        w("Server/Friends2.ini", "other server\n");
        w("Saves/Multiplayer/Friends/map/12/34.bin", "cell");
        w("Saves/Multiplayer/Friends2/map_t.bin", "other");
        w("db/Friends.db", "db");
        s
    }

    #[test]
    fn backs_up_only_the_servers_files() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = server(&paths);
        let b = create(&paths, &s, "manual").unwrap();
        assert_eq!(b.reason, "manual");
        let mut z = zip::ZipArchive::new(fs::File::open(&b.file).unwrap()).unwrap();
        let mut names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().to_string()).collect();
        names.sort();
        assert_eq!(names, ["Saves/Multiplayer/Friends/map/12/34.bin", "Server/Friends.ini", "Server/Friends_SandboxVars.lua", "db/Friends.db"]);
        assert_eq!(list(&paths, "Friends"), [b]);
        let empty = Server { name: "Nobody".into(), cachedir: Some(paths.zomboid.clone()), ..Default::default() };
        assert!(create(&paths, &empty, "manual").is_err(), "nothing to back up");
        assert!(!dir(&paths, "Nobody").join("x").exists());
    }

    #[test]
    fn retention_by_count_and_size() {
        let b = |n: u64, size: u64| Backup { file: format!("{n}.zip").into(), time: n, size, reason: String::new(), label: String::new() };
        let all = [b(5, 100), b(4, 100), b(3, 300), b(2, 100), b(1, 100)];
        let names = |v: Vec<PathBuf>| v.into_iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
        assert_eq!(names(prune(&all, 3, 0)), ["2.zip", "1.zip"]);
        assert_eq!(names(prune(&all, 0, 450)), ["3.zip", "2.zip", "1.zip"], "the oldest go until it fits");
        assert_eq!(names(prune(&all, 10, 50)), ["4.zip", "3.zip", "2.zip", "1.zip"], "the newest always stays");
        assert!(prune(&all, 0, 0).is_empty());
        assert!(prune(&[], 1, 1).is_empty());
    }

    #[test]
    fn restore_backs_up_first_and_puts_it_back() {
        let _one = one_at_a_time();
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = Server { auto: crate::pz::autopilot::Auto { backup_keep: 1, ..Default::default() }, ..server(&paths) };
        let old = take(&paths, &s, "scheduled").unwrap().unwrap();
        // the world moves on: a new cell, a changed ini, a deleted database
        fs::write(paths.zomboid.join("Saves/Multiplayer/Friends/map/99.bin"), "new").unwrap();
        fs::write(paths.zomboid.join("Server/Friends.ini"), "PVP=false\n").unwrap();
        fs::remove_file(paths.zomboid.join("db/Friends.db")).unwrap();

        // the database's folder can't be made (a file has its name): what was swapped goes back
        fs::remove_dir(paths.zomboid.join("db")).unwrap();
        fs::write(paths.zomboid.join("db"), "").unwrap();
        assert!(restore(&paths, &s, &old.file).is_err());
        assert_eq!(fs::read_to_string(paths.zomboid.join("Server/Friends.ini")).unwrap(), "PVP=false\n", "unchanged");
        assert!(paths.zomboid.join("Saves/Multiplayer/Friends/map/99.bin").exists());
        assert!(!paths.zomboid.join(".pzmm-old").exists() && !paths.zomboid.join(".pzmm-restoring").exists());
        fs::remove_file(paths.zomboid.join("db")).unwrap();

        restore(&paths, &s, &old.file).unwrap();
        assert!(!paths.zomboid.join(".pzmm-old").exists() && !paths.zomboid.join(".pzmm-restoring").exists());
        assert_eq!(fs::read_to_string(paths.zomboid.join("Server/Friends.ini")).unwrap(), "PVP=true\n");
        assert_eq!(fs::read_to_string(paths.zomboid.join("db/Friends.db")).unwrap(), "db");
        assert!(!paths.zomboid.join("Saves/Multiplayer/Friends/map/99.bin").exists(), "what came after the backup is gone");
        assert_eq!(fs::read_to_string(paths.zomboid.join("Server/Friends2.ini")).unwrap(), "other server\n", "other servers untouched");
        // keep 1: retention kept the "restore" backup of the state before it, and the one restored
        let left = list(&paths, "Friends");
        assert_eq!(left.iter().map(|b| b.reason.as_str()).collect::<Vec<_>>(), ["restore", "scheduled"]);
        fs::create_dir(paths.zomboid.join(".pzmm-old")).unwrap();
        assert!(restore(&paths, &s, &old.file).unwrap_err().to_string().contains("earlier restore"));
        fs::remove_dir(paths.zomboid.join(".pzmm-old")).unwrap();
        let mut z = zip::ZipArchive::new(fs::File::open(&left[0].file).unwrap()).unwrap();
        let mut ini = String::new();
        std::io::Read::read_to_string(&mut z.by_name("Server/Friends.ini").unwrap(), &mut ini).unwrap();
        assert_eq!(ini, "PVP=false\n", "the state before the restore");
        assert!(z.by_name("Saves/Multiplayer/Friends/map/99.bin").is_ok());
        // the player's own backups are never pruned
        create(&paths, &s, "manual").unwrap();
        create(&paths, &s, "manual").unwrap();
        assert_eq!(list(&paths, "Friends").iter().map(|b| b.reason.as_str()).collect::<Vec<_>>(), ["manual", "manual", "restore"]);
    }

    #[test]
    fn restore_refuses_foreign_zips() {
        let _one = one_at_a_time();
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = server(&paths);
        let bad = dir(&paths, "Friends").join("1-manual.zip");
        fs::create_dir_all(bad.parent().unwrap()).unwrap();
        let mut z = zip::ZipWriter::new(fs::File::create(&bad).unwrap());
        z.start_file("../../evil.txt", zip::write::SimpleFileOptions::default()).unwrap();
        z.finish().unwrap();
        assert!(restore(&paths, &s, &bad).is_err());
        assert!(paths.zomboid.join("Server/Friends.ini").exists(), "nothing changed");
        assert!(restore(&paths, &s, &paths.zomboid.join("Server/Friends.ini")).is_err(), "only its own backups");
        assert!(covered("Friends", "Saves/Multiplayer/Friends/map_t.bin") && !covered("Friends", "Saves/Multiplayer/Friends2/map_t.bin") && !covered("Friends", "Server/Friends2.ini"));
        assert!(!covered("Friends", "Saves/Multiplayer/Friends/../../../Server/Victim.ini") && !covered("Friends", "Saves/Multiplayer/Friends/./x") && !covered("Friends", "Saves/Multiplayer/Friends\\..\\x"));
    }

    #[test]
    fn export_and_import_rename_to_the_server() {
        let (t, paths) = crate::store::tests::temp_paths();
        let s = server(&paths);
        let b = create(&paths, &s, "manual").unwrap();
        let out = export(&paths, "Friends", &b.file, t.path()).unwrap();
        assert!(out.file_name().unwrap().to_string_lossy().starts_with("Friends-"));
        assert!(export(&paths, "Friends", &paths.zomboid.join("Server/Friends.ini"), t.path()).is_err());
        // into another server: renamed, listed, restorable
        let other = Server { name: "Other".into(), cachedir: Some(paths.zomboid.clone()), ..Default::default() };
        let got = import(&paths, &other, &out).unwrap();
        assert_eq!(got.reason, "import");
        let mut z = zip::ZipArchive::new(fs::File::open(&got.file).unwrap()).unwrap();
        let mut names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().to_string()).collect();
        names.sort();
        assert_eq!(names, ["Saves/Multiplayer/Other/map/12/34.bin", "Server/Other.ini", "Server/Other_SandboxVars.lua", "db/Other.db"]);
        restore(&paths, &other, &got.file).unwrap();
        assert_eq!(fs::read_to_string(paths.zomboid.join("Server/Other.ini")).unwrap(), "PVP=true\n");
        // a zip with anything else is refused whole
        let bad = t.path().join("bad.zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&bad).unwrap());
        for n in ["Server/X.ini", "Users/me/secret.txt"] {
            w.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
        }
        w.finish().unwrap();
        assert!(import(&paths, &other, &bad).is_err());
        assert_eq!(list(&paths, "Other").len(), 1, "only the import (Other had nothing to back up before the restore)");
    }

    #[test]
    fn zone_backups_hold_map_meta_alone() {
        let _one = one_at_a_time();
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = server(&paths);
        assert!(zones_save(&paths, &s, "manual").is_err(), "no map_meta.bin yet");
        let meta = paths.zomboid.join("Saves/Multiplayer/Friends/map_meta.bin");
        fs::write(&meta, "zones v1").unwrap();
        let b = zones_save(&paths, &s, "manual").unwrap();
        assert_eq!((b.reason.as_str(), zones_list(&paths, "Friends").len(), list(&paths, "Friends").len()), ("manual", 1, 0), "apart from full backups");
        let z = zip::ZipArchive::new(fs::File::open(&b.file).unwrap()).unwrap();
        assert_eq!(z.file_names().map(|n| n.unwrap().to_string()).collect::<Vec<_>>(), ["map_meta.bin"]);
        fs::write(&meta, "zones v2").unwrap();
        zones_restore(&paths, &s, &b.file).unwrap();
        assert_eq!(fs::read_to_string(&meta).unwrap(), "zones v1");
        assert_eq!(fs::read_to_string(paths.zomboid.join("Saves/Multiplayer/Friends/map/12/34.bin")).unwrap(), "cell", "the map stays");
        let undo = zones_list(&paths, "Friends").into_iter().find(|b| b.reason == "restore").unwrap();
        zones_restore(&paths, &s, &undo.file).unwrap();
        assert_eq!(fs::read_to_string(&meta).unwrap(), "zones v2", "the restore's own backup undoes it");
        // anything else is refused
        let full = create(&paths, &s, "manual").unwrap();
        assert!(zones_restore(&paths, &s, &full.file).is_err());
        let odd = zones_dir(&paths, "Friends").join("1-odd.zip");
        zip_items(&[(meta.clone(), "other.bin".into())], &odd).unwrap();
        assert!(zones_restore(&paths, &s, &odd).unwrap_err().to_string().contains("no map_meta.bin"));
        assert_eq!(fs::read_to_string(&meta).unwrap(), "zones v2");
    }

    #[test]
    fn wipes_keep_what_was_asked() {
        let _one = one_at_a_time();
        let files = ["players.db", "WorldDictionary.bin", "map_meta.bin", "map_t.bin", "vehicles.db", "map/12/34.bin", "map_40_22.bin", "zpop_1_2.bin", "chunkdata/chunkdata_1_2.bin", "metagrid/x.bin", "map_sand.bin"];
        let left = |kc: bool, kz: bool| files.iter().filter(|f| !wiped(f, kc, kz)).copied().collect::<Vec<_>>();
        assert!(left(false, false).is_empty(), "a full wipe");
        assert_eq!(left(true, false), ["players.db", "WorldDictionary.bin"]);
        assert_eq!(left(true, true), ["players.db", "WorldDictionary.bin", "map_meta.bin", "map_t.bin", "metagrid/x.bin", "map_sand.bin"]);
        assert_eq!(left(false, true), ["WorldDictionary.bin", "map_meta.bin", "map_t.bin", "metagrid/x.bin", "map_sand.bin"]);

        let (_t, paths) = crate::store::tests::temp_paths();
        let s = server(&paths);
        let save = paths.zomboid.join("Saves/Multiplayer/Friends");
        for f in ["players.db", "map_meta.bin"] {
            fs::write(save.join(f), "x").unwrap();
        }
        fs::create_dir_all(paths.zomboid.join("Saves/Multiplayer/Friends_player")).unwrap();
        assert_eq!(wipe(&paths, &s, true, true).unwrap(), 1, "the one chunk");
        assert!(save.join("players.db").exists() && save.join("map_meta.bin").exists() && !save.join("map").exists());
        assert!(paths.zomboid.join("Saves/Multiplayer/Friends_player").exists());
        assert_eq!(list(&paths, "Friends")[0].reason, "wipe");
        wipe(&paths, &s, false, false).unwrap();
        assert!(!save.join("players.db").exists() && !paths.zomboid.join("Saves/Multiplayer/Friends_player").exists());
        assert!(paths.zomboid.join("db/Friends.db").exists() && paths.zomboid.join("Server/Friends.ini").exists(), "accounts and settings stay");
    }

    #[test]
    fn chunk_resets_by_area() {
        let _one = one_at_a_time();
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = server(&paths);
        let save = paths.zomboid.join("Saves/Multiplayer/Friends");
        for rel in ["map/1/1.bin", "map/2/1.bin", "map/10/10.bin", "map_3_3.bin"] {
            let p = save.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "c").unwrap();
        }
        let all = chunks(&save);
        assert_eq!(all.iter().map(|c| (c.x, c.y, c.size)).collect::<Vec<_>>(), [(8, 8, 8), (16, 8, 8), (30, 30, 10), (80, 80, 8), (96, 272, 8)]);
        // the base covers tiles 8..=20, 8..=10: chunks 1/1 and 2/1
        let base = [Area { x1: 20, y1: 10, x2: 8, y2: 8 }];
        let names = |v: Vec<&Chunk>| v.into_iter().map(|c| c.rel.clone()).collect::<Vec<_>>();
        assert_eq!(names(to_reset(&all, &base, true)), ["map/1/1.bin", "map/2/1.bin"]);
        assert_eq!(names(to_reset(&all, &base, false)), ["map_3_3.bin", "map/10/10.bin", "map/12/34.bin"]);
        assert_eq!(reset_chunks(&paths, &s, &base, false).unwrap(), 3);
        assert_eq!(names(chunks(&save).iter().collect()), ["map/1/1.bin", "map/2/1.bin"]);
        assert_eq!(list(&paths, "Friends")[0].reason, "chunks");
        assert_eq!(reset_chunks(&paths, &s, &[], true).unwrap(), 0);
    }

    #[test]
    fn another_servers_settings_become_this_ones() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().into(), data: t.path().join("data") };
        let s = server(&paths);
        fs::write(paths.zomboid.join("Server/Friends.ini"), "RCONPort=27099\nRCONPassword=mine\n").unwrap();
        let got = own_settings(&paths, &s, "Other", true, "PVP=false\r\nRCONPort=27015\r\nRCONPassword=theirs\r\n");
        assert_eq!(got, "PVP=false\r\nRCONPort=27099\r\nRCONPassword=mine\r\n");
        let regions = own_settings(&paths, &s, "Other", false, "{ name = \"x\", serverfile = \"Other_spawnpoints.lua\" }");
        assert!(regions.contains("\"Friends_spawnpoints.lua\""));
    }

    #[test]
    fn finds_zero_byte_cells() {
        let t = tempfile::tempdir().unwrap();
        for (rel, len) in [("map/12/34.bin", 0), ("map/12/35.bin", 10), ("map_40_22.bin", 0), ("chunkdata/chunkdata_1_2.bin", 0), ("hidden_authors.ini", 0)] {
            let p = t.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, vec![1u8; len]).unwrap();
        }
        assert_eq!(empty_cells(t.path()), ["chunkdata/chunkdata_1_2.bin", "map/12/34.bin", "map_40_22.bin"]);
        assert!(empty_cells(&t.path().join("none")).is_empty());
    }
}
