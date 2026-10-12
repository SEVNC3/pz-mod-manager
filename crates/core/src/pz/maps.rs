//! Map mods: which world cells each map fills, read from the map files themselves.
//!
//! A map folder (`media/maps/<Name>/`) holds one `world_X_Y.lotpack` and one `X_Y.lotheader`
//! per cell. B41 and B42 both ship the two, but their cells differ in size (300 tiles in B41,
//! 256 in B42), so cells are only comparable within one build: B41 reads a mod's root
//! `media/maps`, B42 its version folders (`42.x/`, `common/`). Cell lists are cached in
//! `<data>/maps-cache.json`, keyed by the folder's (and its map.info's) modified time.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{Paths, is_version_dir};
use crate::sources::steam_manifest::Session;
use crate::read_json;
use crate::sources::workshop_item;
use crate::sources::steam_workshop::{self, Item};
use crate::store;

pub type Cell = (i32, i32);

/// Why a Maps action failed: a stable key the UI says in the player's language
/// (`maps.err.<key>`); Display is the English, for logs and the CLI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fail {
    ScanRunning,
    SteamStopped,
    Timeout,
    Stopped,
    NotAnId,
    NoItem,
    Installed,
    NoFiles,
    FileList,
    Login,
    DownloadFailed,
}

impl Fail {
    pub fn key(self) -> &'static str {
        match self {
            Fail::ScanRunning => "scanRunning",
            Fail::SteamStopped => "steamStopped",
            Fail::Timeout => "timeout",
            Fail::Stopped => "stopped",
            Fail::NotAnId => "notAnId",
            Fail::NoItem => "noItem",
            Fail::Installed => "installed",
            Fail::NoFiles => "noFiles",
            Fail::FileList => "fileList",
            Fail::Login => "login",
            Fail::DownloadFailed => "downloadFailed",
        }
    }
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Fail::ScanRunning => "a Workshop map scan is already running",
            Fail::SteamStopped => "Steam stopped answering; what was read is kept, scan again later",
            Fail::Timeout => "Steam didn't answer in time",
            Fail::Stopped => "stopped",
            Fail::NotAnId => "that isn't a Workshop item id",
            Fail::NoItem => "there's no such Workshop item",
            Fail::Installed => "that Workshop item is already installed: if it adds maps, they are in the list",
            Fail::NoFiles => "Steam lists no files for it",
            Fail::FileList => "Steam didn't give the item's file list",
            Fail::Login => "couldn't log in to Steam anonymously",
            Fail::DownloadFailed => "the download failed",
        })
    }
}

impl std::error::Error for Fail {}

/// An error for the UI: `maps.err.<key>`, then what went wrong under it (English) after ": ",
/// when it's a Maps failure; else its text.
pub fn ui_error(e: &anyhow::Error) -> String {
    let Some(fail) = e.downcast_ref::<Fail>() else { return format!("{e:#}") };
    let under: Vec<String> = e.chain().map(|c| c.to_string()).skip_while(|c| *c != fail.to_string()).skip(1).collect();
    format!("maps.err.{}{}", fail.key(), if under.is_empty() { String::new() } else { format!(": {}", under.join(": ")) })
}

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

