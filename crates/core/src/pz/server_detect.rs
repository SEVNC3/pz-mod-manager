//! Servers run by something else, found in a folder the user picks: LinuxGSM installs
//! (`lgsm/config-lgsm/pzserver/<instance>.cfg`, configs in `Zomboid/Server`) and Docker Compose
//! setups of the common images (Danixu, Renegade-Master, indifferentbroccoli: any compose file
//! that mentions Zomboid, with a volume holding `Server/<name>.ini`). PZMM manages their configs,
//! mods and backups; starting and stopping stays with them.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How a server PZMM doesn't run is run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct External {
    /// "linuxgsm" or "docker".
    pub kind: String,
    /// Where it's run from (the LinuxGSM home, the compose file's folder).
    pub folder: PathBuf,
    /// The commands that start, stop and restart it there.
    pub run: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub name: String,
    /// Its data folder (holds Server/, Saves/, db/).
    pub data: PathBuf,
    /// The dedicated server's files, when they're there.
    pub install: Option<PathBuf>,
    pub external: External,
}

/// The commands that update and check a server's files where it's run: (what, command). LinuxGSM's
/// `update` and `validate` run SteamCMD; the Docker images update the server when it starts.
pub fn upkeep(e: &External) -> Vec<(String, String)> {
    let pair = |k: &str, c: String| (k.to_string(), c);
    match e.kind.as_str() {
        "linuxgsm" => {
            let script = e.run.first().and_then(|r| r.split_whitespace().next()).unwrap_or("./pzserver");
            vec![pair("checkUpdate", format!("{script} check-update")), pair("update", format!("{script} update")), pair("validate", format!("{script} validate"))]
        }
        "docker" => vec![pair("pull", "docker compose pull".into()), pair("recreate", "docker compose up -d --force-recreate".into())],
        _ => vec![],
    }
}

/// The Workshop items a server has downloaded and when each was last updated (Unix seconds), from
/// `steamapps/workshop/appworkshop_108600.acf` in its install.
pub fn downloaded(install: &Path) -> std::collections::BTreeMap<String, u64> {
    let text = fs::read_to_string(install.join("steamapps/workshop/appworkshop_108600.acf")).unwrap_or_default();
    let mut out = std::collections::BTreeMap::new();
    let (mut inside, mut item) = (false, None::<String>);
    for l in text.lines().map(str::trim) {
        let words: Vec<&str> = l.split('"').filter(|w| !w.trim().is_empty()).collect();
        match words.as_slice() {
            ["WorkshopItemsInstalled"] => inside = true,
            ["WorkshopItemDetails"] => inside = false,
            [id] if inside && crate::sources::steam_workshop::valid_id(id) => item = Some(id.to_string()),
            ["timeupdated", t] if inside => {
                if let (Some(i), Ok(t)) = (item.take(), t.parse()) {
                    out.insert(i, t);
                }
            }
            _ => {}
        }
    }
    out
}

fn inis(data: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(data.join("Server")).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".ini").map(String::from)).collect();
    out.sort();
    out
}

fn linuxgsm(home: &Path) -> Vec<Found> {
    let cfg = home.join("lgsm/config-lgsm/pzserver");
    let mut instances: Vec<String> = fs::read_dir(&cfg)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".cfg").map(String::from))
        .filter(|n| !n.starts_with('_') && n != "common" && !n.starts_with("secrets-"))
        .collect();
    instances.sort();
    let data = home.join("Zomboid");
    let install = Some(home.join("serverfiles")).filter(|d| d.is_dir());
    // LinuxGSM names the ini after the instance (servercfg="${selfname}.ini")
    instances
        .into_iter()
        .filter(|i| data.join("Server").join(format!("{i}.ini")).is_file())
        .map(|i| Found {
            external: External { kind: "linuxgsm".into(), folder: home.into(), run: ["start", "stop", "restart"].iter().map(|c| format!("./{i} {c}")).collect() },
            name: i,
            data: data.clone(),
            install: install.clone(),
        })
        .collect()
}

