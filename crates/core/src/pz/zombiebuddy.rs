//! ZombieBuddy (github.com/zed-0xff/ZombieBuddy): a Java agent that lets Java mods patch the
//! game. Besides the normal mod, it needs its jar (plus zbNative.dll on Windows) in the game
//! folder and a launcher flag in ProjectZomboid64.json. This mirrors the official installer's
//! "Normal Launch: ProjectZomboid64.json" mode; Steam launch options and the .bat are not touched.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

pub const WORKSHOP_ID: &str = "3619862853";
pub const MOD_ID: &str = "ZombieBuddy";
const LAUNCHER: &str = "ProjectZomboid64.json";

/// The JVM flag that loads the agent (zbNative.dll works around the Windows JRE's DLL path).
pub fn launch_arg() -> &'static str {
    if cfg!(windows) { "-agentlib:zbNative" } else { "-javaagent:ZombieBuddy.jar" }
}

fn agent_files() -> &'static [&'static str] {
    if cfg!(windows) { &["zbNative.dll", "ZombieBuddy.jar"] } else { &["ZombieBuddy.jar"] }
}

fn is_zb_arg(a: &str) -> bool {
    ["-agentlib:zbNative", "-javaagent:ZombieBuddy.jar"].iter().any(|p| a == *p || a.starts_with(&format!("{p}=")))
}

/// The game's install folder: `PZ_GAME_DIR`, else the setting, else the Steam library that has it.
pub fn game_dir() -> Option<PathBuf> {
    super::folders::path("game", &crate::settings::load())
}

/// Folder holding ProjectZomboid64.json: the game root on Windows, `projectzomboid/` on Linux.
pub fn launcher_dir(game: &Path) -> Option<PathBuf> {
    [game.to_path_buf(), game.join("projectzomboid")].into_iter().find(|d| d.join(LAUNCHER).is_file())
}

/// Whether a scanned mod's source is the official ZombieBuddy Workshop item. Only its jar
/// may become the game's JVM agent; a look-alike local mod named ZombieBuddy must not.
pub fn official_source(source: &str) -> bool {
    crate::sources::workshop_item(source) == Some(WORKSHOP_ID)
}

/// Where the agent goes (the game's launcher folder) and comes from (the official
/// ZombieBuddy Workshop item's libs folder), given a mod scan.
pub fn locate(mods: &std::collections::BTreeMap<String, crate::store::Row>) -> (Option<PathBuf>, Option<PathBuf>) {
    let launcher = game_dir().and_then(|g| launcher_dir(&g));
    let libs = mods.get(MOD_ID).filter(|m| official_source(&m.source)).and_then(|m| libs_dir(&m.dir));
    (launcher, libs)
}

/// The folder inside a ZombieBuddy mod that holds `ZombieBuddy.jar` (normally `libs/`).
pub fn libs_dir(mod_dir: &Path) -> Option<PathBuf> {
    let mut stack = vec![(mod_dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        if d.join("ZombieBuddy.jar").is_file() {
            return Some(d);
        }
        if depth < 3 {
            stack.extend(fs::read_dir(&d).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).map(|e| (e.path(), depth + 1)));
        }
    }
    None
}

fn read_launcher(launcher: &Path) -> Result<Value> {
    let p = launcher.join(LAUNCHER);
    serde_json::from_str(&fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?)
        .with_context(|| format!("{} isn't valid JSON", p.display()))
}

fn write_launcher(launcher: &Path, v: &Value) -> Result<()> {
    let p = launcher.join(LAUNCHER);
    let backup = launcher.join(format!("{LAUNCHER}.pzmm-backup"));
    if !backup.exists() {
        fs::copy(&p, backup)?;
    }
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b"\t"));
    serde::Serialize::serialize(v, &mut ser)?;
    out.push(b'\n');
    crate::write_atomic(&p, out)?;
    Ok(())
}

pub fn launcher_patched(launcher: &Path) -> bool {
    read_launcher(launcher).is_ok_and(|v| v["vmArgs"].as_array().is_some_and(|a| a.iter().any(|x| x.as_str().is_some_and(is_zb_arg))))
}

/// Whether the agent files in the game folder match the mod's copies.
pub fn files_current(launcher: &Path, libs: &Path) -> bool {
    agent_files().iter().all(|f| matches!((fs::read(libs.join(f)), fs::read(launcher.join(f))), (Ok(a), Ok(b)) if a == b))
}

