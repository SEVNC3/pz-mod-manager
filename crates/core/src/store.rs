//! Mods PZMM installed: the index (`<data>/installed.json`), copies into the game's
//! mods folder, one-deep backups and rollback. Only indexed mods are ours to touch.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::pz::{self, Paths};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// `workshop:<id>`, `github:<owner/repo[@ref]>` or `local:<path>`.
    pub source: String,
    /// When PZMM last installed it (seconds since 1970); 0 for v0.1 imports.
    #[serde(default)]
    pub installed_at: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub type Index = BTreeMap<String, Entry>;

fn index_path(paths: &Paths) -> PathBuf {
    paths.data.join("installed.json")
}

pub fn load(paths: &Paths) -> Result<Index> {
    let p = index_path(paths);
    if p.exists() {
        return serde_json::from_str(&fs::read_to_string(&p)?).with_context(|| format!("reading {}", p.display()));
    }
    // v0.1 kept `{id: source}` in Zomboid/mods/pzmm.json
    let old = paths.mods().join("pzmm.json");
    let v1: BTreeMap<String, String> =
        if old.exists() { serde_json::from_str(&fs::read_to_string(old)?).unwrap_or_default() } else { BTreeMap::new() };
    Ok(v1.into_iter().map(|(id, source)| (id, Entry { source, installed_at: 0 })).collect())
}

pub fn save(paths: &Paths, index: &Index) -> Result<()> {
    fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&index_path(paths), serde_json::to_string_pretty(index)?)?;
    Ok(())
}

/// The mod's folder in `Zomboid/mods`. Refuses ids that can't be a single safe folder
/// name (`..`, `CON`, `a/b`...), so no id can point outside the mods folder.
fn mod_dir(paths: &Paths, id: &str) -> Result<PathBuf> {
    if !crate::safe_file_name(id) {
        bail!("refusing mod id {id:?}: it can't be used as a folder name");
    }
    Ok(paths.mods().join(id))
}

fn backup_dir(paths: &Paths, id: &str) -> Result<PathBuf> {
    mod_dir(paths, id)?;
    Ok(paths.data.join("backups").join(id))
}

pub(crate) fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        let kind = e.file_type()?;
        if kind.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else if kind.is_file() {
            fs::copy(e.path(), dest)?;
        } // symlinks inside downloaded mods are skipped
    }
    Ok(())
}

/// Rename, falling back to copy + delete across drives.
fn move_dir(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        // only a move across drives needs a copy; anything else (a file the game has open) must
        // fail cleanly, or the copy succeeds and the delete stops halfway
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            copy_dir(from, to)?;
            fs::remove_dir_all(from)?;
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("moving {} (close the game and try again)", from.display())),
    }
}

/// Same files with the same bytes (so an "update" would change nothing).
fn same_tree(a: &Path, b: &Path) -> bool {
    fn files(root: &Path) -> Option<BTreeMap<PathBuf, u64>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).ok()? {
                let e = e.ok()?;
                let kind = e.file_type().ok()?;
                if kind.is_dir() {
                    stack.push(e.path());
                } else if kind.is_file() {
                    out.insert(e.path().strip_prefix(root).ok()?.to_path_buf(), e.metadata().ok()?.len());
                }
            }
        }
        Some(out)
    }
    match (files(a), files(b)) {
        (Some(fa), Some(fb)) if fa == fb => fa.keys().all(|rel| fs::read(a.join(rel)).ok() == fs::read(b.join(rel)).ok()),
        _ => false,
    }
}

