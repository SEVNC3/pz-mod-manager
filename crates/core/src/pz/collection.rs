//! A Workshop collection (or one modpack item) read without installing it: every item it
//! holds plus what they require, each item's mod IDs from its mod.info (installed items from
//! disk, the rest from Steam's file lists), the maps they add, and any sandbox preset a pack
//! ships. Feeds a profile, a server's mod lines and a Maps preview.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::Paths;
use crate::sources::steam_manifest::Session;
use crate::sources::steam_workshop::{self, Item};
use crate::sources::{self, Source};
use crate::store;

/// What a collection loads on one build.
#[derive(Debug, Default, Serialize)]
pub struct Pack {
    pub title: String,
    /// Its items and the items they require.
    pub items: Vec<String>,
    /// Mod IDs in load order (what a mod requires before it).
    pub mods: Vec<String>,
    /// Mod ID -> the Workshop item that holds it (for the profile; the UI doesn't need it).
    #[serde(skip)]
    pub item_of: BTreeMap<String, String>,
    /// Workshop item -> every mod it has on the build (to choose from), and its title.
    pub item_mods: BTreeMap<String, Vec<String>>,
    pub titles: BTreeMap<String, String>,
    /// Items already installed, and items the Workshop no longer has (removed, hidden or private).
    pub installed: Vec<String>,
    pub delisted: Vec<String>,
    /// The map folders those mods add, for `Map=` (the game's own map last).
    pub maps: Vec<String>,
    /// Sandbox presets it ships, once saved as your own presets by [`save_presets`] (their names).
    pub presets: Vec<String>,
    /// The sandbox presets it ships, not saved yet: (name, values).
    #[serde(skip)]
    pub shipped: Vec<(String, BTreeMap<String, String>)>,
    /// What the player should know: items read from their description, mods left out...
    pub notes: Vec<String>,
}

/// One mod as its mod.info says.
#[derive(Debug, Default, Clone, PartialEq)]
struct Found {
    id: String,
    requires: Vec<String>,
    incompatible: Vec<String>,
}

/// What one item adds on a build.
#[derive(Debug, Default)]
struct Read {
    mods: Vec<Found>,
    /// (mod id, map folder)
    maps: Vec<(String, String)>,
    /// (preset name, its Lua text)
    presets: Vec<(String, String)>,
}

/// A sandbox preset file: `media/lua/shared/Sandbox/<Name>.lua` (as the game ships its own) or a
/// server's `<name>_SandboxVars.lua`. Returns the preset's name.
fn preset_name(path: &str) -> Option<String> {
    let file = path.rsplit('/').next()?;
    if !file.to_ascii_lowercase().ends_with(".lua") {
        return None;
    }
    let stem = &file[..file.len() - 4];
    if path.to_ascii_lowercase().contains("media/lua/shared/sandbox/") {
        return Some(stem.to_string());
    }
    stem.strip_suffix("_SandboxVars").filter(|n| !n.is_empty()).map(String::from)
}

/// An item's mods, maps and presets on `build`, from its file list (paths with `/`); `read`
/// gives a listed file's text.
fn from_files(item: &str, files: &[String], build: u8, read: &mut dyn FnMut(&str) -> Result<String>) -> Result<Read> {
    let mut out = Read::default();
    for (dir, info, _) in super::maps::listing_mods(files, build) {
        let i = super::parse_info(&read(&info)?);
        let id = i.get("id").filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| dir.rsplit('/').next().unwrap_or(&dir).to_string());
        out.mods.push(Found { id, requires: super::requires(&i), incompatible: super::id_list(&i, "incompatible") });
    }
    for m in super::maps::from_listing(item, files, build, read)?.0 {
        out.maps.extend(m.maps.into_iter().map(|f| (m.id.clone(), f.folder)));
    }
    for f in files {
        if let Some(name) = preset_name(f).filter(|n| !out.presets.iter().any(|p| &p.0 == n)) {
            out.presets.push((name, read(f)?));
        }
    }
    Ok(out)
}

