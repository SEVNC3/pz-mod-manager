//! Dedicated server configs: `Zomboid/Server/<name>.ini` (`Mods=`, `WorkshopItems=`, `Map=`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::Paths;

pub fn dir(paths: &Paths) -> PathBuf {
    paths.zomboid.join("Server")
}

/// Server config names (the part before `.ini`).
pub fn list(paths: &Paths) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir(paths))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".ini").map(String::from))
        .collect();
    names.sort();
    names
}

pub fn path(paths: &Paths, name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || name.starts_with('.') {
        bail!("invalid server name: {name:?}");
    }
    Ok(dir(paths).join(format!("{name}.ini")))
}

/// What a server config loads.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct ServerMods {
    pub mods: Vec<String>,
    pub workshop: Vec<String>,
}

fn value<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).unwrap_or("").trim()
}

fn split(v: &str) -> Vec<String> {
    v.split(';').map(|s| s.trim().trim_start_matches('\\').to_string()).filter(|s| !s.is_empty()).collect()
}

pub fn read(path: &Path) -> Result<ServerMods> {
    let text = fs::read_to_string(path)?;
    Ok(ServerMods { mods: split(value(&text, "Mods")), workshop: split(value(&text, "WorkshopItems")) })
}

/// Set `Mods=`, `WorkshopItems=` and, when any mod adds maps, `Map=` (mod maps first,
/// then the base map). Every other line is kept. The previous file is kept as `.ini.bak`.
pub fn write(path: &Path, mods: &[String], workshop: &[String], maps: &[String], b41: bool) -> Result<()> {
    let old = fs::read_to_string(path).unwrap_or_default();
    if path.exists() {
        fs::copy(path, path.with_extension("ini.bak"))?;
    }
    let prefix = if b41 { "" } else { "\\" };
    let mut set = vec![
        ("Mods", mods.iter().map(|m| format!("{prefix}{m}")).collect::<Vec<_>>().join(";")),
        ("WorkshopItems", workshop.join(";")),
    ];
    if !maps.is_empty() {
        let mut all = maps.to_vec();
        all.push("Muldraugh, KY".into());
        set.push(("Map", all.join(";")));
    }
    let mut lines: Vec<String> = old.lines().map(String::from).collect();
    for (key, val) in set {
        let line = format!("{key}={val}");
        match lines.iter_mut().find(|l| l.split_once('=').is_some_and(|(k, _)| k == key)) {
            Some(l) => *l = line,
            None => lines.push(line),
        }
    }
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Write the enabled mods (in load order) to `Server/<name>.ini`: installed mods go in
/// `Mods=`, their Workshop items in `WorkshopItems=`, their maps in `Map=`. Returns notes
/// for the user (the summary first, then anything the server won't get automatically).
pub fn export(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>) -> Result<Vec<String>> {
    let (mut ids, mut workshop, mut maps, mut notes) = (vec![], vec![], vec![], vec![]);
    for id in enabled {
        let Some(m) = mods.get(id) else {
            notes.push(format!("{id} isn't installed, so it was left out"));
            continue;
        };
        ids.push(id.clone());
        match m.source.split_once(':') {
            Some(("steam" | "workshop", item)) => {
                if !workshop.iter().any(|w| w == item) {
                    workshop.push(item.to_string());
                }
            }
            _ => notes.push(format!("{id} isn't from the Workshop: copy its folder into the server's mods folder yourself")),
        }
        maps.extend(super::files::maps(&m.dir));
    }
    let b41 = crate::profile::effective_build(paths) == 41;
    let file = path(paths, name)?;
    write(&file, &ids, &workshop, &maps, b41)?;
    notes.insert(0, format!("Wrote {} mods and {} Workshop items to {}", ids.len(), workshop.len(), file.display()));
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_other_settings() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("servertest.ini");
        fs::write(&p, "PVP=true\nMods=Old\nWorkshopItems=1\nMap=Muldraugh, KY\nPublic=false\n").unwrap();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        write(&p, &ids(&["A", "B"]), &ids(&["111", "222"]), &ids(&["Riverside Plus"]), false).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert_eq!(text, "PVP=true\nMods=\\A;\\B\nWorkshopItems=111;222\nMap=Riverside Plus;Muldraugh, KY\nPublic=false\n");
        assert_eq!(read(&p).unwrap(), ServerMods { mods: ids(&["A", "B"]), workshop: ids(&["111", "222"]) });
        assert!(p.with_extension("ini.bak").exists());

        write(&p, &ids(&["A"]), &[], &[], true).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("Mods=A\n") && text.contains("Map=Riverside Plus;Muldraugh, KY"), "no maps: Map= untouched");
    }
}
