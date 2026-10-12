//! Mod ID -> Workshop items for the whole Workshop, so any server's mod IDs resolve when it
//! carries no PZMM list. Read like the map index: every item's file list on one anonymous
//! Steam session. A mod folder whose name the description's "Mod ID:" lines confirm takes
//! that ID; any other folder's mod.info files are read (one small request each). Shipped as a
//! seed (`pzmm mod-index --seed`); the app reads the items updated since.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::Paths;
use super::maps::{Fail, Progress, READERS, browse_patiently, network_error, pause, unless_stopped};
use crate::read_json;
use crate::sources::steam_manifest::Session;
use crate::sources::steam_workshop::{self, Item};

/// One Workshop item's mod IDs (short keys: the seed holds every item).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    /// Its Workshop update time when read.
    #[serde(rename = "u")]
    pub updated: u64,
    #[serde(rename = "s")]
    pub subscriptions: u64,
    #[serde(rename = "i")]
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Index {
    /// Every item updated before this (Unix seconds) has been read.
    pub at: u64,
    pub items: BTreeMap<String, Entry>,
}

fn local_file(paths: &Paths) -> PathBuf {
    paths.data.join("mod-index.json")
}

const SEED: &[u8] = include_bytes!("../../data/mod-index.json.gz");

fn parse_seed(gz: &[u8]) -> Result<Index> {
    Ok(serde_json::from_reader(flate2::read::GzDecoder::new(gz))?)
}

/// The seed, parsed once (empty in tests, and when it doesn't parse).
fn seed() -> &'static Index {
    static PARSED: std::sync::OnceLock<Index> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        if cfg!(test) {
            return Index::default();
        }
        parse_seed(SEED).unwrap_or_else(|e| {
            crate::log!("error: PZMM's built-in mod ID index doesn't parse: {e:#}");
            Index::default()
        })
    })
}

/// The seed with what this install read since: an entry read later wins, and so does the later `at`.
fn merged(local: Index, seed: &Index) -> Index {
    let mut all = Index { at: local.at.max(seed.at), items: local.items };
    for (id, e) in &seed.items {
        if all.items.get(id).is_none_or(|o| o.updated < e.updated) {
            all.items.insert(id.clone(), e.clone());
        }
    }
    all
}

pub fn index(paths: &Paths) -> Index {
    merged(read_json(&local_file(paths)).unwrap_or_default(), seed())
}

/// Mod ID -> the items that hold it, most subscribed first.
pub fn by_id(index: &Index) -> HashMap<String, Vec<String>> {
    let mut items: Vec<(&String, &Entry)> = index.items.iter().collect();
    items.sort_by_key(|(_, e)| std::cmp::Reverse(e.subscriptions));
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (item, e) in items {
        for id in &e.ids {
            out.entry(id.clone()).or_default().push(item.clone());
        }
    }
    out
}

type Kept = Option<(PathBuf, Option<std::time::SystemTime>, std::sync::Arc<HashMap<String, Vec<String>>>)>;
/// [`kept_by_id`]'s map, with the local index file and its modified time it was built from.
static KEPT: Mutex<Kept> = Mutex::new(None);

/// [`by_id`] of [`index`], kept until the local index changes: the seed holds the whole Workshop,
/// and doctor, server joins and installs each look IDs up.
pub fn kept_by_id(paths: &Paths) -> std::sync::Arc<HashMap<String, Vec<String>>> {
    let file = local_file(paths);
    let stamp = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((f, s, map)) = kept.as_ref()
        && *f == file
        && *s == stamp
    {
        return map.clone();
    }
    let map = std::sync::Arc::new(by_id(&index(paths)));
    *kept = Some((file, stamp, map.clone()));
    map
}

/// Save entries (and, when given, the new `at`) under the data lock, re-read and merged.
fn save(paths: &Paths, new: BTreeMap<String, Entry>, at: Option<u64>) -> Result<()> {
    if new.is_empty() && at.is_none() {
        return Ok(());
    }
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let _lock = crate::lock_data(paths)?;
    let file = local_file(paths);
    let mut all: Index = read_json(&file).context("nothing was saved")?;
    for (id, e) in new {
        if all.items.get(&id).is_none_or(|o| o.updated <= e.updated) {
            all.items.insert(id, e);
        }
    }
    if let Some(at) = at {
        all.at = all.at.max(at);
    }
    Ok(crate::write_atomic(&file, serde_json::to_vec(&all)?)?)
}

