//! Files the game's mod menu and the Workshop mod managers keep in `Zomboid/Lua`, read and
//! written in their own formats so PZMM and they share one set of lists:
//! - `saved_modlists.txt` (Mod Manager): `VERSION=2`, then `name:A;B;C` presets; its
//!   favourites and hidden mods are the `mmFavorites` and `mmHidden` presets.
//! - `pz_modlist_settings.cfg` (the game's own B42 mod menu, and [B42] Mod Manager): a first
//!   `!fav!:A;B;` line of favourites, then `name:A;B;` presets.
//! - `saved_modlists_server.txt` (Mod Manager: Server): `VERSION=1`, then `[name]` blocks of
//!   `WorkshopItems=` and `Mods=` lines.
//! - `ModManager/ModListData.ini` ([B42] Mod Manager): a Lua table with `hidden = true` per mod.
//! - `sorting_rules.txt` (Mod Load Order Sorter): `[ModId]` blocks of `loadAfter=`,
//!   `loadBefore=`, `incompatibleMods=`, `loadFirst=`, `loadLast=` and `category=` lines.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::Paths;
use super::server::ServerMods;

const MOD_MANAGER: &str = "saved_modlists.txt";
const GAME: &str = "pz_modlist_settings.cfg";
const SERVER: &str = "saved_modlists_server.txt";
const B42_MANAGER: &str = "ModManager/ModListData.ini";
const RULES: &str = "sorting_rules.txt";
const FAVOURITES: &str = "mmFavorites";
const HIDDEN: &str = "mmHidden";
const GAME_FAVOURITES: &str = "!fav!";

fn file(paths: &Paths, name: &str) -> PathBuf {
    // the Lua folder of the Zomboid folder the game plays in (the active profile's own, if it has one)
    let pzmm = super::bridge::dir(paths);
    pzmm.parent().map_or_else(|| paths.zomboid.join("Lua"), std::path::Path::to_path_buf).join(name)
}

/// One of the game's Lua files: missing is empty; one that can't be read is an error (a write
/// built on it would wipe it).
fn try_read(paths: &Paths, name: &str) -> Result<String> {
    match fs::read(file(paths, name)) {
        Ok(b) => Ok(super::decode_text(&b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("reading {}", file(paths, name).display())),
    }
}

/// `try_read` for showing: unreadable shows as empty.
fn read(paths: &Paths, name: &str) -> String {
    try_read(paths, name).unwrap_or_default()
}

/// Write one of the game's Lua files, keeping the original as `<file>.bak` the first time.
fn write(paths: &Paths, name: &str, text: &str) -> Result<()> {
    let f = file(paths, name);
    if let Some(dir) = f.parent() {
        fs::create_dir_all(dir)?;
    }
    crate::backup_once(&f)?;
    Ok(crate::write_atomic(&f, text)?)
}

/// Mod ids in a `;` or `,` list, without B42's `\` and without blanks.
fn ids(list: &str) -> Vec<String> {
    super::server::split(&list.replace(',', ";"))
}

/// The `name:A;B;C` lines of Mod Manager's or the game's presets file (`VERSION=` skipped).
fn parse_lines(text: &str) -> Vec<(String, Vec<String>)> {
    text.lines()
        .filter(|l| !l.starts_with("VERSION="))
        .filter_map(|l| l.split_once(':'))
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, list)| (name.to_string(), ids(list)))
        .collect()
}

/// `text` with line `name:...` set to `ids` (added at the end when new; removed when `ids` is
/// empty). `game` writes the game's format (`A;B;`, `\` before each id when the file's ids
/// have it, and a first `!fav!` line), else Mod Manager's (`A;B`, after `VERSION=2`).
fn set_line(text: &str, name: &str, ids: &[String], game: bool) -> String {
    // the game's ids carry B42's `\` up to 42.14 and not after: follow the file
    let slash = if game && (text.contains(":\\") || text.contains(";\\")) { "\\" } else { "" };
    let line = if game { format!("{name}:{}", ids.iter().map(|i| format!("{slash}{i};")).collect::<String>()) } else { format!("{name}:{}", ids.join(";")) };
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let at = lines.iter().position(|l| l.split_once(':').is_some_and(|(n, _)| n == name));
    match at {
        Some(i) if ids.is_empty() && !(game && name == GAME_FAVOURITES) => drop(lines.remove(i)),
        Some(i) => lines[i] = line,
        None if ids.is_empty() => {}
        None => lines.push(line),
    }
    if game && !lines.first().is_some_and(|l| l.starts_with(GAME_FAVOURITES)) {
        lines.insert(0, format!("{GAME_FAVOURITES}:")); // the game reads favourites from the first line only
    }
    if !game && !lines.first().is_some_and(|l| l.starts_with("VERSION=")) {
        lines.insert(0, "VERSION=2".into());
    }
    lines.iter().map(|l| format!("{l}\n")).collect()
}

