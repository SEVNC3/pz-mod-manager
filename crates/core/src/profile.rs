//! Profiles: named mod setups stored in `<data>/profiles/<name>/`. `modlist.txt` is the
//! library, organised by the player (its order doesn't change what loads):
//!
//! ```text
//! # --- Maps
//! +Muldraugh_Expanded      enabled
//! -SomeMod                 known but disabled
//! ```
//! `loadorder.txt` is the load order: one enabled mod id per line, as written to the game.
//! The active profile is applied to the game's mod list; changes made in-game are
//! adopted back on the next sync, so the two never drift.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::pz::{self, Paths};

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Mod { id: String, on: bool },
    Separator(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub name: String,
    pub lines: Vec<Line>,
    /// The load order (`loadorder.txt`). Only a preference: [`Profile::enabled`] drops ids
    /// that aren't enabled and adds enabled ones it lacks at the end.
    pub order: Vec<String>,
}

impl Profile {
    /// Each mod appears once (first line wins); empty ids are dropped.
    pub fn parse(name: &str, text: &str) -> Profile {
        let mut seen = HashSet::new();
        let lines = text
            .lines()
            .map(str::trim)
            .filter_map(|l| {
                if let Some(s) = l.strip_prefix("# ---") {
                    Some(Line::Separator(s.trim().into()))
                } else if let Some(id) = l.strip_prefix('+') {
                    Some(Line::Mod { id: id.trim().into(), on: true })
                } else {
                    l.strip_prefix('-').map(|id| Line::Mod { id: id.trim().into(), on: false })
                }
            })
            .filter(|l| match l {
                Line::Mod { id, .. } => !id.is_empty() && seen.insert(id.clone()),
                Line::Separator(_) => true,
            })
            .collect();
        Profile { name: name.into(), lines, order: vec![] }
    }

    /// A profile enabling `ids` in order (duplicates and empty ids dropped).
    pub fn from_ids(name: &str, ids: impl IntoIterator<Item = String>) -> Profile {
        let mut seen = HashSet::new();
        let lines = ids.into_iter().filter(|id| !id.is_empty() && seen.insert(id.clone())).map(|id| Line::Mod { id, on: true }).collect();
        Profile { name: name.into(), lines, order: vec![] }
    }

    pub fn to_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| match l {
                Line::Mod { id, on } => format!("{}{id}\n", if *on { '+' } else { '-' }),
                Line::Separator(s) => format!("# --- {s}\n"),
            })
            .collect()
    }

    /// The enabled mods in load order: `order`, then enabled mods it doesn't list in library order.
    pub fn enabled(&self) -> Vec<String> {
        let on: Vec<&String> = self.lines.iter().filter_map(|l| if let Line::Mod { id, on: true } = l { Some(id) } else { None }).collect();
        let (set, mut seen): (HashSet<&String>, _) = (on.iter().copied().collect(), HashSet::new());
        self.order.iter().filter(|id| set.contains(id)).chain(on.iter().copied()).filter(|id| seen.insert(*id)).cloned().collect()
    }

    /// Enable or disable; unknown ids join the library at the end. Newly enabled mods load last.
    pub fn set(&mut self, id: &str, on: bool) {
        match self.lines.iter_mut().find(|l| matches!(l, Line::Mod { id: i, .. } if i == id)) {
            Some(Line::Mod { on: o, .. }) => *o = on,
            _ => self.lines.push(Line::Mod { id: id.into(), on }),
        }
    }

    pub fn forget(&mut self, id: &str) {
        self.lines.retain(|l| !matches!(l, Line::Mod { id: i, .. } if i == id));
    }

    /// Take on the game's current enabled list (it changes when players use the in-game
    /// mod menu): its order becomes the load order. The library keeps its order; mods it
    /// doesn't know yet go at the bottom, after a break if it ends in a named section.
    /// Returns whether anything changed.
    pub fn adopt(&mut self, game: &[String]) -> bool {
        let mut seen = HashSet::new();
        let game: Vec<String> = game.iter().filter(|i| !i.is_empty() && seen.insert(*i)).cloned().collect();
        if self.enabled() == game {
            return false;
        }
        let mut known = HashSet::new();
        for line in &mut self.lines {
            if let Line::Mod { id, on } = line {
                *on = game.contains(id);
                known.insert(id.clone());
            }
        }
        let new: Vec<&String> = game.iter().filter(|i| !known.contains(*i)).collect();
        let in_section = self.lines.iter().rev().find_map(|l| if let Line::Separator(s) = l { Some(!s.is_empty()) } else { None });
        if in_section == Some(true) && !new.is_empty() {
            self.lines.push(Line::Separator(String::new()));
        }
        self.lines.extend(new.into_iter().map(|id| Line::Mod { id: id.clone(), on: true }));
        self.order = game;
        true
    }
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    active: String,
    /// Force the B41 default.txt format; otherwise the file's format is kept.
    #[serde(default)]
    b41: Option<bool>,
    /// default.txt's format the last time it listed mods: an empty list can't show it.
    #[serde(default)]
    seen_b41: Option<bool>,
    /// Profile name -> kind (e.g. "viewpoint"). Untagged profiles are the default kind.
    #[serde(default)]
    kinds: std::collections::BTreeMap<String, String>,
    /// Kind -> the profile of that kind used last, so switching kinds returns to it.
    #[serde(default)]
    last: std::collections::BTreeMap<String, String>,
}

/// The default profile kind: plain Project Zomboid.
pub const DEFAULT_KIND: &str = "zomboid";

/// A profile's kind ("zomboid" unless tagged otherwise).
pub fn kind_of(paths: &Paths, name: &str) -> String {
    state(paths).kinds.get(name).cloned().unwrap_or_else(|| DEFAULT_KIND.into())
}

pub fn set_kind(paths: &Paths, name: &str, kind: &str) -> Result<()> {
    let mut s = state(paths);
    if kind == DEFAULT_KIND {
        s.kinds.remove(name);
    } else {
        s.kinds.insert(name.into(), kind.into());
    }
    save_state(paths, &s)
}