/// Maps that clash, as (i, j, folder): both add the same map folder (`Some`), or they cover some of
/// the same cells (`None`) and neither is built on the other (an add-on overlaps the map it requires on purpose).
pub fn clashes(on: &[&MapMod]) -> Vec<(usize, usize, Option<String>)> {
    let cells: Vec<HashSet<&Cell>> = on.iter().map(|m| m.cells().collect()).collect();
    let mut out = vec![];
    for (i, a) in on.iter().enumerate() {
        for (j, b) in on.iter().enumerate().skip(i + 1) {
            if let Some(f) = a.maps.iter().find(|f| b.maps.iter().any(|g| g.folder == f.folder)) {
                out.push((i, j, Some(f.folder.clone())));
            } else if !a.requires.contains(&b.id) && !b.requires.contains(&a.id) && !cells[i].is_disjoint(&cells[j]) {
                out.push((i, j, None));
            }
        }
    }
    out
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
    let text = |p: PathBuf| fs::read(p).map(|b| super::decode_text(&b)).unwrap_or_default();
    let root = dir.parent().and_then(Path::parent).and_then(Path::parent).unwrap_or(dir);
    MapFolder { cells, ..parse_map_info(&folder, &text(dir.join("map.info")), |rel| text(root.join(rel))) }
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
                let cells = fs::read(&file).map(|b| water_cells(&WorldMap::from_xml(&super::decode_text(&b)), if build == 41 { 300 } else { 256 })).unwrap_or_default();
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

/// A mod's map folder names for `build` (see `pick_roots`), without reading their cells.
pub fn folder_names(mod_dir: &Path, build: u8) -> Vec<String> {
    let mut out: Vec<String> = map_roots(mod_dir, build)
        .iter()
        .flat_map(|r| fs::read_dir(r.join("media/maps")).into_iter().flatten().flatten())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out.dedup();
    out
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

/// The game's map folders (B42's `media/maps`; B41 has fewer): a `lots=` naming one is met
/// even when the install isn't found.
const VANILLA_FOLDERS: [&str; 12] = [
    "Brandenburg, KY",
    "challengemaps",
    "Echo Creek, KY",
    "Ekron, KY",
    "Fallas Lake, KY",
    "Irvington, KY",
    "March Ridge, KY",
    "Muldraugh, KY",
    "Riverside, KY",
    "Rosewood, KY",
    "Valley Station, KY",
    "West Point, KY",
];

/// One of the game's own map folders (see `VANILLA_FOLDERS`).
pub fn is_vanilla(folder: &str) -> bool {
    VANILLA_FOLDERS.contains(&folder)
}

/// The game's own maps (`<game>/media/maps/*`, and the known vanilla folders): their folder
/// names, and the cells when the install is `build` (B42's lotheaders start with `LOTH`,
/// B41's don't), else None.
pub fn vanilla(game: Option<&Path>, build: u8, cache: &mut Cache) -> (Vec<String>, Option<Vec<Cell>>) {
    let (mut folders, mut cells, mut game_build): (Vec<String>, _, _) = (VANILLA_FOLDERS.map(String::from).to_vec(), HashSet::new(), None);
    for e in game.and_then(|g| fs::read_dir(g.join("media/maps")).ok()).into_iter().flatten().flatten() {
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
    folders.dedup();
    let mut cells: Vec<Cell> = cells.into_iter().collect();
    cells.sort();
    (folders, (game_build == Some(build)).then_some(cells))
}

/// What the in-game map draws, in world tiles (the same in B41 and B42): polygons per layer,
/// each a list of rings (`[x0, y0, x1, y1, ...]`, the outline then any holes), and roads drawn
/// as lines.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct WorldMap {
    pub water: Vec<Vec<Vec<i32>>>,
    pub roads: Vec<Vec<Vec<i32>>>,
    pub buildings: Vec<Vec<Vec<i32>>>,
    pub road_lines: Vec<Vec<i32>>,
}

impl WorldMap {
    /// File one feature by its property names; what the grid doesn't draw (forest, labels) is dropped.
    fn add(&mut self, line: bool, rings: Vec<Vec<i32>>, keys: &[&str]) {
        let has = |k: &str| keys.contains(&k);
        if rings.is_empty() {
        } else if line {
            if has("highway") {
                self.road_lines.extend(rings);
            }
        } else if has("water") {
            self.water.push(rings);
        } else if has("building") {
            self.buildings.push(rings);
        } else if has("highway") || has("railway") {
            self.roads.push(rings);
        }
    }

    fn extend(&mut self, m: WorldMap) {
        self.water.extend(m.water);
        self.roads.extend(m.roads);
        self.buildings.extend(m.buildings);
        self.road_lines.extend(m.road_lines);
    }

    /// worldmap.xml: 300-tile cells, one element per line as WorldEd writes it.
    pub fn from_xml(xml: &str) -> WorldMap {
        let attr = |l: &str, k: &str| -> i32 { l.split_once(&format!(" {k}=\"")).and_then(|(_, v)| v.split('"').next()?.parse().ok()).unwrap_or(0) };
        let mut out = WorldMap::default();
        let (mut origin, mut ring, mut rings, mut keys, mut kind) = ((0, 0), vec![], vec![], vec![], "");
        for l in xml.lines().map(str::trim) {
            if l.starts_with("<cell ") {
                origin = (attr(l, "x") * 300, attr(l, "y") * 300);
            } else if l.starts_with("<feature") {
                (rings, keys, kind) = (vec![], vec![], "");
            } else if let Some(t) = l.strip_prefix("<geometry type=\"") {
                kind = t.split('"').next().unwrap_or_default();
            } else if l.starts_with("<point ") {
                ring.extend([origin.0 + attr(l, "x"), origin.1 + attr(l, "y")]);
            } else if l.starts_with("</coordinates") {
                rings.push(std::mem::take(&mut ring));
            } else if let Some(k) = l.strip_prefix("<property name=\"") {
                keys.push(k.split('"').next().unwrap_or_default());
            } else if l.starts_with("</feature") && kind != "Point" {
                out.add(kind == "LineString", std::mem::take(&mut rings), &keys);
            }
        }
        out
    }

    /// worldmap.xml.bin version 2 (the file B42 loads): "IGMB", version, cell size, width and
    /// height in cells, a string table, then per cell either -1 or its x, y and features (geometry
    /// type as a string index, u8 ring count, per ring a u16 point count and i16 x,y pairs relative
    /// to the cell, u8 property count and name,value string indexes). None when it isn't one.
    pub fn from_bin(b: &[u8]) -> Option<WorldMap> {
        let mut r = Reader { b, at: 0 };
        if r.bytes(4)? != b"IGMB" {
            return None;
        }
        let (version, size, w, h) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        if version != 2 || size <= 0 {
            return None;
        }
        let strings: Vec<&str> = (0..r.i32()?).map(|_| r.u16().and_then(|n| r.bytes(n as usize)).map(|s| std::str::from_utf8(s).unwrap_or_default())).collect::<Option<_>>()?;
        let (mut out, mut seen) = (WorldMap::default(), HashSet::new());
        for _ in 0..w.checked_mul(h)? {
            let cx = r.i32()?;
            if cx == -1 {
                continue;
            }
            let (cy, features) = (r.i32()?, r.i32()?);
            for _ in 0..features {
                let kind = *strings.get(r.u16()? as usize)?;
                let rings: Vec<Vec<i32>> = (0..r.u8()?)
                    .map(|_| {
                        let n = r.u16()? as usize;
                        let pts = r.bytes(n * 4)?;
                        Some(pts.chunks(2).enumerate().map(|(i, c)| i16::from_le_bytes([c[0], c[1]]) as i32 + if i % 2 == 0 { cx } else { cy } * size).collect())
                    })
                    .collect::<Option<_>>()?;
                let keys: Vec<&str> = (0..r.u8()?).map(|_| r.u16().zip(r.u16()).and_then(|(k, _)| strings.get(k as usize).copied())).collect::<Option<_>>()?;
                // a feature crossing cells is stored once per cell
                if kind != "Point" && seen.insert(rings.clone()) {
                    out.add(kind == "LineString", rings, &keys);
                }
            }
        }
        Some(out)
    }
}

/// Little-endian reads over a byte slice; None past the end.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes(2)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
}

/// One map folder's world map: worldmap.xml.bin when it's the version B42 reads, else
/// worldmap.xml. None when the folder has neither (the grid stays plain there).
pub fn world_map(dir: &Path) -> Option<WorldMap> {
    fs::read(dir.join("worldmap.xml.bin"))
        .ok()
        .and_then(|b| WorldMap::from_bin(&b))
        .or_else(|| fs::read(dir.join("worldmap.xml")).ok().map(|b| WorldMap::from_xml(&super::decode_text(&b))))
}

/// The world maps of every map folder under `roots` (the game folder, or a mod's version folders), together.
pub fn world_maps(roots: &[PathBuf]) -> WorldMap {
    let mut out = WorldMap::default();
    for e in roots.iter().flat_map(|r| fs::read_dir(r.join("media/maps")).into_iter().flatten().flatten()) {
        out.extend(world_map(&e.path()).unwrap_or_default());
    }
    out
}

/// A mod's world maps for `build` (see `pick_roots`).
pub fn mod_world(mod_dir: &Path, build: u8) -> WorldMap {
    world_maps(&map_roots(mod_dir, build))
}

/// Cells (`cell_size` tiles: 256 in B42, 300 in B41) that are mostly water on the world map:
/// a cell counts when at least half of 16 sample points fall in a water polygon.
pub fn water_cells(map: &WorldMap, cell_size: i32) -> Vec<Cell> {
    let mut hits: BTreeMap<Cell, u32> = BTreeMap::new();
    for rings in &map.water {
        let rings: Vec<Vec<(f64, f64)>> = rings.iter().map(|r| r.chunks(2).map(|p| (p[0] as f64, p[1] as f64)).collect()).collect();
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
/// ones (the enabled maps, which stay shown whether or not `all` lists them). Ties go to the
/// higher `rank` (installed, popular). Covering vanilla isn't a conflict; maps without cells
/// (spawn points only) are left out. Ids come back in `all`'s order.
pub fn quick_select(all: &[MapMod], fixed: &[MapMod], rank: impl Fn(&MapMod) -> u64) -> Vec<String> {
    let fixed_ids: HashSet<&str> = fixed.iter().map(|m| m.id.as_str()).collect();
    let taken: HashSet<&Cell> = fixed.iter().flat_map(|m| m.cells()).collect();
    let cands: Vec<&MapMod> =
        all.iter().filter(|m| !fixed_ids.contains(m.id.as_str()) && m.cells().next().is_some() && !m.cells().any(|c| taken.contains(c))).collect();
    // the conflict graph: maps sharing a cell
    let mut at: HashMap<&Cell, Vec<usize>> = HashMap::new();
    for (i, m) in cands.iter().enumerate() {
        for c in m.cells() {
            let v = at.entry(c).or_default();
            if v.last() != Some(&i) {
                v.push(i);
            }
        }
    }
    let mut adj: Vec<Vec<usize>> = vec![vec![]; cands.len()];
    for ids in at.values() {
        for &a in ids {
            adj[a].extend(ids.iter().filter(|&&b| b != a));
        }
    }
    for a in &mut adj {
        a.sort_unstable();
        a.dedup();
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
    let mut alive = vec![false; adj.len()];
    comp.iter().for_each(|&v| alive[v] = true);
    // rivals still in play, kept up to date as maps are taken or ruled out
    let mut deg: Vec<usize> = (0..adj.len()).map(|v| adj[v].iter().filter(|&&n| alive[n]).count()).collect();
    let mut out = vec![];
    while let Some(v) = comp.iter().copied().filter(|&v| alive[v]).min_by_key(|&v| (deg[v], std::cmp::Reverse(weight[v]))) {
        out.push(v);
        for gone in std::iter::once(v).chain(adj[v].iter().copied()) {
            if std::mem::take(&mut alive[gone]) {
                adj[gone].iter().for_each(|&n| deg[n] = deg[n].saturating_sub(1));
            }
        }
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
    /// When it was first published.
    pub created: u64,
    pub tags: Vec<String>,
    /// All its files, in bytes.
    pub size: u64,
    /// Checked by hand ("Check a Workshop map") rather than by the Workshop scan.
    pub manual: bool,
    /// Why its file list couldn't be read: it's read again once the item is updated.
    pub failed: String,
    /// Its map mods per build: B41 and B42 read different folders.
    pub builds: BTreeMap<u8, Vec<MapMod>>,
    /// How many mods (maps or not) it holds per build: a mod pack holds several.
    pub mods: BTreeMap<u8, usize>,
}

impl Checked {
    fn from_item(item: &Item) -> Checked {
        Checked {
            title: item.title.clone(),
            author: item.author.clone(),
            subscriptions: item.subscriptions,
            updated: item.time_updated,
            created: item.time_created,
            tags: item.tags.clone(),
            ..Default::default()
        }
    }
}

/// Whether an item updated at `updated` has to be read (again): new, updated since, or not
/// read for both builds yet. One that couldn't be read waits for an update.
pub fn stale(old: Option<&Checked>, updated: u64) -> bool {
    old.is_none_or(|c| c.updated != updated || (c.failed.is_empty() && !(c.builds.contains_key(&41) && c.builds.contains_key(&42))))
}

fn checked_file(paths: &Paths) -> PathBuf {
    paths.data.join("maps-checked.json")
}

/// The Workshop map index shipped with PZMM (`pzmm map-scan --seed`): every Map-tagged item
/// as of the release, so a fresh install lists them at once and its first scan only reads
/// the items updated since.
const SEED: &[u8] = include_bytes!("../../data/map-seed.json.gz");

fn parse_seed(gz: &[u8]) -> Result<BTreeMap<String, Checked>> {
    Ok(serde_json::from_reader(flate2::read::GzDecoder::new(gz))?)
}

/// The seed, parsed once. One that doesn't parse is logged (once) and counts as empty.
fn seed() -> &'static BTreeMap<String, Checked> {
    static PARSED: std::sync::OnceLock<BTreeMap<String, Checked>> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        // tests start from nothing, like a build without a seed
        if cfg!(test) {
            return BTreeMap::new();
        }
        parse_seed(SEED).unwrap_or_else(|e| {
            crate::say!("mapIndexBroken", "error: PZMM's built-in map index doesn't parse ({error}); the Workshop scan reads every map instead", error = format!("{e:#}"));
            BTreeMap::new()
        })
    })
}

/// Seed entries `all` lacks, or has older (`updated`) and not hand-checked, go in: a newer
/// PZMM brings its newer index to an existing install.
fn merge_seed(all: &mut BTreeMap<String, Checked>, seed: &BTreeMap<String, Checked>) {
    for (id, s) in seed {
        if all.get(id).is_none_or(|c| !c.manual && c.updated < s.updated) {
            all.insert(id.clone(), s.clone());
        }
    }
}

/// Workshop items read without installing, by item id, with the seed merged in (see
/// `merge_seed`; a damaged file starts over from the seed, see `read_json`).
fn read_checked(paths: &Paths) -> Result<BTreeMap<String, Checked>> {
    let mut all = read_json(&checked_file(paths))?;
    merge_seed(&mut all, seed());
    Ok(all)
}

/// Workshop items read without installing, by item id (none while the file can't be read).
pub fn checked(paths: &Paths) -> BTreeMap<String, Checked> {
    read_checked(paths).unwrap_or_else(|e| {
        crate::log!("error: {e:#}");
        BTreeMap::new()
    })
}

/// Add or replace entries in maps-checked.json (the scan and "Check" can both write, in the
/// app and the CLI at once): under the data lock, re-read and merged per item, the newer
/// `updated` winning.
fn save_checked(paths: &Paths, new: BTreeMap<String, Checked>) -> Result<()> {
    if new.is_empty() {
        return Ok(());
    }
    let _lock = crate::lock_data(paths)?;
    let mut all = read_checked(paths).context("nothing was saved")?;
    for (id, mut c) in new {
        match all.get(&id) {
            // another scan saved a newer read since this one took its snapshot
            Some(o) if o.updated > c.updated => continue,
            // checked by hand since the scan took its snapshot: it stays hand-checked
            Some(o) => c.manual |= o.manual,
            None => {}
        }
        all.insert(id, c);
    }
    Ok(crate::write_atomic(&checked_file(paths), serde_json::to_vec(&all)?)?)
}

/// Write the scan's results (not hand-checked items) to `out` as a gzipped seed index (see
/// `SEED`). Returns how many items it holds.
pub fn write_seed(paths: &Paths, out: &Path) -> Result<usize> {
    let all: BTreeMap<String, Checked> = read_checked(paths)?.into_iter().filter(|(_, c)| !c.manual).collect();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    serde_json::to_writer(&mut gz, &all)?;
    crate::write_atomic(out, gz.finish()?)?;
    Ok(all.len())
}

/// The mods in an item's file list (paths with `/`) on `build`: each mod's folder, the
/// mod.info the game reads there (the root one in B41, the newest version folder's in B42)
/// and its version folders.
pub(crate) fn listing_mods(files: &[String], build: u8) -> Vec<(String, String, Vec<String>)> {
    let lower: Vec<String> = files.iter().map(|f| f.to_ascii_lowercase()).collect();
    let find = |p: &str| lower.iter().position(|l| *l == p.to_ascii_lowercase()).map(|i| files[i].clone());
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
        let mut versions: Vec<String> =
            (0..files.len()).filter(|i| lower[*i].starts_with(&prefix)).filter_map(|i| files[i][prefix.len()..].split_once('/').map(|v| v.0.to_string())).filter(|v| is_version_dir(v)).collect();
        versions.sort();
        versions.dedup();
        let info = if build == 41 {
            find(&join(&dir, "mod.info"))
        } else {
            let mut vs: Vec<&String> = versions.iter().filter(|v| find(&join(&dir, &join(v, "mod.info"))).is_some()).collect();
            vs.sort_by_key(|v| v.split('.').filter_map(|p| p.parse().ok()).collect::<Vec<u32>>());
            vs.last().and_then(|v| find(&join(&dir, &join(v, "mod.info"))))
        };
        if let Some(info) = info {
            out.push((dir, info, versions));
        }
    }
    out
}

fn split(p: &str) -> (String, String) {
    let (a, b) = p.rsplit_once('/').unwrap_or(("", p));
    (a.to_string(), b.to_string())
}

fn join(a: &str, b: &str) -> String {
    if a.is_empty() || b.is_empty() { format!("{a}{b}") } else { format!("{a}/{b}") }
}

/// The map mods in an item's file list (paths with `/`) for `build`, by the rules `mod_maps`
/// reads folders with, and how many mods it holds on `build`. `read` gives a listed file's
/// text: mod.info (id, name, require=), map.info (title, lots=) and B42 title files; nothing
/// else is fetched.
pub fn from_listing(item: &str, files: &[String], build: u8, read: &mut dyn FnMut(&str) -> Result<String>) -> Result<(Vec<MapMod>, usize)> {
    let lower: Vec<String> = files.iter().map(|f| f.to_ascii_lowercase()).collect();
    let find = |p: &str| lower.iter().position(|l| *l == p.to_ascii_lowercase()).map(|i| files[i].clone());
    let (mut out, mut count) = (vec![], 0);
    for (dir, info, versions) in listing_mods(files, build) {
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/").to_ascii_lowercase() };
        let under: Vec<usize> = (0..files.len()).filter(|i| lower[*i].starts_with(&prefix)).collect();
        count += 1;
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
    Ok((out, count))
}

/// Read a Workshop item's maps (both builds) from its manifest on `session`; `stop` ends it.
fn read_listing(session: &Session, item: &Item, stop: &AtomicBool) -> Result<Checked> {
    if item.hcontent == 0 {
        return Err(anyhow::Error::new(Fail::NoFiles).context(format!("workshop:{}", item.id)));
    }
    let files = session.files(item.hcontent, stop)?;
    let names: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    let mut read = |p: &str| -> Result<String> {
        if let Some(t) = texts.get(p) {
            return Ok(t.clone());
        }
        let f = files.iter().find(|f| f.path == p).context("not in the file list")?;
        let t = super::decode_text(&session.read(f, stop).with_context(|| format!("reading {p}"))?);
        texts.insert(p.to_string(), t.clone());
        Ok(t)
    };
    let mut c = Checked { size: files.iter().map(|f| f.size).sum(), ..Checked::from_item(item) };
    for build in [41, 42] {
        let (mods, count) = from_listing(&item.id, &names, build, &mut read)?;
        c.builds.insert(build, mods);
        c.mods.insert(build, count);
    }
    Ok(c)
}

/// The map mods in a downloaded item folder, and how many mods it holds, per build (not
/// cached: the folder is about to go).
pub fn read_item(dir: &Path, item: &str) -> (BTreeMap<u8, Vec<MapMod>>, BTreeMap<u8, usize>) {
    let mut cache = Cache::default();
    let found = super::find_mods(dir, 8);
    let (mut builds, mut counts) = (BTreeMap::new(), BTreeMap::new());
    for build in [41, 42] {
        let mods = found
            .iter()
            .filter_map(|m| {
                let maps = mod_maps(&m.dir, build, &mut cache);
                (!maps.is_empty()).then(|| MapMod { id: m.id.clone(), name: m.name.clone(), source: format!("workshop:{item}"), requires: m.requires.clone(), maps })
            })
            .collect();
        builds.insert(build, mods);
        counts.insert(build, found.iter().filter(|m| if build == 41 { m.supports_41 } else { m.supports_42 }).count());
    }
    (builds, counts)
}

/// Read Workshop `item`'s maps from its file list (keeping them in `maps-checked.json`).
/// Nothing is installed. When Steam won't give the list and `download` is set, SteamCMD
/// downloads the item instead, and the download is deleted after reading it. Setting `stop`
/// ends it (`Fail::Stopped`).
pub fn check(paths: &Paths, item: &str, download: bool, stop: &AtomicBool) -> Result<Checked> {
    let info = steam_workshop::details(&[item])?.pop().ok_or(Fail::NoItem).with_context(|| format!("workshop:{item}"))?;
    let mut c = match Session::open(stop).and_then(|s| read_listing(&s, &info, stop)) {
        Ok(c) => c,
        Err(_) if stop.load(Relaxed) => return Err(Fail::Stopped.into()),
        Err(e) if !download => return Err(e.context(Fail::FileList)),
        Err(e) => {
            crate::say!("noFileList", "workshop:{item}: no file list from Steam ({error}), downloading it with SteamCMD instead", item = item, error = format!("{e:#}"));
            // the download goes on every way out: read, failed, timed out or stopped
            let _gone = RemoveOnDrop(steam_workshop::steamcmd_dirs(paths, super::STEAM_APP, item));
            let got = steam_workshop::download(paths, super::STEAM_APP, &[item], stop)?;
            if stop.load(Relaxed) {
                return Err(Fail::Stopped.into());
            }
            let dir = got.into_iter().next().and_then(|(_, d)| d).ok_or(Fail::DownloadFailed).with_context(|| format!("workshop:{item}"))?;
            let (builds, mods) = read_item(&dir, item);
            Checked { size: info.file_size, builds, mods, ..Checked::from_item(&info) }
        }
    };
    c.manual = true;
    save_checked(paths, [(item.to_string(), c.clone())].into())?;
    Ok(c)
}

/// Deletes these folders when dropped.
struct RemoveOnDrop([PathBuf; 2]);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        for d in self.0.iter().filter(|d| d.exists()) {
            if let Err(e) = fs::remove_dir_all(d) {
                crate::say!("downloadNotDeleted", "error: deleting the download in {dir}: {error}", dir = d.display(), error = e);
            }
        }
    }
}

