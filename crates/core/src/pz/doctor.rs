//! Health check for the active profile and server configs ("doctor"): problems the
//! game reports badly or not at all (it skips a mod silently, or kicks players later).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::{Paths, server};
use crate::store::Row;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// The game won't load something, or players can't join.
    Error,
    /// Likely to misbehave.
    Warning,
    /// Worth knowing; nothing wrong (the Java mod scan's lesser findings).
    Note,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Finding {
    pub level: Level,
    /// The mod or file it's about.
    pub subject: String,
    /// In English (the CLI, diagnostics).
    pub text: String,
    /// The app's wording: doctor.<key> in its locale files, with the values for its {placeholders}.
    pub key: &'static str,
    pub vars: BTreeMap<&'static str, String>,
    /// What would fix it, for a button beside it.
    pub action: Option<Action>,
    /// The player chose to hide it (see [`hide`]); it shows under Ignored, uncounted.
    pub hidden: bool,
}

/// A fix the UI can offer: install a missing mod, disable a clashing one, or stop reporting a
/// scan's findings about a file (by its SHA-256, see `javamods::dismiss`).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Install(String),
    Disable(String),
    Dismiss(String),
}

/// A finding worded by the English template (with {name} placeholders) and keyed for the app;
/// scripts/i18n-check.ts checks the template matches doctor.<key> in en.json.
fn finding(level: Level, subject: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    Finding { level, subject: subject.into(), text: crate::fill(template, vars), key, vars: vars.iter().cloned().collect(), action: None, hidden: false }
}

// --- findings the player hid ("It's fine" / Hide): back when the mod changes ---

/// A hidden finding: which one, and the mod.info it was about (its SHA-256), so an update brings it back.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Hidden {
    key: String,
    subject: String,
    vars: BTreeMap<String, String>,
    stamp: String,
}

fn hidden_file(paths: &Paths) -> std::path::PathBuf {
    paths.data.join("finding-dismissals.json")
}

/// The mod's mod.info as it is now (SHA-256), or "" when the subject isn't a mod.
fn stamp(mods: &BTreeMap<String, Row>, subject: &str) -> String {
    mods.get(subject).and_then(|m| std::fs::read(&m.info).ok()).map(|b| super::javamods::sha256(&b)).unwrap_or_default()
}

fn same(h: &Hidden, f: &Finding) -> bool {
    h.key == f.key && h.subject == f.subject && h.vars.len() == f.vars.len() && f.vars.iter().all(|(k, v)| h.vars.get(*k) == Some(v))
}

/// Hide the finding `key` about `subject` with `vars` (show it again with `on` false). Errors are
/// never marked hidden (see [`mark_hidden`]), so hiding one does nothing.
pub fn hide(paths: &Paths, mods: &BTreeMap<String, Row>, key: &str, subject: &str, vars: BTreeMap<String, String>, on: bool) -> anyhow::Result<()> {
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut all: Vec<Hidden> = crate::read_json(&hidden_file(paths))?;
    all.retain(|h| !(h.key == key && h.subject == subject && h.vars == vars));
    if on {
        all.push(Hidden { key: key.into(), subject: subject.into(), vars, stamp: stamp(mods, subject) });
    }
    std::fs::create_dir_all(&paths.data)?;
    Ok(crate::write_atomic(&hidden_file(paths), serde_json::to_vec(&all)?)?)
}

/// Mark the findings the player hid, while their mod is unchanged.
pub fn mark_hidden(paths: &Paths, mods: &BTreeMap<String, Row>, found: &mut [Finding]) {
    let all: Vec<Hidden> = crate::read_json(&hidden_file(paths)).unwrap_or_default();
    for f in found.iter_mut().filter(|f| f.level != Level::Error) {
        f.hidden = all.iter().any(|h| same(h, f) && h.stamp == stamp(mods, &f.subject));
    }
}

pub(crate) fn err(subject: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    finding(Level::Error, subject, key, template, vars)
}

pub(crate) fn warn(subject: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    finding(Level::Warning, subject, key, template, vars)
}

pub(crate) fn note(subject: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    finding(Level::Note, subject, key, template, vars)
}

