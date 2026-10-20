//! Hosting dedicated servers: the servers PZMM manages (`<data>/servers.json`), installing one
//! (SteamCMD, app 380870), running it (java directly, never the .bat: cmd.exe splits
//! `-cachedir=` at the `=` and the bat drops arguments), its log, console and players.
//!
//! A server keeps its data (Server/, Saves/, db/, backups/) in the Zomboid folder unless it has its
//! own data folder (`-cachedir`). Configs in `Zomboid/Server` that PZMM doesn't manage are listed
//! read-only until adopted.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::Paths;
use super::server::{self, ServerMods};
use crate::sources::steam_workshop;
use crate::store::now;

/// The dedicated server's Steam app.
pub const APP: &str = "380870";
/// Build 41's branch; Build 42 is the default branch.
pub const LEGACY: &str = "legacy41";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    pub name: String,
    /// "b42" (the default branch) or [`LEGACY`].
    pub build: String,
    pub install_dir: PathBuf,
    /// Its own data folder (`-cachedir`); None: the Zomboid folder.
    pub cachedir: Option<PathBuf>,
    pub game_port: u16,
    pub direct_port: u16,
    /// The JVM's `-Xmx`.
    pub memory_mb: u32,
    pub admin_user: String,
    /// While it runs: the java process (so a later PZMM finds it again) and its start (Unix seconds).
    pub pid: Option<u32>,
    pub started: u64,
    /// Update checks, schedules, backups, crash watch, Discord.
    pub auto: super::autopilot::Auto,
    /// Run by LinuxGSM or Docker: PZMM manages its files but never starts or stops it.
    pub external: Option<super::server_detect::External>,
    /// Leave the PZMM list out of its public description (see `tag`).
    pub no_tag: bool,
    /// The address players join at, when it isn't this machine's (a tunnel such as playit.gg); "": none.
    pub address: String,
    /// Mods marked "server" (only the server needs them) or "optional" (players may leave them
    /// out); the rest are needed on both sides. The game itself still loads every `Mods=` entry:
    /// this tells PZMM players' joins what to skip (see `tag`).
    pub sides: BTreeMap<String, String>,
}

impl Server {
    pub fn cachedir(&self, paths: &Paths) -> PathBuf {
        self.cachedir.clone().unwrap_or_else(|| paths.zomboid.clone())
    }

    pub fn ini(&self, paths: &Paths) -> PathBuf {
        self.cachedir(paths).join("Server").join(format!("{}.ini", self.name))
    }

    pub fn build_number(&self) -> u8 {
        if self.build == LEGACY { 41 } else { 42 }
    }
}

fn list_file(paths: &Paths) -> PathBuf {
    paths.data.join("servers.json")
}

/// The servers PZMM manages (none when servers.json can't be read; changes refuse then, see `update`).
pub fn load(paths: &Paths) -> Vec<Server> {
    crate::read_json(&list_file(paths)).unwrap_or_default()
}

pub fn find(paths: &Paths, name: &str) -> Option<Server> {
    load(paths).into_iter().find(|s| s.name == name)
}

/// Whether a server PZMM manages or one run elsewhere is called `name` (their passwords, ini and
/// history go by the name, so two can't share one).
fn name_taken(paths: &Paths, name: &str) -> bool {
    find(paths, name).is_some() || remote(paths, name).is_some()
}

/// Change a server's automation settings.
pub fn set_auto(paths: &Paths, name: &str, f: impl FnOnce(&mut super::autopilot::Auto)) -> Result<()> {
    let mut found = false;
    update(paths, |all| {
        if let Some(s) = all.iter_mut().find(|s| s.name == name) {
            f(&mut s.auto);
            found = true;
        }
    })?;
    if !found {
        bail!("PZMM doesn't manage a server named {name}");
    }
    Ok(())
}

/// Manage a server LinuxGSM or Docker runs (see `server_detect`): its configs, mods and backups.
pub fn add_external(paths: &Paths, f: &super::server_detect::Found) -> Result<Server> {
    server::path(paths, &f.name)?; // the name's rules
    if name_taken(paths, &f.name) {
        bail!("PZMM already has a server named {}", f.name);
    }
    let text = std::fs::read_to_string(f.data.join("Server").join(format!("{}.ini", f.name))).with_context(|| format!("reading {}'s ini", f.name))?;
    let port = |key, default| server::value(&text, key).parse().unwrap_or(default);
    let s = Server {
        name: f.name.clone(),
        build: f.install.as_deref().and_then(install_build).unwrap_or("b42").into(),
        install_dir: f.install.clone().unwrap_or_default(),
        cachedir: Some(f.data.clone()),
        game_port: port("DefaultPort", 16261),
        direct_port: port("UDPPort", 16262),
        external: Some(f.external.clone()),
        ..Default::default()
    };
    update(paths, |all| all.push(s.clone()))?;
    Ok(s)
}

/// Change the list under PZMM's data lock (the app and the CLI may both write it).
fn update(paths: &Paths, f: impl FnOnce(&mut Vec<Server>)) -> Result<()> {
    let _lock = crate::lock_data(paths)?;
    let mut all: Vec<Server> = crate::read_json(&list_file(paths))?;
    f(&mut all);
    crate::write_atomic(&list_file(paths), serde_json::to_string_pretty(&all)?)?;
    Ok(())
}

pub fn default_install_dir(paths: &Paths, build: &str) -> PathBuf {
    paths.data.join("servers").join(if build == LEGACY { LEGACY } else { "b42" })
}

// --- install (SteamCMD) ---

/// SteamCMD's `progress: 12.34 (...)`, as a percentage.
fn install_progress(line: &str) -> Option<f32> {
    line.split("progress: ").nth(1)?.split_whitespace().next()?.parse().ok()
}

/// Whether `dir` holds an installed server (its launcher settings, which PZMM starts java from).
pub fn installed(dir: &Path) -> bool {
    dir.join("ProjectZomboid64.json").is_file()
}