/// The Workshop maps read without installing that aren't installed, one per mod id (when
/// several items provide one: the hand-checked one, else the most subscribed), and the mod
/// ids several items provide (installed ones included), with those items.
pub fn workshop_maps(checked: &BTreeMap<String, Checked>, build: u8, installed: &BTreeMap<String, store::Row>) -> (Vec<MapMod>, BTreeMap<String, Vec<String>>) {
    let mut ranked: Vec<(&String, &Checked)> = checked.iter().collect();
    ranked.sort_by_key(|(_, c)| (!c.manual, std::cmp::Reverse(c.subscriptions)));
    let (mut out, mut seen, mut items) = (vec![], HashSet::new(), BTreeMap::<String, Vec<String>>::new());
    for (item, c) in ranked {
        for m in c.builds.get(&build).into_iter().flatten() {
            items.entry(m.id.clone()).or_default().push(item.clone());
            if !installed.contains_key(&m.id) && seen.insert(m.id.as_str()) {
                out.push(m.clone());
            }
        }
    }
    for (id, list) in &mut items {
        if let Some(it) = installed.get(id).and_then(|r| workshop_item(&r.source))
            && !list.iter().any(|x| x == it)
        {
            list.push(it.to_string());
        }
    }
    items.retain(|_, list| list.len() > 1);
    (out, items)
}