/// Check the enabled mods (in load order) for `build` (41/42), against everything
/// installed (`mods`), plus every server config.
pub fn check(paths: &Paths, enabled: &[String], mods: &BTreeMap<String, Row>, build: u8) -> Vec<Finding> {
    // tests never read the real settings or probe the real folders
    let mut out = if cfg!(test) { vec![] } else { check_folders() };
    // the Workshop items that provide each id (the scan shows one copy, a local one when there is)
    let mut items_of: HashMap<String, BTreeSet<String>> = HashMap::new();
    if let Some(ws) = super::workshop_dir() {
        for m in super::workshop_mods(&ws) {
            let item = m.dir.strip_prefix(&ws).ok().and_then(|r| r.iter().next()).map(|s| s.to_string_lossy().into_owned());
            items_of.entry(m.id).or_default().extend(item);
        }
    }
    let game = super::logs::game_version(&paths.zomboid).filter(|_| build >= 42);
    // what ZombieBuddy knows (tests never read the real home folder)
    let (authors, approvals) = match super::javamods::zb_config_dir().filter(|_| !cfg!(test)) {
        Some(d) => (super::javamods::known_authors(mods), super::javamods::read_approvals(&d.join("mod_approvals.json"))),
        None => Default::default(),
    };
    let except = super::javamods::exceptions(paths);
    let pos: HashMap<&str, usize> = enabled.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    for (i, id) in enabled.iter().enumerate() {
        let Some(m) = mods.get(id) else {
            out.push(Finding { action: Some(Action::Install(id.clone())), ..err(id, "notInstalled", "enabled but not installed: the game skips it", &[]) });
            continue;
        };
        out.extend(check_info(m, game.as_deref()));
        if let Some(items) = items_of.get(id).filter(|s| s.len() > 1) {
            let items = items.iter().cloned().collect::<Vec<_>>().join(", ");
            out.push(warn(id, "sameId", "Workshop items {items} all use this mod ID: the game loads only one of them", &[("items", items)]));
        }
        let official_zb = id == super::zombiebuddy::MOD_ID && super::zombiebuddy::official_source(&m.source);
        if !official_zb {
            if let Some(j) = super::javamods::inspect(m, &authors, &approvals) {
                out.extend(super::javamods::findings(&j, &except));
            }
            out.extend(super::javamods::scan_folder(m, &except));
        }
        for r in &m.requires {
            let req = [("req", r.clone())];
            match pos.get(r.as_str()) {
                None if mods.contains_key(r) => out.push(err(id, "needsDisabled", "needs {req}, which is disabled", &req)),
                None => out.push(Finding { action: Some(Action::Install(r.clone())), ..err(id, "needsMissing", "needs {req}, which isn't installed", &req) }),
                Some(&j) if j > i => out.push(err(id, "needsLater", "needs {req}, which loads after it", &req)),
                Some(_) => {}
            }
        }
        for x in &m.incompatible {
            if pos.contains_key(x.as_str()) {
                let other = [("other", x.clone())];
                out.push(Finding { action: Some(Action::Disable(x.clone())), ..err(id, "incompatible", "is incompatible with {other}, which is also enabled", &other) });
            }
        }
        for a in &m.load_after {
            if pos.get(a.as_str()).is_some_and(|&j| j > i) {
                out.push(warn(id, "loadAfter", "should load after {other}", &[("other", a.clone())]));
            }
        }
        for b in &m.load_before {
            if pos.get(b.as_str()).is_some_and(|&j| j < i) {
                out.push(warn(id, "loadBefore", "should load before {other}", &[("other", b.clone())]));
            }
        }
        if build >= 42 && !m.supports_42 {
            out.push(err(id, "only41", "is built for Build 41 only (no 42/ or common/ folder): Build 42 doesn't load it", &[]));
        } else if build == 41 && !m.supports_41 {
            out.push(warn(id, "only42", "is built for Build 42 only (no root mod.info): Build 41 won't load it", &[]));
        }
        if !m.source.starts_with("steam:") && items_of.contains_key(id) {
            out.push(warn(id, "twoCopies", "is in Zomboid/mods and also subscribed in Steam: the Zomboid/mods copy wins and won't get Steam's updates", &[]));
        }
    }
    let on: Vec<&Row> = enabled.iter().filter_map(|id| mods.get(id)).collect();
    out.extend(check_lua(&on, &except));
    // the game folder and its launcher (tests never read the real game folder or environment)
    if let Some(game) = super::zombiebuddy::game_dir().filter(|_| !cfg!(test)) {
        let env: Vec<(&str, String)> = ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"].into_iter().filter_map(|k| Some((k, std::env::var(k).ok().filter(|v| !v.trim().is_empty())?))).collect();
        out.extend(super::javamods::scan_game(&game, super::zombiebuddy::locate(mods).1.as_deref(), &env, &except));
    }
    if let Some(vanilla) = vanilla_lua() {
        out.extend(missing_requires(&on, mods, &vanilla));
    }
    // ZombieBuddy's agent: the mod alone isn't enough (tests never read the real game folder)
    let zb = super::zombiebuddy::required_by(enabled, mods);
    // the same test as the Java tab's "set up" tick: installed, agent current, and enabled here
    if !zb.is_empty() && !cfg!(test) && (!enabled.iter().any(|id| id == super::zombiebuddy::MOD_ID) || !super::zombiebuddy::agent_ready(mods)) {
        out.extend(zb.iter().map(|id| err(id, "zbAgent", "needs ZombieBuddy's Java agent, which isn't set up in the game folder: set it up in the Java mods tab", &[])));
    }
    // Leaf mods need Leaf installed and started (tests never read the real game folder)
    if let Some(game) = super::zombiebuddy::game_dir().filter(|_| !cfg!(test)) {
        let leaf = || super::javamods::other_loaders(&game, super::javamods::steam_launch_options).into_iter().find(|l| l.name == "leaf");
        out.extend(super::javamods::leaf_findings(enabled.iter().filter_map(|id| mods.get(id)), leaf));
    }
    // a launcher flag left without the agent's files stops the game starting, whatever is enabled
    if !cfg!(test)
        && let Some(launcher) = super::zombiebuddy::game_dir().and_then(|g| super::zombiebuddy::launcher_dir(&g))
        && super::zombiebuddy::launcher_broken(&launcher)
    {
        out.push(err("ProjectZomboid64.json", "zbBroken", "loads ZombieBuddy's Java agent, whose files aren't in the game folder, so the game won't start: set it up or remove it in the Java mods tab", &[]));
    }
    if build == 41 && enabled.iter().any(|id| id == super::viewpoint::MOD_ID) {
        out.push(err(super::viewpoint::MOD_ID, "viewpoint41", "needs Build 42: it doesn't run on Build 41", &[]));
    }
    out.extend(check_server_profile(paths, build));
    let on: BTreeMap<String, Row> = enabled.iter().filter_map(|id| Some((id.clone(), mods.get(id)?.clone()))).collect();
    out.extend(check_maps(&super::maps::installed(&on, build, &mut super::maps::Cache::load(paths))));
    out.extend(check_servers(paths, mods));
    out
}

/// What the Workshop says about the enabled Workshop mods (asks Steam; nothing when it can't):
/// items gone, taken down, hidden or unlisted, and mods whose ID's original item is another one.
pub fn check_workshop(paths: &Paths, enabled: &[String], mods: &BTreeMap<String, Row>) -> Vec<Finding> {
    let mine: Vec<(&str, &str)> = enabled.iter().filter_map(|id| Some((id.as_str(), crate::sources::workshop_item(&mods.get(id)?.source)?))).collect();
    if mine.is_empty() {
        return vec![];
    }
    let index = super::mod_index::kept_by_id(paths);
    // each mod's own item and the most used items with its ID (the original is usually among them)
    let mut ids: Vec<&str> = mine.iter().flat_map(|(id, item)| std::iter::once(*item).chain(index.get(*id).into_iter().flatten().take(5).map(String::as_str))).collect();
    ids.sort_unstable();
    ids.dedup();
    let (mut details, mut status) = (vec![], vec![]);
    for chunk in ids.chunks(100) {
        let Ok((d, s)) = crate::sources::steam_workshop::lookup(chunk) else { return vec![] };
        details.extend(d);
        status.extend(s);
    }
    workshop_findings(&mine, &index, &details, &status)
}

/// [`check_workshop`]'s findings for `mine` (mod ID, its Workshop item), given the mod ID index
/// (ID → items) and what Steam said about the items.
fn workshop_findings(mine: &[(&str, &str)], index: &HashMap<String, Vec<String>>, details: &[crate::sources::steam_workshop::Item], status: &[(String, &str)]) -> Vec<Finding> {
    let mut out = vec![];
    for &(id, item) in mine {
        let v = [("item", item.to_string())];
        match status.iter().find(|(i, _)| i == item).map(|s| s.1) {
            Some("gone") => out.push(warn(id, "workshopGone", "its Workshop item {item} is gone (removed, or made private): it gets no updates, and servers can't download it", &v)),
            Some("banned") => out.push(warn(id, "workshopBanned", "its Workshop item {item} was taken down by Steam: it gets no updates, and servers can't download it", &v)),
            Some("hidden") => out.push(warn(id, "workshopHidden", "its Workshop item {item} is hidden (friends only or private): servers and other players can't download it", &v)),
            Some(_) => out.push(note(id, "workshopUnlisted", "its Workshop item {item} is unlisted: it still works, but its author may have stopped updating it", &v)),
            None => {}
        }
        // an earlier item with this ID that more players use: the original, and this a copy or repack
        let Some(me) = details.iter().find(|d| d.id == item) else { continue };
        let original = index
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|i| details.iter().find(|d| &d.id == i))
            .filter(|d| d.id != item && d.time_created < me.time_created && d.subscriptions > me.subscriptions)
            .min_by_key(|d| d.time_created);
        if let Some(o) = original {
            let vars = [("mine", item.to_string()), ("item", o.id.clone()), ("title", o.title.clone())];
            out.push(warn(id, "notOriginal", "comes from Workshop item {mine}, but this mod ID's original is Workshop item {item} ({title}): yours may be an old copy or a repack", &vars));
        }
    }
    out
}

