//! Map mods: which world cells each map fills, read from the map files themselves.
//!
//! A map folder (`media/maps/<Name>/`) holds one `world_X_Y.lotpack` and one `X_Y.lotheader`
//! per cell. B41 and B42 both ship the two, but their cells differ in size (300 tiles in B41,
//! 256 in B42), so cells are only comparable within one build: B41 reads a mod's root
//! `media/maps`, B42 its version folders (`42.x/`, `common/`). Cell lists are cached in
//! `<data>/maps-cache.json`, keyed by the folder's (and its map.info's) modified time.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::{Paths, is_version_dir};
use crate::sources::steam_manifest::Session;
use crate::sources::steam_workshop::{self, Item};
use crate::store;

pub type Cell = (i32, i32);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MapFolder {
    /// The folder name: what `Map=` lists.
    pub folder: String,
    pub title: String,
    /// map.info `lots=`: map folders it builds on (usually `Muldraugh, KY`).
    pub lots: Vec<String>,
    pub cells: Vec<Cell>,
}

/// A mod with map folders (installed, or checked on the Workshop without installing).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MapMod {
    pub id: String,
    pub name: String,
    /// As in the scan: `steam:<item>`, `workshop:<item>`, `manual`...
    pub source: String,
    pub requires: Vec<String>,
    pub maps: Vec<MapFolder>,
}

impl MapMod {
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.maps.iter().flat_map(|m| &m.cells)
    }
}

/// `world_X_Y.lotpack` or `X_Y.lotheader` -> (X, Y).
fn cell_of(name: &str) -> Option<Cell> {
    let xy = name.strip_prefix("world_").and_then(|n| n.strip_suffix(".lotpack")).or_else(|| name.strip_suffix(".lotheader"))?;
    let (x, y) = xy.split_once('_')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

/// A map folder's title and `lots=` from its map.info text. B42 titles point at a
/// translation file ("See media/lua/shared/Translate/EN/<map>/title.txt"): `title_at`
/// reads that path (relative to the version folder).
fn parse_map_info(folder: &str, info: &str, title_at: impl FnOnce(&str) -> String) -> MapFolder {
    // map.info repeats `lots=` once per folder, and folder names hold commas ("Muldraugh, KY")
    let value = |l: &str, key: &str| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().to_string());
    let lots = info.lines().filter_map(|l| value(l, "lots")).filter(|v| !v.is_empty()).collect();
    let mut title = info.lines().find_map(|l| value(l, "title")).unwrap_or_default();
    if let Some(rel) = title.strip_prefix("See ") {
        title = title_at(rel.trim()).trim().to_string();
    }
    if title.is_empty() {
        title = folder.to_string();
    }
    MapFolder { folder: folder.to_string(), title, lots, cells: vec![] }
}

/// One map folder, read from disk.
pub fn read_folder(dir: &Path) -> MapFolder {
    let mut cells: Vec<Cell> = fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| cell_of(e.file_name().to_str()?)).collect();
    cells.sort();
    cells.dedup();
    let folder = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let info = fs::read(dir.join("map.info")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let root = dir.parent().and_then(Path::parent).and_then(Path::parent).unwrap_or(dir);
    MapFolder { cells, ..parse_map_info(&folder, &info, |rel| fs::read_to_string(root.join(rel)).unwrap_or_default()) }
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    stamp: u64,
    map: MapFolder,
}

/// Map folders already read, by path. Only the entries used since `load` are saved back.
#[derive(Default)]
pub struct Cache {
    old: BTreeMap<String, Entry>,
    used: BTreeMap<String, Entry>,
}

fn cache_file(paths: &Paths) -> PathBuf {
    paths.data.join("maps-cache.json")
}

/// Newest modified time of the folder and its map.info (nanoseconds): adding or removing a
/// cell changes the folder's time, editing map.info changes its own.
fn stamp(dir: &Path) -> u64 {
    let t = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as u64);
    t(dir).max(t(&dir.join("map.info")))
}

impl Cache {
    pub fn load(paths: &Paths) -> Cache {
        let old = fs::read(cache_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Cache { old, used: BTreeMap::new() }
    }

    /// Write back the entries used this time (dropping folders that are gone), if anything changed.
    pub fn save(self, paths: &Paths) -> Result<()> {
        let same = self.used.len() == self.old.len() && self.used.iter().all(|(k, e)| self.old.get(k).is_some_and(|o| o.stamp == e.stamp));
        if same {
            return Ok(());
        }
        fs::create_dir_all(&paths.data)?;
        crate::write_atomic(&cache_file(paths), serde_json::to_vec(&self.used)?)?;
        Ok(())
    }

    /// The game's river cells for `build` (see `water_cells`), kept while worldmap.xml is unchanged.
    pub fn water(&mut self, game: &Path, build: u8) -> Vec<Cell> {
        let file = game.join("media/maps/Muldraugh, KY/worldmap.xml");
        let key = format!("{}#water{build}", file.display());
        let stamp = stamp(&file);
        let entry = match self.old.get(&key) {
            Some(e) if e.stamp == stamp => e.clone(),
            _ => {
                let cells = fs::read_to_string(&file).map(|t| water_cells(&t, if build == 41 { 300 } else { 256 })).unwrap_or_default();
                Entry { stamp, map: MapFolder { folder: "water".into(), cells, ..Default::default() } }
            }
        };
        let cells = entry.map.cells.clone();
        self.used.insert(key, entry);
        cells
    }

    pub fn folder(&mut self, dir: &Path) -> MapFolder {
        let key = dir.to_string_lossy().into_owned();
        let stamp = stamp(dir);
        let entry = match self.old.get(&key) {
            Some(e) if e.stamp == stamp => e.clone(),
            _ => Entry { stamp, map: read_folder(dir) },
        };
        let map = entry.map.clone();
        self.used.insert(key, entry);
        map
    }
}

/// Which of a mod's version folders (`42.x`, `common`) hold its maps on `build`: none in
/// B41 (the mod folder itself), in B42 `common/` and the newest version folder (as the game
/// picks; see `find_mods`).
fn pick_roots<'a>(names: impl IntoIterator<Item = &'a str>, build: u8) -> Vec<&'a str> {
    if build == 41 {
        return vec![""];
    }
    let mut out = vec![];
    let mut newest: Option<(Vec<u32>, &str)> = None;
    for name in names.into_iter().filter(|n| is_version_dir(n)) {
        if name == "common" {
            out.push(name);
        } else {
            let v: Vec<u32> = name.split('.').filter_map(|p| p.parse().ok()).collect();
            if newest.as_ref().is_none_or(|(n, _)| v > *n) {
                newest = Some((v, name));
            }
        }
    }
    out.extend(newest.map(|n| n.1));
    out
}