/// Where a Workshop scan is.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Progress {
    /// "listing" (Map-tagged items, page by page), "reading" (their file lists) or "done".
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
    /// Items whose file list couldn't be read this time.
    pub failed: usize,
}

/// File lists read at once, on the one Steam session.
pub(crate) const READERS: usize = 4;

/// Sleep `secs`, waking early once `done` says so.
pub(crate) fn pause(secs: u64, done: impl Fn() -> bool) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while !done() && std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// `f` on its own thread, given up on (None) once `stop` is set: a web call (and its 429
/// back-off) can't be interrupted, but it times out, so the thread ends on its own. One that
/// hasn't answered in two minutes counts as timed out (a stalled HTTPS read was seen to hang for good).
pub(crate) fn unless_stopped<T: Send + 'static>(stop: &AtomicBool, f: impl FnOnce() -> Result<T> + Send + 'static) -> Option<Result<T>> {
    use std::sync::mpsc::RecvTimeoutError;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(f()));
    let end = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(v) => return Some(v),
            Err(RecvTimeoutError::Timeout) if std::time::Instant::now() > end => return Some(Err(Fail::Timeout.into())),
            Err(RecvTimeoutError::Timeout) if !stop.load(Relaxed) => {}
            Err(RecvTimeoutError::Timeout) => return None,
            Err(RecvTimeoutError::Disconnected) => return Some(Err(anyhow::anyhow!("the request failed"))),
        }
    }
}

/// One Workshop browse page, tried again after 20 and 40 s (Steam's request limit resets within
/// a minute); None once `stop` is set.
pub(crate) fn browse_patiently(q: &steam_workshop::Query, stop: &AtomicBool) -> Option<Result<steam_workshop::Page>> {
    let stopped = || stop.load(Relaxed);
    let browse = || {
        let q = q.clone();
        unless_stopped(stop, move || steam_workshop::browse(super::STEAM_APP, &q))
    };
    let mut got = browse();
    for wait in [20, 40] {
        if !matches!(got, Some(Err(_))) || stopped() {
            break;
        }
        pause(wait, stopped);
        got = browse();
    }
    got.filter(|_| !stopped())
}

/// Whether a failed read says Steam (or the connection) is struggling, not that the item can't
/// be read: only these count towards giving up, and they're read again next scan.
pub(crate) fn network_error(e: &anyhow::Error) -> bool {
    use steamroom::enums::EResultError as R;
    use steamroom::error::{ConnectionError as C, Error as E};
    e.chain().any(|c| match c.downcast_ref::<E>() {
        Some(E::Http(_) | E::Io(_)) => true,
        Some(E::CdnStatus { status, .. }) => status.as_u16() == 429 || status.is_server_error(),
        Some(E::Connection(C::ServiceMethodFailed(r))) => matches!(r, R::NoConnection | R::Busy | R::Timeout | R::ServiceUnavailable | R::RateLimitExceeded),
        Some(E::Connection(c)) => !matches!(c, C::DepotAccessDenied(_)),
        _ => c.is::<std::io::Error>() || c.downcast_ref::<Fail>() == Some(&Fail::Timeout),
    })
}

/// What to keep for an item that couldn't be read (`why`): its last good read, still due to be
/// read again (its `updated` is the old one), else just its Workshop facts.
fn unread(old: Option<&Checked>, item: &Item, why: String) -> Checked {
    match old.filter(|c| !c.builds.is_empty()) {
        Some(c) => Checked { failed: why, ..c.clone() },
        None => Checked { failed: why, ..Checked::from_item(item) },
    }
}

