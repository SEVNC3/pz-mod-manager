//! The game's performance: its memory (`-Xmx` in ProjectZomboid64.json) and PZ_Optimization
//! (github.com/xD3I/PZ_Optimization): class files its installer copies next to projectzomboid.jar,
//! each build made for one game revision. PZMM runs the installer the official Workshop item ships.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::zombiebuddy::{read_launcher, write_launcher};

pub const PZOPT_ITEM: &str = "3805285544";
pub const PZOPT_ID: &str = "PZ_Optimization";

/// `-Xmx<n>[k|m|g]` in MB.
fn xmx_mb(arg: &str) -> Option<u32> {
    let v = arg.strip_prefix("-Xmx")?.to_ascii_lowercase();
    let (n, unit) = match v.char_indices().last()? {
        (i, c) if c.is_ascii_alphabetic() => (&v[..i], c),
        _ => (v.as_str(), 'b'),
    };
    let n: u64 = n.parse().ok()?;
    let mb = match unit {
        'g' => n * 1024,
        'm' => n,
        'k' => n / 1024,
        'b' => n >> 20,
        _ => return None,
    };
    u32::try_from(mb).ok()
}

/// The game's `-Xmx` in MB (None: not set), and whether PZ_Optimization's heap option set it.
pub fn memory(launcher: &Path) -> Result<(Option<u32>, bool)> {
    let v = read_launcher(launcher)?;
    let args: Vec<&str> = v["vmArgs"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    Ok((args.iter().rev().find_map(|a| xmx_mb(a)), args.iter().any(|a| a.starts_with("-Dpzopt.heap="))))
}

/// Set the game's `-Xmx` (the JSON before PZMM's first change stays as `.pzmm-backup`).
pub fn set_memory(launcher: &Path, mb: u32) -> Result<()> {
    if mb < 1024 {
        bail!("the game needs at least 1 GB");
    }
    let mut v = read_launcher(launcher)?;
    let Some(args) = v["vmArgs"].as_array_mut() else { bail!("ProjectZomboid64.json has no vmArgs list") };
    let arg = Value::from(format!("-Xmx{mb}m"));
    match args.iter().position(|a| a.as_str().is_some_and(|a| a.starts_with("-Xmx"))) {
        Some(i) => {
            args.retain(|a| !a.as_str().is_some_and(|a| a.starts_with("-Xmx")));
            args.insert(i.min(args.len()), arg);
        }
        None => args.push(arg),
    }
    write_launcher(launcher, &v)
}

/// The garbage collectors offered (`-XX:+Use<name>GC`); the game picks Z on Windows 10 and later.
const GCS: [&str; 3] = ["Z", "G1", "Shenandoah"];

fn gc_of(arg: &str) -> Option<&str> {
    arg.strip_prefix("-XX:+Use")?.strip_suffix("GC")
}

/// Every `vmArgs` list in the launcher JSON: the main one and the per-OS-version ones.
fn vm_lists(v: &mut Value, f: &mut impl FnMut(&mut Vec<Value>)) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                match x {
                    Value::Array(a) if k == "vmArgs" => f(a),
                    _ => vm_lists(x, f),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| vm_lists(x, f)),
        _ => {}
    }
}

/// The game's JVM options beyond memory: its garbage collector (the main list's, else the last
/// per-OS one, which is the newest Windows) and the main `vmArgs` other than `-Xmx` and the collector.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Jvm {
    pub gc: String,
    pub args: Vec<String>,
}

