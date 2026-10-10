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

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{Paths, is_version_dir};
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

/// One map folder, read from disk.
pub fn read_folder(dir: &Path) -> MapFolder {
    let mut cells: Vec<Cell> = fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| cell_of(e.file_name().to_str()?)).collect();
    cells.sort();
    cells.dedup();
    let folder = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    // map.info repeats `lots=` once per folder, and folder names hold commas ("Muldraugh, KY")
    let info = fs::read(dir.join("map.info")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let value = |l: &str, key: &str| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().to_string());
    let lots = info.lines().filter_map(|l| value(l, "lots")).filter(|v| !v.is_empty()).collect();
    let mut title = info.lines().find_map(|l| value(l, "title")).unwrap_or_default();
    // B42 points at a translation: "See media/lua/shared/Translate/EN/<map>/title.txt"
    if let Some(rel) = title.strip_prefix("See ") {
        let root = dir.parent().and_then(Path::parent).and_then(Path::parent).unwrap_or(dir);
        title = fs::read_to_string(root.join(rel.trim())).map(|t| t.trim().to_string()).unwrap_or_default();
    }
    if title.is_empty() {
        title = folder.clone();
    }
    MapFolder { folder, title, lots, cells }
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

/// Where a mod's `media/maps` lives for `build`: the mod folder in B41; in B42 `common/` and
/// the newest version folder (as the game picks; see `find_mods`).
fn map_roots(mod_dir: &Path, build: u8) -> Vec<PathBuf> {
    if build == 41 {
        return vec![mod_dir.to_path_buf()];
    }
    let mut versions: Vec<(Vec<u32>, PathBuf)> = vec![];
    let mut out = vec![];
    for e in fs::read_dir(mod_dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !e.file_type().is_ok_and(|t| t.is_dir()) || !is_version_dir(&name) {
            continue;
        }
        if name == "common" {
            out.push(e.path());
        } else {
            versions.push((name.split('.').filter_map(|p| p.parse().ok()).collect(), e.path()));
        }
    }
    out.extend(versions.into_iter().max().map(|v| v.1));
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

/// One cell of the grid: what fills it, among vanilla and the shown maps.
#[derive(Debug, PartialEq, Serialize)]
pub struct GridCell {
    pub x: i32,
    pub y: i32,
    /// "vanilla" (the game's only), "map" (one map), "over" (one map over a vanilla area,
    /// usually on purpose) or "conflict" (two or more maps).
    pub kind: &'static str,
    /// Ids of the shown maps that fill it.
    pub maps: Vec<String>,
}

pub fn grid(vanilla: &[Cell], shown: &[&MapMod]) -> Vec<GridCell> {
    let base: HashSet<&Cell> = vanilla.iter().collect();
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
                (0, _) => "vanilla",
                (1, false) => "map",
                (1, true) => "over",
                _ => "conflict",
            };
            GridCell { x, y, kind, maps }
        })
        .collect()
}

/// Maps from `all` (in order) that share no cell with the `shown` ones or each other: what
/// "Select maps that don't conflict" adds. Covering vanilla isn't a conflict; maps without
/// cells (spawn points only) are left out.
pub fn non_conflicting(all: &[MapMod], shown: &[String]) -> Vec<String> {
    let mut taken: HashSet<Cell> = all.iter().filter(|m| shown.contains(&m.id)).flat_map(|m| m.cells().copied()).collect();
    let mut add = vec![];
    for m in all.iter().filter(|m| !shown.contains(&m.id)) {
        let cells: Vec<Cell> = m.cells().copied().collect();
        if !cells.is_empty() && !cells.iter().any(|c| taken.contains(c)) {
            taken.extend(cells);
            add.push(m.id.clone());
        }
    }
    add
}

fn checked_file(paths: &Paths) -> PathBuf {
    paths.data.join("maps-checked.json")
}

/// Workshop items checked without installing: item id -> its map mods (cells only, no files).
pub fn checked(paths: &Paths) -> BTreeMap<String, Vec<MapMod>> {
    fs::read(checked_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The map mods in a downloaded item folder (not cached: the folder is about to go).
pub fn read_item(dir: &Path, item: &str, build: u8) -> Vec<MapMod> {
    let mut cache = Cache::default();
    super::find_mods(dir, 8)
        .into_iter()
        .filter_map(|m| {
            let maps = mod_maps(&m.dir, build, &mut cache);
            (!maps.is_empty()).then(|| MapMod { id: m.id, name: m.name, source: format!("workshop:{item}"), requires: m.requires, maps })
        })
        .collect()
}

/// Download Workshop `item` with SteamCMD, read its cells, keep only those (in
/// `maps-checked.json`) and delete the download. Nothing is installed.
pub fn check(paths: &Paths, item: &str, build: u8) -> Result<Vec<MapMod>> {
    let got = crate::sources::steam_workshop::download(paths, super::STEAM_APP, &[item])?;
    let dir = got.into_iter().next().and_then(|(_, d)| d).with_context(|| format!("workshop:{item}: download failed"))?;
    let found = read_item(&dir, item, build);
    fs::remove_dir_all(&dir).with_context(|| format!("deleting the download in {}", dir.display()))?;
    let mut all = checked(paths);
    all.insert(item.to_string(), found.clone());
    crate::write_atomic(&checked_file(paths), serde_json::to_vec(&all)?)?;
    Ok(found)
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
        let g = grid(&[(1, 1), (3, 3), (9, 9)], &[&a, &b]);
        let kinds: Vec<(Cell, &str, usize)> = g.iter().map(|c| ((c.x, c.y), c.kind, c.maps.len())).collect();
        assert_eq!(kinds, [((1, 1), "over", 1), ((2, 2), "conflict", 2), ((3, 3), "over", 1), ((5, 5), "map", 1), ((9, 9), "vanilla", 0)]);
    }

    #[test]
    fn selects_maps_that_dont_conflict() {
        let all = [map("A", &[(1, 1)]), map("B", &[(1, 1), (2, 2)]), map("C", &[(3, 3)]), map("D", &[(3, 3)]), map("Spawn", &[]), map("E", &[(4, 4)])];
        assert_eq!(non_conflicting(&all, &["A".into()]), ["C", "E"], "B clashes with A, D with C, Spawn has no cells");
        assert_eq!(non_conflicting(&all, &[]), ["A", "C", "E"]);
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