/// A compose file's bind mounts, as host folders (relative ones from `dir`).
fn volumes(compose: &str, dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for l in compose.lines() {
        let l = l.trim();
        let host = if let Some(v) = l.strip_prefix("- ") {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
            // C:\x:/data on Windows hosts: the drive's colon isn't the separator
            let from = if v.as_bytes().get(1) == Some(&b':') { 2 } else { 0 };
            match v[from..].find(':') {
                Some(at) => &v[..from + at],
                None => continue,
            }
        } else if let Some(v) = l.strip_prefix("source:") {
            v.trim().trim_matches(|c| c == '"' || c == '\'')
        } else {
            continue;
        };
        if host.starts_with('.') || host.starts_with('/') || host.starts_with('~') || host.as_bytes().get(1) == Some(&b':') {
            let p = if host.starts_with('.') { dir.join(host) } else { PathBuf::from(host) };
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

fn docker(dir: &Path) -> Vec<Found> {
    let mut out = vec![];
    for f in ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"] {
        let Ok(text) = fs::read_to_string(dir.join(f)) else { continue };
        if !text.to_ascii_lowercase().contains("zomboid") {
            continue;
        }
        let vols = volumes(&text, dir);
        // the server's files: the mount holding ProjectZomboid64.json or start-server.sh
        let install = vols.iter().find(|v| v.join("ProjectZomboid64.json").is_file() || v.join("start-server.sh").is_file()).cloned();
        for v in &vols {
            for name in inis(v) {
                out.push(Found {
                    name,
                    data: v.clone(),
                    install: install.clone(),
                    external: External { kind: "docker".into(), folder: dir.into(), run: vec!["docker compose up -d".into(), "docker compose down".into(), "docker compose restart".into()] },
                });
            }
        }
    }
    out
}

/// Servers in `folder` or the folders just inside it.
pub fn detect(folder: &Path) -> Vec<Found> {
    let subs = fs::read_dir(folder).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path());
    let mut out = vec![];
    for d in std::iter::once(folder.to_path_buf()).chain(subs) {
        out.extend(linuxgsm(&d));
        out.extend(docker(&d));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upkeep_commands_and_downloads() {
        let lgsm = External { kind: "linuxgsm".into(), folder: "/home/pz".into(), run: vec!["./pzserver start".into()] };
        assert_eq!(upkeep(&lgsm).iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(), ["./pzserver check-update", "./pzserver update", "./pzserver validate"]);
        assert_eq!(upkeep(&External { kind: "docker".into(), ..Default::default() })[0].1, "docker compose pull");
        let t = tempfile::tempdir().unwrap();
        let acf = t.path().join("steamapps/workshop/appworkshop_108600.acf");
        fs::create_dir_all(acf.parent().unwrap()).unwrap();
        fs::write(&acf, "\"AppWorkshop\"\n{\n\t\"appid\"\t\t\"108600\"\n\t\"WorkshopItemsInstalled\"\n\t{\n\t\t\"2392709985\"\n\t\t{\n\t\t\t\"size\"\t\t\"100\"\n\t\t\t\"timeupdated\"\t\t\"1700000000\"\n\t\t}\n\t}\n\t\"WorkshopItemDetails\"\n\t{\n\t\t\"2392709985\"\n\t\t{\n\t\t\t\"timeupdated\"\t\t\"1\"\n\t\t}\n\t}\n}\n").unwrap();
        assert_eq!(downloaded(t.path()), std::collections::BTreeMap::from([("2392709985".to_string(), 1_700_000_000)]));
        assert!(downloaded(&t.path().join("none")).is_empty());
    }

    fn touch(p: PathBuf, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn finds_linuxgsm() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("pzserver");
        for f in ["_default.cfg", "common.cfg", "secrets-common.cfg", "pzserver.cfg", "pzserver-2.cfg"] {
            touch(home.join("lgsm/config-lgsm/pzserver").join(f), "");
        }
        touch(home.join("Zomboid/Server/pzserver.ini"), "PVP=true\n");
        touch(home.join("serverfiles/ProjectZomboid64.json"), "{}");
        let found = detect(t.path());
        assert_eq!(found.len(), 1, "pzserver-2 has no ini yet: {found:?}");
        let f = &found[0];
        assert_eq!((f.name.as_str(), f.data.clone(), f.install.clone()), ("pzserver", home.join("Zomboid"), Some(home.join("serverfiles"))));
        assert_eq!((f.external.kind.as_str(), f.external.run[0].as_str()), ("linuxgsm", "./pzserver start"));
    }

    #[test]
    fn finds_docker_compose_setups() {
        let t = tempfile::tempdir().unwrap();
        // Danixu's layout
        let a = t.path().join("danixu");
        touch(a.join("docker-compose.yml"), "services:\n  project-zomboid:\n    image: \"danixu86/project-zomboid-dedicated-server:latest\"\n    environment:\n      - \"SERVERNAME=myserver\"\n    volumes:\n      - ./ServerFiles:/home/steam/pz-dedicated\n      - ./ServerData:/home/steam/Zomboid\n");
        touch(a.join("ServerData/Server/myserver.ini"), "");
        touch(a.join("ServerFiles/start-server.sh"), "");
        // Renegade-Master's, long volume syntax
        let b = t.path().join("renegade");
        touch(b.join("compose.yaml"), "services:\n  zomboid:\n    image: renegademaster/zomboid-dedicated-server:latest\n    volumes:\n      - type: bind\n        source: ./ZomboidConfig\n        target: /home/steam/Zomboid/\n");
        touch(b.join("ZomboidConfig/Server/ZomboidServer.ini"), "");
        // something else entirely
        touch(t.path().join("web/docker-compose.yml"), "services:\n  web:\n    image: nginx\n    volumes:\n      - ./data:/srv\n");
        touch(t.path().join("web/data/Server/decoy.ini"), "");
        let mut found = detect(t.path());
        found.sort_by(|x, y| x.name.cmp(&y.name));
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["ZomboidServer", "myserver"]);
        assert_eq!((found[1].data.clone(), found[1].install.clone()), (a.join("./ServerData"), Some(a.join("./ServerFiles"))));
        assert_eq!((found[0].external.kind.as_str(), found[0].external.folder.clone(), found[0].install.clone()), ("docker", b.clone(), None));
        assert_eq!(found[0].external.run[0], "docker compose up -d");
    }

    #[test]
    fn compose_volumes() {
        let d = Path::new("/srv/pz");
        let v = volumes("    volumes:\n      - './data:/x:rw'\n      - C:\\pz\\cfg:/home/steam/Zomboid\n      - named:/y\n      - /abs/path:/z\n    ports:\n      - \"16261:16261/udp\"\n", d);
        assert_eq!(v, [d.join("./data"), PathBuf::from("C:\\pz\\cfg"), PathBuf::from("/abs/path")]);
    }
}