/// Copy the agent files into the game folder and add the launcher flag. With `dry_run`
/// nothing changes; either way the returned lines describe each change.
pub fn install(launcher: &Path, libs: &Path, dry_run: bool) -> Result<Vec<String>> {
    let mut changes = vec![];
    for f in agent_files() {
        let src = libs.join(f);
        let dst = launcher.join(f);
        let new = fs::read(&src).with_context(|| format!("ZombieBuddy's {f} wasn't found in {}", libs.display()))?;
        if fs::read(&dst).is_ok_and(|old| old == new) {
            continue;
        }
        if dry_run {
            changes.push(format!("copy {f} to {}", launcher.display()));
        } else if crate::write_atomic(&dst, &new).is_ok() {
            changes.push(format!("copied {f} to {}", launcher.display()));
        } else if *f == "ZombieBuddy.jar" {
            // the game has it open: zbNative swaps in ZombieBuddy.jar.new on next launch
            fs::write(launcher.join("ZombieBuddy.jar.new"), &new)?;
            changes.push("staged ZombieBuddy.jar.new (the game is running; it takes effect on next launch)".into());
        } else {
            bail!("couldn't replace {} (close the game and try again)", dst.display());
        }
    }
    let mut v = read_launcher(launcher)?;
    let Some(args) = v["vmArgs"].as_array_mut() else { bail!("{LAUNCHER} has no vmArgs list") };
    if !args.iter().any(|x| x.as_str().is_some_and(is_zb_arg)) {
        changes.push(format!("add {} to {}", launch_arg(), launcher.join(LAUNCHER).display()));
        if !dry_run {
            args.insert(0, Value::from(launch_arg()));
            write_launcher(launcher, &v)?;
        }
    }
    Ok(changes)
}

/// Remove the launcher flag and the agent files from the game folder.
pub fn uninstall(launcher: &Path, dry_run: bool) -> Result<Vec<String>> {
    let mut changes = vec![];
    let mut v = read_launcher(launcher)?;
    if let Some(args) = v["vmArgs"].as_array_mut()
        && args.iter().any(|x| x.as_str().is_some_and(is_zb_arg))
    {
        changes.push(format!("remove the ZombieBuddy flag from {}", launcher.join(LAUNCHER).display()));
        if !dry_run {
            args.retain(|x| !x.as_str().is_some_and(is_zb_arg));
            write_launcher(launcher, &v)?;
        }
    }
    for f in ["zbNative.dll", "ZombieBuddy.jar", "ZombieBuddy.jar.new"] {
        let p = launcher.join(f);
        if p.exists() {
            changes.push(format!("delete {}", p.display()));
            if !dry_run {
                fs::remove_file(&p).with_context(|| format!("deleting {} (close the game and try again)", p.display()))?;
            }
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = "{\n\t\"mainClass\": \"zombie/gameStates/MainScreenState\",\n\t\"classpath\": [\".\"],\n\t\"vmArgs\": [\"-Xmx3072m\"],\n\t\"windows\": {\"10.0\": {\"vmArgs\": [\"-XX:+UseZGC\"]}}\n}\n";

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let game = t.path().join("game");
        let libs = t.path().join("ZombieBuddy/libs");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&libs).unwrap();
        fs::write(game.join(LAUNCHER), JSON).unwrap();
        for f in ["zbNative.dll", "ZombieBuddy.jar"] {
            fs::write(libs.join(f), f).unwrap();
        }
        (t, game, libs)
    }

    #[test]
    fn install_and_uninstall() {
        let (t, game, libs) = setup();
        assert_eq!(libs_dir(&t.path().join("ZombieBuddy")), Some(libs.clone()));
        assert_eq!(launcher_dir(&game), Some(game.clone()));

        let preview = install(&game, &libs, true).unwrap();
        assert_eq!(preview.len(), agent_files().len() + 1);
        assert!(!launcher_patched(&game) && !files_current(&game, &libs));

        install(&game, &libs, false).unwrap();
        assert!(launcher_patched(&game) && files_current(&game, &libs));
        let v = read_launcher(&game).unwrap();
        assert_eq!(v["vmArgs"][0], launch_arg());
        assert_eq!(v["windows"]["10.0"]["vmArgs"][0], "-XX:+UseZGC");
        let text = fs::read_to_string(game.join(LAUNCHER)).unwrap();
        assert!(text.starts_with("{\n\t\"mainClass\""), "key order and tabs kept: {text}");
        assert!(game.join(format!("{LAUNCHER}.pzmm-backup")).exists());
        assert!(install(&game, &libs, false).unwrap().is_empty(), "second run changes nothing");

        uninstall(&game, false).unwrap();
        assert!(!launcher_patched(&game) && !game.join("ZombieBuddy.jar").exists());
        assert_eq!(read_launcher(&game).unwrap()["vmArgs"], serde_json::json!(["-Xmx3072m"]));
    }

    #[test]
    fn only_the_official_item_supplies_the_agent() {
        assert!(official_source("steam:3619862853") && official_source("workshop:3619862853"));
        assert!(!official_source("manual") && !official_source("local:C:/x.zip") && !official_source("steam:1"));
    }

    #[test]
    fn recognises_flags_with_options() {
        assert!(is_zb_arg("-agentlib:zbNative") && is_zb_arg("-agentlib:zbNative=verbosity=1"));
        assert!(is_zb_arg("-javaagent:ZombieBuddy.jar=policy=deny-new"));
        assert!(!is_zb_arg("-agentlib:zbNativeX") && !is_zb_arg("-Xmx3072m"));
    }
}