/// Mod Manager: Server's presets: `[name]` blocks of `key=A;B` lines.
fn parse_server(text: &str) -> Vec<(String, ServerMods)> {
    let mut out: Vec<(String, ServerMods)> = vec![];
    for l in text.lines().map(str::trim).filter(|l| !l.starts_with("VERSION=")) {
        if let Some(name) = l.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((name.trim().to_string(), ServerMods::default()));
        } else if let (Some((k, v)), Some((_, s))) = (l.split_once('='), out.last_mut()) {
            match k.trim() {
                "Mods" => s.mods = ids(v),
                "WorkshopItems" => s.workshop = ids(v),
                "Map" => s.maps = v.split(';').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect(),
                _ => {}
            }
        }
    }
    out
}

fn write_server(presets: &[(String, ServerMods)]) -> String {
    let mut out = String::from("VERSION=1\r\n");
    for (name, s) in presets {
        out += &format!("[{name}]\r\nWorkshopItems={}\r\nMods={}\r\n", s.workshop.join(";"), s.mods.join(";"));
        if !s.maps.is_empty() {
            out += &format!("Map={}\r\n", s.maps.join(";"));
        }
    }
    out
}

/// A mod list another manager saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// "modManager" (Mod Manager), "game" (the game's own presets) or "server" (Mod Manager: Server).
    pub from: String,
    pub name: String,
    /// Mod ids, in load order.
    pub mods: Vec<String>,
    /// Workshop items (server presets only).
    pub workshop: Vec<String>,
}

/// Every preset the game and the Workshop mod managers saved on this computer.
pub fn presets(paths: &Paths) -> Vec<Preset> {
    let lines = |from: &str, text: String| -> Vec<Preset> {
        parse_lines(&text)
            .into_iter()
            .filter(|(n, m)| ![FAVOURITES, HIDDEN, GAME_FAVOURITES].contains(&n.as_str()) && !m.is_empty())
            .map(|(name, mods)| Preset { from: from.into(), name, mods, workshop: vec![] })
            .collect()
    };
    let mut out = lines("modManager", read(paths, MOD_MANAGER));
    out.extend(lines("game", read(paths, GAME)));
    out.extend(parse_server(&read(paths, SERVER)).into_iter().map(|(name, s)| Preset { from: "server".into(), name, mods: s.mods, workshop: s.workshop }));
    out
}

/// Characters none of the three formats allow in a preset name.
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && !name.contains(['/', '\\', '|', ':', ';', '.', ',', '[', ']', '\r', '\n']) && ![FAVOURITES, HIDDEN].contains(&name)
}

/// Save a preset (replacing one of that name) where `p.from` says, in that manager's format.
pub fn save_preset(paths: &Paths, p: &Preset) -> Result<()> {
    if !valid_name(&p.name) {
        bail!("a preset name can't contain / \\ | : ; . , [ or ]");
    }
    match p.from.as_str() {
        "modManager" => write(paths, MOD_MANAGER, &set_line(&try_read(paths, MOD_MANAGER)?, &p.name, &p.mods, false)),
        "game" => write(paths, GAME, &set_line(&try_read(paths, GAME)?, &p.name, &p.mods, true)),
        "server" => {
            let mut all = parse_server(&try_read(paths, SERVER)?);
            let s = ServerMods { mods: p.mods.clone(), workshop: p.workshop.clone(), maps: vec![] };
            match all.iter_mut().find(|(n, _)| *n == p.name) {
                Some(old) => old.1 = ServerMods { maps: old.1.maps.clone(), ..s },
                None => all.push((p.name.clone(), s)),
            }
            write(paths, SERVER, &write_server(&all))
        }
        other => bail!("unknown preset kind {other:?}"),
    }
}

