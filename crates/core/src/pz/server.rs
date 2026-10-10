//! Dedicated server configs: `Zomboid/Server/<name>.ini` (`Mods=`, `WorkshopItems=`, `Map=`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

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
    // beyond file-name rules, nothing a shell would treat specially (the name reaches the launcher's command line)
    if !crate::safe_file_name(name) || name.starts_with('.') || name.contains(['&', '^', '%', '!', '(', ')', ';', ',', '=', '`', '$']) {
        bail!("invalid server name: {name:?}");
    }
    Ok(dir(paths).join(format!("{name}.ini")))
}

/// What a server config loads.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct ServerMods {
    pub mods: Vec<String>,
    pub workshop: Vec<String>,
    pub maps: Vec<String>,
}

/// The game's own map, which mod maps must come before.
pub const BASE_MAP: &str = "Muldraugh, KY";

/// `folder` is `BASE_MAP`, in any case and with spaces around it (Windows and the game don't mind).
pub fn is_base_map(folder: &str) -> bool {
    folder.trim().eq_ignore_ascii_case(BASE_MAP)
}

fn value<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).unwrap_or("").trim()
}

fn split(v: &str) -> Vec<String> {
    v.split(';').map(|s| s.trim().trim_start_matches('\\').to_string()).filter(|s| !s.is_empty()).collect()
}

pub fn read(path: &Path) -> Result<ServerMods> {
    let text = fs::read_to_string(path)?;
    let workshop = split(value(&text, "WorkshopItems")).into_iter().filter(|w| crate::sources::steam_workshop::valid_id(w)).collect();
    Ok(ServerMods { mods: split(value(&text, "Mods")), workshop, maps: split(value(&text, "Map")) })
}

/// The `Map=` list for `mod_maps` (the enabled mods' maps): maps, and the game's own ones,
/// keep their place in `order` (the user's arrangement or the config's current line), new
/// ones go before the first of the game's maps, and Muldraugh always goes last. Empty when
/// no mod adds a map.
pub fn map_order(order: &[String], mod_maps: &[String]) -> Vec<String> {
    if mod_maps.is_empty() {
        return vec![];
    }
    let mut out: Vec<String> = vec![];
    for m in order.iter().filter(|m| !is_base_map(m) && (mod_maps.contains(m) || super::maps::is_vanilla(m))) {
        if !out.contains(m) {
            out.push(m.clone());
        }
    }
    let at = out.iter().position(|m| super::maps::is_vanilla(m)).unwrap_or(out.len());
    let mut new: Vec<String> = vec![];
    for m in mod_maps.iter().filter(|m| !out.contains(m) && !is_base_map(m)) {
        if !new.contains(m) {
            new.push(m.clone());
        }
    }
    out.splice(at..at, new);
    out.push(BASE_MAP.into());
    out
}

/// `arranged`, with the entries of `saved` it lacks put back after the entry they followed:
/// maps that left the list keep their place for when they come back.
pub fn merge_order(saved: &[String], arranged: &[String]) -> Vec<String> {
    let mut out = arranged.to_vec();
    let mut at = 0;
    for s in saved {
        match out.iter().position(|o| o == s) {
            Some(p) => at = p + 1,
            None => {
                out.insert(at, s.clone());
                at += 1;
            }
        }
    }
    out
}

/// The maps the enabled mods add on `build` (the folders the game reads, see `maps::folder_names`), in load order.
pub fn mod_maps(enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, build: u8) -> Vec<String> {
    enabled.iter().filter_map(|id| mods.get(id)).flat_map(|m| super::maps::folder_names(&m.dir, build)).collect()
}