/// The active profile is a server's (see `profile::for_server`) that runs another build.
fn check_server_profile(paths: &Paths, build: u8) -> Option<Finding> {
    let name = crate::profile::active_name(paths);
    let key = crate::profile::server_of(paths, &name)?;
    let theirs = match key.strip_prefix("ini:") {
        Some(ini) => server::build(paths, ini),
        None => super::launcher::state(paths).history.into_iter().find(|s| s.addr == key)?.build.split('.').next()?.parse().ok()?,
    };
    (theirs != build).then(|| warn(&name, "serverBuild", "is for a server on Build {theirs}, but you play Build {mine}: the server won't let you in", &[("theirs", theirs.to_string()), ("mine", build.to_string())]))
}

/// Enabled maps that clash: two mods adding the same map folder, or maps on the same cells (a map
/// built on another one, which it requires, overlaps it on purpose).
fn check_maps(on: &[super::maps::MapMod]) -> Vec<Finding> {
    let on: Vec<_> = on.iter().collect();
    super::maps::clashes(&on)
        .into_iter()
        .map(|(i, j, folder)| {
            let (a, other) = (&on[i].id, ("other", on[j].id.clone()));
            match folder {
                Some(map) => warn(a, "mapFolderTwice", "adds the map folder {map}, which {other} adds too: the game loads only one of them", &[("map", map), other]),
                None => warn(a, "mapOverlap", "its map covers some of the same cells as {other}'s: where they overlap, the one listed first in Map= wins", &[other]),
            }
        })
        .collect()
}

/// mod.info problems: no id= or name=, a Build 41 id that differs, a game version it doesn't take.
fn check_info(m: &Row, game: Option<&str>) -> Vec<Finding> {
    let mut out = vec![];
    let info = super::read_info(&m.info);
    let id = m.id.as_str();
    if info.get("id").is_none_or(|v| v.is_empty()) {
        out.push(warn(id, "noId", "its mod.info has no id=, so the game may not list it (PZMM uses the folder name)", &[]));
    }
    if info.get("name").is_none_or(|v| v.is_empty()) {
        out.push(warn(id, "noName", "its mod.info has no name=, so the game's mod list shows it blank", &[]));
    }
    let root = m.dir.join("mod.info");
    if m.info != root
        && let Some(other) = super::read_info(&root).remove("id").filter(|o| !o.is_empty() && o != id)
    {
        out.push(warn(id, "idMismatch", "its Build 41 mod.info says id={other}: players on the other build see a different mod ID", &[("other", other)]));
    }
    if let Some(game) = game {
        let (gv, vars) = (version(game), |v: &String| [("version", v.clone()), ("game", game.to_string())]);
        if let Some(min) = info.get("versionMin").filter(|v| !version(v).is_empty() && version(v) > gv) {
            out.push(err(id, "versionMin", "needs game version {version} or newer (you have {game}): the game won't load it", &vars(min)));
        }
        // versionMax=42.13 takes every 42.13.x: compare as many parts as it gives
        if let Some(max) = info.get("versionMax").filter(|v| !version(v).is_empty() && gv.iter().take(version(v).len()).copied().collect::<Vec<_>>() > version(v)) {
            out.push(err(id, "versionMax", "is made for game version {version} or older (you have {game}): the game won't load it", &vars(max)));
        }
    }
    out
}

/// `42.13.1` → [42, 13, 1], trailing zeros dropped so 42.13 == 42.13.0.
fn version(v: &str) -> Vec<u32> {
    let mut out: Vec<u32> = v.trim().split('.').map_while(|p| p.trim().parse().ok()).collect();
    while out.last() == Some(&0) {
        out.pop();
    }
    out
}

/// The game's Lua modules (and what its own code asks `require` for), or None when the game
/// folder isn't found: without it every vanilla `require` would look missing.
fn vanilla_lua() -> Option<HashSet<String>> {
    if cfg!(test) {
        return None;
    }
    let lua = super::folders::path("game", &crate::settings::load())?.join("media").join("lua");
    if !lua.is_dir() {
        return None;
    }
    let mut known = HashSet::new();
    let mut files = vec![];
    lua_modules(&lua, &mut known, &mut files);
    // vanilla asks for a few files that don't exist; a mod copying that is no worse off
    for f in files {
        known.extend(lua_requires(&String::from_utf8_lossy(&std::fs::read(f).unwrap_or_default())));
    }
    Some(known)
}

/// `require` names as the game resolves them: case-insensitive, `/`-separated, no `.lua`.
fn module_name(s: &str) -> String {
    // `A\\B` in Lua source is one backslash; dropping empty parts also drops a leading `/`
    let s = s.trim().replace('\\', "/").to_ascii_lowercase();
    s.split('/').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("/").trim_end_matches(".lua").to_string()
}

/// Add the modules under a `media/lua` folder (`client/ISUI/X.lua` is `isui/x`) and list its files.
fn lua_modules(media_lua: &std::path::Path, known: &mut HashSet<String>, files: &mut Vec<std::path::PathBuf>) {
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lua")) {
                out.push(p);
            }
        }
    }
    for side in ["shared", "client", "server"] {
        let base = media_lua.join(side);
        let start = files.len();
        walk(&base, files);
        known.extend(files[start..].iter().filter_map(|f| f.strip_prefix(&base).ok()).map(|r| module_name(&r.to_string_lossy())));
    }
}

/// The `media/lua` folders a mod may load from (root for Build 41; `common/` and its version folder for 42).
fn lua_roots(m: &Row) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = vec![];
    for d in [Some(m.dir.clone()), Some(m.dir.join("common")), m.info.parent().map(|p| p.to_path_buf())].into_iter().flatten() {
        let lua = d.join("media").join("lua");
        if lua.is_dir() && !out.contains(&lua) {
            out.push(lua);
        }
    }
    out
}

