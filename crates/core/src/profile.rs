//! Profiles: named mod setups stored in `<data>/profiles/<name>/`. `modlist.txt` is the
//! library, organised by the player (its order doesn't change what loads):
//!
//! ```text
//! # --- Maps
//! +Muldraugh_Expanded      enabled
//! -SomeMod                 known but disabled
//! ```
//! `loadorder.txt` is the load order: one enabled mod id per line, as written to the game.
//! The active profile is applied to the game's mod list; changes made in-game are
//! adopted back on the next sync, so the two never drift.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::pz::{self, Paths};

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Mod { id: String, on: bool },
    Separator(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub name: String,
    pub lines: Vec<Line>,
    /// The load order (`loadorder.txt`). Only a preference: [`Profile::enabled`] drops ids
    /// that aren't enabled and adds enabled ones it lacks at the end.
    pub order: Vec<String>,
}

impl Profile {
    /// Each mod appears once (first line wins); empty ids are dropped.
    pub fn parse(name: &str, text: &str) -> Profile {
        let mut seen = HashSet::new();
        let lines = text
            .lines()
            .map(str::trim)
            .filter_map(|l| {
                if let Some(s) = l.strip_prefix("# ---") {
                    Some(Line::Separator(s.trim().into()))
                } else if let Some(id) = l.strip_prefix('+') {
                    Some(Line::Mod { id: id.trim().into(), on: true })
                } else {
                    l.strip_prefix('-').map(|id| Line::Mod { id: id.trim().into(), on: false })
                }
            })
            .filter(|l| match l {
                Line::Mod { id, .. } => !id.is_empty() && seen.insert(id.clone()),
                Line::Separator(_) => true,
            })
            .collect();
        Profile { name: name.into(), lines, order: vec![] }
    }

    /// A profile enabling `ids` in order (duplicates and empty ids dropped).
    pub fn from_ids(name: &str, ids: impl IntoIterator<Item = String>) -> Profile {
        let mut seen = HashSet::new();
        let lines = ids.into_iter().filter(|id| !id.is_empty() && seen.insert(id.clone())).map(|id| Line::Mod { id, on: true }).collect();
        Profile { name: name.into(), lines, order: vec![] }
    }

    pub fn to_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| match l {
                Line::Mod { id, on } => format!("{}{id}\n", if *on { '+' } else { '-' }),
                Line::Separator(s) => format!("# --- {s}\n"),
            })
            .collect()
    }

    /// The enabled mods in load order: `order`, then enabled mods it doesn't list in library order.
    pub fn enabled(&self) -> Vec<String> {
        let on: Vec<&String> = self.lines.iter().filter_map(|l| if let Line::Mod { id, on: true } = l { Some(id) } else { None }).collect();
        let (set, mut seen): (HashSet<&String>, _) = (on.iter().copied().collect(), HashSet::new());
        self.order.iter().filter(|id| set.contains(id)).chain(on.iter().copied()).filter(|id| seen.insert(*id)).cloned().collect()
    }

    /// Enable or disable; unknown ids join the library at the end. Newly enabled mods load last.
    pub fn set(&mut self, id: &str, on: bool) {
        match self.lines.iter_mut().find(|l| matches!(l, Line::Mod { id: i, .. } if i == id)) {
            Some(Line::Mod { on: o, .. }) => *o = on,
            _ => self.lines.push(Line::Mod { id: id.into(), on }),
        }
    }

    pub fn forget(&mut self, id: &str) {
        self.lines.retain(|l| !matches!(l, Line::Mod { id: i, .. } if i == id));
    }

    /// Take on the game's current enabled list (it changes when players use the in-game
    /// mod menu): its order becomes the load order. The library keeps its order; mods it
    /// doesn't know yet go at the bottom, after a break if it ends in a named section.
    /// Returns whether anything changed.
    pub fn adopt(&mut self, game: &[String]) -> bool {
        let mut seen = HashSet::new();
        let game: Vec<String> = game.iter().filter(|i| !i.is_empty() && seen.insert(*i)).cloned().collect();
        if self.enabled() == game {
            return false;
        }
        let mut known = HashSet::new();
        for line in &mut self.lines {
            if let Line::Mod { id, on } = line {
                *on = game.contains(id);
                known.insert(id.clone());
            }
        }
        let new: Vec<&String> = game.iter().filter(|i| !known.contains(*i)).collect();
        let in_section = self.lines.iter().rev().find_map(|l| if let Line::Separator(s) = l { Some(!s.is_empty()) } else { None });
        if in_section == Some(true) && !new.is_empty() {
            self.lines.push(Line::Separator(String::new()));
        }
        self.lines.extend(new.into_iter().map(|id| Line::Mod { id: id.clone(), on: true }));
        self.order = game;
        true
    }
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    active: String,
    /// Force the B41 default.txt format; otherwise the file's format is kept.
    #[serde(default)]
    b41: Option<bool>,
    /// default.txt's format the last time it listed mods: an empty list can't show it.
    #[serde(default)]
    seen_b41: Option<bool>,
    /// What `b41` and `seen_b41` were before a Viewpoint profile moved PZMM to Build 42: put back
    /// when another kind of profile is used (a build picked in Settings meanwhile drops it).
    #[serde(default)]
    before_viewpoint: Option<(Option<bool>, Option<bool>)>,
    /// Profile name -> kind (e.g. "viewpoint"). Untagged profiles are the default kind.
    #[serde(default)]
    kinds: std::collections::BTreeMap<String, String>,
    /// Kind -> the profile of that kind used last, so switching kinds returns to it.
    #[serde(default)]
    last: std::collections::BTreeMap<String, String>,
    /// Server profile name -> the server it's for (see [`for_server`]).
    #[serde(default)]
    servers: std::collections::BTreeMap<String, String>,
    /// Server (its key) -> the mods it listed when its profile was last set up.
    #[serde(default)]
    listed: std::collections::BTreeMap<String, Vec<String>>,
    /// Profiles imported from a share code: someone else picked their mods.
    #[serde(default)]
    imported: std::collections::BTreeSet<String>,
    /// state.json couldn't be read: this is an empty stand-in, never saved over it.
    #[serde(skip)]
    unread: bool,
}

/// The player picked this profile's mods themselves: it wasn't set up from a server's list or
/// imported from someone's share code (whose Java mods ZombieBuddy should still ask about).
pub fn picked_by_player(paths: &Paths, name: &str) -> bool {
    let s = state(paths);
    // unknown (state.json unreadable): not the player's, so ZombieBuddy still asks
    !s.unread && s.kinds.get(name).is_none_or(|k| k != SERVER_KIND) && !s.imported.contains(name)
}

/// A mod whose id changed only in case (its author renamed `modx` to `ModX`): the profile's lines take the
/// installed spelling, so the game still finds it.
pub fn follow_case(p: &mut Profile, installed: &std::collections::BTreeMap<String, crate::store::Row>) {
    for l in &mut p.lines {
        if let Line::Mod { id, .. } = l
            && !installed.contains_key(id.as_str())
            && let Some(real) = installed.keys().find(|k| k.eq_ignore_ascii_case(id))
        {
            *id = real.clone();
        }
    }
    for id in &mut p.order {
        if !installed.contains_key(id.as_str())
            && let Some(real) = installed.keys().find(|k| k.eq_ignore_ascii_case(id))
        {
            *id = real.clone();
        }
    }
}

/// The default profile kind: plain Project Zomboid.
pub const DEFAULT_KIND: &str = "zomboid";

/// A profile's kind ("zomboid" unless tagged otherwise).
pub fn kind_of(paths: &Paths, name: &str) -> String {
    state(paths).kinds.get(name).cloned().unwrap_or_else(|| DEFAULT_KIND.into())
}

pub fn set_kind(paths: &Paths, name: &str, kind: &str) -> Result<()> {
    let mut s = state(paths);
    if kind == DEFAULT_KIND {
        s.kinds.remove(name);
    } else {
        s.kinds.insert(name.into(), kind.into());
    }
    save_state(paths, &s)
}

/// Record the active profile as the last one of its kind (done before switching kinds,
/// so switching back returns to it even if it was last applied before kinds existed).
pub fn remember_active(paths: &Paths) -> Result<()> {
    let mut s = state(paths);
    let name = active_name(paths);
    let kind = s.kinds.get(&name).cloned().unwrap_or_else(|| DEFAULT_KIND.into());
    s.last.insert(kind, name);
    save_state(paths, &s)
}

/// The profile of `kind` used most recently, if it still exists.
pub fn last_of_kind(paths: &Paths, kind: &str) -> Option<String> {
    let s = state(paths);
    let existing = list(paths);
    s.last
        .get(kind)
        .filter(|n| existing.contains(n))
        .cloned()
        .or_else(|| existing.into_iter().find(|n| s.kinds.get(n).map_or(DEFAULT_KIND, String::as_str) == kind))
}

/// The kind of the profiles made for a server ("Server <name>"): Project Zomboid profiles all the same.
const SERVER_KIND: &str = "server";

/// The profile for a server (`key`: its address, or `ini:<name>` for a config here), with `ids`
/// enabled (as [`with_list`]) and `maps` arranged, tagged as a server profile: the one made for
/// that server before, even if renamed since; else "Server <name>", or "Server <name> 2"... when
/// that name is taken (public servers often share default names). While the server lists the same
/// mods as last time, its profile stays as the player left it (mods they turned off stay off).
pub fn for_server(paths: &Paths, name: &str, key: &str, ids: Vec<String>, maps: &[String]) -> Result<Profile> {
    state_to_change(paths)?;
    let key = &own_server_key(paths, key);
    let s = state(paths);
    let base = pz::launcher::profile_name(name);
    let mine = s.servers.iter().find(|(p, k)| *k == key && exists(paths, p)).map(|(p, _)| p.clone());
    let p = match mine {
        Some(name) if s.listed.get(key) == Some(&ids) => load(paths, &name)?,
        Some(name) => with_list(paths, &name, ids.clone())?,
        None => Profile::from_ids(&std::iter::once(base.clone()).chain((2..).map(|i| format!("{base} {i}"))).find(|n| !exists(paths, n)).unwrap(), ids.clone()),
    };
    save(paths, &p)?;
    set_map_order(paths, &p.name, maps)?;
    let mut s = state(paths);
    s.kinds.insert(p.name.clone(), SERVER_KIND.into());
    s.servers.insert(p.name.clone(), key.into());
    s.listed.insert(key.into(), ids);
    save_state(paths, &s)?;
    Ok(p)
}

/// Profile `name` was made for the server at `addr` (by [`for_server`]).
pub fn is_for_server(paths: &Paths, name: &str, addr: &str) -> bool {
    state(paths).servers.get(name).is_some_and(|k| *k == own_server_key(paths, addr))
}

/// A server PZMM hosts, joined at this machine's loopback or LAN address and its game port, is keyed as
/// its config (`ini:<name>`, as "Use as profile" keys it), so one server has one profile. Of two
/// on that port (only one can run at a time), the running one.
fn own_server_key(paths: &Paths, key: &str) -> String {
    let here = |h: &str| h.eq_ignore_ascii_case("localhost") || h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback() || Some(ip) == lan_ip());
    let Some((_, port)) = key.rsplit_once(':').filter(|(h, _)| here(h)) else { return key.to_string() };
    let mut own: Vec<_> = pz::host::load(paths).into_iter().filter(|s| port.parse() == Ok(s.game_port)).collect();
    own.sort_by_cached_key(|s| matches!(pz::host::state(s).0, pz::host::Status::Stopped | pz::host::Status::Crashed));
    own.first().map_or_else(|| key.to_string(), |s| format!("ini:{}", s.name))
}

/// This machine's address on its network (the one its default route leaves from; nothing is sent).
fn lan_ip() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    Some(s.local_addr().ok()?.ip())
}

/// A profile for a save played in profile `from`'s game folder: the save's mods `ids` (as
/// [`with_list`]) and `maps` arranged, playing in that same folder (so the game finds the save),
/// a Viewpoint profile when the save uses Viewpoint. Saved, not applied.
pub fn for_save(paths: &Paths, from: &str, name: &str, ids: Vec<String>, maps: &[String]) -> Result<Profile> {
    let existed = exists(paths, name);
    let viewpoint = ids.iter().any(|id| id == pz::viewpoint::MOD_ID);
    let p = with_list(paths, name, ids)?;
    save(paths, &p)?;
    set_map_order(paths, &p.name, maps)?;
    let source = launch_of(paths, from);
    let mut l = if existed { launch_of(paths, &p.name) } else { source.clone() };
    l.cache_dir = source.cache_dir;
    if !existed {
        l.kept = None; // the source profile's own folder, switched off: still its
    }
    if l != launch_of(paths, &p.name) {
        set_launch(paths, &p.name, &l)?;
    }
    if viewpoint {
        set_kind(paths, &p.name, pz::viewpoint::KIND)?;
    }
    Ok(p)
}

