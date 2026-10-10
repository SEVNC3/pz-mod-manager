//! Mods PZMM installed: the index (`<data>/installed.json`), copies into the game's
//! mods folder, one-deep backups and rollback. Only indexed mods are ours to touch.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::pz::{self, Paths};
use crate::sources::steam_workshop;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// `workshop:<id>`, `github:<owner/repo[@ref]>` or `local:<path>`.
    pub source: String,
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
    Ok(v1.into_iter().map(|(id, source)| (id, Entry { source })).collect())
}

pub fn save(paths: &Paths, index: &Index) -> Result<()> {
    fs::create_dir_all(&paths.data)?;
    fs::write(index_path(paths), serde_json::to_string_pretty(index)?)?;
    Ok(())
}

fn folder_name(id: &str) -> String {
    id.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect()
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

/// Rename, falling back to copy + delete across drives.
fn move_dir(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(from, to).is_err() {
        copy_dir(from, to)?;
        fs::remove_dir_all(from)?;
    }
    Ok(())
}

fn backup_dir(paths: &Paths, id: &str) -> PathBuf {
    paths.data.join("backups").join(folder_name(id))
}

/// Copy every mod found under `root` into the game's mods folder. The version it
/// replaces is kept as a backup for `rollback`. Returns the installed mod ids.
pub fn install(paths: &Paths, root: &Path, source: &str) -> Result<Vec<String>> {
    let found = pz::find_mods(root, 8);
    if found.is_empty() {
        bail!("{source}: no mod.info found");
    }
    let mut index = load(paths)?;
    let mut ids = vec![];
    for m in found {
        let dest = paths.mods().join(folder_name(&m.id));
        if dest.exists() {
            let backup = backup_dir(paths, &m.id);
            if backup.exists() {
                fs::remove_dir_all(&backup)?;
            }
            move_dir(&dest, &backup)?;
        }
        copy_dir(&m.dir, &dest).with_context(|| format!("copying {} to {}", m.id, dest.display()))?;
        index.insert(m.id.clone(), Entry { source: source.to_string() });
        crate::log!("installed {} ({source})", m.id);
        ids.push(m.id);
    }
    save(paths, &index)?;
    Ok(ids)
}

pub fn has_backup(paths: &Paths, id: &str) -> bool {
    backup_dir(paths, id).exists()
}

/// Swap a mod back to the version it had before its last install/update.
pub fn rollback(paths: &Paths, id: &str) -> Result<()> {
    let backup = backup_dir(paths, id);
    if !backup.exists() {
        bail!("{id}: no previous version to restore");
    }
    let dest = paths.mods().join(folder_name(id));
    let tmp = paths.data.join("backups").join(format!("{}.swap", folder_name(id)));
    if dest.exists() {
        move_dir(&dest, &tmp)?;
    }
    move_dir(&backup, &dest)?;
    if tmp.exists() {
        move_dir(&tmp, &backup)?; // rolling back again re-applies the update
    }
    Ok(())
}

/// Delete mods PZMM installed. Returns ids it refused because they aren't ours.
pub fn remove(paths: &Paths, ids: &[String]) -> Result<Vec<String>> {
    let mut index = load(paths)?;
    let mut refused = vec![];
    for id in ids {
        if index.remove(id).is_some() {
            let dir = paths.mods().join(folder_name(id));
            if dir.exists() {
                fs::remove_dir_all(dir)?;
            }
        } else {
            refused.push(id.clone());
        }
    }
    save(paths, &index)?;
    Ok(refused)
}

pub struct Row {
    pub id: String,
    pub name: String,
    /// Index source, `steam:<item>` for Workshop subscriptions, or `manual`.
    pub source: String,
    pub dir: PathBuf,
    pub info: PathBuf,
}

/// Every mod the game can load: Steam subscriptions, then Zomboid/mods (wins on clashes).
pub fn scan(paths: &Paths) -> Result<BTreeMap<String, Row>> {
    let index = load(paths)?;
    let mut out = BTreeMap::new();
    if let Some(ws) = steam_workshop::content_dir(pz::STEAM_APP) {
        for m in pz::workshop_mods(&ws) {
            let item = m.dir.strip_prefix(&ws).ok().and_then(|r| r.iter().next()).map(|s| s.to_string_lossy().into_owned());
            let source = format!("steam:{}", item.unwrap_or_default());
            out.insert(m.id.clone(), Row { id: m.id, name: m.name, source, dir: m.dir, info: m.info });
        }
    }
    for m in pz::local_mods(paths) {
        let source = index.get(&m.id).map_or("manual".to_string(), |e| e.source.clone());
        out.insert(m.id.clone(), Row { id: m.id, name: m.name, source, dir: m.dir, info: m.info });
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
        assert_eq!(remove(&paths, &["RootMod".into(), "Manual".into()]).unwrap(), ["Manual"]);
        assert!(!paths.mods().join("RootMod").exists() && paths.mods().join("Manual").exists());
    }

    #[test]
    fn migrates_v01_index() {
        let (_t, paths) = temp_paths();
        fs::create_dir_all(paths.mods()).unwrap();
        fs::write(paths.mods().join("pzmm.json"), r#"{"ModA": "workshop:1"}"#).unwrap();
        assert_eq!(load(&paths).unwrap()["ModA"].source, "workshop:1");
    }
}