/// Delete the preset `name` from the file `from` says (one of that name elsewhere stays). Any name the game or a
/// manager saved it under, but only a preset that's there.
pub fn delete_preset(paths: &Paths, from: &str, name: &str) -> Result<()> {
    if !presets(paths).iter().any(|p| p.from == from && p.name == name) {
        bail!("there's no preset called {name}");
    }
    match from {
        "server" => {
            let mut all = parse_server(&try_read(paths, SERVER)?);
            all.retain(|(n, _)| n != name);
            write(paths, SERVER, &write_server(&all))
        }
        // an empty list drops the line
        "modManager" => write(paths, MOD_MANAGER, &set_line(&try_read(paths, MOD_MANAGER)?, name, &[], false)),
        _ => write(paths, GAME, &set_line(&try_read(paths, GAME)?, name, &[], true)),
    }
}

// --- favourites and hidden mods ---

/// The `["Id"] = {` blocks of [B42] Mod Manager's list, and where each `hidden =` line is.
fn b42_hidden_lines(text: &str) -> Vec<(String, usize, bool)> {
    let mut out = vec![];
    let mut current: Option<String> = None;
    for (n, l) in text.lines().enumerate() {
        let l = l.trim();
        if let Some(id) = l.strip_prefix("[\"").and_then(|r| r.split_once("\"]")).map(|(id, _)| id) {
            current = Some(id.to_string());
        } else if let (Some(v), Some(id)) = (l.strip_prefix("hidden ="), &current) {
            out.push((id.trim_start_matches('\\').to_string(), n, v.trim().trim_end_matches(',').trim() == "true"));
        }
    }
    out
}

/// Mods the player starred: in the game's mod menu (and [B42] Mod Manager) or in Mod Manager.
pub fn favourites(paths: &Paths) -> Vec<String> {
    let mut out = parse_lines(&read(paths, GAME)).into_iter().find(|(n, _)| n == GAME_FAVOURITES).map(|l| l.1).unwrap_or_default();
    out.extend(parse_lines(&read(paths, MOD_MANAGER)).into_iter().find(|(n, _)| n == FAVOURITES).map(|l| l.1).unwrap_or_default());
    out.sort();
    out.dedup();
    out
}

/// Mods the player hid: in Mod Manager or [B42] Mod Manager.
pub fn hidden(paths: &Paths) -> Vec<String> {
    let mut out: Vec<String> = b42_hidden_lines(&read(paths, B42_MANAGER)).into_iter().filter(|h| h.2).map(|h| h.0).collect();
    out.extend(parse_lines(&read(paths, MOD_MANAGER)).into_iter().find(|(n, _)| n == HIDDEN).map(|l| l.1).unwrap_or_default());
    out.sort();
    out.dedup();
    out
}

/// `list` with `id` added or taken out.
fn toggled(mut list: Vec<String>, id: &str, on: bool) -> Vec<String> {
    list.retain(|i| i != id);
    if on {
        list.push(id.to_string());
    }
    list
}

/// Star or unstar a mod: in the game's list (which [B42] Mod Manager uses too), and in Mod
/// Manager's when it keeps one.
pub fn set_favourite(paths: &Paths, id: &str, on: bool) -> Result<()> {
    let game = try_read(paths, GAME)?;
    let list = parse_lines(&game).into_iter().find(|(n, _)| n == GAME_FAVOURITES).map(|l| l.1).unwrap_or_default();
    write(paths, GAME, &set_line(&game, GAME_FAVOURITES, &toggled(list, id, on), true))?;
    let mm = try_read(paths, MOD_MANAGER)?;
    if let Some((_, list)) = parse_lines(&mm).into_iter().find(|(n, _)| n == FAVOURITES) {
        write(paths, MOD_MANAGER, &set_line(&mm, FAVOURITES, &toggled(list, id, on), false))?;
    }
    Ok(())
}

/// When the favourites last changed (the game's list's write, in ms; 0: never), for `app.txt`'s `fav_rev`.
pub fn favourites_rev(paths: &Paths) -> u128 {
    rev(paths, GAME)
}

/// When the sorting rules (pins) last changed, in ms (0: no file), for `app.txt`'s `rules_rev`.
pub fn rules_rev(paths: &Paths) -> u128 {
    rev(paths, RULES)
}

fn rev(paths: &Paths, name: &str) -> u128 {
    fs::metadata(file(paths, name)).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis())
}

