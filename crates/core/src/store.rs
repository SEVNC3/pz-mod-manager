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
    /// The player holds its updates: PZMM never updates it (see `sources::fetch`), short of an Update anyway.
    #[serde(default)]
    pub held: bool,
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
    Ok(v1.into_iter().map(|(id, source)| (id, Entry { source, installed_at: 0, held: false })).collect())
}

pub fn save(paths: &Paths, index: &Index) -> Result<()> {
    fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&index_path(paths), serde_json::to_string_pretty(index)?)?;
    installed_for_game(paths, index);
    held_for_game(paths, index);
    Ok(())
}

/// The mods whose updates are held, for the PZMM - Client mod: `Zomboid/Lua/pzmm/held.txt`, a mod id per line, where
/// that folder is.
pub fn held_for_game(paths: &Paths, index: &Index) {
    let dir = pz::bridge::dir(paths);
    if dir.is_dir() {
        let text: String = index.iter().filter(|(_, e)| e.held).map(|(id, _)| format!("{id}\n")).collect();
        pz::bridge::write_for_game(&dir.join("held.txt"), text);
    }
}

/// Hold (`on`) or allow the updates of mods PZMM installed, with the other mods of their source (they update
/// together). Returns the ids changed; those PZMM didn't install aren't held, nor local ones (they never update).
pub fn set_held(paths: &Paths, ids: &[String], on: bool) -> Result<Vec<String>> {
    let mut index = load(paths)?;
    let sources: Vec<String> =
        ids.iter().filter_map(|id| index.get(id)).map(|e| e.source.clone()).filter(|s| s.starts_with("workshop:") || s.starts_with("github:")).collect();
    let mut changed = vec![];
    for (id, e) in index.iter_mut().filter(|(_, e)| sources.contains(&e.source) && e.held != on) {
        e.held = on;
        changed.push(id.clone());
    }
    if !changed.is_empty() {
        save(paths, &index)?;
    }
    Ok(changed)
}

/// The player subscribed to Workshop `item` again through PZMM: a held copy of it stops being held, so Steam's copy wins
/// and the next scan removes PZMM's once Steam's is there (never before: the mod is never gone meanwhile). Returns the
/// mod ids that were held.
pub fn subscribed_again(paths: &Paths, item: &str) -> Result<Vec<String>> {
    let source = format!("workshop:{item}");
    let ids: Vec<String> = load(paths)?.into_iter().filter(|(_, e)| e.source == source && e.held).map(|(id, _)| id).collect();
    set_held(paths, &ids, false)
}

/// When PZMM installed each mod, for the PZMM - Client mod's "Date installed" sort: `Zomboid/Lua/pzmm/installed.txt`,
/// a `<mod id>\t<seconds>\t<Workshop item>\t<source>` line per mod, where that folder is.
pub fn installed_for_game(paths: &Paths, index: &Index) {
    let dir = pz::bridge::dir(paths);
    if dir.is_dir() {
        // and its Workshop item (empty when it isn't from one): the game sees no Workshop ID for a mod in Zomboid/mods;
        // then the source as installed.json has it, so the game tells GitHub mods (they can be held) from local ones
        let text: String = index
            .iter()
            .map(|(id, e)| {
                let source = e.source.replace(['\t', '\r', '\n'], " ");
                format!("{id}\t{}\t{}\t{source}\n", crate::sources::installed_at(paths, id, e), e.source.strip_prefix("workshop:").unwrap_or(""))
            })
            .collect();
        pz::bridge::write_for_game(&dir.join("installed.txt"), &text);
    }
}

/// When a mod first arrived on this PC, as far as PZMM can tell: the earliest of its first install or subscription in
/// the download history, PZMM's install time and its folder's creation (0s don't count); 0 when none is known. An
/// update or a fresh Steam download can make the folder newer, which is why the earliest wins.
pub fn first_seen(history_first: Option<u64>, installed_at: u64, created: u64) -> u64 {
    [history_first.unwrap_or(0), installed_at, created].into_iter().filter(|t| *t > 0).min().unwrap_or(0)
}