/// Write the index (seed and local, items with mod IDs) to `out` as the gzipped seed.
/// Returns the items and mod IDs it holds.
pub fn write_seed(paths: &Paths, out: &Path) -> Result<(usize, usize)> {
    let mut all = merged(read_json(&local_file(paths))?, seed());
    all.items.retain(|_, e| !e.ids.is_empty());
    let ids = by_id(&all).len();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    serde_json::to_writer(&mut gz, &all)?;
    crate::write_atomic(out, gz.finish()?)?;
    Ok((all.items.len(), ids))
}

/// Mod folders in an item's file list (paths with `/`): each folder holding a mod.info (a
/// version folder's parent), with the mod.info files under it (its own, then version folders').
fn mod_dirs(files: &[String]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    for f in files {
        let (parent, name) = f.rsplit_once('/').unwrap_or(("", f));
        if !name.eq_ignore_ascii_case("mod.info") {
            continue;
        }
        let dir = match parent.rsplit_once('/') {
            Some((up, last)) if super::is_version_dir(last) => up,
            None if super::is_version_dir(parent) => "",
            _ => parent,
        };
        match out.iter_mut().find(|(d, _)| d == dir) {
            Some((_, infos)) => infos.push(f.clone()),
            None => out.push((dir.to_string(), vec![f.clone()])),
        }
    }
    for (_, infos) in &mut out {
        infos.sort_by_key(|i| i.matches('/').count());
    }
    out
}

/// The mod IDs in an item's file list: a folder named as one of `declared` (the description's
/// "Mod ID:" lines) takes that ID; others read every mod.info under them (B41's and B42's may
/// differ). `read` gives a listed file's text.
fn ids_of(files: &[String], declared: &[String], read: &mut dyn FnMut(&str) -> Result<String>) -> Result<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for (dir, infos) in mod_dirs(files) {
        let folder = dir.rsplit('/').next().unwrap_or(&dir);
        if let Some(d) = declared.iter().find(|d| d.eq_ignore_ascii_case(folder)) {
            if !out.contains(d) {
                out.push(d.clone());
            }
            continue;
        }
        for info in infos {
            let id = super::parse_info(&read(&info)?).get("id").filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| folder.to_string());
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    Ok(out)
}

/// Read one item's mod IDs on `session`.
fn read_item(session: &Session, item: &Item, declared: &[String], stop: &AtomicBool) -> Result<Entry> {
    if item.hcontent == 0 {
        return Err(anyhow::Error::new(Fail::NoFiles));
    }
    let files = session.files(item.hcontent, stop)?;
    let names: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let ids = ids_of(&names, declared, &mut |p| {
        let f = files.iter().find(|f| f.path == p).context("not in the file list")?;
        Ok(super::decode_text(&session.read(f, stop).with_context(|| format!("reading {p}"))?))
    })?;
    Ok(Entry { updated: item.time_updated, subscriptions: item.subscriptions, ids })
}

/// Browse pages stop at 1,000: a wider window is split in two.
const MOST: u64 = 49_000;

/// Every item created (else updated) in `from..=to`, newest first, added to `out` (skipping ones
/// `seen`). Steam's updated filter leaves out items never updated, so only both find everything new.
fn list(created: bool, from: u64, to: u64, stop: &AtomicBool, seen: &mut HashSet<String>, out: &mut Vec<Item>, progress: &mut dyn FnMut(usize)) -> Result<bool> {
    let mut total = 0;
    for page in 1.. {
        let q = if created {
            steam_workshop::Query { sort: "mostrecent".into(), created_from: from, created_to: to, ..Default::default() }
        } else {
            steam_workshop::Query { sort: "lastupdated".into(), updated_from: from, updated_to: to, ..Default::default() }
        };
        let q = steam_workshop::Query { per_page: 50, page, ..q };
        let Some(got) = browse_patiently(&q, stop) else { return Ok(false) };
        let got = got?;
        if page == 1 && got.total > MOST && to > from {
            let mid = from + (to - from) / 2;
            return Ok(list(created, from, mid, stop, seen, out, progress)? && list(created, mid + 1, to, stop, seen, out, progress)?);
        }
        let before = (page as u64 - 1) * 50;
        if page == 1 {
            total = got.total;
        } else if got.items.is_empty() && before + 50 < total {
            // a page Steam sent without results, well before the window's end: not the end of the list
            return Err(anyhow::Error::new(Fail::SteamStopped).context(format!("page {page} of {total} items came back empty")));
        }
        let empty = got.items.is_empty();
        out.extend(got.items.into_iter().filter(|i| seen.insert(i.id.clone())));
        progress(out.len());
        if empty || before + 50 >= total {
            return Ok(true);
        }
    }
    Ok(true)
}

