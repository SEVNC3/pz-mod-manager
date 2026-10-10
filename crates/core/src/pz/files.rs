//! What a mod's files do to the game: which files override other mods (by load order) and
//! which maps it adds.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::is_version_dir;

/// Game-visible paths a mod provides under `media/`, B42 `42/` and `common/` folded together.
pub fn media_files(mod_dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let roots: Vec<PathBuf> = std::iter::once(mod_dir.to_path_buf())
        .chain(fs::read_dir(mod_dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| {
            p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(is_version_dir)
        }))
        .collect();
    for root in roots {
        let media = root.join("media");
        let mut stack = vec![media.clone()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(&root) {
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
    out.sort_by(|a, b| b.files.cmp(&a.files));
    out
}

/// Map folder names a mod adds (`media/maps/<Name>`).
pub fn maps(mod_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::iter::once(mod_dir.to_path_buf())
        .chain(["42", "common"].iter().map(|v| mod_dir.join(v)))
        .flat_map(|root| fs::read_dir(root.join("media/maps")).into_iter().flatten().flatten())
        .filter(|e| e.path().is_dir())
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

        let c = conflicts(&a, 0, &[("B".into(), b.clone(), 1)]);
        assert_eq!(c, [Conflict { other: "B".into(), files: 1, wins: false, examples: vec!["media/lua/shared/items.lua".into()] }]);
        assert!(conflicts(&a, 2, &[("B".into(), b.clone(), 1)])[0].wins);
        assert_eq!(maps(&b), ["Riverside Plus"]);
        assert!(maps(&a).is_empty());
    }
}