/// An installed item: its mods as scanned, maps from their folders, presets from disk.
fn from_installed(rows: &[&store::Row], build: u8) -> Read {
    let mut out = Read::default();
    for r in rows {
        out.mods.push(Found { id: r.id.clone(), requires: r.requires.clone(), incompatible: r.incompatible.clone() });
        out.maps.extend(super::maps::folder_names(&r.dir, build).into_iter().map(|f| (r.id.clone(), f)));
        for root in super::files::content_roots(&r.dir) {
            for e in fs::read_dir(root.join("media/lua/shared/Sandbox")).into_iter().flatten().flatten() {
                let path = e.path().to_string_lossy().replace('\\', "/");
                if let (Some(name), Ok(text)) = (preset_name(&path), fs::read(e.path()).map(|b| super::decode_text(&b))) {
                    out.presets.push((name, text));
                }
            }
        }
    }
    out
}

/// Read an item from Steam's file list on `session`.
fn from_steam(session: &Session, item: &Item, build: u8, stop: &AtomicBool) -> Result<Read> {
    if item.hcontent == 0 {
        bail!("Steam lists no files for it");
    }
    let files = session.files(item.hcontent, stop)?;
    let names: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let mut read = |p: &str| -> Result<String> {
        let f = files.iter().find(|f| f.path == p).context("not in the file list")?;
        Ok(super::decode_text(&session.read(f, stop).with_context(|| format!("reading {p}"))?))
    };
    from_files(&item.id, &names, build, &mut read)
}

/// The Workshop IDs in `input`: links or IDs, any number, split by spaces, commas or lines.
pub fn ids(input: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for part in input.split(|c: char| c.is_whitespace() || c == ',').filter(|p| !p.is_empty()) {
        match Source::parse(part) {
            Source::Workshop(id) if steam_workshop::valid_id(&id) => {
                if !out.contains(&id) {
                    out.push(id)
                }
            }
            _ => bail!("{part} isn't a Workshop link or ID"),
        }
    }
    if out.is_empty() {
        bail!("paste a Workshop collection link or ID");
    }
    Ok(out)
}

/// The collections' titles and their items with what they require (an item that isn't a
/// collection counts as a one-item pack). `input` may hold several links or IDs.
pub fn items(paths: &Paths, input: &str) -> Result<(String, Vec<String>)> {
    let ids = ids(input)?;
    // reading the pages stops on the downloads' Stop, as installing does
    sources::STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let titles: Vec<String> = steam_workshop::details(&refs)?.into_iter().map(|i| i.title.trim().to_string()).filter(|t| !t.is_empty()).collect();
    Ok((titles.join(" + "), sources::expand_workshop(paths, ids).0))
}