/// Install or update the server in `dir` anonymously with SteamCMD (`validate` checks every file).
/// Progress goes to the progress sink as a percentage; setting `stop` ends SteamCMD.
pub fn install(paths: &Paths, dir: &Path, build: &str, stop: &AtomicBool) -> Result<()> {
    if !dir.is_absolute() {
        bail!("the install folder must be a full path: {}", dir.display());
    }
    std::fs::create_dir_all(dir)?;
    let exe = steam_workshop::steamcmd(paths)?;
    let mut cmd = Command::new(&exe);
    cmd.arg("+force_install_dir").arg(dir).args(["+login", "anonymous", "+app_update", APP]);
    if build == LEGACY {
        cmd.args(["-beta", LEGACY]);
    }
    cmd.args(["validate", "+quit"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    hide_window(&mut cmd);
    let _one = steam_workshop::STEAMCMD.lock().unwrap_or_else(|e| e.into_inner());
    crate::say!("serverInstalling", "installing the dedicated server into {dir} (about 2 GB to download)...", dir = dir.display());
    crate::progress(0, 100);
    let mut child = cmd.spawn().with_context(|| format!("running {}", exe.display()))?;
    let out = child.stdout.take().context("SteamCMD's output")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut ok = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(line) => {
                if let Some(p) = install_progress(&line) {
                    crate::progress(p as usize, 100);
                } else if !line.trim().is_empty() {
                    crate::log!("{}", line.trim());
                }
                ok |= line.contains(&format!("App '{APP}' fully installed"));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if stop.load(Relaxed) => {
                let _ = child.kill();
                let _ = child.wait();
                bail!("the server install was stopped");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = child.wait();
    if !ok || !installed(dir) {
        bail!("SteamCMD didn't finish installing the server (its output is in the activity log)");
    }
    crate::progress(100, 100);
    Ok(())
}

// --- the command line ---

/// How to start java for a server: the program, its arguments and the folder it runs in.
#[derive(Debug, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

impl Launch {
    /// The command line to show or log: the admin password masked.
    pub fn shown(&self) -> String {
        let mut out = vec![self.program.display().to_string()];
        let mut hide = false;
        for a in &self.args {
            out.push(if hide { "********".into() } else if a.contains(' ') { format!("\"{a}\"") } else { a.clone() });
            hide = a == "-adminpassword";
        }
        out.join(" ")
    }
}

/// The java command the server's own launcher settings (`ProjectZomboid64.json` in `install`, read
/// as `json`) describe, with PZMM's memory instead of theirs, `-cachedir=` as one argument, and
/// the admin account (`(user, password)`) for the first start.
pub fn launch(install: &Path, json: &str, cachedir: &Path, name: &str, admin: Option<(&str, &str)>, memory_mb: u32) -> Result<Launch> {
    let v: serde_json::Value = serde_json::from_str(json).context("reading ProjectZomboid64.json")?;
    let strings = |v: &serde_json::Value| v.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect::<Vec<_>>();
    let main = v["mainClass"].as_str().filter(|m| !m.is_empty()).context("ProjectZomboid64.json has no mainClass")?.replace('/', ".");
    let classpath = strings(&v["classpath"]);
    if classpath.is_empty() {
        bail!("ProjectZomboid64.json has no classpath");
    }
    if !cachedir.is_absolute() {
        bail!("the data folder must be a full path: {}", cachedir.display());
    }
    // PZMM sets the memory: the file's own -Xmx/-Xms are dropped
    let mut args: Vec<String> = strings(&v["vmArgs"]).into_iter().filter(|a| !a.starts_with("-Xmx") && !a.starts_with("-Xms")).collect();
    if cfg!(windows) {
        // per Windows version ("7", "10"): the newest one's extras (B42: ZGC)
        let newest = v["windows"].as_object().and_then(|o| o.iter().max_by_key(|(k, _)| k.parse::<u32>().unwrap_or(0)));
        args.extend(newest.map(|(_, w)| strings(&w["vmArgs"])).unwrap_or_default());
    }
    args.push(format!("-Xmx{memory_mb}m"));
    args.extend(["-cp".into(), classpath.join(if cfg!(windows) { ";" } else { ":" }), main]);
    args.extend([format!("-cachedir={}", cachedir.display()), "-servername".into(), name.into()]);
    if let Some((user, pw)) = admin {
        args.extend(["-adminusername".into(), user.into(), "-adminpassword".into(), pw.into()]);
    }
    let java = install.join("jre64").join("bin").join(if cfg!(windows) { "java.exe" } else { "java" });
    Ok(Launch { program: java, args, cwd: install.to_path_buf() })
}

// --- the admin password, kept in the OS credential store like the GitHub token ---

fn admin_target(name: &str) -> String {
    format!("server-admin:{name}")
}

fn save_admin_password(name: &str, password: &str) -> Result<()> {
    crate::settings::set_secret(&admin_target(name), password).context("saving the admin password")
}

/// The saved admin password. Unit tests never read the credential store.
pub fn admin_password(name: &str) -> Option<String> {
    crate::settings::secret(&admin_target(name))
}

fn forget_admin_password(name: &str) {
    if !cfg!(test) {
        let _ = crate::settings::set_secret(&admin_target(name), "");
    }
}

/// A random password: 20 letters and digits, none that read alike (0/O, 1/l/I).
pub fn generate_password() -> String {
    const ABC: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 20];
    getrandom::fill(&mut bytes).expect("the OS random number generator");
    // 256 % 57 leaves a slight bias; at 20 characters it costs well under a bit of strength
    bytes.iter().map(|b| ABC[*b as usize % ABC.len()] as char).collect()
}

// --- creating one ---

/// What the setup wizard asks for.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewServer {
    pub name: String,
    pub build: String,
    /// None: [`default_install_dir`].
    pub install_dir: Option<PathBuf>,
    /// None: the Zomboid folder.
    pub cachedir: Option<PathBuf>,
    pub game_port: u16,
    pub direct_port: u16,
    pub admin_user: String,
    pub admin_password: String,
    pub join_password: String,
    pub max_players: u32,
    pub public: bool,
    pub upnp: bool,
    pub memory_mb: u32,
    pub lines: ServerMods,
    /// The address players join at when it isn't this machine's (a tunnel's); "": none.
    pub address: String,
}

/// The ini lines a new server starts with; the server adds every other setting (and the sandbox
/// files) with their defaults on its first start.
fn new_ini(n: &NewServer, rcon_port: u16, rcon_password: &str) -> Vec<(&'static str, String)> {
    let mut set = vec![
        ("PublicName", n.name.clone()),
        ("DefaultPort", n.game_port.to_string()),
        ("UDPPort", n.direct_port.to_string()),
        ("UPnP", n.upnp.to_string()),
        ("Public", n.public.to_string()),
        ("MaxPlayers", n.max_players.to_string()),
        ("Password", n.join_password.clone()),
        ("RCONPort", rcon_port.to_string()),
        ("RCONPassword", rcon_password.into()),
    ];
    if n.lines.maps.is_empty() {
        set.push(("Map", server::BASE_MAP.into()));
    }
    set
}

/// Check the wizard's answers, install the server when its folder doesn't have one, write
/// `<data folder>/Server/<name>.ini`, keep the admin password in the credential store and add it to
/// PZMM's servers. The first start is the caller's.
pub fn create(paths: &Paths, n: &NewServer, stop: &AtomicBool) -> Result<Server> {
    server::path(paths, &n.name)?; // the name's rules
    if name_taken(paths, &n.name) {
        bail!("PZMM already has a server named {}", n.name);
    }
    let cachedir = n.cachedir.clone().filter(|c| !c.as_os_str().is_empty());
    if cachedir.as_ref().is_some_and(|c| !c.is_absolute()) {
        bail!("the data folder must be a full path");
    }
    let s = Server {
        name: n.name.clone(),
        build: if n.build == LEGACY { LEGACY.into() } else { "b42".into() },
        install_dir: n.install_dir.clone().filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| default_install_dir(paths, &n.build)),
        cachedir,
        game_port: n.game_port,
        direct_port: n.direct_port,
        memory_mb: n.memory_mb.max(1024),
        admin_user: n.admin_user.trim().to_string(),
        address: clean_address(&n.address)?,
        ..Default::default()
    };
    let ini = s.ini(paths);
    if ini.exists() {
        bail!("{} already exists: add that server with Adopt instead", ini.display());
    }
    if n.game_port == 0 || n.direct_port == 0 || n.game_port == n.direct_port {
        bail!("the game port and the direct port must be two different ports");
    }
    if s.admin_user.is_empty() || s.admin_user.contains(|c: char| c.is_whitespace() || c == '"') || n.admin_password.trim().is_empty() {
        bail!("the admin needs a user name (no spaces or quotes) and a password");
    }
    if let Some(w) = n.lines.workshop.iter().find(|w| !steam_workshop::valid_id(w)) {
        bail!("{w} isn't a Workshop ID: a bad ID crashes the server");
    }
    // checked before anything is saved or installed (set_values refuses them too, but late)
    let values = new_ini(n, free_rcon_port(paths), &generate_password());
    let lines = server::line_values(&n.lines.mods, &n.lines.workshop, &n.lines.maps, s.build == LEGACY);
    if let Some((key, _)) = values.iter().chain(&lines).find(|(_, v)| v.contains(['\n', '\r'])) {
        bail!("{key} must be one line");
    }
    // first: a password that can't be kept stops the create before anything is written
    if !cfg!(test) {
        save_admin_password(&s.name, &n.admin_password)?;
    }
    let made = (|| -> Result<()> {
        if !installed(&s.install_dir) {
            install(paths, &s.install_dir, &s.build, stop)?;
        }
        server::set_values(&ini, &values)?;
        server::set_values(&ini, &lines)?;
        update(paths, |all| all.push(s.clone()))
    })();
    if made.is_err() {
        forget_admin_password(&s.name);
    }
    made.map(|()| s)
}

/// The first RCON port from 27015 that no other server PZMM manages is set to (two servers on one
/// port: the second can't take RCON, and PZMM's commands for it reach the first).
pub(crate) fn free_rcon_port(paths: &Paths) -> u16 {
    let taken: Vec<u16> = load(paths)
        .iter()
        // a server without the line (or its ini) uses the game's default
        .map(|s| std::fs::read_to_string(s.ini(paths)).ok().and_then(|t| server::value(&t, "RCONPort").parse().ok()).unwrap_or(27015))
        .collect();
    (27015..u16::MAX).find(|p| !taken.contains(p)).unwrap_or(27015)
}

/// Where each mod on server `s`'s `Mods=` line (`ids`) keeps its `media/` (its newest version
/// folder and `common/`), with its id and name: the server's own Workshop downloads first, else
/// the copies in `installed`.
pub fn mod_roots(s: &Server, ids: &[String], installed: &BTreeMap<String, crate::store::Row>) -> Vec<(String, String, Vec<PathBuf>)> {
    let own = if s.install_dir.as_os_str().is_empty() { vec![] } else { super::workshop_mods(&s.install_dir.join("steamapps/workshop/content/108600")) };
    ids.iter()
        .filter_map(|id| {
            let (name, dir, info) = match own.iter().find(|m| &m.id == id) {
                Some(m) => (m.name.clone(), m.dir.clone(), m.info.clone()),
                None => installed.get(id).map(|r| (r.name.clone(), r.dir.clone(), r.info.clone()))?,
            };
            let mut roots: Vec<PathBuf> = [info.parent().map(Path::to_path_buf), Some(dir.join("common"))].into_iter().flatten().filter(|r| r.is_dir()).collect();
            roots.dedup();
            Some((id.clone(), if name.is_empty() { id.clone() } else { name }, roots))
        })
        .collect()
}

/// Update a stopped server's files with SteamCMD (`validate` checks every file) from `build`'s
/// branch. A switch of branch rewrites its mod lines in that build's form. Every server run from the
/// same folder must be stopped, and none of the others may be on the other build.
pub fn update_server(paths: &Paths, name: &str, build: &str, stop: &AtomicBool) -> Result<()> {
    let s = find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    if let Some(e) = &s.external {
        bail!("{name} is run by {}: update it there", e.kind);
    }
    let build = if build == LEGACY { LEGACY } else { "b42" };
    let all = load(paths);
    let sharing: Vec<&Server> = all.iter().filter(|o| o.install_dir == s.install_dir).collect();
    // a crashed server can't be stopped, and its Java is gone
    if let Some(o) = sharing.iter().find(|o| reserved(&o.name) || !matches!(state(o).0, Status::Stopped | Status::Crashed)) {
        bail!(crate::keyed!("stopForUpdate", "Stop {name} first: SteamCMD can't replace the files of a running server.", name = o.name));
    }
    // none of them starts (or has its files changed) until the update is done
    let _held = sharing.iter().map(|o| reserve(&o.name)).collect::<Result<Vec<_>>>()?;
    if let Some(o) = sharing.iter().find(|o| o.name != name && o.build != build) {
        bail!("{} runs from the same folder on the other build: switching would change it too", o.name);
    }
    install(paths, &s.install_dir, build, stop)?;
    if s.build != build {
        update(paths, |all| {
            for o in all.iter_mut().filter(|o| o.name == name) {
                o.build = build.into();
            }
        })?;
        let ini = s.ini(paths);
        if ini.is_file() {
            let _lock = crate::lock_data(paths)?; // the app's config editor writes it under the lock too
            let l = server::read(&ini)?;
            server::write(&ini, &l.mods, &l.workshop, &l.maps, build == LEGACY)?;
        }
    }
    super::autopilot::record(paths, name, "server", if build == LEGACY { "41" } else { "42" });
    crate::say!("serverUpdated", "Updated {name}'s server files", name = name);
    Ok(())
}

/// A new server with `from`'s settings (ini, sandbox and spawn files, not its world) in the same data
/// folder and install, with its automation and admin password. RCON gets a port and password of its
/// own; the game ports stay the same: change them before running both at once.
pub fn clone_server(paths: &Paths, from: &str, to: &str) -> Result<Server> {
    server::path(paths, to)?; // the name's rules
    let s = find(paths, from).with_context(|| format!("PZMM doesn't manage a server named {from}"))?;
    if let Some(e) = &s.external {
        bail!("{from} is run by {}: copy it there", e.kind);
    }
    if name_taken(paths, to) {
        bail!("PZMM already has a server named {to}");
    }
    let mut auto = s.auto.clone();
    // the webhook is kept per server
    auto.discord = super::discord::Settings { saved: false, message_id: String::new(), ..auto.discord };
    let new = Server { name: to.into(), pid: None, started: 0, auto, ..s.clone() };
    let rcon_port = free_rcon_port(paths);
    let dir = s.cachedir(paths).join("Server");
    let file = |n: &str, suffix: &str| dir.join(format!("{n}{suffix}"));
    if file(to, ".ini").exists() {
        bail!("{} already exists: add that server with Adopt instead", file(to, ".ini").display());
    }
    if !file(from, ".ini").is_file() {
        bail!("{from} has no settings yet: start it once first");
    }
    for suffix in [".ini", "_SandboxVars.lua", "_spawnregions.lua", "_spawnpoints.lua"] {
        let Ok(mut text) = std::fs::read_to_string(file(from, suffix)) else { continue };
        if suffix == "_spawnregions.lua" {
            // its own spawn points file goes by the server's name
            text = text.replace(&format!("\"{from}_spawnpoints.lua\""), &format!("\"{to}_spawnpoints.lua\""));
        }
        crate::write_atomic(&file(to, suffix), &text)?;
        if suffix == ".ini" {
            // its own RCON: a shared port or password sends PZMM's commands for one to the other
            let rcon_pw = if server::value(&text, "RCONPassword").is_empty() { String::new() } else { generate_password() };
            server::set_values(&file(to, suffix), &[("RCONPort", rcon_port.to_string()), ("RCONPassword", rcon_pw)])?;
        }
    }
    if let Some(pw) = admin_password(from) {
        save_admin_password(to, &pw)?;
    }
    update(paths, |all| all.push(new.clone()))?;
    crate::say!("serverCloned", "Made {name} from {from}'s settings", name = to, from = from);
    Ok(new)
}

/// The co-op worlds hosted from the game on this machine (`Zomboid/Saves/Multiplayer/<name>`),
/// leaving out the servers PZMM manages there.
/// `from` is the Zomboid folder the game plays in.
pub fn coop_worlds(paths: &Paths, from: &Path) -> Vec<String> {
    let managed: Vec<String> = load(paths).into_iter().filter(|s| super::bridge::same_dir(&s.cachedir(paths), from)).map(|s| s.name).collect();
    let mut out: Vec<String> = std::fs::read_dir(from.join("Saves/Multiplayer"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| !n.ends_with("_player") && !managed.iter().any(|m| m.eq_ignore_ascii_case(n)))
        .collect();
    out.sort();
    out
}

/// Make co-op world `world` server `name`'s world: its save (with the host's `_player` folder), its
/// accounts, and the mod lines and sandbox it was played with. The server's own world is backed up
/// first and replaced; its other settings (ports, passwords) stay.
pub fn migrate(paths: &Paths, name: &str, world: &str, from: &Path) -> Result<()> {
    let s = find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    check_stopped(paths, &s)?;
    let _held = reserve(name)?;
    if !coop_worlds(paths, from).iter().any(|w| w == world) {
        bail!("there's no co-op world named {world}");
    }
    // the game may be saving the co-op world mid-copy
    if !cfg!(test) && game_running() {
        bail!("close the game first: it may be saving {world}");
    }
    let to = s.cachedir(paths);
    // its own world (folders ignore case on Windows): copying it onto itself would delete it
    if world.eq_ignore_ascii_case(name) && super::bridge::same_dir(from, &to) {
        bail!("{world} is {name}'s own world already");
    }
    let mp = to.join("Saves/Multiplayer");
    if [mp.join(name), mp.join(format!("{name}_player")), to.join("db").join(format!("{name}.db"))].iter().any(|p| p.exists()) {
        super::server_backup::create(paths, &s, "migrate")?;
    }
    for (a, b) in [(world.to_string(), name.to_string()), (format!("{world}_player"), format!("{name}_player"))] {
        let (src, dst) = (from.join("Saves/Multiplayer").join(a), to.join("Saves/Multiplayer").join(b));
        if dst.exists() {
            std::fs::remove_dir_all(&dst).with_context(|| format!("removing {}", dst.display()))?;
        }
        if src.is_dir() {
            crate::store::copy_dir(&src, &dst)?;
        }
    }
    let db = from.join("db").join(format!("{world}.db"));
    if db.is_file() {
        std::fs::create_dir_all(to.join("db"))?;
        std::fs::copy(&db, to.join("db").join(format!("{name}.db")))?;
    }
    // the world needs the mods and rules it was played with
    let (coop, ini) = (from.join("Server"), s.ini(paths));
    let _lock = crate::lock_data(paths)?; // the app's config editor writes them under the lock too
    if let Ok(l) = server::read(&coop.join(format!("{world}.ini")))
        && ini.is_file()
    {
        super::server_config::keep_prev(&ini)?;
        server::write(&ini, &l.mods, &l.workshop, &l.maps, s.build == LEGACY)?;
    }
    let sandbox = coop.join(format!("{world}_SandboxVars.lua"));
    let own = ini.with_file_name(format!("{name}_SandboxVars.lua"));
    if sandbox.is_file() {
        if own.is_file() {
            super::server_config::keep_prev(&own)?;
        }
        std::fs::copy(&sandbox, &own)?;
    }
    crate::say!("serverMigrated", "{name} now runs the co-op world {world}", name = name, world = world);
    Ok(())
}

/// The script that starts `l` from a terminal, for this system: a .bat on Windows, else a .sh.
fn bundle_text(l: &Launch) -> String {
    if cfg!(windows) {
        let q = |a: &str| format!("\"{}\"", a.replace('%', "%%"));
        let args: Vec<String> = l.args.iter().map(|a| q(a)).collect();
        format!("@echo off\r\nrem Starts the server as PZMM does.\r\ncd /d {}\r\n{} {}\r\npause\r\n", q(&l.cwd.display().to_string()), q(&l.program.display().to_string()), args.join(" "))
    } else {
        let q = |a: &str| format!("'{}'", a.replace('\'', "'\\''"));
        let args: Vec<String> = l.args.iter().map(|a| q(a)).collect();
        let cwd = q(&l.cwd.display().to_string());
        format!("#!/bin/sh\n# Starts the server as PZMM does.\ncd {cwd} || exit 1\nexport LD_LIBRARY_PATH=\"$PWD/linux64:$PWD/natives:$PWD:$PWD/jre64/lib/amd64\"\nexec {} {}\n", q(&l.program.display().to_string()), args.join(" "))
    }
}

/// Write a script into `dir` that starts server `name` the way PZMM does (without the admin
/// password: the server has it after its first start). Returns the file.
pub fn bundle(paths: &Paths, name: &str, dir: &Path) -> Result<PathBuf> {
    let s = find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    if let Some(e) = &s.external {
        bail!("{name} is run by {}: it has its own start command", e.kind);
    }
    let json = std::fs::read_to_string(s.install_dir.join("ProjectZomboid64.json")).with_context(|| format!("{} has no dedicated server", s.install_dir.display()))?;
    let l = launch(&s.install_dir, &json, &s.cachedir(paths), name, None, s.memory_mb)?;
    let file = dir.join(format!("start-{name}.{}", if cfg!(windows) { "bat" } else { "sh" }));
    std::fs::write(&file, bundle_text(&l))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(file)
}

/// A support bundle for one server: its record, its ini with passwords, tokens and webhooks
/// masked, its mod lines and the end of its log; the user's name masked in paths.
pub fn diagnostics(paths: &Paths, name: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let ini = match server::path(paths, name) {
        Ok(ini) => ini,
        Err(e) => return format!("{e}\n"),
    };
    match find(paths, name) {
        Some(s) => {
            let run_by = s.external.as_ref().map(|e| format!(", run by {}", e.kind)).unwrap_or_default();
            let _ = writeln!(out, "Server {name}: {:?}{run_by}, build {}, ports {}/{}, memory {} MB", state(&s).0, s.build, s.game_port, s.direct_port, s.memory_mb);
            let _ = writeln!(out, "Install: {}\nData: {}", s.install_dir.display(), s.cachedir(paths).display());
        }
        None => {
            let _ = writeln!(out, "Server {name}: a config PZMM doesn't manage");
        }
    }
    let secret = |k: &str| ["password", "token", "webhook", "secret"].iter().any(|w| k.to_ascii_lowercase().contains(w));
    let text = std::fs::read_to_string(&ini).unwrap_or_default();
    let _ = writeln!(out, "\n--- {} ---", ini.display());
    for l in text.lines().filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty()) {
        let _ = match l.split_once('=') {
            Some((k, v)) if secret(k) && !v.trim().is_empty() => writeln!(out, "{k}=********"),
            _ => writeln!(out, "{l}"),
        };
    }
    let lines = log(paths, name, 0).1;
    let tail = &lines[lines.len().saturating_sub(200)..];
    let _ = writeln!(out, "\n--- the last {} log lines ---", tail.len());
    for l in tail {
        // the console echoes `adduser "name" "password"`
        let _ = match l.text.to_ascii_lowercase().find("adduser ") {
            Some(i) => writeln!(out, "{}adduser ********", &l.text[..i]),
            None => writeln!(out, "{}", l.text),
        };
    }
    super::doctor::mask_home_user(&out)
}

/// The build a server install runs: Build 42 ships the game as `java/projectzomboid.jar`, Build 41
/// as loose classes. None without a server there.
pub fn install_build(dir: &Path) -> Option<&'static str> {
    let json = std::fs::read_to_string(dir.join("ProjectZomboid64.json")).ok()?;
    Some(if json.contains("projectzomboid.jar") { "b42" } else { LEGACY })
}

/// Manage a server config PZMM didn't make (`Zomboid/Server/<name>.ini`), run from `install_dir`
/// (its build is the install's).
pub fn adopt(paths: &Paths, name: &str, install_dir: &Path) -> Result<Server> {
    if remote(paths, name).is_some() {
        bail!("PZMM already has a server named {name}");
    }
    let ini = server::path(paths, name)?;
    let text = std::fs::read_to_string(&ini).with_context(|| format!("reading {}", ini.display()))?;
    let Some(build) = install_build(install_dir) else {
        bail!("{} has no dedicated server (ProjectZomboid64.json)", install_dir.display());
    };
    let port = |key, default| server::value(&text, key).parse().unwrap_or(default);
    let s = Server {
        name: name.into(),
        build: build.into(),
        install_dir: install_dir.into(),
        cachedir: None,
        game_port: port("DefaultPort", 16261),
        direct_port: port("UDPPort", 16262),
        memory_mb: default_memory_mb(total_ram_mb()),
        admin_user: "admin".into(),
        ..Default::default()
    };
    update(paths, |all| {
        all.retain(|o| o.name != name);
        all.push(s.clone());
    })?;
    Ok(s)
}

