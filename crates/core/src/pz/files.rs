//! What a mod's files do to the game: which files override other mods (by load order) and
//! which maps it adds.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::is_version_dir;

/// Game-visible paths a mod provides under `media/`, B42 `42/` and `common/` folded together.
/// The mod folder plus its version folders (`42/`, `42.13/`, `common/`): where `media/` can live.
pub(crate) fn content_roots(mod_dir: &Path) -> Vec<PathBuf> {
    std::iter::once(mod_dir.to_path_buf())
        .chain(
            fs::read_dir(mod_dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name().to_str().is_some_and(is_version_dir))
                .map(|e| e.path()),
        )
        .collect()
}

/// A cheap stamp of a mod folder: the modified times of it, its version folders and their `media`.
/// Installs and Workshop updates replace the folder, so these change with it.
fn stamp(dir: &Path) -> Vec<Option<SystemTime>> {
    content_roots(dir).into_iter().flat_map(|r| [r.join("media"), r]).map(|p| fs::metadata(p).and_then(|m| m.modified()).ok()).collect()
}

type Cache<T> = Mutex<BTreeMap<PathBuf, (Vec<Option<SystemTime>>, T)>>;

static MEDIA: Cache<Arc<BTreeSet<String>>> = Mutex::new(BTreeMap::new());
static STATS: Cache<Stats> = Mutex::new(BTreeMap::new());

/// `walk(dir)`, kept until the folder's [`stamp`] changes or [`forget`]: overlaps, sizes and
/// Performance look at every enabled mod's files each time.
fn kept<T: Clone>(cache: &Cache<T>, dir: &Path, walk: impl FnOnce(&Path) -> T) -> T {
    let s = stamp(dir);
    if let Some((at, v)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(dir)
        && *at == s
    {
        return v.clone();
    }
    let v = walk(dir);
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(dir.to_path_buf(), (s, v.clone()));
    v
}

/// Drop what [`kept`] keeps (a reload: a file changed deep in a folder leaves its stamp as it was).
pub fn forget() {
    MEDIA.lock().unwrap_or_else(|e| e.into_inner()).clear();
    STATS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn media_files(mod_dir: &Path) -> Arc<BTreeSet<String>> {
    kept(&MEDIA, mod_dir, |d| Arc::new(walk_media(d)))
}

fn walk_media(mod_dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for root in content_roots(mod_dir) {
        let mut stack = vec![root.join("media")];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push(e.path());
                } else if let Ok(rel) = e.path().strip_prefix(&root) {
                    out.insert(rel.to_string_lossy().replace('\\', "/").to_lowercase());
                }
            }
        }
    }
    out
}

/// A shared-file relationship between the selected mod and another enabled mod.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Conflict {
    pub other: String,
    /// Number of files both provide.
    pub files: usize,
    /// True when the selected mod loads later, so its copies win.
    pub wins: bool,
    /// A few example paths.
    pub examples: Vec<String>,
}

/// Compare `mine` (the selected mod, at position `my_pos` in load order) with other
/// enabled mods `(id, dir, position)`.
pub fn conflicts(mine: &Path, my_pos: usize, others: &[(String, PathBuf, usize)]) -> Vec<Conflict> {
    let my_files = media_files(mine);
    let mut out: Vec<Conflict> = others
        .iter()
        .filter_map(|(id, dir, pos)| {
            let shared: Vec<String> = media_files(dir).intersection(&my_files).cloned().collect();
            (!shared.is_empty()).then(|| Conflict { other: id.clone(), files: shared.len(), wins: my_pos > *pos, examples: shared.into_iter().take(5).collect() })
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.files));
    out
}

/// Which mods a mod beats, and is beaten by, on shared files.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Overlap {
    pub wins_over: BTreeSet<String>,
    pub loses_to: BTreeSet<String>,
}

/// For enabled mods in load order (`(id, dir)`), who overrides whom: on each shared
/// path the mod loaded last wins. Mods with no shared files are absent.
pub fn overlaps(mods: &[(String, PathBuf)]) -> std::collections::HashMap<String, Overlap> {
    let mut owners: std::collections::HashMap<String, Vec<usize>> = Default::default();
    for (i, (_, dir)) in mods.iter().enumerate() {
        for f in media_files(dir).iter() {
            owners.entry(f.clone()).or_default().push(i);
        }
    }
    let mut out: std::collections::HashMap<String, Overlap> = Default::default();
    for idx in owners.values().filter(|v| v.len() > 1) {
        let winner = &mods[*idx.last().unwrap()].0;
        for &i in &idx[..idx.len() - 1] {
            let loser = &mods[i].0;
            out.entry(winner.clone()).or_default().wins_over.insert(loser.clone());
            out.entry(loser.clone()).or_default().loses_to.insert(winner.clone());
        }
    }
    out
}