/// The server a profile was made for (see [`for_server`]).
pub fn server_of(paths: &Paths, name: &str) -> Option<String> {
    state(paths).servers.get(&on_disk(paths, name)).cloned()
}

/// Server profiles you no longer use: their server isn't in the join history or favourites, nor one
/// PZMM hosts, and they aren't the active profile.
pub fn unused_server_profiles(paths: &Paths) -> Vec<String> {
    let s = state(paths);
    let seen = pz::launcher::state(paths);
    // PZMM's servers and the configs in Zomboid/Server (Use as profile)
    let hosted: Vec<String> = pz::host::load(paths).into_iter().map(|h| h.name).chain(pz::server::list(paths)).map(|n| format!("ini:{n}")).collect();
    let active = active_name(paths);
    list(paths)
        .into_iter()
        .filter(|n| s.kinds.get(n).is_some_and(|k| k == SERVER_KIND) && !n.eq_ignore_ascii_case(&active))
        .filter(|n| s.servers.get(n).is_some_and(|key| !hosted.contains(key) && !seen.history.iter().chain(&seen.favourites).any(|v| v.addr == *key)))
        .collect()
}

fn dir(paths: &Paths) -> PathBuf {
    paths.data.join("profiles")
}

fn valid_name(name: &str) -> Result<()> {
    if !crate::safe_file_name(name) || name.starts_with('.') {
        bail!(crate::keyed!("profileInvalidName", "invalid profile name: \"{name}\"", name = name));
    }
    Ok(())
}

/// Whether a profile with this name exists, ignoring case (Windows folders do).
pub fn exists(paths: &Paths, name: &str) -> bool {
    list(paths).iter().any(|n| n.eq_ignore_ascii_case(name))
}

pub fn list(paths: &Paths) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir(paths))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("modlist.txt").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The on-disk spelling of a profile name (folders ignore case on Windows; PZMM follows them).
fn on_disk(paths: &Paths, name: &str) -> String {
    list(paths).into_iter().find(|n| n.eq_ignore_ascii_case(name)).unwrap_or_else(|| name.to_string())
}

/// How the game starts with a profile (`<profile>/launch.json`): arguments after the ones in
/// Settings, and optionally a game folder of its own (`-cachedir`: saves, options, logs and mod
/// list apart from the main Zomboid folder, so a server's profile leaves your worlds alone).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Launch {
    pub args: String,
    /// The folder the game uses instead of `Zomboid` (None: the usual one).
    pub cache_dir: Option<PathBuf>,
    /// Its own folder while it's switched off: still its (its saves), so no other profile is given it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<PathBuf>,
}

fn launch_file(paths: &Paths, name: &str) -> PathBuf {
    dir(paths).join(on_disk(paths, name)).join("launch.json")
}

pub fn launch_of(paths: &Paths, name: &str) -> Launch {
    crate::read_json(&launch_file(paths, name)).unwrap_or_default()
}

/// The folder a profile gets as its own: `<PZMM data>/game-folders/<name>`, letters, digits, `-`
/// and `_` only (the game reads `-cachedir=` up to the first space). A name that needed changing
/// gets a short hash of itself, so `Server A` and `Server_A` (or two Cyrillic names) never share one.
pub fn own_game_folder(paths: &Paths, name: &str) -> PathBuf {
    let mut clean: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    if clean != name {
        clean = format!("{clean}-{}", &pz::javamods::sha256(name.as_bytes())[..8]);
    }
    // a renamed profile keeps its folder: a new profile of its old name mustn't share it (its saves)
    let long = |p: &std::path::Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase();
    let taken: Vec<String> = list(paths).iter().filter(|n| !n.eq_ignore_ascii_case(name)).flat_map(|n| { let l = launch_of(paths, n); [l.cache_dir, l.kept] }).flatten().map(|d| long(&d)).collect();
    let base = paths.data.join("game-folders");
    std::iter::once(base.join(&clean)).chain((2..).map(|i| base.join(format!("{clean}-{i}")))).find(|d| !taken.contains(&long(d))).unwrap_or_else(|| base.join(clean))
}

/// Existing path `d`'s short (8.3) form, as Windows names it for programs that can't take spaces
/// (None where the drive keeps no short names), or with `long` its long form.
#[cfg(windows)]
pub(crate) fn path_form(d: &std::path::Path, long: bool) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(long: *const u16, short: *mut u16, len: u32) -> u32;
        fn GetLongPathNameW(short: *const u16, long: *mut u16, len: u32) -> u32;
    }
    let wide: Vec<u16> = d.as_os_str().encode_wide().chain([0]).collect();
    let mut buf = vec![0u16; 1024];
    let convert = if long { GetLongPathNameW } else { GetShortPathNameW };
    // SAFETY: a NUL-terminated path, and an output buffer of the length given
    let n = unsafe { convert(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])))
}

#[cfg(not(windows))]
pub(crate) fn path_form(_: &std::path::Path, _: bool) -> Option<PathBuf> {
    None
}

pub fn set_launch(paths: &Paths, name: &str, l: &Launch) -> Result<()> {
    valid_name(name)?;
    if !exists(paths, name) {
        bail!(crate::keyed!("profileMissing", "no profile named \"{name}\"", name = name));
    }
    if l.args.contains(['\n', '\r']) {
        bail!("the arguments must be on one line");
    }
    let mut l = l.clone();
    if let Some(d) = &mut l.cache_dir {
        let spaced = |p: &std::path::Path| p.to_string_lossy().contains(char::is_whitespace);
        if !d.is_absolute() {
            bail!("the game can't use {} as its folder: it needs a full path", d.display());
        }
        fs::create_dir_all(&*d).with_context(|| format!("making {}", d.display()))?;
        // the game reads -cachedir= up to the first space: a path with spaces goes by its short name
        if spaced(d) {
            match path_form(d, false).filter(|s| !spaced(s)) {
                Some(short) => *d = short,
                None => {
                    let _ = fs::remove_dir(&*d);
                    bail!("the game can't use {} as its folder: it needs a path without spaces", d.display());
                }
            }
        }
    }
    crate::write_atomic(&launch_file(paths, name), serde_json::to_string_pretty(&l)?)?;
    Ok(())
}

/// The game's arguments for profile `name` (after the ones in Settings): its own, and with a
/// folder of its own `-cachedir=<folder>`, whose `mods` leads to the Zomboid folder's (see
/// [`share_mods`]) so the game loads the same mods. Also that folder, or None for the usual one.
pub fn launch_args(paths: &Paths, name: &str) -> Result<(String, Option<PathBuf>)> {
    let l = launch_of(paths, name);
    let Some(d) = l.cache_dir else { return Ok((l.args.trim().to_string(), None)) };
    fs::create_dir_all(&d).with_context(|| format!("making {}", d.display()))?;
    share_mods(&paths.mods(), &d.join("mods"))?;
    Ok((format!("{} -cachedir={}", l.args.trim(), d.display()).trim().to_string(), Some(d)))
}

/// With `-cachedir` the game reads mods and its mod list from `<folder>/mods` only, so `link`
/// becomes a link (a junction on Windows) to `mods`, where PZMM installs them. A folder an older
/// PZMM made there (just the copied list) gives way; one holding more stays, with the list copied in.
fn share_mods(mods: &std::path::Path, link: &std::path::Path) -> Result<()> {
    let same = |a: &std::path::Path, b: &std::path::Path| fs::canonicalize(a).ok().is_some_and(|a| fs::canonicalize(b).is_ok_and(|b| a == b));
    if same(link, mods) {
        return Ok(());
    }
    let meta = fs::symlink_metadata(link);
    if meta.as_ref().is_ok_and(|m| m.file_type().is_symlink()) {
        // a link to where the Zomboid folder used to be
        fs::remove_dir(link).or_else(|_| fs::remove_file(link)).with_context(|| format!("removing {}", link.display()))?;
    } else if meta.is_ok() {
        let just_the_list = fs::read_dir(link)?.flatten().all(|e| e.file_name() == "default.txt");
        if !just_the_list {
            let list = mods.join("default.txt");
            if list.is_file() {
                fs::copy(&list, link.join("default.txt")).with_context(|| format!("copying the mod list to {}", link.display()))?;
            }
            return Ok(());
        }
        fs::remove_dir_all(link).with_context(|| format!("removing {}", link.display()))?;
    }
    fs::create_dir_all(mods)?;
    // cmd expands % even quoted (see link_dir); said here, not under "linking", so it's in the player's language
    if let Some(p) = [link, mods].into_iter().find(|p| cfg!(windows) && p.to_string_lossy().contains('%')) {
        bail!(crate::keyed!("linkPercent", "{path} can't be linked: its path has a % in it. Rename or move that folder.", path = p.display()));
    }
    link_dir(mods, link).with_context(|| format!("linking {} to {}", link.display(), mods.display()))
}

#[cfg(windows)]
fn link_dir(target: &std::path::Path, link: &std::path::Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // a junction needs no admin rights or Developer Mode, unlike a symbolic link
    const NO_WINDOW: u32 = 0x0800_0000;
    // quoted for cmd, where & ^ ( ) are then plain; a path can't hold ", but % can still expand (share_mods refuses it)
    let out = std::process::Command::new("cmd").raw_arg(format!("/C mklink /J \"{}\" \"{}\"", link.display(), target.display())).creation_flags(NO_WINDOW).output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(not(windows))]
fn link_dir(target: &std::path::Path, link: &std::path::Path) -> Result<()> {
    Ok(std::os::unix::fs::symlink(target, link)?)
}

pub fn load(paths: &Paths, name: &str) -> Result<Profile> {
    valid_name(name)?;
    let name = &on_disk(paths, name);
    let p = dir(paths).join(name).join("modlist.txt");
    if !p.exists() {
        bail!(crate::keyed!("profileMissing", "no profile named \"{name}\"", name = name));
    }
    // lossy: a separator saved as ANSI by Notepad still loads (one odd character) rather than failing
    let mut profile = Profile::parse(name, &String::from_utf8_lossy(&fs::read(p)?));
    // no load order yet (made before 1.1): the library's order was the load order, so nothing moves
    profile.order = match fs::read(dir(paths).join(name).join("loadorder.txt")) {
        Ok(b) => String::from_utf8_lossy(&b).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => profile.enabled(),
        // unreadable is not missing: guessing would overwrite the player's order on the next save
        Err(e) => return Err(e).with_context(|| format!("couldn't read the load order of {name:?}")),
    };
    Ok(profile)
}

/// Profile `name` with `ids` enabled in order. An existing one keeps its library (sections,
/// disabled mods) and takes the new list instead of being replaced (snapshotted first, if it changes).
pub fn with_list(paths: &Paths, name: &str, ids: Vec<String>) -> Result<Profile> {
    if !exists(paths, name) {
        return Ok(Profile::from_ids(name, ids));
    }
    let mut p = load(paths, name)?;
    if p.adopt(&ids) {
        snapshot(paths, name, "import")?; // the copy on disk is still the one before
    }
    Ok(p)
}

pub fn save(paths: &Paths, profile: &Profile) -> Result<()> {
    valid_name(&profile.name)?;
    let d = dir(paths).join(&profile.name);
    let new = !d.exists();
    fs::create_dir_all(&d)?;
    crate::write_atomic(&d.join("modlist.txt"), profile.to_text())?;
    crate::write_atomic(&d.join("loadorder.txt"), profile.enabled().iter().map(|id| format!("{id}\n")).collect::<String>())?;
    if new {
        // a new profile (listed once its modlist.txt is there): the game's list of them follows
        profiles_for_game(paths);
    }
    Ok(())
}

/// The profile's arranged `Map=` order (`maporder.txt`, beside loadorder.txt): what the Maps
/// tab and the server card start from. Empty until maps are arranged.
pub fn map_order(paths: &Paths, name: &str) -> Vec<String> {
    let file = dir(paths).join(on_disk(paths, name)).join("maporder.txt");
    fs::read_to_string(file).map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default()
}