/// The literal `require "x"` / `require("x")` names in Lua source, skipping comments and `pcall`
/// lines (an optional require is allowed to fail).
pub(crate) fn lua_requires(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut in_block = false;
    for line in text.lines() {
        let mut l = line;
        if in_block {
            match l.find("]]") {
                Some(i) => (l, in_block) = (&l[i + 2..], false),
                None => continue,
            }
        }
        if let Some(i) = l.find("--") {
            in_block = l[i..].starts_with("--[[") && !l[i..].contains("]]");
            l = &l[..i];
        }
        // ponytail: top-level lines only; an indented require sits in an `if` or a function, usually
        // a compatibility patch for a mod that may be absent. Misses top-level requires in blocks.
        if l.contains("pcall") || l.starts_with([' ', '\t']) {
            continue;
        }
        let mut rest = l;
        while let Some(i) = rest.find("require") {
            let word_start = !rest[..i].ends_with(|c: char| c.is_alphanumeric() || "_.:".contains(c));
            let after = rest[i + 7..].trim_start();
            let after = after.strip_prefix('(').map_or(after, str::trim_start);
            if word_start
                && let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'')
                && let Some(end) = after[1..].find(q)
                && !after[end + 2..].trim_start().starts_with("..") // a name built at run time
            {
                out.push(module_name(&after[1..1 + end]));
            }
            rest = &rest[i + 7..];
        }
    }
    out
}

/// The first syntax problem in Lua source that stops the game compiling the file: a block
/// (`function`, `if`, `do`, `while`/`for`, `repeat`) or bracket left open or closed by the wrong
/// word, or a string or long comment that never ends. Returns its 1-based line. Expressions
/// aren't checked: this finds what breaks a whole file, without false alarms on code Kahlua runs.
pub(crate) fn lua_syntax(src: &[u8]) -> Option<usize> {
    let b = src.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(src);
    // what's open: the word or bracket, and its line (`for` and `while` wait for their `do`)
    let mut open: Vec<(&[u8], usize)> = vec![];
    let (mut i, mut line) = (0, 1);
    let end_of_line = |mut j: usize| {
        while j < b.len() && !matches!(b[j], b'\n' | b'\r') {
            j += 1;
        }
        j
    };
    if b.starts_with(b"#") {
        i = end_of_line(0); // a `#!` first line
    }
    // the level of a long bracket (`[[`, `[==[`) at `at`
    let long = |at: usize| {
        let eq = b[at + 1..].iter().take_while(|&&c| c == b'=').count();
        (b.get(at + 1 + eq) == Some(&b'[')).then_some(eq)
    };
    // the end of the long bracket at `at`; None when it never closes
    let skip_long = |at: usize, eq: usize, line: &mut usize| {
        let close = [b"]".as_slice(), &vec![b'='; eq], b"]"].concat();
        let mut j = at + eq + 2;
        while j < b.len() {
            if b[j..].starts_with(&close) {
                return Some(j + close.len());
            }
            *line += usize::from(b[j] == b'\n');
            j += 1;
        }
        None
    };
    let mut field = false; // after `.` or `:`, a keyword is a field's name
    while i < b.len() {
        let c = b[i];
        match c {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b' ' | b'\t' | b'\r' | b'\x0b' | b'\x0c' => i += 1,
            b'-' if b.get(i + 1) == Some(&b'-') => match (b.get(i + 2) == Some(&b'[')).then(|| long(i + 2)).flatten() {
                Some(eq) => {
                    let start = line;
                    match skip_long(i + 2, eq, &mut line) {
                        Some(end) => i = end,
                        None => return Some(start),
                    }
                }
                None => i = end_of_line(i),
            },
            b'"' | b'\'' => {
                let mut j = i + 1;
                loop {
                    match b.get(j) {
                        None | Some(b'\n' | b'\r') => return Some(line),
                        Some(b'\\') if b[j + 1..].starts_with(b"\r\n") => (line, j) = (line + 1, j + 3),
                        Some(b'\\') => (line, j) = (line + usize::from(b.get(j + 1) == Some(&b'\n')), j + 2),
                        Some(&q) if q == c => break,
                        _ => j += 1,
                    }
                }
                (i, field) = (j + 1, false);
            }
            b'[' if long(i).is_some() => {
                let start = line;
                match skip_long(i, long(i).unwrap_or_default(), &mut line) {
                    Some(end) => (i, field) = (end, false),
                    None => return Some(start),
                }
            }
            b'(' | b'[' | b'{' => {
                open.push((&b[i..i + 1], line));
                (i, field) = (i + 1, false);
            }
            b')' | b']' | b'}' => {
                let want: &[u8] = match c {
                    b')' => b"(",
                    b']' => b"[",
                    _ => b"{",
                };
                if open.pop().is_none_or(|(w, _)| w != want) {
                    return Some(line);
                }
                (i, field) = (i + 1, false);
            }
            b'0'..=b'9' | b'.' if c != b'.' || b.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                // 12, .5, 0x1F, 1.5e-3
                let start = i;
                let hex = b[i..].starts_with(b"0x") || b[i..].starts_with(b"0X");
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || (!hex && matches!(b[i], b'+' | b'-') && matches!(b[i - 1], b'e' | b'E') && i > start)) {
                    i += 1;
                }
                field = false;
            }
            b'.' | b':' => {
                // `.` and `:` name a field; `..` joins, `...` is the varargs, `::` marks a label
                let run = b[i..].iter().take_while(|&&x| x == c).count();
                (i, field) = (i + run, run == 1);
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 0x80) {
                    i += 1;
                }
                if std::mem::take(&mut field) {
                    continue;
                }
                let word = &b[start..i];
                let top = open.last().map(|(w, _)| *w);
                match word {
                    b"function" | b"if" | b"repeat" | b"for" | b"while" | b"do" if !(word == b"do" && matches!(top, Some(b"for" | b"while"))) => open.push((word, line)),
                    b"do" => {
                        // the loop's `do` opens its block
                        let at = open.pop().map_or(line, |(_, l)| l);
                        open.push((b"do", at));
                    }
                    b"then" | b"elseif" | b"else" if top != Some(b"if") => return Some(line),
                    b"end" if matches!(top, Some(b"function" | b"if" | b"do")) => drop(open.pop()),
                    b"until" if top == Some(b"repeat") => drop(open.pop()),
                    b"end" | b"until" => return Some(line),
                    _ => {}
                }
            }
            _ => (i, field) = (i + 1, false),
        }
    }
    open.last().map(|(_, l)| *l)
}

