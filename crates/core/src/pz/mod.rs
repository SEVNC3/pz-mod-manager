//! Everything Project Zomboid-specific: paths, mod.info, B41/B42 layout, default.txt.
//! Nothing outside this module should know PZ file formats.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

pub mod zombiebuddy;

pub const STEAM_APP: &str = "108600";

pub struct Paths {
    /// The game's user folder (`~/Zomboid`).
    pub zomboid: PathBuf,
    /// PZMM's own data: profiles, index, backups, steamcmd.
    pub data: PathBuf,
}

impl Paths {
    /// `PZ_HOME` / `PZMM_DATA` override the defaults.
    pub fn detect() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let zomboid = std::env::var_os("PZ_HOME").map_or_else(|| home.join("Zomboid"), PathBuf::from);
        let data = std::env::var_os("PZMM_DATA")
            .map_or_else(|| dirs::data_local_dir().unwrap_or(home).join("pzmm"), PathBuf::from);
        Paths { zomboid, data }
    }

    pub fn mods(&self) -> PathBuf {
        self.zomboid.join("mods")
    }

    pub fn default_txt(&self) -> PathBuf {
        self.mods().join("default.txt")
    }
}

pub fn read_info(path: &Path) -> HashMap<String, String> {
    let text = fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// B42 puts mod.info in `42/`, `42.13/`... and shared files in `common/`.
fn is_version_dir(name: &str) -> bool {
    name == "common" || name.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

pub struct FoundMod {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
    /// The mod.info it was read from (B42: inside `42/`).
    pub info: PathBuf,
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if depth > 1 {
                walk(&p, depth - 1, out);
            }
        } else if p.file_name().is_some_and(|n| n == "mod.info") {
            out.push(p);
        }
    }
}

/// Every mod under `root` whose mod.info is at most `depth` levels down (B41 or B42 layout).
pub fn find_mods(root: &Path, depth: usize) -> Vec<FoundMod> {
    let mut infos = vec![];
    walk(root, depth, &mut infos);
    let mut seen = HashSet::new();
    let mut out = vec![];
    for info in infos {
        let parent = info.parent().unwrap_or(root);
        let dir = match parent.file_name().and_then(|n| n.to_str()) {
            Some(n) if is_version_dir(n) => parent.parent().unwrap_or(parent),
            _ => parent,
        }
        .to_path_buf();
        if seen.insert(dir.clone()) {
            let mut i = read_info(&info);
            let fallback = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            out.push(FoundMod {
                id: i.remove("id").filter(|s| !s.is_empty()).unwrap_or(fallback),
                name: i.remove("name").unwrap_or_default(),
                dir,
                info,
            });
        }
    }
    out
}

/// Mods in `Zomboid/mods`.
pub fn local_mods(paths: &Paths) -> Vec<FoundMod> {
    find_mods(&paths.mods(), 3)
}

/// Mods inside a Steam Workshop content folder (`<ws>/<item>/mods/<Mod>/[42/]mod.info`).
pub fn workshop_mods(workshop_dir: &Path) -> Vec<FoundMod> {
    find_mods(workshop_dir, 5)
}

/// `mods/default.txt`: the enabled mods, in load order.
#[derive(Debug, Default, PartialEq)]
pub struct DefaultTxt {
    pub mods: Vec<String>,
    /// Body of the `maps { }` block, kept verbatim.
    pub maps: String,
    /// B41 writes `mod = Id,`; B42 writes `mod = \Id,`.
    pub b41: bool,
}

/// Inner text of `name { ... }`.
fn block<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = text[from..].find(name).map(|i| i + from) {
        from = i + name.len();
        let word_start = i == 0 || !text.as_bytes()[i - 1].is_ascii_alphanumeric();
        let rest = text[from..].trim_start();
        if word_start && rest.starts_with('{') {
            let open = text.len() - rest.len() + 1;
            return text[open..].find('}').map(|close| &text[open..open + close]);
        }
    }
    None
}

pub fn read_default(path: &Path) -> DefaultTxt {
    let text = fs::read_to_string(path).unwrap_or_default();
    let body = block(&text, "mods").unwrap_or("");
    let mods: Vec<String> = body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("mod")?.trim_start().strip_prefix('='))
        .map(|v| v.trim().trim_end_matches(',').trim().trim_start_matches('\\').to_string())
        .filter(|v| !v.is_empty())
        .collect();
    DefaultTxt {
        maps: block(&text, "maps").unwrap_or("\n").to_string(),
        b41: !mods.is_empty() && !body.contains('\\'),
        mods,
    }
}

/// Rewrite default.txt with `ids` in order. `b41: None` keeps the file's current format
/// (B42 when unknown). The previous file is kept as `default.txt.bak`.
pub fn write_default(path: &Path, ids: &[String], b41: Option<bool>) -> Result<()> {
    let old = read_default(path);
    let prefix = if b41.unwrap_or(old.b41) { "" } else { "\\" };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if path.exists() {
        fs::copy(path, path.with_extension("txt.bak"))?;
    }
    let mut seen = HashSet::new();
    let body: String = ids
        .iter()
        .filter(|i| seen.insert(*i))
        .map(|i| format!("    mod = {prefix}{i},\n"))
        .collect();
    fs::write(path, format!("VERSION = 1,\n\nmods\n{{\n{body}}}\n\nmaps\n{{{}}}\n", old.maps))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn finds_b41_and_b42_mods() {
        let t = tempfile::tempdir().unwrap();
        write(&t.path().join("Contents/mods/ModA/mod.info"), "name=Mod A\nid=ModA\n");
        write(&t.path().join("Contents/mods/ModB/42/mod.info"), "name = Mod B\nid = ModB\n");
        fs::create_dir_all(t.path().join("Contents/mods/ModB/common")).unwrap();
        let found = find_mods(t.path(), 8);
        let ids: Vec<_> = found.iter().map(|m| (m.id.as_str(), m.name.as_str())).collect();
        assert_eq!(ids, [("ModA", "Mod A"), ("ModB", "Mod B")]);
        assert!(found[1].dir.ends_with("ModB"));
    }

    #[test]
    fn default_txt_round_trip() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("default.txt");
        write(&p, "VERSION = 1,\n\nmods\n{\n}\n\nmaps\n{\n    map = Muldraugh, KY,\n}\n");
        let ids = ["ModA".to_string(), "ModB".into(), "ModA".into()];
        write_default(&p, &ids, None).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("mod = \\ModA,") && text.contains("map = Muldraugh, KY,"));
        let d = read_default(&p);
        assert_eq!((d.mods, d.b41), (vec!["ModA".to_string(), "ModB".into()], false));
        assert!(p.with_extension("txt.bak").exists());
    }

    #[test]
    fn b41_format_is_kept_unless_overridden() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("default.txt");
        write(&p, "VERSION = 1,\nmods\n{\n    mod = Old,\n}\nmaps\n{\n}\n");
        assert!(read_default(&p).b41);
        write_default(&p, &["Old".into(), "New".into()], None).unwrap();
        assert!(fs::read_to_string(&p).unwrap().contains("mod = New,"));
        write_default(&p, &["Old".into()], Some(false)).unwrap();
        assert!(fs::read_to_string(&p).unwrap().contains("mod = \\Old,"));
    }

    #[test]
    fn version_dirs() {
        assert!(is_version_dir("42") && is_version_dir("42.13") && is_version_dir("common"));
        assert!(!is_version_dir("media") && !is_version_dir("42.") && !is_version_dir(""));
    }
}