/// Save `maps` as the arranged order; maps it lacks keep their place (see `server::merge_order`).
/// Returns whether the saved order changed.
pub fn set_map_order(paths: &Paths, name: &str, maps: &[String]) -> Result<bool> {
    valid_name(name)?;
    let old = map_order(paths, name);
    let new = pz::server::merge_order(&old, maps);
    if new == old {
        return Ok(false);
    }
    let d = dir(paths).join(on_disk(paths, name));
    fs::create_dir_all(&d)?;
    crate::write_atomic(&d.join("maporder.txt"), new.iter().map(|m| format!("{m}\n")).collect::<String>())?;
    Ok(true)
}

/// Where `profile`'s `Map=` order starts: its arranged order (maporder.txt); never arranged,
/// default.txt's `maps` block as it stands (hand-arranged, or another profile's), then the
/// enabled mods' maps on `build` in load order. Reads only.
pub fn map_start(paths: &Paths, profile: &Profile, mods: &std::collections::BTreeMap<String, crate::store::Row>, build: u8) -> Vec<String> {
    let order = map_order(paths, &profile.name);
    if !order.is_empty() {
        return order;
    }
    let mut out = pz::read_default(&paths.default_txt()).map_folders();
    out.extend(pz::server::mod_maps(&profile.enabled(), mods, build));
    out
}

/// default.txt's `maps` block for `profile`: its enabled mods' maps on `build`, in the order
/// `map_start` gives (so a never-arranged profile keeps the block's order but drops other
/// profiles' maps), without Muldraugh (`write_default` keeps it last), from the mod scan `mods`.
pub fn game_maps(paths: &Paths, profile: &Profile, mods: &std::collections::BTreeMap<String, crate::store::Row>, build: u8) -> Vec<String> {
    let mut allowed = pz::server::mod_maps(&profile.enabled(), mods, build);
    // folders no installed mod provides (a mod outside PZMM's view) aren't PZMM's to drop
    let ids: Vec<String> = mods.keys().cloned().collect();
    let known = pz::server::mod_maps(&ids, mods, build);
    let block = pz::read_default(&paths.default_txt()).map_folders();
    let unknown: Vec<String> = block.iter().filter(|f| !known.contains(f)).cloned().collect();
    allowed.extend(unknown.iter().cloned());
    let start = map_start(paths, profile, mods, build);
    // ...and keep their place in the block (after the map they followed), arranged or not
    let anchors: Vec<String> = block.into_iter().filter(|f| unknown.contains(f) || start.contains(f)).collect();
    let line = pz::server::map_order(&pz::server::merge_order(&anchors, &start), &allowed);
    line.into_iter().filter(|m| !pz::server::is_base_map(m)).collect()
}

pub fn delete(paths: &Paths, name: &str) -> Result<()> {
    valid_name(name)?;
    let name = &on_disk(paths, name);
    if active_name(paths).eq_ignore_ascii_case(name) {
        bail!(crate::keyed!("profileActive", "\"{name}\" is the active profile; switch to another first", name = name));
    }
    let folder = dir(paths).join(name);
    if crate::settings::load().recycle && !cfg!(test) {
        trash::delete(&folder).with_context(|| format!("moving {} to the Recycle Bin", folder.display()))?;
    } else {
        fs::remove_dir_all(folder)?;
    }
    let mut s = state(paths);
    s.kinds.remove(name);
    s.servers.remove(name);
    s.imported.remove(name);
    s.last.retain(|_, n| n != name);
    save_state(paths, &s)
}

/// Rename a profile (its folder, its kind, and the active / last-used records). A change of
/// case only goes through a temporary name, since the folder is the same one on Windows.
/// Folders are renamed, never copied and deleted, so a failure leaves the profile as it was.
pub fn rename(paths: &Paths, from: &str, to: &str) -> Result<()> {
    state_to_change(paths)?;
    valid_name(from)?;
    valid_name(to)?;
    if !exists(paths, from) {
        bail!(crate::keyed!("profileMissing", "no profile named \"{name}\"", name = from));
    }
    let from = on_disk(paths, from);
    if from == to {
        return Ok(());
    }
    let case_only = from.eq_ignore_ascii_case(to);
    if !case_only && exists(paths, to) {
        bail!(crate::keyed!("profileExists", "a profile named \"{name}\" already exists", name = to));
    }
    let active = active_name(paths).eq_ignore_ascii_case(&from);
    let (old, new) = (dir(paths).join(&from), dir(paths).join(to));
    if case_only {
        // valid and visible: if PZMM stops between the two steps, the profile still shows up
        let tmp = dir(paths).join(format!("{from}.renaming"));
        fs::rename(&old, &tmp).with_context(|| format!("renaming {from:?} (close anything using its folder)"))?;
        fs::rename(&tmp, &new).with_context(|| format!("renaming {from:?}: it's in {}", tmp.display()))?;
    } else {
        fs::rename(&old, &new).with_context(|| format!("renaming {from:?} (close anything using its folder)"))?;
    }
    let mut s = state(paths);
    if let Some(k) = s.kinds.remove(&from) {
        s.kinds.insert(to.into(), k);
    }
    if let Some(k) = s.servers.remove(&from) {
        s.servers.insert(to.into(), k);
    }
    if s.imported.remove(&from) {
        s.imported.insert(to.into());
    }
    s.last.values_mut().filter(|n| **n == from).for_each(|n| *n = to.into());
    if active {
        s.active = to.into();
    }
    save_state(paths, &s)?;
    // the game's copy names the profile
    if active && let Ok(p) = load(paths, to) {
        library_for_game(paths, &p);
    }
    Ok(())
}

/// A copy of profile `from` (library, load order, map order, launch options, kind) named `to`. A copy of a
/// server's profile is a plain one: it isn't that server's (joining it again won't pick it).
pub fn duplicate(paths: &Paths, from: &str, to: &str) -> Result<()> {
    state_to_change(paths)?;
    valid_name(to)?;
    if exists(paths, to) {
        bail!(crate::keyed!("profileExists", "a profile named \"{name}\" already exists", name = to));
    }
    let p = load(paths, from)?;
    let picked = picked_by_player(paths, from);
    let maps = read_map_order(paths, &p.name)?;
    let kind = Some(kind_of(paths, &p.name)).filter(|k| k != SERVER_KIND).unwrap_or_else(|| DEFAULT_KIND.into());
    save(paths, &Profile { name: to.into(), ..p })?;
    if !maps.is_empty() {
        crate::write_atomic(&dir(paths).join(to).join("maporder.txt"), maps.iter().map(|m| format!("{m}\n")).collect::<String>())?;
    }
    let mut l: Launch = crate::read_json(&launch_file(paths, &p.name))?;
    if l != Launch::default() {
        // a game folder of its own is its own: the copy gets a new one, not the original's (its saves, its options)
        if l.cache_dir.is_some() {
            l.cache_dir = Some(own_game_folder(paths, to));
        }
        l.kept = None; // the original's
        set_launch(paths, to, &l)?;
    }
    set_kind(paths, to, &kind)?;
    // someone else picked its mods: ZombieBuddy still asks about the copy's Java mods
    if !picked {
        let mut s = state(paths);
        s.imported.insert(to.into());
        save_state(paths, &s)?;
    }
    Ok(())
}

/// maporder.txt for snapshots and copies: missing is empty, unreadable is an error (an empty
/// copy would later overwrite the player's arrangement).
fn read_map_order(paths: &Paths, name: &str) -> Result<Vec<String>> {
    match fs::read_to_string(dir(paths).join(on_disk(paths, name)).join("maporder.txt")) {
        Ok(t) => Ok(t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e).with_context(|| format!("couldn't read the map order of {name:?}")),
    }
}

/// How many snapshots each profile keeps.
const SNAPSHOTS: usize = 10;

/// A profile's lists as they were before a big change (`<profile>/snapshots/<ms>.json`).
#[derive(Serialize, Deserialize)]
struct Snapshot {
    time: u64,
    /// What came next: "update", "autosort", "group", "remove", "import", "restore", "maps".
    label: String,
    /// modlist.txt
    list: String,
    order: Vec<String>,
    maps: Vec<String>,
}

/// One snapshot in the list (newest first).
#[derive(Debug, Serialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub time: u64,
    pub label: String,
    pub enabled: usize,
    pub mods: usize,
}

fn snapshot_dir(paths: &Paths, name: &str) -> PathBuf {
    dir(paths).join(on_disk(paths, name)).join("snapshots")
}

/// The snapshot ids of `name`, newest first (ids are creation times in ms).
fn snapshot_ids(paths: &Paths, name: &str) -> Vec<u64> {
    let mut ids: Vec<u64> = fs::read_dir(snapshot_dir(paths, name))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json")?.parse().ok())
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids
}

/// Keep the saved state of profile `name` before a change labelled `label`; the oldest beyond
/// [`SNAPSHOTS`] go. Refuses (and changes nothing) when the profile can't be read.
pub fn snapshot(paths: &Paths, name: &str, label: &str) -> Result<()> {
    let p = load(paths, name)?;
    let snap = Snapshot { time: crate::store::now(), label: label.into(), list: p.to_text(), order: p.enabled(), maps: read_map_order(paths, name)? };
    let d = snapshot_dir(paths, name);
    fs::create_dir_all(&d)?;
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let id = (ms..).find(|i| !d.join(format!("{i}.json")).exists()).unwrap_or(ms);
    crate::write_atomic(&d.join(format!("{id}.json")), serde_json::to_string(&snap)?)?;
    for old in snapshot_ids(paths, name).into_iter().skip(SNAPSHOTS) {
        fs::remove_file(d.join(format!("{old}.json")))?;
    }
    Ok(())
}

/// Profile `name`'s snapshots, newest first (unreadable ones are left out).
pub fn snapshots(paths: &Paths, name: &str) -> Vec<SnapshotInfo> {
    let d = snapshot_dir(paths, name);
    snapshot_ids(paths, name)
        .into_iter()
        .filter_map(|id| {
            let s: Snapshot = serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.json"))).ok()?).ok()?;
            let p = Profile { order: s.order, ..Profile::parse(name, &s.list) };
            let mods = p.lines.iter().filter(|l| matches!(l, Line::Mod { .. })).count();
            Some(SnapshotInfo { id: id.to_string(), time: s.time, label: s.label, enabled: p.enabled().len(), mods })
        })
        .collect()
}

/// Put profile `name` back as snapshot `id` had it, after snapshotting its current state (so a
/// restore can be undone the same way). Saves it; the caller applies it if it's active.
pub fn restore(paths: &Paths, name: &str, id: &str) -> Result<Profile> {
    let id: u64 = id.parse().with_context(|| format!("not a snapshot: {id:?}"))?;
    let file = snapshot_dir(paths, name).join(format!("{id}.json"));
    let text = fs::read_to_string(&file).with_context(|| format!("no snapshot {id} of {name:?}"))?;
    let s: Snapshot = serde_json::from_str(&text).with_context(|| format!("reading {}", file.display()))?;
    snapshot(paths, name, "restore")?;
    let name = on_disk(paths, name);
    let p = Profile { order: s.order, ..Profile::parse(&name, &s.list) };
    save(paths, &p)?;
    let maps = dir(paths).join(&name).join("maporder.txt");
    if s.maps.is_empty() {
        if maps.exists() {
            fs::remove_file(maps)?; // it was never arranged then
        }
    } else {
        crate::write_atomic(&maps, s.maps.iter().map(|m| format!("{m}\n")).collect::<String>())?;
    }
    Ok(p)
}

fn state(paths: &Paths) -> State {
    crate::read_json(&paths.data.join("state.json")).unwrap_or_else(|e| {
        crate::log!("error: {e:#}");
        State { unread: true, ..Default::default() }
    })
}

/// The state, for a change: an error when state.json can't be read (before anything is written).
fn state_to_change(paths: &Paths) -> Result<State> {
    let s = state(paths);
    if s.unread {
        bail!("couldn't read {}: try again", paths.data.join("state.json").display());
    }
    Ok(s)
}

fn save_state(paths: &Paths, s: &State) -> Result<()> {
    if s.unread {
        bail!("couldn't read {}, so it wasn't saved over: try again", paths.data.join("state.json").display());
    }
    fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&paths.data.join("state.json"), serde_json::to_string_pretty(s)?)?;
    profiles_for_game(paths);
    Ok(())
}

