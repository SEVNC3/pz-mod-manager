//! Dedicated server configs: `Zomboid/Server/<name>.ini` (`Mods=`, `WorkshopItems=`, `Map=`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::Paths;

pub fn dir(paths: &Paths) -> PathBuf {
    paths.zomboid.join("Server")
}

/// Server config names (the part before `.ini`): the Zomboid folder's, and those of servers PZMM
/// manages that keep their data in a folder of their own (`-cachedir`).
pub fn list(paths: &Paths) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir(paths))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".ini").map(String::from))
        .collect();
    names.extend(super::host::load(paths).into_iter().filter(|s| s.ini(paths).is_file()).map(|s| s.name));
    names.sort();
    names.dedup();
    names
}

/// The build server `name` runs: its own when PZMM manages it, else the one you play.
pub fn build(paths: &Paths, name: &str) -> u8 {
    super::host::find(paths, name).map_or_else(|| crate::profile::effective_build(paths), |s| s.build_number())
}

pub fn path(paths: &Paths, name: &str) -> Result<PathBuf> {
    // beyond file-name rules, nothing a shell would treat specially (the name reaches the launcher's command line)
    if !crate::safe_file_name(name) || name.starts_with('.') || name.contains(['&', '^', '%', '!', '(', ')', ';', ',', '=', '`', '$']) {
        bail!("invalid server name: {name:?}");
    }
    // a server PZMM manages may keep its data in its own folder (-cachedir); one run elsewhere has PZMM's copy of its files
    let dir = match super::host::find(paths, name) {
        Some(s) => s.cachedir(paths).join("Server"),
        None if super::host::remote(paths, name).is_some() => super::ftp::mirror(paths, name).join("Server"),
        None => dir(paths),
    };
    Ok(dir.join(format!("{name}.ini")))
}

/// What a server config loads.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServerMods {
    pub mods: Vec<String>,
    pub workshop: Vec<String>,
    pub maps: Vec<String>,
}

/// The game's own map, which mod maps must come before.
pub const BASE_MAP: &str = "Muldraugh, KY";

/// `folder` is `BASE_MAP`, in any case and with spaces around it (Windows and the game don't mind).
pub fn is_base_map(folder: &str) -> bool {
    folder.trim().eq_ignore_ascii_case(BASE_MAP)
}

/// The value of a `key=value` line ("" when there's none).
pub fn value<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).unwrap_or("").trim()
}

/// The entries of a `;` list (`Mods=`, `WorkshopItems=`, `Map=`), without B42's `\` and blanks.
pub(crate) fn split(v: &str) -> Vec<String> {
    v.split(';').map(|s| s.trim().trim_start_matches('\\').trim().to_string()).filter(|s| !s.is_empty()).collect()
}

pub fn read(path: &Path) -> Result<ServerMods> {
    let text = fs::read_to_string(path)?;
    let workshop = split(value(&text, "WorkshopItems")).into_iter().filter(|w| crate::sources::steam_workshop::valid_id(w)).collect();
    Ok(ServerMods { mods: split(value(&text, "Mods")), workshop, maps: split(value(&text, "Map")) })
}

/// The `Map=` list for `mod_maps` (the enabled mods' maps): maps, and the game's own ones,
/// keep their place in `order` (the user's arrangement or the config's current line), new
/// ones go before the first of the game's maps, and Muldraugh always goes last. Empty when
/// no mod adds a map.
pub fn map_order(order: &[String], mod_maps: &[String]) -> Vec<String> {
    if mod_maps.is_empty() {
        return vec![];
    }
    let mut out: Vec<String> = vec![];
    for m in order.iter().filter(|m| !is_base_map(m) && (mod_maps.contains(m) || super::maps::is_vanilla(m))) {
        if !out.contains(m) {
            out.push(m.clone());
        }
    }
    let at = out.iter().position(|m| super::maps::is_vanilla(m)).unwrap_or(out.len());
    let mut new: Vec<String> = vec![];
    for m in mod_maps.iter().filter(|m| !out.contains(m) && !is_base_map(m)) {
        if !new.contains(m) {
            new.push(m.clone());
        }
    }
    out.splice(at..at, new);
    out.push(BASE_MAP.into());
    out
}