/// Record the active profile as the last one of its kind (done before switching kinds,
/// so switching back returns to it even if it was last applied before kinds existed).
pub fn remember_active(paths: &Paths) -> Result<()> {
    let mut s = state(paths);
    let name = active_name(paths);
    let kind = s.kinds.get(&name).cloned().unwrap_or_else(|| DEFAULT_KIND.into());
    s.last.insert(kind, name);
    save_state(paths, &s)
}

/// The profile of `kind` used most recently, if it still exists.
pub fn last_of_kind(paths: &Paths, kind: &str) -> Option<String> {
    let s = state(paths);
    let existing = list(paths);
    s.last
        .get(kind)
        .filter(|n| existing.contains(n))
        .cloned()
        .or_else(|| existing.into_iter().find(|n| s.kinds.get(n).map_or(DEFAULT_KIND, String::as_str) == kind))
}

fn dir(paths: &Paths) -> PathBuf {
    paths.data.join("profiles")
}

fn valid_name(name: &str) -> Result<()> {
    if !crate::safe_file_name(name) || name.starts_with('.') {
        bail!("invalid profile name: {name:?}");
    }
    Ok(())
}

/// Whether a profile with this name exists, ignoring case (Windows folders do).
pub fn exists(paths: &Paths, name: &str) -> bool {
    list(paths).iter().any(|n| n.eq_ignore_ascii_case(name))
}

pub fn list(paths: &Paths) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir(paths))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("modlist.txt").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The on-disk spelling of a profile name (folders ignore case on Windows; PZMM follows them).
fn on_disk(paths: &Paths, name: &str) -> String {
    list(paths).into_iter().find(|n| n.eq_ignore_ascii_case(name)).unwrap_or_else(|| name.to_string())
}

pub fn load(paths: &Paths, name: &str) -> Result<Profile> {
    valid_name(name)?;
    let name = &on_disk(paths, name);
    let p = dir(paths).join(name).join("modlist.txt");
    if !p.exists() {
        bail!("no profile named {name:?}");
    }
    // lossy: a separator saved as ANSI by Notepad still loads (one odd character) rather than failing
    let mut profile = Profile::parse(name, &String::from_utf8_lossy(&fs::read(p)?));
    // no load order yet (made before 1.1): the library's order was the load order, so nothing moves
    profile.order = match fs::read(dir(paths).join(name).join("loadorder.txt")) {
        Ok(b) => String::from_utf8_lossy(&b).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => profile.enabled(),
        // unreadable is not missing: guessing would overwrite the player's order on the next save
        Err(e) => return Err(e).with_context(|| format!("couldn't read the load order of {name:?}")),
    };
    Ok(profile)
}

/// Profile `name` with `ids` enabled in order. An existing one keeps its library (sections,
/// disabled mods) and takes the new list instead of being replaced (snapshotted first).
pub fn with_list(paths: &Paths, name: &str, ids: Vec<String>) -> Result<Profile> {
    if !exists(paths, name) {
        return Ok(Profile::from_ids(name, ids));
    }
    snapshot(paths, name, "import")?;
    let mut p = load(paths, name)?;
    p.adopt(&ids);
    Ok(p)
}

pub fn save(paths: &Paths, profile: &Profile) -> Result<()> {
    valid_name(&profile.name)?;
    let d = dir(paths).join(&profile.name);
    fs::create_dir_all(&d)?;
    crate::write_atomic(&d.join("modlist.txt"), profile.to_text())?;
    crate::write_atomic(&d.join("loadorder.txt"), profile.enabled().iter().map(|id| format!("{id}\n")).collect::<String>())?;
    Ok(())
}

/// The profile's arranged `Map=` order (`maporder.txt`, beside loadorder.txt): what the Maps
/// tab and the server card start from. Empty until maps are arranged.
pub fn map_order(paths: &Paths, name: &str) -> Vec<String> {
    let file = dir(paths).join(on_disk(paths, name)).join("maporder.txt");
    fs::read_to_string(file).map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default()
}

/// Save `maps` as the arranged order; maps it lacks keep their place (see `server::merge_order`).
/// Returns whether the saved order changed.
pub fn set_map_order(paths: &Paths, name: &str, maps: &[String]) -> Result<bool> {
    valid_name(name)?;
    let old = map_order(paths, name);
    let new = pz::server::merge_order(&old, maps);
    if new == old {
        return Ok(false);
    }
    let d = dir(paths).join(on_disk(paths, name));
    fs::create_dir_all(&d)?;
    crate::write_atomic(&d.join("maporder.txt"), new.iter().map(|m| format!("{m}\n")).collect::<String>())?;
    Ok(true)
}

/// Where `profile`'s `Map=` order starts: its arranged order (maporder.txt); never arranged,
/// default.txt's `maps` block as it stands (hand-arranged, or another profile's), then the
/// enabled mods' maps on `build` in load order. Reads only.
pub fn map_start(paths: &Paths, profile: &Profile, mods: &std::collections::BTreeMap<String, crate::store::Row>, build: u8) -> Vec<String> {
    let order = map_order(paths, &profile.name);
    if !order.is_empty() {
        return order;
    }
    let mut out = pz::read_default(&paths.default_txt()).map_folders();
    out.extend(pz::server::mod_maps(&profile.enabled(), mods, build));
    out
}

/// default.txt's `maps` block for `profile`: its enabled mods' maps on `build`, in the order
/// `map_start` gives (so a never-arranged profile keeps the block's order but drops other
/// profiles' maps), without Muldraugh (`write_default` keeps it last), from the mod scan `mods`.
fn game_maps(paths: &Paths, profile: &Profile, mods: &std::collections::BTreeMap<String, crate::store::Row>, build: u8) -> Vec<String> {
    let mut allowed = pz::server::mod_maps(&profile.enabled(), mods, build);
    // folders no installed mod provides (a mod outside PZMM's view) aren't PZMM's to drop
    let ids: Vec<String> = mods.keys().cloned().collect();
    let known = pz::server::mod_maps(&ids, mods, build);
    let block = pz::read_default(&paths.default_txt()).map_folders();
    let unknown: Vec<String> = block.iter().filter(|f| !known.contains(f)).cloned().collect();
    allowed.extend(unknown.iter().cloned());
    let start = map_start(paths, profile, mods, build);
    // ...and keep their place in the block (after the map they followed), arranged or not
    let anchors: Vec<String> = block.into_iter().filter(|f| unknown.contains(f) || start.contains(f)).collect();
    let line = pz::server::map_order(&pz::server::merge_order(&anchors, &start), &allowed);
    line.into_iter().filter(|m| !pz::server::is_base_map(m)).collect()
}