/// Copy every mod found under `root` into the game's mods folder. A mod already there is
/// replaced only if PZMM installed it; its previous version becomes the rollback backup
/// (kept as is when the new copy is identical). Each mod is staged next to its final
/// folder and swapped in, then recorded, so a failure never leaves a half-copied mod.
/// Mods that can't be installed are logged and skipped. Returns the installed ids.
pub fn install(paths: &Paths, root: &Path, source: &str) -> Result<Vec<String>> {
    let found = pz::find_mods(root, 8);
    if found.is_empty() {
        bail!("{source}: no mod.info found");
    }
    let mut index = load(paths)?;
    let mut ids: Vec<String> = vec![];
    for m in found {
        if ids.contains(&m.id) {
            crate::say!("duplicateId", "{source}: skipped a second mod with the id {id}", source = source, id = m.id);
            continue;
        }
        let dest = match mod_dir(paths, &m.id) {
            Ok(d) => d,
            Err(e) => {
                crate::log!("error: {source}: {e:#}");
                continue;
            }
        };
        if dest.exists() && !index.contains_key(&m.id) {
            crate::say!(
                "notOursInMods",
                "error: {id} is already in {dir} and wasn't installed by PZMM; left it alone. Remove that folder to install this copy.",
                id = m.id,
                dir = paths.mods().display()
            );
            continue;
        }
        let mut noted = None;
        if dest.exists() && same_tree(&m.dir, &dest) {
            crate::say!("alreadyCurrent", "{id} is already up to date ({source})", id = m.id, source = source);
        } else {
            // staged beside Zomboid/mods (on its drive, even through a link: one rename into place,
            // never a copy) and outside it, so the game never sees a half-copied mod
            let mods = fs::canonicalize(paths.mods()).unwrap_or_else(|_| paths.mods());
            let staging = mods.parent().unwrap_or(&paths.zomboid).join(".pzmm-staging").join(&m.id);
            if staging.exists() {
                fs::remove_dir_all(&staging)?;
            }
            if let Err(e) = copy_dir(&m.dir, &staging) {
                let _ = fs::remove_dir_all(&staging);
                return Err(e).with_context(|| format!("copying {} into {}", m.id, paths.mods().display()));
            }
            let update = dest.exists();
            if update {
                let backup = backup_dir(paths, &m.id)?;
                if backup.exists() {
                    fs::remove_dir_all(&backup)?;
                }
                move_dir(&dest, &backup)?;
            }
            move_dir(&staging, &dest).with_context(|| format!("moving {} into place", m.id))?;
            let _ = fs::remove_dir(staging.parent().unwrap_or(&staging)); // when empty
            if update {
                crate::say!("updated", "updated {id} ({source})", id = m.id, source = source);
            } else {
                crate::say!("installed", "installed {id} ({source})", id = m.id, source = source);
            }
            noted = Some(update);
        }
        index.insert(m.id.clone(), Entry { source: source.to_string(), installed_at: now() });
        save(paths, &index)?;
        // noted once it's in the index
        if let Some(update) = noted {
            crate::history::add(paths, &m.id, &m.name, source, if update { "updated" } else { "installed" });
        }
        ids.push(m.id);
    }
    Ok(ids)
}

pub fn has_backup(paths: &Paths, id: &str) -> bool {
    backup_dir(paths, id).is_ok_and(|b| b.exists())
}

/// Swap a mod back to the version it had before its last install/update.
pub fn rollback(paths: &Paths, id: &str) -> Result<()> {
    // a stale backup must never be swapped onto a folder PZMM doesn't own (a hand-installed copy)
    if !load(paths)?.contains_key(id) {
        bail!("{id}: PZMM didn't install this mod, so it has no previous version to restore");
    }
    let backup = backup_dir(paths, id)?;
    if !backup.exists() {
        bail!("{id}: no previous version to restore");
    }
    let dest = mod_dir(paths, id)?;
    let tmp = paths.data.join("backups").join(format!("{id}.swap"));
    if dest.exists() {
        move_dir(&dest, &tmp)?;
    }
    move_dir(&backup, &dest)?;
    if tmp.exists() {
        move_dir(&tmp, &backup)?; // rolling back again re-applies the update
    }
    Ok(())
}

/// Remove mods PZMM installed: to the Recycle Bin with `recycle`, else deleted. Returns ids
/// it refused because they aren't ours.
pub fn remove(paths: &Paths, ids: &[String], recycle: bool) -> Result<Vec<String>> {
    let mut index = load(paths)?;
    let mut refused = vec![];
    for id in ids {
        if index.contains_key(id) && let Ok(dir) = mod_dir(paths, id) {
            if dir.exists() && recycle {
                trash::delete(&dir).with_context(|| format!("moving {} to the Recycle Bin", dir.display()))?;
            } else if dir.exists() {
                fs::remove_dir_all(dir)?;
            }
            // its backup goes too, so a later install of the same id can't roll back to it
            if let Ok(backup) = backup_dir(paths, id)
                && backup.exists()
            {
                fs::remove_dir_all(backup)?;
            }
            if let Some(e) = index.remove(id) {
                save(paths, &index)?;
                crate::say!("removed", "removed {id} ({source})", id = id, source = e.source);
                crate::history::add(paths, id, "", &e.source, "removed");
            }
        } else {
            refused.push(id.clone());
        }
    }
    save(paths, &index)?;
    Ok(refused)
}

