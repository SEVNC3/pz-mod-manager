//! The game's items and vehicles, from its scripts (`media/scripts/**/*.txt`, read only), named
//! from its English translation files: for the admin commands that give items and spawn vehicles.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Entry {
    /// `Module.Type`, as the commands take it.
    pub id: String,
    /// The English name ("" when the game has none).
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Catalog {
    pub items: Vec<Entry>,
    pub vehicles: Vec<Entry>,
}

/// The catalog of the game (or dedicated server) installed in `dir`, read once per folder.
pub fn of(dir: &Path) -> Result<Arc<Catalog>> {
    static CACHE: Mutex<Option<HashMap<PathBuf, Arc<Catalog>>>> = Mutex::new(None);
    if let Some(c) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(dir)) {
        return Ok(c.clone());
    }
    let scripts = dir.join("media/scripts");
    if !scripts.is_dir() {
        bail!("{} has no media/scripts folder", dir.display());
    }
    let mut files = vec![];
    walk(&scripts, &mut files);
    let texts = files.iter().filter_map(|f| std::fs::read(f).ok()).map(|b| super::decode_text(&b));
    let names = translations(&dir.join("media/lua/shared/Translate/EN"));
    let c = Arc::new(read(texts, &names));
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(dir.to_path_buf(), c.clone());
    Ok(c)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt")) {
            out.push(p);
        }
    }
}

/// Item and vehicle names from the translation files: Build 42's JSON (`"Base.Axe": "Axe"`) and
/// Build 41's text (`ItemName_Base.Axe = "Axe",`); vehicles by model (`IGUI_VehicleName<model>`).
fn translations(dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let file = e.file_name().to_string_lossy().to_string();
        if !(file.starts_with("ItemName") || file.starts_with("IG_UI")) {
            continue;
        }
        let Ok(bytes) = std::fs::read(e.path()) else { continue };
        for (k, v) in super::decode_text(&bytes).lines().filter_map(pair) {
            if file.starts_with("ItemName") || k.starts_with("IGUI_VehicleName") {
                out.insert(k.trim_start_matches("ItemName_").to_string(), v);
            }
        }
    }
    out
}

/// `"key": "value"` or `key = "value"`.
fn pair(line: &str) -> Option<(String, String)> {
    let (k, v) = line.split_once("\": ").or_else(|| line.split_once(" = "))?;
    let k = k.trim().trim_matches('"');
    let v = v.trim().trim_end_matches(',').trim().trim_matches('"');
    (!k.is_empty() && !k.contains(' ')).then(|| (k.to_string(), v.to_string()))
}

/// The items and vehicles in script `texts` (vehicle templates aren't vehicles), named from
/// `names` or their own `DisplayName`.
pub fn read(texts: impl Iterator<Item = String>, names: &HashMap<String, String>) -> Catalog {
    let mut items: BTreeMap<String, String> = BTreeMap::new();
    let mut vehicles: BTreeMap<String, String> = BTreeMap::new();
    for text in texts {
        let text = strip_comments(&text);
        let (mut module, mut depth) = (String::from("Base"), 0usize);
        // the item or vehicle whose block this is: (is a vehicle, Module.Type)
        let mut current: Option<(bool, String)> = None;
        for line in text.lines().map(str::trim) {
            let head = line.split('{').next().unwrap_or("");
            if !head.contains('=') {
                // a block's header: what's in the module
                let words: Vec<&str> = head.split_whitespace().collect();
                match (depth, words.as_slice()) {
                    (0, ["module", m, ..]) => module = m.to_string(),
                    (1, [kind @ ("item" | "vehicle"), t, ..]) => {
                        let id = format!("{module}.{t}");
                        if *kind == "item" { items.entry(id.clone()).or_default() } else { vehicles.entry(id.clone()).or_default() };
                        current = Some((*kind == "vehicle", id));
                    }
                    (1, [_, ..]) => current = None,
                    _ => {}
                }
            } else if depth == 2
                && let Some((vehicle, id)) = &current
                && let Some((k, val)) = line.split_once('=')
            {
                let val = val.trim().trim_end_matches(',').trim();
                let set = match (k.trim(), vehicle) {
                    ("DisplayName", false) => Some((&mut items, val.to_string())),
                    ("carModelName", true) => Some((&mut vehicles, names.get(&format!("IGUI_VehicleName{val}")).cloned().unwrap_or_default())),
                    _ => None,
                };
                if let Some((map, name)) = set
                    && let Some(n) = map.get_mut(id).filter(|n| n.is_empty())
                {
                    *n = name;
                }
            }
            depth = (depth + line.matches('{').count()).saturating_sub(line.matches('}').count());
        }
    }
    let entries = |m: BTreeMap<String, String>| m.into_iter().map(|(id, own)| Entry { name: names.get(&id).cloned().unwrap_or(own), id }).collect();
    Catalog { items: entries(items), vehicles: entries(vehicles) }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        // keep the line breaks so a header after the comment stays on its own line
        let end = rest[at..].find("*/").map_or(rest.len(), |e| at + e + 2);
        out.extend(rest[at..end].chars().filter(|c| *c == '\n'));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_and_vehicles_from_scripts() {
        let items = "module Base\n{\n    item Axe\n    {\n        DisplayCategory = Tool,\n        Weight = 3,\n    }\n\n    /* item Commented { } */\n    item Plank { DisplayName = Plank of wood, }\n    item Nail\n    {\n        DisplayName = Nail,\n    }\n    recipe Make Thing\n    {\n        DisplayName = not an item,\n    }\n}\n";
        let vehicles = "module Base\n{\n    template vehicle Lights\n    {\n        carModelName = Nope,\n    }\n    vehicle Van\n    {\n        part Door\n        {\n            carModelName = Inner,\n        }\n        carModelName = PickUpVan,\n    }\n}\nmodule Farm\n{\n    vehicle Tractor { }\n}\n";
        let names: HashMap<String, String> = [("Base.Axe", "Axe"), ("Base.Nail", "Nails"), ("IGUI_VehicleNamePickUpVan", "Pickup Van")].map(|(k, v)| (k.to_string(), v.to_string())).into();
        let c = read([items.to_string(), vehicles.to_string()].into_iter(), &names);
        let e = |id: &str, name: &str| Entry { id: id.into(), name: name.into() };
        assert_eq!(c.items, [e("Base.Axe", "Axe"), e("Base.Nail", "Nails"), e("Base.Plank", "")], "translations win; a one-line block keeps no name");
        assert_eq!(c.vehicles, [e("Base.Van", "Pickup Van"), e("Farm.Tractor", "")]);
        assert_eq!(pair(r#"    "Base.Axe": "Axe","#), Some(("Base.Axe".into(), "Axe".into())));
        assert_eq!(pair(r#"	ItemName_Base.Axe = "Axe","#), Some(("ItemName_Base.Axe".into(), "Axe".into())));
    }

    #[test]
    #[ignore = "reads the installed game (PZ_GAME_DIR)"]
    fn live_game_catalog() {
        let c = of(&super::super::zombiebuddy::game_dir().unwrap()).unwrap();
        eprintln!("{} items, {} vehicles; {:?} {:?}", c.items.len(), c.vehicles.len(), c.items.iter().find(|e| e.id == "Base.Axe"), &c.vehicles[..3]);
        assert!(c.items.iter().any(|e| e.id == "Base.Axe" && !e.name.is_empty()));
    }
}