/// Hide or show a mod: in Mod Manager's list (PZMM keeps its own there too), and in [B42] Mod
/// Manager's where it lists the mod.
pub fn set_hidden(paths: &Paths, id: &str, on: bool) -> Result<()> {
    let mm = try_read(paths, MOD_MANAGER)?;
    let list = parse_lines(&mm).into_iter().find(|(n, _)| n == HIDDEN).map(|l| l.1).unwrap_or_default();
    write(paths, MOD_MANAGER, &set_line(&mm, HIDDEN, &toggled(list, id, on), false))?;
    let b42 = try_read(paths, B42_MANAGER)?;
    if let Some(&(_, n, was)) = b42_hidden_lines(&b42).iter().find(|h| h.0 == id)
        && was != on
    {
        let lines: Vec<String> = b42.lines().enumerate().map(|(i, l)| if i == n { l.replace(&was.to_string(), &on.to_string()) } else { l.to_string() }).collect();
        write(paths, B42_MANAGER, &(lines.join("\r\n") + "\r\n"))?;
    }
    Ok(())
}

// --- Mod Load Order Sorter's rules ---

/// One mod's sorting rules. `load_first` / `load_last`: "on" (the whole list) or "category"
/// (its category), else empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    pub load_after: Vec<String>,
    pub load_before: Vec<String>,
    pub incompatible: Vec<String>,
    pub load_first: String,
    pub load_last: String,
    /// The Sorter's own category for the mod (coreRequirement, tweaks, resource, map, ...).
    pub category: String,
    /// The block's other `key=value` lines (keys PZMM doesn't use: the Sorter's or the game's), written back as they were.
    #[serde(skip)]
    pub other: Vec<String>,
}

/// A key [`Rule::add`] reads.
fn known_key(k: &str) -> bool {
    matches!(k.trim().to_ascii_lowercase().as_str(), "loadafter" | "loadmodafter" | "loadbefore" | "loadmodbefore" | "incompatiblemods" | "incompatible" | "loadfirst" | "loadlast" | "category")
}

/// A `loadFirst=` / `loadLast=` value as the Sorter reads it: true/0 mean "on".
pub fn position(v: &str) -> String {
    match v.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "0" => "on".into(),
        "category" => "category".into(),
        _ => String::new(),
    }
}

impl Rule {
    /// Add a mod.info's or a rules block's `key=value` (keys in any case), as the Sorter reads them.
    pub fn add(&mut self, key: &str, value: &str) {
        let merge = |list: &mut Vec<String>| {
            for id in ids(value) {
                if !list.contains(&id) {
                    list.push(id);
                }
            }
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "loadafter" | "loadmodafter" => merge(&mut self.load_after),
            "loadbefore" | "loadmodbefore" => merge(&mut self.load_before),
            "incompatiblemods" | "incompatible" => merge(&mut self.incompatible),
            "loadfirst" => self.load_first = position(value),
            "loadlast" => self.load_last = position(value),
            "category" => self.category = value.trim().to_string(),
            _ => {}
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Rule::default()
    }
}

fn parse_rules(text: &str) -> BTreeMap<String, Rule> {
    let mut out: BTreeMap<String, Rule> = BTreeMap::new();
    let mut current: Option<String> = None;
    for l in text.lines().map(str::trim) {
        if let Some(id) = l.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let id = id.trim().trim_start_matches('\\').to_string();
            out.entry(id.clone()).or_default();
            current = Some(id);
        } else if let (Some((k, v)), Some(id)) = (l.split_once('='), &current) {
            let r = out.entry(id.clone()).or_default();
            if known_key(k) { r.add(k, v) } else { r.other.push(l.to_string()) }
        }
    }
    out
}

fn write_rules(rules: &BTreeMap<String, Rule>) -> String {
    let mut out = String::new();
    for (id, r) in rules.iter().filter(|(_, r)| !r.is_empty()) {
        out += &format!("[{id}]\r\n");
        for (k, v) in [("loadAfter", &r.load_after), ("loadBefore", &r.load_before), ("incompatibleMods", &r.incompatible)] {
            if !v.is_empty() {
                out += &format!("{k}={}\r\n", v.join(","));
            }
        }
        for (k, v) in [("loadFirst", &r.load_first), ("loadLast", &r.load_last), ("category", &r.category)] {
            if !v.is_empty() {
                out += &format!("{k}={v}\r\n");
            }
        }
        for l in &r.other {
            out += &format!("{l}\r\n");
        }
    }
    out
}