/// Each mod's first install or subscription in the download history (oldest record), by mod id.
pub fn history_first(paths: &Paths) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    for r in crate::history::list(paths).into_iter().filter(|r| r.action == "installed" || r.action == "subscribed") {
        out.insert(r.id, r.at); // newest first: the last one kept is the oldest
    }
    out
}

/// When each mod first arrived (see [`first_seen`]), for the PZMM - Client mod's "First seen":
/// `Zomboid/Lua/pzmm/firstseen.txt`, a `<mod id>\t<seconds>` line per mod (unknown ones left out), where that folder is.
pub fn first_seen_for_game(paths: &Paths, seen: &BTreeMap<String, u64>) {
    let dir = pz::bridge::dir(paths);
    if dir.is_dir() {
        let text: String = seen.iter().filter(|(_, t)| **t > 0).map(|(id, t)| format!("{id}\t{t}\n")).collect();
        pz::bridge::write_for_game(&dir.join("firstseen.txt"), &text);
    }
}

/// [`first_seen_for_game`] from what's quick to read (the download history and PZMM's install times; the folders'
/// creation waits for the stats run): an earlier time the file already has stays.
pub fn first_seen_known_for_game(paths: &Paths) {
    let file = pz::bridge::dir(paths).join("firstseen.txt");
    let had: std::collections::HashMap<String, u64> =
        std::fs::read_to_string(&file).unwrap_or_default().lines().filter_map(|l| l.split_once('\t')).filter_map(|(id, t)| Some((id.to_string(), t.trim().parse().ok()?))).collect();
    let (index, first) = (load(paths).unwrap_or_default(), history_first(paths));
    let ids: std::collections::BTreeSet<&String> = index.keys().chain(first.keys()).chain(had.keys()).collect();
    let seen = ids.into_iter().map(|id| (id.clone(), first_seen(first.get(id).copied(), index.get(id).map_or(0, |e| e.installed_at), had.get(id).copied().unwrap_or(0)))).collect();
    first_seen_for_game(paths, &seen);
}

/// The player's tags for the PZMM - Client mod: `Zomboid/Lua/pzmm/tags.txt`, a `<mod id>\t<tag>;<tag>` line per mod
/// (tags never hold `;` or tabs there), where that folder is.
pub fn tags_for_game(paths: &Paths, tags: &Tags) {
    let dir = pz::bridge::dir(paths);
    if dir.is_dir() {
        let one = |t: &str| t.replace([';', '\t', '\r', '\n'], " ");
        let text: String = tags.iter().map(|(id, list)| format!("{id}\t{}\n", list.iter().map(|t| one(t)).collect::<Vec<_>>().join(";"))).collect();
        pz::bridge::write_for_game(&dir.join("tags.txt"), &text);
    }
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

/// Copy one mod's files into `dest`: staged next to it and swapped in, the folder it replaces kept
/// as the rollback backup. Returns whether it replaced a version already there.
fn swap_in(paths: &Paths, m: &pz::FoundMod, dest: &Path) -> Result<bool> {
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
        // renamed aside on its own drive (a rename is whole or nothing), the new copy swapped in, then the
        // old one moved to the backups, which may be on another drive (a copy there can stop halfway)
        let aside = staging.with_file_name(format!("{}.old", m.id));
        if aside.exists() {
            fs::remove_dir_all(&aside)?;
        }
        let backup = backup_dir(paths, &m.id)?;
        fs::rename(dest, &aside).with_context(|| format!("moving {} aside (close the game and try again)", dest.display()))?;
        if let Err(e) = fs::rename(&staging, dest) {
            let _ = fs::rename(&aside, dest);
            return Err(e).with_context(|| format!("moving {} into place", m.id));
        }
        // the version before this one was the backup: replaced now the new one is in
        if backup.exists()
            && let Err(e) = fs::remove_dir_all(&backup)
        {
            crate::log!("error: replacing the backup of {}: {e:#}", m.id);
        }
        if let Err(e) = move_dir(&aside, &backup) {
            crate::log!("error: keeping the old version of {} as a backup: {e:#}", m.id);
            // a copy cut short is no version to roll back to
            let _ = fs::remove_dir_all(&backup);
            let _ = fs::remove_dir_all(&aside);
        }
    } else {
        move_dir(&staging, dest).with_context(|| format!("moving {} into place", m.id))?;
    }
    let _ = fs::remove_dir(staging.parent().unwrap_or(&staging)); // when empty
    Ok(update)
}