/// Every Map-tagged Workshop item (all result pages, both builds), then the file lists of the
/// ones not read yet or updated since, most subscribed first, up to `READERS` at once on one
/// anonymous Steam session; each read serves both builds. Results are saved as it goes;
/// setting `stop` ends it (waits included).
pub fn scan_workshop(paths: &Paths, stop: &AtomicBool, progress: &mut dyn FnMut(&Progress)) -> Result<Progress> {
    let stopped = || stop.load(Relaxed);
    let mut p = Progress { phase: "listing", ..Default::default() };
    let mut items: Vec<Item> = vec![];
    for page in 1.. {
        let q = steam_workshop::Query { required: vec!["Map".into()], sort: "mostrecent".into(), per_page: 50, page, ..Default::default() };
        let Some(got) = browse_patiently(&q, stop) else { return Ok(p) };
        let got = got?;
        let empty = got.items.is_empty();
        for i in got.items {
            // newest first: an upload during the scan shifts the pages, repeating an item
            if !items.iter().any(|o| o.id == i.id) {
                items.push(i);
            }
        }
        (p.done, p.total) = (items.len(), got.total as usize);
        progress(&p);
        if empty || items.len() >= p.total {
            break;
        }
    }
    let old = read_checked(paths)?;
    let (mut todo, mut refreshed, mut failed) = (vec![], BTreeMap::new(), 0);
    for i in &items {
        match old.get(&i.id) {
            Some(c) if !stale(Some(c), i.time_updated) => {
                let fresh = Checked::from_item(i);
                refreshed.insert(i.id.clone(), Checked { title: fresh.title, author: fresh.author, subscriptions: fresh.subscriptions, created: fresh.created, tags: fresh.tags, ..c.clone() });
            }
            // no files to read: wait for an update (not Steam's fault, so not a reason to give up)
            _ if i.hcontent == 0 => {
                refreshed.insert(i.id.clone(), unread(old.get(&i.id), i, Fail::NoFiles.to_string()));
                failed += 1;
            }
            _ => todo.push(i),
        }
    }
    save_checked(paths, refreshed)?;
    todo.sort_by_key(|i| std::cmp::Reverse(i.subscriptions));
    p = Progress { phase: "reading", total: todo.len(), failed, ..Default::default() };
    progress(&p);
    if !todo.is_empty() && !stopped() {
        let session = match Session::open(stop) {
            Err(_) if stopped() => return Ok(p),
            s => s?,
        };
        let (next, streak, quit) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicBool::new(false));
        let done = || stopped() || quit.load(Relaxed);
        let (tx, rx) = std::sync::mpsc::channel::<(&Item, Result<Checked>, bool)>();
        let mut batch = BTreeMap::new();
        std::thread::scope(|s| -> Result<()> {
            for _ in 0..READERS.min(todo.len()) {
                let (tx, session, next, streak, todo, done) = (tx.clone(), &session, &next, &streak, &todo, &done);
                s.spawn(move || {
                    while !done() {
                        let Some(&i) = todo.get(next.fetch_add(1, Relaxed)) else { break };
                        let res = read_listing(session, i, stop);
                        if res.is_err() && done() {
                            break; // cut short, not a failure
                        }
                        let net = res.as_ref().err().is_some_and(network_error);
                        let n = if net { streak.fetch_add(1, Relaxed) + 1 } else { 0 };
                        if res.is_ok() {
                            streak.store(0, Relaxed);
                        }
                        if tx.send((i, res, net)).is_err() {
                            break;
                        }
                        if net {
                            pause(2 * n as u64, done); // back off while Steam struggles
                        }
                    }
                });
            }
            drop(tx);
            let res = (|| -> Result<()> {
                for (i, res, net) in &rx {
                    p.done += 1;
                    match res {
                        Ok(c) => {
                            batch.insert(i.id.clone(), c); // keeps `manual` (see save_checked)
                        }
                        Err(e) => {
                            crate::log!("workshop:{} ({}): {e:#}", i.id, i.title);
                            p.failed += 1;
                            if !net {
                                batch.insert(i.id.clone(), unread(old.get(&i.id), i, format!("{e:#}")));
                            } else if streak.load(Relaxed) >= 5 {
                                save_checked(paths, std::mem::take(&mut batch))?;
                                return Err(e.context(Fail::SteamStopped));
                            }
                        }
                    }
                    if batch.len() >= 20 {
                        save_checked(paths, std::mem::take(&mut batch))?;
                    }
                    progress(&p);
                }
                Ok(())
            })();
            quit.store(true, Relaxed);
            res
        })?;
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
    /// The server's maps (folders) whose map mods cover some of the same cells: the one first in its `Map=` wins there.
    pub overlap: Vec<[String; 2]>,
}

/// Compare a server config's `Map=` with `yours` (map folders), leaving out the game's own
/// maps. A map's provider is the map mod in `all` (installed first) that adds it, preferring the
/// server's own mods and Workshop items; the providers that clash on cells are its overlaps.
pub fn compare(server: &super::server::ServerMods, yours: &[String], vanilla: &[String], all: &[MapMod]) -> Compare {
    let ours = |f: &String| !vanilla.contains(f) && !super::server::is_base_map(f);
    let theirs: Vec<(&String, Option<&MapMod>)> = server
        .maps
        .iter()
        .filter(|f| ours(f))
        .map(|f| {
            let best = all.iter().enumerate().filter(|(_, m)| m.maps.iter().any(|x| x.folder == *f)).min_by_key(|(i, m)| {
                let item = workshop_item(&m.source);
                (!server.mods.contains(&m.id), !item.is_some_and(|it| server.workshop.iter().any(|w| w == it)), *i)
            });
            (f, best.map(|(_, m)| m))
        })
        .collect();
    let only_server = theirs
        .iter()
        .filter(|(f, _)| !yours.contains(f))
        .map(|(f, m)| ServerMap { folder: (*f).clone(), id: m.map(|m| m.id.clone()), item: m.and_then(|m| workshop_item(&m.source)).map(String::from) })
        .collect();
    let only_you = yours.iter().filter(|f| ours(f) && !server.maps.contains(f)).cloned().collect();
    // each provider once, named by the first of its folders the server loads
    let mut on: Vec<(&String, &MapMod)> = vec![];
    for (f, m) in &theirs {
        if let Some(m) = m
            && !on.iter().any(|(_, x)| x.id == m.id)
        {
            on.push((f, m));
        }
    }
    let mods: Vec<&MapMod> = on.iter().map(|(_, m)| *m).collect();
    let overlap = clashes(&mods).into_iter().filter(|c| c.2.is_none()).map(|(i, j, _)| [on[i].0.clone(), on[j].0.clone()]).collect();
    Compare { only_server, only_you, overlap }
}

// --- presets ---

fn presets_file(paths: &Paths) -> PathBuf {
    paths.data.join("map-presets.json")
}

/// Named sets of maps (mod ids) saved from the Maps tab (none while the file can't be read).
pub fn presets(paths: &Paths) -> BTreeMap<String, Vec<String>> {
    read_json(&presets_file(paths)).unwrap_or_else(|e| {
        crate::log!("error: {e:#}");
        BTreeMap::new()
    })
}

/// Save preset `name` (None deletes it). A file that can't be read is never saved over.
pub fn set_preset(paths: &Paths, name: &str, ids: Option<Vec<String>>) -> Result<()> {
    let mut all: BTreeMap<String, Vec<String>> = read_json(&presets_file(paths)).context("nothing was saved")?;
    match ids {
        Some(ids) => all.insert(name.trim().to_string(), ids),
        None => all.remove(name),
    };
    fs::create_dir_all(&paths.data)?;
    Ok(crate::write_atomic(&presets_file(paths), serde_json::to_vec_pretty(&all)?)?)
}

// --- background image ---

/// A picture of the player's own drawn under the grid instead of the world map: the file (left
/// where it is) and where it lies, image pixel `p` being world tile `p * scale + offset` per axis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Background {
    pub path: String,
    pub scale: [f64; 2],
    pub offset: [f64; 2],
}

fn backgrounds_file(paths: &Paths) -> PathBuf {
    paths.data.join("map-background.json")
}

/// Build `build`'s background (none while the file can't be read).
pub fn background(paths: &Paths, build: u8) -> Option<Background> {
    read_json::<BTreeMap<String, Background>>(&backgrounds_file(paths))
        .unwrap_or_else(|e| {
            crate::log!("error: {e:#}");
            BTreeMap::new()
        })
        .remove(&build.to_string())
}

/// Save build `build`'s background (None forgets it). A file that can't be read is never saved over.
pub fn set_background(paths: &Paths, build: u8, bg: Option<Background>) -> Result<()> {
    let mut all: BTreeMap<String, Background> = read_json(&backgrounds_file(paths)).context("nothing was saved")?;
    match bg {
        Some(bg) => all.insert(build.to_string(), bg),
        None => all.remove(&build.to_string()),
    };
    fs::create_dir_all(&paths.data)?;
    Ok(crate::write_atomic(&backgrounds_file(paths), serde_json::to_vec_pretty(&all)?)?)
}

/// The export's words, in the UI's language: `ids` fills `{id}` and `{folders}`, `missing`
/// fills `{ids}`. The server lines stay as the game reads them.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ExportLabels {
    pub ids: String,
    pub required: String,
    pub missing: String,
}

impl Default for ExportLabels {
    fn default() -> Self {
        ExportLabels {
            ids: "Mod ID: {id} · Map folder: {folders}".into(),
            required: "Required mods".into(),
            missing: "Required but not found (not installed, not on the Workshop): {ids}".into(),
        }
    }
}

/// Looks a mod id up on the Workshop (from the item that needs it): its title and item id.
type FindMod<'a> = dyn FnMut(&str, Option<&str>) -> Option<(String, String)> + 'a;