/// What `items` load on `build`: installed items read from disk, the rest from Steam's file
/// lists (several at once on one session), else from their description's "Mod ID:" lines.
/// Sandbox presets it ships are read, not saved (see [`save_presets`]). Mods in `skip` are left out; of an
/// item's alternative versions (mods incompatible with each other), the first one left stays.
pub fn read(paths: &Paths, title: &str, items: &[String], build: u8, skip: &[String], stop: &AtomicBool) -> Result<Pack> {
    let installed = store::rows(paths).unwrap_or_default();
    let rows = |item: &str| -> Vec<&store::Row> {
        installed.values().filter(|r| sources::workshop_item(&r.source) == Some(item) && if build == 41 { r.supports_41 } else { r.supports_42 }).collect()
    };
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    let details: Vec<Item> = refs.chunks(100).map(steam_workshop::details).collect::<Result<Vec<_>>>()?.concat();
    let have = sources::workshop_available(paths);
    let todo: Vec<&Item> = details.iter().filter(|d| !have.contains(&d.id)).collect();
    // file lists: up to 4 at once on one Steam session
    let read_from_steam: Mutex<BTreeMap<String, Result<Read>>> = Mutex::new(BTreeMap::new());
    if !todo.is_empty() {
        match Session::open(stop) {
            Ok(session) => {
                let next = AtomicUsize::new(0);
                std::thread::scope(|s| {
                    for _ in 0..4.min(todo.len()) {
                        s.spawn(|| {
                            while let Some(item) = todo.get(next.fetch_add(1, Relaxed)).filter(|_| !stop.load(Relaxed)) {
                                let r = from_steam(&session, item, build, stop);
                                read_from_steam.lock().unwrap_or_else(|e| e.into_inner()).insert(item.id.clone(), r);
                            }
                        });
                    }
                });
            }
            Err(e) => crate::log!("error: no Steam session for the collection's file lists: {e:#}"),
        }
    }
    let mut from_steam = read_from_steam.into_inner().unwrap_or_else(|e| e.into_inner());
    let mut pack = Pack { title: title.to_string(), items: items.to_vec(), ..Default::default() };
    pack.titles = details.iter().map(|d| (d.id.clone(), d.title.trim().to_string())).collect();
    pack.installed = items.iter().filter(|i| have.contains(*i)).cloned().collect();
    // kept, marked: an installed copy still loads, and a server's check flags it
    pack.delisted = items.iter().filter(|i| !details.iter().any(|d| &d.id == *i)).cloned().collect();
    let mut all: Vec<Found> = vec![];
    let mut maps: Vec<(String, String)> = vec![];
    let mut presets: Vec<(String, String)> = vec![];
    for item in items {
        let d = details.iter().find(|d| &d.id == item);
        let name = d.map(|d| d.title.trim()).filter(|t| !t.is_empty()).map_or_else(|| item.clone(), |t| format!("{t} ({item})"));
        let mut noted = false;
        let read = if have.contains(item) {
            from_installed(&rows(item), build)
        } else if d.is_none() {
            noted = true;
            pack.notes.push(format!("{item} isn't on the Workshop any more (removed, hidden or private): it can't be installed"));
            Read::default()
        } else {
            match from_steam.remove(item) {
                Some(Ok(r)) => r,
                other => {
                    let why = match other {
                        Some(Err(e)) => format!("{e:#}"),
                        _ => "Steam didn't give its file list".into(),
                    };
                    crate::log!("workshop:{item}: {why}");
                    noted = true;
                    let ids = d.map(|d| super::workshop_mod_ids(&d.description)).unwrap_or_default();
                    pack.notes.push(if ids.is_empty() {
                        format!("{name}: couldn't read its files and its description names no Mod ID: add it to Mods by hand")
                    } else {
                        format!("{name}: couldn't read its files, so its Mod ID came from its description ({})", ids.join(", "))
                    });
                    Read { mods: ids.into_iter().map(|id| Found { id, ..Default::default() }).collect(), ..Default::default() }
                }
            }
        };
        if read.mods.is_empty() && !noted {
            pack.notes.push(format!("{name} has no mods for Build {build}"));
        }
        pack.item_mods.insert(item.clone(), read.mods.iter().map(|m| m.id.clone()).collect());
        for m in read.mods {
            if all.iter().any(|o| o.id == m.id) || skip.contains(&m.id) {
                continue;
            }
            // items often ship alternative versions of one mod: the first one stays
            if let Some(o) = all.iter().find(|o| o.incompatible.contains(&m.id) || m.incompatible.contains(&o.id)) {
                pack.notes.push(format!("{} left out: it's incompatible with {}", m.id, o.id));
                continue;
            }
            pack.item_of.insert(m.id.clone(), item.clone());
            all.push(m);
        }
        maps.extend(read.maps);
        presets.extend(read.presets);
    }
    // requirements already installed are turned on with it (one installed but not listed would stay off)
    let mut skip_more: Vec<String> = vec![];
    loop {
        let more: Vec<Found> = all
            .iter()
            .flat_map(|m| &m.requires)
            .filter(|r| !all.iter().any(|o| &o.id == *r) && !skip.contains(*r) && !skip_more.contains(*r))
            .filter_map(|r| installed.get(r))
            .filter(|r| if build == 41 { r.supports_41 } else { r.supports_42 })
            .map(|r| Found { id: r.id.clone(), requires: r.requires.clone(), incompatible: r.incompatible.clone() })
            .collect();
        if more.is_empty() {
            break;
        }
        let before = all.len();
        for m in more {
            if all.iter().any(|o| o.id == m.id) {
                continue;
            }
            if let Some(o) = all.iter().find(|o| o.incompatible.contains(&m.id) || m.incompatible.contains(&o.id)) {
                pack.notes.push(format!("{} left out: it's incompatible with {}", m.id, o.id));
                skip_more.push(m.id.clone());
                continue;
            }
            all.push(m);
        }
        if all.len() == before {
            break;
        }
    }
    for m in &all {
        // one installed that doesn't load on this build counts as missing
        let loads = |r: &String| installed.get(r).is_some_and(|i| if build == 41 { i.supports_41 } else { i.supports_42 });
        for r in m.requires.iter().filter(|r| !all.iter().any(|o| &o.id == *r) && !loads(r)) {
            pack.notes.push(format!("{} needs {r}, which the collection doesn't include", m.id));
        }
    }
    pack.mods = super::requirements_first(&all.iter().map(|m| m.id.clone()).collect::<Vec<_>>(), |id| Some(all.iter().find(|m| m.id == id)?.requires.clone())).0;
    let folders: Vec<String> = maps.into_iter().filter(|(id, _)| pack.mods.contains(id)).map(|(_, f)| f).collect();
    pack.maps = super::server::map_order(&[], &folders);
    // a Lua file in a Sandbox folder that isn't a preset (mod code) doesn't parse: skipped
    for (name, values) in presets.into_iter().filter_map(|(n, t)| Some((n, super::server_config::preset_values(&t).ok().filter(|v| !v.is_empty())?))) {
        let full = if title.trim().is_empty() { name } else { format!("{} - {name}", title.trim()) };
        let name: String = full.chars().filter(|c| !r#"<>:"/\|?*"#.contains(*c)).collect::<String>().trim().to_string();
        if !pack.shipped.iter().any(|s| s.0 == name) {
            pack.shipped.push((name, values));
        }
    }
    Ok(pack)
}

/// Save the sandbox presets `pack` ships as the player's own (once they import it), each under a
/// name no preset has yet (`<name> 2`...): one of theirs is never replaced. One saved before with
/// the same values (the same collection loaded again) is used as it is. Notes what was saved.
pub fn save_presets(paths: &Paths, pack: &mut Pack) {
    let version = super::server_config::sandbox_version(super::zombiebuddy::game_dir().and_then(|g| super::zombiebuddy::launcher_dir(&g)).as_deref());
    for (base, values) in std::mem::take(&mut pack.shipped) {
        let taken = super::server_config::own_presets(paths);
        let same = |n: &String| super::server_config::preset(paths, std::path::Path::new(""), n, true).is_ok_and(|v| v == values);
        let free = std::iter::once(base.clone()).chain((2..).map(|i| format!("{base} {i}"))).find(|n| !taken.iter().any(|t| t.eq_ignore_ascii_case(n)) || same(n)).unwrap_or(base);
        if taken.iter().any(|t| t.eq_ignore_ascii_case(&free)) {
            pack.presets.push(free);
            continue;
        }
        match super::server_config::save_preset(paths, &free, &values, version) {
            Ok(()) => {
                pack.notes.push(format!("Saved the sandbox preset it ships as \"{free}\""));
                pack.presets.push(free);
            }
            Err(e) => crate::log!("error: saving the sandbox preset {free}: {e:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(id: &str, requires: &[&str]) -> Found {
        Found { id: id.into(), requires: requires.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn a_pack_read_from_its_file_list() {
        let files: Vec<String> = [
            "mods/Pack/42/mod.info",
            "mods/Pack/common/media/lua/shared/Sandbox/Hard Times.lua",
            "mods/Pack/mod.info",
            "mods/Town/42/mod.info",
            "mods/Town/42/media/maps/Town/map.info",
            "mods/Town/42/media/maps/Town/0_0.lotheader",
            "server/pvp_SandboxVars.lua",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut read = |p: &str| -> Result<String> {
            Ok(match p {
                "mods/Pack/42/mod.info" => "id=Pack42\nrequire=\\Lib\nincompatible=PackLite\n",
                "mods/Pack/mod.info" => "id=Pack41\n",
                "mods/Town/42/mod.info" => "name=Town\n",
                "mods/Town/42/media/maps/Town/map.info" => "title=Town\n",
                _ => "return {\n    Zombies = 4,\n}\n",
            }
            .to_string())
        };
        let r = from_files("1", &files, 42, &mut read).unwrap();
        assert_eq!(r.mods, [Found { id: "Pack42".into(), requires: vec!["Lib".into()], incompatible: vec!["PackLite".into()] }, found("Town", &[])], "a mod.info without id= is its folder's name");
        assert_eq!(r.maps, [("Town".to_string(), "Town".to_string())]);
        assert_eq!(r.presets.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(), ["Hard Times", "pvp"]);
        assert_eq!(from_files("1", &files, 41, &mut read).unwrap().mods, [found("Pack41", &[])], "B41 reads root mod.info files");
        assert_eq!(super::super::server_config::preset_values(&r.presets[0].1).unwrap(), [("Zombies".to_string(), "4".to_string())].into());
    }

    #[test]
    fn shipped_presets_never_replace_the_players() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let values: BTreeMap<String, String> = [("Zombies".to_string(), "4".to_string())].into();
        super::super::server_config::save_preset(&paths, "Pack - Hard", &[("Zombies".to_string(), "1".to_string())].into(), 5).unwrap();
        let mut pack = Pack { shipped: vec![("Pack - Hard".into(), values.clone()), ("Pack - Easy".into(), values)], ..Default::default() };
        save_presets(&paths, &mut pack);
        assert_eq!(pack.presets, ["Pack - Hard 2", "Pack - Easy"]);
        assert!(pack.shipped.is_empty());
        assert_eq!(super::super::server_config::own_presets(&paths), ["Pack - Easy", "Pack - Hard", "Pack - Hard 2"]);
        // loaded again: the copies saved before are used, not saved once more
        let mut again = Pack { shipped: vec![("Pack - Hard".into(), [("Zombies".to_string(), "4".to_string())].into())], ..Default::default() };
        save_presets(&paths, &mut again);
        assert_eq!(again.presets, ["Pack - Hard 2"]);
        assert_eq!(super::super::server_config::own_presets(&paths).len(), 3);
    }

    #[test]
    fn several_links_at_once() {
        let got = ids("https://steamcommunity.com/sharedfiles/filedetails/?id=111&searchtext=x
 222, 333	111").unwrap();
        assert_eq!(got, ["111", "222", "333"]);
        assert!(ids("  ").is_err());
        assert!(ids("111 not-a-link").is_err());
    }

    #[test]
    #[ignore = "reads a real Workshop collection from Steam (PZMM_COLLECTION=<id>)"]
    fn live_collection() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let id = std::env::var("PZMM_COLLECTION").unwrap_or_else(|_| "3814792910".into());
        let (title, items) = items(&paths, &id).unwrap();
        let pack = read(&paths, &title, &items, 42, &[], &AtomicBool::new(false)).unwrap();
        eprintln!("{pack:#?}");
        assert!(!pack.mods.is_empty());
    }
}