/// A mod folder's total size in bytes, its newest file time and when the folder was made
/// (installed or subscribed; 0 where the system doesn't keep it), in seconds since 1970.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct Stats {
    pub size: u64,
    pub modified: u64,
    pub created: u64,
    /// When it first arrived on this PC, as far as PZMM can tell (see `store::first_seen`); 0 if unknown.
    pub first_seen: u64,
}

pub fn stats(dir: &Path) -> Stats {
    kept(&STATS, dir, walk_stats)
}

fn walk_stats(dir: &Path) -> Stats {
    let secs = |t: std::io::Result<std::time::SystemTime>| t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let mut s = Stats { created: fs::metadata(dir).map_or(0, |m| secs(m.created())), ..Default::default() };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                stack.push(e.path());
            } else if let Ok(meta) = e.metadata() {
                s.size += meta.len();
                s.modified = s.modified.max(secs(meta.modified()));
            }
        }
    }
    s
}

/// Languages a mod has texts in: its `media/lua/shared/Translate/<LANG>` folders (`EN`, `RU`, `PTBR`...).
pub fn languages(mod_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = content_roots(mod_dir)
        .into_iter()
        .flat_map(|root| fs::read_dir(root.join("media/lua/shared/Translate")).into_iter().flatten().flatten())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_uppercase())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Map folder names a mod adds (`media/maps/<Name>`).
pub fn maps(mod_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = content_roots(mod_dir)
        .into_iter()
        .flat_map(|root| fs::read_dir(root.join("media/maps")).into_iter().flatten().flatten())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "x").unwrap();
    }

    #[test]
    fn languages_from_translate_folders() {
        let t = tempfile::tempdir().unwrap();
        let m = t.path().join("M");
        touch(&m.join("media/lua/shared/Translate/RU/UI_RU.txt"));
        touch(&m.join("42/media/lua/shared/Translate/ptbr/UI.json"));
        touch(&m.join("common/media/lua/shared/Translate/RU/IG_UI.json"));
        touch(&m.join("media/lua/shared/Translate/stray.txt"));
        assert_eq!(languages(&m), ["PTBR", "RU"]);
        assert!(languages(&t.path().join("none")).is_empty());
    }

    #[test]
    fn finds_conflicts_and_maps() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("A");
        let b = t.path().join("B");
        touch(&a.join("42/media/lua/shared/Items.lua"));
        touch(&a.join("common/media/textures/x.png"));
        touch(&a.join("mod.info"));
        touch(&b.join("media/lua/shared/items.lua")); // B41 layout, different case: same game path
        touch(&b.join("media/maps/Riverside Plus/map.info"));
        assert_eq!(media_files(&a).len(), 2);
        assert_eq!(stats(&a).size, 3, "three one-byte files");

        let c = conflicts(&a, 0, &[("B".into(), b.clone(), 1)]);
        assert_eq!(c, [Conflict { other: "B".into(), files: 1, wins: false, examples: vec!["media/lua/shared/items.lua".into()] }]);
        assert!(conflicts(&a, 2, &[("B".into(), b.clone(), 1)])[0].wins);
        assert_eq!(maps(&b), ["Riverside Plus"]);
        let m = t.path().join("MapMod");
        touch(&m.join("42.13/media/maps/Riverside Plus/map.info"));
        touch(&m.join("common/media/maps/Bedford/map.info"));
        assert_eq!(maps(&m), ["Bedford", "Riverside Plus"], "any version folder");

        let c = t.path().join("C");
        touch(&c.join("media/lua/shared/Items.lua"));
        let o = overlaps(&[("A".into(), a.clone()), ("B".into(), b.clone()), ("C".into(), c.clone())]);
        assert_eq!(o["C"].wins_over.iter().collect::<Vec<_>>(), ["A", "B"]);
        assert!(o["C"].loses_to.is_empty());
        assert_eq!(o["A"].loses_to.iter().collect::<Vec<_>>(), ["C"]);
        assert!(o["A"].wins_over.is_empty(), "A loads first, so it beats no one");
        assert!(maps(&a).is_empty());

        // kept until the folder changes
        assert_eq!(stats(&c).size, 1);
        touch(&c.join("media/More.txt"));
        assert_eq!((media_files(&c).len(), stats(&c).size), (2, 2), "a new file is seen");
        touch(&c.join("media/lua/shared/Deep.lua"));
        assert_eq!(media_files(&c).len(), 2, "deep inside: kept");
        forget();
        assert_eq!(media_files(&c).len(), 3, "until a reload");
    }
}