/// `arranged`, with the entries of `saved` it lacks put back after the entry they followed:
/// maps that left the list keep their place for when they come back.
pub fn merge_order(saved: &[String], arranged: &[String]) -> Vec<String> {
    let mut out = arranged.to_vec();
    let mut at = 0;
    for s in saved {
        match out.iter().position(|o| o == s) {
            Some(p) => at = p + 1,
            None => {
                out.insert(at, s.clone());
                at += 1;
            }
        }
    }
    out
}

/// The maps the enabled mods add on `build` (the folders the game reads, see `maps::folder_names`), in load order.
pub fn mod_maps(enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, build: u8) -> Vec<String> {
    enabled.iter().filter_map(|id| mods.get(id)).flat_map(|m| super::maps::folder_names(&m.dir, build)).collect()
}

/// Set `Mods=`, `WorkshopItems=` and, when `maps` isn't empty, `Map=`. Every other line is
/// kept. The file as it was before PZMM's first write is kept as `.ini.bak`.
pub fn write(path: &Path, mods: &[String], workshop: &[String], maps: &[String], b41: bool) -> Result<()> {
    set_values(path, &line_values(mods, workshop, maps, b41))
}

/// The `Mods=`, `WorkshopItems=` and (when `maps` isn't empty) `Map=` values, as (key, value).
/// Build 42 puts `\` before each mod ID.
pub fn line_values(mods: &[String], workshop: &[String], maps: &[String], b41: bool) -> Vec<(&'static str, String)> {
    let prefix = if b41 { "" } else { "\\" };
    let mut set = vec![("Mods", mods.iter().map(|m| format!("{prefix}{m}")).collect::<Vec<_>>().join(";")), ("WorkshopItems", workshop.join(";"))];
    if !maps.is_empty() {
        set.push(("Map", maps.join(";")));
    }
    set
}

/// Set `key=value` lines (added at the end when missing); every other line is kept, and the
/// file as it was before PZMM's first write is kept as `.ini.bak`.
pub fn set_values(path: &Path, set: &[(&str, String)]) -> Result<()> {
    // a line break would add lines of its own
    if let Some((key, _)) = set.iter().find(|(_, v)| v.contains(['\n', '\r'])) {
        bail!("{key} must be one line");
    }
    // only a missing file starts empty: rewriting one we couldn't read would wipe every other setting
    let old = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {} (nothing was changed)", path.display())),
    };
    crate::backup_once(path)?;
    let mut lines: Vec<String> = old.lines().map(String::from).collect();
    for (key, val) in set {
        let line = format!("{key}={val}");
        match lines.iter_mut().find(|l| l.split_once('=').is_some_and(|(k, _)| k == *key)) {
            Some(l) => *l = line,
            None => lines.push(line),
        }
    }
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    crate::write_atomic(path, lines.join("\n") + "\n")?;
    Ok(())
}
/// Write the enabled mods (in load order) to `Server/<name>.ini`: installed mods go in
/// `Mods=`, their Workshop items in `WorkshopItems=`, their maps in `Map=` (kept as
/// `arranged`, else as the config has them: see [`map_order`]). Returns notes for the user
/// (the summary first, then anything the server won't get automatically).
pub fn export(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, arranged: Option<&[String]>) -> Result<Vec<String>> {
    let (file, w) = plan(paths, name, enabled, mods, arranged)?;
    write(&file, &w.ids, &w.workshop, &w.maps, w.build == 41)?;
    let mut notes = w.notes;
    notes.insert(0, format!("Wrote {} mods and {} Workshop items to {}", w.ids.len(), w.workshop.len(), file.display()));
    Ok(notes)
}

/// What `export` would change in `Server/<name>.ini`, for a confirmation first.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Preview {
    /// The file doesn't exist yet.
    pub new: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// `Map=` now and after (the same when it stays as it is).
    pub maps_from: Vec<String>,
    pub maps_to: Vec<String>,
    /// `WorkshopItems=` changes.
    pub items_added: Vec<String>,
    pub items_removed: Vec<String>,
}