pub fn delete(paths: &Paths, name: &str) -> Result<()> {
    valid_name(name)?;
    let name = &on_disk(paths, name);
    if active_name(paths).eq_ignore_ascii_case(name) {
        bail!("{name:?} is the active profile; switch to another first");
    }
    fs::remove_dir_all(dir(paths).join(name))?;
    let mut s = state(paths);
    s.kinds.remove(name);
    s.last.retain(|_, n| n != name);
    save_state(paths, &s)
}

/// Rename a profile (its folder, its kind, and the active / last-used records). A change of
/// case only goes through a temporary name, since the folder is the same one on Windows.
/// Folders are renamed, never copied and deleted, so a failure leaves the profile as it was.
pub fn rename(paths: &Paths, from: &str, to: &str) -> Result<()> {
    valid_name(from)?;
    valid_name(to)?;
    if !exists(paths, from) {
        bail!("no profile named {from:?}");
    }
    let from = on_disk(paths, from);
    if from == to {
        return Ok(());
    }
    let case_only = from.eq_ignore_ascii_case(to);
    if !case_only && exists(paths, to) {
        bail!("a profile named {to:?} already exists");
    }
    let active = active_name(paths).eq_ignore_ascii_case(&from);
    let (old, new) = (dir(paths).join(&from), dir(paths).join(to));
    if case_only {
        // valid and visible: if PZMM stops between the two steps, the profile still shows up
        let tmp = dir(paths).join(format!("{from}.renaming"));
        fs::rename(&old, &tmp).with_context(|| format!("renaming {from:?} (close anything using its folder)"))?;
        fs::rename(&tmp, &new).with_context(|| format!("renaming {from:?}: it's in {}", tmp.display()))?;
    } else {
        fs::rename(&old, &new).with_context(|| format!("renaming {from:?} (close anything using its folder)"))?;
    }
    let mut s = state(paths);
    if let Some(k) = s.kinds.remove(&from) {
        s.kinds.insert(to.into(), k);
    }
    s.last.values_mut().filter(|n| **n == from).for_each(|n| *n = to.into());
    if active {
        s.active = to.into();
    }
    save_state(paths, &s)
}

/// A copy of profile `from` (library, load order, map order, kind) named `to`.
pub fn duplicate(paths: &Paths, from: &str, to: &str) -> Result<()> {
    valid_name(to)?;
    if exists(paths, to) {
        bail!("a profile named {to:?} already exists");
    }
    let p = load(paths, from)?;
    let (maps, kind) = (read_map_order(paths, &p.name)?, kind_of(paths, &p.name));
    save(paths, &Profile { name: to.into(), ..p })?;
    if !maps.is_empty() {
        crate::write_atomic(&dir(paths).join(to).join("maporder.txt"), maps.iter().map(|m| format!("{m}\n")).collect::<String>())?;
    }
    set_kind(paths, to, &kind)
}

/// maporder.txt for snapshots and copies: missing is empty, unreadable is an error (an empty
/// copy would later overwrite the player's arrangement).
fn read_map_order(paths: &Paths, name: &str) -> Result<Vec<String>> {
    match fs::read_to_string(dir(paths).join(on_disk(paths, name)).join("maporder.txt")) {
        Ok(t) => Ok(t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e).with_context(|| format!("couldn't read the map order of {name:?}")),
    }
}

/// How many snapshots each profile keeps.
pub const SNAPSHOTS: usize = 10;

/// A profile's lists as they were before a big change (`<profile>/snapshots/<ms>.json`).
#[derive(Serialize, Deserialize)]
struct Snapshot {
    time: u64,
    /// What came next: "update", "autosort", "group", "remove", "import", "restore".
    label: String,
    /// modlist.txt
    list: String,
    order: Vec<String>,
    maps: Vec<String>,
}

/// One snapshot in the list (newest first).
#[derive(Debug, Serialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub time: u64,
    pub label: String,
    pub enabled: usize,
    pub mods: usize,
}

fn snapshot_dir(paths: &Paths, name: &str) -> PathBuf {
    dir(paths).join(on_disk(paths, name)).join("snapshots")
}

/// The snapshot ids of `name`, newest first (ids are creation times in ms).
fn snapshot_ids(paths: &Paths, name: &str) -> Vec<u64> {
    let mut ids: Vec<u64> = fs::read_dir(snapshot_dir(paths, name))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json")?.parse().ok())
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids
}

/// Keep the saved state of profile `name` before a change labelled `label`; the oldest beyond
/// [`SNAPSHOTS`] go. Refuses (and changes nothing) when the profile can't be read.
pub fn snapshot(paths: &Paths, name: &str, label: &str) -> Result<()> {
    let p = load(paths, name)?;
    let snap = Snapshot { time: crate::store::now(), label: label.into(), list: p.to_text(), order: p.enabled(), maps: read_map_order(paths, name)? };
    let d = snapshot_dir(paths, name);
    fs::create_dir_all(&d)?;
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let id = (ms..).find(|i| !d.join(format!("{i}.json")).exists()).unwrap_or(ms);
    crate::write_atomic(&d.join(format!("{id}.json")), serde_json::to_string(&snap)?)?;
    for old in snapshot_ids(paths, name).into_iter().skip(SNAPSHOTS) {
        fs::remove_file(d.join(format!("{old}.json")))?;
    }
    Ok(())
}

/// Profile `name`'s snapshots, newest first (unreadable ones are left out).
pub fn snapshots(paths: &Paths, name: &str) -> Vec<SnapshotInfo> {
    let d = snapshot_dir(paths, name);
    snapshot_ids(paths, name)
        .into_iter()
        .filter_map(|id| {
            let s: Snapshot = serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.json"))).ok()?).ok()?;
            let p = Profile { order: s.order, ..Profile::parse(name, &s.list) };
            let mods = p.lines.iter().filter(|l| matches!(l, Line::Mod { .. })).count();
            Some(SnapshotInfo { id: id.to_string(), time: s.time, label: s.label, enabled: p.enabled().len(), mods })
        })
        .collect()
}

