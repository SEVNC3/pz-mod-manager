//! The built `pzmm` end to end in a scratch folder: its data, Zomboid, game and settings folders all
//! point there (nothing real is read or written, nothing is downloaded).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    let settings = root.join("appdata/pzmm");
    std::fs::create_dir_all(&settings).unwrap();
    std::fs::create_dir_all(root.join("workshop")).unwrap();
    // the Workshop folder has no environment variable: Settings point it at the scratch one
    let workshop = serde_json::json!({ "folders": { "workshop": root.join("workshop") } });
    std::fs::write(settings.join("settings.json"), workshop.to_string()).unwrap();
    root
}

fn pzmm(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pzmm"))
        .args(args)
        .env("APPDATA", root.join("appdata"))
        .env("PZMM_DATA", root.join("data"))
        .env("PZ_HOME", root.join("Zomboid"))
        .env("PZ_GAME_DIR", root.join("game"))
        .env("PZ_SERVER_DIR", root.join("server"))
        .env_remove("GITHUB_TOKEN")
        .output()
        .unwrap()
}

fn ok(root: &Path, args: &[&str]) -> String {
    let out = pzmm(root, args);
    assert!(out.status.success(), "pzmm {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn json(root: &Path, args: &[&str]) -> serde_json::Value {
    serde_json::from_str(&ok(root, &[args, &["--json"]].concat())).unwrap()
}

#[test]
fn installs_switches_and_lists() {
    let root = scratch("smoke");
    let src = root.join("src/Lamp");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("mod.info"), "id=Lamp\nname=Lamp\n").unwrap();

    ok(&root, &["add", &src.display().to_string()]);
    assert!(root.join("Zomboid/mods/Lamp/mod.info").is_file());
    let list = json(&root, &["list"]);
    assert_eq!((list["mods"][0]["id"].as_str(), list["mods"][0]["enabled"].as_bool()), (Some("Lamp"), Some(true)));

    ok(&root, &["disable", "Lamp"]);
    assert_eq!(json(&root, &["list"])["mods"][0]["enabled"], false);

    ok(&root, &["enable", "Lamp"]);
    ok(&root, &["profile", "new", "Second"]);
    ok(&root, &["profile", "use", "second"]);
    assert_eq!(json(&root, &["list"])["mods"][0]["enabled"], false, "a new profile starts empty");
    let mode = json(&root, &["mode"]);
    assert_eq!((mode["profile"].as_str(), mode["mode"].as_str()), (Some("Second"), Some("zomboid")));

    let bad = pzmm(&root, &["add", &root.join("nothing here").display().to_string()]);
    assert!(!bad.status.success(), "nothing installed is an error");
    assert!(!pzmm(&root, &["server", "explode", "x"]).status.success());
    assert!(ok(&root, &["--help"]).contains("Only in the app"));
}
