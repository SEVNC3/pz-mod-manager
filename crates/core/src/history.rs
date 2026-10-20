//! Download history: what PZMM installed, updated, removed, subscribed or unsubscribed, newest
//! first, so the app can offer them again (Reinstall) or take them out. Best effort: a history
//! that can't be written never stops an install.

use crate::pz::Paths;
use serde::{Deserialize, Serialize};

/// Entries kept: the oldest go first.
const KEEP: usize = 500;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// The mod id (for a subscription: the Workshop item id).
    pub id: String,
    pub name: String,
    /// Where it came from: `workshop:<id>`, `steam:<id>` (Steam's subscription), `github:...`, `local:...`.
    pub source: String,
    /// "installed", "updated", "removed", "subscribed" or "unsubscribed".
    pub action: String,
    /// Seconds since 1970.
    pub at: u64,
}

fn file(paths: &Paths) -> std::path::PathBuf {
    paths.data.join("download-history.json")
}

/// The history, newest first.
pub fn list(paths: &Paths) -> Vec<Record> {
    crate::read_json(&file(paths)).unwrap_or_default()
}

/// Note one change (its name, when not given, from the last record of the same id).
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn add(paths: &Paths, id: &str, name: &str, source: &str, action: &str) {
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    // unreadable now: not written over with this one record
    let Ok(mut all) = crate::read_json::<Vec<Record>>(&file(paths)) else { return };
    let name = if name.is_empty() { all.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_default() } else { name.to_string() };
    all.insert(0, Record { id: id.into(), name, source: source.into(), action: action.into(), at: crate::store::now() });
    all.truncate(KEEP);
    write(paths, &all);
}

/// Forget the records of `ids`, or all of them.
pub fn clear(paths: &Paths, ids: Option<&[String]>) -> Vec<Record> {
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(mut all) = crate::read_json::<Vec<Record>>(&file(paths)) else { return list(paths) };
    match ids {
        Some(ids) => all.retain(|r| !ids.contains(&r.id)),
        None => all.clear(),
    }
    write(paths, &all);
    all
}

fn write(paths: &Paths, all: &[Record]) {
    let _ = std::fs::create_dir_all(&paths.data);
    if let Ok(bytes) = serde_json::to_vec(all) {
        let _ = crate::write_atomic(&file(paths), bytes);
    }
    for_game(paths, all);
    crate::store::first_seen_known_for_game(paths);
}

/// The history for the PZMM - Client mod: `history.txt`, newest first, `<at>\t<action>\t<id>\t<name>\t<source>` lines.
/// Only where PZMM - Client is.
pub fn for_game(paths: &Paths, all: &[Record]) {
    let dir = crate::pz::bridge::dir(paths);
    if dir.is_dir() {
        let flat = |s: &str| s.replace(['\t', '\r', '\n'], " ");
        let text: String = all.iter().map(|r| format!("{}\t{}\t{}\t{}\t{}\n", r.at, r.action, flat(&r.id), flat(&r.name), flat(&r.source))).collect();
        crate::pz::bridge::write_for_game(&dir.join("history.txt"), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_newest_first_and_names_removals() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().join("Zomboid"), data: t.path().join("data") };
        add(&paths, "A", "Mod A", "workshop:1", "installed");
        add(&paths, "B", "Mod B", "workshop:2", "installed");
        std::fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        add(&paths, "A", "", "workshop:1", "removed");
        let game = std::fs::read_to_string(paths.zomboid.join("Lua/pzmm/history.txt")).unwrap();
        assert!(game.lines().count() == 3 && game.lines().next().unwrap().ends_with("\tremoved\tA\tMod A\tworkshop:1"), "the game's copy, newest first: {game}");
        let l = list(&paths);
        assert_eq!(l.iter().map(|r| (r.id.as_str(), r.name.as_str(), r.action.as_str())).collect::<Vec<_>>(), [("A", "Mod A", "removed"), ("B", "Mod B", "installed"), ("A", "Mod A", "installed")]);
        assert_eq!(clear(&paths, Some(&["A".into()])).len(), 1);
        assert!(clear(&paths, None).is_empty());
        assert!(list(&paths).is_empty());
    }
}
