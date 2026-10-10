//! Everything Project Zomboid-specific: paths, mod.info, B41/B42 layout, default.txt.
//! Nothing outside this module should know PZ file formats.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub mod category;
pub mod doctor;
pub mod files;
pub mod folders;
pub mod logs;
pub mod maps;
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
    /// `PZ_HOME` / `PZMM_DATA`, else the settings, else the defaults (see [`folders`]).
    pub fn detect() -> Self {
        let s = crate::settings::load();
        Paths { zomboid: folders::path("zomboid", &s).unwrap_or_default(), data: folders::path("data", &s).unwrap_or_default() }
    }

    pub fn mods(&self) -> PathBuf {
        self.zomboid.join("mods")
    }

    pub fn default_txt(&self) -> PathBuf {
        self.mods().join("default.txt")
    }
}

/// The Steam Workshop content folder for Project Zomboid (the setting, else Steam's libraries).
pub fn workshop_dir() -> Option<PathBuf> {
    folders::path("workshop", &crate::settings::load())
}

pub fn read_info(path: &Path) -> HashMap<String, String> {
    parse_info(&fs::read(path).map(|b| decode_text(&b)).unwrap_or_default())
}

/// A text file the game reads (mod.info, map.info, title files): UTF-8, else Windows-1252,
/// which older mods were saved in by Windows editors.
pub fn decode_text(b: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    // 0x80..0x9F are the only bytes where Windows-1252 differs from Latin-1 (the gaps decode as themselves)
    const HIGH: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F, //
        0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    b.iter().map(|&c| if (0x80..0xA0).contains(&c) { char::from_u32(HIGH[c as usize - 0x80] as u32).unwrap_or('?') } else { char::from(c) }).collect()
}