/// The Sorter's rules file, by mod id (none without the file).
pub fn rules(paths: &Paths) -> BTreeMap<String, Rule> {
    parse_rules(&read(paths, RULES))
}

/// Set one mod's rule (an empty one removes it), keeping the others.
pub fn set_rule(paths: &Paths, id: &str, rule: Rule) -> Result<()> {
    let mut all = parse_rules(&try_read(paths, RULES)?);
    let id = id.trim_start_matches('\\').to_string();
    // the block's keys PZMM doesn't edit stay (the UI never sees them)
    let other = all.get(&id).map(|r| r.other.clone()).unwrap_or_default();
    all.insert(id, Rule { other, ..rule });
    write(paths, RULES, &write_rules(&all))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn presets_round_trip_in_each_format() {
        let (_t, paths) = crate::store::tests::temp_paths();
        // as the Workshop mods and the game write them
        fs::create_dir_all(file(&paths, "ModManager")).unwrap();
        fs::write(file(&paths, MOD_MANAGER), "VERSION=2\r\nFriends:A;B;C\nmmFavorites:B\nmmHidden:X\n").unwrap();
        fs::write(file(&paths, GAME), "!fav!:\\C;D;\nSolo:\\A;D;\nOld. one:\\A;\n").unwrap();
        fs::write(file(&paths, SERVER), "VERSION=1\r\n[Public]\r\nWorkshopItems=111;222\r\nMods=\\A;B\r\n").unwrap();
        let all = presets(&paths);
        let want = |from: &str, name: &str, mods: &[&str], ws: &[&str]| Preset { from: from.into(), name: name.into(), mods: s(mods), workshop: s(ws) };
        assert_eq!(all, [want("modManager", "Friends", &["A", "B", "C"], &[]), want("game", "Solo", &["A", "D"], &[]), want("game", "Old. one", &["A"], &[]), want("server", "Public", &["A", "B"], &["111", "222"])]);

        // written back: replaced in place, new ones appended, the rest untouched
        save_preset(&paths, &want("modManager", "Friends", &["C", "A"], &[])).unwrap();
        save_preset(&paths, &want("game", "Coop", &["E"], &[])).unwrap();
        save_preset(&paths, &want("server", "Public", &["A"], &["111"])).unwrap();
        save_preset(&paths, &want("server", "Test", &["Z"], &["9"])).unwrap();
        assert_eq!(fs::read_to_string(file(&paths, MOD_MANAGER)).unwrap(), "VERSION=2\nFriends:C;A\nmmFavorites:B\nmmHidden:X\n");
        assert_eq!(fs::read_to_string(file(&paths, GAME)).unwrap(), "!fav!:\\C;D;\nSolo:\\A;D;\nOld. one:\\A;\nCoop:\\E;\n", "ids as the file has them");
        assert_eq!(
            fs::read_to_string(file(&paths, SERVER)).unwrap(),
            "VERSION=1\r\n[Public]\r\nWorkshopItems=111\r\nMods=A\r\n[Test]\r\nWorkshopItems=9\r\nMods=Z\r\n"
        );
        assert!(file(&paths, MOD_MANAGER).with_extension("txt.bak").exists(), "the original is kept");
        assert_eq!(presets(&paths).len(), 6);
        assert!(save_preset(&paths, &want("game", "a:b", &["A"], &[])).is_err());
        assert!(save_preset(&paths, &want("modManager", HIDDEN, &["A"], &[])).is_err());

        // deleted from its own file only
        delete_preset(&paths, "game", "Solo").unwrap();
        delete_preset(&paths, "server", "Public").unwrap();
        // a name PZMM wouldn't save, as the game saved it; nothing that isn't exactly a preset there
        delete_preset(&paths, "game", "Old. one").unwrap();
        assert!(delete_preset(&paths, "game", "old. one").is_err() && delete_preset(&paths, "game", "!fav!").is_err() && delete_preset(&paths, "modManager", "mmHidden").is_err());
        assert_eq!(fs::read_to_string(file(&paths, GAME)).unwrap(), "!fav!:\\C;D;\nCoop:\\E;\n");
        assert_eq!(fs::read_to_string(file(&paths, SERVER)).unwrap(), "VERSION=1\r\n[Test]\r\nWorkshopItems=9\r\nMods=Z\r\n");
        assert_eq!(presets(&paths).len(), 3);
    }

    #[test]
    fn favourites_and_hidden_from_every_manager() {
        let (_t, paths) = crate::store::tests::temp_paths();
        fs::create_dir_all(file(&paths, "ModManager")).unwrap();
        fs::write(file(&paths, MOD_MANAGER), "VERSION=2\r\nmmFavorites:B\nmmHidden:X\n").unwrap();
        fs::write(file(&paths, GAME), "!fav!:\\C;\nSolo:A;\n").unwrap();
        let b42 = "return {\r\n    version = 2,\r\n    mods = {\r\n        [\"Y\"] = {\r\n            index = 1,\r\n            category = nil,\r\n            hidden = true,\r\n        },\r\n        [\"Z\"] = {\r\n            index = 2,\r\n            category = \"Mine\",\r\n            hidden = false,\r\n        },\r\n    },\r\n}\r\n";
        fs::write(file(&paths, B42_MANAGER), b42).unwrap();
        assert_eq!((favourites(&paths), hidden(&paths)), (s(&["B", "C"]), s(&["X", "Y"])));

        set_favourite(&paths, "C", false).unwrap();
        set_favourite(&paths, "D", true).unwrap();
        set_hidden(&paths, "Y", false).unwrap();
        set_hidden(&paths, "Z", true).unwrap();
        assert_eq!((favourites(&paths), hidden(&paths)), (s(&["B", "D"]), s(&["X", "Z"])));
        assert_eq!(fs::read_to_string(file(&paths, GAME)).unwrap(), "!fav!:D;\nSolo:A;\n", "presets stay");
        assert_eq!(fs::read_to_string(file(&paths, MOD_MANAGER)).unwrap(), "VERSION=2\nmmFavorites:B;D\nmmHidden:X;Z\n");
        assert!(fs::read_to_string(file(&paths, B42_MANAGER)).unwrap().contains("index = 1,\r\n            category = nil,\r\n            hidden = false,"));

        // no files yet: the game's favourites line is made first
        let (_t, paths) = crate::store::tests::temp_paths();
        set_favourite(&paths, "A", true).unwrap();
        assert_eq!(fs::read_to_string(file(&paths, GAME)).unwrap(), "!fav!:A;\n");
        // one that can't be read isn't written over as if empty
        fs::create_dir_all(file(&paths, MOD_MANAGER)).unwrap();
        assert!(set_hidden(&paths, "A", true).is_err());
    }

    #[test]
    fn sorting_rules_round_trip() {
        let (_t, paths) = crate::store::tests::temp_paths();
        fs::create_dir_all(file(&paths, "")).unwrap();
        let text = "[\\Core]\r\nloadFirst=true\r\n[Patch]\r\nloadAfter=\\Core, Big\r\nloadModBefore=Late\r\nincompatibleMods=Old\r\nloadLast=category\r\ncategory=tweaks\r\nweird=1\r\n";
        fs::write(file(&paths, RULES), text).unwrap();
        let r = rules(&paths);
        assert_eq!(r["Core"], Rule { load_first: "on".into(), ..Default::default() });
        assert_eq!(
            r["Patch"],
            Rule { load_after: s(&["Core", "Big"]), load_before: s(&["Late"]), incompatible: s(&["Old"]), load_last: "category".into(), category: "tweaks".into(), other: s(&["weird=1"]), ..Default::default() }
        );
        set_rule(&paths, "New", Rule { load_after: s(&["Patch"]), ..Default::default() }).unwrap();
        set_rule(&paths, "Core", Rule::default()).unwrap();
        assert_eq!(
            fs::read_to_string(file(&paths, RULES)).unwrap(),
            "[New]\r\nloadAfter=Patch\r\n[Patch]\r\nloadAfter=Core,Big\r\nloadBefore=Late\r\nincompatibleMods=Old\r\nloadLast=category\r\ncategory=tweaks\r\nweird=1\r\n"
        );
        assert_eq!(rules(&paths)["Patch"], r["Patch"], "what PZMM writes reads back the same");
        // the game edits it too (LF, any key order, keys PZMM doesn't know): those keys survive PZMM's write
        write(&paths, RULES, "[Kept]\nsomeFutureKey=1\nloadAfter=Core\n").unwrap();
        set_rule(&paths, "Kept", Rule { load_after: s(&["Core", "Big"]), ..Default::default() }).unwrap();
        let r = rules(&paths);
        assert_eq!((r["Kept"].load_after.clone(), r["Kept"].other.clone()), (s(&["Core", "Big"]), s(&["someFutureKey=1"])));
    }
}