/// The profiles for the PZMM - Client mod's in-game screen: `Zomboid/Lua/pzmm/profiles.txt`, `active\t<name>`
/// then a `<name>\t<kind>` line per profile. Written with every profile change, where that folder is.
pub fn profiles_for_game(paths: &Paths) {
    let dir = crate::pz::bridge::dir(paths);
    if !dir.is_dir() {
        return;
    }
    let s = state(paths);
    let mut text = format!("active\t{}\n", active_name(paths));
    for n in list(paths) {
        text += &format!("{n}\t{}\n", s.kinds.get(&n).map_or(DEFAULT_KIND, String::as_str));
    }
    crate::pz::bridge::write_for_game(&dir.join("profiles.txt"), text);
}

pub fn active_name(paths: &Paths) -> String {
    Some(state(paths).active).filter(|s| !s.is_empty()).unwrap_or_else(|| "Default".into())
}

/// Forced default.txt format (41 or 42), or None to keep the file's own.
pub fn build(paths: &Paths) -> Option<u8> {
    state(paths).b41.map(|b| if b { 41 } else { 42 })
}

/// The build being played: the forced format, else what default.txt looks like (B42 if unknown).
pub fn effective_build(paths: &Paths) -> u8 {
    let d = pz::read_default(&paths.default_txt());
    let b41 = if d.mods.is_empty() { state(paths).seen_b41.unwrap_or(false) } else { d.b41 };
    build(paths).unwrap_or(if b41 { 41 } else { 42 })
}

pub fn set_b41(paths: &Paths, b41: Option<bool>) -> Result<()> {
    save_state(paths, &State { b41, before_viewpoint: None, ..state(paths) })
}

/// The active profile as saved, without picking up in-game changes (for callers that don't
/// hold the data lock and so must not write).
pub fn peek(paths: &Paths) -> Profile {
    let name = active_name(paths);
    load(paths, &name).unwrap_or(Profile { name, ..Default::default() })
}

/// Load the active profile (creating "Default" from the game's list on first run) and
/// adopt any changes made in-game. Returns the profile and whether it adopted changes.
pub fn sync(paths: &Paths) -> Result<(Profile, bool)> {
    state_to_change(paths)?;
    let name = active_name(paths);
    // a profile that exists but can't be read is an error: starting empty would replace it with the game's list
    let mut p = if dir(paths).join(&name).join("modlist.txt").exists() { load(paths, &name)? } else { Profile { name, ..Default::default() } };
    // a missing or unreadable default.txt (game running, odd encoding) says nothing about
    // what's enabled: adopting it would switch the whole profile off
    let game = pz::try_read_default(&paths.default_txt());
    let reset = game.as_ref().is_some_and(|g| game_reset(paths, &p, g));
    // said once per reset, not on every view
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if reset && !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
        crate::say!("gameReset", "the game emptied its mod list (it does that once after some updates), so PZMM kept yours: launch from PZMM to put it back");
    }
    if !reset {
        SAID.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    let game = game.filter(|_| !reset);
    // the in-game mod menu turned mods off: keep the list as it was, in case that wasn't meant
    if let Some(g) = &game
        && dir(paths).join(&p.name).join("modlist.txt").exists()
        && p.enabled().iter().any(|id| !g.mods.contains(id))
    {
        snapshot(paths, &p.name, "adopt")?;
    }
    // the game's Maps order (its mod screen, or PZMM - Client's) is the profile's, once it has arranged one
    // (never arranged, it starts from default.txt's order anyway). Only when the game wrote default.txt after
    // PZMM did: PZMM's own write puts newly enabled maps in, which isn't the game arranging them.
    let modified = |f: PathBuf| fs::metadata(f).and_then(|m| m.modified()).ok();
    let ours = dir(paths).join(on_disk(paths, &p.name));
    let saved = [modified(ours.join("loadorder.txt")), modified(ours.join("maporder.txt"))].into_iter().flatten().max();
    let by_game = match (modified(paths.default_txt()), saved) {
        (Some(game), Some(ours)) => game.duration_since(ours).is_ok_and(|d| d > std::time::Duration::from_secs(2)),
        _ => false,
    };
    if let Some(g) = &game
        && by_game
        && !map_order(paths, &p.name).is_empty()
        && set_map_order(paths, &p.name, &g.map_folders())?
    {
        crate::say!("mapOrderAdopted", "Took the map order arranged in the game.");
    }
    let took = game_sections(paths, &mut p);
    let changed = game.is_some_and(|game| p.adopt(&game.mods)) || took;
    if changed || !dir(paths).join(&p.name).exists() {
        save(paths, &p)?;
        // the game's copy of the sections follows (mods it turned on join them)
        library_for_game(paths, &p);
    }
    Ok((p, changed))
}

/// Sections made in the game before PZMM knew it (PZMM - Client writes "# PZMM profile: In game" to
/// library.txt without the app): `p`, if it has none, takes them and their order; its mods the game didn't
/// list follow after a break. Returns whether it did.
fn game_sections(paths: &Paths, p: &mut Profile) -> bool {
    let named = |p: &Profile| p.lines.iter().any(|l| matches!(l, Line::Separator(s) if !s.is_empty()));
    let text = fs::read_to_string(crate::pz::bridge::dir(paths).join("library.txt")).unwrap_or_default();
    let Some(body) = text.strip_prefix("# PZMM profile: In game\n") else { return false };
    let game = Profile::parse("In game", body);
    if named(p) || !named(&game) {
        return false;
    }
    let on: std::collections::HashMap<&String, bool> = p.lines.iter().filter_map(|l| if let Line::Mod { id, on } = l { Some((id, *on)) } else { None }).collect();
    let mut lines: Vec<Line> = game
        .lines
        .iter()
        .map(|l| match l {
            Line::Mod { id, on: o } => Line::Mod { id: id.clone(), on: *on.get(id).unwrap_or(o) },
            sep => sep.clone(),
        })
        .collect();
    let listed: HashSet<&String> = game.lines.iter().filter_map(|l| if let Line::Mod { id, .. } = l { Some(id) } else { None }).collect();
    let rest: Vec<Line> = p.lines.iter().filter(|l| matches!(l, Line::Mod { id, .. } if !listed.contains(id))).cloned().collect();
    if !rest.is_empty() {
        lines.push(Line::Separator(String::new()));
        lines.extend(rest);
    }
    p.lines = lines;
    true
}

/// Whether the game's mod list isn't `p`'s (the game emptied it, or it's missing): the game
/// would start without `p`'s mods. An unreadable one (the game has it open) says nothing.
pub fn game_differs(paths: &Paths, p: &Profile) -> bool {
    let file = paths.default_txt();
    if !file.exists() {
        return !p.enabled().is_empty();
    }
    pz::try_read_default(&file).is_some_and(|d| d.mods != p.enabled())
}

/// Build 42 empties default.txt on its first start after some updates, unless the marker
/// `mods/reset-mods-<release>.txt` exists (`ZomboidFileSystem.resetDefaultModsForNewRelease`).
/// True when the game's list is empty, `p` has mods on and a marker is newer than `p`'s last save
/// and was written with the list (within seconds): a list the player emptied later is theirs.
fn game_reset(paths: &Paths, p: &Profile, game: &pz::DefaultTxt) -> bool {
    let modified = |f: PathBuf| fs::metadata(f).and_then(|m| m.modified()).ok();
    let Some(saved) = modified(dir(paths).join(on_disk(paths, &p.name)).join("loadorder.txt")) else { return false };
    let Some(emptied) = modified(paths.default_txt()) else { return false };
    let together = |t: std::time::SystemTime| t.duration_since(emptied).unwrap_or_else(|e| e.duration()) <= std::time::Duration::from_secs(10);
    game.mods.is_empty()
        && !p.enabled().is_empty()
        && fs::read_dir(paths.zomboid.join("mods"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("reset-mods-") && modified(e.path()).is_some_and(|t| t > saved && together(t)))
}

/// Write the marker that stops Build 42 emptying default.txt (see [`game_reset`]). Before the
/// profile's save: a marker newer than the save reads as the game's reset.
pub fn keep_through_game_reset(paths: &Paths) {
    // tests keep a game folder next to their Zomboid folder
    let game = if cfg!(test) { Some(paths.zomboid.with_file_name("game")) } else { pz::folders::path("game", &crate::settings::load()) };
    let Some(game) = game else { return };
    let Some(release) = ["projectzomboid.jar", "projectzomboid/projectzomboid.jar"].iter().find_map(|j| reset_release(&game.join(j))) else {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::say!("resetMarkerUnknown", "Couldn't tell which game release this is, so the game may empty its mod list on its first start: Play from PZMM puts it back");
        }
        return;
    };
    let marker = paths.zomboid.join("mods").join(format!("reset-mods-{release}.txt"));
    if !marker.exists() {
        let _ = fs::write(marker, "If this file does not exist, default.txt will be reset to empty (no mods active).");
    }
}

/// The marker's release part (`42_00`): the only `<digits>_<digits>` string in the game's `GameWindow` class.
fn reset_release(jar: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut class = vec![];
    zip::ZipArchive::new(fs::File::open(jar).ok()?).ok()?.by_name("zombie/GameWindow.class").ok()?.read_to_end(&mut class).ok()?;
    // constant pool strings: tag 1, a big-endian u16 length, the bytes
    let found: HashSet<&str> = (0..class.len().saturating_sub(3))
        .filter(|&i| class[i] == 1)
        .filter_map(|i| {
            let len = u16::from_be_bytes([class[i + 1], class[i + 2]]) as usize;
            let s = std::str::from_utf8(class.get(i + 3..i + 3 + len)?).ok()?;
            let (a, b) = s.split_once('_')?;
            let digits = |t: &str| !t.is_empty() && t.len() <= 4 && t.bytes().all(|c| c.is_ascii_digit());
            (digits(a) && digits(b)).then_some(s)
        })
        .collect();
    // ponytail: a guess from the class file; two candidates means the game changed, so write nothing
    (found.len() == 1).then(|| found.into_iter().next().unwrap().to_string())
}

/// Save `profile`, make it active and write it to the game's mod list, its maps from the mod
/// scan `mods` (None: the mods couldn't be read, so the `maps` block is kept as it is).
/// With the "keep favourites and hidden mods" setting, this runs on every profile made or switched
/// to, except a server's (its list is the server's, which a favourite or hidden mod would break).
pub fn apply(paths: &Paths, profile: &Profile, mods: Option<&std::collections::BTreeMap<String, crate::store::Row>>) -> Result<()> {
    let before = state_to_change(paths)?;
    // a switch: what the game changed in the profile being left is kept first
    let leaving = Some(before.active).filter(|a| !a.is_empty() && *a != profile.name && exists(paths, a));
    // the running game stays in its folder: a switch to a profile that plays in another would cut it off from PZMM
    if leaving.is_some() && pz::host::game_running() && !pz::bridge::same_dir(&pz::bridge::dir_for(paths, &profile.name), &pz::bridge::game_dir(paths)) {
        bail!(crate::keyed!("profileOtherFolder", "{name} plays in another game folder: quit the game first, then Play from PZMM", name = profile.name));
    }
    if leaving.is_some()
        && let Err(e) = sync(paths)
    {
        // the profile being left can't be read: switching away from it must still work
        crate::log!("error: {e:#}");
    }
    let mut s = state(paths);
    let mut profile = profile.clone();
    let server = s.kinds.get(&profile.name).is_some_and(|k| k == SERVER_KIND);
    if s.active != profile.name && !server && crate::settings::load().keep_marks {
        let (on, off) = keep_marks(&mut profile, &pz::managers::favourites(paths), &pz::managers::hidden(paths), |id| mods.is_some_and(|m| m.contains_key(id)));
        if on > 0 {
            crate::say!("marksOn", "\"{name}\": turned on {count} favourite(s), as Settings ask.", name = profile.name, count = on);
        }
        if off > 0 {
            crate::say!("marksOff", "\"{name}\": turned off {count} hidden mod(s), as Settings ask.", name = profile.name, count = off);
        }
    }
    let profile = &profile;
    if let Some(d) = pz::try_read_default(&paths.default_txt()).filter(|d| !d.mods.is_empty()) {
        s.seen_b41 = Some(d.b41);
    }
    // Project Viewpoint only runs on Build 42; leaving it puts back the build played before
    let viewpoint = s.kinds.get(&profile.name).is_some_and(|k| k == pz::viewpoint::KIND);
    if viewpoint && s.b41.or(s.seen_b41) == Some(true) {
        s.before_viewpoint = Some((s.b41, s.seen_b41));
        s.b41 = Some(false);
        crate::say!("viewpointB42", "Switched to Build 42 for \"{name}\": Project Viewpoint doesn't run on Build 41.", name = profile.name);
    } else if !viewpoint && let Some((b41, seen)) = s.before_viewpoint.take() {
        (s.b41, s.seen_b41) = (b41, seen);
    }
    let b41 = s.b41.or(s.seen_b41);
    if b41 != Some(true) {
        keep_through_game_reset(paths);
    }
    save(paths, profile)?;
    let maps = mods.map(|m| game_maps(paths, profile, m, if b41.unwrap_or(false) { 41 } else { 42 }));
    pz::write_default(&paths.default_txt(), &profile.enabled(), b41, maps.as_deref())?;
    let kind = s.kinds.get(&profile.name).cloned().unwrap_or_else(|| DEFAULT_KIND.into());
    s.last.insert(kind, profile.name.clone());
    save_state(paths, &State { active: profile.name.clone(), ..s })?;
    // once it's active: its own game folder, if it has one, is the bridge now
    library_for_game(paths, profile);
    Ok(())
}