/// Set `Mods=`, `WorkshopItems=` and, when `maps` isn't empty, `Map=`. Every other line is
/// kept. The file as it was before PZMM's first write is kept as `.ini.bak`.
pub fn write(path: &Path, mods: &[String], workshop: &[String], maps: &[String], b41: bool) -> Result<()> {
    // only a missing file starts empty: rewriting one we couldn't read would wipe every other setting
    let old = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {} (nothing was changed)", path.display())),
    };
    crate::backup_once(path)?;
    let prefix = if b41 { "" } else { "\\" };
    let mut set = vec![
        ("Mods", mods.iter().map(|m| format!("{prefix}{m}")).collect::<Vec<_>>().join(";")),
        ("WorkshopItems", workshop.join(";")),
    ];
    if !maps.is_empty() {
        set.push(("Map", maps.join(";")));
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
    crate::write_atomic(path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Write the enabled mods (in load order) to `Server/<name>.ini`: installed mods go in
/// `Mods=`, their Workshop items in `WorkshopItems=`, their maps in `Map=` (kept as
/// `arranged`, else as the config has them: see [`map_order`]). Returns notes for the user
/// (the summary first, then anything the server won't get automatically).
pub fn export(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, arranged: Option<&[String]>) -> Result<Vec<String>> {
    let (file, w) = plan(paths, name, enabled, mods, arranged)?;
    write(&file, &w.ids, &w.workshop, &w.maps, crate::profile::effective_build(paths) == 41)?;
    let mut notes = w.notes;
    notes.insert(0, format!("Wrote {} mods and {} Workshop items to {}", w.ids.len(), w.workshop.len(), file.display()));
    Ok(notes)
}

/// What `export` would change in `Server/<name>.ini`, for a confirmation first.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Preview {
    /// The file doesn't exist yet.
    pub new: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// `Map=` now and after (the same when it stays as it is).
    pub maps_from: Vec<String>,
    pub maps_to: Vec<String>,
}

pub fn preview(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, arranged: Option<&[String]>) -> Result<Preview> {
    let (file, w) = plan(paths, name, enabled, mods, arranged)?;
    let now = if file.exists() { read(&file).with_context(|| format!("reading {}", file.display()))? } else { ServerMods::default() };
    Ok(Preview {
        new: !file.exists(),
        added: w.ids.iter().filter(|i| !now.mods.contains(i)).cloned().collect(),
        removed: now.mods.iter().filter(|i| !w.ids.contains(i)).cloned().collect(),
        maps_to: if w.maps.is_empty() { now.maps.clone() } else { w.maps },
        maps_from: now.maps,
    })
}

/// The lines `export` writes, and notes about mods the server won't get.
struct Written {
    ids: Vec<String>,
    workshop: Vec<String>,
    maps: Vec<String>,
    notes: Vec<String>,
}

fn plan(paths: &Paths, name: &str, enabled: &[String], mods: &std::collections::BTreeMap<String, crate::store::Row>, arranged: Option<&[String]>) -> Result<(PathBuf, Written)> {
    let (mut ids, mut workshop, mut notes) = (vec![], vec![], vec![]);
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
    }
    let build = crate::profile::effective_build(paths);
    let file = path(paths, name)?;
    let current = read(&file).map(|c| c.maps).unwrap_or_default();
    let maps = map_order(arranged.unwrap_or(&current), &mod_maps(&ids, mods, build));
    Ok((file, Written { ids, workshop, maps, notes }))
}

/// The script that starts a dedicated server: the Dedicated Server tool (`PZ_SERVER_DIR`, the
/// setting or Steam's libraries), else the server launcher that ships with the game.
pub fn start_script() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["StartServer64.bat", "ProjectZomboidServer.bat"] } else { &["start-server.sh"] };
    let dirs = [super::folders::path("server", &crate::settings::load()), super::zombiebuddy::game_dir()];
    dirs.into_iter().flatten().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