/// Remove mods added to `Zomboid/mods` outside PZMM: always to the Recycle Bin, since PZMM can't
/// get them back. A folder holding other mods too is left alone. Returns the ids it refused.
pub fn remove_manual(paths: &Paths, ids: &[String]) -> Result<Vec<String>> {
    let index = load(paths)?;
    let mods = paths.mods();
    let found = pz::local_mods(paths);
    let top = |m: &pz::FoundMod| m.dir.strip_prefix(&mods).ok().and_then(|r| r.iter().next()).map(|f| mods.join(f));
    let mut refused = vec![];
    for id in ids {
        let dir = found.iter().find(|m| &m.id == id && !index.contains_key(id)).and_then(top);
        let shared = |d: &PathBuf| found.iter().any(|m| !ids.contains(&m.id) && top(m).as_ref() == Some(d));
        match dir {
            Some(d) if !shared(&d) => {
                if d.exists() {
                    trash::delete(&d).with_context(|| format!("moving {} to the Recycle Bin", d.display()))?;
                }
            }
            _ => refused.push(id.clone()),
        }
    }
    Ok(refused)
}

/// Each backup (a mod's previous version) with its size and age (its newest file).
fn backups(paths: &Paths) -> Vec<(PathBuf, pz::files::Stats)> {
    let dirs = fs::read_dir(paths.data.join("backups")).into_iter().flatten().flatten();
    dirs.filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| (e.path(), pz::files::stats(&e.path()))).collect()
}

/// Bytes all backups take.
pub fn backups_size(paths: &Paths) -> u64 {
    backups(paths).iter().map(|(_, s)| s.size).sum()
}

/// Delete the oldest backups until all of them fit in `cap` bytes. Returns how many went.
pub fn prune_backups(paths: &Paths, cap: u64) -> Result<usize> {
    let mut all = backups(paths);
    all.sort_by_key(|(_, s)| s.modified);
    let mut total: u64 = all.iter().map(|(_, s)| s.size).sum();
    let mut gone = 0;
    for (dir, s) in all {
        if total <= cap {
            break;
        }
        fs::remove_dir_all(&dir)?;
        total -= s.size;
        gone += 1;
    }
    if gone > 0 {
        crate::say!("backupsPruned", "Deleted the {count} oldest backup(s) to keep backups under {mb} MB", count = gone, mb = cap >> 20);
    }
    Ok(gone)
}

/// Delete every backup (no mod can be restored to its previous version afterwards).
pub fn clear_backups(paths: &Paths) -> Result<()> {
    let dir = paths.data.join("backups");
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

// --- tags: the player's own labels per mod, in `<data>/tags.json` (every profile shares them) ---

pub type Tags = BTreeMap<String, Vec<String>>;

/// Mod id -> its tags (none without the file; a damaged one is kept aside, see `read_json`).
pub fn tags(paths: &Paths) -> Tags {
    crate::read_json(&paths.data.join("tags.json")).unwrap_or_default()
}

/// Set a mod's tags (trimmed, blanks and repeats dropped; none removes it). Returns them all.
pub fn set_tags(paths: &Paths, id: &str, list: &[String]) -> Result<Tags> {
    let mut all: Tags = crate::read_json(&paths.data.join("tags.json"))?;
    let mut clean: Vec<String> = vec![];
    for t in list.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !clean.iter().any(|c| c.eq_ignore_ascii_case(t)) {
            clean.push(t.to_string());
        }
    }
    if clean.is_empty() {
        all.remove(id);
    } else {
        all.insert(id.to_string(), clean);
    }
    fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&paths.data.join("tags.json"), serde_json::to_string_pretty(&all)?)?;
    Ok(all)
}

#[derive(Clone, Default)]
pub struct Row {
    pub id: String,
    pub name: String,
    /// Index source, `steam:<item>` for Workshop subscriptions, or `manual`.
    pub source: String,
    pub dir: PathBuf,
    pub info: PathBuf,
    pub requires: Vec<String>,
    pub incompatible: Vec<String>,
    pub load_after: Vec<String>,
    pub load_before: Vec<String>,
    /// Mod Load Order Sorter's `loadFirst=` / `loadLast=` ("on", "category" or empty).
    pub load_first: String,
    pub load_last: String,
    pub author: String,
    pub supports_41: bool,
    pub supports_42: bool,
    /// Languages it has texts in (`Translate/<LANG>` folders).
    pub languages: Vec<String>,
}

impl Row {
    fn from_found(m: pz::FoundMod, source: String) -> Row {
        Row {
            languages: pz::files::languages(&m.dir),
            id: m.id,
            name: m.name,
            source,
            dir: m.dir,
            info: m.info,
            requires: m.requires,
            incompatible: m.incompatible,
            load_after: m.load_after,
            load_before: m.load_before,
            load_first: m.load_first,
            load_last: m.load_last,
            author: m.author,
            supports_41: m.supports_41,
            supports_42: m.supports_42,
        }
    }
}