/// Stop managing a server (its files stay); its saved admin password, Discord webhook and bot
/// token are forgotten.
pub fn forget(paths: &Paths, name: &str) -> Result<()> {
    // one left running from an earlier session too: PZMM would lose track of its java
    if attached(name) || find(paths, name).is_some_and(|s| state(&s).0 == Status::Running) {
        bail!(crate::keyed!("stopFirst", "Stop {name} first.", name = name));
    }
    // nor while it starts, updates or is restored
    let _held = reserve(name)?;
    update(paths, |all| all.retain(|s| s.name != name))?;
    forget_admin_password(name);
    if !cfg!(test) {
        let _ = super::discord::save(name, "");
        let _ = super::discord_bot::save_token(name, "");
    }
    Ok(())
}

/// The mod lines for a pasted collection (its items and what they require) on `build`: mod IDs
/// from each item's mod.info, its maps, and notes (see `collection::read`). The player loaded it
/// (a button, not a preview): the sandbox presets it ships are saved as theirs.
pub fn lines_from_collection(paths: &Paths, input: &str, build: u8, stop: &std::sync::atomic::AtomicBool) -> Result<(ServerMods, Vec<String>)> {
    let (title, items) = super::collection::items(paths, input)?;
    let mut pack = super::collection::read(paths, &title, &items, build, &[], stop)?;
    super::collection::save_presets(paths, &mut pack);
    Ok((ServerMods { mods: pack.mods, workshop: pack.items, maps: pack.maps }, pack.notes))
}

/// Workshop items the `start` items require that `have` lacks, and what those require in turn:
/// (the item that needs it, the required item). `requires` gives an item's required items.
pub fn missing_required(have: &[String], start: &[String], requires: impl Fn(&str) -> Vec<String>) -> Vec<(String, String)> {
    let mut seen: HashSet<String> = have.iter().chain(start).cloned().collect();
    let mut queue: VecDeque<String> = start.iter().cloned().collect();
    let mut out: Vec<(String, String)> = vec![];
    // ponytail: 500 items is far past any real server list; stops a runaway chain of page reads
    while let Some(id) = queue.pop_front().filter(|_| out.len() < 500) {
        for r in requires(&id) {
            if seen.insert(r.clone()) {
                out.push((id.clone(), r.clone()));
                queue.push_back(r);
            }
        }
    }
    out
}

/// `lines` with the Workshop items in `input` added (links or IDs; a collection gives its items) and the items they require
/// that the list lacks: their mod IDs (see `collection::read`) and maps. `first`: the new mods go
/// before the listed ones (they're what those need), else after them. Also notes.
pub fn lines_with(paths: &Paths, lines: &ServerMods, input: &str, build: u8, first: bool, stop: &std::sync::atomic::AtomicBool) -> Result<(ServerMods, Vec<String>)> {
    let items = super::collection::ids(input)?;
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    let members = steam_workshop::collections(&refs)?;
    let mut new: Vec<String> = vec![];
    for i in items.iter().flat_map(|i| members.get(i).cloned().unwrap_or_else(|| vec![i.clone()])) {
        if !lines.workshop.contains(&i) && !new.contains(&i) {
            new.push(i);
        }
    }
    steam_workshop::warm(&new.iter().map(String::as_str).collect::<Vec<_>>());
    let required = missing_required(&lines.workshop, &new, |id| {
        steam_workshop::required_items(id).unwrap_or_else(|e| {
            crate::log!("error: what workshop:{id} requires: {e:#}");
            vec![]
        })
    });
    new.extend(required.into_iter().map(|(_, r)| r));
    if new.is_empty() {
        return Ok((lines.clone(), vec![]));
    }
    let pack = super::collection::read(paths, "", &new, build, &[], stop)?;
    let added: Vec<String> = pack.mods.into_iter().filter(|m| !lines.mods.contains(m)).collect();
    let mods = if first { added.iter().chain(&lines.mods).cloned().collect() } else { lines.mods.iter().chain(&added).cloned().collect() };
    let mut workshop = lines.workshop.clone();
    workshop.extend(new.iter().cloned());
    let mod_maps: Vec<String> = lines.maps.iter().chain(&pack.maps).filter(|m| !server::is_base_map(m) && !super::maps::is_vanilla(m)).cloned().collect();
    let maps = if pack.maps.iter().all(|m| server::is_base_map(m) || lines.maps.contains(m)) { lines.maps.clone() } else { server::map_order(&lines.maps, &mod_maps) };
    Ok((ServerMods { mods, workshop, maps }, pack.notes))
}

// --- servers run elsewhere, reached over RCON ---

/// A server on another machine: PZMM reaches it over RCON only (its console and players).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Remote {
    pub name: String,
    pub host: String,
    pub port: u16,
    /// "b42" or [`LEGACY`]: some commands differ.
    pub build: String,
    /// Its files over FTP, when set (the password is in the credential store).
    pub ftp: Option<super::ftp::Ftp>,
}

fn remotes_file(paths: &Paths) -> PathBuf {
    paths.data.join("remote-servers.json")
}

fn remotes(paths: &Paths) -> Vec<Remote> {
    crate::read_json(&remotes_file(paths)).unwrap_or_default()
}

pub fn remote(paths: &Paths, name: &str) -> Option<Remote> {
    remotes(paths).into_iter().find(|r| r.name == name)
}

fn rcon_secret(name: &str) -> String {
    format!("server-rcon:{name}")
}

fn set_remotes(paths: &Paths, f: impl FnOnce(&mut Vec<Remote>)) -> Result<()> {
    let _lock = crate::lock_data(paths)?;
    let mut all: Vec<Remote> = crate::read_json(&remotes_file(paths))?;
    f(&mut all);
    crate::write_atomic(&remotes_file(paths), serde_json::to_string_pretty(&all)?)?;
    Ok(())
}

/// Add a server run elsewhere once its RCON login works; `password` goes to the credential store
/// (not in tests).
pub fn add_remote(paths: &Paths, r: &Remote, password: &str) -> Result<()> {
    server::path(paths, &r.name)?; // the name's rules
    let host = r.host.trim();
    if host.is_empty() || host.contains(char::is_whitespace) {
        bail!("type the server's address (a host name or IP)");
    }
    if name_taken(paths, &r.name) || server::list(paths).contains(&r.name) {
        bail!("there's already a server named {}", r.name);
    }
    super::rcon::Rcon::connect(host, r.port, password)?;
    if !cfg!(test) {
        crate::settings::set_secret(&rcon_secret(&r.name), password)?;
    }
    set_remotes(paths, |all| all.push(Remote { host: host.into(), ..r.clone() }))
}

pub fn forget_remote(paths: &Paths, name: &str) -> Result<()> {
    set_remotes(paths, |all| all.retain(|r| r.name != name))?;
    if !cfg!(test) {
        crate::settings::set_secret(&rcon_secret(name), "")?;
        super::ftp::save_password(name, "")?;
    }
    Ok(())
}

/// Change a server run elsewhere's FTP settings.
pub fn set_ftp(paths: &Paths, name: &str, f: impl FnOnce(&mut Option<super::ftp::Ftp>)) -> Result<()> {
    if remote(paths, name).is_none() {
        bail!("there's no server run elsewhere named {name}");
    }
    set_remotes(paths, |all| {
        if let Some(r) = all.iter_mut().find(|r| r.name == name) {
            f(&mut r.ftp);
        }
    })
}

// --- the PZMM list in the description ---

/// The PZMM list for these mod lines: the Workshop items with their update times now.
pub fn tag_for(lines: &ServerMods, sides: &BTreeMap<String, String>) -> Result<super::tag::Tag> {
    let ids: Vec<&str> = lines.workshop.iter().map(String::as_str).filter(|w| steam_workshop::valid_id(w)).collect();
    let details = ids.chunks(100).map(steam_workshop::details).collect::<Result<Vec<_>>>()?.concat();
    let time = |i: &str| details.iter().find(|d| d.id == i).map_or(0, |d| d.time_updated);
    let side = |s: &str| lines.mods.iter().filter(|m| sides.get(*m).is_some_and(|v| v == s)).cloned().collect();
    Ok(super::tag::Tag { items: ids.iter().map(|i| (i.to_string(), time(i))).collect(), mods: lines.mods.clone(), maps: lines.maps.clone(), server_only: side("server"), optional: side("optional") })
}

/// Put `tag` at the end of the ini's `PublicDescription=` (replacing an old one), or with None
/// take it out; the owner's text stays. Returns the form that fit (see `tag::with_tag`).
pub fn set_tag(ini: &Path, tag: Option<&super::tag::Tag>) -> Result<&'static str> {
    let text = std::fs::read_to_string(ini).with_context(|| format!("reading {}", ini.display()))?;
    let desc = server::value(&text, "PublicDescription");
    let (new, form) = match tag {
        Some(t) => super::tag::with_tag(desc, t),
        None => (super::tag::strip(desc).to_string(), "none"),
    };
    if new != desc {
        server::set_values(ini, &[("PublicDescription", new)])?;
    }
    Ok(form)
}

/// Refresh a server's list before it starts (its items' times are the ones it's about to download).
fn refresh_tag(paths: &Paths, s: &Server) {
    if s.no_tag {
        return;
    }
    let ini = s.ini(paths);
    // the Workshop is asked outside the lock; the ini's read-modify-write happens under it
    let tag = server::read(&ini).and_then(|l| tag_for(&l, &s.sides));
    match tag.and_then(|t| {
        let _lock = crate::lock_data(paths)?;
        set_tag(&ini, Some(&t))
    }) {
        Ok("none") => crate::say!("tagTooBig", "{name}'s mod list is too long for its description, so PZMM players match its mods by ID", name = s.name),
        Ok(_) => {}
        Err(e) => crate::say!("tagFailed", "error: couldn't update the mod list in {name}'s description: {error}", name = s.name, error = format!("{e:#}")),
    }
}

/// An address players type to join: a host name or IP with an optional port, nothing else.
fn clean_address(a: &str) -> Result<String> {
    let a = a.trim();
    if a.len() > 120 || a.contains(|c: char| c.is_whitespace() || c.is_control() || "/\\\"'<>@".contains(c)) {
        bail!("{a} isn't an address players can join at (a host name or IP, with :port when it isn't 16261)");
    }
    Ok(a.to_string())
}

/// Set the address players join at ("": none).
pub fn set_address(paths: &Paths, name: &str, address: &str) -> Result<()> {
    let address = clean_address(address)?;
    let mut found = false;
    update(paths, |all| {
        if let Some(s) = all.iter_mut().find(|s| s.name == name) {
            s.address = address;
            found = true;
        }
    })?;
    if !found {
        bail!("PZMM doesn't manage a server named {name}");
    }
    Ok(())
}

/// Turn the list in a server's description on or off (off takes it out now; on puts it in at the next start).
pub fn set_tag_on(paths: &Paths, name: &str, on: bool) -> Result<()> {
    let mut found = None;
    update(paths, |all| {
        if let Some(s) = all.iter_mut().find(|s| s.name == name) {
            s.no_tag = !on;
            found = Some(s.clone());
        }
    })?;
    let s = found.with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    if !on && std::fs::exists(s.ini(paths)).unwrap_or(false) {
        let _lock = crate::lock_data(paths)?;
        set_tag(&s.ini(paths), None)?;
    }
    Ok(())
}

/// Mark a server's mods "server" or "optional" (others: both sides).
pub fn set_sides(paths: &Paths, name: &str, sides: BTreeMap<String, String>) -> Result<()> {
    if let Some((id, v)) = sides.iter().find(|(_, v)| !matches!(v.as_str(), "server" | "optional")) {
        bail!("unknown side {v:?} for {id}");
    }
    let mut found = false;
    update(paths, |all| {
        if let Some(s) = all.iter_mut().find(|s| s.name == name) {
            s.sides = sides;
            found = true;
        }
    })?;
    if !found {
        bail!("PZMM doesn't manage a server named {name}");
    }
    Ok(())
}

// --- the machine ---

/// The ports a server would bind: UDP for the game, TCP for RCON. False when something holds it.
pub fn port_free(port: u16, udp: bool) -> bool {
    // Windows asks about every new program listening on all interfaces, and a bind on loopback
    // succeeds beside a server's on all of them: read the table of ports in use instead
    #[cfg(windows)]
    {
        let mut cmd = Command::new("netstat");
        cmd.args(["-an", "-p", if udp { "UDP" } else { "TCP" }]);
        hide_window(&mut cmd);
        let Ok(out) = cmd.output() else { return true };
        // "  UDP    0.0.0.0:16261    *:*" (the column titles are in the system's language)
        let proto = if udp { "UDP" } else { "TCP" };
        !String::from_utf8_lossy(&out.stdout).lines().any(|l| {
            let mut cols = l.split_whitespace();
            cols.next() == Some(proto) && cols.next().and_then(|a| a.rsplit_once(':')).is_some_and(|(_, p)| p == port.to_string())
        })
    }
    #[cfg(not(windows))]
    {
        if udp { std::net::UdpSocket::bind(("0.0.0.0", port)).is_ok() } else { std::net::TcpListener::bind(("0.0.0.0", port)).is_ok() }
    }
}

/// Memory installed, in MB (0 when unknown).
pub fn total_ram_mb() -> u64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct MemoryStatusEx {
            length: u32,
            load: u32,
            total_phys: u64,
            rest: [u64; 6],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
        }
        let mut m = MemoryStatusEx { length: std::mem::size_of::<MemoryStatusEx>() as u32, load: 0, total_phys: 0, rest: [0; 6] };
        // SAFETY: a MEMORYSTATUSEX-sized buffer with its length set, as the call requires
        if unsafe { GlobalMemoryStatusEx(&mut m) } != 0 { m.total_phys >> 20 } else { 0 }
    }
    #[cfg(not(windows))]
    {
        let info = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        info.lines().find_map(|l| l.strip_prefix("MemTotal:")?.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()).map_or(0, |kb| kb >> 10)
    }
}

/// A starting `-Xmx`: half the machine (the game or Windows needs the rest), 2-8 GB.
pub fn default_memory_mb(total_mb: u64) -> u32 {
    ((total_mb / 2).clamp(2048, 8192) / 512 * 512) as u32
}

fn hide_window(_cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
}

