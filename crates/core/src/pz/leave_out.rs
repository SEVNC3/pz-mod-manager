//! Mods a server doesn't get when PZMM writes its mod lines (client-side add-ons, say), kept per server:
//! `<data>/server-leave-out.json`, `{"<server>": ["ModId", ...]}`, the server being its ini's base name (a hosted
//! server's name is its ini's). The game reads a copy (`Zomboid/Lua/pzmm/leaveout.txt`) and asks for changes.

use std::collections::BTreeMap;

use anyhow::Result;

use super::Paths;

fn file(paths: &Paths) -> std::path::PathBuf {
    paths.data.join("server-leave-out.json")
}

fn all(paths: &Paths) -> BTreeMap<String, Vec<String>> {
    crate::read_json(&file(paths)).unwrap_or_default()
}

/// The mods `server` doesn't get (its name in any case).
pub fn get(paths: &Paths, server: &str) -> Vec<String> {
    all(paths).into_iter().find(|(k, _)| k.eq_ignore_ascii_case(server)).map(|(_, v)| v).unwrap_or_default()
}

/// Leave `ids` out of `server`'s lines from now on (none: it gets every mod again).
pub fn set(paths: &Paths, server: &str, ids: &[String]) -> Result<()> {
    let mut all = all(paths);
    all.retain(|k, _| !k.eq_ignore_ascii_case(server));
    let mut ids: Vec<String> = ids.iter().filter(|i| !i.trim().is_empty()).cloned().collect();
    ids.sort();
    ids.dedup();
    if !ids.is_empty() {
        all.insert(server.to_string(), ids);
    }
    std::fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&file(paths), serde_json::to_vec_pretty(&all)?)?;
    for_game(paths);
    Ok(())
}

/// The game's copy: a `<server>\t<id>;<id>` line per server that leaves something out.
pub fn for_game(paths: &Paths) {
    let dir = super::bridge::dir(paths);
    if dir.is_dir() {
        let text: String = all(paths).iter().map(|(s, ids)| format!("{s}\t{}\n", ids.join(";"))).collect();
        super::bridge::write_for_game(&dir.join("leaveout.txt"), &text);
    }
}

/// Mods that look client-side: their requirements (through installed mods) reach Project Viewpoint or ZombieBuddy,
/// which usually run on players' PCs only. Only a label: nothing is left out for the player.
pub fn client_side(id: &str, requires: &dyn Fn(&str) -> Vec<String>) -> bool {
    let (mut todo, mut seen) = (requires(id), std::collections::HashSet::new());
    while let Some(r) = todo.pop() {
        if r == "Viewpoint" || r == "ZombieBuddy" {
            return true;
        }
        if seen.insert(r.clone()) {
            todo.extend(requires(&r));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_per_server_in_any_case() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().join("Zomboid"), data: t.path().join("data") };
        std::fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        set(&paths, "servertest", &["UI".into(), "VPAddon".into()]).unwrap();
        set(&paths, "Other", &["X".into()]).unwrap();
        assert_eq!(get(&paths, "ServerTest"), ["UI", "VPAddon"]);
        assert_eq!(std::fs::read_to_string(paths.zomboid.join("Lua/pzmm/leaveout.txt")).unwrap(), "Other\tX\nservertest\tUI;VPAddon\n");
        set(&paths, "SERVERTEST", &[]).unwrap();
        assert!(get(&paths, "servertest").is_empty(), "none left out: every mod again");
        assert_eq!(std::fs::read_to_string(paths.zomboid.join("Lua/pzmm/leaveout.txt")).unwrap(), "Other\tX\n");
    }

    #[test]
    fn client_side_follows_requirements() {
        let req = |id: &str| -> Vec<String> {
            match id {
                "VPAddon" => vec!["ViewpointLib".into()],
                "ViewpointLib" => vec!["Viewpoint".into()],
                "Loop" => vec!["Loop".into()],
                "Java" => vec!["ZombieBuddy".into()],
                _ => vec![],
            }
        };
        assert!(client_side("VPAddon", &req) && client_side("Java", &req));
        assert!(!client_side("Viewpoint", &req) && !client_side("Loop", &req) && !client_side("Plain", &req));
    }
}