/// Where a mod's `media/maps` lives for `build` (see `pick_roots`).
fn map_roots(mod_dir: &Path, build: u8) -> Vec<PathBuf> {
    let names: Vec<String> =
        fs::read_dir(mod_dir).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    pick_roots(names.iter().map(String::as_str), build).into_iter().map(|r| mod_dir.join(r)).collect()
}

/// A mod's map folders for `build` (one per name, cells merged when two roots ship it).
pub fn mod_maps(mod_dir: &Path, build: u8, cache: &mut Cache) -> Vec<MapFolder> {
    let mut out: Vec<MapFolder> = vec![];
    for root in map_roots(mod_dir, build) {
        for e in fs::read_dir(root.join("media/maps")).into_iter().flatten().flatten() {
            if !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let m = cache.folder(&e.path());
            match out.iter_mut().find(|o| o.folder == m.folder) {
                Some(o) => {
                    o.cells.extend(m.cells);
                    o.cells.sort();
                    o.cells.dedup();
                }
                None => out.push(m),
            }
        }
    }
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    out
}

/// The installed mods that add maps for `build`, by name.
pub fn installed(mods: &BTreeMap<String, store::Row>, build: u8, cache: &mut Cache) -> Vec<MapMod> {
    let mut out: Vec<MapMod> = mods
        .values()
        .filter_map(|m| {
            let maps = mod_maps(&m.dir, build, cache);
            (!maps.is_empty()).then(|| MapMod { id: m.id.clone(), name: m.name.clone(), source: m.source.clone(), requires: m.requires.clone(), maps })
        })
        .collect();
    out.sort_by_key(|m| (if m.name.is_empty() { &m.id } else { &m.name }).to_lowercase());
    out
}

/// The game's own maps (`<game>/media/maps/*`): its folder names, and its cells when the
/// install is `build` (B42's lotheaders start with `LOTH`, B41's don't), else None.
pub fn vanilla(game: &Path, build: u8, cache: &mut Cache) -> (Vec<String>, Option<Vec<Cell>>) {
    let dir = game.join("media/maps");
    let (mut folders, mut cells, mut game_build) = (vec![], HashSet::new(), None);
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        if !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let m = cache.folder(&e.path());
        if let Some((x, y)) = m.cells.first()
            && game_build.is_none()
        {
            let head = fs::read(e.path().join(format!("{x}_{y}.lotheader"))).unwrap_or_default();
            game_build = Some(if head.starts_with(b"LOTH") { 42 } else { 41 });
        }
        cells.extend(m.cells);
        folders.push(m.folder);
    }
    folders.sort();
    let mut cells: Vec<Cell> = cells.into_iter().collect();
    cells.sort();
    (folders, (game_build == Some(build)).then_some(cells))
}

/// Cells (`cell_size` tiles: 256 in B42, 300 in B41) that are mostly water on the game's
/// world map. worldmap.xml draws features as polygons inside 300-tile cells, one `<point>`
/// per line; a cell counts when at least half of 16 sample points fall in a water polygon.
pub fn water_cells(xml: &str, cell_size: i32) -> Vec<Cell> {
    let attr = |l: &str, k: &str| -> i32 { l.split_once(&format!(" {k}=\"")).and_then(|(_, v)| v.split('"').next()?.parse().ok()).unwrap_or(0) };
    let (mut origin, mut ring, mut rings, mut water) = ((0, 0), vec![], vec![], false);
    let mut hits: BTreeMap<Cell, u32> = BTreeMap::new();
    for l in xml.lines().map(str::trim) {
        if l.starts_with("<cell ") {
            origin = (attr(l, "x") * 300, attr(l, "y") * 300);
        } else if l.starts_with("<feature") {
            (rings, water) = (vec![], false);
        } else if l.starts_with("<point ") {
            ring.push(((origin.0 + attr(l, "x")) as f64, (origin.1 + attr(l, "y")) as f64));
        } else if l.starts_with("</coordinates") {
            rings.push(std::mem::take(&mut ring));
        } else if l.starts_with("<property name=\"water\"") {
            water = true;
        } else if l.starts_with("</feature") && water {
            let pts = || rings.iter().flatten();
            let (x0, x1) = (pts().map(|p| p.0).fold(f64::MAX, f64::min), pts().map(|p| p.0).fold(f64::MIN, f64::max));
            let (y0, y1) = (pts().map(|p| p.1).fold(f64::MAX, f64::min), pts().map(|p| p.1).fold(f64::MIN, f64::max));
            let size = cell_size as f64;
            for cx in (x0 / size).floor() as i32..=(x1 / size).floor() as i32 {
                for cy in (y0 / size).floor() as i32..=(y1 / size).floor() as i32 {
                    for i in 0..16 {
                        let (px, py) = ((cx as f64 + (i % 4) as f64 / 4.0 + 0.125) * size, (cy as f64 + (i / 4) as f64 / 4.0 + 0.125) * size);
                        // even-odd over every ring (holes included)
                        let mut inside = false;
                        for r in &rings {
                            for (a, b) in r.iter().zip(r.iter().cycle().skip(1)) {
                                if (a.1 > py) != (b.1 > py) && px < a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                                    inside = !inside;
                                }
                            }
                        }
                        if inside {
                            *hits.entry((cx, cy)).or_default() += 1;
                        }
                    }
                }
            }
        }
    }
    hits.into_iter().filter(|(_, n)| *n >= 8).map(|(c, _)| c).collect()
}

/// One cell of the grid: what fills it, among vanilla and the shown maps.
#[derive(Debug, PartialEq, Serialize)]
pub struct GridCell {
    pub x: i32,
    pub y: i32,
    /// "vanilla" (the game's only), "water" (the game's, mostly river), "map" (one map),
    /// "over" (one map over a vanilla area, usually on purpose) or "conflict" (two or more maps).
    pub kind: &'static str,
    /// Ids of the shown maps that fill it.
    pub maps: Vec<String>,
}

pub fn grid(vanilla: &[Cell], water: &[Cell], shown: &[&MapMod]) -> Vec<GridCell> {
    let base: HashSet<&Cell> = vanilla.iter().collect();
    let water: HashSet<&Cell> = water.iter().collect();
    let mut cells: BTreeMap<Cell, Vec<String>> = vanilla.iter().map(|c| (*c, vec![])).collect();
    for m in shown {
        for c in m.cells() {
            let ids = cells.entry(*c).or_default();
            if !ids.contains(&m.id) {
                ids.push(m.id.clone());
            }
        }
    }
    cells
        .into_iter()
        .map(|((x, y), maps)| {
            let kind = match (maps.len(), base.contains(&(x, y))) {
                (0, _) if water.contains(&(x, y)) => "water",
                (0, _) => "vanilla",
                (1, false) => "map",
                (1, true) => "over",
                _ => "conflict",
            };
            GridCell { x, y, kind, maps }
        })
        .collect()
}