/// The active profile's library (its sections and mods, as My mods shows them) for the PZMM - Client
/// mod's in-game mod screen: `Zomboid/Lua/pzmm/library.txt`, where that folder is (the mod or the app's start makes it).
pub fn library_for_game(paths: &Paths, p: &Profile) {
    let dir = crate::pz::bridge::dir(paths);
    if dir.is_dir() {
        crate::pz::bridge::write_for_game(&dir.join("library.txt"), format!("# PZMM profile: {}\n{}", p.name, p.to_text()));
        // the load order (library.txt only says on or off): the game's "Use PZMM's mods" takes PZMM's places too
        crate::pz::bridge::write_for_game(&dir.join("order.txt"), p.enabled().iter().map(|id| format!("{id}\n")).collect::<String>());
    }
}

/// Tell the PZMM - Client mod the app is installed and running: `Zomboid/Lua/pzmm/app.txt` (game Lua can
/// only read under `Zomboid/Lua`), written every few seconds while PZMM runs, so a `running_at` more than
/// 15 s old means it's closed. Plain `key=value` lines, which Lua reads easily.
pub fn app_for_game(paths: &Paths, version: &str) {
    let dir = crate::pz::bridge::dir(paths);
    // where the game has run (its Lua folder): the folder for PZMM - Client is made here
    if dir.parent().is_some_and(std::path::Path::is_dir) {
        let _ = std::fs::create_dir_all(&dir);
        let name = active_name(paths);
        // busy: the game's requests are still being handled (queued, or a question about one is open), so its
        // sorting edits wait for PZMM rather than giving up. default_rev: when default.txt last changed, so the game
        // can tell PZMM changed the mod list since its screen opened.
        let busy = crate::pz::bridge::GAME_BUSY.load(std::sync::atomic::Ordering::Relaxed) || dir.join("requests.queue").exists();
        let rev = fs::metadata(paths.default_txt()).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let (accent, theme) = crate::pz::bridge::LOOK.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let text = format!(
            "version={version}\nproto=1\nrunning_at={}\nprofile={name}\nkind={}\nbusy={}\ndefault_rev={rev}\naccent={accent}\ntheme={theme}\n",
            crate::store::now(),
            kind_of(paths, &name),
            u8::from(busy)
        );
        let _ = crate::write_atomic(&dir.join("app.txt"), text);
        // a folder made since PZMM started (a profile's own game folder, on its first run): what the game reads from it
        if !dir.join("library.txt").exists() {
            library_for_game(paths, &peek(paths));
            profiles_for_game(paths);
        }
    }
}

/// Turn on the `installed` favourites and turn off the hidden mods (hidden wins over favourite).
/// Returns how many it turned on and off.
pub fn keep_marks(p: &mut Profile, favourites: &[String], hidden: &[String], installed: impl Fn(&str) -> bool) -> (usize, usize) {
    let enabled: HashSet<String> = p.enabled().into_iter().collect();
    let on: Vec<&String> = favourites.iter().filter(|id| !hidden.contains(id) && !enabled.contains(*id) && installed(id)).collect();
    let off: Vec<&String> = hidden.iter().filter(|id| enabled.contains(*id)).collect();
    on.iter().for_each(|id| p.set(id, true));
    off.iter().for_each(|id| p.set(id, false));
    (on.len(), off.len())
}

/// The mod scan for `apply`, None (and said in the activity log) when it failed.
pub fn maps_scan<T>(scan: Result<T>) -> Option<T> {
    scan.inspect_err(|e| crate::say!("mapsNotUpdated", "error: couldn't read your mods, so the game's map list wasn't updated: {error}", error = format!("{e:#}"))).ok()
}

/// A profile to share with friends or a server: its list plus where each mod comes
/// from on the Workshop, so the receiver can install what they're missing.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Shared {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    /// The profile in modlist.txt form (`+Id`, `-Id`, `# --- Separator`).
    pub list: String,
    /// The load order (codes made before 1.1 lack it: then `list`'s order is used).
    #[serde(default)]
    pub order: Vec<String>,
    /// Mod id -> Workshop item id, for mods that come from the Workshop.
    #[serde(default)]
    pub workshop: std::collections::BTreeMap<String, String>,
    /// The arranged `Map=` order (maporder.txt; codes made before 1.2 lack it: then load order).
    #[serde(default)]
    pub maps: Vec<String>,
}

/// Package the active profile for sharing (`mods`: the current scan, for Workshop ids).
pub fn export(paths: &Paths, mods: &std::collections::BTreeMap<String, crate::store::Row>) -> Result<Shared> {
    let p = sync(paths)?.0;
    let workshop = p
        .lines
        .iter()
        .filter_map(|l| if let Line::Mod { id, .. } = l { Some(id) } else { None })
        .filter_map(|id| {
            let item = crate::sources::workshop_item(&mods.get(id)?.source)?;
            Some((id.clone(), item.to_string()))
        })
        .collect();
    Ok(Shared { kind: kind_of(paths, &p.name), list: p.to_text(), order: p.enabled(), maps: read_map_order(paths, &p.name)?, name: p.name, workshop })
}

/// Workshop items a shared profile needs that aren't installed (`mods`: the current scan).
pub fn import_needs(shared: &Shared, mods: &std::collections::BTreeMap<String, crate::store::Row>) -> Vec<crate::sources::Source> {
    // only what the sender has on (their list names every mod they have, most of them off)
    let on: std::collections::HashSet<String> = Profile::parse("", &shared.list).enabled().into_iter().collect();
    let mut items: Vec<&String> = shared
        .workshop
        .iter()
        .filter(|(id, item)| on.contains(*id) && !mods.contains_key(*id) && crate::sources::steam_workshop::valid_id(item))
        .map(|(_, item)| item)
        .collect();
    items.sort();
    items.dedup();
    items.into_iter().map(|i| crate::sources::Source::Workshop(i.clone())).collect()
}