/// A running process's program file name, memory in bytes (the working set) and start (Unix
/// seconds). Asked of Windows directly: the autopilot and the server cards ask every few seconds.
#[cfg(windows)]
fn process_info(pid: u32) -> Option<(String, u64, u64)> {
    #[repr(C)]
    struct MemoryCounters {
        size: u32,
        faults: u32,
        peak: usize,
        working_set: usize,
        rest: [usize; 6],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn GetExitCodeProcess(process: isize, code: *mut u32) -> i32;
        fn GetProcessTimes(process: isize, created: *mut u64, exited: *mut u64, kernel: *mut u64, user: *mut u64) -> i32;
        fn QueryFullProcessImageNameW(process: isize, flags: u32, name: *mut u16, size: *mut u32) -> i32;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut MemoryCounters, size: u32) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    // SAFETY: a handle opened to query only (PROCESS_QUERY_LIMITED_INFORMATION), closed after
    // calls that each fill buffers of the sizes they're given
    unsafe {
        let h = OpenProcess(0x1000, 0, pid);
        if h == 0 {
            return None;
        }
        let (mut code, mut created, mut exited, mut kernel, mut user) = (0u32, 0u64, 0u64, 0u64, 0u64);
        let mut name = [0u16; 1024];
        let mut len = name.len() as u32;
        let mut m = MemoryCounters { size: std::mem::size_of::<MemoryCounters>() as u32, faults: 0, peak: 0, working_set: 0, rest: [0; 6] };
        // an ended process stays queryable while something holds a handle to it: 259 is STILL_ACTIVE
        let ok = GetExitCodeProcess(h, &mut code) != 0
            && code == 259
            && GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user) != 0
            && QueryFullProcessImageNameW(h, 0, name.as_mut_ptr(), &mut len) != 0
            && K32GetProcessMemoryInfo(h, &mut m, m.size) != 0;
        CloseHandle(h);
        let path = String::from_utf16_lossy(&name[..len as usize]);
        // FILETIME: 100 ns units since 1601
        ok.then(|| (path.rsplit('\\').next().unwrap_or("").to_string(), m.working_set as u64, (created / 10_000_000).saturating_sub(11_644_473_600)))
    }
}

/// When `pid` is a running java: its memory in bytes (the working set). None when it isn't.
pub fn java_memory(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    {
        process_info(pid).filter(|(exe, ..)| exe.eq_ignore_ascii_case("java.exe")).map(|(_, memory, _)| memory)
    }
    #[cfg(not(windows))]
    {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        if !status.lines().next()?.contains("java") {
            return None;
        }
        Some(status.lines().find_map(|l| l.strip_prefix("VmRSS:")?.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()).unwrap_or(0) * 1024)
    }
}

/// Server `s`'s java found again by the pid it saved (PZMM started it in an earlier session). None
/// when that java has gone, or the pid now belongs to another process.
fn saved_java(s: &Server) -> Option<u32> {
    let pid = s.pid?;
    #[cfg(windows)]
    {
        // started with the server: a later process given the same pid started later
        let (exe, _, created) = process_info(pid)?;
        (exe.eq_ignore_ascii_case("java.exe") && created.abs_diff(s.started) < 120).then_some(pid)
    }
    #[cfg(not(windows))]
    {
        java_memory(pid)?;
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        runs_server(&String::from_utf8_lossy(&cmdline).replace('\0', " "), &s.name).then_some(pid)
    }
}

/// Whether the game (the client, not a server) is running on this machine.
pub fn game_running() -> bool {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("tasklist");
        cmd.args(["/FI", "IMAGENAME eq ProjectZomboid64.exe", "/NH", "/FO", "CSV"]);
        hide_window(&mut cmd);
        cmd.output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("\"projectzomboid64.exe\""))
    }
    #[cfg(not(windows))]
    {
        std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
            // the launcher binary starts the JVM in its own process
            std::fs::read(e.path().join("cmdline")).is_ok_and(|c| {
                let c = String::from_utf8_lossy(&c);
                // the Windows game under Proton/Wine: ProjectZomboid64.exe
                c.split('\0').next().is_some_and(|exe| exe.ends_with("ProjectZomboid64") || exe.ends_with("ProjectZomboid64.exe")) || c.contains("zombie.gameStates.MainScreenState")
            })
        })
    }
}

/// The CPU time `pid` has used (kernel and user), in 100 ns units. None when it can't be read.
pub fn cpu_time(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
            fn GetProcessTimes(process: isize, created: *mut u64, exited: *mut u64, kernel: *mut u64, user: *mut u64) -> i32;
            fn CloseHandle(handle: isize) -> i32;
        }
        // SAFETY: a handle opened to query only (PROCESS_QUERY_LIMITED_INFORMATION), closed after one
        // call that fills four FILETIMEs (two little-endian u32s each, read as u64)
        unsafe {
            let h = OpenProcess(0x1000, 0, pid);
            if h == 0 {
                return None;
            }
            let (mut created, mut exited, mut kernel, mut user) = (0u64, 0u64, 0u64, 0u64);
            let ok = GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user);
            CloseHandle(h);
            (ok != 0).then_some(kernel + user)
        }
    }
    #[cfg(not(windows))]
    {
        // after the name in brackets: state, then utime and stime are the 12th and 13th fields
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let f: Vec<u64> = stat.rsplit_once(')')?.1.split_whitespace().skip(11).take(2).filter_map(|v| v.parse().ok()).collect();
        // ponytail: assumes 100 clock ticks a second (sysconf(_SC_CLK_TCK) on every common Linux)
        (f.len() == 2).then(|| (f[0] + f[1]) * 100_000)
    }
}

fn kill_pid(pid: u32) {
    let mut cmd = if cfg!(windows) { Command::new("taskkill") } else { Command::new("kill") };
    if cfg!(windows) {
        cmd.args(["/F", "/PID", &pid.to_string()]);
    } else {
        cmd.args(["-9", &pid.to_string()]);
    }
    hide_window(&mut cmd);
    let _ = cmd.output();
}

// --- the log ---

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// A known log line in plain words: `key` is `hosting.note.<key>` in the app's locale files.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Note {
    pub key: &'static str,
    pub text: String,
    pub vars: BTreeMap<&'static str, String>,
}

fn note(key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Option<Note> {
    Some(Note { key, text: crate::fill(template, vars), vars: vars.iter().cloned().collect() })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Line {
    pub seq: u64,
    pub text: String,
    pub level: Level,
    pub note: Option<Note>,
}

const STARTED: &str = "*** SERVER STARTED";
const TERMINATED: &str = "Server Terminated.";

/// The server's own "Server Terminated." line: its message (B42's prefix stripped), or the end of a line in
/// another format (B41's `...> Server Terminated.`). Text a chat or servermsg echo carries isn't it.
fn terminated(text: &str) -> bool {
    message(text).trim() == TERMINATED || text.trim_end().ends_with(&format!("> {TERMINATED}"))
}
const ADMIN_PROMPT: &str = "Enter new administrator password";

/// A server log line's level, and what it means when it's one PZMM knows.
pub fn classify(text: &str) -> (Level, Option<Note>) {
    let t = text.trim_start();
    // the server's own line only: a chat message can say it too
    let terminated = terminated(text);
    let level = if t.starts_with("ERROR") || t.contains("Exception") || terminated {
        Level::Error
    } else if t.starts_with("WARN") {
        Level::Warn
    } else {
        Level::Info
    };
    let after = |pat: &str| text.split(pat).nth(1).map(|s| s.chars().take_while(|c| *c != '"' && !c.is_whitespace()).collect::<String>());
    let n = if text.contains(STARTED) {
        note("started", "The server is up: players can join.", &[])
    } else if terminated {
        note("terminated", "The server stopped on an error it can't recover from (often a mod's scripts, see the errors above), and Java hangs after it: PZMM stops Java.", &[])
    } else if text.contains("there are script load errors") {
        note("scriptErrors", "A mod's scripts failed to load: the errors just above name the file.", &[])
    } else if let Some(id) = after("onItemNotDownloaded itemID=") {
        let result = after("result=").unwrap_or_default();
        if result == "9" {
            note("badItem", "Workshop item {id} doesn't exist (a bad Workshop ID): remove it from WorkshopItems.", &[("id", id)])
        } else {
            note("itemFailed", "Workshop item {id} couldn't be downloaded (Steam result {result}).", &[("id", id), ("result", result)])
        }
    } else if let Some(id) = text.split("required mod \"").nth(1).and_then(|s| s.split('"').next()).filter(|_| text.contains("not found")) {
        note("modMissing", "Mod ID {id} isn't in any of the server's Workshop items: check Mods and WorkshopItems.", &[("id", id.to_string())])
    } else if text.contains("timeUpdated doesn't match") {
        note("outdated", "A Workshop mod changed since the last start: the server downloads the new version.", &[])
    } else if text.contains("CheckModsNeedUpdate: Mods need update") {
        note("needUpdate", "A Workshop mod has an update: restart the server so players can join.", &[])
    } else if text.contains("CheckModsNeedUpdate: Mods updated") {
        note("upToDate", "Every Workshop mod is up to date.", &[])
    } else if text.contains("RCON: password doesn't match") {
        note("rconRefused", "Someone tried RCON with the wrong password.", &[])
    } else if text.contains(ADMIN_PROMPT) {
        note("adminPrompt", "The server is waiting for an admin password: type one in the console.", &[])
    } else {
        None
    };
    (level, n)
}

/// A log line without the server's `LOG  : General      f:0 st:1,234> ` prefix.
pub fn message(text: &str) -> &str {
    let t = text.trim_start();
    match t.split_once("> ") {
        Some((head, rest)) if [": ", " st:"].iter().all(|p| head.contains(p)) => rest,
        _ => t,
    }
}

/// The names in a `players` reply (from RCON, or the log lines after it on the console).
pub fn parse_players<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<Vec<String>> {
    let mut it = lines.into_iter().flat_map(str::lines).map(message);
    let count: usize = it.find_map(|l| l.trim().strip_prefix("Players connected (")?.split(')').next()?.parse().ok())?;
    Some(it.filter_map(|l| l.trim().strip_prefix('-').map(|n| n.trim().to_string())).take(count).collect())
}

/// The end of a log file (up to 512 KB), classified, each line's `seq` where it starts in the file.
/// `after` (a line's `seq` from an earlier read): only the lines after it, while the file still has a
/// line starting there (a new, shorter file is read from its end again).
pub fn tail(file: &Path, max_lines: usize, after: u64) -> Vec<Line> {
    let Ok(mut f) = std::fs::File::open(file) else { return vec![] };
    let len = f.metadata().map_or(0, |m| m.len());
    let mut start = len.saturating_sub(512 << 10);
    let mut prev = [0u8];
    let more = after > start && after < len && f.seek(std::io::SeekFrom::Start(after - 1)).is_ok() && f.read_exact(&mut prev).is_ok() && prev[0] == b'\n';
    if more {
        start = after;
    }
    let _ = f.seek(std::io::SeekFrom::Start(start));
    let mut b = vec![];
    let _ = f.read_to_end(&mut b);
    let mut at = start;
    let mut lines = vec![];
    for raw in b.split_inclusive(|c| *c == b'\n') {
        let seq = at;
        at += raw.len() as u64;
        if more && seq == after {
            continue; // read last time
        }
        let text = String::from_utf8_lossy(raw);
        let text = text.trim_end_matches(['\n', '\r']);
        let (level, note) = classify(text);
        lines.push(Line { seq, text: text.to_string(), level, note });
    }
    lines.drain(..lines.len().saturating_sub(max_lines));
    lines
}

/// The server's chat log (`kind` "chat": `Logs/*_chat.txt`) or perk log ("perk": `*_PerkLog.txt`):
/// the newest one in the data folder's `Logs` (shared by the servers using that folder), and its
/// last 1000 lines in short.
pub fn game_log(paths: &Paths, name: &str, kind: &str) -> (Option<PathBuf>, Vec<String>) {
    let file = game_log_file(paths, name, kind);
    let lines = file.as_deref().map(|f| tail(f, 1000, 0).into_iter().map(|l| short_line(&l.text)).filter(|l| !l.is_empty()).collect()).unwrap_or_default();
    (file, lines)
}

/// The newest chat or perk log (see `game_log`).
pub fn game_log_file(paths: &Paths, name: &str, kind: &str) -> Option<PathBuf> {
    let dir = find(paths, name).map_or_else(|| paths.zomboid.clone(), |s| s.cachedir(paths)).join("Logs");
    let suffix = if kind == "perk" { "_PerkLog.txt" } else { "_chat.txt" };
    std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(suffix))).max()
}

/// A chat log line's message: (chat, author, text).
pub fn chat_message(line: &str) -> Option<(String, String, String)> {
    let m = line.split_once("ChatMessage{")?.1;
    let between = |s: &str, a: &str, b: &str| s.split_once(a).and_then(|(_, r)| r.split_once(b)).map(|(v, _)| v.to_string());
    let text = between(m, "text='", "'}").or_else(|| between(m, "text='", "'")).unwrap_or_default();
    Some((between(m, "chat=", ",").unwrap_or_default(), between(m, "author='", "'").unwrap_or_default(), text))
}

/// A chat or perk log line in short: `[11-10-26 06:23:47.576][info] Got message:ChatMessage{chat=General,
/// author='Bob', text='hi'}.` reads `06:23:47 General · Bob: hi`; a perk line `[time] [steam id][Bob][x,y,z][Login][Hours Survived: 3].`
/// reads `06:23:47 Bob · Login · Hours Survived: 3`.
fn short_line(line: &str) -> String {
    let mut groups = vec![];
    let mut rest = line.trim();
    while let Some(r) = rest.strip_prefix('[') {
        let Some((g, after)) = r.split_once(']') else { break };
        groups.push(g);
        rest = after.trim_start();
    }
    let time = groups.first().and_then(|t| t.split_whitespace().nth(1)).map_or("", |t| t.split('.').next().unwrap_or(t));
    if let Some((chat, author, text)) = chat_message(rest) {
        return format!("{time} {chat} · {author}: {text}");
    }
    if groups.len() >= 5 && groups[1].bytes().all(|b| b.is_ascii_digit()) {
        return format!("{time} {} · {}", groups[2], groups[4..].join(" · "));
    }
    format!("{time} {}", rest.trim_end_matches('.')).trim().to_string()
}

// --- running servers ---

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

/// A server PZMM started: its process, console and the log it printed.
struct Proc {
    child: Child,
    stdin: Option<ChildStdin>,
    status: Status,
    reason: Option<Note>,
    lines: VecDeque<Line>,
    seq: u64,
    stopping: bool,
    players: Option<usize>,
}

type Shared = Arc<Mutex<Proc>>;
static PROCS: Mutex<BTreeMap<String, Shared>> = Mutex::new(BTreeMap::new());
/// The servers `start` is getting ready to run (until java runs or the start fails).
static STARTING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Server `name` held in [`STARTING`] until dropped: see [`reserve`].
pub struct Starting(String);

impl Drop for Starting {
    fn drop(&mut self) {
        STARTING.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
    }
}

/// Hold server `name` while it starts or its files change (a restore, wipe, chunk reset, migrate or
/// update), so none of those can run at once. Fails when it's held already or PZMM runs it.
pub fn reserve(name: &str) -> Result<Starting> {
    let mut starting = STARTING.lock().unwrap_or_else(|e| e.into_inner());
    if starting.contains(&held_as(name)) || attached(name) {
        bail!("{name} is already running, or busy starting or changing its files");
    }
    starting.insert(held_as(name));
    Ok(Starting(held_as(name)))
}

/// Whether server `name` is held by [`reserve`].
fn reserved(name: &str) -> bool {
    STARTING.lock().unwrap_or_else(|e| e.into_inner()).contains(&held_as(name))
}

/// The key a hold is kept by: the name (each test, on its own thread, holds its own servers).
fn held_as(name: &str) -> String {
    if cfg!(test) { format!("{name}#{:?}", std::thread::current().id()) } else { name.to_string() }
}
const KEEP_LINES: usize = 5000;