// --- quick select: the most maps that fit together ---

/// Components up to this size are solved exactly; bigger ones greedily.
const EXACT_UP_TO: usize = 40;

/// The largest set of maps from `all` that share no cell with each other or with the `fixed`
/// ones (enabled maps, which stay shown). Ties go to the higher `rank` (installed, popular).
/// Covering vanilla isn't a conflict; maps without cells (spawn points only) are left out.
/// Ids come back in `all`'s order.
pub fn quick_select(all: &[MapMod], fixed: &[String], rank: impl Fn(&MapMod) -> u64) -> Vec<String> {
    let taken: HashSet<&Cell> = all.iter().filter(|m| fixed.contains(&m.id)).flat_map(|m| m.cells()).collect();
    let cands: Vec<&MapMod> = all.iter().filter(|m| !fixed.contains(&m.id) && m.cells().next().is_some() && !m.cells().any(|c| taken.contains(c))).collect();
    // the conflict graph: maps sharing a cell
    let mut at: BTreeMap<&Cell, Vec<usize>> = BTreeMap::new();
    for (i, m) in cands.iter().enumerate() {
        for c in m.cells() {
            let v = at.entry(c).or_default();
            if !v.contains(&i) {
                v.push(i);
            }
        }
    }
    let mut adj: Vec<Vec<usize>> = vec![vec![]; cands.len()];
    for ids in at.values() {
        for &a in ids {
            for &b in ids {
                if b != a && !adj[a].contains(&b) {
                    adj[a].push(b);
                }
            }
        }
    }
    let weight: Vec<u64> = cands.iter().map(|m| rank(m)).collect();
    let mut pick = vec![false; cands.len()];
    let mut seen = vec![false; cands.len()];
    for start in 0..cands.len() {
        if seen[start] {
            continue;
        }
        // one connected component
        let (mut comp, mut stack) = (vec![], vec![start]);
        seen[start] = true;
        while let Some(v) = stack.pop() {
            comp.push(v);
            for &n in &adj[v] {
                if !seen[n] {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        for v in if comp.len() <= EXACT_UP_TO { exact(&comp, &adj, &weight) } else { greedy(&comp, &adj, &weight) } {
            pick[v] = true;
        }
    }
    cands.iter().zip(pick).filter(|(_, p)| *p).map(|(m, _)| m.id.clone()).collect()
}

/// Branch and bound over a small component (bitmasks): most maps, then most weight.
fn exact(comp: &[usize], adj: &[Vec<usize>], weight: &[u64]) -> Vec<usize> {
    let local: Vec<u64> = comp.iter().map(|v| adj[*v].iter().filter_map(|n| comp.iter().position(|c| c == n)).fold(0, |m, i| m | 1 << i)).collect();
    let w: Vec<u128> = comp.iter().map(|v| weight[*v] as u128).collect();
    struct Best(u32, u128, u64);
    fn bits(m: u64) -> impl Iterator<Item = usize> {
        (0..64).filter(move |i| m >> i & 1 == 1)
    }
    fn go(rem: u64, cur: u64, cw: u128, adj: &[u64], w: &[u128], best: &mut Best) {
        let rw: u128 = bits(rem).map(|i| w[i]).sum();
        if (cur.count_ones() + rem.count_ones(), cw + rw) <= (best.0, best.1) {
            return; // can't beat the best so far
        }
        let Some(v) = bits(rem).max_by_key(|&i| (adj[i] & rem).count_ones()) else {
            *best = Best(cur.count_ones(), cw, cur);
            return;
        };
        go(rem & !(1 << v) & !adj[v], cur | 1 << v, cw + w[v], adj, w, best);
        if adj[v] & rem != 0 {
            go(rem & !(1 << v), cur, cw, adj, w, best); // leaving out a map with no rival never helps
        }
    }
    let mut best = Best(0, 0, 0);
    go(u64::MAX >> (64 - comp.len()), 0, 0, &local, &w, &mut best);
    bits(best.2).map(|i| comp[i]).collect()
}

/// Big components: keep taking the map with the fewest rivals left (then the heaviest).
// ponytail: min-degree greedy is near-optimal on map overlaps, not exact; raise EXACT_UP_TO if it shows
fn greedy(comp: &[usize], adj: &[Vec<usize>], weight: &[u64]) -> Vec<usize> {
    let mut left: Vec<usize> = comp.to_vec();
    let mut out = vec![];
    while let Some(&v) = left.iter().min_by_key(|&&v| (adj[v].iter().filter(|n| left.contains(n)).count(), std::cmp::Reverse(weight[v]))) {
        out.push(v);
        left.retain(|n| *n != v && !adj[v].contains(n));
    }
    out
}

// --- Workshop maps read without installing ---

/// A Workshop item read from its file list (or a download), without installing it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Checked {
    pub title: String,
    pub author: String,
    pub subscriptions: u64,
    /// Its Workshop update time when read: a newer one means reading it again.
    pub updated: u64,
    /// All its files, in bytes.
    pub size: u64,
    /// Checked by hand ("Check a Workshop map") rather than by the Workshop scan.
    pub manual: bool,
    /// Its map mods per build: B41 and B42 read different folders.
    pub builds: BTreeMap<u8, Vec<MapMod>>,
}

impl Checked {
    fn from_item(item: &Item) -> Checked {
        Checked { title: item.title.clone(), author: item.author.clone(), subscriptions: item.subscriptions, updated: item.time_updated, ..Default::default() }
    }
}

/// Whether an item updated at `updated` has to be read (again) for `build`.
pub fn stale(old: Option<&Checked>, updated: u64, build: u8) -> bool {
    old.is_none_or(|c| c.updated != updated || !c.builds.contains_key(&build))
}

fn checked_file(paths: &Paths) -> PathBuf {
    paths.data.join("maps-checked.json")
}

/// Workshop items read without installing, by item id. (A file from before builds were kept
/// apart doesn't parse, so it's dropped and read again.)
pub fn checked(paths: &Paths) -> BTreeMap<String, Checked> {
    fs::read(checked_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Add or replace entries in maps-checked.json (the scan and "Check" can both write).
pub fn save_checked(paths: &Paths, new: BTreeMap<String, Checked>) -> Result<()> {
    static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _w = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = checked(paths);
    all.extend(new);
    fs::create_dir_all(&paths.data)?;
    Ok(crate::write_atomic(&checked_file(paths), serde_json::to_vec(&all)?)?)
}

/// The map mods in an item's file list (paths with `/`) for `build`, by the rules `mod_maps`
/// reads folders with. `read` gives a listed file's text: mod.info (id, name, require=),
/// map.info (title, lots=) and B42 title files; nothing else is fetched.
pub fn from_listing(item: &str, files: &[String], build: u8, read: &mut dyn FnMut(&str) -> Result<String>) -> Result<Vec<MapMod>> {
    let lower: Vec<String> = files.iter().map(|f| f.to_ascii_lowercase()).collect();
    let find = |p: &str| lower.iter().position(|l| *l == p.to_ascii_lowercase()).map(|i| files[i].clone());
    let split = |p: &str| -> (String, String) {
        let (a, b) = p.rsplit_once('/').unwrap_or(("", p));
        (a.to_string(), b.to_string())
    };
    let join = |a: &str, b: &str| if a.is_empty() || b.is_empty() { format!("{a}{b}") } else { format!("{a}/{b}") };
    // mod folders: where each mod.info sits (a version folder's parent)
    let mut dirs: Vec<String> = vec![];
    for f in files.iter().filter(|f| split(f).1.eq_ignore_ascii_case("mod.info")) {
        let parent = split(f).0;
        let dir = if !parent.is_empty() && is_version_dir(&split(&parent).1) { split(&parent).0 } else { parent };
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    let mut out = vec![];
    for dir in dirs {
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/").to_ascii_lowercase() };
        let under: Vec<usize> = (0..files.len()).filter(|i| lower[*i].starts_with(&prefix)).collect();
        let mut versions: Vec<String> =
            under.iter().filter_map(|i| files[*i][prefix.len()..].split_once('/').map(|v| v.0.to_string())).filter(|v| is_version_dir(v)).collect();
        versions.sort();
        versions.dedup();
        // which mod.info the game reads: the root one in B41, the newest version folder's in B42
        let info = if build == 41 {
            find(&join(&dir, "mod.info"))
        } else {
            let mut vs: Vec<&String> = versions.iter().filter(|v| find(&join(&dir, &join(v, "mod.info"))).is_some()).collect();
            vs.sort_by_key(|v| v.split('.').filter_map(|p| p.parse().ok()).collect::<Vec<u32>>());
            vs.last().and_then(|v| find(&join(&dir, &join(v, "mod.info"))))
        };
        let Some(info) = info else { continue }; // not a mod on this build
        let mut maps: Vec<MapFolder> = vec![];
        for root in pick_roots(versions.iter().map(String::as_str), build) {
            let base = join(&join(&dir, root), "media/maps/");
            let base_lc = base.to_ascii_lowercase();
            for i in &under {
                // <base><folder>/<file>: a file directly in a map folder
                let Some((_, file)) = lower[*i].strip_prefix(&base_lc).and_then(|r| r.split_once('/')).filter(|(_, f)| !f.contains('/')) else { continue };
                let real = &files[*i][base.len()..];
                let (folder, name) = (&real[..real.len() - file.len() - 1], &real[real.len() - file.len()..]);
                let at = match maps.iter().position(|m| m.folder == folder) {
                    Some(p) => p,
                    None => {
                        let text = match find(&format!("{base}{folder}/map.info")) {
                            Some(p) => read(&p)?,
                            None => String::new(),
                        };
                        // a B42 title in a translation file: read only when map.info points at one
                        let title_file = text.lines().find_map(|l| l.trim().strip_prefix("title=See ")).and_then(|rel| find(&join(&join(&dir, root), rel.trim())));
                        let title = match title_file {
                            Some(p) => read(&p)?,
                            None => String::new(),
                        };
                        maps.push(parse_map_info(folder, &text, |_| title));
                        maps.len() - 1
                    }
                };
                maps[at].cells.extend(cell_of(name));
            }
        }
        if maps.is_empty() {
            continue;
        }
        for m in &mut maps {
            m.cells.sort();
            m.cells.dedup();
        }
        maps.sort_by(|a, b| a.folder.cmp(&b.folder));
        let i = super::parse_info(&read(&info)?);
        // no id= (a broken mod.info): the folder name, as the game shows it
        let id = i.get("id").filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| split(&dir).1);
        out.push(MapMod { name: i.get("name").cloned().unwrap_or_default(), requires: super::requires(&i), source: format!("workshop:{item}"), id, maps });
    }
    Ok(out)
}

/// Read a Workshop item's maps (both builds) from its manifest on `session`.
pub fn read_listing(session: &Session, item: &Item) -> Result<Checked> {
    if item.hcontent == 0 {
        bail!("workshop:{}: Steam lists no files for it", item.id);
    }
    let files = session.files(item.hcontent)?;
    let names: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    let mut read = |p: &str| -> Result<String> {
        if let Some(t) = texts.get(p) {
            return Ok(t.clone());
        }
        let f = files.iter().find(|f| f.path == p).context("not in the file list")?;
        let t = String::from_utf8_lossy(&session.read(f).with_context(|| format!("reading {p}"))?).into_owned();
        texts.insert(p.to_string(), t.clone());
        Ok(t)
    };
    let mut c = Checked { size: files.iter().map(|f| f.size).sum(), ..Checked::from_item(item) };
    for build in [41, 42] {
        c.builds.insert(build, from_listing(&item.id, &names, build, &mut read)?);
    }
    Ok(c)
}

/// The map mods in a downloaded item folder, per build (not cached: the folder is about to go).
pub fn read_item(dir: &Path, item: &str) -> BTreeMap<u8, Vec<MapMod>> {
    let mut cache = Cache::default();
    let found = super::find_mods(dir, 8);
    [41, 42]
        .into_iter()
        .map(|build| {
            let mods = found
                .iter()
                .filter_map(|m| {
                    let maps = mod_maps(&m.dir, build, &mut cache);
                    (!maps.is_empty()).then(|| MapMod { id: m.id.clone(), name: m.name.clone(), source: format!("workshop:{item}"), requires: m.requires.clone(), maps })
                })
                .collect();
            (build, mods)
        })
        .collect()
}

/// Read Workshop `item`'s maps from its file list (keeping them in `maps-checked.json`).
/// Nothing is installed. When Steam won't give the list and `download` is set, SteamCMD
/// downloads the item instead, and the download is deleted after reading it.
pub fn check(paths: &Paths, item: &str, download: bool) -> Result<Checked> {
    let info = steam_workshop::details(&[item])?.pop().with_context(|| format!("there's no Workshop item {item}"))?;
    let mut c = match Session::open().and_then(|s| read_listing(&s, &info)) {
        Ok(c) => c,
        Err(e) if !download => return Err(e.context(format!("reading the file list of {} from Steam", info.title))),
        Err(e) => {
            crate::log!("workshop:{item}: no file list from Steam ({e:#}), downloading it with SteamCMD instead");
            let got = steam_workshop::download(paths, super::STEAM_APP, &[item])?;
            let dir = got.into_iter().next().and_then(|(_, d)| d).with_context(|| format!("workshop:{item}: download failed"))?;
            let builds = read_item(&dir, item);
            fs::remove_dir_all(&dir).with_context(|| format!("deleting the download in {}", dir.display()))?;
            Checked { size: info.file_size, builds, ..Checked::from_item(&info) }
        }
    };
    c.manual = true;
    save_checked(paths, [(item.to_string(), c.clone())].into())?;
    Ok(c)
}

/// Where a Workshop scan is.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Progress {
    /// "listing" (Map-tagged items, page by page), "reading" (their file lists) or "done".
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
    /// Items whose file list couldn't be read (tried again next scan).
    pub failed: usize,
}

/// Every Map-tagged Workshop item for `build` (all result pages), then the file lists of the
/// ones not read yet or updated since: one anonymous Steam session, one item at a time.
/// Results are saved as it goes; `progress` returning false stops it.
pub fn scan_workshop(paths: &Paths, build: u8, progress: &mut dyn FnMut(&Progress) -> bool) -> Result<Progress> {
    let mut p = Progress { phase: "listing", ..Default::default() };
    let mut items: Vec<Item> = vec![];
    for page in 1.. {
        let q = steam_workshop::Query { required: vec!["Map".into(), format!("Build {build}")], sort: "mostrecent".into(), per_page: 50, page, ..Default::default() };
        let got = match steam_workshop::browse(super::STEAM_APP, &q) {
            Ok(g) => g,
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_secs(20)); // Steam's request limit resets within a minute
                steam_workshop::browse(super::STEAM_APP, &q)?
            }
        };
        let empty = got.items.is_empty();
        for i in got.items {
            // newest first: an upload during the scan shifts the pages, repeating an item
            if !items.iter().any(|o| o.id == i.id) {
                items.push(i);
            }
        }
        (p.done, p.total) = (items.len(), got.total as usize);
        if !progress(&p) {
            return Ok(p);
        }
        if empty || items.len() >= p.total {
            break;
        }
    }
    let old = checked(paths);
    let (mut todo, mut refreshed) = (vec![], BTreeMap::new());
    for i in &items {
        match old.get(&i.id) {
            Some(c) if !stale(Some(c), i.time_updated, build) => {
                refreshed.insert(i.id.clone(), Checked { title: i.title.clone(), author: i.author.clone(), subscriptions: i.subscriptions, ..c.clone() });
            }
            _ => todo.push(i),
        }
    }
    save_checked(paths, refreshed)?;
    p = Progress { phase: "reading", total: todo.len(), ..Default::default() };
    if !todo.is_empty() && progress(&p) {
        let session = Session::open()?;
        let (mut batch, mut streak) = (BTreeMap::new(), 0);
        for i in todo {
            match read_listing(&session, i) {
                Ok(mut c) => {
                    c.manual = old.get(&i.id).is_some_and(|o| o.manual);
                    batch.insert(i.id.clone(), c);
                    streak = 0;
                }
                Err(e) => {
                    crate::log!("workshop:{} ({}): {e:#}", i.id, i.title);
                    (p.failed, streak) = (p.failed + 1, streak + 1);
                    if streak >= 5 {
                        save_checked(paths, batch)?;
                        bail!("Steam stopped answering ({e:#}); what was read is kept, scan again later");
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2 * streak));
                }
            }
            p.done += 1;
            if batch.len() >= 20 {
                save_checked(paths, std::mem::take(&mut batch))?;
            }
            if !progress(&p) {
                break;
            }
        }
        save_checked(paths, batch)?;
    }
    p.phase = "done";
    progress(&p);
    Ok(p)
}

