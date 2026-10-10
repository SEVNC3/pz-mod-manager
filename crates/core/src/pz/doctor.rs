//! Health check for the active profile and server configs ("doctor"): problems the
//! game reports badly or not at all (it skips a mod silently, or kicks players later).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use super::{Paths, files, is_version_dir, server};
use crate::store::Row;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// The game won't load something, or players can't join.
    Error,
    /// Likely to misbehave.
    Warning,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Finding {
    pub level: Level,
    /// The mod or file it's about.
    pub subject: String,
    pub text: String,
}

fn err(subject: &str, text: String) -> Finding {
    Finding { level: Level::Error, subject: subject.into(), text }
}

fn warn(subject: &str, text: String) -> Finding {
    Finding { level: Level::Warning, subject: subject.into(), text }
}

/// Which builds a mod's folder layout supports: (has a root mod.info, has version folders).
fn layout(dir: &Path) -> (bool, bool) {
    let root = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case("mod.info")));
    let versioned = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name().to_str().is_some_and(is_version_dir));
    (root, versioned)
}

/// Check the enabled mods (in load order) for `build` (41/42), against everything
/// installed (`mods`), plus every server config.
pub fn check(paths: &Paths, enabled: &[String], mods: &BTreeMap<String, Row>, build: u8) -> Vec<Finding> {
    let mut out = vec![];
    // ids the Steam subscriptions provide (the scan shows a local copy instead when both exist)
    let subscribed: HashSet<String> = crate::sources::steam_workshop::content_dir(super::STEAM_APP)
        .map(|ws| super::workshop_mods(&ws).into_iter().map(|m| m.id).collect())
        .unwrap_or_default();
    let pos: HashMap<&str, usize> = enabled.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    for (i, id) in enabled.iter().enumerate() {
        let Some(m) = mods.get(id) else {
            out.push(err(id, "enabled but not installed: the game skips it".into()));
            continue;
        };
        for r in &m.requires {
            match pos.get(r.as_str()) {
                None if mods.contains_key(r) => out.push(err(id, format!("needs {r}, which is disabled"))),
                None => out.push(err(id, format!("needs {r}, which isn't installed"))),
                Some(&j) if j > i => out.push(err(id, format!("needs {r}, which loads after it"))),
                Some(_) => {}
            }
        }
        for x in &m.incompatible {
            if pos.contains_key(x.as_str()) {
                out.push(err(id, format!("is incompatible with {x}, which is also enabled")));
            }
        }
        for a in &m.load_after {
            if pos.get(a.as_str()).is_some_and(|&j| j > i) {
                out.push(warn(id, format!("should load after {a}")));
            }
        }
        for b in &m.load_before {
            if pos.get(b.as_str()).is_some_and(|&j| j < i) {
                out.push(warn(id, format!("should load before {b}")));
            }
        }
        let (root, versioned) = layout(&m.dir);
        if build >= 42 && !versioned {
            out.push(warn(id, "is built for Build 41 (no 42/ or common/ folder); on Build 42 it may error or do nothing".into()));
        } else if build == 41 && !root {
            out.push(warn(id, "is built for Build 42 only (no root mod.info)".into()));
        }
        if !m.source.starts_with("steam:") && subscribed.contains(id) {
            out.push(warn(id, "is in Zomboid/mods and also subscribed in Steam: the Zomboid/mods copy wins and won't get Steam's updates".into()));
        }
    }
    out.extend(check_servers(paths, mods, build));
    out
}