fn proc_of(name: &str) -> Option<Shared> {
    PROCS.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned()
}

fn lock(p: &Shared) -> std::sync::MutexGuard<'_, Proc> {
    p.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the app hears while servers run: a new log line, or a change of status.
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub name: String,
    pub status: Status,
    pub line: Option<Line>,
    pub reason: Option<Note>,
}

type Sink = Box<dyn Fn(&Event) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

pub fn set_event_sink(f: impl Fn(&Event) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(f));
}

fn emit(name: &str, p: &Proc, line: Option<Line>) {
    if let Some(f) = SINK.get() {
        f(&Event { name: name.into(), status: p.status, line, reason: p.reason.clone() });
    }
}

/// Whether PZMM started `name` and it's still up (or on its way up or down).
pub fn attached(name: &str) -> bool {
    proc_of(name).is_some_and(|p| matches!(lock(&p).status, Status::Starting | Status::Running | Status::Stopping))
}

/// The servers PZMM started that are still up.
pub fn attached_running() -> Vec<String> {
    let names: Vec<String> = PROCS.lock().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
    names.into_iter().filter(|n| attached(n)).collect()
}

/// Start server `name` (one PZMM manages). `admin` overrides the saved admin password (the
/// integration test passes its own; the app and CLI use the credential store).
pub fn start(paths: &Paths, name: &str, admin: Option<&str>) -> Result<()> {
    let s = find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    if let Some(e) = &s.external {
        bail!("{name} is run by {}: start it there ({} in {})", e.kind, e.run.first().map_or("", |r| r.as_str()), e.folder.display());
    }
    // the name is taken before the slow part (a backup, the Workshop), so a second start can't run java too
    let _starting = reserve(name)?;
    if saved_java(&s).is_some() {
        bail!("{name} is already running");
    }
    // one started outside PZMM (or by another PZMM): unknown when the processes can't be read
    if command_lines().is_ok_and(|c| c.iter().any(|c| runs_server(c, name))) {
        bail!(crate::keyed!("runningOutside", "{name} is running on this machine (started outside PZMM): stop it first.", name = name));
    }
    if s.auto.backup_on_start && std::fs::exists(s.ini(paths)).unwrap_or(false) {
        super::server_backup::create(paths, &s, "start")?;
    }
    if !cfg!(test) && std::fs::exists(s.ini(paths)).unwrap_or(false) {
        refresh_tag(paths, &s);
    }
    let json = std::fs::read_to_string(s.install_dir.join("ProjectZomboid64.json"))
        .with_context(|| format!("{} has no dedicated server: reinstall it from the setup", s.install_dir.display()))?;
    let cachedir = s.cachedir(paths);
    // only for the first start, which makes the account: on a command line any local user can read it
    let pw = admin.map(String::from).or_else(|| admin_password(name)).filter(|_| !cachedir.join("db").join(format!("{name}.db")).exists());
    std::fs::create_dir_all(&cachedir)?;
    let l = launch(&s.install_dir, &json, &cachedir, name, pw.as_deref().map(|p| (s.admin_user.as_str(), p)), s.memory_mb)?;
    let mut cmd = Command::new(&l.program);
    cmd.args(&l.args).current_dir(&l.cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if cfg!(not(windows)) {
        // ponytail: what start-server.sh sets; Linux hosting is untested
        let d = |p: &str| l.cwd.join(p).display().to_string();
        cmd.env("LD_LIBRARY_PATH", [d("linux64"), d("natives"), d("."), d("jre64/lib/amd64")].join(":"));
    }
    hide_window(&mut cmd);
    crate::say!("serverStarting", "starting {name}: {command}", name = name, command = l.shown());
    let mut child = cmd.spawn().with_context(|| format!("running {}", l.program.display()))?;
    let pid = child.id();
    let (out, err, stdin) = (child.stdout.take(), child.stderr.take(), child.stdin.take());
    let p: Shared = Arc::new(Mutex::new(Proc { child, stdin, status: Status::Starting, reason: None, lines: VecDeque::new(), seq: 0, stopping: false, players: None }));
    PROCS.lock().unwrap_or_else(|e| e.into_inner()).insert(name.into(), p.clone());
    emit(name, &lock(&p), None);
    update(paths, |all| {
        if let Some(s) = all.iter_mut().find(|s| s.name == name) {
            (s.pid, s.started) = (Some(pid), now());
        }
    })?;
    for stream in [out.map(|o| Box::new(o) as Box<dyn Read + Send>), err.map(|e| Box::new(e) as Box<dyn Read + Send>)].into_iter().flatten() {
        let (p, name) = (p.clone(), name.to_string());
        std::thread::spawn(move || {
            for text in BufReader::new(stream).lines().map_while(Result::ok) {
                took_line(&name, &p, text);
            }
        });
    }
    let (p, name, data) = (p.clone(), name.to_string(), Paths { zomboid: paths.zomboid.clone(), data: paths.data.clone() });
    std::thread::spawn(move || {
        let code = loop {
            std::thread::sleep(Duration::from_millis(300));
            if let Ok(Some(st)) = lock(&p).child.try_wait() {
                break st.code();
            }
        };
        let mut g = lock(&p);
        g.stdin = None;
        if g.status != Status::Crashed {
            // a clean exit is a `quit` from anywhere (another PZMM, RCON, an admin in game)
            g.status = if g.stopping || code == Some(0) { Status::Stopped } else { Status::Crashed };
            if g.status == Status::Crashed {
                let code = code.map_or("?".into(), |c| c.to_string());
                g.reason = note("exited", "The server stopped by itself (exit code {code}).", &[("code", code)]);
            }
        }
        emit(&name, &g, None);
        drop(g);
        let _ = update(&data, |all| {
            if let Some(s) = all.iter_mut().find(|s| s.name == name && s.pid == Some(pid)) {
                s.pid = None;
            }
        });
    });
    Ok(())
}

fn took_line(name: &str, p: &Shared, text: String) {
    let (level, note) = classify(&text);
    let mut g = lock(p);
    g.seq += 1;
    let line = Line { seq: g.seq, text, level, note };
    if line.text.contains(STARTED) && g.status == Status::Starting {
        g.status = Status::Running;
    }
    if terminated(&line.text) {
        // it never exits on its own after this
        g.status = Status::Crashed;
        g.reason = line.note.clone();
        let _ = g.child.kill();
    }
    if g.lines.len() >= KEEP_LINES {
        g.lines.pop_front();
    }
    g.lines.push_back(line.clone());
    emit(name, &g, Some(line));
}

/// The log: what PZMM's own server printed after `after` (by `seq`), else the end of its
/// `server-console.txt` (servers left running or not run by PZMM: its lines after `after` when that's
/// one of its lines). Says which it is.
pub fn log(paths: &Paths, name: &str, after: u64) -> (bool, Vec<Line>) {
    if let Some(p) = proc_of(name) {
        return (true, lock(&p).lines.iter().filter(|l| l.seq > after).cloned().collect());
    }
    if remote(paths, name).is_some() {
        return (false, vec![]); // its log is on its own machine
    }
    let dir = find(paths, name).map_or_else(|| paths.zomboid.clone(), |s| s.cachedir(paths));
    (false, tail(&dir.join("server-console.txt"), 2000, after))
}

fn rcon(paths: &Paths, name: &str) -> Result<super::rcon::Rcon> {
    if let Some(r) = remote(paths, name) {
        let pw = crate::settings::secret(&rcon_secret(name)).with_context(|| format!("no RCON password is saved for {name}: forget it and add it again"))?;
        return super::rcon::Rcon::connect(&r.host, r.port, &pw);
    }
    let ini = server::path(paths, name)?;
    let text = std::fs::read_to_string(&ini).with_context(|| format!("reading {}", ini.display()))?;
    let pw = server::value(&text, "RCONPassword");
    if pw.is_empty() {
        bail!("RCON is off for {name} (RCONPassword is empty in its ini), so PZMM can only reach it when it started it");
    }
    super::rcon::Rcon::connect("127.0.0.1", server::value(&text, "RCONPort").parse().unwrap_or(27015), pw)
}

/// Send a console command: on the console when PZMM started the server (the reply shows in the
/// log: None), else over RCON (the reply).
pub fn send(paths: &Paths, name: &str, cmd: &str) -> Result<Option<String>> {
    super::rcon::check_command(cmd)?;
    // the console echoes a command into its log: a password goes over RCON only
    if has_password(cmd) {
        let mut r = rcon(paths, name).context("a command with a password goes over RCON only (the console writes it into the log), and RCON isn't reachable")?;
        return r.exec(cmd).map(Some);
    }
    if let Some(p) = proc_of(name) {
        let mut g = lock(&p);
        // asked to: it stops, it doesn't crash
        g.stopping |= cmd.trim().eq_ignore_ascii_case("quit");
        let up = matches!(g.status, Status::Starting | Status::Running);
        if let Some(stdin) = g.stdin.as_mut().filter(|_| up) {
            stdin.write_all(format!("{cmd}\n").as_bytes()).and_then(|_| stdin.flush()).context("writing to the server's console")?;
            return Ok(None);
        }
    }
    rcon(paths, name)?.exec(cmd).map(Some)
}

/// `adduser "name" "password"`: a command with a password in it.
fn has_password(cmd: &str) -> bool {
    cmd.trim_start().get(..8).is_some_and(|v| v.eq_ignore_ascii_case("adduser "))
}

/// Who's online.
pub fn players(paths: &Paths, name: &str) -> Result<Vec<String>> {
    let since = proc_of(name).map(|p| lock(&p).seq);
    let names = match send(paths, name, "players")? {
        Some(reply) => parse_players([reply.as_str()]),
        None => {
            // the reply comes as log lines
            let p = proc_of(name).context("the server stopped")?;
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                let got = {
                    let g = lock(&p);
                    let lines: Vec<&str> = g.lines.iter().filter(|l| l.seq > since.unwrap_or(0)).map(|l| l.text.as_str()).collect();
                    parse_players(lines.iter().copied()).filter(|n| lines.iter().filter(|l| message(l).trim_start().starts_with('-')).count() >= n.len())
                };
                if got.is_some() || Instant::now() > until {
                    break got;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
    };
    let names = names.context("the server didn't answer `players`")?;
    if let Some(p) = proc_of(name) {
        lock(&p).players = Some(names.len());
    }
    Ok(names)
}

/// Stop a server: `save`, then `quit`, then wait up to `timeout` before ending java.
pub fn stop(paths: &Paths, name: &str, timeout: Duration) -> Result<()> {
    if let Some(p) = proc_of(name).filter(|_| attached(name)) {
        {
            let mut g = lock(&p);
            g.stopping = true;
            if g.status != Status::Crashed {
                g.status = Status::Stopping;
            }
            emit(name, &g, None);
        }
        // on its console; a server PZMM no longer has the console of gets them over RCON
        let on_console = lock(&p).stdin.as_mut().is_some_and(|i| i.write_all(b"save
quit
").and_then(|_| i.flush()).is_ok());
        // saved but `quit` failed (or the connection closed as it went): a shorter wait
        let wait = match if on_console { Ok(Ok(String::new())) } else { rcon(paths, name).and_then(|mut r| r.exec("save").map(|_| r.exec("quit"))) } {
            Ok(Ok(_)) => timeout,
            Ok(Err(_)) => timeout.min(Duration::from_secs(15)),
            Err(_) => Duration::ZERO,
        };
        let until = Instant::now() + wait;
        while attached(name) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(200));
        }
        if attached(name) {
            crate::say!("serverKilled", "{name} didn't stop in time: PZMM ended Java", name = name);
            let _ = lock(&p).child.kill();
            // the exit watcher notes it within its poll: a start right after must find it stopped
            let until = Instant::now() + Duration::from_secs(10);
            while attached(name) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        return Ok(());
    }
    let s = find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    if let Some(e) = &s.external {
        bail!("{name} is run by {}: stop it there ({} in {})", e.kind, e.run.get(1).map_or("", |r| r.as_str()), e.folder.display());
    }
    let Some(pid) = saved_java(&s) else { return Ok(()) };
    let mut r = rcon(paths, name).context("PZMM didn't start this server, so it stops it over RCON")?;
    r.exec("save")?;
    let _ = r.exec("quit"); // the server may close the connection as it goes
    let until = Instant::now() + timeout;
    while java_memory(pid).is_some() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(500));
    }
    if java_memory(pid).is_some() {
        crate::say!("serverKilled", "{name} didn't stop in time: PZMM ended Java", name = name);
        kill_pid(pid);
        let until = Instant::now() + Duration::from_secs(10);
        while java_memory(pid).is_some() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(200));
        }
        // its pid stays saved: PZMM still sees it running
        if java_memory(pid).is_some() {
            bail!("{name} didn't stop, and PZMM couldn't end its Java (process {pid})");
        }
    }
    update(paths, |all| {
        if let Some(s) = all.iter_mut().find(|s| s.name == name) {
            s.pid = None;
        }
    })
}

/// A server's status and process: PZMM's own, else found again by its pid.
pub fn state(s: &Server) -> (Status, Option<u32>) {
    match proc_of(&s.name) {
        Some(p) => {
            let g = lock(&p);
            (g.status, Some(g.child.id()))
        }
        None => match saved_java(s) {
            Some(pid) => (Status::Running, Some(pid)),
            None => (Status::Stopped, None),
        },
    }
}

/// Refuse while server `s` runs: PZMM's own or one found by its pid, a java started outside PZMM
/// with `-servername <name>` (a server, or the game hosting a co-op world), or the game when `s`
/// is a config in the Zomboid folder PZMM doesn't manage (a co-op world it may be hosting).
pub fn check_stopped(paths: &Paths, s: &Server) -> Result<()> {
    let running = match s.external {
        None => reserved(&s.name) || !matches!(state(s).0, Status::Stopped | Status::Crashed),
        // PZMM doesn't see a Docker server's process: it runs while it answers on its game port
        Some(_) => super::a2s::Client::new(([127, 0, 0, 1], s.game_port).into(), Duration::from_millis(300)).and_then(|c| c.info()).is_ok(),
    };
    if running {
        bail!(crate::keyed!("stopFirst", "Stop {name} first.", name = s.name));
    }
    if command_lines().with_context(|| crate::keyed!("runningUnknown", "Couldn't check whether {name} is running: stop it first.", name = s.name))?.iter().any(|c| runs_server(c, &s.name)) {
        bail!(crate::keyed!("runningOutside", "{name} is running on this machine (started outside PZMM): stop it first.", name = s.name));
    }
    if !cfg!(test) && find(paths, &s.name).is_none() && s.cachedir(paths) == paths.zomboid && game_running() {
        bail!(crate::keyed!("closeGameFirst", "The game is running and may be hosting {name}: close it first.", name = s.name));
    }
    Ok(())
}

/// Whether a command line starts server `name` (`-servername <name>`, quoted or not).
fn runs_server(cmdline: &str, name: &str) -> bool {
    [format!("-servername {name}"), format!("-servername \"{name}\"")]
        .iter()
        .any(|pat| cmdline.match_indices(pat.as_str()).any(|(i, m)| cmdline[i + m.len()..].chars().next().is_none_or(char::is_whitespace)))
}