/// Save a shared profile under a free name (`<name>`, `<name> 2`...), keep its kind, and
/// switch to it. Returns the profile.
pub fn import(paths: &Paths, shared: &Shared) -> Result<Profile> {
    let base = Some(crate::clean_file_name(&shared.name)).filter(|n| crate::safe_file_name(n)).unwrap_or_else(|| "Shared".to_string());
    let name = if exists(paths, &base) { (2..).map(|i| format!("{base} {i}")).find(|n| !exists(paths, n)).unwrap() } else { base };
    let p = Profile { order: shared.order.clone(), ..Profile::parse(&name, &shared.list) };
    if !shared.kind.is_empty() && crate::safe_file_name(&shared.kind) {
        set_kind(paths, &name, &shared.kind)?;
    }
    set_map_order(paths, &name, &shared.maps)?;
    let mut s = state(paths);
    s.imported.insert(name.clone());
    save_state(paths, &s)?;
    apply(paths, &p, crate::store::scan(paths).ok().as_ref())?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(paths: &Paths, p: &Profile) -> Result<()> {
        super::apply(paths, p, super::maps_scan(crate::store::scan(paths)).as_ref())
    }

    #[test]
    fn the_client_mod_hears_the_app_is_installed() {
        let (_t, paths) = temp_paths();
        let file = paths.zomboid.join("Lua/pzmm/app.txt");
        app_for_game(&paths, "1.2.7");
        assert!(!file.exists(), "no game run yet: nothing written");
        std::fs::create_dir_all(paths.zomboid.join("Lua")).unwrap();
        app_for_game(&paths, "1.2.7");
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("version=1.2.7\nproto=1\nrunning_at=") && text.contains("\nprofile=Default\n"), "{text}");
    }

    #[test]
    fn the_client_mod_gets_the_library_once_it_has_its_folder() {
        let (_t, paths) = temp_paths();
        let p = Profile::parse("Solo", "# --- Frameworks & libraries\n+A\n-B\n");
        let file = paths.zomboid.join("Lua/pzmm/library.txt");
        apply(&paths, &p).unwrap();
        assert!(!file.exists(), "no folder made for players without the mod");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        apply(&paths, &p).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# PZMM profile: Solo\n# --- Frameworks & libraries\n+A\n-B\n");
    }

    #[test]
    fn keep_marks_turns_favourites_on_and_hidden_off() {
        let mut p = Profile { order: ids(&["B", "H"]), ..Profile::parse("P", "+B
-F
+H
-FH
") };
        let installed = |id: &str| id != "Gone";
        let n = keep_marks(&mut p, &ids(&["F", "FH", "Gone", "B"]), &ids(&["H", "FH"]), installed);
        assert_eq!(n, (1, 1));
        assert_eq!(p.enabled(), ids(&["B", "F"]), "F joins the end of the load order; not-installed favourites stay out");
        assert_eq!(keep_marks(&mut p, &ids(&["F"]), &ids(&["H"]), installed), (0, 0), "nothing left to do");
    }

    #[test]
    fn share_round_trip() {
        let (_t, paths) = temp_paths();
        let friends = Profile { order: ids(&["C", "A"]), ..Profile::parse("Friends", "# --- Core\n+A\n-B\n+C\n") };
        apply(&paths, &friends).unwrap();
        set_map_order(&paths, "Friends", &ids(&["Town", "Muldraugh, KY"])).unwrap();
        let shared = export(&paths, &Default::default()).unwrap();
        assert_eq!((shared.list.as_str(), &shared.order, &shared.maps), ("# --- Core\n+A\n-B\n+C\n", &ids(&["C", "A"]), &ids(&["Town", "Muldraugh, KY"])));
        let back: Shared = serde_json::from_str(&serde_json::to_string(&shared).unwrap()).unwrap();
        let p = import(&paths, &back).unwrap();
        assert_eq!((p.name.as_str(), p.to_text().as_str(), p.enabled()), ("Friends 2", "# --- Core\n+A\n-B\n+C\n", ids(&["C", "A"])));
        assert_eq!(map_order(&paths, "Friends 2"), ids(&["Town", "Muldraugh, KY"]), "the map arrangement comes along");
        // someone else picked an imported profile's mods (ZombieBuddy still asks about its Java mods), even renamed
        assert!(picked_by_player(&paths, "Friends") && !picked_by_player(&paths, "Friends 2"));
        rename(&paths, "Friends 2", "Theirs").unwrap();
        assert!(!picked_by_player(&paths, "Theirs"));
        // a code made before maps were shared still reads
        let old: Shared = serde_json::from_str(r#"{"name":"Old","list":"+A\n"}"#).unwrap();
        assert!(old.maps.is_empty() && import(&paths, &old).is_ok());
        let needs = import_needs(
            &Shared { list: "+X\n+Y\n-Z\n".into(), workshop: [("X".into(), "123".into()), ("Y".into(), "+bad".into()), ("Z".into(), "456".into())].into(), ..back },
            &Default::default(),
        );
        assert_eq!(needs, [crate::sources::Source::Workshop("123".into())], "only what the sender has on, from a real item");
    }
    use crate::store::tests::temp_paths;

    #[test]
    fn launch_options_and_an_own_game_folder() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Server Our place", ids(&["A"]))).unwrap();
        assert_eq!(launch_args(&paths, "Server Our place").unwrap(), (String::new(), None));
        let folder = own_game_folder(&paths, "Server Our place");
        assert!(folder.to_string_lossy().contains("Server_Our_place-"), "{}", folder.display());
        assert!(own_game_folder(&paths, "Server_A").ends_with("game-folders/Server_A"));
        assert_ne!(own_game_folder(&paths, "Server A"), own_game_folder(&paths, "Server_A"));
        assert_ne!(own_game_folder(&paths, "Друзья"), own_game_folder(&paths, "Сервер"));
        set_launch(&paths, "server our place", &Launch { args: " -debug ".into(), cache_dir: Some(folder.clone()), kept: None }).unwrap();
        let (args, dir) = launch_args(&paths, "Server Our place").unwrap();
        assert_eq!((args, dir.as_ref()), (format!("-debug -cachedir={}", folder.display()), Some(&folder)));
        assert_eq!(pz::read_default(&folder.join("mods/default.txt")).mods, ["A"], "the game finds the profile's mods there");
        fs::create_dir_all(paths.mods().join("A")).unwrap();
        assert!(folder.join("mods/A").is_dir(), "and the mods themselves: its mods folder leads to Zomboid's");
        launch_args(&paths, "Server Our place").unwrap(); // again: the link stays
        // a folder an older PZMM made (just the list) gives way to the link
        let old = own_game_folder(&paths, "Older");
        fs::create_dir_all(old.join("mods")).unwrap();
        fs::write(old.join("mods/default.txt"), "old").unwrap();
        share_mods(&paths.mods(), &old.join("mods")).unwrap();
        assert!(old.join("mods/A").is_dir());
        // follows a rename (it lives in the profile's folder)
        rename(&paths, "Server Our place", "Ours").unwrap();
        assert_eq!(launch_of(&paths, "Ours").args, " -debug ");
        assert!(set_launch(&paths, "Ours", &Launch { args: String::new(), cache_dir: Some("relative".into()), kept: None }).is_err());
        // a folder with spaces goes by its short name, where the drive keeps them
        match set_launch(&paths, "Ours", &Launch { args: String::new(), cache_dir: Some(paths.data.join("with space")), kept: None }) {
            Ok(()) => assert!(!launch_of(&paths, "Ours").cache_dir.unwrap().to_string_lossy().contains(' ')),
            Err(e) => assert!(e.to_string().contains("without spaces") && path_form(&paths.data, false).is_none(), "{e}"),
        }
        assert!(set_launch(&paths, "Nobody", &Launch::default()).is_err());
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn profile_names_ignore_case_like_their_folders() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "+A
")).unwrap();
        assert_eq!(load(&paths, "default").unwrap().name, "Default");
        assert!(delete(&paths, "DEFAULT").is_err(), "the active profile can't be deleted under another casing");
    }

    #[test]
    fn unreadable_profile_is_not_replaced() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "# --- Maps\n+A\n-B\n")).unwrap();
        // Notepad re-saved it as ANSI: the separator is no longer valid UTF-8
        let file = dir(&paths).join("Default").join("modlist.txt");
        fs::write(&file, b"# --- V\xe9g\n+A\n-B\n").unwrap();
        let (p, _) = sync(&paths).unwrap();
        assert_eq!(p.lines.len(), 3, "separator and disabled entry kept: {:?}", p.lines);
        assert!(matches!(&p.lines[2], Line::Mod { id, on: false } if id == "B"));
    }

    #[test]
    fn map_order_is_kept_per_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "+A\n")).unwrap();
        assert!(map_order(&paths, "Default").is_empty(), "never arranged");
        set_map_order(&paths, "default", &ids(&["Town", "Muldraugh, KY"])).unwrap();
        // saving the profile (a toggle) leaves it alone; other profiles have their own
        apply(&paths, &Profile::parse("Default", "+A\n+B\n")).unwrap();
        assert_eq!(map_order(&paths, "Default"), ids(&["Town", "Muldraugh, KY"]));
        apply(&paths, &Profile::parse("Other", "+A\n")).unwrap();
        assert!(map_order(&paths, "Other").is_empty());
        assert!(set_map_order(&paths, "../x", &[]).is_err());
    }

    #[test]
    fn applying_writes_the_map_order_to_the_game() {
        let (_t, paths) = temp_paths();
        for (id, map) in [("A", "Town A"), ("B", "Town B"), ("C", "Off")] {
            let d = paths.mods().join(id).join("42");
            fs::create_dir_all(d.join("media/maps").join(map)).unwrap();
            fs::write(d.join("mod.info"), format!("id={id}\n")).unwrap();
        }
        apply(&paths, &Profile::parse("Default", "+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).maps, "\n    map = Town A,\n    map = Town B,\n", "never arranged: the enabled maps in load order");
        set_map_order(&paths, "Default", &ids(&["Town B", "Off", "Town A", "Muldraugh, KY"])).unwrap();
        apply(&paths, &Profile::parse("Default", "+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).maps, "\n    map = Town B,\n    map = Town A,\n", "enabled maps only, Muldraugh left to the game");
        // a map that leaves the arrangement keeps its place in the saved order
        set_map_order(&paths, "Default", &ids(&["Town A", "Town B", "Muldraugh, KY"])).unwrap();
        assert_eq!(map_order(&paths, "Default"), ids(&["Town A", "Town B", "Off", "Muldraugh, KY"]));
    }

    /// Mods `ids`, each adding one map folder (B42 layout).
    fn map_mods(paths: &Paths, mods: &[(&str, &str)]) {
        for (id, map) in mods {
            let d = paths.mods().join(id).join("42");
            fs::create_dir_all(d.join("media/maps").join(map)).unwrap();
            fs::write(d.join("mod.info"), format!("id={id}\n")).unwrap();
        }
    }

    #[test]
    fn opening_keeps_a_hand_arranged_block() {
        let (_t, paths) = temp_paths();
        map_mods(&paths, &[("A", "Town A"), ("B", "Town B")]);
        // arranged by hand (or by the game), never in PZMM: no maporder.txt
        let game = "VERSION = 1,\n\nmods\n{\n    mod = \\A,\n    mod = \\B,\n}\n\nmaps\n{\n    map = Town B,\n    map = Town A,\n    map = Muldraugh, KY,\n}\n";
        fs::create_dir_all(paths.default_txt().parent().unwrap()).unwrap();
        fs::write(paths.default_txt(), game).unwrap();
        // opening: the view syncs the profile, the Maps tab asks where the order starts
        let (p, _) = sync(&paths).unwrap();
        let mods = crate::store::scan(&paths).unwrap();
        let line = pz::server::map_order(&map_start(&paths, &p, &mods, 42), &pz::server::mod_maps(&p.enabled(), &mods, 42));
        assert_eq!(line, ids(&["Town B", "Town A", "Muldraugh, KY"]), "the block's order, not A-Z");
        assert_eq!(fs::read_to_string(paths.default_txt()).unwrap(), game, "default.txt unchanged byte for byte");
        assert!(map_order(&paths, "Default").is_empty(), "nothing saved");
        // and applying the same list (a toggle elsewhere) keeps the arrangement
        apply(&paths, &p).unwrap();
        assert_eq!(fs::read_to_string(paths.default_txt()).unwrap(), game);
    }

    #[test]
    fn switching_to_a_never_arranged_profile_drops_the_other_maps() {
        let (_t, paths) = temp_paths();
        map_mods(&paths, &[("A", "Town A"), ("B", "Town B"), ("C", "Other"), ("D", "Town D")]);
        apply(&paths, &Profile::parse("Big", "+A\n+B\n+C\n")).unwrap();
        set_map_order(&paths, "Big", &ids(&["Other", "Town B", "Town A", "Muldraugh, KY"])).unwrap();
        apply(&paths, &Profile::parse("Big", "+A\n+B\n+C\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Other", "Town B", "Town A"]));
        // Small was never arranged: the block's order where it's still valid, then its own new maps
        apply(&paths, &Profile::parse("Small", "+D\n+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Town B", "Town A", "Town D"]));
        assert!(map_order(&paths, "Small").is_empty(), "still never arranged");
        // a folder no installed mod provides stays where it was
        let d = fs::read_to_string(paths.default_txt()).unwrap().replace("    map = Town A,", "    map = Unknown Map,\n    map = Town A,");
        fs::write(paths.default_txt(), d).unwrap();
        apply(&paths, &Profile::parse("Small", "+D\n+A\n+B\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Town B", "Unknown Map", "Town A", "Town D"]));
        // ...and in an arranged profile too (not moved to the end or before the game's maps)
        apply(&paths, &Profile::parse("Big", "+A\n+B\n+C\n")).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).map_folders(), ids(&["Other", "Town B", "Unknown Map", "Town A"]));
    }

    #[test]
    fn snapshots_keep_the_last_ones_and_restore() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile { order: ids(&["B", "A"]), ..Profile::parse("Default", "# --- S\n+A\n+B\n-C\n") }).unwrap();
        set_map_order(&paths, "Default", &ids(&["Town", "Muldraugh, KY"])).unwrap();
        snapshot(&paths, "default", "autosort").unwrap();
        // the change it guards against
        apply(&paths, &Profile::parse("Default", "+C\n")).unwrap();
        set_map_order(&paths, "Default", &ids(&["Other", "Muldraugh, KY"])).unwrap();
        let list = snapshots(&paths, "Default");
        assert_eq!((list.len(), list[0].label.as_str(), list[0].enabled, list[0].mods), (1, "autosort", 2, 3));
        let p = restore(&paths, "Default", &list[0].id).unwrap();
        assert_eq!((p.to_text().as_str(), p.enabled()), ("# --- S\n+A\n+B\n-C\n", ids(&["B", "A"])));
        assert_eq!(load(&paths, "Default").unwrap(), p, "saved");
        assert_eq!(map_order(&paths, "Default"), ids(&["Town", "Muldraugh, KY"]));
        // the restore kept what it replaced, so it can be undone the same way
        let list = snapshots(&paths, "Default");
        assert_eq!((list[0].label.as_str(), list[0].enabled), ("restore", 1));
        restore(&paths, "Default", &list[0].id).unwrap();
        assert_eq!(load(&paths, "Default").unwrap().enabled(), ids(&["C"]));
        assert_eq!(restore(&paths, "Default", "123").unwrap_err().to_string(), "no snapshot 123 of \"Default\"");
        // only the newest are kept
        for _ in 0..12 {
            snapshot(&paths, "Default", "update").unwrap();
        }
        assert_eq!(snapshots(&paths, "Default").len(), SNAPSHOTS);
        // an unreadable profile isn't snapshotted (or restored over)
        let order = dir(&paths).join("Default/loadorder.txt");
        fs::remove_file(&order).unwrap();
        fs::create_dir(&order).unwrap();
        assert!(snapshot(&paths, "Default", "update").is_err());
        assert!(restore(&paths, "Default", &snapshots(&paths, "Default")[0].id).is_err());
        assert!(restore(&paths, "Default", "../x").is_err());
    }

    #[test]
    fn rename_and_duplicate_keep_everything() {
        let (_t, paths) = temp_paths();
        set_kind(&paths, "Solo", "viewpoint").unwrap();
        apply(&paths, &Profile::parse("Solo", "# --- S\n+A\n")).unwrap();
        set_map_order(&paths, "Solo", &ids(&["Town", "Muldraugh, KY"])).unwrap();
        apply(&paths, &Profile::parse("Other", "+B\n")).unwrap();
        // names clash whatever their case
        assert!(rename(&paths, "Solo", "OTHER").is_err());
        assert!(duplicate(&paths, "Solo", "other").is_err());
        assert!(rename(&paths, "Solo", "../x").is_err() && rename(&paths, "Gone", "New").is_err());
        assert_eq!(list(&paths), ids(&["Other", "Solo"]), "nothing changed");
        // a change of case only
        rename(&paths, "solo", "SOLO").unwrap();
        assert_eq!(list(&paths), ids(&["Other", "SOLO"]));
        assert_eq!((kind_of(&paths, "SOLO"), last_of_kind(&paths, "viewpoint")), ("viewpoint".to_string(), Some("SOLO".to_string())));
        assert_eq!(load(&paths, "SOLO").unwrap().to_text(), "# --- S\n+A\n");
        assert_eq!(map_order(&paths, "SOLO"), ids(&["Town", "Muldraugh, KY"]));
        // the active profile stays active under its new name
        rename(&paths, "Other", "Main").unwrap();
        assert_eq!((active_name(&paths), sync(&paths).unwrap().0.enabled()), ("Main".to_string(), ids(&["B"])));
        duplicate(&paths, "SOLO", "Copy").unwrap();
        assert_eq!(list(&paths), ids(&["Copy", "Main", "SOLO"]));
        assert_eq!((load(&paths, "Copy").unwrap().to_text(), kind_of(&paths, "Copy")), ("# --- S\n+A\n".to_string(), "viewpoint".to_string()));
        assert_eq!(map_order(&paths, "Copy"), map_order(&paths, "SOLO"));
        assert_eq!(active_name(&paths), "Main", "duplicating doesn't switch");
        set_launch(&paths, "SOLO", &Launch { args: "-debug".into(), cache_dir: None, kept: None }).unwrap();
        duplicate(&paths, "SOLO", "Copy 2").unwrap();
        assert_eq!(launch_of(&paths, "Copy 2"), Launch { args: "-debug".into(), cache_dir: None, kept: None }, "launch options come along");
        // its own game folder: the copy gets one of its own
        set_launch(&paths, "SOLO", &Launch { args: String::new(), cache_dir: Some(own_game_folder(&paths, "SOLO")), kept: None }).unwrap();
        duplicate(&paths, "SOLO", "Copy 3").unwrap();
        let own = launch_of(&paths, "Copy 3").cache_dir.unwrap();
        let long = |p: PathBuf| path_form(&p, true).unwrap_or(p); // the temp dir may itself be an 8.3 path (CI)
        assert_eq!(long(own), long(own_game_folder(&paths, "Copy 3")));
    }

    #[test]
    fn joining_a_server_resolves_makes_and_applies_its_profile() {
        // the Join flow on fixture data: resolve the server's mod IDs, make its profile from the
        // installed ones (requirements first), apply it; then the same mods as a server's lines
        let (_t, paths) = temp_paths();
        map_mods(&paths, &[("Lib", "Town")]);
        fs::create_dir_all(paths.mods().join("Lib/media/maps/Old Town")).unwrap();
        let d = paths.mods().join("Patch/42");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("mod.info"), "id=Patch\nrequire=\\Lib\n").unwrap();
        let mut installed = crate::store::scan(&paths).unwrap();
        let listed = ids(&["Patch", "Lib", "Ghost"]);
        let r = pz::launcher::resolve(&paths, &listed, 42, &installed, &mut |_| Ok(None), &[]);
        assert_eq!(r.iter().map(|r| r.status.as_str()).collect::<Vec<_>>(), ["installed", "installed", "unknown"]);
        let (order, missing) = pz::requirements_first(&listed, |id| Some(installed.get(id)?.requires.clone()));
        assert_eq!((order.clone(), missing), (ids(&["Lib", "Patch"]), ids(&["Ghost"])));
        let p = for_server(&paths, "Fun", "198.51.100.1:16261", order, &ids(&["Town", "Muldraugh, KY"])).unwrap();
        super::apply(&paths, &p, Some(&installed)).unwrap();
        let game = pz::read_default(&paths.default_txt());
        assert_eq!((game.mods, game.maps.as_str()), (ids(&["Lib", "Patch"]), "\n    map = Town,\n"));
        assert_eq!(active_name(&paths), "Server Fun");

        // hosting the same mods: B41 reads the root map and takes bare IDs, B42 takes `\ID`
        for (id, item) in [("Lib", "111"), ("Patch", "222")] {
            installed.get_mut(id).unwrap().source = format!("steam:{item}");
        }
        let line = |build: u8| {
            let (l, notes) = pz::server::lines_for(&p.enabled(), &installed, build, &[]);
            assert!(notes.is_empty(), "{notes:?}");
            pz::server::line_values(&l.mods, &l.workshop, &l.maps, build == 41)
        };
        let v = |s: &[(&'static str, &str)]| s.iter().map(|(k, v)| (*k, v.to_string())).collect::<Vec<_>>();
        assert_eq!(line(41), v(&[("Mods", "Lib;Patch"), ("WorkshopItems", "111;222"), ("Map", "Old Town;Muldraugh, KY")]));
        assert_eq!(line(42), v(&[("Mods", "\\Lib;\\Patch"), ("WorkshopItems", "111;222"), ("Map", "Town;Muldraugh, KY")]));
    }

    #[test]
    #[cfg(windows)]
    fn junctions_quote_paths_for_cmd() {
        let t = tempfile::tempdir().unwrap();
        let target = t.path().join("mods & more (^1)");
        fs::create_dir_all(&target).unwrap();
        link_dir(&target, &t.path().join("link & co")).unwrap();
        assert!(t.path().join("link & co").read_dir().is_ok());
        let e = share_mods(&target, &t.path().join("100%")).unwrap_err();
        assert!(format!("{e:#}").starts_with("errors.linkPercent"), "keyed outermost: {e:#}");
    }

    #[test]
    fn server_profiles_are_kept_per_server() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "+A\n")).unwrap();
        let a = for_server(&paths, "Fun", "198.51.100.1:16261", ids(&["A"]), &ids(&["Town", "Muldraugh, KY"])).unwrap();
        assert_eq!((a.name.as_str(), kind_of(&paths, "Server Fun")), ("Server Fun", SERVER_KIND.to_string()));
        assert_eq!(map_order(&paths, "Server Fun"), ids(&["Town", "Muldraugh, KY"]));
        assert!(is_for_server(&paths, "Server Fun", "198.51.100.1:16261") && !is_for_server(&paths, "Default", "198.51.100.1:16261"), "joins as is aren't the server's set");
        // another server of the same name gets a profile of its own
        assert_eq!(for_server(&paths, "Fun", "198.51.100.2:16261", ids(&["B"]), &[]).unwrap().name, "Server Fun 2");
        // renamed, it's still that server's
        rename(&paths, "Server Fun", "Mine").unwrap();
        assert_eq!(for_server(&paths, "Fun", "198.51.100.1:16261", ids(&["A", "B"]), &[]).unwrap().name, "Mine");
        assert_eq!(server_of(&paths, "mine").as_deref(), Some("198.51.100.1:16261"));
        assert_eq!(snapshots(&paths, "Mine").len(), 1, "its list changed once");
        // the player turns B off; joining again with the same list keeps it off, and snapshots nothing
        let mut p = load(&paths, "Mine").unwrap();
        p.set("B", false);
        save(&paths, &p).unwrap();
        assert_eq!(for_server(&paths, "Fun", "198.51.100.1:16261", ids(&["A", "B"]), &[]).unwrap().enabled(), ids(&["A"]));
        assert_eq!(snapshots(&paths, "Mine").len(), 1);
        // the player's own profile with a server's name is never taken over
        save(&paths, &Profile::parse("Server Solo", "+X\n")).unwrap();
        assert_eq!(for_server(&paths, "Solo", "198.51.100.3:16261", ids(&["A"]), &[]).unwrap().name, "Server Solo 2");
        assert_eq!(load(&paths, "Server Solo").unwrap().enabled(), ids(&["X"]));
        // a copy is a plain profile
        duplicate(&paths, "Mine", "Copy").unwrap();
        assert_eq!((kind_of(&paths, "Copy"), server_of(&paths, "Copy")), (DEFAULT_KIND.to_string(), None));
    }

    #[test]
    fn unused_server_profiles_are_the_forgotten_ones() {
        let (_t, paths) = temp_paths();
        fs::create_dir_all(&paths.data).unwrap();
        fs::write(paths.data.join("servers.json"), r#"[{"name":"Ours","game_port":16300}]"#).unwrap();
        let server = |addr: &str| pz::launcher::Server { addr: addr.into(), ..Default::default() };
        pz::launcher::record_join(&paths, &server("198.51.100.1:16261"), None).unwrap();
        pz::launcher::set_favourite(&paths, &server("198.51.100.2:16261"), true).unwrap();
        for (name, key) in [("Joined", "198.51.100.1:16261"), ("Fave", "198.51.100.2:16261"), ("Gone", "198.51.100.3:16261"), ("Ours", "ini:Ours"), ("Playing", "198.51.100.4:16261")] {
            for_server(&paths, name, key, ids(&["A"]), &[]).unwrap();
        }
        apply(&paths, &load(&paths, "Server Playing").unwrap()).unwrap();
        save(&paths, &Profile::parse("Mine", "+A\n")).unwrap();
        assert_eq!(unused_server_profiles(&paths), ["Server Gone"], "history, favourites, hosted, active and plain profiles stay");
    }

    #[test]
    fn a_server_hosted_here_has_one_profile() {
        let (_t, paths) = temp_paths();
        fs::create_dir_all(&paths.data).unwrap();
        fs::write(paths.data.join("servers.json"), r#"[{"name":"Friends","game_port":16261}]"#).unwrap();
        // "Use as profile", then joining it at its local address: the same profile
        assert_eq!(for_server(&paths, "Friends", "ini:Friends", ids(&["A"]), &[]).unwrap().name, "Server Friends");
        for addr in ["127.0.0.1:16261", "localhost:16261", "[::1]:16261"] {
            assert_eq!(for_server(&paths, "My Friends Server", addr, ids(&["A"]), &[]).unwrap().name, "Server Friends", "{addr}");
        }
        if let Some(ip) = lan_ip() {
            assert_eq!(for_server(&paths, "Friends", &format!("{ip}:16261"), ids(&["A"]), &[]).unwrap().name, "Server Friends", "its LAN address");
        }
        assert_eq!(server_of(&paths, "Server Friends").as_deref(), Some("ini:Friends"));
        // another port, or another machine, is another server
        assert_eq!(for_server(&paths, "Other", "127.0.0.1:16300", ids(&["A"]), &[]).unwrap().name, "Server Other");
        assert_eq!(for_server(&paths, "Friends", "198.51.100.1:16261", ids(&["A"]), &[]).unwrap().name, "Server Friends 2");
    }

    #[test]
    fn text_round_trip() {
        let text = "# --- Maps\n+A\n-B\n+C\n";
        assert_eq!(Profile::parse("x", text).to_text(), text);
    }

    #[test]
    fn adopt_takes_the_game_order_and_keeps_the_library() {
        let mut p = Profile::parse("x", "# --- Maps\n+A\n-B\n+C\n");
        assert!(!p.adopt(&ids(&["A", "C"])));
        // in-game: C moved before A, B enabled, D added, duplicates ignored
        assert!(p.adopt(&ids(&["C", "A", "B", "D", "A"])));
        assert_eq!(p.enabled(), ids(&["C", "A", "B", "D"]));
        assert_eq!(p.to_text(), "# --- Maps\n+A\n+B\n+C\n# --- \n+D\n", "library order kept; D after a break");
        assert!(p.adopt(&ids(&["B"])));
        assert_eq!(p.to_text(), "# --- Maps\n-A\n+B\n-C\n# --- \n-D\n");
    }

    #[test]
    fn upgrade_without_a_load_order_keeps_the_library_order() {
        let (_t, paths) = temp_paths();
        let d = dir(&paths).join("Default");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("modlist.txt"), "# --- Maps\n+B\n-C\n+A\n").unwrap();
        let p = load(&paths, "Default").unwrap();
        assert_eq!(p.enabled(), ids(&["B", "A"]));
        save(&paths, &p).unwrap();
        assert_eq!(fs::read_to_string(d.join("loadorder.txt")).unwrap(), "B\nA\n");
    }

    #[test]
    fn enabling_appends_to_the_load_order_and_disabling_drops() {
        let mut p = Profile { order: ids(&["A", "B"]), ..Profile::parse("x", "-C\n+B\n+A\n") };
        p.set("C", true);
        p.set("D", true);
        assert_eq!(p.enabled(), ids(&["A", "B", "C", "D"]), "newly enabled mods load last, whatever their library place");
        p.set("B", false);
        p.forget("D");
        assert_eq!(p.enabled(), ids(&["A", "C"]));
    }

    #[test]
    fn sync_adopts_the_game_order() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "# --- Mine\n+A\n+B\n-C\n")).unwrap();
        pz::write_default(&paths.default_txt(), &ids(&["C", "B", "A"]), None, None).unwrap();
        let (p, adopted) = sync(&paths).unwrap();
        assert!(adopted);
        assert_eq!(p.enabled(), ids(&["C", "B", "A"]));
        assert_eq!(p.to_text(), "# --- Mine\n+A\n+B\n+C\n");
        assert_eq!(load(&paths, "Default").unwrap().enabled(), ids(&["C", "B", "A"]), "saved");
        assert!(snapshots(&paths, "Default").is_empty(), "nothing turned off: no snapshot");
    }

    #[test]
    fn sync_takes_sections_made_in_game_once() {
        let (_t, paths) = temp_paths();
        let lua = paths.zomboid.join("Lua/pzmm");
        fs::create_dir_all(&lua).unwrap();
        fs::write(lua.join("library.txt"), "# PZMM profile: In game\n# --- Maps\n+B\n-C\n# --- Gameplay\n+A\n").unwrap();
        pz::write_default(&paths.default_txt(), &ids(&["A", "B", "D"]), None, None).unwrap();
        let (p, _) = sync(&paths).unwrap();
        assert_eq!(p.to_text(), "# --- Maps\n+B\n-C\n# --- Gameplay\n+A\n# --- \n+D\n");
        assert!(fs::read_to_string(lua.join("library.txt")).unwrap().starts_with("# PZMM profile: Default\n"), "the game's copy is the profile's now");
        // a profile with sections of its own keeps them
        let mut q = Profile::parse("Default", "# --- Mine\n+A\n");
        fs::write(lua.join("library.txt"), "# PZMM profile: In game\n# --- Maps\n+A\n").unwrap();
        assert!(!game_sections(&paths, &mut q));
    }

    #[test]
    fn sync_takes_the_games_map_order_once_arranged() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        pz::write_default(&paths.default_txt(), &ids(&["A"]), None, Some(&ids(&["Town B", "Town A"]))).unwrap();
        // the game wrote it after PZMM's last save
        // when PZMM last wrote its load order and map order
        let saved = |ago: u64| {
            for f in ["loadorder.txt", "maporder.txt"] {
                if let Ok(ours) = fs::File::options().write(true).open(dir(&paths).join("Default").join(f)) {
                    ours.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(ago)).unwrap();
                }
            }
        };
        saved(10);
        sync(&paths).unwrap();
        assert!(map_order(&paths, "Default").is_empty(), "never arranged: nothing saved");
        set_map_order(&paths, "Default", &ids(&["Town A", "Town B"])).unwrap();
        // PZMM saved after default.txt was written
        saved(0);
        sync(&paths).unwrap();
        assert_eq!(map_order(&paths, "Default")[..2], ids(&["Town A", "Town B"]), "default.txt as PZMM left it (written with its save): not the game's arranging");
        saved(10);
        sync(&paths).unwrap();
        assert_eq!(map_order(&paths, "Default")[..2], ids(&["Town B", "Town A"]));
    }

    #[test]
    fn renaming_the_active_profile_renames_the_games_copy() {
        let (_t, paths) = temp_paths();
        fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        rename(&paths, "Default", "Mine").unwrap();
        assert!(fs::read_to_string(paths.zomboid.join("Lua/pzmm/library.txt")).unwrap().starts_with("# PZMM profile: Mine\n"));
    }

    #[test]
    fn unreadable_default_txt_keeps_the_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A", "B"]))).unwrap();
        // the game has it locked / it was deleted / it isn't UTF-8
        fs::write(paths.default_txt(), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), ids(&["A", "B"]));
        fs::remove_file(paths.default_txt()).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), ids(&["A", "B"]));
    }

    #[test]
    fn a_game_reset_keeps_the_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A", "B"]))).unwrap();
        let marker = paths.zomboid.join("mods/reset-mods-42_00.txt");
        let touch = |f: &std::path::Path, secs: u64| {
            fs::File::options().write(true).open(f).unwrap().set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(secs)).unwrap();
        };
        // the game's first start after an update: it writes the marker and empties its list
        fs::write(&marker, "").unwrap();
        pz::write_default(&paths.default_txt(), &[], None, None).unwrap();
        touch(&marker, 60);
        touch(&paths.default_txt(), 61);
        let p = sync(&paths).unwrap().0;
        assert_eq!(p.enabled(), ids(&["A", "B"]));
        assert!(game_differs(&paths, &p), "so Play puts it back");
        assert!(snapshots(&paths, "Default").is_empty(), "nothing was taken from the profile");
        // the list emptied well after the marker: the player did it in game, so it's taken (kept in a snapshot first)
        touch(&paths.default_txt(), 600);
        let p = sync(&paths).unwrap().0;
        assert_eq!(p.enabled(), Vec::<String>::new());
        assert!(!game_differs(&paths, &p));
        assert_eq!(snapshots(&paths, "Default").iter().map(|s| (s.label.as_str(), s.enabled)).collect::<Vec<_>>(), [("adopt", 2)]);
        fs::remove_file(paths.default_txt()).unwrap();
        assert!(!game_differs(&paths, &p), "no list and no mods: nothing to put back");
        assert!(game_differs(&paths, &Profile::from_ids("Default", ids(&["A"]))));
    }

    /// A game jar whose `GameWindow` names release 42_00.
    fn fake_jar(jar: &std::path::Path) {
        use std::io::Write;
        fs::create_dir_all(jar.parent().unwrap()).unwrap();
        let mut z = zip::ZipWriter::new(fs::File::create(jar).unwrap());
        z.start_file("zombie/GameWindow.class", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(b"\xca\xfe\xba\xbe\x01\x00\x0542_00\x01\x00\x03abc\x01\x00\x0442.1").unwrap();
        z.finish().unwrap();
    }

    #[test]
    fn pzmms_own_marker_doesnt_hide_the_players_disable_all() {
        let (_t, paths) = temp_paths();
        fake_jar(&paths.zomboid.with_file_name("game").join("projectzomboid.jar"));
        fs::create_dir_all(paths.zomboid.join("mods")).unwrap();
        apply(&paths, &Profile::from_ids("Default", ids(&["A", "B"]))).unwrap();
        let marker = paths.zomboid.join("mods/reset-mods-42_00.txt");
        let modified = |f: PathBuf| fs::metadata(f).unwrap().modified().unwrap();
        assert!(modified(marker) <= modified(dir(&paths).join("Default/loadorder.txt")), "the marker comes before the save");
        // the player turns everything off in game
        pz::write_default(&paths.default_txt(), &[], None, None).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), Vec::<String>::new());
    }

    #[test]
    fn reads_the_reset_release_from_the_game() {
        let t = tempfile::tempdir().unwrap();
        let jar = t.path().join("projectzomboid.jar");
        fake_jar(&jar);
        assert_eq!(reset_release(&jar).as_deref(), Some("42_00"));
        assert_eq!(reset_release(&t.path().join("missing.jar")), None);
    }

    #[test]
    fn duplicates_and_empty_ids_are_dropped() {
        assert_eq!(Profile::parse("x", "+A\n+\n-A\n+B\n").to_text(), "+A\n+B\n");
        assert_eq!(Profile::from_ids("x", ids(&["A", "", "A", "B"])).enabled(), ids(&["A", "B"]));
        let mut p = Profile::parse("x", "+A\n");
        p.adopt(&ids(&["A", "", "A"]));
        assert_eq!(p.to_text(), "+A\n");
    }

    #[test]
    fn viewpoint_moves_to_build_42_and_back() {
        let (_t, paths) = temp_paths();
        set_b41(&paths, Some(true)).unwrap();
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        set_kind(&paths, "3D", "viewpoint").unwrap();
        apply(&paths, &Profile::from_ids("3D", ids(&["Viewpoint"]))).unwrap();
        assert_eq!((build(&paths), pz::read_default(&paths.default_txt()).b41), (Some(42), false));
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        assert_eq!((build(&paths), pz::read_default(&paths.default_txt()).b41), (Some(41), true), "the B41 profile is B41 again");
        // a build picked in Settings while on Viewpoint stays
        apply(&paths, &Profile::from_ids("3D", ids(&["Viewpoint"]))).unwrap();
        set_b41(&paths, None).unwrap();
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        assert_eq!(build(&paths), None);
    }

    #[test]
    fn kinds_remember_their_last_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        set_kind(&paths, "3D", "viewpoint").unwrap();
        apply(&paths, &Profile::from_ids("3D", ids(&["Viewpoint"]))).unwrap();
        assert_eq!(kind_of(&paths, "3D"), "viewpoint");
        assert_eq!(kind_of(&paths, "Default"), DEFAULT_KIND);
        assert_eq!(last_of_kind(&paths, DEFAULT_KIND).as_deref(), Some("Default"));
        assert_eq!(last_of_kind(&paths, "viewpoint").as_deref(), Some("3D"));
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        delete(&paths, "3D").unwrap();
        assert_eq!(last_of_kind(&paths, "viewpoint"), None);
    }

    #[test]
    fn importing_again_keeps_the_library() {
        let (_t, paths) = temp_paths();
        save(&paths, &Profile::parse("Server x", "# --- Maps
+A
-B
")).unwrap();
        let p = with_list(&paths, "Server x", ids(&["C", "A"])).unwrap();
        assert_eq!((p.to_text().as_str(), p.enabled()), ("# --- Maps
+A
-B
# --- 
+C
", ids(&["C", "A"])));
        assert_eq!(with_list(&paths, "New", ids(&["A"])).unwrap(), Profile::from_ids("New", ids(&["A"])));
        assert_eq!(snapshots(&paths, "Server x").iter().map(|s| (s.label.as_str(), s.enabled)).collect::<Vec<_>>(), [("import", 1)], "kept before the list was replaced");
        save(&paths, &p).unwrap();
        with_list(&paths, "Server x", ids(&["C", "A"])).unwrap();
        assert_eq!(snapshots(&paths, "Server x").len(), 1, "the same list again: nothing to keep");
    }

    #[test]
    fn a_saves_profile_plays_where_the_save_is() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Mine", ids(&["A"]))).unwrap();
        let folder = own_game_folder(&paths, "Mine");
        set_launch(&paths, "Mine", &Launch { args: "-debug".into(), cache_dir: Some(folder.clone()), kept: None }).unwrap();
        let p = for_save(&paths, "Mine", "Save World", ids(&["Viewpoint", "B"]), &ids(&["Town", "Muldraugh, KY"])).unwrap();
        assert_eq!(p.enabled(), ids(&["Viewpoint", "B"]));
        assert_eq!(launch_of(&paths, "Save World"), Launch { args: "-debug".into(), cache_dir: Some(folder), kept: None });
        assert_eq!((map_order(&paths, "Save World"), kind_of(&paths, "Save World")), (ids(&["Town", "Muldraugh, KY"]), "viewpoint".to_string()));
        // again from a profile in the main folder: its own arguments stay, the folder follows the save
        set_launch(&paths, "Save World", &Launch { args: "-nosteam".into(), ..launch_of(&paths, "Save World") }).unwrap();
        for_save(&paths, "Default", "Save World", ids(&["B"]), &[]).unwrap();
        assert_eq!(launch_of(&paths, "Save World"), Launch { args: "-nosteam".into(), cache_dir: None, kept: None });
    }

    #[test]
    fn b41_format_survives_an_empty_list() {
        let (_t, paths) = temp_paths();
        pz::write_default(&paths.default_txt(), &ids(&["A"]), Some(true), None).unwrap();
        apply(&paths, &Profile::from_ids("Default", ids(&[]))).unwrap();
        assert_eq!(effective_build(&paths), 41);
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        assert!(pz::read_default(&paths.default_txt()).b41);
    }

    #[test]
    fn sync_and_apply() {
        let (_t, paths) = temp_paths();
        pz::write_default(&paths.default_txt(), &ids(&["A", "B"]), None, None).unwrap();
        let (mut p, _) = sync(&paths).unwrap();
        assert_eq!((p.name.as_str(), p.enabled()), ("Default", ids(&["A", "B"])));

        p.set("A", false);
        p.set("C", true);
        apply(&paths, &p).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).mods, ids(&["B", "C"]));

        let solo = Profile { name: "Solo".into(), lines: vec![Line::Mod { id: "A".into(), on: true }], order: ids(&["A"]) };
        apply(&paths, &solo).unwrap();
        assert_eq!(sync(&paths).unwrap().0, solo);
        assert_eq!(list(&paths), ids(&["Default", "Solo"]));
        assert!(delete(&paths, "Solo").is_err());
        assert!(load(&paths, "../x").is_err());
    }
}
