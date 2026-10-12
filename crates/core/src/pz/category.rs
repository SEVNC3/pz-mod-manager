//! Sorting mods into categories: by their Workshop tags, or for mods without any, by
//! whether they add a map.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use crate::pz::files;
use crate::sources::steam_workshop;
use crate::store::Row;

const FRAMEWORKS: &str = "Frameworks & libraries";
const GAMEPLAY: &str = "Gameplay";
const MAPS: &str = "Maps";
const OTHER: &str = "Other";

/// Workshop tags (lowercase) per category, strongest first: a mod takes its first match,
/// so a vehicle mod also tagged Models and Textures lands in Vehicles.
const BY_TAG: &[(&str, &[&str])] = &[
    (FRAMEWORKS, &["framework", "modding or configuration"]),
    ("Translations", &["language/translation"]),
    (MAPS, &["map", "maps or levels"]),
    ("Vehicles", &["vehicles"]),
    ("Weapons", &["weapons"]),
    ("Clothing & armour", &["clothing/armor"]),
    ("Characters & traits", &["traits", "skills", "characters", "classes"]),
    ("Interface", &["interface"]),
    ("Items & crafting", &["items", "crafting", "building", "farming", "food", "literature", "loot", "animals", "military", "trading"]),
    (GAMEPLAY, &["hardmode", "balance", "game modes", "gameplay basics"]),
    ("Quality of life", &["qol"]),
    ("Visuals & audio", &["models", "textures", "audio"]),
];

/// Sections in list order: libraries first so what needs them loads after, translations
/// last so they override the text they translate.
pub const ORDER: &[&str] = &[
    FRAMEWORKS,
    GAMEPLAY,
    "Quality of life",
    "Interface",
    "Characters & traits",
    "Items & crafting",
    "Weapons",
    "Clothing & armour",
    "Vehicles",
    "Visuals & audio",
    MAPS,
    "Translations",
    OTHER,
];

/// The category for a mod's Workshop tags; None without any (build tags don't count).
/// Tags that say how a mod plays but not what it adds (Realistic, Multiplayer, ...) mean Gameplay.
fn from_tags(tags: &[String]) -> Option<&'static str> {
    let tags: Vec<String> = tags.iter().map(|t| t.to_lowercase()).filter(|t| !t.starts_with("build ")).collect();
    if tags.is_empty() {
        return None;
    }
    let hit = BY_TAG.iter().find(|(_, want)| tags.iter().any(|t| want.contains(&t.as_str())));
    Some(hit.map_or(GAMEPLAY, |(name, _)| name))
}

/// Workshop tags by item id, fetched once per run.
static TAGS: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);

/// Workshop tags of the mods' items, by item id (fetched once per run; Steam being
/// unreachable is said and leaves them out).
fn workshop_tags(mods: &BTreeMap<String, Row>) -> HashMap<String, Vec<String>> {
    let mut cache = TAGS.lock().unwrap_or_else(|e| e.into_inner());
    let tags = cache.get_or_insert_with(HashMap::new);
    let mut todo: Vec<String> = mods.values().filter_map(|r| crate::sources::workshop_item(&r.source).map(String::from)).filter(|i| !tags.contains_key(i)).collect();
    todo.sort();
    todo.dedup();
    for chunk in todo.chunks(100) {
        let ids: Vec<&str> = chunk.iter().map(String::as_str).collect();
        match steam_workshop::details(&ids) {
            Ok(items) => tags.extend(items.into_iter().map(|i| (i.id, i.tags))),
            Err(e) => crate::say!("tagsFailed", "Couldn't read Workshop tags: {error}", error = format!("{e:#}")),
        }
    }
    tags.clone()
}

fn tags_of<'a>(r: &Row, tags: &'a HashMap<String, Vec<String>>) -> &'a [String] {
    crate::sources::workshop_item(&r.source).and_then(|i| tags.get(i)).map_or(&[], Vec::as_slice)
}

/// The category of every mod. Steam being unreachable only loses the tags: those mods
/// fall back to Maps or Other.
pub fn categorize(mods: &BTreeMap<String, Row>) -> BTreeMap<String, String> {
    let tags = workshop_tags(mods);
    mods.iter()
        .map(|(id, r)| {
            let cat = from_tags(tags_of(r, &tags)).unwrap_or_else(|| if files::maps(&r.dir).is_empty() { OTHER } else { MAPS });
            (id.clone(), cat.to_string())
        })
        .collect()
}