/// The command lines of the processes that may run a server: java, and the game (none in unit tests).
/// When they can't be read: none if tasklist finds no java or game running, else an error.
fn command_lines() -> Result<Vec<String>> {
    if cfg!(test) {
        return Ok(vec![]);
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", "Get-CimInstance Win32_Process | Where-Object { $_.Name -in 'java.exe','javaw.exe','ProjectZomboid64.exe' } | ForEach-Object { $_.CommandLine }"]);
        hide_window(&mut cmd);
        let error = match cmd.output() {
            Ok(o) if o.status.success() => return Ok(String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()),
            Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
            Err(e) => e.to_string(),
        };
        crate::log!("error: reading the processes' command lines: {error}");
        let mut cmd = Command::new("tasklist");
        cmd.args(["/NH", "/FO", "CSV"]);
        hide_window(&mut cmd);
        let names = cmd.output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase()).context("tasklist failed too")?;
        if names.lines().any(|l| ["\"java.exe\"", "\"javaw.exe\"", "\"projectzomboid64.exe\""].iter().any(|n| l.starts_with(n))) {
            bail!("java or the game is running, and its command line can't be read ({error})");
        }
        Ok(vec![])
    }
    #[cfg(not(windows))]
    {
        Ok(std::fs::read_dir("/proc").into_iter().flatten().flatten().filter_map(|e| std::fs::read(e.path().join("cmdline")).ok()).map(|c| String::from_utf8_lossy(&c).replace('\0', " ")).collect())
    }
}

/// When PZMM's own `name` crashed (it exited by itself, or printed `Server Terminated.` and was
/// ended): its process id and its last 40 log lines.
pub fn crashed(name: &str) -> Option<(u32, Vec<String>)> {
    let p = proc_of(name)?;
    let g = lock(&p);
    if g.status != Status::Crashed || g.stopping {
        return None;
    }
    let skip = g.lines.len().saturating_sub(40);
    Some((g.child.id(), g.lines.iter().skip(skip).map(|l| l.text.clone()).collect()))
}

/// A player joining (true) or leaving, from the server's `<steam id> "<name>" fully connected` and
/// `... "<name>" disconnected player` lines (strings in the 42.21 server; not yet seen with a real player).
pub fn player_event(text: &str) -> Option<(bool, String)> {
    let (joined, before) = match (text.split_once("\" fully connected"), text.split_once("\" disconnected player")) {
        (Some((b, _)), _) => (true, b),
        (_, Some((b, _))) => (false, b),
        _ => return None,
    };
    let name = before.rsplit_once('"')?.1;
    (!name.is_empty()).then(|| (joined, name.to_string()))
}

/// One server as the server page shows it.
#[derive(Debug, Default, Serialize)]
pub struct Card {
    pub name: String,
    /// PZMM manages it (else it's a config in the Zomboid folder, read-only until adopted).
    pub managed: bool,
    pub build: String,
    pub install_dir: PathBuf,
    pub cachedir: PathBuf,
    pub ini: PathBuf,
    pub game_port: u16,
    pub direct_port: u16,
    pub memory_mb: u32,
    pub admin_user: String,
    pub status: Status,
    /// PZMM started it in this session: commands go to its console.
    pub attached: bool,
    pub started: u64,
    /// The java process's memory in use, in bytes.
    pub memory_used: u64,
    /// From the last `players`.
    pub players: Option<usize>,
    pub reason: Option<Note>,
    pub auto: super::autopilot::Auto,
    pub external: Option<super::server_detect::External>,
    /// Workshop items updated since it started (titles), and when the restart countdown ends (0: none).
    pub updates: Vec<String>,
    pub restart_at: u64,
    /// Its starts put the PZMM list in its description (see `tag`).
    pub tag: bool,
    pub sides: BTreeMap<String, String>,
    /// Run elsewhere, reached over RCON at this `host:port` (see [`Remote`]).
    pub remote: Option<String>,
    /// The next planned restart while it runs (Unix seconds, 0: none).
    pub next_restart: u64,
    /// CPU (percent) and memory (bytes) over the last minutes, oldest first.
    pub samples: Vec<(f32, u64)>,
    /// The in-game date and time at its last save: year, month, day, hour, minute.
    pub game_time: Option<[u32; 5]>,
    /// Run elsewhere: the commands that update and check its files there, (what, command).
    pub upkeep: Vec<(String, String)>,
    /// The address players join at, when set (a tunnel's).
    pub address: String,
    /// Reached over RCON: its files over FTP, when set.
    pub ftp: Option<super::ftp::Ftp>,
    /// Another server uses its data folder too, so its Discord chat relay stays off.
    pub shares_logs: bool,
}

