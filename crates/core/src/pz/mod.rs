//! Everything Project Zomboid-specific: paths, mod.info, B41/B42 layout, default.txt.
//! Nothing outside this module should know PZ file formats.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

pub mod doctor;
pub mod files;
pub mod logs;
pub mod saves;
pub mod server;
pub mod viewpoint;
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
    text.trim_start_matches('\u{feff}') // a UTF-8 BOM would otherwise hide the first key (usually `id`)
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Mod ids listed in mod.info's `require=` (B42 writes them as `\Id`).
pub fn requires(info: &HashMap<String, String>) -> Vec<String> {
    id_list(info, "require")
}

/// A mod.info list of mod ids (`key=\\A,\\B` or `A;B`).
pub fn id_list(info: &HashMap<String, String>, key: &str) -> Vec<String> {
    info.get(key)
        .map(|r| r.split([',', ';']).map(|s| s.trim().trim_start_matches('\\').to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

/// Mod ids a Workshop description declares, by the community convention `Mod ID: X`
/// (also `Mod IDs:`, BBCode around it, several ids separated by `,` or `;`).
pub fn workshop_mod_ids(description: &str) -> Vec<String> {
    let mut plain = String::new();
    let mut in_tag = false;
    for c in description.chars() {
        match c {
            '[' => in_tag = true,
            ']' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(c),
            _ => {}
        }
    }
    let mut ids = vec![];
    for line in plain.lines() {
        let lower = line.to_lowercase();
        let Some(at) = lower.find("mod id") else { continue };
        let Some(colon) = line[at..].find(':') else { continue };
        if line[at..at + colon].len() > 9 {
            continue; // "Mod ID" must be the label, not part of a sentence
        }
        for id in line[at + colon + 1..].split([',', ';']) {
            let id = id.trim().trim_start_matches('\\').trim();
            if !id.is_empty() && !ids.iter().any(|x: &String| x == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// B42 puts mod.info in `42/`, `42.13/`... and shared files in `common/`.
pub(crate) fn is_version_dir(name: &str) -> bool {
    name == "common" || name.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

pub struct FoundMod {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
    /// The mod.info it was read from (B42: inside `42/`).
    pub info: PathBuf,
    /// Mod ids it lists in `require=`.
    pub requires: Vec<String>,
    /// `incompatible=`: mods that must not be enabled with it.
    pub incompatible: Vec<String>,
    /// `loadModAfter=` / `loadModBefore=`: ordering hints the game reads.
    pub load_after: Vec<String>,
    pub load_before: Vec<String>,
    /// Builds its folder layout supports: 41 needs a root mod.info, 42 a version folder
    /// (`42/`, `42.x/`, `common/`). Both are true for mods that ship both.
    pub supports_41: bool,
    pub supports_42: bool,
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    // file_type() comes with the directory listing: no extra stat per entry
    let mut entries: Vec<(PathBuf, bool)> = rd.flatten().map(|e| (e.path(), e.file_type().is_ok_and(|t| t.is_dir()))).collect();
    entries.sort();
    for (p, is_dir) in entries {
        if is_dir {
            if depth > 1 {
                walk(&p, depth - 1, out);
            }
        } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("mod.info")) {
            out.push(p); // case-insensitive: Linux filesystems keep authors' `Mod.info`
        }
    }
}

/// Every mod under `root` whose mod.info is at most `depth` levels down (B41 or B42 layout).
pub fn find_mods(root: &Path, depth: usize) -> Vec<FoundMod> {
    let mut infos = vec![];
    walk(root, depth, &mut infos);
    // One mod can ship several mod.info files (root for B41, 42.12/, 42.13/...). The game
    // uses the newest version folder, so prefer that: rank = version numbers, root = [].
    // ponytail: newest folder, not "newest <= installed game version"; needs the game version to do better
    let version_of = |info: &Path| -> Vec<u32> {
        info.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .filter(|n| *n != "common" && is_version_dir(n))
            .map(|n| n.split('.').filter_map(|p| p.parse().ok()).collect())
            .unwrap_or_default()
    };
    let mut best: Vec<(PathBuf, PathBuf)> = vec![]; // (mod dir, chosen mod.info), in discovery order
    for info in infos {
        let parent = info.parent().unwrap_or(root);
        let dir = match parent.file_name().and_then(|n| n.to_str()) {
            Some(n) if is_version_dir(n) => parent.parent().unwrap_or(parent),
            _ => parent,
        }
        .to_path_buf();
        match best.iter_mut().find(|(d, _)| *d == dir) {
            Some((_, chosen)) if version_of(&info) > version_of(chosen) => *chosen = info,
            Some(_) => {}
            None => best.push((dir, info)),
        }
    }
    let mut out = vec![];
    for (dir, info) in best {
        {
            let mut i = read_info(&info);
            let requires = requires(&i);
            let supports_41 = dir.join("mod.info").is_file() || dir.join("Mod.info").is_file();
            let supports_42 = fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name().to_str().is_some_and(is_version_dir));
            let (incompatible, load_after, load_before) =
                (id_list(&i, "incompatible"), id_list(&i, "loadModAfter"), id_list(&i, "loadModBefore"));
            let fallback = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            out.push(FoundMod {
                id: i.remove("id").filter(|s| !s.is_empty()).unwrap_or(fallback),
                name: i.remove("name").unwrap_or_default(),
                dir,
                info,
                requires,
                incompatible,
                load_after,
                load_before,
                supports_41,
                supports_42,
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

/// default.txt, or an empty list when it doesn't exist or can't be read.
pub fn read_default(path: &Path) -> DefaultTxt {
    try_read_default(path).unwrap_or_else(|| DefaultTxt { maps: "\n".into(), ..Default::default() })
}

/// default.txt, or None when it's missing or unreadable (locked by the game, not UTF-8).
/// Callers that act on "nothing is enabled" must use this, not `read_default`.
pub fn try_read_default(path: &Path) -> Option<DefaultTxt> {
    let text = fs::read_to_string(path).ok()?;
    let body = block(&text, "mods").unwrap_or("");
    let mods: Vec<String> = body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("mod")?.trim_start().strip_prefix('='))
        .map(|v| v.trim().trim_end_matches(',').trim().trim_start_matches('\\').to_string())
        .filter(|v| !v.is_empty())
        .collect();
    Some(DefaultTxt {
        maps: block(&text, "maps").unwrap_or("\n").to_string(),
        b41: !mods.is_empty() && !body.contains('\\'),
        mods,
    })
}

/// Rewrite default.txt with `ids` in order. `b41: None` keeps the file's current format
/// (B42 when unknown). The file as it was before PZMM's first write is kept as `default.txt.bak`.
pub fn write_default(path: &Path, ids: &[String], b41: Option<bool>) -> Result<()> {
    let old = read_default(path);
    let prefix = if b41.unwrap_or(old.b41) { "" } else { "\\" };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    crate::backup_once(path)?;
    let mut seen = HashSet::new();
    let body: String = ids
        .iter()
        .filter(|i| !i.is_empty() && seen.insert(*i))
        .map(|i| format!("    mod = {prefix}{i},\n"))
        .collect();
    crate::write_atomic(path, format!("VERSION = 1,\n\nmods\n{{\n{body}}}\n\nmaps\n{{{}}}\n", old.maps))?;
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
    fn bom_and_capitalised_mod_info() {
        let t = tempfile::tempdir().unwrap();
        write(&t.path().join("A/mod.info"), "\u{feff}id=WithBom\r\nname=Bom\r\n");
        write(&t.path().join("B/Mod.info"), "id=CapitalM\n");
        let ids: Vec<String> = find_mods(t.path(), 3).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["WithBom", "CapitalM"]);
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
    fn newest_version_folder_wins() {
        // Tomb's Player Body - Customisation: 42.12 still requires the old id, 42.13 the new one
        let t = tempfile::tempdir().unwrap();
        let m = t.path().join("TombBodyCustom");
        write(&m.join("mod.info"), "id=TombBodyCustom\nrequire=SpnCharCustom\n");
        write(&m.join("42.12/mod.info"), "id=TombBodyCustom\nrequire=\\SpnCharCustom\n");
        write(&m.join("42.13/mod.info"), "id=TombBodyCustom\nrequire=\\SPNCC\n");
        write(&m.join("42.9/mod.info"), "id=TombBodyCustom\nrequire=\\Old\n");
        let found = find_mods(t.path(), 3);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].requires, ["SPNCC"]);
        assert!(found[0].info.ends_with("42.13/mod.info"));
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
        assert!(p.with_file_name("default.txt.bak").exists());
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
    fn parses_requires() {
        let info = HashMap::from([("require".to_string(), r"\ZombieBuddy, \ModOptions;tsarslib,".to_string())]);
        assert_eq!(requires(&info), ["ZombieBuddy", "ModOptions", "tsarslib"]);
        assert!(requires(&HashMap::new()).is_empty());
    }

    #[test]
    fn reads_mod_ids_from_descriptions() {
        assert_eq!(workshop_mod_ids("Great mod.
Workshop ID: 3302128165
Mod ID: Denseenvironment"), ["Denseenvironment"]);
        assert_eq!(workshop_mod_ids("[b]Mod ID:[/b] A; B,C
[h1]Mod ID: A[/h1]"), ["A", "B", "C"]);
        assert_eq!(workshop_mod_ids(r"Mod IDs: \TLOG_Remaster,TLOG_Remaster Flamethrower"), ["TLOG_Remaster", "TLOG_Remaster Flamethrower"]);
        assert!(workshop_mod_ids("This mod id is great: really").is_empty());
    }

    #[test]
    fn version_dirs() {
        assert!(is_version_dir("42") && is_version_dir("42.13") && is_version_dir("common"));
        assert!(!is_version_dir("media") && !is_version_dir("42.") && !is_version_dir(""));
    }
}