/// The kinds a mod is (framework, patch, map, ui, translation: the library filters on them):
/// `tags` its Workshop tags, `maps` whether it adds map folders, `needed_by` how many other mods
/// require it.
fn kinds_of(id: &str, name: &str, tags: &[String], maps: bool, needed_by: usize) -> Vec<&'static str> {
    let tag = |t: &[&str]| tags.iter().any(|x| t.contains(&x.to_lowercase().as_str()));
    let text = format!(" {} {} ", id, name).to_lowercase();
    let says = |words: &[&str]| words.iter().any(|w| text.contains(w));
    let mut out = vec![];
    if tag(&["framework"]) || needed_by >= 2 || says(&["framework", " lib ", "library", " api ", "_api ", " core "]) {
        out.push("framework");
    }
    if says(&["patch", "compat", " fix ", "fixes", "hotfix", " addon", "add-on"]) {
        out.push("patch");
    }
    if maps {
        out.push("map");
    }
    if tag(&["interface"]) || says(&[" ui ", " ui)", "(ui", " hud", "interface", "menu"]) {
        out.push("ui");
    }
    if tag(&["language/translation"]) || says(&["translat", "tradu", "übersetz", "перевод", "汉化", "中文", "localiz", "locali"]) {
        out.push("translation");
    }
    out
}

/// Every mod's kinds (see `KINDS`); asks Steam for Workshop tags like `categorize`.
pub fn kinds(mods: &BTreeMap<String, Row>) -> BTreeMap<String, Vec<&'static str>> {
    let tags = workshop_tags(mods);
    let mut needed: HashMap<&str, usize> = HashMap::new();
    for r in mods.values() {
        for q in &r.requires {
            *needed.entry(q.as_str()).or_default() += 1;
        }
    }
    mods.iter()
        .map(|(id, r)| (id.clone(), kinds_of(id, &r.name, tags_of(r, &tags), !files::maps(&r.dir).is_empty(), needed.get(id.as_str()).copied().unwrap_or(0))))
        .filter(|(_, k)| !k.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn strongest_tag_wins() {
        assert_eq!(from_tags(&tags(&["Build 42"])), None);
        assert_eq!(from_tags(&tags(&["Models", "Textures", "Vehicles"])), Some("Vehicles"));
        assert_eq!(from_tags(&tags(&["Clothing/Armor", "Items", "Models"])), Some("Clothing & armour"));
        assert_eq!(from_tags(&tags(&["Realistic", "QOL"])), Some("Quality of life"));
        assert_eq!(from_tags(&tags(&["Balance", "Hardmode", "QOL"])), Some(GAMEPLAY));
        assert_eq!(from_tags(&tags(&["Multiplayer"])), Some(GAMEPLAY));
        assert_eq!(from_tags(&tags(&["Framework", "Interface"])), Some(FRAMEWORKS));
        assert_eq!(from_tags(&tags(&["building"])), Some("Items & crafting"));
        assert!(BY_TAG.iter().all(|(c, _)| ORDER.contains(c)));
    }

    #[test]
    fn kinds_from_tags_names_files_and_needs() {
        assert_eq!(kinds_of("tsarslib", "TsarLib", &[], false, 5), ["framework"]);
        assert_eq!(kinds_of("A", "Brita's Weapon Pack", &tags(&["Weapons"]), false, 1), Vec::<&str>::new());
        assert_eq!(kinds_of("ArsenalGunFighterPatch", "Arsenal(26) GunFighter Compatibility Patch", &[], false, 0), ["patch"]);
        assert_eq!(kinds_of("Raven", "Raven Creek", &tags(&["Map"]), true, 0), ["map"]);
        assert_eq!(kinds_of("MinimalDisplayBars", "Minimal Display Bars", &tags(&["Interface"]), false, 0), ["ui"]);
        assert_eq!(kinds_of("X_RU", "Перевод модов", &[], false, 0), ["translation"]);
        assert_eq!(kinds_of("ModOptions", "Mod Options", &tags(&["Framework", "Interface"]), false, 0), ["framework", "ui"]);
    }
}