pub fn cards(paths: &Paths) -> Vec<Card> {
    let managed = load(paths);
    let shared = super::discord_bot::shared_logs(paths, &managed);
    let (now, offset) = (now(), if managed.iter().any(|s| !s.auto.restarts.is_empty()) { super::autopilot::utc_offset() } else { 0 });
    let save = |dir: PathBuf, name: &str| super::saves::game_time(&dir.join("Saves/Multiplayer").join(name));
    let mut out: Vec<Card> = managed
        .iter()
        .map(|s| {
            let p = proc_of(&s.name);
            let (status, players, reason) = p.as_ref().map_or((Status::Stopped, None, None), |p| {
                let g = lock(p);
                (g.status, g.players, g.reason.clone())
            });
            let used = saved_java(s).and_then(java_memory);
            let status = if p.is_none() && used.is_some() { Status::Running } else { status };
            Card {
                name: s.name.clone(),
                managed: true,
                build: s.build.clone(),
                install_dir: s.install_dir.clone(),
                cachedir: s.cachedir(paths),
                ini: s.ini(paths),
                game_port: s.game_port,
                direct_port: s.direct_port,
                memory_mb: s.memory_mb,
                admin_user: s.admin_user.clone(),
                attached: attached(&s.name),
                started: if used.is_some() { s.started } else { 0 },
                memory_used: used.unwrap_or(0),
                status,
                players,
                reason,
                auto: s.auto.clone(),
                external: s.external.clone(),
                updates: super::autopilot::pending(&s.name).0,
                restart_at: super::autopilot::pending(&s.name).1,
                tag: !s.no_tag,
                sides: s.sides.clone(),
                remote: None,
                next_restart: if status == Status::Running { super::autopilot::next_planned(&s.auto, s.started, now, offset).unwrap_or(0) } else { 0 },
                samples: super::autopilot::samples(&s.name),
                game_time: save(s.cachedir(paths), &s.name),
                upkeep: s.external.as_ref().map(super::server_detect::upkeep).unwrap_or_default(),
                address: s.address.clone(),
                ftp: None,
                shares_logs: shared.contains(&s.name),
            }
        })
        .collect();
    for name in server::list(paths).into_iter().filter(|n| !managed.iter().any(|s| &s.name == n)) {
        let ini = server::dir(paths).join(format!("{name}.ini"));
        let text = std::fs::read_to_string(&ini).unwrap_or_default();
        let game_time = save(paths.zomboid.clone(), &name);
        out.push(Card {
            game_time,
            game_port: server::value(&text, "DefaultPort").parse().unwrap_or(16261),
            direct_port: server::value(&text, "UDPPort").parse().unwrap_or(16262),
            name,
            managed: false,
            build: String::new(),
            install_dir: PathBuf::new(),
            cachedir: paths.zomboid.clone(),
            ini,
            memory_mb: 0,
            admin_user: String::new(),
            status: Status::Stopped,
            attached: false,
            started: 0,
            memory_used: 0,
            players: None,
            reason: None,
            auto: Default::default(),
            external: None,
            updates: vec![],
            restart_at: 0,
            tag: false,
            sides: BTreeMap::new(),
            remote: None,
            next_restart: 0,
            samples: vec![],
            upkeep: vec![],
            address: String::new(),
            ftp: None,
            shares_logs: false,
        });
    }
    // shown as running, reachable or not: a command's error says when it isn't
    out.extend(remotes(paths).into_iter().map(|r| Card {
        remote: Some(format!("{}:{}", r.host, r.port)),
        // the settings editors open PZMM's copy of its files
        cachedir: super::ftp::mirror(paths, &r.name),
        ini: super::ftp::mirror(paths, &r.name).join("Server").join(format!("{}.ini", r.name)),
        ftp: r.ftp,
        name: r.name,
        managed: true,
        build: r.build,
        status: Status::Running,
        ..Default::default()
    }));
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn a_held_server_counts_as_running() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = Server { name: "pzmmheld".into(), ..Default::default() };
        let held = reserve(&s.name).unwrap();
        assert!(reserve(&s.name).is_err(), "one hold at a time: a start can't run beside a restore");
        assert!(check_stopped(&paths, &s).is_err(), "held: a start or file change is under way");
        drop(held);
        check_stopped(&paths, &s).unwrap();
        drop(reserve(&s.name).unwrap());
    }

    #[test]
    fn required_items_follow_their_chain() {
        let pages: BTreeMap<&str, &[&str]> = [("1", &["2", "3"][..]), ("2", &["4"]), ("4", &["1", "5"]), ("9", &["3"])].into();
        let requires = |id: &str| pages.get(id).map(|r| r.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let got = missing_required(&ids(&["3"]), &ids(&["1", "9"]), requires);
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(got, [pair("1", "2"), pair("2", "4"), pair("4", "5")], "3 is listed; 1 is where it started");
    }

    #[test]
    fn remote_servers_are_listed_and_forgotten() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l); // nothing listens: the login fails and nothing is saved
        let r = Remote { name: "Friends".into(), host: "127.0.0.1".into(), port, build: "b42".into(), ..Default::default() };
        assert!(add_remote(&paths, &r, "pw").is_err());
        assert!(remotes(&paths).is_empty());
        // added as if the login had worked
        set_remotes(&paths, |all| all.push(r.clone())).unwrap();
        let card = cards(&paths).into_iter().find(|c| c.name == "Friends").unwrap();
        assert_eq!((card.remote.as_deref(), card.status), (Some(format!("127.0.0.1:{port}").as_str()), Status::Running));
        assert_eq!(log(&paths, "Friends", 0), (false, vec![]), "its log is elsewhere");
        let taken = add_remote(&paths, &r, "pw").unwrap_err().to_string();
        assert!(taken.contains("already"), "{taken}");
        assert!(adopt(&paths, "Friends", Path::new("D:/none")).unwrap_err().to_string().contains("already"), "one name for one server");
        assert!(send(&paths, "Friends", "players").unwrap_err().to_string().contains("no RCON password"), "tests never read the credential store");
        forget_remote(&paths, "Friends").unwrap();
        assert!(remotes(&paths).is_empty());
    }

    /// A server PZMM manages in a scratch folder, run from `install` (for the tests on the real server).
    pub(crate) fn scratch_server(paths: &Paths, name: &str, install: PathBuf, cachedir: PathBuf) -> Server {
        let s = Server { name: name.into(), build: "b42".into(), install_dir: install, cachedir: Some(cachedir), game_port: 16261, direct_port: 16262, memory_mb: 3072, admin_user: "admin".into(), ..Default::default() };
        let n = NewServer { name: name.into(), game_port: 16261, direct_port: 16262, max_players: 4, ..Default::default() };
        server::set_values(&s.ini(paths), &new_ini(&n, 27015, &generate_password())).unwrap();
        update(paths, |all| all.push(s.clone())).unwrap();
        s
    }

    #[test]
    fn the_address_players_join_at() {
        let (_t, paths) = crate::store::tests::temp_paths();
        update(&paths, |all| all.push(Server { name: "Tunnel".into(), ..Default::default() })).unwrap();
        set_address(&paths, "Tunnel", " example.gl.joinmc.link:12345 ").unwrap();
        assert_eq!(find(&paths, "Tunnel").unwrap().address, "example.gl.joinmc.link:12345");
        for bad in ["a b", "http://x", "x\"y"] {
            assert!(set_address(&paths, "Tunnel", bad).is_err(), "{bad}");
        }
        assert!(set_address(&paths, "Nope", "1.2.3.4").is_err());
        set_address(&paths, "Tunnel", "").unwrap();
        assert_eq!(find(&paths, "Tunnel").unwrap().address, "");
    }

    #[test]
    fn servers_json_round_trip() {
        let (_t, paths) = crate::store::tests::temp_paths();
        assert!(load(&paths).is_empty());
        let s = Server { name: "Friends".into(), build: LEGACY.into(), install_dir: "D:/srv".into(), game_port: 16261, pid: Some(7), ..Default::default() };
        update(&paths, |all| all.push(s.clone())).unwrap();
        assert_eq!(find(&paths, "Friends"), Some(s.clone()));
        assert_eq!(s.build_number(), 41);
        assert_eq!(s.ini(&paths), paths.zomboid.join("Server").join("Friends.ini"), "no data folder: the Zomboid folder");
        let own = Server { cachedir: Some("E:/pz".into()), ..s };
        assert_eq!(own.ini(&paths), PathBuf::from("E:/pz/Server/Friends.ini"));
        // fields added later default; unknown ones are ignored
        std::fs::write(list_file(&paths), r#"[{"name": "Old", "future": 1}]"#).unwrap();
        assert_eq!(load(&paths), [Server { name: "Old".into(), ..Default::default() }]);
        // a damaged list is kept aside before anything replaces it
        std::fs::write(list_file(&paths), "not json").unwrap();
        assert!(load(&paths).is_empty());
        assert_eq!(std::fs::read_to_string(list_file(&paths).with_extension("json.bad")).unwrap(), "not json");
        // one that can't be read is never written over
        std::fs::remove_file(list_file(&paths)).unwrap();
        std::fs::create_dir(list_file(&paths)).unwrap();
        assert!(update(&paths, |all| all.push(Server::default())).is_err());
        assert!(set_auto(&paths, "Old", |_| {}).is_err() && list_file(&paths).is_dir());
    }

    #[test]
    fn each_server_gets_its_own_rcon_port() {
        let (_t, paths) = crate::store::tests::temp_paths();
        assert_eq!(free_rcon_port(&paths), 27015);
        update(&paths, |all| all.extend([Server { name: "A".into(), ..Default::default() }, Server { name: "B".into(), ..Default::default() }])).unwrap();
        // B's ini says otherwise: the ini wins
        server::set_values(&server::path(&paths, "B").unwrap(), &[("RCONPort", "27016".into()), ("RCONPassword", "pw".into())]).unwrap();
        assert_eq!(free_rcon_port(&paths), 27017);
        let c = clone_server(&paths, "B", "C").unwrap();
        let text = std::fs::read_to_string(c.ini(&paths)).unwrap();
        assert_eq!(server::value(&text, "RCONPort"), "27017");
        assert!(!["", "pw"].contains(&server::value(&text, "RCONPassword")), "a password of its own");
    }

    #[test]
    fn finds_servers_started_elsewhere_and_the_build_installed() {
        let java = r#""C:\pz\jre64\bin\java.exe" -Xmx4g -cp x zombie.network.GameServer "-cachedir=D:\d" -servername "My Server" -adminusername a"#;
        assert!(runs_server(java, "My Server"));
        assert!(runs_server("java -servername servertest -coop", "servertest") && runs_server("java -servername Friends", "Friends"));
        assert!(!runs_server(java, "My") && !runs_server("java -servername Friends2", "Friends"));
        let t = tempfile::tempdir().unwrap();
        assert_eq!(install_build(t.path()), None);
        std::fs::write(t.path().join("ProjectZomboid64.json"), JSON).unwrap();
        assert_eq!(install_build(t.path()), Some("b42"));
        std::fs::write(t.path().join("ProjectZomboid64.json"), r#"{"classpath": ["java/.", "java/trove-3.0.3.jar"]}"#).unwrap();
        assert_eq!(install_build(t.path()), Some(LEGACY));
    }

    #[test]
    fn a_managed_server_with_its_own_folder_keeps_its_ini_there() {
        let (t, paths) = crate::store::tests::temp_paths();
        let data = t.path().join("Own");
        update(&paths, |all| all.push(Server { name: "Own".into(), cachedir: Some(data.clone()), ..Default::default() })).unwrap();
        assert_eq!(server::path(&paths, "Own").unwrap(), data.join("Server/Own.ini"));
        assert_eq!(server::path(&paths, "Other").unwrap(), paths.zomboid.join("Server/Other.ini"));
    }

    #[test]
    fn the_list_goes_in_the_description_and_comes_out() {
        let (_t, paths) = crate::store::tests::temp_paths();
        update(&paths, |all| all.push(Server { name: "Tagged".into(), ..Default::default() })).unwrap();
        let ini = server::path(&paths, "Tagged").unwrap();
        server::set_values(&ini, &[("PublicDescription", r"Friends only \n Discord: x".into()), ("Mods", r"\A".into())]).unwrap();
        let t = super::super::tag::Tag { items: vec![("111".into(), 5)], mods: vec!["A".into()], ..Default::default() };
        assert_eq!(set_tag(&ini, Some(&t)).unwrap(), "full");
        let desc = |ini: &Path| server::value(&std::fs::read_to_string(ini).unwrap(), "PublicDescription").to_string();
        assert_eq!(super::super::tag::decode(&desc(&ini)).unwrap().unwrap(), t);
        assert!(desc(&ini).starts_with(r"Friends only \n Discord: x\n[PZMM:1:"));
        assert!(std::fs::read_to_string(&ini).unwrap().contains(r"Mods=\A"), "other lines stay");
        set_tag_on(&paths, "Tagged", false).unwrap();
        assert_eq!(desc(&ini), r"Friends only \n Discord: x");
        assert!(find(&paths, "Tagged").unwrap().no_tag);
        set_tag_on(&paths, "Tagged", true).unwrap();
        assert!(!find(&paths, "Tagged").unwrap().no_tag);
    }

    #[test]
    fn mod_sides_are_kept_and_listed() {
        let (_t, paths) = crate::store::tests::temp_paths();
        update(&paths, |all| all.push(Server { name: "Sided".into(), ..Default::default() })).unwrap();
        let sides: BTreeMap<String, String> = [("B".into(), "server".into()), ("C".into(), "optional".into()), ("Gone".into(), "server".into())].into();
        set_sides(&paths, "Sided", sides.clone()).unwrap();
        assert_eq!(find(&paths, "Sided").unwrap().sides, sides);
        assert!(set_sides(&paths, "Sided", [("B".into(), "client".into())].into()).is_err());
        assert!(set_sides(&paths, "Nobody", BTreeMap::new()).is_err());
        let lines = ServerMods { mods: vec!["A".into(), "B".into(), "C".into()], ..Default::default() };
        let t = tag_for(&lines, &sides).unwrap();
        assert_eq!((t.server_only, t.optional), (vec!["B".to_string()], vec!["C".to_string()]), "only mods on the list");
    }

    // made up in the shape of the server's ProjectZomboid64.json
    const JSON: &str = r#"{"mainClass": "zombie/network/GameServer", "classpath": ["java/.", "java/projectzomboid.jar"],
        "vmArgs": ["-Djava.awt.headless=true", "-Xmx3072m", "-Djava.library.path=natives/"],
        "windows": {"7": {"vmArgs": ["-XX:+UseG1GC"]}, "10": {"vmArgs": ["-XX:+UseZGC"]}}}"#;

    #[test]
    fn launches_java_directly_with_one_cachedir_argument() {
        let cache = std::env::temp_dir().join("My Server Data");
        let l = launch(Path::new("D:/pz server"), JSON, &cache, "Friends", Some(("admin", "s3cret pw")), 6144).unwrap();
        assert!(l.program.ends_with(if cfg!(windows) { "jre64/bin/java.exe" } else { "jre64/bin/java" }), "{}", l.program.display());
        assert!(!l.program.to_string_lossy().ends_with(".bat") && !l.args.iter().any(|a| a.contains(".bat")));
        assert_eq!(l.cwd, PathBuf::from("D:/pz server"));
        let cd: Vec<&String> = l.args.iter().filter(|a| a.contains("cachedir")).collect();
        assert_eq!(cd, [&format!("-cachedir={}", cache.display())], "one argument, spaces and all");
        assert_eq!(l.args.iter().filter(|a| a.starts_with("-Xm")).collect::<Vec<_>>(), ["-Xmx6144m"], "PZMM's memory, not the file's");
        let at = |s: &str| l.args.iter().position(|a| a == s).unwrap();
        assert_eq!(l.args[at("-cp") + 1], if cfg!(windows) { "java/.;java/projectzomboid.jar" } else { "java/.:java/projectzomboid.jar" });
        assert_eq!(l.args[at("-cp") + 2], "zombie.network.GameServer");
        assert!(at("-Djava.library.path=natives/") < at("-cp"), "JVM options before the class");
        assert_eq!(l.args[at("-servername") + 1], "Friends");
        assert_eq!(l.args[at("-adminusername") + 1], "admin");
        assert_eq!(l.args[at("-adminpassword") + 1], "s3cret pw");
        if cfg!(windows) {
            assert!(l.args.contains(&"-XX:+UseZGC".into()) && !l.args.contains(&"-XX:+UseG1GC".into()), "the newest Windows extras");
        }
        let shown = l.shown();
        assert!(!shown.contains("s3cret") && shown.contains("-adminpassword ********"), "{shown}");
        assert!(shown.contains(&format!("\"-cachedir={}\"", cache.display())));

        let later = launch(Path::new("D:/pz"), JSON, &cache, "Friends", None, 2048).unwrap();
        assert!(!later.args.iter().any(|a| a.starts_with("-admin")));
        assert!(launch(Path::new("D:/pz"), JSON, Path::new("relative"), "x", None, 2048).is_err());
        assert!(launch(Path::new("D:/pz"), r#"{"mainClass": "x"}"#, &cache, "x", None, 2048).is_err());
    }

    #[test]
    fn clone_copies_settings_not_the_world() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = Server { name: "Friends".into(), build: "b42".into(), game_port: 16261, auto: super::super::autopilot::Auto { save_minutes: 15, ..Default::default() }, ..Default::default() };
        update(&paths, |all| all.push(s.clone())).unwrap();
        assert!(clone_server(&paths, "Friends", "Copy").is_err(), "no settings yet");
        let dir = paths.zomboid.join("Server");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Friends.ini"), "PVP=false\n").unwrap();
        std::fs::write(dir.join("Friends_spawnregions.lua"), "\t\t{ name = \"Bar\", serverfile = \"Friends_spawnpoints.lua\" },\n").unwrap();
        std::fs::create_dir_all(paths.zomboid.join("Saves/Multiplayer/Friends")).unwrap();
        let c = clone_server(&paths, "Friends", "Copy").unwrap();
        assert_eq!((c.game_port, c.auto.save_minutes, c.pid), (16261, 15, None));
        assert_eq!(std::fs::read_to_string(dir.join("Copy.ini")).unwrap(), "PVP=false\nRCONPort=27016\nRCONPassword=\n", "RCON stays off; Friends has the default port");
        assert!(std::fs::read_to_string(dir.join("Copy_spawnregions.lua")).unwrap().contains("serverfile = \"Copy_spawnpoints.lua\""));
        assert!(!paths.zomboid.join("Saves/Multiplayer/Copy").exists(), "a new world");
        assert!(find(&paths, "Copy").is_some());
        assert!(clone_server(&paths, "Friends", "Copy").is_err(), "taken");
        assert!(clone_server(&paths, "Friends", "../x").is_err());
    }

    #[test]
    fn a_coop_world_moves_into_a_server() {
        let (t, paths) = crate::store::tests::temp_paths();
        let own = t.path().join("srvdata");
        update(&paths, |all| all.push(Server { name: "Friends".into(), cachedir: Some(own.clone()), ..Default::default() })).unwrap();
        let w = |p: PathBuf, text: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        let z = &paths.zomboid;
        w(z.join("Saves/Multiplayer/servertest/map_t.bin"), "coop");
        w(z.join("Saves/Multiplayer/servertest_player/players.db"), "host");
        w(z.join("db/servertest.db"), "accounts");
        w(z.join("Server/servertest.ini"), "Mods=\\Hydro\nWorkshopItems=111\n");
        w(z.join("Server/servertest_SandboxVars.lua"), "SandboxVars = {\n}\n");
        w(own.join("Server/Friends.ini"), "PVP=true\nMods=\nWorkshopItems=\n");
        w(own.join("Saves/Multiplayer/Friends/map_t.bin"), "old world");
        assert_eq!(coop_worlds(&paths, &paths.zomboid), ["servertest"]);
        assert!(migrate(&paths, "Friends", "nope", &paths.zomboid).is_err());
        migrate(&paths, "Friends", "servertest", &paths.zomboid).unwrap();
        assert_eq!(std::fs::read_to_string(own.join("Saves/Multiplayer/Friends/map_t.bin")).unwrap(), "coop");
        assert_eq!(std::fs::read_to_string(own.join("Saves/Multiplayer/Friends_player/players.db")).unwrap(), "host");
        assert_eq!(std::fs::read_to_string(own.join("db/Friends.db")).unwrap(), "accounts");
        let ini = std::fs::read_to_string(own.join("Server/Friends.ini")).unwrap();
        assert!(ini.contains("PVP=true") && ini.contains("WorkshopItems=111"), "{ini}");
        assert!(own.join("Server/Friends_SandboxVars.lua").is_file());
        assert_eq!(super::super::server_backup::list(&paths, "Friends")[0].reason, "migrate", "the old world was backed up");
    }

    #[test]
    fn the_start_script_runs_the_same_java() {
        let l = launch(Path::new("D:/pz server"), JSON, &std::env::temp_dir().join("My Data"), "Friends", None, 4096).unwrap();
        let text = bundle_text(&l);
        for a in &l.args {
            assert!(text.contains(a.as_str()), "{a}");
        }
        assert!(!text.contains("-adminpassword"));
        if cfg!(windows) {
            assert!(text.contains(&format!("\"-cachedir={}\"", std::env::temp_dir().join("My Data").display())), "one quoted argument: {text}");
            assert!(text.starts_with("@echo off\r\n"));
        } else {
            assert!(text.starts_with("#!/bin/sh\n") && text.contains("exec '"));
        }
    }

    #[test]
    fn chat_and_perk_lines_in_short() {
        assert_eq!(short_line("[11-10-26 06:23:47.576][info] Got message:ChatMessage{chat=General, author='Bob', text='hi there'}."), "06:23:47 General · Bob: hi there");
        assert_eq!(short_line("[11-10-26 06:23:47.580][info] General chat has id = 0."), "06:23:47 General chat has id = 0");
        assert_eq!(short_line("[11-10-26 06:24:01.002] [76561190000000000][Bob][10829,9429,0][Login][Hours Survived: 3]."), "06:24:01 Bob · Login · Hours Survived: 3");
        assert_eq!(short_line("[11-10-26 06:24:01.002] [76561190000000000][Bob][10829,9429,0][Level Changed][Nimble][2]."), "06:24:01 Bob · Level Changed · Nimble · 2");
        let (_t, paths) = crate::store::tests::temp_paths();
        let logs = paths.zomboid.join("Logs");
        std::fs::create_dir_all(logs.join("logs_old")).unwrap();
        std::fs::write(logs.join("2026-10-11_06-20_chat.txt"), "[11-10-26 06:20:00.000][info] old\n").unwrap();
        std::fs::write(logs.join("2026-10-11_06-23_chat.txt"), "[11-10-26 06:23:00.000][info] new\n").unwrap();
        let (file, lines) = game_log(&paths, "Friends", "chat");
        assert!(file.unwrap().ends_with("2026-10-11_06-23_chat.txt"));
        assert_eq!(lines, ["06:23:00 new"]);
        assert_eq!(game_log(&paths, "Friends", "perk"), (None, vec![]));
    }

    #[test]
    fn diagnostics_mask_secrets() {
        let (_t, paths) = crate::store::tests::temp_paths();
        update(&paths, |all| all.push(Server { name: "Friends".into(), build: "b42".into(), ..Default::default() })).unwrap();
        let ini = server::path(&paths, "Friends").unwrap();
        std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
        std::fs::write(&ini, "# Password for the server\nPassword=door\nRCONPassword=s3cret\nDiscordToken=abc\nPublicName=Friends\nPassword2=\n").unwrap();
        std::fs::write(paths.zomboid.join("server-console.txt"), "LOG  : General      f:0 st:1> AddUser \"Bob\" \"hunter2\"\n").unwrap();
        let d = diagnostics(&paths, "Friends");
        assert!(!d.contains("door") && !d.contains("s3cret") && !d.contains("abc") && !d.contains("hunter2") && d.contains("adduser ********"), "{d}");
        assert!(d.contains("Password=********") && d.contains("PublicName=Friends") && d.contains("Password2=\n"));
        assert!(d.contains("Server Friends: Stopped, build b42"));
        let d = diagnostics(&paths, r"..\..\outside");
        assert!(d.starts_with("invalid server name") && !d.contains("outside.ini"), "{d}");
    }

    #[test]
    fn a_crashed_server_can_be_updated() {
        let (_t, paths) = crate::store::tests::temp_paths();
        update(&paths, |all| all.push(Server { name: "pzmmcrash".into(), build: "b42".into(), install_dir: "relative".into(), ..Default::default() })).unwrap();
        let child = Command::new(std::env::current_exe().unwrap()).arg("--list").stdout(Stdio::null()).spawn().unwrap();
        let p = Proc { child, stdin: None, status: Status::Crashed, reason: None, lines: VecDeque::new(), seq: 0, stopping: false, players: None };
        PROCS.lock().unwrap().insert("pzmmcrash".into(), Arc::new(Mutex::new(p)));
        let e = update_server(&paths, "pzmmcrash", "b42", &AtomicBool::new(false)).unwrap_err().to_string();
        let p = PROCS.lock().unwrap().remove("pzmmcrash").unwrap();
        let _ = lock(&p).child.wait();
        assert!(e.contains("full path"), "past the status check to SteamCMD: {e}");
    }

    #[test]
    fn passwords_are_random_and_readable() {
        let a = generate_password();
        assert_eq!(a.len(), 20);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() && !"0O1lI".contains(c)), "{a}");
        assert_ne!(a, generate_password());
        assert_eq!(admin_target("Friends"), "server-admin:Friends");
        assert_eq!(admin_password("Friends"), None, "tests never read the credential store");
    }

    #[test]
    fn new_servers_turn_upnp_off_and_rcon_on() {
        let n = NewServer { name: "Friends".into(), game_port: 16261, direct_port: 16262, max_players: 8, join_password: "door".into(), ..Default::default() };
        let set: BTreeMap<_, _> = new_ini(&n, 27016, "rconpw").into_iter().collect();
        assert_eq!(set["UPnP"], "false");
        assert_eq!(set["Public"], "false");
        assert_eq!((set["DefaultPort"].as_str(), set["UDPPort"].as_str(), set["MaxPlayers"].as_str()), ("16261", "16262", "8"));
        assert_eq!((set["RCONPassword"].as_str(), set["Password"].as_str(), set["RCONPort"].as_str()), ("rconpw", "door", "27016"));
        assert_eq!(set["Map"], server::BASE_MAP, "no mod maps: the game's map");
    }

    #[test]
    fn create_checks_before_writing() {
        let (t, paths) = crate::store::tests::temp_paths();
        let install = t.path().join("srv");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::write(install.join("ProjectZomboid64.json"), JSON).unwrap();
        let ok = NewServer {
            name: "Friends".into(),
            install_dir: Some(install.clone()),
            game_port: 16261,
            direct_port: 16262,
            admin_user: "admin".into(),
            admin_password: "pw".into(),
            max_players: 8,
            memory_mb: 4096,
            lines: ServerMods { mods: vec!["A".into()], workshop: vec!["123".into()], maps: vec![] },
            ..Default::default()
        };
        let never = AtomicBool::new(false);
        for bad in [
            NewServer { name: "R&D".into(), ..ok.clone() },
            NewServer { direct_port: 16261, ..ok.clone() },
            NewServer { admin_user: "my admin".into(), ..ok.clone() },
            NewServer { lines: ServerMods { workshop: vec!["12a".into()], ..Default::default() }, ..ok.clone() },
            NewServer { cachedir: Some("relative".into()), ..ok.clone() },
            NewServer { join_password: "door\nRCONPassword=".into(), ..ok.clone() },
            NewServer { lines: ServerMods { mods: vec!["A\nB".into()], ..Default::default() }, ..ok.clone() },
        ] {
            assert!(create(&paths, &bad, &never).is_err(), "{bad:?}");
        }
        assert!(load(&paths).is_empty() && !paths.zomboid.join("Server").exists(), "nothing written");
        let s = create(&paths, &ok, &never).unwrap();
        let text = std::fs::read_to_string(s.ini(&paths)).unwrap();
        for line in ["UPnP=false", "Mods=\\A", "WorkshopItems=123", "Map=Muldraugh, KY", "DefaultPort=16261"] {
            assert!(text.lines().any(|l| l == line), "{line} in\n{text}");
        }
        assert!(server::value(&text, "RCONPassword").len() == 20);
        assert_eq!(find(&paths, "Friends").unwrap().install_dir, install);
        assert!(create(&paths, &ok, &never).is_err(), "names are unique");
        forget(&paths, "Friends").unwrap();
        assert!(load(&paths).is_empty() && s.ini(&paths).exists(), "forgetting keeps the files");
    }

    #[test]
    fn memory_and_install_progress() {
        assert_eq!(default_memory_mb(32768), 8192);
        assert_eq!(default_memory_mb(12000), 5632);
        assert_eq!(default_memory_mb(4096), 2048);
        assert_eq!(install_progress(" Update state (0x61) downloading, progress: 12.04 (860705099 / 7150498296)"), Some(12.04));
        assert_eq!(install_progress("Success! App '380870' fully installed."), None);
        let (u, t) = (std::net::UdpSocket::bind("127.0.0.1:0").unwrap(), std::net::TcpListener::bind("127.0.0.1:0").unwrap());
        assert!(!port_free(u.local_addr().unwrap().port(), true) && !port_free(t.local_addr().unwrap().port(), false));
    }

    #[cfg(windows)]
    #[test]
    fn reads_a_process_without_tasklist() {
        let (exe, memory, created) = process_info(std::process::id()).unwrap();
        assert!(exe.ends_with(".exe") && memory > 0 && created.abs_diff(now()) < 3600, "{exe} {memory} {created}");
        let s = Server { pid: Some(std::process::id()), started: created, ..Default::default() };
        assert_eq!(saved_java(&s), None, "not java");
        assert_eq!(process_info(u32::MAX - 1), None);
    }

    // scrubbed lines from a real 42.21 server's output
    const LOG: &str = r#"LOG  : General      f:0 st:114,771,239> version=42.21.0 4a0e9546ec demo=false