/// Put profile `name` back as snapshot `id` had it, after snapshotting its current state (so a
/// restore can be undone the same way). Saves it; the caller applies it if it's active.
pub fn restore(paths: &Paths, name: &str, id: &str) -> Result<Profile> {
    let id: u64 = id.parse().with_context(|| format!("not a snapshot: {id:?}"))?;
    let file = snapshot_dir(paths, name).join(format!("{id}.json"));
    let s: Snapshot = serde_json::from_str(&fs::read_to_string(&file)?).with_context(|| format!("reading {}", file.display()))?;
    snapshot(paths, name, "restore")?;
    let name = on_disk(paths, name);
    let p = Profile { order: s.order, ..Profile::parse(&name, &s.list) };
    save(paths, &p)?;
    let maps = dir(paths).join(&name).join("maporder.txt");
    if s.maps.is_empty() {
        if maps.exists() {
            fs::remove_file(maps)?; // it was never arranged then
        }
    } else {
        crate::write_atomic(&maps, s.maps.iter().map(|m| format!("{m}\n")).collect::<String>())?;
    }
    Ok(p)
}

fn state(paths: &Paths) -> State {
    let path = paths.data.join("state.json");
    let Ok(text) = fs::read_to_string(&path) else { return State::default() };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        // keep the broken file for inspection instead of silently overwriting it on the next save
        let _ = fs::copy(&path, paths.data.join("state.json.bad"));
        crate::say!("stateDamaged", "error: {file} is damaged ({error}); kept a copy as state.json.bad and started fresh", file = path.display(), error = e);
        State::default()
    })
}