/// Read the mod IDs of every item updated since the index's `at` (all of them the first time),
/// most subscribed first, `READERS` at once on one anonymous Steam session, 100 at a time with
/// their descriptions. Saved as it goes, so a stopped scan goes on where it was; `at` moves on
/// only when every item was read. Setting `stop` ends it.
pub fn scan(paths: &Paths, stop: &AtomicBool, progress: &mut dyn FnMut(&Progress)) -> Result<Progress> {
    let stopped = || stop.load(Relaxed);
    let start = crate::store::now();
    let old = index(paths);
    let mut p = Progress { phase: "listing", ..Default::default() };
    let (mut seen, mut items) = (HashSet::new(), vec![]);
    // the first time every item by creation (the Workshop's first PZ items are from 2013), later what's new or updated since
    let from = old.at.saturating_sub(3600).max(1_300_000_000);
    for created in if old.at == 0 { &[true][..] } else { &[true, false] } {
        let mut said = |n| {
            p.done = n;
            progress(&p);
        };
        if !list(*created, from, start + 86_400, stop, &mut seen, &mut items, &mut said)? {
            return Ok(p);
        }
    }
    // read already (an earlier, stopped scan): skipped
    let mut todo: Vec<&Item> = items.iter().filter(|i| old.items.get(&i.id).is_none_or(|e| e.updated < i.time_updated)).collect();
    todo.sort_by_key(|i| std::cmp::Reverse(i.subscriptions));
    p = Progress { phase: "reading", total: todo.len(), ..Default::default() };
    progress(&p);
    let mut session: Option<Session> = None;
    let (streak, mut reopened, mut net_failed) = (AtomicUsize::new(0), 0, false);
    for chunk in todo.chunks(100) {
        if stopped() {
            return Ok(p);
        }
        if session.is_none() {
            session = match Session::open(stop) {
                Err(_) if stopped() => return Ok(p),
                s => Some(s?),
            };
        }
        let s = session.as_ref().context("no Steam session")?;
        let ids: Vec<String> = chunk.iter().map(|i| i.id.clone()).collect();
        // the full descriptions (browse pages cut them short); without them every mod.info is read
        let details = unless_stopped(stop, move || steam_workshop::details(&ids.iter().map(String::as_str).collect::<Vec<_>>()));
        let declared: HashMap<String, Vec<String>> =
            details.and_then(Result::ok).unwrap_or_default().into_iter().map(|d| (d.id.clone(), super::workshop_mod_ids(&d.description))).collect();
        let (next, done) = (AtomicUsize::new(0), Mutex::new(vec![]));
        std::thread::scope(|scope| {
            for _ in 0..READERS.min(chunk.len()) {
                scope.spawn(|| {
                    while !stopped() && streak.load(Relaxed) < 5 {
                        let Some(&i) = chunk.get(next.fetch_add(1, Relaxed)) else { break };
                        let res = read_item(s, i, declared.get(&i.id).map_or(&[][..], Vec::as_slice), stop);
                        let net = res.as_ref().err().is_some_and(network_error);
                        if net {
                            let n = streak.fetch_add(1, Relaxed) + 1;
                            pause(2 * n as u64, stopped); // back off while Steam struggles
                        } else {
                            streak.store(0, Relaxed);
                        }
                        done.lock().unwrap_or_else(|e| e.into_inner()).push((i, res, net));
                    }
                });
            }
        });
        let mut batch = BTreeMap::new();
        for (i, res, net) in done.into_inner().unwrap_or_else(|e| e.into_inner()) {
            p.done += 1;
            match res {
                Ok(e) => {
                    batch.insert(i.id.clone(), e);
                }
                // can't be read (no files, a broken list): waits for an update
                Err(e) if !net => {
                    crate::log!("workshop:{} ({}): {e:#}", i.id, i.title);
                    p.failed += 1;
                    batch.insert(i.id.clone(), Entry { updated: i.time_updated, subscriptions: i.subscriptions, ids: vec![] });
                }
                Err(_) => {
                    p.failed += 1;
                    net_failed = true;
                }
            }
        }
        save(paths, batch, None)?;
        progress(&p);
        if streak.load(Relaxed) >= 5 && !stopped() {
            // the session may have dropped: a new one, a few times
            reopened += 1;
            if reopened > 3 {
                return Err(anyhow::Error::new(Fail::SteamStopped));
            }
            pause(60, stopped);
            session = None;
            streak.store(0, Relaxed);
        }
    }
    if stopped() {
        return Ok(p);
    }
    save(paths, BTreeMap::new(), (!net_failed).then_some(start))?;
    p.phase = "done";
    progress(&p);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mod_folders_and_their_ids() {
        let files = paths(&[
            "mods/Lib/mod.info",
            "mods/Lib/42/mod.info",
            "mods/Lib/42/media/lua/x.lua",
            "mods/Way/mod.info",
            "mods/Way/42.13/mod.info",
            "mods/Broken/mod.info",
            "mods/NoInfo/media/x.txt",
            "preview.png",
        ]);
        assert_eq!(
            mod_dirs(&files),
            [
                ("mods/Lib".to_string(), paths(&["mods/Lib/mod.info", "mods/Lib/42/mod.info"])),
                ("mods/Way".to_string(), paths(&["mods/Way/mod.info", "mods/Way/42.13/mod.info"])),
                ("mods/Broken".to_string(), paths(&["mods/Broken/mod.info"])),
            ]
        );
        // Lib is confirmed by the description (no read); Way's B41 and B42 ids differ; Broken has no id=
        let mut read = vec![];
        let ids = ids_of(&files, &paths(&["LIB", "Not a folder"]), &mut |p| {
            read.push(p.to_string());
            Ok(match p {
                "mods/Way/mod.info" => "name=Way\nid=Way_Trajectory",
                "mods/Way/42.13/mod.info" => "\u{feff}id=Way_B42",
                _ => "name=no id",
            }
            .to_string())
        })
        .unwrap();
        assert_eq!(ids, ["LIB", "Way_Trajectory", "Way_B42", "Broken"]);
        assert_eq!(read, ["mods/Way/mod.info", "mods/Way/42.13/mod.info", "mods/Broken/mod.info"]);
    }

    #[test]
    fn index_merges_ranks_and_seeds() {
        let e = |u, s, ids: &[&str]| Entry { updated: u, subscriptions: s, ids: paths(ids) };
        let seed = Index { at: 100, items: [("1".into(), e(50, 10, &["A"])), ("2".into(), e(50, 900, &["A", "B"])), ("3".into(), e(90, 5, &["C"]))].into() };
        let local = Index { at: 0, items: [("1".into(), e(60, 10, &["A", "A2"])), ("3".into(), e(80, 5, &["old"]))].into() };
        let all = merged(local, &seed);
        assert_eq!(all.at, 100);
        assert_eq!(all.items["1"].ids, ["A", "A2"], "read later than the seed");
        assert_eq!(all.items["3"].ids, ["C"], "the seed's read is newer");
        let ids = by_id(&all);
        assert_eq!(ids["A"], ["2", "1"], "most subscribed first");
        assert_eq!(ids["A2"], ["1"]);

        let (_t, p) = crate::store::tests::temp_paths();
        save(&p, all.items.clone(), Some(100)).unwrap();
        save(&p, [("4".into(), e(1, 1, &[]))].into(), None).unwrap();
        assert_eq!(index(&p).items.len(), 4);
        let out = p.data.join("seed.json.gz");
        assert_eq!(write_seed(&p, &out).unwrap(), (3, 4), "the item without mod IDs stays out");
        assert_eq!(parse_seed(&std::fs::read(&out).unwrap()).unwrap().at, 100);
        assert_eq!(kept_by_id(&p)["A"], ["2", "1"]);
        save(&p, [("5".into(), e(1, 1000, &["A"]))].into(), None).unwrap();
        assert_eq!(kept_by_id(&p)["A"], ["5", "2", "1"], "kept until the index is saved");
        // the seed built into PZMM parses
        parse_seed(SEED).unwrap();
    }
}