/// Start the dedicated server with config `name` (Server/<name>.ini). On Windows it gets
/// its own console window; on Linux its output goes to `<data>/logs/server-<name>.log`.
pub fn start(paths: &Paths, name: &str) -> Result<PathBuf> {
    path(paths, name)?; // validates the name
    let Some(script) = start_script() else {
        bail!("no server launcher found: install \"Project Zomboid Dedicated Server\" from Steam (Library > Tools), or set PZ_SERVER_DIR")
    };
    let dir = script.parent().unwrap_or(Path::new("."));
    let mut cmd;
    if cfg!(windows) {
        // run the .bat itself in a new console (no cmd.exe /C string to inject into; Rust quotes .bat arguments)
        cmd = std::process::Command::new(&script);
        cmd.args(["-servername", name]).current_dir(dir);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
            cmd.creation_flags(CREATE_NEW_CONSOLE);
        }
    } else {
        let logs = paths.data.join("logs");
        fs::create_dir_all(&logs)?;
        let log = fs::File::create(logs.join(format!("server-{name}.log")))?;
        cmd = std::process::Command::new("sh");
        cmd.arg(&script).args(["-servername", name]).current_dir(dir).stdout(log.try_clone()?).stderr(log);
    }
    cmd.spawn().map_err(|e| anyhow::anyhow!("starting {}: {e}", script.display()))?;
    Ok(script)
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
        write(&p, &ids(&["A", "B"]), &ids(&["111", "222"]), &ids(&["Riverside Plus", BASE_MAP]), false).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert_eq!(text, "PVP=true\nMods=\\A;\\B\nWorkshopItems=111;222\nMap=Riverside Plus;Muldraugh, KY\nPublic=false\n");
        assert_eq!(
            read(&p).unwrap(),
            ServerMods { mods: ids(&["A", "B"]), workshop: ids(&["111", "222"]), maps: ids(&["Riverside Plus", BASE_MAP]) }
        );
        assert!(p.with_file_name("servertest.ini.bak").exists());

        write(&p, &ids(&["A"]), &[], &[], true).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("Mods=A\n") && text.contains("Map=Riverside Plus;Muldraugh, KY"), "no maps: Map= untouched");
    }

    #[test]
    fn map_order_keeps_the_arrangement() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(map_order(&ids(&["Old", BASE_MAP]), &[]), Vec::<String>::new(), "no mod maps: Map= untouched");
        // B stays first, the gone map drops, the new C goes before Muldraugh
        assert_eq!(map_order(&ids(&["B", "Gone", "A", BASE_MAP]), &ids(&["A", "B", "C", "A"])), ids(&["B", "A", "C", BASE_MAP]));
        assert_eq!(map_order(&[], &ids(&["A"])), ids(&["A", BASE_MAP]));
        assert_eq!(map_order(&ids(&[" muldraugh, ky", "A"]), &ids(&["A"])), ids(&["A", BASE_MAP]), "any case: still the base map, still last");
        assert!(is_base_map("MULDRAUGH, KY ") && !is_base_map("Muldraugh") && !is_base_map("Muldraugh,KY"));
        // Muldraugh is pinned last; the game's other maps keep their place, new mod maps go before them
        assert_eq!(map_order(&ids(&[BASE_MAP, "A", "Riverside, KY", "Gone"]), &ids(&["A", "C"])), ids(&["A", "C", "Riverside, KY", BASE_MAP]));
    }

    #[test]
    fn merge_keeps_maps_that_left() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // X was hidden, then A and B swapped: X stays after A, Y stays first
        assert_eq!(merge_order(&ids(&["Y", "A", "X", "B", BASE_MAP]), &ids(&["B", "A", BASE_MAP])), ids(&["Y", "B", "A", "X", BASE_MAP]));
        // so when X comes back it's where it was, not just before Muldraugh
        let saved = merge_order(&ids(&["A", "X", "B", BASE_MAP]), &ids(&["A", "B", BASE_MAP]));
        assert_eq!(map_order(&saved, &ids(&["A", "B", "X"])), ids(&["A", "X", "B", BASE_MAP]));
        assert_eq!(merge_order(&[], &ids(&["A"])), ids(&["A"]));
    }

    #[test]
    fn server_maps_follow_the_build() {
        // a B41 map at the mod root; B42 maps in common/ and the newest version folder (42.9 is older)
        let t = tempfile::tempdir().unwrap();
        let m = t.path().join("Town");
        for d in ["media/maps/Old Town", "common/media/maps/Town", "42.13/media/maps/Town Extra", "42.9/media/maps/Gone"] {
            fs::create_dir_all(m.join(d)).unwrap();
        }
        let row = crate::store::Row {
            id: "Town".into(),
            name: String::new(),
            source: "manual".into(),
            dir: m,
            info: PathBuf::new(),
            requires: vec![],
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: true,
            supports_42: true,
        };
        let mods = [("Town".to_string(), row)].into();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(mod_maps(&ids(&["Town"]), &mods, 41), ids(&["Old Town"]));
        assert_eq!(mod_maps(&ids(&["Town"]), &mods, 42), ids(&["Town", "Town Extra"]));
    }

    #[test]
    fn preview_lists_what_writing_changes() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let row = |source: &str| crate::store::Row {
            id: String::new(),
            name: String::new(),
            source: source.into(),
            dir: PathBuf::new(),
            info: PathBuf::new(),
            requires: vec![],
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: true,
            supports_42: true,
        };
        let mods = [("A".to_string(), row("steam:1")), ("B".to_string(), row("steam:2"))].into();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(preview(&paths, "srv", &ids(&["A"]), &mods, None).unwrap(), Preview { new: true, added: ids(&["A"]), ..Default::default() });
        assert!(!path(&paths, "srv").unwrap().exists(), "a preview writes nothing");
        export(&paths, "srv", &ids(&["A"]), &mods, None).unwrap();
        let p = preview(&paths, "srv", &ids(&["B"]), &mods, None).unwrap();
        assert_eq!((p.new, p.added, p.removed), (false, ids(&["B"]), ids(&["A"])));
    }

    #[test]
    fn unreadable_ini_is_left_alone() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("servertest.ini");
        let ansi = b"PVP=true\nPublicDescription=Caf\xe9\nMods=Old\n"; // saved as ANSI: not UTF-8
        fs::write(&p, ansi).unwrap();
        assert!(write(&p, &["A".into()], &[], &[], false).is_err());
        assert_eq!(fs::read(&p).unwrap(), ansi, "settings kept");
    }

    #[test]
    fn server_names_carry_no_shell_syntax() {
        let paths = crate::pz::Paths { zomboid: "Z".into(), data: "D".into() };
        assert!(path(&paths, "servertest").is_ok());
        assert!(path(&paths, "My Server").is_ok());
        for bad in ["R&D", "x&calc", "a^b", "%PATH%", "x(1)", "..", "a/b", ""] {
            assert!(path(&paths, bad).is_err(), "{bad}");
        }
    }
}