WARN : Mod          f:0 st:115,085,872 at ZomboidFileSystem.loadModAndRequired> required mod "pzmm_fake_mod_01" not found
LOG  : General      f:0 st:115,419,874> Workshop: onItemNotDownloaded itemID=1999999999 result=9
LOG  : General      f:0 st:115,419,875> Exception in thread "main" LOG  : General      f:0 st:115,419,875> java.lang.NullPointerException
	zombie.world.WorldDictionaryException: World loading could not proceed, there are script load errors. (Actual error may be printed earlier in log) at IsoWorld.init(IsoWorld.java:2802).
LOG  : General      f:0 st:115,108,509> Server Terminated.
LOG  : Network      f:0 st:114,804,498> *** SERVER STARTED ****
LOG  : General      f:0 st:115,463,392> Workshop: Installed status but timeUpdated doesn't match!!!
LOG  : Mod          f:0 st:114,978,072> CheckModsNeedUpdate: Mods updated
LOG  : General      f:0 st:115,026,146> RCON: password doesn't match
LOG  : General      f:0 st:115,572,091> Enter new administrator password:
ERROR: General      f:0 st:114,784,292 at FluidContainerScript.load           > Sanitizing container name 'Large Bucket', name may not contain whitespaces."#;

    #[test]
    fn only_the_servers_own_line_is_its_end() {
        assert!(terminated("Server Terminated."));
        assert!(terminated("LOG  : General     , 1690000000000> Server Terminated."), "Build 41's format");
        assert!(!terminated("[15-10-26 12:00:00.000] LOG  : General f:0, t:1, st:2> ChatMessage{chat=General, author='x', text='Server Terminated.'}"));
        assert!(!terminated("servermsg \"Server Terminated. Back soon\""));
    }

    #[test]
    fn log_lines_in_plain_words() {
        let got: Vec<(Level, Option<&str>)> = LOG.lines().map(classify).map(|(l, n)| (l, n.map(|n| n.key))).collect();
        use Level::*;
        assert_eq!(
            got,
            [
                (Info, None),
                (Warn, Some("modMissing")),
                (Info, Some("badItem")),
                (Error, None),
                (Error, Some("scriptErrors")),
                (Error, Some("terminated")),
                (Info, Some("started")),
                (Info, Some("outdated")),
                (Info, Some("upToDate")),
                (Info, Some("rconRefused")),
                (Info, Some("adminPrompt")),
                (Error, None),
            ]
        );
        let (_, n) = classify(LOG.lines().nth(1).unwrap());
        assert_eq!(n.unwrap().text, "Mod ID pzmm_fake_mod_01 isn't in any of the server's Workshop items: check Mods and WorkshopItems.");
        let (_, n) = classify("Workshop: onItemNotDownloaded itemID=42 result=2");
        assert_eq!(n.unwrap().vars, [("id", "42".to_string()), ("result", "2".to_string())].into());
        // a chat line saying it isn't the server's own
        let (l, n) = classify("LOG  : General      f:0 st:1,234> ChatMessage{chat=General, author='Bob', text='Server Terminated.'}");
        assert_eq!((l, n.map(|n| n.key)), (Info, None));
    }

    #[test]
    fn players_from_rcon_and_the_console() {
        assert_eq!(parse_players(["Players connected (0): "]), Some(vec![]));
        assert_eq!(parse_players(["Players connected (2): \n-Alice\n-Bob the Builder\n"]), Some(vec!["Alice".into(), "Bob the Builder".into()]));
        let console = [
            r#"LOG  : General      f:0 st:114,854,563> command entered via server console (System.in): "players""#,
            "LOG  : General      f:0 st:114,854,587> Players connected (1): ",
            "-Alice",
        ];
        assert_eq!(parse_players(console), Some(vec!["Alice".into()]));
        assert_eq!(parse_players(["Unknown command"]), None);
        assert_eq!(player_event(r#"LOG  : Network      f:0 st:1> 76561198000000000 "Bob the Builder" fully connected (10593,9734,0)"#), Some((true, "Bob the Builder".into())));
        assert_eq!(player_event(r#"76561198000000000 "Alice" disconnected player (10593,9734,0)"#), Some((false, "Alice".into())));
        assert_eq!(player_event("Disconnected player in CurrentCell.getObjectList() removed"), None);
        assert_eq!(message("LOG  : General      f:0 st:1> Players connected (0): "), "Players connected (0): ");
        assert_eq!(message("-Alice"), "-Alice");
    }

    #[test]
    fn passwords_are_told_apart() {
        assert!(has_password(r#"adduser "Bob" "secret""#));
        assert!(has_password(" AddUser Bob pw"));
        assert!(!has_password(r#"addusertowhitelist "Bob""#));
        assert!(!has_password("players"));
    }

    #[test]
    fn log_tail_reads_the_end() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("server-console.txt");
        std::fs::write(&f, (0..3000).map(|i| format!("line {i}\n")).collect::<String>() + "LOG  : Network      f:0 st:1> *** SERVER STARTED ****\n").unwrap();
        let lines = tail(&f, 10, 0);
        assert_eq!(lines.len(), 10);
        assert_eq!(lines[0].text, "line 2991");
        assert_eq!(lines[9].note.as_ref().unwrap().key, "started");
        assert!(tail(&t.path().join("none.txt"), 10, 0).is_empty());
        // read again from the last line: only what came after it
        let last = lines[9].seq;
        assert!(tail(&f, 10, last).is_empty());
        std::fs::OpenOptions::new().append(true).open(&f).unwrap().write_all(b"more\r\n").unwrap();
        let more = tail(&f, 10, last);
        assert_eq!(more.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["more"]);
        // a new, shorter file: its end again
        std::fs::write(&f, "new\n").unwrap();
        assert_eq!(tail(&f, 10, last)[0].text, "new");
    }

    /// The real server in `.temp/pzserver`, with every file it writes under `.temp/s1/it`.
    #[test]
    #[ignore = "runs the dedicated server installed in .temp/pzserver (about a minute)"]
    fn real_server_starts_answers_and_quits() {
        let temp = std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.temp")).unwrap();
        let (install, scratch) = (temp.join("pzserver"), temp.join("s1").join("it"));
        assert!(installed(&install), "no server in {}", install.display());
        let _ = std::fs::remove_dir_all(&scratch);
        let paths = Paths { zomboid: scratch.join("Zomboid"), data: scratch.join("data") };
        let s = Server {
            name: "pzmmit".into(),
            build: "b42".into(),
            install_dir: install,
            cachedir: Some(scratch.join("cache")),
            game_port: 16261,
            direct_port: 16262,
            memory_mb: 3072,
            admin_user: "admin".into(),
            ..Default::default()
        };
        let n = NewServer { name: s.name.clone(), game_port: 16261, direct_port: 16262, max_players: 4, ..Default::default() };
        server::set_values(&s.ini(&paths), &new_ini(&n, 27015, &generate_password())).unwrap();
        update(&paths, |all| all.push(s.clone())).unwrap();

        start(&paths, "pzmmit", Some(&generate_password())).unwrap();
        let p = proc_of("pzmmit").unwrap();
        let until = Instant::now() + Duration::from_secs(180);
        while lock(&p).status == Status::Starting && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(500));
        }
        let status = lock(&p).status;
        if status != Status::Running {
            let _ = lock(&p).child.kill();
            let last: Vec<String> = lock(&p).lines.iter().rev().take(30).map(|l| l.text.clone()).collect();
            panic!("{status:?}: {last:#?}");
        }
        let pid = find(&paths, "pzmmit").unwrap().pid.unwrap();
        assert!(java_memory(pid).is_some());

        assert_eq!(players(&paths, "pzmmit").unwrap(), Vec::<String>::new(), "over the console");
        let reply = rcon(&paths, "pzmmit").unwrap().exec("players").unwrap();
        assert_eq!(parse_players([reply.as_str()]), Some(vec![]), "over RCON: {reply:?}");
        let text = std::fs::read_to_string(s.ini(&paths)).unwrap();
        assert!(text.lines().any(|l| l == "UPnP=false"), "kept after the first start");
        assert!(text.lines().count() > 100, "the server filled in its other settings");
        assert!(s.cachedir(&paths).join("Server/pzmmit_SandboxVars.lua").exists());

        // PZMM quits and leaves it running: its console closes, RCON still reaches it and stops it
        let seq = lock(&p).seq;
        lock(&p).stdin = None;
        PROCS.lock().unwrap().remove("pzmmit");
        std::thread::sleep(Duration::from_secs(5));
        assert!(lock(&p).seq - seq < 50, "a closed console doesn't flood the log");
        assert_eq!(cards(&paths)[0].status, Status::Running, "found again by its pid");
        let reply = rcon(&paths, "pzmmit").unwrap().exec("players").unwrap();
        assert!(reply.starts_with("Players connected (0)"), "{reply:?}");
        stop(&paths, "pzmmit", Duration::from_secs(60)).unwrap();
        assert!(java_memory(pid).is_none(), "java has gone");
        assert_eq!(find(&paths, "pzmmit").unwrap().pid, None);
    }

    /// `quit` typed in the console stops the server: it's Stopped, not Crashed (no crash restart, no
    /// "crashed" post). Its own ports, so it can run beside the test above.
    #[test]
    #[ignore = "runs the dedicated server installed in .temp/pzserver (about a minute)"]
    fn real_server_quit_from_the_console_is_a_stop() {
        let temp = std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.temp")).unwrap();
        let (install, scratch) = (temp.join("pzserver"), temp.join("s1").join("quit"));
        assert!(installed(&install), "no server in {}", install.display());
        let _ = std::fs::remove_dir_all(&scratch);
        let paths = Paths { zomboid: scratch.join("Zomboid"), data: scratch.join("data") };
        let s = Server {
            name: "pzmmquit".into(),
            build: "b42".into(),
            install_dir: install,
            cachedir: Some(scratch.join("cache")),
            game_port: 16271,
            direct_port: 16272,
            memory_mb: 3072,
            admin_user: "admin".into(),
            ..Default::default()
        };
        let n = NewServer { name: s.name.clone(), game_port: 16271, direct_port: 16272, max_players: 4, ..Default::default() };
        server::set_values(&s.ini(&paths), &new_ini(&n, 27025, &generate_password())).unwrap();
        update(&paths, |all| all.push(s.clone())).unwrap();

        start(&paths, "pzmmquit", Some(&generate_password())).unwrap();
        let p = proc_of("pzmmquit").unwrap();
        let until = Instant::now() + Duration::from_secs(180);
        while lock(&p).status == Status::Starting && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(500));
        }
        assert_eq!(lock(&p).status, Status::Running);
        let pid = find(&paths, "pzmmquit").unwrap().pid.unwrap();

        assert_eq!(send(&paths, "pzmmquit", "quit").unwrap(), None, "typed into its console");
        let until = Instant::now() + Duration::from_secs(90);
        while matches!(lock(&p).status, Status::Running | Status::Stopping) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(500));
        }
        assert_eq!(lock(&p).status, Status::Stopped, "a quit is a stop");
        assert!(crashed("pzmmquit").is_none(), "nothing for crash watch to restart");
        assert!(java_memory(pid).is_none(), "java has gone");
    }
}