/// mod.info's `key=value` lines.
pub fn parse_info(text: &str) -> HashMap<String, String> {
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
        // ASCII lowercasing keeps byte offsets identical (to_lowercase turns İ into two chars)
        let lower = line.to_ascii_lowercase();
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
            // a numeric root (a Workshop item folder) is the mod itself, never its parent
            Some(n) if is_version_dir(n) && parent != root => parent.parent().unwrap_or(parent),
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

impl DefaultTxt {
    /// The map folders the `maps` block lists (`map = <folder>,`), in order.
    pub fn map_folders(&self) -> Vec<String> {
        // hand edits: any case and spacing (`Map=Fort, KY,`)
        let key = |l: &&str| l.get(..3).is_some_and(|k| k.eq_ignore_ascii_case("map"));
        self.maps.lines().map(str::trim).filter(key).filter_map(|l| l[3..].trim_start().strip_prefix('=')).map(|v| v.trim().trim_end_matches(',').trim().to_string()).filter(|v| !v.is_empty()).collect()
    }
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
/// (B42 when unknown). `maps` (map folders, load order) replaces the `maps { }` block; None
/// keeps it. Both builds write `map = <folder>,` lines; Muldraugh stays last if the file had it (the game.s
/// `checkMissingMaps` drops it). The file as it was before PZMM's first write is kept as `default.txt.bak`.
pub fn write_default(path: &Path, ids: &[String], b41: Option<bool>, maps: Option<&[String]>) -> Result<()> {
    // an existing file we can't read keeps its maps block and format unknown: don't overwrite it blind
    let old = if path.exists() {
        try_read_default(path).with_context(|| format!("can't read {} (is the game running?)", path.display()))?
    } else {
        read_default(path)
    };
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
    let maps = match maps {
        Some(m) => {
            // the vanilla map stays last when the game wrote it there (B41 is unverified: keep what it had)
            let base = |f: &String| server::is_base_map(f);
            let vanilla = old.map_folders().iter().any(base);
            let m = m.iter().filter(|f| !base(f)).map(String::as_str).chain(vanilla.then_some(server::BASE_MAP));
            format!("\n{}", m.map(|f| format!("    map = {f},\n")).collect::<String>())
        }
        None => old.maps,
    };
    crate::write_atomic(path, format!("VERSION = 1,\n\nmods\n{{\n{body}}}\n\nmaps\n{{{maps}}}\n"))?;
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
        write_default(&p, &ids, None, None).unwrap();
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
        write_default(&p, &["Old".into(), "New".into()], None, None).unwrap();
        assert!(fs::read_to_string(&p).unwrap().contains("mod = New,"));
        write_default(&p, &["Old".into()], Some(false), None).unwrap();
        assert!(fs::read_to_string(&p).unwrap().contains("mod = \\Old,"));
    }

    #[test]
    fn maps_block_for_both_builds() {
        // as the game's ActiveModsFile writes it: `map = <folder>,` per line, the same in B41 and B42
        let t = tempfile::tempdir().unwrap();
        let maps = ["Riverside Plus".to_string(), "Fort, KY".into()];
        for (b41, prefix) in [(true, ""), (false, "\\")] {
            let p = t.path().join(format!("{b41}/default.txt"));
            write(&p, "VERSION = 1,\nmods\n{\n}\nmaps\n{\n    map = Old,\n}\n");
            write_default(&p, &["Town".into()], Some(b41), Some(&maps)).unwrap();
            assert_eq!(
                fs::read_to_string(&p).unwrap(),
                format!("VERSION = 1,\n\nmods\n{{\n    mod = {prefix}Town,\n}}\n\nmaps\n{{\n    map = Riverside Plus,\n    map = Fort, KY,\n}}\n")
            );
            assert_eq!(read_default(&p).maps, "\n    map = Riverside Plus,\n    map = Fort, KY,\n");
            assert_eq!(read_default(&p).map_folders(), maps);
            // None keeps the block; an empty list empties it
            write_default(&p, &["Town".into()], None, None).unwrap();
            assert!(fs::read_to_string(&p).unwrap().contains("map = Fort, KY,"));
            write_default(&p, &["Town".into()], None, Some(&[])).unwrap();
            assert!(fs::read_to_string(&p).unwrap().ends_with("maps\n{\n}\n"));
        }
        // Muldraugh stays last when the game had it
        let p = t.path().join("vanilla/default.txt");
        write(&p, "VERSION = 1,\nmods\n{\n}\nmaps\n{\n    map = Muldraugh, KY,\n}\n");
        write_default(&p, &[], None, Some(&maps)).unwrap();
        assert!(fs::read_to_string(&p).unwrap().ends_with("    map = Fort, KY,\n    map = Muldraugh, KY,\n}\n"));
        // ...hand-edited spacing and case included
        for line in ["map=Muldraugh, KY,", "Map = muldraugh, ky,", "  MAP =Muldraugh, KY  ,"] {
            write(&p, &format!("VERSION = 1,\nmods\n{{\n}}\nmaps\n{{\n{line}\n}}\n"));
            write_default(&p, &[], None, Some(&maps)).unwrap();
            assert!(fs::read_to_string(&p).unwrap().ends_with("    map = Fort, KY,\n    map = Muldraugh, KY,\n}\n"), "{line}");
        }
    }

    #[test]
    fn ansi_text_is_windows_1252() {
        assert_eq!(decode_text("Café – ok".as_bytes()), "Café – ok");
        assert_eq!(decode_text(b"Caf\xe9 \x96 \x93Riverside\x94 \x80"), "Café – \u{201c}Riverside\u{201d} €");
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
        // non-ASCII text before the label used to shift the byte offsets (and could panic)
        assert_eq!(workshop_mod_ids("İNDİRİLEBİLİR İÇERİK İÇİN GEREKLİ MOD ID: X"), ["X"]);
        assert_eq!(workshop_mod_ids("İİİ Mod ID: Şehir"), ["Şehir"]);
    }

    #[test]
    fn version_dirs() {
        assert!(is_version_dir("42") && is_version_dir("42.13") && is_version_dir("common"));
        assert!(!is_version_dir("media") && !is_version_dir("42.") && !is_version_dir(""));
    }
}