/// The mods in a Workshop content folder (`<ws>/<item>/mods/...`), sourced `steam:<item>`.
pub fn workshop_rows(ws: &Path) -> impl Iterator<Item = Row> + '_ {
    pz::workshop_mods(ws).into_iter().map(move |m| {
        let item = m.dir.strip_prefix(ws).ok().and_then(|r| r.iter().next()).map(|s| s.to_string_lossy().into_owned());
        Row::from_found(m, format!("steam:{}", item.unwrap_or_default()))
    })
}

/// Every mod the game can load: Steam subscriptions, then Zomboid/mods (wins on clashes).
/// A PZMM copy of a Workshop item the user has since subscribed to goes to the Recycle Bin.
pub fn scan(paths: &Paths) -> Result<BTreeMap<String, Row>> {
    scan_in(paths, pz::workshop_dir().as_deref(), true)
}

fn scan_in(paths: &Paths, ws: Option<&Path>, recycle: bool) -> Result<BTreeMap<String, Row>> {
    let index = load(paths)?;
    let mut out = BTreeMap::new();
    if let Some(ws) = ws {
        out.extend(workshop_rows(ws).map(|r| (r.id.clone(), r)));
    }
    for m in pz::local_mods(paths) {
        let source = index.get(&m.id).map_or("manual".to_string(), |e| e.source.clone());
        // Steam keeps the subscription updated, and the game would load our copy over it
        let subscribed = source.strip_prefix("workshop:").is_some_and(|item| out.get(&m.id).is_some_and(|r: &Row| r.source == format!("steam:{item}")));
        if subscribed && remove(paths, std::slice::from_ref(&m.id), recycle)?.is_empty() {
            crate::say!("subscribedCopyRemoved", "{id}: you subscribed in Steam; PZMM's copy was removed", id = m.id);
            continue;
        }
        out.insert(m.id.clone(), Row::from_found(m, source));
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn temp_paths() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().join("Zomboid"), data: t.path().join("data") };
        (t, paths)
    }

    fn mod_tree(root: &Path, rel: &str, info: &str) {
        let p = root.join(rel);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("mod.info"), info).unwrap();
    }

    #[test]
    fn subscribed_copy_removed() {
        let (t, paths) = temp_paths();
        let ws = t.path().join("ws");
        mod_tree(&ws, "42/mods/ModA", "id=ModA
name=A");
        mod_tree(&paths.mods(), "ModA", "id=ModA
name=A");
        mod_tree(&paths.mods(), "ModB", "id=ModB
name=B");
        fs::write(paths.mods().join("pzmm.json"), r#"{"ModA": "workshop:42", "ModB": "workshop:7"}"#).unwrap();
        let rows = scan_in(&paths, Some(&ws), false).unwrap();
        assert_eq!(rows["ModA"].source, "steam:42");
        assert_eq!(rows["ModB"].source, "workshop:7");
        assert!(!paths.mods().join("ModA").exists());
        assert!(!load(&paths).unwrap().contains_key("ModA"));
    }

    #[test]
    fn install_update_rollback_remove() {
        let (t, paths) = temp_paths();
        // GitHub zipball: mod at the repo root lands under its id, not "owner-repo-sha"
        mod_tree(t.path(), "src/owner-repo-abc/42", "id=RootMod\nname=v1\n");
        assert_eq!(install(&paths, &t.path().join("src"), "github:owner/repo").unwrap(), ["RootMod"]);
        let info = paths.mods().join("RootMod/42/mod.info");
        assert!(info.exists());

        mod_tree(t.path(), "src/owner-repo-abc/42", "id=RootMod\nname=v2\n");
        install(&paths, &t.path().join("src"), "github:owner/repo").unwrap();
        assert!(fs::read_to_string(&info).unwrap().contains("v2"));
        rollback(&paths, "RootMod").unwrap();
        assert!(fs::read_to_string(&info).unwrap().contains("v1"));
        rollback(&paths, "RootMod").unwrap();
        assert!(fs::read_to_string(&info).unwrap().contains("v2"));

        // manual mods aren't ours to delete
        mod_tree(&paths.mods(), "Manual", "id=Manual\n");
        assert_eq!(remove(&paths, &["RootMod".into(), "Manual".into()], false).unwrap(), ["Manual"]);
        assert!(!paths.mods().join("RootMod").exists() && paths.mods().join("Manual").exists());
        // remove_manual: refuses PZMM's own, unknown ids and a folder that holds another mod too
        // (the Recycle Bin move itself isn't run here, so tests leave nothing in the real one)
        install(&paths, &t.path().join("src"), "github:owner/repo").unwrap();
        mod_tree(&paths.mods(), "Pack/A", "id=PackA\n");
        mod_tree(&paths.mods(), "Pack/B", "id=PackB\n");
        assert_eq!(remove_manual(&paths, &["RootMod".into(), "Nope".into(), "PackA".into()]).unwrap(), ["RootMod", "Nope", "PackA"]);
        assert!(paths.mods().join("Pack/A").exists() && paths.mods().join("RootMod").exists());
    }

    #[test]
    fn refuses_unsafe_ids_and_unowned_folders() {
        let (t, paths) = temp_paths();
        fs::create_dir_all(paths.zomboid.join("Saves")).unwrap();
        mod_tree(t.path(), "evil/x", "id=..\n");
        mod_tree(t.path(), "evil/y", "id=CON\n");
        assert!(install(&paths, &t.path().join("evil"), "local:evil").unwrap().is_empty());
        assert!(paths.zomboid.join("Saves").exists(), "Zomboid folder untouched");
        assert!(rollback(&paths, "..").is_err());
        assert_eq!(remove(&paths, &["..".into()], false).unwrap(), [".."]);

        // a hand-installed mod with the same id is left alone
        mod_tree(&paths.mods(), "Hand", "id=Hand\nname=mine\n");
        mod_tree(t.path(), "dl/Hand", "id=Hand\nname=download\n");
        assert!(install(&paths, &t.path().join("dl"), "local:dl").unwrap().is_empty());
        assert!(fs::read_to_string(paths.mods().join("Hand/mod.info")).unwrap().contains("mine"));
        assert!(!has_backup(&paths, "Hand"));
    }

    #[test]
    fn identical_update_keeps_the_real_backup() {
        let (t, paths) = temp_paths();
        mod_tree(t.path(), "v1/M", "id=M\nname=v1\n");
        mod_tree(t.path(), "v2/M", "id=M\nname=v2\n");
        install(&paths, &t.path().join("v1"), "local:v1").unwrap();
        install(&paths, &t.path().join("v2"), "local:v2").unwrap();
        install(&paths, &t.path().join("v2"), "local:v2").unwrap(); // same content again
        rollback(&paths, "M").unwrap();
        assert!(fs::read_to_string(paths.mods().join("M/mod.info")).unwrap().contains("v1"));
        assert!(!paths.zomboid.join(".pzmm-staging").exists(), "staged beside mods, then gone");
    }

    #[test]
    fn prunes_oldest_backups_first() {
        let (_t, paths) = temp_paths();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        for (id, bytes, aged) in [("Old", 600, true), ("New", 500, false)] {
            let d = paths.data.join("backups").join(id);
            fs::create_dir_all(&d).unwrap();
            let f = d.join("big.bin");
            fs::write(&f, vec![0u8; bytes]).unwrap();
            if aged {
                fs::File::options().write(true).open(&f).unwrap().set_modified(old).unwrap();
            }
        }
        assert_eq!(backups_size(&paths), 1100);
        assert_eq!(prune_backups(&paths, 2000).unwrap(), 0, "under the cap: nothing goes");
        assert_eq!(prune_backups(&paths, 1000).unwrap(), 1);
        assert!(!paths.data.join("backups/Old").exists() && paths.data.join("backups/New").exists());
        clear_backups(&paths).unwrap();
        assert_eq!(backups_size(&paths), 0);
    }

    #[test]
    fn tags_are_cleaned_and_removed_when_empty() {
        let (_t, paths) = temp_paths();
        assert!(tags(&paths).is_empty());
        let all = set_tags(&paths, "A", &[" Guns ".into(), "guns".into(), "".into(), "Must have".into()]).unwrap();
        assert_eq!(all["A"], ["Guns", "Must have"]);
        set_tags(&paths, "B", &["Maps".into()]).unwrap();
        assert_eq!(set_tags(&paths, "A", &[]).unwrap().keys().collect::<Vec<_>>(), ["B"]);
        assert_eq!(tags(&paths)["B"], ["Maps"], "kept on disk");
    }

    #[test]
    fn migrates_v01_index() {
        let (_t, paths) = temp_paths();
        fs::create_dir_all(paths.mods()).unwrap();
        fs::write(paths.mods().join("pzmm.json"), r#"{"ModA": "workshop:1"}"#).unwrap();
        assert_eq!(load(&paths).unwrap()["ModA"].source, "workshop:1");
    }
}