pub fn preview(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, arranged: Option<&[String]>) -> Result<Preview> {
    preview_lines(&path(paths, name)?, &planned(paths, name, enabled, mods, arranged)?)
}

/// The lines `export` would write (to check them first).
pub fn planned(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, arranged: Option<&[String]>) -> Result<ServerMods> {
    let (_, w) = plan(paths, name, enabled, mods, arranged)?;
    Ok(ServerMods { mods: w.ids, workshop: w.workshop, maps: w.maps })
}

/// What writing `to` (an empty `maps` keeps `Map=`) would change in `file`.
pub fn preview_lines(file: &Path, to: &ServerMods) -> Result<Preview> {
    let now = if file.exists() { read(file).with_context(|| format!("reading {}", file.display()))? } else { ServerMods::default() };
    let minus = |a: &[String], b: &[String]| a.iter().filter(|i| !b.contains(i)).cloned().collect::<Vec<_>>();
    Ok(Preview {
        new: !file.exists(),
        added: minus(&to.mods, &now.mods),
        removed: minus(&now.mods, &to.mods),
        items_added: minus(&to.workshop, &now.workshop),
        items_removed: minus(&now.workshop, &to.workshop),
        maps_to: if to.maps.is_empty() { now.maps.clone() } else { to.maps.clone() },
        maps_from: now.maps,
    })
}

/// A problem with a server's mod lines (see [`check`]): `key` is `hosting.check.<key>` in the
/// app's locale files, `text` the English. Errors stop the write: a bad Workshop ID crashes the server.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Problem {
    pub error: bool,
    pub key: &'static str,
    pub id: String,
    /// The Workshop item `id` comes from (`itemMissing` only).
    pub item: String,
    pub text: String,
}

/// `installed`, plus the mods a dedicated server downloaded itself (into its install folder's
/// `steamapps/workshop/content/108600`) that aren't installed here: [`check`] then knows their
/// mod.info's requirements too.
pub fn with_downloads(installed: &std::collections::BTreeMap<String, crate::store::Row>, install_dir: &Path) -> std::collections::BTreeMap<String, crate::store::Row> {
    let mut out = installed.clone();
    for r in crate::store::workshop_rows(&install_dir.join("steamapps").join("workshop").join("content").join(super::STEAM_APP)) {
        out.entry(r.id.clone()).or_insert(r);
    }
    out
}

/// Check `lines` before writing them: every Workshop ID is a number and (when `details` could
/// be fetched) exists on the Workshop; every mod ID comes from a listed Workshop item, by the
/// installed mods' sources or the items' descriptions ("Mod ID: X"), else it's flagged unknown;
/// and each installed mod's requirements are listed before it, its incompatible mods not at all.
pub fn check(lines: &ServerMods, installed: &std::collections::BTreeMap<String, crate::store::Row>, details: Option<&[crate::sources::steam_workshop::Item]>) -> Vec<Problem> {
    let mut out = vec![];
    let mut say = |error, key, id: &str, item: &str, text: String| out.push(Problem { error, key, id: id.to_string(), item: item.to_string(), text });
    for w in &lines.workshop {
        if !crate::sources::steam_workshop::valid_id(w) {
            say(true, "badId", w, "", format!("{w} isn't a Workshop ID (only digits)"));
        } else if details.is_some_and(|d| !d.iter().any(|i| &i.id == w)) {
            say(true, "notOnWorkshop", w, "", format!("Workshop item {w} doesn't exist or isn't public: the server would crash downloading it"));
        }
    }
    if details.is_none() && !lines.workshop.is_empty() {
        say(false, "unchecked", "", "", "Couldn't reach the Workshop to check the IDs exist".into());
    }
    for m in &lines.mods {
        match installed.get(m).map(|r| crate::sources::workshop_item(&r.source)) {
            Some(Some(item)) if !lines.workshop.iter().any(|w| w == item) => {
                say(false, "itemMissing", m, item, format!("{m} comes from Workshop item {item}, which isn't in WorkshopItems"))
            }
            Some(Some(_)) => {}
            Some(None) => say(false, "notWorkshop", m, "", format!("{m} isn't from the Workshop: copy its folder into the server's mods folder yourself")),
            None if details.into_iter().flatten().any(|i| super::workshop_mod_ids(&i.description).contains(m)) => {}
            None => say(false, "unknownMod", m, "", format!("{m} isn't installed here and no listed Workshop item names it: check the Mod ID")),
        }
    }
    let at = |id: &str| lines.mods.iter().position(|m| m == id);
    for (i, m) in lines.mods.iter().enumerate() {
        let Some(r) = installed.get(m) else { continue };
        for req in &r.requires {
            match at(req) {
                None => say(false, "needs", m, req, format!("{m} needs {req}, which isn't in Mods")),
                Some(j) if j > i => say(false, "needsLater", m, req, format!("{m} needs {req}, which comes after it in Mods")),
                Some(_) => {}
            }
        }
        for x in r.incompatible.iter().filter(|x| at(x).is_some()) {
            say(false, "incompatible", m, x, format!("{m} is incompatible with {x}, which is also in Mods"));
        }
    }
    out
}