pub fn jvm(launcher: &Path) -> Result<Jvm> {
    let mut v = read_launcher(launcher)?;
    let main: Vec<String> = v["vmArgs"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    let mut last = None;
    vm_lists(&mut v, &mut |l| {
        if let Some(g) = l.iter().filter_map(Value::as_str).find_map(gc_of) {
            last = Some(g.to_string());
        }
    });
    let gc = main.iter().find_map(|a| gc_of(a)).map(String::from).or(last).unwrap_or_default();
    Ok(Jvm { gc, args: main.into_iter().filter(|a| xmx_mb(a).is_none() && gc_of(a).is_none()).collect() })
}

/// Set the collector (`gc`, one of `GCS`) wherever the JSON names one (else in the main list) and
/// the main list's other options to `args` (its `-Xmx` stays). The JSON before PZMM's first change
/// stays as `.pzmm-backup`.
pub fn set_jvm(launcher: &Path, gc: &str, args: &[String]) -> Result<()> {
    if !GCS.contains(&gc) {
        bail!("unknown garbage collector {gc:?}");
    }
    if let Some(a) = args.iter().find(|a| !a.starts_with('-') || a.contains(char::is_whitespace) || xmx_mb(a).is_some() || gc_of(a).is_some()) {
        bail!("{a:?} isn't a JVM option PZMM can set here: one per line, starting with -, no spaces (memory and the collector have their own settings)");
    }
    let mut v = read_launcher(launcher)?;
    let Some(main) = v["vmArgs"].as_array() else { bail!("ProjectZomboid64.json has no vmArgs list") };
    let flag = format!("-XX:+Use{gc}GC");
    let in_main = main.iter().filter_map(Value::as_str).any(|a| gc_of(a).is_some());
    let mut elsewhere = false;
    vm_lists(&mut v, &mut |l| {
        for a in l.iter_mut().filter(|a| a.as_str().is_some_and(|a| gc_of(a).is_some())) {
            *a = Value::from(flag.as_str());
            elsewhere = true;
        }
    });
    let main = v["vmArgs"].as_array_mut().context("vmArgs")?;
    let xmx: Vec<Value> = main.iter().filter(|a| a.as_str().is_some_and(|a| xmx_mb(a).is_some())).cloned().collect();
    *main = args.iter().map(|a| Value::from(a.as_str())).chain(xmx).collect();
    if in_main || !elsewhere {
        main.push(Value::from(flag));
    }
    write_launcher(launcher, &v)
}

/// What to give the game: 3 GB (its own default), 1 GB more per 50 mods, up to 16 GB, and never
/// more than the machine minus 4 GB (Windows, the graphics driver, Steam).
pub fn suggest_mb(ram_mb: u64, mods: usize) -> u32 {
    let want = 3072 + (mods as u64 / 50) * 1024;
    let room = ram_mb.saturating_sub(4096).max(2048);
    (want.min(16384).min(room) / 512 * 512) as u32
}

/// `zombie/GitVersion`'s REVISION in the jar: 10 hex digits, a length-prefixed constant.
fn game_revision(dir: &Path) -> Option<String> {
    let mut z = zip::ZipArchive::new(fs::File::open(dir.join("projectzomboid.jar")).ok()?).ok()?;
    let mut b = vec![];
    z.by_name("zombie/GitVersion.class").ok()?.read_to_end(&mut b).ok()?;
    let hex = |c: &u8| c.is_ascii_digit() || (b'a'..=b'f').contains(c);
    b.windows(12).find(|w| w[..2] == [0, 10] && w[2..].iter().all(hex)).map(|w| String::from_utf8_lossy(&w[2..]).into_owned())
}

#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct PzOpt {
    /// The installed game's revision (None: no game found).
    pub game_revision: Option<String>,
    /// The revision the installed build was made for (None: not installed).
    pub installed: Option<String>,
    /// The official Workshop copy's folder with its installer, and the revision it's built for.
    pub installer: Option<PathBuf>,
    pub installer_revision: Option<String>,
}

fn script() -> &'static str {
    if cfg!(windows) { "install.ps1" } else { "install.bash" }
}

/// `key=value` from a properties or manifest line starting with `prefix`.
fn value(text: &str, prefix: &str) -> Option<String> {
    text.lines().find_map(|l| l.trim().strip_prefix(prefix)).map(|v| v.split_whitespace().next().unwrap_or("").to_string()).filter(|v| !v.is_empty())
}

/// PZ_Optimization in the game folder `dir`; `copy`: the official Workshop item's version folder
/// (where its mod.info and installer are).
pub fn pzopt(dir: &Path, copy: Option<&Path>) -> PzOpt {
    let installer = copy.filter(|c| c.join(script()).is_file()).map(Path::to_path_buf);
    PzOpt {
        game_revision: game_revision(dir),
        installed: fs::read_to_string(dir.join("pzopt-installed.txt")).ok().map(|t| value(&t, "# revision=").unwrap_or_default()),
        installer_revision: installer.as_ref().and_then(|c| fs::read_to_string(c.join("pzopt-classes/pzopt/build-info.properties")).ok()).and_then(|t| value(&t, "revision=")),
        installer,
    }
}