fn save_state(paths: &Paths, s: &State) -> Result<()> {
    fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&paths.data.join("state.json"), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

pub fn active_name(paths: &Paths) -> String {
    Some(state(paths).active).filter(|s| !s.is_empty()).unwrap_or_else(|| "Default".into())
}

/// Forced default.txt format (41 or 42), or None to keep the file's own.
pub fn build(paths: &Paths) -> Option<u8> {
    state(paths).b41.map(|b| if b { 41 } else { 42 })
}

/// The build being played: the forced format, else what default.txt looks like (B42 if unknown).
pub fn effective_build(paths: &Paths) -> u8 {
    let d = pz::read_default(&paths.default_txt());
    let b41 = if d.mods.is_empty() { state(paths).seen_b41.unwrap_or(false) } else { d.b41 };
    build(paths).unwrap_or(if b41 { 41 } else { 42 })
}

pub fn set_b41(paths: &Paths, b41: Option<bool>) -> Result<()> {
    save_state(paths, &State { b41, ..state(paths) })
}

/// The active profile as saved, without picking up in-game changes (for callers that don't
/// hold the data lock and so must not write).
pub fn peek(paths: &Paths) -> Profile {
    let name = active_name(paths);
    load(paths, &name).unwrap_or(Profile { name, ..Default::default() })
}

/// Load the active profile (creating "Default" from the game's list on first run) and
/// adopt any changes made in-game. Returns the profile and whether it adopted changes.
pub fn sync(paths: &Paths) -> Result<(Profile, bool)> {
    let name = active_name(paths);
    // a profile that exists but can't be read is an error: starting empty would replace it with the game's list
    let mut p = if dir(paths).join(&name).join("modlist.txt").exists() { load(paths, &name)? } else { Profile { name, ..Default::default() } };
    // a missing or unreadable default.txt (game running, odd encoding) says nothing about
    // what's enabled: adopting it would switch the whole profile off
    let game = pz::try_read_default(&paths.default_txt());
    let reset = game.as_ref().is_some_and(|g| game_reset(paths, &p, g));
    if reset {
        crate::say!("gameReset", "the game emptied its mod list (it does that once after some updates), so PZMM kept yours: launch from PZMM to put it back");
    }
    let changed = !reset && game.is_some_and(|game| p.adopt(&game.mods));
    if changed || !dir(paths).join(&p.name).exists() {
        save(paths, &p)?;
    }
    Ok((p, changed))
}

/// Build 42 empties default.txt on its first start after some updates, unless the marker
/// `mods/reset-mods-<release>.txt` exists (`ZomboidFileSystem.resetDefaultModsForNewRelease`).
/// True when the game's list is empty, `p` has mods on and a marker is newer than `p`'s last save.
fn game_reset(paths: &Paths, p: &Profile, game: &pz::DefaultTxt) -> bool {
    let modified = |f: PathBuf| fs::metadata(f).and_then(|m| m.modified()).ok();
    let Some(saved) = modified(dir(paths).join(on_disk(paths, &p.name)).join("loadorder.txt")) else { return false };
    game.mods.is_empty()
        && !p.enabled().is_empty()
        && fs::read_dir(paths.zomboid.join("mods"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("reset-mods-") && modified(e.path()).is_some_and(|t| t > saved))
}

/// Write the marker that stops Build 42 emptying default.txt (see [`game_reset`]).
fn keep_through_game_reset(paths: &Paths) {
    let Some(game) = pz::folders::path("game", &crate::settings::load()) else { return };
    let Some(release) = ["projectzomboid.jar", "projectzomboid/projectzomboid.jar"].iter().find_map(|j| reset_release(&game.join(j))) else { return };
    let marker = paths.zomboid.join("mods").join(format!("reset-mods-{release}.txt"));
    if !marker.exists() {
        let _ = fs::write(marker, "If this file does not exist, default.txt will be reset to empty (no mods active).");
    }
}

/// The marker's release part (`42_00`): the only `<digits>_<digits>` string in the game's `GameWindow` class.
fn reset_release(jar: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut class = vec![];
    zip::ZipArchive::new(fs::File::open(jar).ok()?).ok()?.by_name("zombie/GameWindow.class").ok()?.read_to_end(&mut class).ok()?;
    // constant pool strings: tag 1, a big-endian u16 length, the bytes
    let found: HashSet<&str> = (0..class.len().saturating_sub(3))
        .filter(|&i| class[i] == 1)
        .filter_map(|i| {
            let len = u16::from_be_bytes([class[i + 1], class[i + 2]]) as usize;
            let s = std::str::from_utf8(class.get(i + 3..i + 3 + len)?).ok()?;
            let (a, b) = s.split_once('_')?;
            let digits = |t: &str| !t.is_empty() && t.len() <= 4 && t.bytes().all(|c| c.is_ascii_digit());
            (digits(a) && digits(b)).then_some(s)
        })
        .collect();
    // ponytail: a guess from the class file; two candidates means the game changed, so write nothing
    (found.len() == 1).then(|| found.into_iter().next().unwrap().to_string())
}

/// Save `profile`, make it active and write it to the game's mod list, its maps from the mod
/// scan `mods` (None: the mods couldn't be read, so the `maps` block is kept as it is).
pub fn apply(paths: &Paths, profile: &Profile, mods: Option<&std::collections::BTreeMap<String, crate::store::Row>>) -> Result<()> {
    save(paths, profile)?;
    let mut s = state(paths);
    if let Some(d) = pz::try_read_default(&paths.default_txt()).filter(|d| !d.mods.is_empty()) {
        s.seen_b41 = Some(d.b41);
    }
    let b41 = s.b41.or(s.seen_b41);
    let maps = mods.map(|m| game_maps(paths, profile, m, if b41.unwrap_or(false) { 41 } else { 42 }));
    pz::write_default(&paths.default_txt(), &profile.enabled(), b41, maps.as_deref())?;
    if b41 != Some(true) {
        keep_through_game_reset(paths);
    }
    let kind = s.kinds.get(&profile.name).cloned().unwrap_or_else(|| DEFAULT_KIND.into());
    s.last.insert(kind, profile.name.clone());
    save_state(paths, &State { active: profile.name.clone(), ..s })
}

/// The mod scan for `apply`, None (and said in the activity log) when it failed.
pub fn maps_scan<T>(scan: Result<T>) -> Option<T> {
    scan.inspect_err(|e| crate::say!("mapsNotUpdated", "error: couldn't read your mods, so the game's map list wasn't updated: {error}", error = format!("{e:#}"))).ok()
}

/// A profile to share with friends or a server: its list plus where each mod comes
/// from on the Workshop, so the receiver can install what they're missing.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Shared {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    /// The profile in modlist.txt form (`+Id`, `-Id`, `# --- Separator`).
    pub list: String,
    /// The load order (codes made before 1.1 lack it: then `list`'s order is used).
    #[serde(default)]
    pub order: Vec<String>,
    /// Mod id -> Workshop item id, for mods that come from the Workshop.
    #[serde(default)]
    pub workshop: std::collections::BTreeMap<String, String>,
}

/// Package the active profile for sharing (`mods`: the current scan, for Workshop ids).
pub fn export(paths: &Paths, mods: &std::collections::BTreeMap<String, crate::store::Row>) -> Result<Shared> {
    let p = sync(paths)?.0;
    let workshop = p
        .lines
        .iter()
        .filter_map(|l| if let Line::Mod { id, .. } = l { Some(id) } else { None })
        .filter_map(|id| {
            let item = crate::sources::workshop_item(&mods.get(id)?.source)?;
            Some((id.clone(), item.to_string()))
        })
        .collect();
    Ok(Shared { kind: kind_of(paths, &p.name), list: p.to_text(), order: p.enabled(), name: p.name, workshop })
}

/// Workshop items a shared profile needs that aren't installed (`mods`: the current scan).
pub fn import_needs(shared: &Shared, mods: &std::collections::BTreeMap<String, crate::store::Row>) -> Vec<crate::sources::Source> {
    let mut items: Vec<&String> = shared
        .workshop
        .iter()
        .filter(|(id, item)| !mods.contains_key(*id) && crate::sources::steam_workshop::valid_id(item))
        .map(|(_, item)| item)
        .collect();
    items.sort();
    items.dedup();
    items.into_iter().map(|i| crate::sources::Source::Workshop(i.clone())).collect()
}

/// Save a shared profile under a free name (`<name>`, `<name> 2`...), keep its kind, and
/// switch to it. Returns the profile.
pub fn import(paths: &Paths, shared: &Shared) -> Result<Profile> {
    let base = if crate::safe_file_name(shared.name.trim()) { shared.name.trim().to_string() } else { "Shared".to_string() };
    let name = if exists(paths, &base) { (2..).map(|i| format!("{base} {i}")).find(|n| !exists(paths, n)).unwrap() } else { base };
    let p = Profile { order: shared.order.clone(), ..Profile::parse(&name, &shared.list) };
    if !shared.kind.is_empty() && crate::safe_file_name(&shared.kind) {
        set_kind(paths, &name, &shared.kind)?;
    }
    apply(paths, &p, crate::store::scan(paths).ok().as_ref())?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(paths: &Paths, p: &Profile) -> Result<()> {
        super::apply(paths, p, super::maps_scan(crate::store::scan(paths)).as_ref())
    }

    #[test]
    fn share_round_trip() {
        let (_t, paths) = temp_paths();
        let friends = Profile { order: ids(&["C", "A"]), ..Profile::parse("Friends", "# --- Core\n+A\n-B\n+C\n") };
        apply(&paths, &friends).unwrap();
        let shared = export(&paths, &Default::default()).unwrap();
        assert_eq!((shared.list.as_str(), &shared.order), ("# --- Core\n+A\n-B\n+C\n", &ids(&["C", "A"])));
        let back: Shared = serde_json::from_str(&serde_json::to_string(&shared).unwrap()).unwrap();
        let p = import(&paths, &back).unwrap();
        assert_eq!((p.name.as_str(), p.to_text().as_str(), p.enabled()), ("Friends 2", "# --- Core\n+A\n-B\n+C\n", ids(&["C", "A"])));
        let needs = import_needs(
            &Shared { workshop: [("X".into(), "123".into()), ("Y".into(), "+bad".into())].into(), ..back },
            &Default::default(),
        );
        assert_eq!(needs, [crate::sources::Source::Workshop("123".into())]);
    }
    use crate::store::tests::temp_paths;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn profile_names_ignore_case_like_their_folders() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "+A
")).unwrap();
        assert_eq!(load(&paths, "default").unwrap().name, "Default");
        assert!(delete(&paths, "DEFAULT").is_err(), "the active profile can't be deleted under another casing");
    }

    #[test]
    fn unreadable_profile_is_not_replaced() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "# --- Maps\n+A\n-B\n")).unwrap();
        // Notepad re-saved it as ANSI: the separator is no longer valid UTF-8
        let file = dir(&paths).join("Default").join("modlist.txt");
        fs::write(&file, b"# --- V\xe9g\n+A\n-B\n").unwrap();
        let (p, _) = sync(&paths).unwrap();
        assert_eq!(p.lines.len(), 3, "separator and disabled entry kept: {:?}", p.lines);
        assert!(matches!(&p.lines[2], Line::Mod { id, on: false } if id == "B"));
    }

    #[test]
    fn map_order_is_kept_per_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "+A\n")).unwrap();
        assert!(map_order(&paths, "Default").is_empty(), "never arranged");
        set_map_order(&paths, "default", &ids(&["Town", "Muldraugh, KY"])).unwrap();
        // saving the profile (a toggle) leaves it alone; other profiles have their own
        apply(&paths, &Profile::parse("Default", "+A\n+B\n")).unwrap();
        assert_eq!(map_order(&paths, "Default"), ids(&["Town", "Muldraugh, KY"]));
        apply(&paths, &Profile::parse("Other", "+A\n")).unwrap();
        assert!(map_order(&paths, "Other").is_empty());
        assert!(set_map_order(&paths, "../x", &[]).is_err());
    }

    #[test]
    fn applying_writes_the_map_order_to_the_game() {
        let (_t, paths) = temp_paths();
        for (id, map) in [("A", "Town A"), ("B", "Town B"), ("C", "Off")] {
            let d = paths.mods().join(id).join("42");
            fs::create_dir_all(d.join("media/maps").join(map)).unwrap();
            fs::write(d.join("mod.info"), format!("id={id}\n")).unwrap();
        }
        apply(&paths, &Profile::parse("Default", "+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).maps, "\n    map = Town A,\n    map = Town B,\n", "never arranged: the enabled maps in load order");
        set_map_order(&paths, "Default", &ids(&["Town B", "Off", "Town A", "Muldraugh, KY"])).unwrap();
        apply(&paths, &Profile::parse("Default", "+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).maps, "\n    map = Town B,\n    map = Town A,\n", "enabled maps only, Muldraugh left to the game");
        // a map that leaves the arrangement keeps its place in the saved order
        set_map_order(&paths, "Default", &ids(&["Town A", "Town B", "Muldraugh, KY"])).unwrap();
        assert_eq!(map_order(&paths, "Default"), ids(&["Town A", "Town B", "Off", "Muldraugh, KY"]));
    }

    /// Mods `ids`, each adding one map folder (B42 layout).
    fn map_mods(paths: &Paths, mods: &[(&str, &str)]) {
        for (id, map) in mods {
            let d = paths.mods().join(id).join("42");
            fs::create_dir_all(d.join("media/maps").join(map)).unwrap();
            fs::write(d.join("mod.info"), format!("id={id}\n")).unwrap();
        }
    }

    #[test]
    fn opening_keeps_a_hand_arranged_block() {
        let (_t, paths) = temp_paths();
        map_mods(&paths, &[("A", "Town A"), ("B", "Town B")]);
        // arranged by hand (or by the game), never in PZMM: no maporder.txt
        let game = "VERSION = 1,\n\nmods\n{\n    mod = \\A,\n    mod = \\B,\n}\n\nmaps\n{\n    map = Town B,\n    map = Town A,\n    map = Muldraugh, KY,\n}\n";
        fs::create_dir_all(paths.default_txt().parent().unwrap()).unwrap();
        fs::write(paths.default_txt(), game).unwrap();
        // opening: the view syncs the profile, the Maps tab asks where the order starts
        let (p, _) = sync(&paths).unwrap();
        let mods = crate::store::scan(&paths).unwrap();
        let line = pz::server::map_order(&map_start(&paths, &p, &mods, 42), &pz::server::mod_maps(&p.enabled(), &mods, 42));
        assert_eq!(line, ids(&["Town B", "Town A", "Muldraugh, KY"]), "the block's order, not A-Z");
        assert_eq!(fs::read_to_string(paths.default_txt()).unwrap(), game, "default.txt unchanged byte for byte");
        assert!(map_order(&paths, "Default").is_empty(), "nothing saved");
        // and applying the same list (a toggle elsewhere) keeps the arrangement
        apply(&paths, &p).unwrap();
        assert_eq!(fs::read_to_string(paths.default_txt()).unwrap(), game);
    }

    #[test]
    fn switching_to_a_never_arranged_profile_drops_the_other_maps() {
        let (_t, paths) = temp_paths();
        map_mods(&paths, &[("A", "Town A"), ("B", "Town B"), ("C", "Other"), ("D", "Town D")]);
        apply(&paths, &Profile::parse("Big", "+A\n+B\n+C\n")).unwrap();
        set_map_order(&paths, "Big", &ids(&["Other", "Town B", "Town A", "Muldraugh, KY"])).unwrap();
        apply(&paths, &Profile::parse("Big", "+A\n+B\n+C\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Other", "Town B", "Town A"]));
        // Small was never arranged: the block's order where it's still valid, then its own new maps
        apply(&paths, &Profile::parse("Small", "+D\n+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Town B", "Town A", "Town D"]));
        assert!(map_order(&paths, "Small").is_empty(), "still never arranged");
        // a folder no installed mod provides stays where it was
        let d = fs::read_to_string(paths.default_txt()).unwrap().replace("    map = Town A,", "    map = Unknown Map,\n    map = Town A,");
        fs::write(paths.default_txt(), d).unwrap();
        apply(&paths, &Profile::parse("Small", "+D\n+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Town B", "Unknown Map", "Town A", "Town D"]));
        // ...and in an arranged profile too (not moved to the end or before the game's maps)
        apply(&paths, &Profile::parse("Big", "+A\n+B\n+C\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Other", "Town B", "Unknown Map", "Town A"]));
    }

    #[test]
    fn snapshots_keep_the_last_ones_and_restore() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile { order: ids(&["B", "A"]), ..Profile::parse("Default", "# --- S\n+A\n+B\n-C\n") }).unwrap();
        set_map_order(&paths, "Default", &ids(&["Town", "Muldraugh, KY"])).unwrap();
        snapshot(&paths, "default", "autosort").unwrap();
        // the change it guards against
        apply(&paths, &Profile::parse("Default", "+C\n")).unwrap();
        set_map_order(&paths, "Default", &ids(&["Other", "Muldraugh, KY"])).unwrap();
        let list = snapshots(&paths, "Default");
        assert_eq!((list.len(), list[0].label.as_str(), list[0].enabled, list[0].mods), (1, "autosort", 2, 3));
        let p = restore(&paths, "Default", &list[0].id).unwrap();
        assert_eq!((p.to_text().as_str(), p.enabled()), ("# --- S\n+A\n+B\n-C\n", ids(&["B", "A"])));
        assert_eq!(load(&paths, "Default").unwrap(), p, "saved");
        assert_eq!(map_order(&paths, "Default"), ids(&["Town", "Muldraugh, KY"]));
        // the restore kept what it replaced, so it can be undone the same way
        let list = snapshots(&paths, "Default");
        assert_eq!((list[0].label.as_str(), list[0].enabled), ("restore", 1));
        restore(&paths, "Default", &list[0].id).unwrap();
        assert_eq!(load(&paths, "Default").unwrap().enabled(), ids(&["C"]));
        // only the newest are kept
        for _ in 0..12 {
            snapshot(&paths, "Default", "update").unwrap();
        }
        assert_eq!(snapshots(&paths, "Default").len(), SNAPSHOTS);
        // an unreadable profile isn't snapshotted (or restored over)
        let order = dir(&paths).join("Default/loadorder.txt");
        fs::remove_file(&order).unwrap();
        fs::create_dir(&order).unwrap();
        assert!(snapshot(&paths, "Default", "update").is_err());
        assert!(restore(&paths, "Default", &snapshots(&paths, "Default")[0].id).is_err());
        assert!(restore(&paths, "Default", "../x").is_err());
    }

    #[test]
    fn rename_and_duplicate_keep_everything() {
        let (_t, paths) = temp_paths();
        set_kind(&paths, "Solo", "viewpoint").unwrap();
        apply(&paths, &Profile::parse("Solo", "# --- S\n+A\n")).unwrap();
        set_map_order(&paths, "Solo", &ids(&["Town", "Muldraugh, KY"])).unwrap();
        apply(&paths, &Profile::parse("Other", "+B\n")).unwrap();
        // names clash whatever their case
        assert!(rename(&paths, "Solo", "OTHER").is_err());
        assert!(duplicate(&paths, "Solo", "other").is_err());
        assert!(rename(&paths, "Solo", "../x").is_err() && rename(&paths, "Gone", "New").is_err());
        assert_eq!(list(&paths), ids(&["Other", "Solo"]), "nothing changed");
        // a change of case only
        rename(&paths, "solo", "SOLO").unwrap();
        assert_eq!(list(&paths), ids(&["Other", "SOLO"]));
        assert_eq!((kind_of(&paths, "SOLO"), last_of_kind(&paths, "viewpoint")), ("viewpoint".to_string(), Some("SOLO".to_string())));
        assert_eq!(load(&paths, "SOLO").unwrap().to_text(), "# --- S\n+A\n");
        assert_eq!(map_order(&paths, "SOLO"), ids(&["Town", "Muldraugh, KY"]));
        // the active profile stays active under its new name
        rename(&paths, "Other", "Main").unwrap();
        assert_eq!((active_name(&paths), sync(&paths).unwrap().0.enabled()), ("Main".to_string(), ids(&["B"])));
        duplicate(&paths, "SOLO", "Copy").unwrap();
        assert_eq!(list(&paths), ids(&["Copy", "Main", "SOLO"]));
        assert_eq!((load(&paths, "Copy").unwrap().to_text(), kind_of(&paths, "Copy")), ("# --- S\n+A\n".to_string(), "viewpoint".to_string()));
        assert_eq!(map_order(&paths, "Copy"), map_order(&paths, "SOLO"));
        assert_eq!(active_name(&paths), "Main", "duplicating doesn't switch");
    }

    #[test]
    fn text_round_trip() {
        let text = "# --- Maps\n+A\n-B\n+C\n";
        assert_eq!(Profile::parse("x", text).to_text(), text);
    }

    #[test]
    fn adopt_takes_the_game_order_and_keeps_the_library() {
        let mut p = Profile::parse("x", "# --- Maps\n+A\n-B\n+C\n");
        assert!(!p.adopt(&ids(&["A", "C"])));
        // in-game: C moved before A, B enabled, D added, duplicates ignored
        assert!(p.adopt(&ids(&["C", "A", "B", "D", "A"])));
        assert_eq!(p.enabled(), ids(&["C", "A", "B", "D"]));
        assert_eq!(p.to_text(), "# --- Maps\n+A\n+B\n+C\n# --- \n+D\n", "library order kept; D after a break");
        assert!(p.adopt(&ids(&["B"])));
        assert_eq!(p.to_text(), "# --- Maps\n-A\n+B\n-C\n# --- \n-D\n");
    }

    #[test]
    fn upgrade_without_a_load_order_keeps_the_library_order() {
        let (_t, paths) = temp_paths();
        let d = dir(&paths).join("Default");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("modlist.txt"), "# --- Maps\n+B\n-C\n+A\n").unwrap();
        let p = load(&paths, "Default").unwrap();
        assert_eq!(p.enabled(), ids(&["B", "A"]));
        save(&paths, &p).unwrap();
        assert_eq!(fs::read_to_string(d.join("loadorder.txt")).unwrap(), "B\nA\n");
    }

    #[test]
    fn enabling_appends_to_the_load_order_and_disabling_drops() {
        let mut p = Profile { order: ids(&["A", "B"]), ..Profile::parse("x", "-C\n+B\n+A\n") };
        p.set("C", true);
        p.set("D", true);
        assert_eq!(p.enabled(), ids(&["A", "B", "C", "D"]), "newly enabled mods load last, whatever their library place");
        p.set("B", false);
        p.forget("D");
        assert_eq!(p.enabled(), ids(&["A", "C"]));
    }

    #[test]
    fn sync_adopts_the_game_order() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "# --- Mine\n+A\n+B\n-C\n")).unwrap();
        pz::write_default(&paths.default_txt(), &ids(&["C", "B", "A"]), None, None).unwrap();
        let (p, adopted) = sync(&paths).unwrap();
        assert!(adopted);
        assert_eq!(p.enabled(), ids(&["C", "B", "A"]));
        assert_eq!(p.to_text(), "# --- Mine\n+A\n+B\n+C\n");
        assert_eq!(load(&paths, "Default").unwrap().enabled(), ids(&["C", "B", "A"]), "saved");
    }

    #[test]
    fn unreadable_default_txt_keeps_the_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A", "B"]))).unwrap();
        // the game has it locked / it was deleted / it isn't UTF-8
        fs::write(paths.default_txt(), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), ids(&["A", "B"]));
        fs::remove_file(paths.default_txt()).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), ids(&["A", "B"]));
    }

    #[test]
    fn a_game_reset_keeps_the_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A", "B"]))).unwrap();
        let marker = paths.zomboid.join("mods/reset-mods-42_00.txt");
        let touch = |secs: i64| {
            let now = std::time::SystemTime::now();
            let t = if secs > 0 { now + std::time::Duration::from_secs(secs as u64) } else { now - std::time::Duration::from_secs(-secs as u64) };
            fs::File::options().write(true).open(&marker).unwrap().set_modified(t).unwrap();
        };
        // the game's first start after an update: it writes the marker and empties its list
        fs::write(&marker, "").unwrap();
        touch(60);
        pz::write_default(&paths.default_txt(), &[], None, None).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), ids(&["A", "B"]));
        // an old marker: the player emptied the list in game, so it's taken
        touch(-3600);
        assert_eq!(sync(&paths).unwrap().0.enabled(), Vec::<String>::new());
    }

    #[test]
    fn reads_the_reset_release_from_the_game() {
        use std::io::Write;
        let t = tempfile::tempdir().unwrap();
        let jar = t.path().join("projectzomboid.jar");
        let mut z = zip::ZipWriter::new(fs::File::create(&jar).unwrap());
        z.start_file("zombie/GameWindow.class", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(b"\xca\xfe\xba\xbe\x01\x00\x0542_00\x01\x00\x03abc\x01\x00\x0442.1").unwrap();
        z.finish().unwrap();
        assert_eq!(reset_release(&jar).as_deref(), Some("42_00"));
        assert_eq!(reset_release(&t.path().join("missing.jar")), None);
    }

    #[test]
    fn duplicates_and_empty_ids_are_dropped() {
        assert_eq!(Profile::parse("x", "+A\n+\n-A\n+B\n").to_text(), "+A\n+B\n");
        assert_eq!(Profile::from_ids("x", ids(&["A", "", "A", "B"])).enabled(), ids(&["A", "B"]));
        let mut p = Profile::parse("x", "+A\n");
        p.adopt(&ids(&["A", "", "A"]));
        assert_eq!(p.to_text(), "+A\n");
    }

    #[test]
    fn kinds_remember_their_last_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        set_kind(&paths, "3D", "viewpoint").unwrap();
        apply(&paths, &Profile::from_ids("3D", ids(&["Viewpoint"]))).unwrap();
        assert_eq!(kind_of(&paths, "3D"), "viewpoint");
        assert_eq!(kind_of(&paths, "Default"), DEFAULT_KIND);
        assert_eq!(last_of_kind(&paths, DEFAULT_KIND).as_deref(), Some("Default"));
        assert_eq!(last_of_kind(&paths, "viewpoint").as_deref(), Some("3D"));
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        delete(&paths, "3D").unwrap();
        assert_eq!(last_of_kind(&paths, "viewpoint"), None);
    }

    #[test]
    fn importing_again_keeps_the_library() {
        let (_t, paths) = temp_paths();
        save(&paths, &Profile::parse("Server x", "# --- Maps
+A
-B
")).unwrap();
        let p = with_list(&paths, "Server x", ids(&["C", "A"])).unwrap();
        assert_eq!((p.to_text().as_str(), p.enabled()), ("# --- Maps
+A
-B
# --- 
+C
", ids(&["C", "A"])));
        assert_eq!(with_list(&paths, "New", ids(&["A"])).unwrap(), Profile::from_ids("New", ids(&["A"])));
        assert_eq!(snapshots(&paths, "Server x").iter().map(|s| (s.label.as_str(), s.enabled)).collect::<Vec<_>>(), [("import", 1)], "kept before the list was replaced");
    }

    #[test]
    fn b41_format_survives_an_empty_list() {
        let (_t, paths) = temp_paths();
        pz::write_default(&paths.default_txt(), &ids(&["A"]), Some(true), None).unwrap();
        apply(&paths, &Profile::from_ids("Default", ids(&[]))).unwrap();
        assert_eq!(effective_build(&paths), 41);
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        assert!(pz::read_default(&paths.default_txt()).b41);
    }

    #[test]
    fn sync_and_apply() {
        let (_t, paths) = temp_paths();
        pz::write_default(&paths.default_txt(), &ids(&["A", "B"]), None, None).unwrap();
        let (mut p, _) = sync(&paths).unwrap();
        assert_eq!((p.name.as_str(), p.enabled()), ("Default", ids(&["A", "B"])));

        p.set("A", false);
        p.set("C", true);
        apply(&paths, &p).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).mods, ids(&["B", "C"]));

        let solo = Profile { name: "Solo".into(), lines: vec![Line::Mod { id: "A".into(), on: true }], order: ids(&["A"]) };
        apply(&paths, &solo).unwrap();
        assert_eq!(sync(&paths).unwrap().0, solo);
        assert_eq!(list(&paths), ids(&["Default", "Solo"]));
        assert!(delete(&paths, "Solo").is_err());
        assert!(load(&paths, "../x").is_err());
    }
}