/// Clipboard text for the shown maps: each map's Workshop link, mod id and folders, the mods
/// they need (names and links), then the server lines. A requirement is looked up among the
/// installed `mods`, the shown maps and the `known` Workshop maps, else `find` (id, the
/// Workshop item that needs it) names its Workshop item and title; `Mods=` puts each mod's
/// requirements (recursively) before it, and `WorkshopItems=` holds every item they come from.
/// `map_line` is the arranged `Map=` list (see `server::map_order`).
pub fn export(
    shown: &[&MapMod],
    mods: &BTreeMap<String, store::Row>,
    known: &[MapMod],
    map_line: &[String],
    b41: bool,
    labels: &ExportLabels,
    find: &mut FindMod,
) -> String {
    /// id -> (name, requires, source)
    type Info = HashMap<String, (String, Vec<String>, String)>;
    // installed wins, then the shown maps
    let mut info: Info = HashMap::new();
    for m in known.iter().chain(shown.iter().copied()) {
        info.insert(m.id.clone(), (m.name.clone(), m.requires.clone(), m.source.clone()));
    }
    for r in mods.values() {
        info.insert(r.id.clone(), (r.name.clone(), r.requires.clone(), r.source.clone()));
    }
    // requirements none of those provide: their Workshop item, asked once each
    let (mut queue, mut asked): (Vec<(String, Option<String>)>, HashSet<String>) = (shown.iter().map(|m| (m.id.clone(), None)).collect(), HashSet::new());
    while let Some((id, needed_by)) = queue.pop() {
        if !asked.insert(id.clone()) {
            continue;
        }
        match info.get(&id) {
            Some((_, requires, source)) => {
                let item = workshop_item(source).map(String::from);
                queue.extend(requires.iter().map(|r| (r.clone(), item.clone())));
            }
            None => {
                if let Some((title, item)) = find(&id, needed_by.as_deref()) {
                    info.insert(id, (title, vec![], format!("workshop:{item}")));
                }
            }
        }
    }
    let url = |item: &str| format!("https://steamcommunity.com/sharedfiles/filedetails/?id={item}");
    let shown_ids: Vec<String> = shown.iter().map(|m| m.id.clone()).collect();
    let (ids, missing) = super::requirements_first(&shown_ids, |id| info.get(id).map(|i| i.1.clone()));
    let mut items = vec![];
    let mut text = String::new();
    for m in shown {
        let name = if m.name.is_empty() { &m.id } else { &m.name };
        text += &format!("{name}\n");
        if let Some(item) = workshop_item(&m.source) {
            text += &format!("  {}\n", url(item));
        }
        let folders: Vec<&str> = m.maps.iter().map(|f| f.folder.as_str()).collect();
        text += &format!("  {}\n", labels.ids.replace("{id}", &m.id).replace("{folders}", &folders.join("; ")));
    }
    let item_of = |id: &str| info.get(id).and_then(|(_, _, source)| workshop_item(source));
    let needed: Vec<&String> = ids.iter().filter(|i| !shown.iter().any(|m| m.id == **i)).collect();
    if !needed.is_empty() {
        text += &format!("\n{}\n", labels.required);
        for id in needed {
            let name = info.get(id).map(|i| i.0.as_str()).filter(|n| !n.is_empty() && n != id);
            text += &format!("  {}{}\n", name.map_or(id.clone(), |n| format!("{n} ({id})")), item_of(id).map(|i| format!(": {}", url(i))).unwrap_or_default());
        }
    }
    if !missing.is_empty() {
        text += &format!("\n{}\n", labels.missing.replace("{ids}", &missing.join(", ")));
    }
    for item in ids.iter().filter_map(|id| item_of(id)) {
        if !items.contains(&item) {
            items.push(item);
        }
    }
    let items: Vec<String> = items.into_iter().map(String::from).collect();
    text += "\n";
    for (key, value) in super::server::line_values(&ids, &items, map_line, b41) {
        text += &format!("{key}={value}\n");
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
        touch(&maps.join("Old Vanilla Folder/map.info"));
        let (folders, cells) = vanilla(Some(t.path()), 42, &mut Cache::default());
        assert!(folders.contains(&"Muldraugh, KY".to_string()) && folders.contains(&"Old Vanilla Folder".to_string()));
        assert_eq!(cells, Some(vec![(0, 1)]));
        assert_eq!(vanilla(Some(t.path()), 41, &mut Cache::default()).1, None, "a B42 install has no B41 cells");
        // no install found: Muldraugh is still the game's, so `lots=Muldraugh, KY` is never a missing requirement
        let (folders, cells) = vanilla(None, 42, &mut Cache::default());
        assert!(folders.contains(&super::super::server::BASE_MAP.to_string()) && cells.is_none());
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
        let names = |v: &[MapMod], fixed: &[MapMod]| quick_select(v, fixed, |_| 0);
        // A (first by name) blocks both B and C: greedy-by-name would stop at one of the three
        let all = [map("A", &[(1, 1), (2, 2)]), map("B", &[(1, 1)]), map("C", &[(2, 2)]), map("Spawn", &[]), map("D", &[(9, 9)])];
        assert_eq!(names(&all, &[]), ["B", "C", "D"]);
        // enabled maps stay: what they cover is taken
        assert_eq!(names(&all, &[all[1].clone()]), ["C", "D"]);
        // ...even when the filter hides them from the list
        assert_eq!(names(&all[2..], &[map("Hidden", &[(9, 9)])]), ["C"]);
        // a tie goes to the higher rank (installed, then subscribers)
        let pair = [map("X", &[(5, 5)]), map("Y", &[(5, 5)])];
        assert_eq!(quick_select(&pair, &[], |m| if m.id == "Y" { 7 } else { 1 }), ["Y"]);
        // past the exact limit: a 45-map chain still gets the best answer (23)
        let chain: Vec<MapMod> = (0..45).map(|i| map(&format!("M{i:02}"), &[(i, 0), (i + 1, 0)])).collect();
        assert_eq!(names(&chain, &[]).len(), 23);
    }

    #[test]
    fn quick_select_is_quick_on_the_whole_workshop() {
        // 3000 maps of 1-64 cells across the world, 400 of them piled on one town (like the
        // Workshop's many Louisville and Muldraugh edits)
        let mut seed = 7u64;
        let mut rnd = |n: i32| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) % n as u64) as i32
        };
        let all: Vec<MapMod> = (0..3000)
            .map(|i| {
                let (w, h) = (1 + rnd(8), 1 + rnd(8));
                let (x, y) = if i < 400 { (40 + rnd(6), 40 + rnd(6)) } else { (rnd(300), rnd(300)) };
                let cells: Vec<Cell> = (0..w).flat_map(|dx| (0..h).map(move |dy| (x + dx, y + dy))).collect();
                map(&format!("M{i}"), &cells)
            })
            .collect();
        let start = std::time::Instant::now();
        let pick = quick_select(&all, &all[..5], |_| 0);
        assert!(start.elapsed() < std::time::Duration::from_secs(3), "took {:?}", start.elapsed());
        // and the answer fits: no two picked maps (or a picked and an enabled one) share a cell
        let mut used: HashSet<Cell> = all[..5].iter().flat_map(|m| m.cells().copied()).collect();
        for id in &pick {
            let m = all.iter().find(|m| m.id == *id).unwrap();
            assert!(m.cells().all(|c| used.insert(*c)), "{id} overlaps");
        }
        assert!(pick.len() > 500, "{}", pick.len());
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
        let (b41, n41) = from_listing("2963883586", &files, 41, &mut read).unwrap();
        assert_eq!(n41, 2, "Riverboat and Tiles are B41 mods (a mod pack of two)");
        assert_eq!(b41.len(), 1, "Spawn has no B41 mod.info; Tiles has no maps");
        assert_eq!((b41[0].id.as_str(), b41[0].source.as_str()), ("Riverboat41", "workshop:2963883586"));
        assert_eq!(b41[0].maps, [MapFolder { folder: "Riverboat".into(), title: "Riverboat".into(), lots: vec!["Muldraugh, KY".into()], cells: vec![(43, 3)] }]);
        let (b42, n42) = from_listing("2963883586", &files, 42, &mut read).unwrap();
        assert_eq!(n42, 2);
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
        // a file from before builds were kept apart (item -> mods) is dropped, and kept as .bad
        let old = r#"{"111":[{"id":"A","name":"","source":"workshop:111","requires":[],"maps":[]}]}"#;
        fs::write(checked_file(&paths), old).unwrap();
        assert!(checked(&paths).is_empty());
        assert_eq!(fs::read_to_string(checked_file(&paths).with_extension("json.bad")).unwrap(), old);
        let c = Checked { updated: 100, builds: [(41, vec![map("Old", &[(1, 1)])]), (42, vec![map("New", &[(9, 9)])])].into(), ..Default::default() };
        save_checked(&paths, [("111".to_string(), c)].into()).unwrap();
        save_checked(&paths, [("222".to_string(), Checked { updated: 5, builds: [(41, vec![])].into(), ..Default::default() })].into()).unwrap();
        let all = checked(&paths);
        assert_eq!(all.len(), 2, "saves add to the file");
        let c = &all["111"];
        assert_eq!((c.builds[&41][0].id.as_str(), c.builds[&42][0].id.as_str()), ("Old", "New"), "each build keeps its own cells");
        assert!(!stale(Some(c), 100));
        assert!(stale(Some(c), 101), "the item was updated on the Workshop");
        assert!(stale(Some(&all["222"]), 5), "only read for B41 so far");
        assert!(stale(None, 5));
        // an item that can't be read waits for its next update instead of being retried every scan
        let failed = Checked { updated: 7, failed: "Steam lists no files for it".into(), ..Default::default() };
        assert!(!stale(Some(&failed), 7) && stale(Some(&failed), 8));
    }

    #[test]
    fn a_failed_read_keeps_the_last_good_one() {
        let good = Checked { title: "Town".into(), updated: 10, builds: [(41, vec![]), (42, vec![map("Town", &[(1, 1)])])].into(), manual: true, ..Default::default() };
        let item = Item { id: "111".into(), title: "Town v2".into(), time_updated: 20, ..Default::default() };
        let kept = unread(Some(&good), &item, "timeout".into());
        assert_eq!((kept.builds, kept.updated, kept.manual, kept.failed.as_str()), (good.builds.clone(), 10, true, "timeout"));
        assert!(stale(Some(&unread(Some(&good), &item, "x".into())), 20), "read again next scan");
        let fresh = unread(None, &item, "x".into());
        assert!(fresh.builds.is_empty() && fresh.updated == 20 && !stale(Some(&fresh), 20), "nothing to keep: waits for an update");
    }

    #[test]
    fn hand_checks_survive_a_scans_old_snapshot() {
        let (_t, paths) = store::tests::temp_paths();
        save_checked(&paths, [("111".to_string(), Checked { manual: true, updated: 1, ..Default::default() })].into()).unwrap();
        // the scan read it from a snapshot taken before the check
        save_checked(&paths, [("111".to_string(), Checked { updated: 2, ..Default::default() })].into()).unwrap();
        assert!(checked(&paths)["111"].manual);
        assert_eq!(checked(&paths)["111"].updated, 2);
    }

    #[test]
    fn a_newer_seed_fills_in_the_cache() {
        let at = |updated: u64, manual: bool| Checked { updated, manual, ..Default::default() };
        let mut all: BTreeMap<String, Checked> = [("old".into(), at(1, false)), ("mine".into(), at(1, true)), ("newer".into(), at(9, false))].into();
        let seed: BTreeMap<String, Checked> = [("old".into(), at(5, false)), ("mine".into(), at(5, false)), ("newer".into(), at(5, false)), ("new".into(), at(5, false))].into();
        merge_seed(&mut all, &seed);
        let got: Vec<(&str, u64)> = all.iter().map(|(k, c)| (k.as_str(), c.updated)).collect();
        assert_eq!(got, [("mine", 1), ("new", 5), ("newer", 9), ("old", 5)], "hand-checked and newer entries stay; missing and older ones come from the seed");
    }

    #[test]
    fn errors_carry_a_key_for_the_ui() {
        let e = anyhow::Error::new(Fail::Timeout).context("reading mod.info").context(Fail::FileList).context("workshop:1");
        assert_eq!(ui_error(&e), "maps.err.fileList: reading mod.info: Steam didn't answer in time");
        assert_eq!(ui_error(&anyhow::Error::new(Fail::Installed)), "maps.err.installed");
        assert_eq!(ui_error(&anyhow::anyhow!("disk full")), "disk full");
        assert!(network_error(&anyhow::Error::new(Fail::Timeout).context("reading mod.info")), "a silent Steam counts towards giving up");
    }

    #[test]
    fn presets_survive_a_damaged_or_unreadable_file() {
        let (_t, paths) = store::tests::temp_paths();
        set_preset(&paths, "Mine", Some(vec!["A".into()])).unwrap();
        let file = presets_file(&paths);
        // damaged: kept as .bad, then starts over
        fs::write(&file, "{ not json").unwrap();
        assert!(presets(&paths).is_empty());
        set_preset(&paths, "New", Some(vec!["B".into()])).unwrap();
        assert_eq!(fs::read_to_string(file.with_extension("json.bad")).unwrap(), "{ not json");
        assert_eq!(presets(&paths).into_keys().collect::<Vec<_>>(), ["New"]);
        // unreadable (a folder in its place): never saved over
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(set_preset(&paths, "Other", Some(vec![])).is_err());
        assert!(file.is_dir());
    }

    #[test]
    fn backgrounds_round_trip_per_build() {
        let (_t, paths) = store::tests::temp_paths();
        let bg = Background { path: r"D:\maps\Knox.png".into(), scale: [3.5, 3.25], offset: [-120.0, 6000.5] };
        set_background(&paths, 41, Some(bg.clone())).unwrap();
        assert_eq!(background(&paths, 41), Some(bg));
        assert_eq!(background(&paths, 42), None, "each build has its own");
        set_background(&paths, 41, None).unwrap();
        assert_eq!(background(&paths, 41), None);
    }

    #[test]
    fn concurrent_saves_keep_both_batches() {
        let (_t, paths) = store::tests::temp_paths();
        // two scans (the app's and the CLI's) saving at once; the data lock is per open file, so threads stand in for processes
        std::thread::scope(|s| {
            for scan in ["a", "b"] {
                let paths = &paths;
                s.spawn(move || {
                    for n in 0..10 {
                        save_checked(paths, [(format!("{scan}{n}"), Checked { updated: 1, ..Default::default() })].into()).unwrap();
                    }
                });
            }
        });
        assert_eq!(checked(&paths).len(), 20, "no batch lost");
        // an older read never replaces a newer one
        save_checked(&paths, [("a0".to_string(), Checked { updated: 5, title: "new".into(), ..Default::default() })].into()).unwrap();
        save_checked(&paths, [("a0".to_string(), Checked { updated: 4, title: "old".into(), ..Default::default() })].into()).unwrap();
        assert_eq!(checked(&paths)["a0"].title, "new");
    }

    #[test]
    fn a_checks_download_is_deleted_on_the_way_out() {
        let t = tempfile::tempdir().unwrap();
        let (done, partial) = (t.path().join("content/1"), t.path().join("downloads/1"));
        fs::create_dir_all(done.join("mods")).unwrap();
        fs::write(done.join("mods/x.txt"), "x").unwrap();
        drop(RemoveOnDrop([done.clone(), partial.clone()]));
        assert!(!done.exists() && !partial.exists(), "a missing folder is fine too");
    }

    #[test]
    fn an_unreadable_index_is_never_saved_over() {
        let (_t, paths) = store::tests::temp_paths();
        save_checked(&paths, [("111".to_string(), Checked { updated: 1, ..Default::default() })].into()).unwrap();
        let before = fs::read(checked_file(&paths)).unwrap();
        // a file that can't be read (here: a folder in its place, like a lock that won't let go)
        let file = checked_file(&paths);
        fs::rename(&file, file.with_extension("keep")).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(read_checked(&paths).is_err());
        assert!(save_checked(&paths, [("222".to_string(), Checked::default())].into()).is_err(), "the save stops instead of starting over");
        fs::remove_dir(&file).unwrap();
        fs::rename(file.with_extension("keep"), &file).unwrap();
        assert_eq!(fs::read(&file).unwrap(), before);
    }

    #[test]
    fn stop_ends_a_hung_listing() {
        let stop = AtomicBool::new(false);
        assert_eq!(unless_stopped(&stop, || Ok(7)).unwrap().unwrap(), 7);
        // a server that accepts and never answers: Stop returns at once, not after the timeout
        let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = silent.local_addr().unwrap();
        let start = std::time::Instant::now();
        let got = std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(200));
                stop.store(true, Relaxed);
            });
            unless_stopped(&stop, move || {
                let mut c = std::net::TcpStream::connect(addr)?;
                Ok(std::io::Read::read(&mut c, &mut [0; 1])?)
            })
        });
        assert!(got.is_none());
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn network_errors_are_told_apart() {
        let net = anyhow::Error::from(steamroom::error::Error::Io(std::io::Error::other("reset"))).context("reading mod.info");
        assert!(network_error(&net));
        let gone = anyhow::Error::from(steamroom::error::Error::from(steamroom::error::ConnectionError::ServiceMethodFailed(steamroom::enums::EResultError::FileNotFound)));
        assert!(!network_error(&gone), "Steam answered: the item itself can't be read");
        let busy = anyhow::Error::from(steamroom::error::Error::from(steamroom::error::ConnectionError::ServiceMethodFailed(steamroom::enums::EResultError::RateLimitExceeded)));
        assert!(network_error(&busy));
        assert!(!network_error(&anyhow::anyhow!("not in the file list")));
    }

    #[test]
    fn seed_round_trip() {
        let (t, paths) = store::tests::temp_paths();
        let c = Checked { title: "Town".into(), updated: 9, tags: vec!["Map".into()], builds: [(42, vec![map("Town", &[(1, 2)])])].into(), mods: [(42, 3)].into(), ..Default::default() };
        save_checked(&paths, [("111".to_string(), c.clone()), ("222".to_string(), Checked { manual: true, ..Default::default() })].into()).unwrap();
        let out = t.path().join("seed.json.gz");
        assert_eq!(write_seed(&paths, &out).unwrap(), 1, "hand-checked items stay out");
        assert_eq!(parse_seed(&fs::read(&out).unwrap()).unwrap(), [("111".to_string(), c)].into());
        // the seed built into PZMM parses
        parse_seed(SEED).unwrap();
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
        assert!(c.overlap.is_empty(), "one provider per folder: the Fort reupload isn't counted against Fort");

        // a public server's list: its maps on the same cells overlap, an add-on on the map it requires doesn't
        let (town, mut addon, ridge) = (map("Town", &[(1, 1), (1, 2)]), map("TownAddon", &[(1, 2)]), map("Ridge", &[(1, 2), (9, 9)]));
        addon.requires = vec!["Town".into()];
        let public = super::super::server::ServerMods { maps: ["TownAddon", "Town", "Ridge", "Muldraugh, KY"].map(String::from).to_vec(), ..Default::default() };
        let c = compare(&public, &[], &[], &[town, addon, ridge]);
        assert_eq!(c.only_server.len(), 3, "nothing of yours: every mod map it loads");
        assert_eq!(c.overlap, [["TownAddon", "Ridge"], ["Town", "Ridge"]].map(|p| p.map(String::from)));
    }

    #[test]
    fn reads_the_binary_world_map() {
        // 256-tile cells, 2x1: cell 0,0 empty; cell 1,0 has a road line and a building with a hole
        let mut b = b"IGMB".to_vec();
        let put = |b: &mut Vec<u8>, v: &[i32]| v.iter().for_each(|x| b.extend(x.to_le_bytes()));
        put(&mut b, &[2, 256, 2, 1, 4]);
        for s in ["Polygon", "LineString", "building", "highway"] {
            b.extend((s.len() as u16).to_le_bytes());
            b.extend(s.as_bytes());
        }
        put(&mut b, &[-1, 1, 0, 2]);
        let ring = |b: &mut Vec<u8>, pts: &[i16]| {
            b.extend(((pts.len() / 2) as u16).to_le_bytes());
            pts.iter().for_each(|p| b.extend(p.to_le_bytes()));
        };
        b.extend([1, 0, 1]); // LineString, one ring
        ring(&mut b, &[0, 5, -10, 5]);
        b.extend([1, 3, 0, 0, 0]); // highway=(index 0)
        b.extend([0, 0, 2]); // Polygon, outline and hole
        ring(&mut b, &[0, 0, 10, 0, 10, 10]);
        ring(&mut b, &[2, 2, 3, 3, 2, 3]);
        b.extend([1, 2, 0, 0, 0]);
        let m = WorldMap::from_bin(&b).unwrap();
        assert_eq!(m.road_lines, [vec![256, 5, 246, 5]]);
        assert_eq!(m.buildings, [vec![vec![256, 0, 266, 0, 266, 10], vec![258, 2, 259, 3, 258, 3]]]);
        assert!(m.water.is_empty() && m.roads.is_empty());
        // cut short, or not a version-2 file: no map
        assert_eq!(WorldMap::from_bin(&b[..b.len() - 3]), None);
        b[4] = 1;
        assert_eq!(WorldMap::from_bin(&b), None);
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
        let map = WorldMap::from_xml(xml);
        assert_eq!((map.water.len(), map.buildings.len()), (1, 1));
        assert_eq!(water_cells(&map, 300), [(1, 0)], "8 of 16 samples in water");
        // the same tiles (300..600 x 140..300) cover at most 6/16 of any 256-tile cell
        assert_eq!(water_cells(&map, 256), Vec::<Cell>::new());
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
            ..Default::default()
        };
        let mods: BTreeMap<String, store::Row> = [row("Town", "steam:111", &["Tiles", "Gone"]), row("Tiles", "workshop:222", &["Base"]), row("Base", "steam:111", &[])]
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();
        let mut town = map("Town", &[(1, 1)]);
        (town.source, town.requires, town.name) = ("steam:111".into(), vec!["Tiles".into(), "Gone".into()], "Town Name".into());
        let mut checked = map("Fort", &[(2, 2)]);
        checked.source = "workshop:333".into();
        let none = &mut |_: &str, _: Option<&str>| -> Option<(String, String)> { None };
        let text = export(&[&town, &checked], &mods, &[], &["Town".into(), "Fort".into(), "Muldraugh, KY".into()], false, &ExportLabels::default(), none);
        assert_eq!(
            text,
            "Town Name\n  https://steamcommunity.com/sharedfiles/filedetails/?id=111\n  Mod ID: Town · Map folder: Town\n\
             Fort\n  https://steamcommunity.com/sharedfiles/filedetails/?id=333\n  Mod ID: Fort · Map folder: Fort\n\
             \nRequired mods\n  Base: https://steamcommunity.com/sharedfiles/filedetails/?id=111\n  Tiles: https://steamcommunity.com/sharedfiles/filedetails/?id=222\n\
             \nRequired but not found (not installed, not on the Workshop): Gone\n\
             \nMods=\\Base;\\Tiles;\\Town;\\Fort\nWorkshopItems=111;222;333\nMap=Town;Fort;Muldraugh, KY\n"
        );
        assert!(export(&[&checked], &mods, &[], &[], true, &ExportLabels::default(), none).ends_with("\nMods=Fort\nWorkshopItems=333\n"));
        // a requirement nobody here has: looked up on the Workshop from the item that needs it, named and linked
        let mut asked = vec![];
        let labels = ExportLabels { ids: "Mod-ID: {id} · Kartenordner: {folders}".into(), required: "Benötigte Mods".into(), ..Default::default() };
        let text = export(&[&town], &mods, &[], &[], false, &labels, &mut |id, from| {
            asked.push(format!("{id}<{}", from.unwrap_or("")));
            (id == "Gone").then(|| ("Gone Forever".to_string(), "444".to_string()))
        });
        assert_eq!(asked, ["Gone<111"], "asked once, from Town's item");
        assert!(text.contains("  Mod-ID: Town · Kartenordner: Town\n"), "{text}");
        assert!(
            text.contains("\nBenötigte Mods\n  Base: https://steamcommunity.com/sharedfiles/filedetails/?id=111\n  Tiles: https://steamcommunity.com/sharedfiles/filedetails/?id=222\n  Gone Forever (Gone): https://steamcommunity.com/sharedfiles/filedetails/?id=444\n"),
            "{text}"
        );
        assert!(!text.contains("not found") && text.ends_with("\nMods=\\Base;\\Tiles;\\Gone;\\Town\nWorkshopItems=111;222;444\n"), "{text}");
    }

    #[test]
    fn export_of_previewed_workshop_maps() {
        // none installed: Expansion (shown) needs Base (shown, listed after it) and Tiles (a
        // known Workshop map that isn't shown), which needs Expansion back (a loop)
        let mk = |id: &str, item: &str, requires: &[&str]| MapMod { source: format!("workshop:{item}"), requires: requires.iter().map(|s| s.to_string()).collect(), ..map(id, &[]) };
        let (expansion, base, tiles) = (mk("Expansion", "1", &["Base", "Tiles"]), mk("Base", "2", &[]), mk("Tiles", "3", &["Expansion"]));
        let text = export(&[&expansion, &base], &BTreeMap::new(), &[tiles], &[], false, &ExportLabels::default(), &mut |_, _| None);
        assert!(!text.contains("not found"), "{text}");
        assert!(text.ends_with("\nMods=\\Base;\\Tiles;\\Expansion\nWorkshopItems=2;3;1\n"), "requirements first: {text}");
        assert!(text.contains("Required mods\n  Tiles: https://steamcommunity.com/sharedfiles/filedetails/?id=3\n"));
    }
}