/// Enabled mods' Lua files that don't parse (see [`lua_syntax`], a few per mod at most), and what
/// their Lua names that a game mod has no use for (`javamods::scan_lua`, one finding per kind and mod).
fn check_lua(on: &[&Row], except: &HashSet<String>) -> Vec<Finding> {
    let mut out = vec![];
    for m in on {
        let (mut bad, mut said) = (0, HashSet::new());
        for root in lua_roots(m) {
            let mut files = vec![];
            lua_modules(&root, &mut HashSet::new(), &mut files);
            for f in files {
                let Ok(bytes) = std::fs::read(&f) else { continue };
                let file = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                if let Some(line) = lua_syntax(&bytes) {
                    bad += 1;
                    if bad <= 3 {
                        out.push(warn(&m.id, "luaSyntax", "its Lua file {file} has a syntax error near line {line}: the game won't run that file", &[("file", file.clone()), ("line", line.to_string())]));
                    }
                }
                let sha = super::javamods::sha256(&bytes);
                if except.contains(&sha) {
                    continue; // trusted: its findings don't use up the mod's one-per-kind
                }
                let risks: Vec<Finding> = super::javamods::scan_lua(&m.id, &file, &String::from_utf8_lossy(&bytes)).into_iter().filter(|r| said.insert(r.key)).collect();
                out.extend(super::javamods::dismissable(risks, &sha, except));
            }
        }
    }
    out
}

/// `require` targets in the enabled mods' Lua that neither they nor the game provide.
fn missing_requires(on: &[&Row], all: &BTreeMap<String, Row>, vanilla: &HashSet<String>) -> Vec<Finding> {
    let mut known = vanilla.clone();
    let mut files: Vec<(&str, std::path::PathBuf)> = vec![];
    for m in on {
        for root in lua_roots(m) {
            let mut mine = vec![];
            lua_modules(&root, &mut known, &mut mine);
            files.extend(mine.into_iter().map(|f| (m.id.as_str(), f)));
        }
    }
    let mut out = vec![];
    let mut said = HashSet::new();
    let mut off: Option<Vec<(&str, HashSet<String>)>> = None; // disabled mods' modules, read on the first miss
    for (id, f) in files {
        for target in lua_requires(&String::from_utf8_lossy(&std::fs::read(&f).unwrap_or_default())) {
            if known.contains(&target) || !said.insert((id, target.clone())) {
                continue;
            }
            let off = off.get_or_insert_with(|| {
                all.values()
                    .filter(|m| !on.iter().any(|o| o.id == m.id))
                    .map(|m| {
                        let mut k = HashSet::new();
                        lua_roots(m).iter().for_each(|r| lua_modules(r, &mut k, &mut vec![]));
                        (m.id.as_str(), k)
                    })
                    .collect()
            });
            let file = ("file", format!("{target}.lua"));
            match off.iter().find(|(_, k)| k.contains(&target)) {
                Some((other, _)) => out.push(warn(id, "requireDisabled", "its Lua needs {file} from {other}, which is disabled", &[file, ("other", other.to_string())])),
                None => out.push(warn(id, "requireMissing", "its Lua needs {file}, which no enabled mod or the game has", &[file])),
            }
        }
    }
    out
}

/// Folders that are set (by an environment variable or in Settings) but wrong, and folder warnings.
fn check_folders() -> Vec<Finding> {
    let mut out = vec![];
    for f in super::folders::list(&crate::settings::load()) {
        let Some(p) = &f.path else { continue };
        let path = ("path", p.display().to_string());
        let folder = ("folder", f.label.to_string());
        let key = ("key", f.key.to_string());
        let env = f.env.filter(|_| f.overridden).map(|e| ("env", e.to_string()));
        match (f.status, env) {
            ("missing", Some(env)) => out.push(err(f.label, "missingEnv", "{path} doesn't exist (set by {env})", &[path.clone(), env])),
            ("missing", None) if f.custom => out.push(err(f.label, "missingSet", "{path} doesn't exist (set in Settings)", &[path.clone(), key.clone()])),
            ("missing", None) if f.key == "zomboid" => out.push(warn(f.label, "notYet", "{path} doesn't exist yet: start the game once", std::slice::from_ref(&path))),
            ("not_pz", Some(env)) => out.push(warn(f.label, "notPzEnv", "{path} doesn't look like the {folder} (set by {env})", &[path.clone(), folder, env])),
            ("not_pz", None) if f.custom => out.push(warn(f.label, "notPzSet", "{path} doesn't look like the {folder} (set in Settings)", &[path.clone(), folder, key.clone()])),
            ("not_pz", None) => out.push(warn(f.label, "notPz", "{path} doesn't look like the {folder}", &[path.clone(), folder, key.clone()])),
            _ => {}
        }
        out.extend(f.warnings.iter().map(|w| warn(f.label, "folderWarning", "{path}: {warning}", &[path.clone(), ("warning", w.clone()), key.clone()])));
    }
    out
}

/// [`mask_user`] with this account's name (its home folder's name).
pub fn mask_home_user(text: &str) -> String {
    let user = dirs::home_dir().and_then(|h| h.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
    mask_user(text, &user)
}

/// `text` with `user` (the account name) hidden wherever it's a folder in a path
/// (`C:\Users\<user>\...`, `/home/<user>/...`), for bug reports.
pub fn mask_user(text: &str, user: &str) -> String {
    if user.len() < 2 {
        return text.into();
    }
    // ASCII lowercasing keeps byte offsets identical
    let (hay, needle) = (text.to_ascii_lowercase(), user.to_ascii_lowercase());
    let (mut out, mut last, mut from) = (String::with_capacity(text.len()), 0, 0);
    while let Some(i) = hay[from..].find(&needle).map(|i| i + from) {
        let end = i + needle.len();
        let after = hay[end..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || "._-".contains(c)));
        if hay[..i].ends_with(['\\', '/']) && after {
            out.push_str(&text[last..i]);
            out.push_str("<user>");
            last = end;
        }
        from = end;
    }
    out + &text[last..]
}

/// Server configs whose Mods=, WorkshopItems= and Map= disagree, each judged on the build it runs.
fn check_servers(paths: &Paths, mods: &BTreeMap<String, Row>) -> Vec<Finding> {
    server::list(paths).iter().flat_map(|name| check_server(paths, name, mods)).collect()
}