/// A kept mod's requirement that the server's leave-out list holds says so ("needsLeftOut"), not just "needs".
pub fn mark_left_out(problems: &mut [Problem], leave: &[String]) {
    for p in problems.iter_mut().filter(|p| p.key == "needs" && leave.contains(&p.item)) {
        p.key = "needsLeftOut";
        p.text = format!("{} needs {}, which you left out for this server", p.id, p.item);
    }
}

/// The lines `export` writes, and notes about mods the server won't get.
struct Written {
    build: u8,
    ids: Vec<String>,
    workshop: Vec<String>,
    maps: Vec<String>,
    notes: Vec<String>,
}

fn plan(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, arranged: Option<&[String]>) -> Result<(PathBuf, Written)> {
    let build = build(paths, name);
    let file = path(paths, name)?;
    let current = read(&file).map(|c| c.maps).unwrap_or_default();
    let (lines, notes) = lines_for(enabled, mods, build, arranged.unwrap_or(&current), &super::leave_out::get(paths, name));
    Ok((file, Written { build, ids: lines.mods, workshop: lines.workshop, maps: lines.maps, notes }))
}

/// A server's lines for the `enabled` mods (in load order): installed mods go in `Mods=`, their
/// Workshop items in `WorkshopItems=`, their maps (on `build`, arranged as in `order`, see
/// [`map_order`]) in `Map=`. Mods in `leave` (the server's leave-out list) aren't sent at all: their Workshop item
/// goes only when no mod sent comes from it, their maps with them. Also notes about mods the server won't get.
pub fn lines_for(enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, build: u8, order: &[String], leave: &[String]) -> (ServerMods, Vec<String>) {
    let (mut ids, mut workshop, mut notes) = (vec![], vec![], vec![]);
    for id in enabled {
        if leave.contains(id) {
            notes.push(format!("{id} was left out for this server"));
            continue;
        }
        let Some(m) = mods.get(id) else {
            notes.push(format!("{id} isn't installed, so it was left out"));
            continue;
        };
        ids.push(id.clone());
        match crate::sources::workshop_item(&m.source) {
            Some(item) => {
                if !workshop.iter().any(|w| w == item) {
                    workshop.push(item.to_string());
                }
            }
            None => notes.push(format!("{id} isn't from the Workshop: copy its folder into the server's mods folder yourself")),
        }
    }
    let mut maps = map_order(order, &mod_maps(&ids, mods, build));
    // only left-out mods brought maps: Map= is written back to the game's own (empty would keep their maps there)
    if maps.is_empty() && !mod_maps(leave, mods, build).is_empty() {
        maps = vec![BASE_MAP.to_string()];
    }
    (ServerMods { mods: ids, workshop, maps }, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_left_out_mod_and_only_its_own_item_and_maps_go() {
        let row = |id: &str, item: &str| crate::store::Row { id: id.into(), source: format!("workshop:{item}"), requires: if id == "Kept" { vec!["UI".into()] } else { vec![] }, ..Default::default() };
        let mods: std::collections::BTreeMap<String, crate::store::Row> = [row("Kept", "1"), row("Sibling", "1"), row("UI", "2"), row("Alone", "3")].into_iter().map(|r| (r.id.clone(), r)).collect();
        let on = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (l, notes) = lines_for(&on(&["Kept", "Sibling", "UI", "Alone"]), &mods, 42, &[], &on(&["Sibling", "UI", "Alone"]));
        assert_eq!(l.mods, ["Kept"]);
        assert_eq!(l.workshop, ["1"], "item 1 stays for Kept; 2 and 3 had only left-out mods");
        assert!(notes.iter().any(|n| n == "UI was left out for this server"));
        let mut problems = check(&l, &mods, None);
        mark_left_out(&mut problems, &on(&["UI"]));
        let p = problems.iter().find(|p| p.id == "Kept").unwrap();
        assert_eq!((p.key, p.text.as_str()), ("needsLeftOut", "Kept needs UI, which you left out for this server"));
        // nothing left out: everything as before
        assert_eq!(lines_for(&on(&["Kept", "UI"]), &mods, 42, &[], &[]).0.mods, ["Kept", "UI"]);
    }

    /// tests/fixtures/server-leave-out.json, which the game replays too: lines_for, the check and the label.
    #[test]
    fn the_shared_leave_out_cases() {
        let fx: serde_json::Value = serde_json::from_str(&fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/server-leave-out.json")).unwrap()).unwrap();
        let strs = |v: &serde_json::Value| v.as_array().map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
        for c in fx["cases"].as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            let t = tempfile::tempdir().unwrap();
            let requires = |id: &str| strs(&c["requires"][id]);
            let mut mods = std::collections::BTreeMap::new();
            for id in strs(&c["order"]) {
                let dir = t.path().join(&id);
                for m in strs(&c["mapsOf"][id.as_str()]) {
                    fs::create_dir_all(dir.join("42/media/maps").join(m)).unwrap();
                }
                fs::create_dir_all(&dir).unwrap();
                let source = c["workshop"][id.as_str()].as_str().map_or("manual".to_string(), |w| format!("workshop:{w}"));
                mods.insert(id.clone(), crate::store::Row { id: id.clone(), dir, source, requires: requires(&id), ..Default::default() });
            }
            let left = strs(&c["left"]);
            let (l, _) = lines_for(&strs(&c["order"]), &mods, 42, &strs(&c["maps"]), &left);
            let e = &c["expect"];
            assert_eq!((&l.mods, &l.workshop, &l.maps), (&strs(&e["mods"]), &strs(&e["items"]), &strs(&e["maps"])), "{name}");
            let mut problems = check(&l, &mods, None);
            mark_left_out(&mut problems, &left);
            let got: Vec<Vec<String>> = problems.iter().filter(|p| p.key == "needsLeftOut").map(|p| vec![p.id.clone(), p.item.clone()]).collect();
            let want: Vec<Vec<String>> = e["needsLeftOut"].as_array().unwrap().iter().map(strs).collect();
            assert_eq!(got, want, "{name}: needsLeftOut");
            let side: Vec<String> = strs(&c["order"]).into_iter().filter(|id| super::super::leave_out::client_side(id, &|r: &str| requires(r))).collect();
            assert_eq!(side, strs(&e["clientSide"]), "{name}: clientSide");
        }
    }

    #[test]
    fn round_trip_keeps_other_settings() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("servertest.ini");
        fs::write(&p, "PVP=true\nMods=Old\nWorkshopItems=1\nMap=Muldraugh, KY\nPublic=false\n").unwrap();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        write(&p, &ids(&["A", "B"]), &ids(&["111", "222"]), &ids(&["Riverside Plus", BASE_MAP]), false).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert_eq!(text, "PVP=true\nMods=\\A;\\B\nWorkshopItems=111;222\nMap=Riverside Plus;Muldraugh, KY\nPublic=false\n");
        assert_eq!(
            read(&p).unwrap(),
            ServerMods { mods: ids(&["A", "B"]), workshop: ids(&["111", "222"]), maps: ids(&["Riverside Plus", BASE_MAP]) }
        );
        assert!(p.with_file_name("servertest.ini.bak").exists());

        write(&p, &ids(&["A"]), &[], &[], true).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("Mods=A\n") && text.contains("Map=Riverside Plus;Muldraugh, KY"), "no maps: Map= untouched");
    }

    #[test]
    fn map_order_keeps_the_arrangement() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(map_order(&ids(&["Old", BASE_MAP]), &[]), Vec::<String>::new(), "no mod maps: Map= untouched");
        // B stays first, the gone map drops, the new C goes before Muldraugh
        assert_eq!(map_order(&ids(&["B", "Gone", "A", BASE_MAP]), &ids(&["A", "B", "C", "A"])), ids(&["B", "A", "C", BASE_MAP]));
        assert_eq!(map_order(&[], &ids(&["A"])), ids(&["A", BASE_MAP]));
        assert_eq!(map_order(&ids(&[" muldraugh, ky", "A"]), &ids(&["A"])), ids(&["A", BASE_MAP]), "any case: still the base map, still last");
        assert!(is_base_map("MULDRAUGH, KY ") && !is_base_map("Muldraugh") && !is_base_map("Muldraugh,KY"));
        // Muldraugh is pinned last; the game's other maps keep their place, new mod maps go before them
        assert_eq!(map_order(&ids(&[BASE_MAP, "A", "Riverside, KY", "Gone"]), &ids(&["A", "C"])), ids(&["A", "C", "Riverside, KY", BASE_MAP]));
    }

    #[test]
    fn merge_keeps_maps_that_left() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // X was hidden, then A and B swapped: X stays after A, Y stays first
        assert_eq!(merge_order(&ids(&["Y", "A", "X", "B", BASE_MAP]), &ids(&["B", "A", BASE_MAP])), ids(&["Y", "B", "A", "X", BASE_MAP]));
        // so when X comes back it's where it was, not just before Muldraugh
        let saved = merge_order(&ids(&["A", "X", "B", BASE_MAP]), &ids(&["A", "B", BASE_MAP]));
        assert_eq!(map_order(&saved, &ids(&["A", "B", "X"])), ids(&["A", "X", "B", BASE_MAP]));
        assert_eq!(merge_order(&[], &ids(&["A"])), ids(&["A"]));
    }

    #[test]
    fn server_maps_follow_the_build() {
        // a B41 map at the mod root; B42 maps in common/ and the newest version folder (42.9 is older)
        let t = tempfile::tempdir().unwrap();
        let m = t.path().join("Town");
        for d in ["media/maps/Old Town", "common/media/maps/Town", "42.13/media/maps/Town Extra", "42.9/media/maps/Gone"] {
            fs::create_dir_all(m.join(d)).unwrap();
        }
        let row = crate::store::Row {
            id: "Town".into(),
            name: String::new(),
            source: "manual".into(),
            dir: m,
            info: PathBuf::new(),
            requires: vec![],
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: true,
            supports_42: true,
            ..Default::default()
        };
        let mods = [("Town".to_string(), row)].into();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(mod_maps(&ids(&["Town"]), &mods, 41), ids(&["Old Town"]));
        assert_eq!(mod_maps(&ids(&["Town"]), &mods, 42), ids(&["Town", "Town Extra"]));
    }

    #[test]
    fn preview_lists_what_writing_changes() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let row = |source: &str| crate::store::Row {
            id: String::new(),
            name: String::new(),
            source: source.into(),
            dir: PathBuf::new(),
            info: PathBuf::new(),
            requires: vec![],
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: true,
            supports_42: true,
            ..Default::default()
        };
        let mods = [("A".to_string(), row("steam:1")), ("B".to_string(), row("steam:2"))].into();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(preview(&paths, "srv", &ids(&["A"]), &mods, None).unwrap(), Preview { new: true, added: ids(&["A"]), items_added: ids(&["1"]), ..Default::default() });
        assert!(!path(&paths, "srv").unwrap().exists(), "a preview writes nothing");
        export(&paths, "srv", &ids(&["A"]), &mods, None).unwrap();
        let p = preview(&paths, "srv", &ids(&["B"]), &mods, None).unwrap();
        assert_eq!((p.new, p.added, p.removed), (false, ids(&["B"]), ids(&["A"])));
        assert_eq!((p.items_added, p.items_removed), (ids(&["2"]), ids(&["1"])));
    }

    #[test]
    fn unreadable_ini_is_left_alone() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("servertest.ini");
        let ansi = b"PVP=true\nPublicDescription=Caf\xe9\nMods=Old\n"; // saved as ANSI: not UTF-8
        fs::write(&p, ansi).unwrap();
        assert!(write(&p, &["A".into()], &[], &[], false).is_err());
        assert_eq!(fs::read(&p).unwrap(), ansi, "settings kept");
    }

    #[test]
    fn server_names_carry_no_shell_syntax() {
        let paths = crate::pz::Paths { zomboid: "Z".into(), data: "D".into() };
        assert!(path(&paths, "servertest").is_ok());
        assert!(path(&paths, "My Server").is_ok());
        for bad in ["R&D", "x&calc", "a^b", "%PATH%", "x(1)", "..", "a/b", ""] {
            assert!(path(&paths, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn checks_mod_and_workshop_ids_before_writing() {
        use crate::sources::steam_workshop::Item;
        let row = |source: &str| crate::store::Row {
            id: String::new(),
            name: String::new(),
            source: source.into(),
            dir: PathBuf::new(),
            info: PathBuf::new(),
            requires: vec![],
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: true,
            supports_42: true,
            ..Default::default()
        };
        let installed = [("A".to_string(), row("steam:111")), ("Local".to_string(), row("local:x"))].into();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let lines = ServerMods { mods: ids(&["A", "B", "Ghost", "Local"]), workshop: ids(&["222", "333", "12x"]), maps: vec![] };
        let item = |id: &str, desc: &str| Item { id: id.into(), description: desc.into(), ..Default::default() };
        // 222 says it provides B; 333 isn't on the Workshop
        let details = [item("222", "[b]Mod ID:[/b] B")];
        let got: Vec<(bool, &str, String)> = check(&lines, &installed, Some(&details)).into_iter().map(|p| (p.error, p.key, p.id)).collect();
        assert_eq!(
            got,
            [
                (true, "notOnWorkshop", "333".into()),
                (true, "badId", "12x".into()),
                (false, "itemMissing", "A".into()),
                (false, "unknownMod", "Ghost".into()),
                (false, "notWorkshop", "Local".into()),
            ]
        );
        let offline: Vec<&str> = check(&lines, &installed, None).into_iter().map(|p| p.key).collect();
        assert!(offline.contains(&"unchecked") && !offline.contains(&"notOnWorkshop"), "{offline:?}");
        let fine = ServerMods { mods: ids(&["A"]), workshop: ids(&["111"]), maps: vec![] };
        assert!(check(&fine, &installed, Some(&[item("111", "")])).is_empty());
        // what the mods themselves say: requirements first, no incompatible pairs
        let needs = |requires: &[&str], incompatible: &[&str]| crate::store::Row { requires: ids(requires), incompatible: ids(incompatible), ..row("steam:111") };
        let installed = [("P".to_string(), needs(&["Lib", "Gone"], &["Q"])), ("Lib".to_string(), row("steam:111")), ("Q".to_string(), row("steam:111"))].into();
        let lines = ServerMods { mods: ids(&["P", "Lib", "Q"]), workshop: ids(&["111"]), maps: vec![] };
        let got: Vec<(&str, String)> = check(&lines, &installed, Some(&[item("111", "")])).into_iter().map(|p| (p.key, p.item)).collect();
        assert_eq!(got, [("needsLater", "Lib".into()), ("needs", "Gone".into()), ("incompatible", "Q".into())]);
    }
}
