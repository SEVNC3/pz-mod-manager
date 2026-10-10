//! Saved games: each keeps the mods it was played with in `Saves/<mode>/<save>/mods.txt`,
//! in the same format as `mods/default.txt`.

use std::fs;

use super::{Paths, read_default};

#[derive(Debug, serde::Serialize)]
pub struct Save {
    pub mode: String,
    pub name: String,
    pub mods: Vec<String>,
    /// Last played (seconds since 1970), for sorting.
    pub modified: u64,
}

/// Every save that records its mods, most recently played first.
pub fn list(paths: &Paths) -> Vec<Save> {
    let mut out = vec![];
    for mode in fs::read_dir(paths.zomboid.join("Saves")).into_iter().flatten().flatten() {
        for save in fs::read_dir(mode.path()).into_iter().flatten().flatten() {
            let file = save.path().join("mods.txt");
            let Ok(meta) = fs::metadata(&file) else { continue };
            let modified = fs::metadata(save.path()).and_then(|m| m.modified()).or_else(|_| meta.modified());
            out.push(Save {
                mode: mode.file_name().to_string_lossy().into_owned(),
                name: save.file_name().to_string_lossy().into_owned(),
                mods: read_default(&file).mods,
                modified: modified.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()),
            });
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.modified));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_saves_with_mods() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().into(), data: t.path().join("data") };
        let save = t.path().join("Saves/Sandbox/2026-10-10_12-00-00");
        fs::create_dir_all(&save).unwrap();
        fs::write(save.join("mods.txt"), "VERSION = 1,\nmods\n{\n    mod = \\ModA,\n}\nmaps\n{\n}\n").unwrap();
        fs::create_dir_all(t.path().join("Saves/Sandbox/no-mods-file")).unwrap();
        let saves = list(&paths);
        assert_eq!(saves.len(), 1);
        assert_eq!((saves[0].mode.as_str(), saves[0].mods.clone()), ("Sandbox", vec!["ModA".to_string()]));
    }
}