// --- compare with a server ---

/// A map a server loads that you don't, and the map mod that adds it (if PZMM knows one).
#[derive(Debug, PartialEq, Serialize)]
pub struct ServerMap {
    pub folder: String,
    pub id: Option<String>,
    pub item: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Compare {
    /// In the server's `Map=` but not among yours.
    pub only_server: Vec<ServerMap>,
    /// Your map folders the server doesn't load.
    pub only_you: Vec<String>,
}

/// Compare a server config's `Map=` with `yours` (map folders), leaving out the game's own
/// maps. A missing map's provider is the map mod in `all` (installed first) that adds it,
/// preferring the server's own mods and Workshop items.
pub fn compare(server: &super::server::ServerMods, yours: &[String], vanilla: &[String], all: &[MapMod]) -> Compare {
    let ours = |f: &String| !vanilla.contains(f);
    let only_server = server
        .maps
        .iter()
        .filter(|f| ours(f) && !yours.contains(f))
        .map(|f| {
            let best = all.iter().enumerate().filter(|(_, m)| m.maps.iter().any(|x| x.folder == *f)).min_by_key(|(i, m)| {
                let item = workshop_item(&m.source);
                (!server.mods.contains(&m.id), !item.is_some_and(|it| server.workshop.iter().any(|w| w == it)), *i)
            });
            ServerMap { folder: f.clone(), id: best.map(|(_, m)| m.id.clone()), item: best.and_then(|(_, m)| workshop_item(&m.source)).map(String::from) }
        })
        .collect();
    let only_you = yours.iter().filter(|f| ours(f) && !server.maps.contains(f)).cloned().collect();
    Compare { only_server, only_you }
}

// --- presets ---

fn presets_file(paths: &Paths) -> PathBuf {
    paths.data.join("map-presets.json")
}

/// Named sets of maps (mod ids) saved from the Maps tab.
pub fn presets(paths: &Paths) -> BTreeMap<String, Vec<String>> {
    fs::read(presets_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Save preset `name` (None deletes it).
pub fn set_preset(paths: &Paths, name: &str, ids: Option<Vec<String>>) -> Result<()> {
    let mut all = presets(paths);
    match ids {
        Some(ids) => all.insert(name.trim().to_string(), ids),
        None => all.remove(name),
    };
    fs::create_dir_all(&paths.data)?;
    Ok(crate::write_atomic(&presets_file(paths), serde_json::to_vec_pretty(&all)?)?)
}

/// The Workshop item in a source string (`steam:<id>` or `workshop:<id>`).
pub fn workshop_item(source: &str) -> Option<&str> {
    source.strip_prefix("steam:").or_else(|| source.strip_prefix("workshop:")).filter(|i| !i.is_empty())
}

/// Clipboard text for the shown maps: each map's Workshop link, mod id and folders, then the
/// server lines. `Mods=` puts each mod's requirements (from `mods`, the scan, recursively)
/// before it; `map_line` is the arranged `Map=` list (see `server::map_order`).
pub fn export(shown: &[&MapMod], mods: &BTreeMap<String, store::Row>, map_line: &[String], b41: bool) -> String {
    let (mut ids, mut items, mut missing) = (vec![], vec![], vec![]);
    fn visit(id: &str, requires: &[String], mods: &BTreeMap<String, store::Row>, ids: &mut Vec<String>, missing: &mut Vec<String>, depth: usize) {
        if ids.iter().any(|i| i == id) || depth > 50 {
            return;
        }
        for r in requires {
            match mods.get(r) {
                Some(m) => visit(r, &m.requires, mods, ids, missing, depth + 1),
                None if !missing.contains(r) => missing.push(r.clone()),
                None => {}
            }
        }
        ids.push(id.to_string());
    }
    let mut text = String::new();
    for m in shown {
        visit(&m.id, &m.requires, mods, &mut ids, &mut missing, 0);
        let name = if m.name.is_empty() { &m.id } else { &m.name };
        text += &format!("{name}\n");
        if let Some(item) = workshop_item(&m.source) {
            text += &format!("  https://steamcommunity.com/sharedfiles/filedetails/?id={item}\n");
        }
        let folders: Vec<&str> = m.maps.iter().map(|f| f.folder.as_str()).collect();
        text += &format!("  Mod ID: {} · Map folder: {}\n", m.id, folders.join("; "));
    }
    let needed: Vec<&String> = ids.iter().filter(|i| !shown.iter().any(|m| m.id == **i)).collect();
    if !needed.is_empty() {
        text += "\nRequired mods\n";
        for id in needed {
            let row = mods.get(id);
            let item = row.and_then(|r| workshop_item(&r.source));
            text += &format!("  {}{}\n", id, item.map(|i| format!(": https://steamcommunity.com/sharedfiles/filedetails/?id={i}")).unwrap_or_default());
        }
    }
    if !missing.is_empty() {
        text += &format!("\nRequired but not installed: {}\n", missing.join(", "));
    }
    for id in &ids {
        let source = mods.get(id).map(|r| r.source.as_str()).or_else(|| shown.iter().find(|m| m.id == *id).map(|m| m.source.as_str()));
        if let Some(item) = source.and_then(workshop_item)
            && !items.iter().any(|i| i == item)
        {
            items.push(item.to_string());
        }
    }
    let prefix = if b41 { "" } else { "\\" };
    text += &format!("\nMods={}\n", ids.iter().map(|i| format!("{prefix}{i}")).collect::<Vec<_>>().join(";"));
    text += &format!("WorkshopItems={}\n", items.join(";"));
    if !map_line.is_empty() {
        text += &format!("Map={}\n", map_line.join(";"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "").unwrap();
    }

    fn map(id: &str, cells: &[Cell]) -> MapMod {
        MapMod { id: id.into(), name: id.into(), maps: vec![MapFolder { folder: id.into(), cells: cells.to_vec(), ..Default::default() }], ..Default::default() }
    }

    #[test]
    fn reads_cells_per_build() {
        let t = tempfile::tempdir().unwrap();
        // a mod shipping both layouts, like Louisville Riverboat: B41 at the root, B42 in common/
        let m = t.path().join("Riverboat");
        touch(&m.join("mod.info"));
        touch(&m.join("media/maps/Riverboat/43_3.lotheader"));
        touch(&m.join("media/maps/Riverboat/world_43_3.lotpack"));
        touch(&m.join("media/maps/Riverboat/chunkdata_43_3.bin"));
        fs::write(m.join("media/maps/Riverboat/map.info"), "title=Riverboat\nlots=Muldraugh, KY\nlots=Other\n").unwrap();
        touch(&m.join("common/media/maps/Riverboat/51_4.lotheader"));
        touch(&m.join("common/media/maps/Riverboat/world_51_4.lotpack"));
        touch(&m.join("common/media/maps/Riverboat/world_52_4.lotpack"));
        fs::write(m.join("common/media/maps/Riverboat/map.info"), "title=See media/lua/shared/Translate/EN/Riverboat/title.txt\n").unwrap();
        fs::create_dir_all(m.join("common/media/lua/shared/Translate/EN/Riverboat")).unwrap();
        fs::write(m.join("common/media/lua/shared/Translate/EN/Riverboat/title.txt"), "Louisville Riverboat\n").unwrap();
        touch(&m.join("42.9/media/maps/Old/world_1_1.lotpack")); // an older version folder: ignored
        touch(&m.join("42.13/mod.info"));

        let mut cache = Cache::default();
        let b41 = mod_maps(&m, 41, &mut cache);
        assert_eq!(b41, [MapFolder { folder: "Riverboat".into(), title: "Riverboat".into(), lots: vec!["Muldraugh, KY".into(), "Other".into()], cells: vec![(43, 3)] }]);
        let b42 = mod_maps(&m, 42, &mut cache);
        assert_eq!(b42.len(), 1);
        assert_eq!((b42[0].title.as_str(), b42[0].cells.as_slice()), ("Louisville Riverboat", [(51, 4), (52, 4)].as_slice()));
        assert_eq!(cell_of("world_-1_7.lotpack"), Some((-1, 7)));
        assert_eq!(cell_of("chunkdata_1_2.bin"), None);
    }

    #[test]
    fn vanilla_cells_only_for_the_games_build() {
        let t = tempfile::tempdir().unwrap();
        let maps = t.path().join("media/maps");
        fs::create_dir_all(maps.join("Muldraugh, KY")).unwrap();
        fs::write(maps.join("Muldraugh, KY/0_1.lotheader"), b"LOTH\x01\0\0\0").unwrap();
        touch(&maps.join("Muldraugh, KY/world_0_1.lotpack"));
        touch(&maps.join("Riverside, KY/map.info"));
        let (folders, cells) = vanilla(t.path(), 42, &mut Cache::default());
        assert_eq!(folders, ["Muldraugh, KY", "Riverside, KY"]);
        assert_eq!(cells, Some(vec![(0, 1)]));
        assert_eq!(vanilla(t.path(), 41, &mut Cache::default()).1, None, "a B42 install has no B41 cells");
    }

    #[test]
    fn classifies_cells() {
        let (a, b) = (map("A", &[(1, 1), (2, 2), (5, 5)]), map("B", &[(2, 2), (3, 3)]));
        let g = grid(&[(1, 1), (3, 3), (9, 9)], &[], &[&a, &b]);
        let kinds: Vec<(Cell, &str, usize)> = g.iter().map(|c| ((c.x, c.y), c.kind, c.maps.len())).collect();
        assert_eq!(kinds, [((1, 1), "over", 1), ((2, 2), "conflict", 2), ((3, 3), "over", 1), ((5, 5), "map", 1), ((9, 9), "vanilla", 0)]);
    }

    #[test]
    fn quick_select_finds_the_most_maps() {
        let names = |v: &[MapMod], fixed: &[&str]| quick_select(v, &fixed.iter().map(|s| s.to_string()).collect::<Vec<_>>(), |_| 0);
        // A (first by name) blocks both B and C: greedy-by-name would stop at one of the three
        let all = [map("A", &[(1, 1), (2, 2)]), map("B", &[(1, 1)]), map("C", &[(2, 2)]), map("Spawn", &[]), map("D", &[(9, 9)])];
        assert_eq!(names(&all, &[]), ["B", "C", "D"]);
        // enabled maps stay: what they cover is taken
        assert_eq!(names(&all, &["B"]), ["C", "D"]);
        // a tie goes to the higher rank (installed, then subscribers)
        let pair = [map("X", &[(5, 5)]), map("Y", &[(5, 5)])];
        assert_eq!(quick_select(&pair, &[], |m| if m.id == "Y" { 7 } else { 1 }), ["Y"]);
        // past the exact limit: a 45-map chain still gets the best answer (23)
        let chain: Vec<MapMod> = (0..45).map(|i| map(&format!("M{i:02}"), &[(i, 0), (i + 1, 0)])).collect();
        assert_eq!(names(&chain, &[]).len(), 23);
    }

    #[test]
    fn reads_maps_from_a_file_list() {
        // Louisville Riverboat style: B41 at the mod root, B42 in common/ and 42.13/; an older 42.9/ is ignored
        let files: Vec<String> = [
            "mods/Riverboat/mod.info",
            "mods/Riverboat/poster.png",
            "mods/Riverboat/media/maps/Riverboat/map.info",
            "mods/Riverboat/media/maps/Riverboat/world_43_3.lotpack",
            "mods/Riverboat/media/maps/Riverboat/43_3.lotheader",
            "mods/Riverboat/42.13/mod.info",
            "mods/Riverboat/42.9/mod.info",
            "mods/Riverboat/42.9/media/maps/Old/world_1_1.lotpack",
            "mods/Riverboat/common/media/maps/Riverboat/map.info",
            "mods/Riverboat/common/media/maps/Riverboat/world_51_4.lotpack",
            "mods/Riverboat/common/media/maps/Riverboat/chunkdata_51_4.bin",
            "mods/Riverboat/42.13/media/maps/Riverboat/world_52_4.lotpack",
            "mods/Riverboat/common/media/lua/shared/Translate/EN/Riverboat/title.txt",
            // a B42-only spawn map (no cells) and a mod without maps
            "mods/Spawn/42/mod.info",
            "mods/Spawn/42/media/maps/Spawn/spawnpoints.lua",
            "mods/Tiles/mod.info",
            "mods/Tiles/media/textures/a.png",
        ]
        .map(String::from)
        .to_vec();
        let texts: BTreeMap<&str, &str> = [
            ("mods/Riverboat/mod.info", "id=Riverboat41\nname=Riverboat\n"),
            ("mods/Riverboat/42.13/mod.info", "\u{feff}id=Louisville_Riverboat\nname=Louisville Riverboat\nrequire=\\Tiles,\\Lib\n"),
            ("mods/Riverboat/media/maps/Riverboat/map.info", "title=Riverboat\nlots=Muldraugh, KY\n"),
            ("mods/Riverboat/common/media/maps/Riverboat/map.info", "title=See media/lua/shared/Translate/EN/Riverboat/title.txt\nlots=Muldraugh, KY\n"),
            ("mods/Riverboat/common/media/lua/shared/Translate/EN/Riverboat/title.txt", "Louisville Riverboat\n"),
            ("mods/Spawn/42/mod.info", "name=No id\n"),
        ]
        .into();
        let mut asked = vec![];
        let mut read = |p: &str| -> Result<String> {
            asked.push(p.to_string());
            texts.get(p).map(|t| t.to_string()).context("not listed")
        };
        let b41 = from_listing("2963883586", &files, 41, &mut read).unwrap();
        assert_eq!(b41.len(), 1, "Spawn has no B41 mod.info; Tiles has no maps");
        assert_eq!((b41[0].id.as_str(), b41[0].source.as_str()), ("Riverboat41", "workshop:2963883586"));
        assert_eq!(b41[0].maps, [MapFolder { folder: "Riverboat".into(), title: "Riverboat".into(), lots: vec!["Muldraugh, KY".into()], cells: vec![(43, 3)] }]);
        let b42 = from_listing("2963883586", &files, 42, &mut read).unwrap();
        assert_eq!(b42.len(), 2);
        let r = &b42[0];
        assert_eq!((r.id.as_str(), r.name.as_str(), r.requires.as_slice()), ("Louisville_Riverboat", "Louisville Riverboat", ["Tiles".to_string(), "Lib".into()].as_slice()));
        assert_eq!(r.maps.len(), 1, "42.9/ is older than 42.13/");
        assert_eq!((r.maps[0].title.as_str(), r.maps[0].cells.as_slice()), ("Louisville Riverboat", [(51, 4), (52, 4)].as_slice()));
        assert_eq!((b42[1].id.as_str(), b42[1].maps[0].folder.as_str(), b42[1].maps[0].cells.len()), ("Spawn", "Spawn", 0), "no id=: the folder name");
        assert!(asked.iter().all(|p| p.ends_with(".info") || p.ends_with(".txt")), "only small text files are fetched: {asked:?}");
        // a fetch that fails fails the item (rather than caching a wrong id)
        assert!(from_listing("1", &files, 42, &mut |_| anyhow::bail!("timeout")).is_err());
    }

    #[test]
    fn checked_items_are_kept_per_build_until_updated() {
        let (_t, paths) = store::tests::temp_paths();
        fs::create_dir_all(&paths.data).unwrap();
        // a file from before builds were kept apart (item -> mods) is dropped
        fs::write(checked_file(&paths), r#"{"111":[{"id":"A","name":"","source":"workshop:111","requires":[],"maps":[]}]}"#).unwrap();
        assert!(checked(&paths).is_empty());
        let c = Checked { updated: 100, builds: [(41, vec![map("Old", &[(1, 1)])]), (42, vec![map("New", &[(9, 9)])])].into(), ..Default::default() };
        save_checked(&paths, [("111".to_string(), c)].into()).unwrap();
        save_checked(&paths, [("222".to_string(), Checked { updated: 5, builds: [(41, vec![])].into(), ..Default::default() })].into()).unwrap();
        let all = checked(&paths);
        assert_eq!(all.len(), 2, "saves add to the file");
        let c = &all["111"];
        assert_eq!((c.builds[&41][0].id.as_str(), c.builds[&42][0].id.as_str()), ("Old", "New"), "each build keeps its own cells");
        assert!(!stale(Some(c), 100, 42) && !stale(Some(c), 100, 41));
        assert!(stale(Some(c), 101, 42), "the item was updated on the Workshop");
        assert!(stale(Some(&all["222"]), 5, 42), "only read for B41 so far");
        assert!(stale(None, 5, 42));
    }

    #[test]
    fn compares_with_a_server() {
        let mut fort = map("Fort", &[(1, 1)]);
        fort.source = "workshop:333".into();
        let mut fort2 = map("FortReupload", &[(1, 1)]);
        (fort2.source, fort2.maps[0].folder) = ("workshop:444".into(), "Fort".into());
        let server = super::super::server::ServerMods {
            mods: vec!["Town".into()],
            workshop: vec!["444".into()],
            maps: ["Fort", "Town", "Unknown", "Muldraugh, KY"].map(String::from).to_vec(),
        };
        let c = compare(&server, &["Town".into(), "Mine".into()], &["Muldraugh, KY".into()], &[fort, fort2]);
        assert_eq!(
            c.only_server,
            [ServerMap { folder: "Fort".into(), id: Some("FortReupload".into()), item: Some("444".into()) }, ServerMap { folder: "Unknown".into(), id: None, item: None }],
            "the server's own Workshop item wins"
        );
        assert_eq!(c.only_you, ["Mine"]);
    }

    #[test]
    fn river_cells_from_the_world_map() {
        // world map cell 1,0 (300 tiles): its lower half is river; a building covers the rest
        let xml = r#"<world>
 <cell x="1" y="0">
  <feature>
   <geometry type="Polygon">
    <coordinates>
     <point x="0" y="140"/>
     <point x="300" y="140"/>
     <point x="300" y="300"/>
     <point x="0" y="300"/>
    </coordinates>
   </geometry>
   <properties>
    <property name="water" value="river"/>
   </properties>
  </feature>
  <feature>
    <coordinates>
     <point x="0" y="0"/>
     <point x="300" y="0"/>
     <point x="300" y="140"/>
    </coordinates>
    <property name="building" value="yes"/>
  </feature>
 </cell>
</world>
"#;
        assert_eq!(water_cells(xml, 300), [(1, 0)], "8 of 16 samples in water");
        // the same tiles (300..600 x 140..300) cover at most 6/16 of any 256-tile cell
        assert_eq!(water_cells(xml, 256), Vec::<Cell>::new());
        let g = grid(&[(1, 0), (2, 0)], &[(1, 0)], &[]);
        assert_eq!(g.iter().map(|c| c.kind).collect::<Vec<_>>(), ["water", "vanilla"]);
    }

    #[test]
    fn cache_follows_the_folder_time() {
        let (t, paths) = store::tests::temp_paths();
        let dir = t.path().join("maps/Town");
        touch(&dir.join("world_1_1.lotpack"));
        let mut c = Cache::load(&paths);
        assert_eq!(c.folder(&dir).cells, [(1, 1)]);
        c.save(&paths).unwrap();
        // the saved entry is what's read while the folder is unchanged (no walk)
        let mut stored: BTreeMap<String, Entry> = serde_json::from_slice(&fs::read(cache_file(&paths)).unwrap()).unwrap();
        stored.values_mut().for_each(|e| e.map.cells = vec![(7, 7)]);
        fs::write(cache_file(&paths), serde_json::to_vec(&stored).unwrap()).unwrap();
        assert_eq!(Cache::load(&paths).folder(&dir).cells, [(7, 7)]);
        // a new cell file changes the folder's time
        std::thread::sleep(std::time::Duration::from_millis(20));
        touch(&dir.join("world_1_2.lotpack"));
        let mut c = Cache::load(&paths);
        assert_eq!(c.folder(&dir).cells, [(1, 1), (1, 2)]);
        c.save(&paths).unwrap();
        // folders not seen again are dropped from the file
        Cache::load(&paths).save(&paths).unwrap();
        assert_eq!(fs::read_to_string(cache_file(&paths)).unwrap(), "{}");
    }

    #[test]
    fn export_text() {
        let row = |id: &str, source: &str, requires: &[&str]| store::Row {
            id: id.into(),
            name: id.into(),
            source: source.into(),
            dir: PathBuf::new(),
            info: PathBuf::new(),
            requires: requires.iter().map(|s| s.to_string()).collect(),
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: false,
            supports_42: true,
        };
        let mods: BTreeMap<String, store::Row> = [row("Town", "steam:111", &["Tiles", "Gone"]), row("Tiles", "workshop:222", &["Base"]), row("Base", "steam:111", &[])]
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();
        let mut town = map("Town", &[(1, 1)]);
        (town.source, town.requires, town.name) = ("steam:111".into(), vec!["Tiles".into(), "Gone".into()], "Town Name".into());
        let mut checked = map("Fort", &[(2, 2)]);
        checked.source = "workshop:333".into();
        let text = export(&[&town, &checked], &mods, &["Town".into(), "Fort".into(), "Muldraugh, KY".into()], false);
        assert_eq!(
            text,
            "Town Name\n  https://steamcommunity.com/sharedfiles/filedetails/?id=111\n  Mod ID: Town · Map folder: Town\n\
             Fort\n  https://steamcommunity.com/sharedfiles/filedetails/?id=333\n  Mod ID: Fort · Map folder: Fort\n\
             \nRequired mods\n  Base: https://steamcommunity.com/sharedfiles/filedetails/?id=111\n  Tiles: https://steamcommunity.com/sharedfiles/filedetails/?id=222\n\
             \nRequired but not installed: Gone\n\
             \nMods=\\Base;\\Tiles;\\Town;\\Fort\nWorkshopItems=111;222;333\nMap=Town;Fort;Muldraugh, KY\n"
        );
        assert!(export(&[&checked], &mods, &[], true).ends_with("\nMods=Fort\nWorkshopItems=333\n"));
    }
}