/// Run the Workshop copy's installer on the game folder `dir` (or its uninstall). The game must be
/// closed (the installer would wait for it). Returns what it printed.
pub fn pzopt_run(dir: &Path, installer: &Path, uninstall: bool) -> Result<String> {
    let s = installer.join(script());
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&s).arg("-Dir").arg(dir);
        if uninstall {
            c.arg("-Uninstall");
        }
        c
    } else {
        let mut c = std::process::Command::new("bash");
        c.arg(&s).arg("--dir").arg(dir);
        if uninstall {
            c.arg("--uninstall");
        }
        c
    };
    cmd.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().with_context(|| format!("running {}", s.display()))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if !out.status.success() {
        bail!("{text}");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_setting() {
        assert_eq!([xmx_mb("-Xmx3072m"), xmx_mb("-Xmx8g"), xmx_mb("-Xmx8G"), xmx_mb("-Xmx2097152k"), xmx_mb("-Xmx1073741824"), xmx_mb("-Xms2g"), xmx_mb("-Xmxg")], [Some(3072), Some(8192), Some(8192), Some(2048), Some(1024), None, None]);
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("ProjectZomboid64.json"), "{\n\t\"mainClass\": \"zombie/gameStates/MainScreenState\",\n\t\"vmArgs\": [\"-Djava.awt.headless=true\", \"-Xmx3072m\", \"-Dzomboid.steam=1\"]\n}\n").unwrap();
        assert_eq!(memory(t.path()).unwrap(), (Some(3072), false));
        set_memory(t.path(), 6144).unwrap();
        let v = read_launcher(t.path()).unwrap();
        assert_eq!(v["vmArgs"], serde_json::json!(["-Djava.awt.headless=true", "-Xmx6144m", "-Dzomboid.steam=1"]), "in place");
        assert!(t.path().join("ProjectZomboid64.json.pzmm-backup").exists());
        assert!(set_memory(t.path(), 512).is_err());
    }

    #[test]
    fn jvm_options() {
        let t = tempfile::tempdir().unwrap();
        // the game's own shape: the collector per Windows version
        let stock = r#"{"mainClass":"zombie/gameStates/MainScreenState","vmArgs":["-Djava.awt.headless=true","-Xmx3072m","-Dzomboid.steam=1","-XX:-CreateCoredumpOnCrash"],
            "windows":{"6.1":{"vmArgs":["-XX:+UseG1GC"]},"10.0.17134":{"vmArgs":["-XX:+UseZGC"]}}}"#;
        fs::write(t.path().join("ProjectZomboid64.json"), stock).unwrap();
        let j = jvm(t.path()).unwrap();
        assert_eq!((j.gc.as_str(), j.args.clone()), ("Z", vec!["-Djava.awt.headless=true".to_string(), "-Dzomboid.steam=1".into(), "-XX:-CreateCoredumpOnCrash".into()]));
        let args = vec!["-Djava.awt.headless=true".to_string(), "-Dzomboid.steam=1".into(), "-XX:+AlwaysPreTouch".into()];
        set_jvm(t.path(), "Shenandoah", &args).unwrap();
        let v = read_launcher(t.path()).unwrap();
        assert_eq!(v["windows"]["6.1"]["vmArgs"], serde_json::json!(["-XX:+UseShenandoahGC"]), "every collector flag changes, never two at once");
        assert_eq!(v["windows"]["10.0.17134"]["vmArgs"], serde_json::json!(["-XX:+UseShenandoahGC"]));
        assert_eq!(v["vmArgs"], serde_json::json!(["-Djava.awt.headless=true", "-Dzomboid.steam=1", "-XX:+AlwaysPreTouch", "-Xmx3072m"]), "memory stays");
        assert_eq!(jvm(t.path()).unwrap(), Jvm { gc: "Shenandoah".into(), args });
        assert!(t.path().join("ProjectZomboid64.json.pzmm-backup").exists());
        // no collector anywhere: it goes in the main list
        fs::write(t.path().join("ProjectZomboid64.json"), r#"{"vmArgs":["-Xmx2g"]}"#).unwrap();
        set_jvm(t.path(), "G1", &[]).unwrap();
        assert_eq!(read_launcher(t.path()).unwrap()["vmArgs"], serde_json::json!(["-Xmx2g", "-XX:+UseG1GC"]));
        for bad in [vec!["-Xmx4g".to_string()], vec!["-XX:+UseZGC".into()], vec!["nope".into()], vec!["-Da=1 -Db=2".into()]] {
            assert!(set_jvm(t.path(), "G1", &bad).is_err(), "{bad:?}");
        }
        assert!(set_jvm(t.path(), "Epsilon", &[]).is_err());
    }

    #[test]
    fn memory_advice() {
        assert_eq!(suggest_mb(16384, 0), 3072, "the game's own default");
        assert_eq!(suggest_mb(16384, 260), 8192, "5 GB more for 260 mods");
        assert_eq!(suggest_mb(8192, 400), 4096, "4 GB stay for the rest of the machine");
        assert_eq!(suggest_mb(65536, 2000), 16384);
        assert_eq!(suggest_mb(4096, 10), 2048);
    }

    #[test]
    fn finds_pz_optimization() {
        let t = tempfile::tempdir().unwrap();
        let (game, copy) = (t.path().join("game"), t.path().join("item/mods/PZ_Optimization/42"));
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(copy.join("pzopt-classes/pzopt")).unwrap();
        // GitVersion's constant pool holds REVISION as a 10-character UTF-8 constant
        let mut z = zip::ZipWriter::new(fs::File::create(game.join("projectzomboid.jar")).unwrap());
        z.start_file("zombie/GitVersion.class", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut z, b"\xca\xfe\xba\xbe\x01\x00\x08REVISION\x01\x00\x0a4a0e9546ec\x01\x00\x0aLineNumber").unwrap();
        z.finish().unwrap();
        assert_eq!(pzopt(&game, Some(&copy)), PzOpt { game_revision: Some("4a0e9546ec".into()), ..Default::default() }, "no installer, not installed");
        fs::write(copy.join(script()), "").unwrap();
        fs::write(copy.join("pzopt-classes/pzopt/build-info.properties"), "built=1\nrevision=4a0e9546ec\n").unwrap();
        fs::write(game.join("pzopt-installed.txt"), "# files written by install.ps1 - do not edit\n# revision=b0bbce05d5 installed=2026-10-01T00:00:00Z\npzopt/A.class 00\n").unwrap();
        let p = pzopt(&game, Some(&copy));
        assert_eq!((p.installed.as_deref(), p.installer_revision.as_deref(), p.installer.as_deref()), (Some("b0bbce05d5"), Some("4a0e9546ec"), Some(copy.as_path())), "an old build, and an update waiting");
    }
}