/// Server configs whose Mods=, WorkshopItems= and Map= disagree.
fn check_servers(paths: &Paths, mods: &BTreeMap<String, Row>, build: u8) -> Vec<Finding> {
    let mut out = vec![];
    for name in server::list(paths) {
        let Ok(file) = server::path(paths, &name) else { continue };
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(cfg) = server::read(&file) else { continue };
        let subject = format!("{name}.ini");
        let workshop: HashSet<&str> = cfg.workshop.iter().map(String::as_str).collect();
        let mut needed_items = HashSet::new();
        for id in &cfg.mods {
            match mods.get(id) {
                Some(m) => {
                    if let Some(item) = m.source.strip_prefix("steam:").or_else(|| m.source.strip_prefix("workshop:")) {
                        needed_items.insert(item.to_string());
                        if !workshop.contains(item) {
                            out.push(err(&subject, format!("Mods= has {id}, but its Workshop item {item} isn't in WorkshopItems=: the server won't download it")));
                        }
                    }
                }
                None => out.push(warn(&subject, format!("Mods= has {id}, which isn't installed here, so PZMM can't check it"))),
            }
        }
        for item in &cfg.workshop {
            if !needed_items.contains(item) && mods.values().any(|m| m.source.ends_with(&format!(":{item}"))) {
                out.push(warn(&subject, format!("WorkshopItems= has {item}, but none of its mods are in Mods=")));
            }
        }
        let mods_line = text.lines().find_map(|l| l.strip_prefix("Mods=")).unwrap_or("");
        if build >= 42 && !cfg.mods.is_empty() && !mods_line.contains('\\') {
            out.push(err(&subject, "Mods= entries need a leading backslash on Build 42 (\\ModId)".into()));
        }
        let map_line = text.lines().find_map(|l| l.strip_prefix("Map=")).unwrap_or("");
        let maps: Vec<&str> = map_line.split(';').map(str::trim).filter(|m| !m.is_empty()).collect();
        let mod_maps: Vec<String> = cfg.mods.iter().filter_map(|id| mods.get(id)).flat_map(|m| files::maps(&m.dir)).collect();
        for mm in &mod_maps {
            if !maps.contains(&mm.as_str()) {
                out.push(warn(&subject, format!("a mod adds the map {mm}, but Map= doesn't list it")));
            }
        }
        if let Some(base) = maps.iter().position(|m| *m == "Muldraugh, KY")
            && maps[base + 1..].iter().any(|m| mod_maps.iter().any(|mm| mm == m))
        {
            out.push(warn(&subject, "Map= lists mod maps after Muldraugh, KY: mod maps must come first".into()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_paths;
    use std::fs;
    use std::path::PathBuf;

    fn row(id: &str, dir: PathBuf, source: &str, requires: &[&str]) -> Row {
        Row {
            id: id.into(),
            name: id.into(),
            source: source.into(),
            info: dir.join("mod.info"),
            dir,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
        }
    }

    #[test]
    fn finds_setup_problems() {
        let (t, paths) = temp_paths();
        let b41 = t.path().join("Old");
        fs::create_dir_all(&b41).unwrap();
        fs::write(b41.join("mod.info"), "id=Old\n").unwrap();
        let b42 = t.path().join("New");
        fs::create_dir_all(b42.join("42")).unwrap();
        let mods = BTreeMap::from([
            ("Old".to_string(), row("Old", b41, "steam:1", &[])),
            ("New".to_string(), row("New", b42.clone(), "steam:2", &["Lib"])),
            ("Lib".to_string(), row("Lib", b42, "steam:3", &[])),
        ]);
        let enabled = vec!["New".to_string(), "Old".to_string(), "Gone".to_string()];
        let found = check(&paths, &enabled, &mods, 42);
        let texts: Vec<String> = found.iter().map(|f| format!("{}: {}", f.subject, f.text)).collect();
        assert!(texts.iter().any(|t| t.starts_with("New: needs Lib, which is disabled")), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("Old: is built for Build 41")), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("Gone: enabled but not installed")), "{texts:?}");

        fs::create_dir_all(paths.zomboid.join("Server")).unwrap();
        fs::write(paths.zomboid.join("Server/srv.ini"), "Mods=New;Lib\nWorkshopItems=2\nMap=Muldraugh, KY\n").unwrap();
        let found = check(&paths, &[], &mods, 42);
        let texts: Vec<String> = found.iter().map(|f| f.text.clone()).collect();
        assert!(texts.iter().any(|t| t.contains("Lib, but its Workshop item 3 isn't in WorkshopItems=")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("leading backslash")), "{texts:?}");
    }
}