/// Copy every mod found under `root` into the game's mods folder. A mod already there is
/// replaced only if PZMM installed it; its previous version becomes the rollback backup
/// (kept as is when the new copy is identical). Each mod is staged next to its final
/// folder and swapped in, then recorded, so a failure never leaves a half-copied mod.
/// Mods that can't be installed are logged and skipped. Returns the installed ids.
pub fn install(paths: &Paths, root: &Path, source: &str) -> Result<Vec<String>> {
    put_in(paths, root, source, false, false)
}

/// [`install`] as an update: a mod of the item the player removed (its newest record) stays out.
pub fn update(paths: &Paths, root: &Path, source: &str) -> Result<Vec<String>> {
    put_in(paths, root, source, true, false)
}

/// [`install`] of only the mods PZMM has no entry for: those it has (held ones, say) are left exactly as they are.
pub fn install_new(paths: &Paths, root: &Path, source: &str) -> Result<Vec<String>> {
    put_in(paths, root, source, false, true)
}

fn put_in(paths: &Paths, root: &Path, source: &str, update: bool, only_new: bool) -> Result<Vec<String>> {
    let found = pz::find_mods(root, 8);
    let history = if update { crate::history::list(paths) } else { vec![] };
    if found.is_empty() {
        bail!("{source}: no mod.info found");
    }
    let mut index = load(paths)?;
    let mut ids: Vec<String> = vec![];
    let mut local: Option<Vec<pz::FoundMod>> = None; // read once, when a fresh install needs it
    let mods = paths.mods();
    for m in &found {
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
        // Windows folders ignore case: an author changing only the id's case is still this entry
        let known = index.keys().find(|k| k.eq_ignore_ascii_case(&m.id)).cloned();
        // a copy of this source on disk stays exactly as it is; an entry whose folder is gone is copied again
        if only_new && known.as_ref().is_some_and(|k| index[k].source == source && mod_dir(paths, k).is_ok_and(|d| d.is_dir())) {
            continue;
        }
        if known.is_none() && history.iter().find(|r| r.id.eq_ignore_ascii_case(&m.id)).is_some_and(|r| r.action == "removed" && r.source == source) {
            crate::say!("removedStays", "{id} stays out: you removed it ({source})", id = m.id, source = source);
            continue;
        }
        if dest.exists() && known.is_none() {
            crate::say!(
                "notOursInMods",
                "error: {id} is already in {dir} and wasn't installed by PZMM; left it alone. Remove that folder to install this copy.",
                id = m.id,
                dir = mods.display()
            );
            continue;
        }
        // two Workshop items shipping the same id: the one installed first keeps it (a GitHub or
        // local install is the player's own pick, so it may replace)
        if let Some(had) = known.as_ref().map(|k| &index[k].source)
            && had != source
            && had.starts_with("workshop:")
            && source.starts_with("workshop:")
        {
            crate::say!("otherSource", "error: {id} was installed from {had}; left it alone instead of replacing it with {source}'s copy", id = m.id, had = had, source = source);
            continue;
        }
        if let Some(had) = known.as_ref().map(|k| &index[k].source)
            && !had.starts_with("workshop:")
            && source.starts_with("workshop:")
        {
            crate::say!("otherSource", "error: {id} was installed from {had}; left it alone instead of replacing it with {source}'s copy", id = m.id, had = had, source = source);
            continue;
        }
        // a fresh install beside a hand-installed folder declaring the same id would load it twice
        if known.is_none() {
            let local = local.get_or_insert_with(|| pz::local_mods(paths));
            let folder = |l: &pz::FoundMod| l.dir.strip_prefix(&mods).ok().and_then(|r| r.iter().next()).map(|f| f.to_string_lossy().into_owned());
            if let Some(other) = local.iter().filter(|l| l.id.eq_ignore_ascii_case(&m.id)).find_map(|l| folder(l).filter(|f| *f != m.id)) {
                crate::say!("sameIdElsewhere", "error: {id} is already in {dir}, which PZMM didn't install; left it alone. Remove that folder to install this copy.", id = m.id, dir = mods.join(other).display());
                continue;
            }
        }
        // where folders do mind case, the old spelling's folder becomes the new one, so it's updated, not doubled
        if let Some(k) = known.as_ref().filter(|k| **k != m.id && !dest.exists())
            && let Ok(old) = mod_dir(paths, k)
            && old.exists()
            && let Err(e) = fs::rename(&old, &dest)
        {
            crate::log!("error: {source}: renaming {} to {}: {e:#}", old.display(), m.id);
            continue;
        }
        let mut noted = None;
        if dest.exists() && same_tree(&m.dir, &dest) {
            crate::say!("alreadyCurrent", "{id} is already up to date ({source})", id = m.id, source = source);
        } else {
            match swap_in(paths, m, &dest) {
                Ok(update) => {
                    if update {
                        crate::say!("updated", "updated {id} ({source})", id = m.id, source = source);
                    } else {
                        crate::say!("installed", "installed {id} ({source})", id = m.id, source = source);
                    }
                    noted = Some(update);
                }
                Err(e) => {
                    // the mods before it are in and indexed: they're still returned
                    crate::log!("error: {source}: {e:#}");
                    continue;
                }
            }
        }
        // an Update anyway keeps it held
        let held = known.as_ref().and_then(|k| index.remove(k)).is_some_and(|e| e.held);
        index.insert(m.id.clone(), Entry { source: source.to_string(), installed_at: now(), held });
        if let Err(e) = save(paths, &index) {
            crate::log!("error: {source}: recording {}: {e:#}", m.id);
            continue;
        }
        // noted once it's in the index
        if let Some(update) = noted {
            crate::history::add(paths, &m.id, &m.name, source, if update { "updated" } else { "installed" });
        }
        ids.push(m.id.clone());
    }
    // a mod the source no longer ships (dropped or renamed) would be flagged for update forever: its
    // time moves with the source's, and its files stay
    if !ids.is_empty() {
        let gone: Vec<String> =
            index.iter().filter(|(id, e)| e.source == source && !found.iter().any(|m| m.id.eq_ignore_ascii_case(id))).map(|(id, _)| id.clone()).collect();
        for id in &gone {
            if let Some(e) = index.get_mut(id) {
                e.installed_at = now();
            }
            crate::say!("noLongerInSource", "{id} is no longer in {source}; its files were left in place", id = id, source = source);
        }
        if !gone.is_empty()
            && let Err(e) = save(paths, &index)
        {
            crate::log!("error: {source}: recording {}: {e:#}", gone.join(", "));
        }
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
    // aside on the mod's own drive: a rename, never a copy that can stop halfway
    let mods = fs::canonicalize(paths.mods()).unwrap_or_else(|_| paths.mods());
    let tmp = mods.parent().unwrap_or(&paths.zomboid).join(".pzmm-staging").join(format!("{id}.swap"));
    if tmp.exists() {
        bail!("{id}: an earlier rollback didn't finish; the newer version is in {}", tmp.display());
    }
    if dest.exists() {
        fs::create_dir_all(tmp.parent().unwrap_or(&tmp))?;
        fs::rename(&dest, &tmp).with_context(|| format!("moving {} aside (close the game and try again)", dest.display()))?;
    }
    if let Err(e) = move_dir(&backup, &dest) {
        // the version in use goes back where it was
        let _ = fs::remove_dir_all(&dest);
        let _ = fs::rename(&tmp, &dest);
        return Err(e);
    }
    if tmp.exists()
        && let Err(e) = move_dir(&tmp, &backup)
    {
        // rolling back again would re-apply the update: a copy cut short can't
        let _ = fs::remove_dir_all(&backup);
        return Err(e).with_context(|| format!("{id} is rolled back; the newer version is in {}", tmp.display()));
    }
    let _ = fs::remove_dir(tmp.parent().unwrap_or(&tmp)); // when empty
    // the update only changed the id's case: the entry (and, where folders mind case, the folders) go back to the old one
    if let Some(old) = pz::find_mods(&dest, 8).into_iter().map(|m| m.id).find(|o| o != id && o.eq_ignore_ascii_case(id)) {
        let rekey = || -> Result<()> {
            for (from, to) in [(dest, mod_dir(paths, &old)?), (backup, backup_dir(paths, &old)?)] {
                if from.exists() && !to.exists() {
                    fs::rename(&from, &to).with_context(|| format!("renaming {} to {old}", from.display()))?;
                }
            }
            let mut index = load(paths)?;
            if let Some(e) = index.remove(id) {
                index.insert(old.clone(), e);
                save(paths, &index)?;
            }
            Ok(())
        };
        if let Err(e) = rekey() {
            crate::log!("error: {id} is rolled back, but not yet known by its old id: {e:#}");
        }
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

/// Delete the oldest backups until all of them fit in `cap` bytes, never those of `keep` (just made: their
/// files keep the author's old times, so they'd rank oldest). Returns how many went.
pub fn prune_backups(paths: &Paths, cap: u64, keep: &[String]) -> Result<usize> {
    let mut all = backups(paths);
    all.retain(|(dir, _)| !keep.iter().any(|id| backup_dir(paths, id).is_ok_and(|b| b == *dir)));
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
    tags_for_game(paths, &all);
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

/// [`scan`]'s rows, changing nothing (a PZMM copy of a subscribed item isn't removed): outside the data lock.
pub fn rows(paths: &Paths) -> Result<BTreeMap<String, Row>> {
    let index = load(paths)?;
    let mut out: BTreeMap<String, Row> = pz::workshop_dir().as_deref().map(|ws| workshop_rows(ws).map(|r| (r.id.clone(), r)).collect()).unwrap_or_default();
    for m in pz::local_mods(paths) {
        let source = index.get(&m.id).map_or("manual".to_string(), |e| e.source.clone());
        out.insert(m.id.clone(), Row::from_found(m, source));
    }
    add_staged(paths, &mut out);
    Ok(out)
}

/// The mods staged for upload (see [`pz::staged_mods`]) that nothing else provides: "staged", which PZMM never
/// removes or updates.
fn add_staged(paths: &Paths, out: &mut BTreeMap<String, Row>) {
    for m in pz::staged_mods(paths) {
        if !out.contains_key(&m.id) {
            out.insert(m.id.clone(), Row::from_found(m, "staged".into()));
        }
    }
}

/// [`scan`]'s mod ids, changing nothing: for callers outside the data lock.
pub fn ids(paths: &Paths) -> Result<std::collections::HashSet<String>> {
    let mut out: std::collections::HashSet<String> = pz::workshop_dir().as_deref().map(|ws| workshop_rows(ws).map(|r| r.id).collect()).unwrap_or_default();
    out.extend(pz::local_mods(paths).into_iter().map(|m| m.id));
    out.extend(pz::staged_mods(paths).into_iter().map(|m| m.id));
    Ok(out)
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
        // never a held copy: Steam keeps an unsubscribed item's folder until the game quits, then deletes it, so
        // removing ours on seeing Steam's would lose the mod (Check setup's twoCopies says there are two meanwhile).
        // Subscribing again through PZMM lets Steam's copy win: see [`subscribed_again`]
        let held = index.get(&m.id).is_some_and(|e| e.held);
        if subscribed && !held && remove(paths, std::slice::from_ref(&m.id), recycle)?.is_empty() {
            crate::say!("subscribedCopyRemoved", "{id}: you subscribed in Steam; PZMM's copy was removed", id = m.id);
            continue;
        }
        out.insert(m.id.clone(), Row::from_found(m, source));
    }
    add_staged(paths, &mut out);
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
    fn a_failing_mod_keeps_the_others() {
        let (t, paths) = temp_paths();
        for id in ["A", "B", "C"] {
            mod_tree(t.path(), &format!("src/{id}"), &format!("id={id}
"));
        }
        // B's staging spot is taken by a file, so B alone fails
        fs::create_dir_all(paths.zomboid.join(".pzmm-staging")).unwrap();
        fs::write(paths.zomboid.join(".pzmm-staging/B"), "x").unwrap();
        let mut ids = install(&paths, &t.path().join("src"), "workshop:1").unwrap();
        ids.sort();
        assert_eq!(ids, ["A", "C"]);
        assert!(load(&paths).unwrap().contains_key("C"));
    }

    #[test]
    fn other_items_and_hand_copies_keep_their_id() {
        let (t, paths) = temp_paths();
        mod_tree(t.path(), "one/X", "id=X
name=one
");
        mod_tree(t.path(), "two/X", "id=X
name=two
");
        install(&paths, &t.path().join("one"), "workshop:1").unwrap();
        assert!(install(&paths, &t.path().join("two"), "workshop:2").unwrap().is_empty(), "another item's copy is skipped");
        assert_eq!(load(&paths).unwrap()["X"].source, "workshop:1");
        assert_eq!(install(&paths, &t.path().join("two"), "local:two").unwrap(), ["X"], "the player's own pick replaces");
        assert!(install(&paths, &t.path().join("one"), "workshop:1").unwrap().is_empty(), "a Workshop copy doesn't replace it back");
        // a hand-installed folder of another name declaring the id
        mod_tree(&paths.mods(), "Folder", "id=y
");
        mod_tree(t.path(), "dl/Y", "id=Y
");
        assert!(install(&paths, &t.path().join("dl"), "workshop:3").unwrap().is_empty());
        assert!(!paths.mods().join("Y").exists());
        assert!(!load(&paths).unwrap().contains_key("Y"));
    }

    #[test]
    fn case_only_id_change_updates_the_entry() {
        let (t, paths) = temp_paths();
        mod_tree(t.path(), "v1/modx", "id=modx
name=v1
");
        mod_tree(t.path(), "v2/ModX", "id=ModX
name=v2
");
        install(&paths, &t.path().join("v1"), "workshop:1").unwrap();
        assert_eq!(install(&paths, &t.path().join("v2"), "workshop:1").unwrap(), ["ModX"]);
        let index = load(&paths).unwrap();
        assert!(index.contains_key("ModX") && !index.contains_key("modx"));
        assert!(fs::read_to_string(paths.mods().join("ModX/mod.info")).unwrap().contains("v2"));
        // rolled back: the old id is the entry again
        rollback(&paths, "ModX").unwrap();
        let index = load(&paths).unwrap();
        assert!(index.contains_key("modx") && !index.contains_key("ModX"));
        assert!(fs::read_to_string(mod_dir(&paths, "modx").unwrap().join("mod.info")).unwrap().contains("v1"));
    }

    #[test]
    fn first_seen_is_the_earliest_known() {
        assert_eq!(first_seen(Some(100), 300, 200), 100, "the history's first install");
        assert_eq!(first_seen(None, 300, 200), 200, "an update made the install time newer than the folder");
        assert_eq!(first_seen(None, 0, 200), 200, "a subscription: only the folder");
        assert_eq!(first_seen(None, 300, 0), 300, "no folder time on this system");
        assert_eq!(first_seen(None, 0, 0), 0, "unknown: hidden");
        // the quick write keeps the stats run's earlier times
        let (_t, paths) = temp_paths();
        let file = paths.zomboid.join("Lua/pzmm/firstseen.txt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "A\t50\n").unwrap();
        crate::history::add(&paths, "A", "", "workshop:1", "installed");
        crate::history::add(&paths, "B", "", "workshop:2", "installed");
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("A\t50\nB\t") && text.lines().count() == 2, "{text}");
    }

    #[test]
    fn a_staged_mod_counts_as_installed() {
        let (_t, paths) = temp_paths();
        mod_tree(&paths.zomboid, "Workshop/MyItem/Contents/mods/Mine/42", "id=Mine
");
        mod_tree(&paths.zomboid, "Workshop/MyItem/Contents/mods/Both/42", "id=Both
");
        mod_tree(&paths.mods(), "Both", "id=Both
");
        let rows = scan_in(&paths, None, false).unwrap();
        assert_eq!(rows["Mine"].source, "staged");
        assert_eq!(rows["Both"].source, "manual", "a copy in mods wins");
        assert!(ids(&paths).unwrap().contains("Mine"));
    }

    #[test]
    fn an_update_leaves_out_a_removed_mod() {
        let (t, paths) = temp_paths();
        mod_tree(t.path(), "dl/A", "id=A
");
        mod_tree(t.path(), "dl/B", "id=B
");
        install(&paths, &t.path().join("dl"), "workshop:7").unwrap();
        remove(&paths, &["A".to_string()], false).unwrap();
        assert_eq!(update(&paths, &t.path().join("dl"), "workshop:7").unwrap(), ["B"], "the update skips A");
        assert!(!paths.mods().join("A").exists());
        assert_eq!(install(&paths, &t.path().join("dl"), "workshop:7").unwrap(), ["A", "B"], "a reinstall brings it back");
    }

    #[test]
    fn a_dropped_mod_stops_waiting_for_updates() {
        let (t, paths) = temp_paths();
        mod_tree(t.path(), "v1/A", "id=A
");
        mod_tree(t.path(), "v1/B", "id=B
");
        mod_tree(t.path(), "v2/A", "id=A
name=v2
");
        install(&paths, &t.path().join("v1"), "workshop:5").unwrap();
        let mut index = load(&paths).unwrap();
        index.get_mut("B").unwrap().installed_at = 1;
        save(&paths, &index).unwrap();
        install(&paths, &t.path().join("v2"), "workshop:5").unwrap();
        assert!(load(&paths).unwrap()["B"].installed_at > 1);
        assert!(paths.mods().join("B/mod.info").exists(), "its files stay");
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
        assert_eq!(prune_backups(&paths, 2000, &[]).unwrap(), 0, "under the cap: nothing goes");
        assert_eq!(prune_backups(&paths, 1000, &[]).unwrap(), 1);
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
    fn held_is_kept_and_told_to_the_game() {
        let (t, paths) = temp_paths();
        fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        // an index from before holds reads as not held
        fs::create_dir_all(&paths.data).unwrap();
        fs::write(paths.data.join("installed.json"), r#"{"A": {"source": "workshop:1", "installed_at": 5}}"#).unwrap();
        assert!(!load(&paths).unwrap()["A"].held);
        for (id, item) in [("A", "1"), ("B", "1"), ("C", "2")] {
            mod_tree(t.path(), &format!("src{item}/{id}"), &format!("id={id}\n"));
        }
        install(&paths, &t.path().join("src1"), "workshop:1").unwrap();
        install(&paths, &t.path().join("src2"), "workshop:2").unwrap();
        // the item's other mod is held with it; a mod PZMM didn't install isn't
        assert_eq!(set_held(&paths, &["A".into(), "Nope".into()], true).unwrap(), ["A", "B"]);
        let index = load(&paths).unwrap();
        assert!(index["A"].held && index["B"].held && !index["C"].held);
        assert!(fs::read_to_string(paths.data.join("installed.json")).unwrap().contains(r#""held": true"#));
        assert_eq!(fs::read_to_string(paths.zomboid.join("Lua/pzmm/held.txt")).unwrap(), "A\nB\n");
        // installed.txt: id, install time, Workshop item, source
        let at = load(&paths).unwrap()["A"].installed_at;
        assert!(fs::read_to_string(paths.zomboid.join("Lua/pzmm/installed.txt")).unwrap().starts_with(&format!("A\t{at}\t1\tworkshop:1\n")));
        let gh: Index = [("G".to_string(), Entry { source: "github:owner/repo".into(), installed_at: 7, held: false })].into();
        installed_for_game(&paths, &gh);
        assert_eq!(fs::read_to_string(paths.zomboid.join("Lua/pzmm/installed.txt")).unwrap(), "G\t7\t\tgithub:owner/repo\n");
        // an Update anyway keeps it held
        mod_tree(t.path(), "src1/A", "id=A\nname=v2\n");
        update(&paths, &t.path().join("src1"), "workshop:1").unwrap();
        assert!(load(&paths).unwrap()["A"].held);
        set_held(&paths, &["B".into()], false).unwrap();
        assert_eq!(fs::read_to_string(paths.zomboid.join("Lua/pzmm/held.txt")).unwrap(), "");
    }

    #[test]
    fn a_held_copy_of_a_subscribed_item_stays() {
        let (t, paths) = temp_paths();
        let ws = t.path().join("ws");
        mod_tree(&ws, "42/mods/ModA", "id=ModA\n");
        mod_tree(t.path(), "src/ModA", "id=ModA\n");
        install(&paths, &t.path().join("src"), "workshop:42").unwrap();
        set_held(&paths, &["ModA".into()], true).unwrap();
        let rows = scan_in(&paths, Some(&ws), false).unwrap();
        assert_eq!(rows["ModA"].source, "workshop:42", "PZMM's held copy, not removed while the unsubscribe lands");
        assert!(paths.mods().join("ModA").exists());
        // at any age (Steam keeps the folder while the game runs, then deletes it)
        crate::history::clear(&paths, None);
        let mut index = load(&paths).unwrap();
        index.get_mut("ModA").unwrap().installed_at = 1;
        save(&paths, &index).unwrap();
        assert_eq!(scan_in(&paths, Some(&ws), false).unwrap()["ModA"].source, "workshop:42");
        assert!(paths.mods().join("ModA").exists() && load(&paths).unwrap()["ModA"].held);
        // subscribed again through PZMM: the hold ends, and Steam's copy wins
        assert_eq!(subscribed_again(&paths, "42").unwrap(), ["ModA"]);
        assert_eq!(scan_in(&paths, Some(&ws), false).unwrap()["ModA"].source, "steam:42");
        assert!(!paths.mods().join("ModA").exists() && !load(&paths).unwrap().contains_key("ModA"));
    }

    #[test]
    fn local_mods_are_never_held() {
        let (t, paths) = temp_paths();
        mod_tree(t.path(), "dl/L", "id=L\n");
        install(&paths, &t.path().join("dl"), "local:dl").unwrap();
        assert!(set_held(&paths, &["L".into()], true).unwrap().is_empty());
        assert!(!load(&paths).unwrap()["L"].held);
    }

    #[test]
    fn migrates_v01_index() {
        let (_t, paths) = temp_paths();
        fs::create_dir_all(paths.mods()).unwrap();
        fs::write(paths.mods().join("pzmm.json"), r#"{"ModA": "workshop:1"}"#).unwrap();
        assert_eq!(load(&paths).unwrap()["ModA"].source, "workshop:1");
    }
}