/// [`check_servers`] for one server (before it starts).
pub fn check_server(paths: &Paths, name: &str, mods: &BTreeMap<String, Row>) -> Vec<Finding> {
    let mut out = vec![];
    let build = server::build(paths, name);
    let Ok(file) = server::path(paths, name) else { return out };
    let Ok(text) = std::fs::read_to_string(&file) else { return out };
    let Ok(cfg) = server::read(&file) else { return out };
    let subject = format!("{name}.ini");
    // what the server downloaded itself counts too: its mod.info files name their requirements
    let known = match super::host::find(paths, name) {
        Some(s) => server::with_downloads(mods, &s.install_dir),
        None => mods.clone(),
    };
    let mut index = None; // the Workshop's mod IDs, read on the first unknown one
    // Mods= against WorkshopItems= and the mods' requirements: the check Hosting runs before writing (the Workshop isn't asked here)
    for p in server::check(&cfg, &known, None) {
        let id = ("id", p.id.clone());
        match p.key {
            "itemMissing" => out.push(err(&subject, "itemNotListed", "Mods= has {id}, but its Workshop item {item} isn't in WorkshopItems=: the server won't download it", &[id, ("item", p.item)])),
            "unknownMod" => {
                let items = index.get_or_insert_with(|| super::mod_index::kept_by_id(paths)).get(&p.id).cloned().unwrap_or_default();
                match mods.keys().find(|k| k.eq_ignore_ascii_case(&p.id)) {
                    Some(real) => out.push(warn(&subject, "idCase", "Mods= has {id}, but the mod's ID is {real}: IDs must match exactly", &[id, ("real", real.clone())])),
                    // the most used item with this ID
                    None if !items.is_empty() && !items.iter().any(|i| cfg.workshop.contains(i)) => {
                        out.push(err(&subject, "itemNotListed", "Mods= has {id}, but its Workshop item {item} isn't in WorkshopItems=: the server won't download it", &[id, ("item", items[0].clone())]))
                    }
                    None if !items.is_empty() => {} // a listed item has it; it just isn't downloaded here
                    None => out.push(warn(&subject, "serverModMissing", "Mods= has {id}, which isn't installed here, so PZMM can't check it", &[id])),
                }
            }
            "needs" => out.push(err(&subject, "serverNeeds", "Mods= has {id}, which needs {req}, but Mods= doesn't list it: {id} won't load", &[id, ("req", p.item)])),
            "needsLater" => out.push(warn(&subject, "serverNeedsLater", "Mods= lists {req} after {id}, which needs it: {req} must come first", &[id, ("req", p.item)])),
            "incompatible" => out.push(warn(&subject, "serverIncompatible", "Mods= has {id} and {other}, which don't work together", &[id, ("other", p.item)])),
            _ => {}
        }
    }
    let used: HashSet<&str> = cfg.mods.iter().filter_map(|id| crate::sources::workshop_item(&known.get(id)?.source)).collect();
    for item in &cfg.workshop {
        if !used.contains(item.as_str()) && known.values().any(|m| crate::sources::workshop_item(&m.source) == Some(item.as_str())) {
            out.push(warn(&subject, "itemUnused", "WorkshopItems= has {item}, but none of its mods are in Mods=", &[("item", item.clone())]));
        }
    }
    let mods_line = text.lines().find_map(|l| l.strip_prefix("Mods=")).unwrap_or("");
    if build >= 42 && !cfg.mods.is_empty() && !mods_line.contains('\\') {
        out.push(err(&subject, "backslash", "Mods= entries need a leading backslash on Build 42 (\\ModId)", &[]));
    }
    let mod_maps = server::mod_maps(&cfg.mods, &known, build);
    for mm in mod_maps.iter().filter(|mm| !cfg.maps.contains(mm)) {
        out.push(warn(&subject, "mapNotListed", "a mod adds the map {map}, but Map= doesn't list it", &[("map", mm.clone())]));
    }
    if let Some(base) = cfg.maps.iter().position(|m| server::is_base_map(m))
        && cfg.maps[base + 1..].iter().any(|m| mod_maps.contains(m))
    {
        out.push(warn(&subject, "mapsAfterBase", "Map= lists mod maps after Muldraugh, KY: mod maps must come first", &[]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_paths;
    use std::fs;
    use std::path::PathBuf;

    fn row(id: &str, dir: PathBuf, source: &str, requires: &[&str]) -> Row {
        let versioned = dir.join("42").is_dir();
        Row {
            id: id.into(),
            name: id.into(),
            source: source.into(),
            info: dir.join("mod.info"),
            dir,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: !versioned,
            supports_42: versioned,
            ..Default::default()
        }
    }

    #[test]
    fn masks_the_user_name_in_paths() {
        let text = r"Zomboid: C:\Users\Sam\Zomboid, data: /home/sam/.local/share/pzmm, Sam's profile, C:\Users\Samuel\x, D:\sam";
        assert_eq!(
            mask_user(text, "Sam"),
            r"Zomboid: C:\Users\<user>\Zomboid, data: /home/<user>/.local/share/pzmm, Sam's profile, C:\Users\Samuel\x, D:\<user>"
        );
        assert_eq!(mask_user(r"C:\Users\A\x", "A"), r"C:\Users\A\x", "one letter is too short to mask safely");
    }

    #[test]
    fn hidden_findings_come_back_when_the_mod_changes() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().join("Zomboid"), data: t.path().join("data") };
        let dir = t.path().join("M");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("mod.info"), "id=M\n").unwrap();
        let mods = BTreeMap::from([("M".to_string(), Row { id: "M".into(), info: dir.join("mod.info"), dir: dir.clone(), ..Default::default() })]);
        let f = warn("M", "only41", "is built for Build 41 only (no 42/ or common/ folder): Build 42 doesn't load it", &[]);
        hide(&paths, &mods, "only41", "M", BTreeMap::new(), true).unwrap();
        let mut found = vec![f.clone(), warn("N", "only41", "is built for Build 41 only (no 42/ or common/ folder): Build 42 doesn't load it", &[])];
        mark_hidden(&paths, &mods, &mut found);
        assert_eq!(found.iter().map(|f| f.hidden).collect::<Vec<_>>(), [true, false]);
        fs::write(dir.join("mod.info"), "id=M\nname=New\n").unwrap();
        mark_hidden(&paths, &mods, &mut found);
        assert!(!found[0].hidden, "an update brings it back");
        hide(&paths, &mods, "notInstalled", "M", BTreeMap::new(), true).unwrap();
        let mut errs = vec![err("M", "notInstalled", "enabled but not installed: the game skips it", &[])];
        mark_hidden(&paths, &mods, &mut errs);
        assert!(!errs[0].hidden, "errors stay");
    }

    #[test]
    fn checks_mod_info_and_game_version() {
        let (t, paths) = temp_paths();
        let dir = t.path().join("M");
        fs::create_dir_all(dir.join("42")).unwrap();
        fs::write(dir.join("mod.info"), "id=M41\nname=M\n").unwrap();
        fs::write(dir.join("42/mod.info"), "id=M\nversionMin=42.22\nversionMax=42.13\n").unwrap();
        let mut m = row("M", dir.clone(), "steam:1", &[]);
        m.info = dir.join("42/mod.info");
        let keys = |game| check_info(&m, game).into_iter().map(|f| f.key).collect::<Vec<_>>();
        assert_eq!(keys(Some("42.21.0")), ["noName", "idMismatch", "versionMin", "versionMax"]);
        assert_eq!(keys(None), ["noName", "idMismatch"]);
        fs::write(dir.join("mod.info"), "id=M\nname=M\n").unwrap();
        fs::write(dir.join("42/mod.info"), "id=M\nname=M\nversionMin=42.0\nversionMax=42.21\n").unwrap();
        assert!(keys(Some("42.21.3")).is_empty(), "42.21 takes every 42.21.x");
        assert!(version("42.21.0") == version("42.21") && version("42.9") < version("42.13"));

        // the game version comes from the last run's console.txt
        fs::create_dir_all(&paths.zomboid).unwrap();
        fs::write(paths.zomboid.join("console.txt"), "LOG  : General      f:0> version=42.1.0 abc demo=false\n").unwrap();
        fs::write(dir.join("42/mod.info"), "id=M\nname=M\nversionMin=42.2\n").unwrap();
        let found = check(&paths, &["M".into()], &BTreeMap::from([("M".to_string(), m.clone())]), 42);
        assert!(found.iter().any(|f| f.key == "versionMin" && f.text.contains("(you have 42.1.0)")), "{found:?}");
    }

    #[test]
    fn finds_lua_that_does_not_parse() {
        let fine = "#!/usr/bin/lua\r\nlocal t = { a = [[long ]] .. [==[ ]] still ]==], n = 0x1F + 1.5e-3 + .5, s = \"q\\\"uote\", e = 'it\\'s' }\n\
            --[[ block\nend end ]] -- end\nfunction t:go(x, ...)\n  for i = 1, #x do if x[i] then break elseif i then else end end\n  \
            while x do x = nil end\n  repeat local y = f(function() return 1 end) until y\n  goto done\n  ::done::\n  \
            local s = \"line\\\r\ncontinued\" return t.x:y(...)\nend\n";
        assert_eq!(lua_syntax(fine.as_bytes()), None);
        assert_eq!(lua_syntax(b"function a()\n  if x then\n    y()\nend\n"), Some(1), "a missing end: where the open block starts");
        assert_eq!(lua_syntax(b"local x = 1\nend\n"), Some(2));
        assert_eq!(lua_syntax(b"print(\"oops)\nx()\n"), Some(1));
        assert_eq!(lua_syntax(b"x()\n--[[ never closed\n"), Some(2));
        assert_eq!(lua_syntax(b"local t = { f(1 }\n"), Some(1));
        assert_eq!(lua_syntax(b"for i = 1, 2 do\nthen\nend\n"), Some(2));
        assert_eq!(lua_syntax(b"repeat\nx()\nend\n"), Some(3));

        // found in an enabled mod's Lua, named from its media/lua folder
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("M/media/lua/client")).unwrap();
        fs::write(t.path().join("M/media/lua/client/Good.lua"), "x = 1").unwrap();
        fs::write(t.path().join("M/media/lua/client/Bad.lua"), "if x then\n").unwrap();
        let m = row("M", t.path().join("M"), "steam:1", &[]);
        let found: Vec<String> = check_lua(&[&m], &HashSet::new()).into_iter().map(|f| f.text).collect();
        assert_eq!(found, ["its Lua file client/Bad.lua has a syntax error near line 1: the game won't run that file"]);
    }

    #[test]
    fn finds_lua_requires_nothing_provides() {
        assert_eq!(
            lua_requires("require \"ISUI/ISPanel\"\nlocal x = require('Foo.lua') -- require \"Commented\"\n--[[\nrequire \"Block\"\n]]\npcall(require, \"Optional\")\nmyrequire(\"No\")\nrequire(\"A\\\\B\")\nrequire(\"Lang_\" .. id)\nif x then\n    require \"Compat/Other\"\nend"),
            ["isui/ispanel", "foo", "a/b"]
        );
        let t = tempfile::tempdir().unwrap();
        let lua = |m: &str, file: &str, text: &str| {
            let p = t.path().join(m).join(file);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
            row(m, t.path().join(m), "manual", &[])
        };
        let a = lua("A", "media/lua/client/A/Main.lua", "require \"ISUI/ISPanel\"\nrequire \"A/Util\"\nrequire \"B/Api\"\nrequire \"C/Gone\"\nrequire \"C/Gone\"");
        lua("A", "media/lua/shared/A/Util.lua", "");
        let b = lua("B", "media/lua/shared/B/Api.lua", "");
        let all = BTreeMap::from([("A".to_string(), a.clone()), ("B".to_string(), b)]);
        let vanilla = HashSet::from(["isui/ispanel".to_string()]);
        let found: Vec<String> = missing_requires(&[&a], &all, &vanilla).into_iter().map(|f| format!("{} {}", f.key, f.text)).collect();
        assert_eq!(found, ["requireDisabled its Lua needs b/api.lua from B, which is disabled", "requireMissing its Lua needs c/gone.lua, which no enabled mod or the game has"]);
    }

    #[test]
    fn finds_setup_problems() {
        let (t, paths) = temp_paths();
        let b41 = t.path().join("Old");
        fs::create_dir_all(&b41).unwrap();
        fs::write(b41.join("mod.info"), "id=Old\n").unwrap();
        let b42 = t.path().join("New");
        fs::create_dir_all(b42.join("42")).unwrap();
        let mods = BTreeMap::from([
            ("Old".to_string(), row("Old", b41, "steam:1", &[])),
            ("New".to_string(), row("New", b42.clone(), "steam:2", &["Lib"])),
            ("Lib".to_string(), row("Lib", b42, "steam:3", &[])),
        ]);
        let enabled = vec!["New".to_string(), "Old".to_string(), "Gone".to_string()];
        let found = check(&paths, &enabled, &mods, 42);
        let texts: Vec<String> = found.iter().map(|f| format!("{}: {}", f.subject, f.text)).collect();
        assert!(texts.iter().any(|t| t.starts_with("New: needs Lib, which is disabled")), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("Old: is built for Build 41")), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("Gone: enabled but not installed")), "{texts:?}");
        // fixes the UI can offer: install what's missing, disable the other of an incompatible pair
        let mut clash = mods.clone();
        clash.insert("Old".into(), Row { requires: vec!["Lib2".into()], incompatible: vec!["New".into()], ..mods["Old"].clone() });
        let actions: Vec<_> = check(&paths, &["New".into(), "Lib".into(), "Old".into()], &clash, 41).into_iter().filter_map(|f| f.action).collect();
        assert_eq!(actions, [Action::Install("Lib2".into()), Action::Disable("New".into())]);

        fs::create_dir_all(paths.zomboid.join("Server")).unwrap();
        fs::write(paths.zomboid.join("Server/srv.ini"), "Mods=New;Lib;old\nWorkshopItems=2\nMap=Muldraugh, KY\n").unwrap();
        let found = check(&paths, &[], &mods, 42);
        let texts: Vec<String> = found.iter().map(|f| f.text.clone()).collect();
        assert!(texts.iter().any(|t| t == "Mods= has old, but the mod's ID is Old: IDs must match exactly"), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("Lib, but its Workshop item 3 isn't in WorkshopItems=")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("leading backslash")), "{texts:?}");
    }

    #[test]
    fn server_profiles_know_their_servers_build() {
        let (_t, paths) = temp_paths();
        let server = super::super::launcher::Server { addr: "198.51.100.1:16261".into(), name: "Old".into(), build: "41.78.16".into(), ..Default::default() };
        super::super::launcher::record_join(&paths, &server, None).unwrap();
        let p = crate::profile::for_server(&paths, "Old", &server.addr, vec![], &[]).unwrap();
        crate::profile::apply(&paths, &p, None).unwrap();
        let found: Vec<String> = check(&paths, &[], &BTreeMap::new(), 42).into_iter().filter(|f| f.key == "serverBuild").map(|f| f.text).collect();
        assert_eq!(found, ["is for a server on Build 41, but you play Build 42: the server won't let you in"]);
        assert!(check(&paths, &[], &BTreeMap::new(), 41).iter().all(|f| f.key != "serverBuild"));
    }

    #[test]
    fn finds_map_clashes() {
        use super::super::maps::{MapFolder, MapMod};
        let map = |id: &str, folder: &str, cells: &[(i32, i32)], requires: &[&str]| MapMod {
            id: id.into(),
            requires: requires.iter().map(|r| r.to_string()).collect(),
            maps: vec![MapFolder { folder: folder.into(), cells: cells.to_vec(), ..Default::default() }],
            ..Default::default()
        };
        let on = [map("Town", "Town", &[(1, 1), (1, 2)], &[]), map("Town2", "Town", &[(5, 5)], &[]), map("Fort", "Fort", &[(1, 2)], &[]), map("FortAddon", "FortX", &[(1, 2)], &["Fort"])];
        let found: Vec<String> = check_maps(&on).into_iter().map(|f| format!("{} {}", f.subject, f.text)).collect();
        assert_eq!(found, [
            "Town adds the map folder Town, which Town2 adds too: the game loads only one of them",
            "Town its map covers some of the same cells as Fort's: where they overlap, the one listed first in Map= wins",
            "Town its map covers some of the same cells as FortAddon's: where they overlap, the one listed first in Map= wins",
        ], "an add-on on the map it requires overlaps on purpose");
    }

    #[test]
    fn servers_are_judged_on_their_own_build_and_folder() {
        let (t, paths) = temp_paths();
        // a Build 41 server PZMM manages, its data in a folder of its own: no `\` before mod ids there
        let data = t.path().join("old41");
        fs::create_dir_all(data.join("Server")).unwrap();
        fs::write(data.join("Server/old.ini"), "Mods=A\nWorkshopItems=\nMap=Muldraugh, KY\n").unwrap();
        let host = super::super::host::Server { name: "old".into(), build: super::super::host::LEGACY.into(), cachedir: Some(data), ..Default::default() };
        fs::create_dir_all(&paths.data).unwrap();
        fs::write(paths.data.join("servers.json"), serde_json::to_string(&[host]).unwrap()).unwrap();
        assert_eq!(server::list(&paths), ["old"]);
        let found = check(&paths, &[], &BTreeMap::new(), 42);
        assert!(found.iter().any(|f| f.subject == "old.ini" && f.key == "serverModMissing"), "found in its own folder: {found:?}");
        assert!(!found.iter().any(|f| f.key == "backslash"), "Build 41 servers take plain ids: {found:?}");
    }

    #[test]
    fn finds_workshop_status_and_the_original_copy() {
        use crate::sources::steam_workshop::Item;
        let item = |id: &str, created, subs| Item { id: id.into(), title: format!("T{id}"), time_created: created, subscriptions: subs, ..Default::default() };
        // Pack (item 9) repacks Orig's mod (item 1, older and more used); 5 is an older but less used copy
        let index = HashMap::from([("M".to_string(), vec!["1".to_string(), "9".into(), "5".into()]), ("N".to_string(), vec!["2".to_string()])]);
        let details = [item("1", 100, 5000), item("9", 300, 40), item("5", 50, 10), item("2", 10, 1)];
        let status = [("2".to_string(), "gone"), ("3".to_string(), "unlisted")];
        let found: Vec<String> = workshop_findings(&[("M", "9"), ("N", "2"), ("U", "3"), ("O", "1")], &index, &details, &status).into_iter().map(|f| format!("{} {} {}", f.subject, f.key, f.text)).collect();
        assert_eq!(found, [
            "M notOriginal comes from Workshop item 9, but this mod ID's original is Workshop item 1 (T1): yours may be an old copy or a repack",
            "N workshopGone its Workshop item 2 is gone (removed, or made private): it gets no updates, and servers can't download it",
            "U workshopUnlisted its Workshop item 3 is unlisted: it still works, but its author may have stopped updating it",
        ], "the original itself is fine");
    }

    #[test]
    fn server_mods_need_their_requirements_listed() {
        let (t, paths) = temp_paths();
        // A (installed here) needs B; Z is only in the server's own Workshop downloads and needs Y;
        // Q is in neither, but the mod ID index knows its Workshop item
        let info = |rel: &str, text: &str| {
            fs::create_dir_all(t.path().join(rel)).unwrap();
            fs::write(t.path().join(rel).join("mod.info"), text).unwrap();
        };
        info("ws/1/mods/A", "id=A\nrequire=B\n");
        info("ws/2/mods/B", "id=B\n");
        info("srv/steamapps/workshop/content/108600/7/mods/Z", "id=Z\nrequire=\\Y\n");
        let mods: BTreeMap<String, Row> = ["A", "B"].iter().enumerate().map(|(i, id)| (id.to_string(), row(id, t.path().join(format!("ws/{}/mods/{id}", i + 1)), &format!("steam:{}", i + 1), if *id == "A" { &["B"] } else { &[] }))).collect();
        fs::create_dir_all(paths.zomboid.join("Server")).unwrap();
        fs::write(paths.zomboid.join("Server/srv.ini"), "Mods=\\A;\\Z;\\Q\nWorkshopItems=1;7\nMap=Muldraugh, KY\n").unwrap();
        let host = super::super::host::Server { name: "srv".into(), install_dir: t.path().join("srv"), ..Default::default() };
        fs::create_dir_all(&paths.data).unwrap();
        fs::write(paths.data.join("servers.json"), serde_json::to_string(&[host]).unwrap()).unwrap();
        fs::write(paths.data.join("mod-index.json"), r#"{"at":1,"items":{"55":{"u":1,"s":9,"i":["Q"]}}}"#).unwrap();
        let found: Vec<String> = check_server(&paths, "srv", &mods).into_iter().map(|f| format!("{} {}", f.key, f.text)).collect();
        assert_eq!(found, [
            "itemNotListed Mods= has Q, but its Workshop item 55 isn't in WorkshopItems=: the server won't download it",
            "serverNeeds Mods= has A, which needs B, but Mods= doesn't list it: A won't load",
            "serverNeeds Mods= has Z, which needs Y, but Mods= doesn't list it: Z won't load",
        ]);
    }
}
