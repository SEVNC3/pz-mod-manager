//! The game's side of the conversation with PZMM - Client: what the game asks PZMM to do
//! (`Zomboid/Lua/pzmm/requests.txt`, a line per request) and what it last reported (`game.json`).
//! Game Lua can only write under `Zomboid/Lua`, and any mod can write there, so every request is
//! checked here and the app asks you before it acts on one that changes anything. While PZMM runs it
//! takes them off the game within a second and comes to the front; when it doesn't, the game opens a
//! pzmm://game link (through a web page: the game opens only https) that starts it.
//! Each request is stamped (`<at>-<n>`, the profile the game showed) and PZMM answers it in `acks.txt`; it
//! tells the game it's running, and which profile is active, in `app.txt` (see `profile::app_for_game`).

use serde::{Deserialize, Serialize};

use super::Paths;

/// The folder PZMM and PZMM - Client talk through: `Lua/pzmm` in the Zomboid folder the game plays in (the
/// active profile's own game folder, `-cachedir`, when it has one).
pub fn dir(paths: &Paths) -> std::path::PathBuf {
    dir_for(paths, &crate::profile::active_name(paths))
}

/// The bridge folder of the game running now: the one its fresh report (under 10 minutes, or while the game runs)
/// names, else [`dir`].
pub fn game_dir(paths: &Paths) -> std::path::PathBuf {
    let fresh = game_report(paths).filter(|g| !g.zomboid.is_empty() && (crate::store::now().saturating_sub(g.at) < 600 || super::host::game_running()));
    fresh.map(|g| std::path::Path::new(&g.zomboid).join("Lua").join("pzmm")).filter(|d| d.is_dir()).unwrap_or_else(|| dir(paths))
}

/// [`dir`] for profile `name`: where the game plays when that profile is active.
pub fn dir_for(paths: &Paths, name: &str) -> std::path::PathBuf {
    let own = crate::profile::launch_of(paths, name).cache_dir;
    own.unwrap_or_else(|| paths.zomboid.clone()).join("Lua").join("pzmm")
}

/// The UI is handling the game's requests (a question about one may be open): `app.txt` says busy.
pub static GAME_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The app's look for the in-game manager to match: its accent (`#rrggbb`) and theme (`dark` or `light`).
pub static LOOK: std::sync::Mutex<(String, String)> = std::sync::Mutex::new((String::new(), String::new()));

/// One request from the game.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum Request {
    /// Install these Workshop items.
    Install { ids: Vec<String> },
    /// Back up a save (`mode/name`, as the game names its save folders).
    Backup { mode: String, name: String },
    /// Run Check setup.
    Check,
    /// Switch to this PZMM profile (after asking).
    Profile { name: String },
    /// Run one of Check setup's fixes (`findings.txt` names them): safe ones at once, others after asking.
    Fix { fix: String },
    /// Replace a mod's own tags (the player's labels; none clears them). Quiet.
    Tags {
        /// The mod (`mod` to the UI: `id` is the request's own).
        #[serde(rename = "mod")]
        id: String,
        tags: Vec<String>,
    },
    /// Put a save's backup back (after asking; only while the game has no save open). `file` is as
    /// `backups.txt` gave it; the restore checks it's one of that save's backups.
    Restore { mode: String, name: String, file: String },
    /// Show one of the app's tabs.
    Open { tab: String },
    /// Move a mod in the profile's library: before `before`, else to the end of `section` (a drag in the
    /// game's My mods). Quiet: PZMM doesn't come to the front for it, or start for it.
    Place {
        /// The mod (`mod` to the UI: `id` is the request's own).
        #[serde(rename = "mod")]
        id: String,
        section: String,
        before: Option<String>,
    },
    /// The library as the game has it after an Undo or Redo there: `# --- name`, `+id` and `-id` lines (the
    /// app keeps its own on and off). Quiet.
    Library { lines: Vec<String> },
    /// A section added under `to`, renamed to `to`, removed, or moved before section `to` (else to the end)
    /// (`op`: add, rename, remove, move). Quiet too.
    Section { op: String, name: String, to: Option<String> },
}

impl Request {
    /// A sorting change: PZMM doesn't come to the front for it, or start for it.
    fn quiet(&self) -> bool {
        matches!(self, Request::Place { .. } | Request::Section { .. } | Request::Library { .. } | Request::Tags { .. })
    }
}

/// A request as the game stamped it: `<at>-<seq>\t<profile>\t<action>\t<args>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stamped {
    /// The game's name for it (`<at>-<seq>`, `at` in seconds since 1970): its ack carries it back.
    pub id: String,
    /// The PZMM profile the game showed when it asked.
    pub profile: String,
    #[serde(flatten)]
    pub req: Request,
    /// The line as the game wrote it (queued as it is).
    #[serde(skip)]
    pub line: String,
}

/// What became of a request (`acks.txt`): applied, or not and why. The game's sorting edits show at once
/// and settle when every one is acked.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Ack {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub why: String,
}

/// A request that asks for PZMM's window older than this (seconds) is dropped: PZMM jumping up a minute
/// after you asked is worse than asking again.
const STALE: u64 = 60;
/// `acks.txt` keeps the newest this many.
const ACKS: usize = 200;

/// The tabs a request may open (the app's `tab` values).
const TABS: [&str; 10] = ["home", "mods", "feed", "workshop", "maps", "servers", "saves", "zb", "authoring", "settings"];
/// At most this many requests that ask something of the player (all but sorting edits) are taken at once (drags queue up while PZMM is closed); the rest of a
/// runaway file is dropped.
const MAX: usize = 100;

/// The watcher and the UI both move requests: one at a time.
static QUEUE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Move the game's new requests into the app's queue (`requests.queue`), at once while PZMM runs: the
/// game sees `requests.txt` gone, so it knows PZMM is up and doesn't start it. Returns whether there were
/// any, and whether any wants PZMM in front (all but quiet ones, and not stale ones).
pub fn queue_requests(paths: &Paths) -> Option<bool> {
    queue_requests_in(paths, &dir(paths))
}

/// `queue_requests` for bridge folder `dir`: the active one, or one a profile switch left that a game may still
/// have open (its requests are taken, and acked there, from then on).
pub fn queue_requests_in(paths: &Paths, dir: &std::path::Path) -> Option<bool> {
    let _one = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    if !same_dir(dir, &self::dir(paths)) {
        let mut earlier = EARLIER.lock().unwrap_or_else(|e| e.into_inner());
        if !earlier.iter().any(|(data, d)| *data == paths.data && d == dir) {
            earlier.push((paths.data.clone(), dir.to_path_buf()));
        }
    }
    // moved aside first, then read: a line the game appends meanwhile starts a new file, not a lost one. A
    // `requests.taken` left by a failed queue write is taken again first.
    let taken = dir.join("requests.taken");
    if !taken.exists() {
        let file = dir.join("requests.txt");
        // elsewhere a file still being written can be renamed: one untouched for a second is whole
        #[cfg(not(windows))]
        if !cfg!(test) && std::fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_none_or(|age| age.as_secs() < 1) {
            return None;
        }
        std::fs::rename(&file, &taken).ok()?;
    }
    // a byte that isn't UTF-8 (any mod can write here) spoils its line, not the batch
    let text = String::from_utf8_lossy(&std::fs::read(&taken).ok()?).into_owned();
    // aged now, when they're taken off the game: one PZMM took in time stays fresh while the UI is busy
    let (reqs, dropped) = parse(&text, Some(crate::store::now()), &crate::profile::active_name(paths));
    let kept: String = reqs.iter().map(|s| format!("{}\n", s.line)).collect();
    use std::io::Write;
    let queued = if kept.is_empty() { Ok(()) } else { std::fs::OpenOptions::new().create(true).append(true).open(dir.join("requests.queue")).and_then(|mut f| f.write_all(kept.as_bytes())) };
    // not queued: kept for the next look
    queued.ok()?;
    // queued: gone, or emptied when it can't go (a scanner has it open), so it's never queued twice
    if std::fs::remove_file(&taken).is_err() {
        let _ = std::fs::write(&taken, "");
    }
    write_acks(dir, &dropped);
    Some(reqs.iter().any(|s| !s.req.quiet()))
}

/// The game's requests, oldest first, from the active bridge folder and any earlier one still heard: each is
/// handed out once a session, and stays queued until it's acked, so one PZMM quit (or crashed) before
/// answering is handed out again on its next start. Stale ones and sorting edits made for another profile are
/// acked as not done here.
pub fn take_requests(paths: &Paths) -> Vec<Stamped> {
    queue_requests(paths);
    let _one = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let mut folders = vec![dir(paths)];
    for (data, d) in EARLIER.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        // one folder, however it's spelled, is read once
        if *data == paths.data && !folders.iter().any(|f| same_dir(f, d)) {
            folders.push(d.clone());
        }
    }
    let mut handed = HANDED.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = vec![];
    for from in folders {
        let text = String::from_utf8_lossy(&std::fs::read(from.join("requests.queue")).unwrap_or_default()).into_owned();
        // not the ones the UI has already
        let had = |l: &&str| handed.iter().any(|(data, d, id)| *data == paths.data && *d == from && l.split('\t').next() == Some(id.as_str()));
        let fresh: String = text.lines().filter(|l| !had(l)).map(|l| format!("{l}\n")).collect();
        // aged when they were queued
        let (reqs, dropped) = parse(&fresh, None, &crate::profile::active_name(paths));
        write_acks(&from, &dropped);
        unqueue(&from, &dropped);
        handed.extend(reqs.iter().map(|s| (paths.data.clone(), from.clone(), s.id.clone())));
        out.extend(reqs);
    }
    out
}

/// Tell the game what became of its requests (`acks.txt`, `<id>\t<ok|no>\t<why>` lines, the newest kept), in the
/// bridge folder each came from (a profile switch may have moved it since), and take them off the queue.
/// The overridden files the app counts (the game can't list folders deep): `overlaps.txt`, `<id>\t<loses to,...>\t<wins over,...>`
/// for each enabled mod sharing a file with another.
pub fn overlaps_for_game(paths: &Paths, overlaps: &std::collections::HashMap<String, super::files::Overlap>) {
    let d = dir(paths);
    if d.is_dir() {
        let mut ids: Vec<&String> = overlaps.keys().collect();
        ids.sort();
        let join = |s: &std::collections::BTreeSet<String>| s.iter().cloned().collect::<Vec<_>>().join(",");
        write_for_game(&d.join("overlaps.txt"), ids.iter().map(|id| format!("{id}\t{}\t{}\n", join(&overlaps[*id].loses_to), join(&overlaps[*id].wins_over))).collect::<String>());
    }
}

/// What PZMM writes for the game (app.txt aside): a game playing in another folder gets copies (see [`mirror`]).
const MIRRORED: [&str; 10] = ["library.txt", "order.txt", "overlaps.txt", "profiles.txt", "tags.txt", "findings.txt", "installed.txt", "categories.txt", "feed.txt", "servers.txt"];

/// Copy what PZMM writes for the game from `from` (the active bridge folder) to `to` (a game playing elsewhere), the
/// files that differ only.
pub fn mirror(from: &std::path::Path, to: &std::path::Path) {
    for f in MIRRORED {
        if let Ok(t) = std::fs::read(from.join(f))
            && std::fs::read(to.join(f)).ok().as_deref() != Some(t.as_slice())
        {
            write_for_game(&to.join(f), t);
        }
    }
}

pub fn ack(paths: &Paths, acks: &[Ack]) {
    let _one = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let handed = HANDED.lock().unwrap_or_else(|e| e.into_inner());
    let mut to: Vec<(std::path::PathBuf, Vec<Ack>)> = vec![];
    for a in acks {
        let from = handed.iter().find(|(data, _, id)| *data == paths.data && *id == a.id).map_or_else(|| dir(paths), |(_, d, _)| d.clone());
        match to.iter_mut().find(|(d, _)| *d == from) {
            Some((_, list)) => list.push(a.clone()),
            None => to.push((from, vec![a.clone()])),
        }
    }
    for (from, acks) in to {
        // a game playing elsewhere reads the result of its edits before their acks (else they snap back)
        if !same_dir(&from, &dir(paths)) {
            mirror(&dir(paths), &from);
        }
        write_acks(&from, &acks);
        unqueue(&from, &acks);
    }
}

/// The requests handed to the UI this session (PZMM's data folder, the bridge folder, the request's id).
static HANDED: std::sync::Mutex<Vec<(std::path::PathBuf, std::path::PathBuf, String)>> = std::sync::Mutex::new(Vec::new());

/// Bridge folders a profile switch left that requests were queued from (with PZMM's data folder).
static EARLIER: std::sync::Mutex<Vec<(std::path::PathBuf, std::path::PathBuf)>> = std::sync::Mutex::new(Vec::new());

/// Take the answered requests off `dir`'s queue (gone once it's empty).
fn unqueue(dir: &std::path::Path, acks: &[Ack]) {
    let queue = dir.join("requests.queue");
    let Ok(text) = std::fs::read_to_string(&queue) else { return };
    let left: String = text.lines().filter(|l| !acks.iter().any(|a| l.split('\t').next() == Some(a.id.as_str()))).map(|l| format!("{l}\n")).collect();
    if left.len() == text.len() {
        return;
    }
    if !left.is_empty() || std::fs::remove_file(&queue).is_err() {
        write_for_game(&queue, &left);
    }
}

/// Write a file the game reads: a rename fails on Windows while the game has the file open, so a few quick tries.
pub fn write_for_game(path: &std::path::Path, text: impl AsRef<[u8]>) {
    // ponytail: a few quick tries; a write still failing after them is lost until the next one
    for _ in 0..10 {
        if crate::write_atomic(path, text.as_ref()).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn write_acks(dir: &std::path::Path, acks: &[Ack]) {
    let file = dir.join("acks.txt");
    if acks.is_empty() || !file.parent().is_some_and(|d| d.is_dir()) {
        return;
    }
    let old = std::fs::read_to_string(&file).unwrap_or_default();
    let one_line = |s: &str| s.replace(['\t', '\r', '\n'], " ");
    let new = acks.iter().map(|a| format!("{}\t{}\t{}", one_line(&a.id), if a.ok { "ok" } else { "no" }, one_line(&a.why)));
    let lines: Vec<String> = old.lines().map(String::from).chain(new).collect();
    let text: String = lines[lines.len().saturating_sub(ACKS)..].iter().map(|l| format!("{l}\n")).collect();
    // an ack still unwritten after the tries is lost, and the game's edit waits for PZMM's next start
    write_for_game(&file, &text);
}

/// The requests in `text` still worth doing at `now` (None: don't age them) while `active` is the profile, and
/// acks for the rest.
fn parse(text: &str, now: Option<u64>, active: &str) -> (Vec<Stamped>, Vec<Ack>) {
    let (mut out, mut dropped): (Vec<Stamped>, Vec<Ack>) = (vec![], vec![]);
    for line in text.lines() {
        let mut parts = line.trim_end_matches('\r').splitn(4, '\t');
        let (id, profile, action, arg) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or("").trim());
        // `<at>-<seq>`: a request without its stamp can't be acked or aged, so it isn't one
        let Some(at) = id.split_once('-').filter(|(_, s)| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).and_then(|(a, _)| a.parse::<f64>().ok()).filter(|a| a.is_finite() && *a >= 0.0).map(|a| a as u64) else { continue };
        if profile.len() > 200 {
            continue;
        }
        let req = match action {
            "install" => {
                let ids: Vec<String> = arg.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
                // Workshop IDs are digits; anything else in the list spoils the request
                (!ids.is_empty() && ids.len() <= 2000 && ids.iter().all(|i| i.len() <= 20 && i.bytes().all(|b| b.is_ascii_digit()))).then_some(Request::Install { ids })
            }
            "backup" => arg.split_once('/').filter(|(m, n)| plain(m) && plain(n)).map(|(m, n)| Request::Backup { mode: m.into(), name: n.into() }),
            "check" => Some(Request::Check),
            "profile" => named(arg).then(|| Request::Profile { name: arg.into() }),
            "tags" => {
                let (id, list) = arg.split_once('\t').unwrap_or((arg, ""));
                let tags: Vec<String> = list.split(';').map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect();
                (named(id) && tags.len() <= 50 && tags.iter().all(|t| t.chars().count() <= 40 && !t.chars().any(char::is_control))).then(|| Request::Tags { id: id.into(), tags })
            }
            "fix" => (!arg.is_empty() && arg.len() <= 20_000 && !arg.chars().any(char::is_control)).then(|| Request::Fix { fix: arg.into() }),
            "restore" => arg
                .split_once('\t')
                .and_then(|(save, file)| Some((save.split_once('/')?, file)))
                .filter(|((m, n), file)| plain(m) && plain(n) && !file.is_empty() && file.len() <= 1000 && !file.chars().any(char::is_control))
                .map(|((m, n), file)| Request::Restore { mode: m.into(), name: n.into(), file: file.into() }),
            "library" => {
                let lines: Vec<String> = arg.split('\t').map(String::from).collect();
                let line_ok = |l: &String| l.strip_prefix("# ---").map(|n| n.trim().is_empty() || named(n.trim())).unwrap_or_else(|| l.strip_prefix(['+', '-']).is_some_and(named));
                (lines.len() <= 5000 && lines.iter().all(line_ok)).then_some(Request::Library { lines })
            }
            "open" => TABS.contains(&arg).then(|| Request::Open { tab: arg.into() }),
            "section" => {
                let mut f = arg.split('\t');
                let (op, name, to) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
                let ok = named(name)
                    && match op {
                        "add" | "rename" => named(to),
                        "remove" => to.is_empty(),
                        // before section `to`, or to the end
                        "move" => to.is_empty() || named(to),
                        _ => false,
                    };
                ok.then(|| Request::Section { op: op.into(), name: name.into(), to: (!to.is_empty()).then(|| to.into()) })
            }
            "place" => {
                let mut f = arg.split('\t');
                let (id, section, before) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
                (named(id) && named(section) && (before.is_empty() || named(before))).then(|| Request::Place {
                    id: id.into(),
                    section: section.into(),
                    before: (!before.is_empty()).then(|| before.into()),
                })
            }
            _ => None,
        };
        let no = |why: &str| Ack { id: id.into(), ok: false, why: why.into() };
        let Some(req) = req else {
            dropped.push(no("PZMM couldn't read that request."));
            continue;
        };
        if (!req.quiet() && out.iter().filter(|s| !s.req.quiet()).count() >= MAX) || out.len() >= 5000 {
            dropped.push(no("Too many requests at once. Ask again."));
        } else if !req.quiet() && now.is_some_and(|now| now.saturating_sub(at) > STALE) {
            dropped.push(no("PZMM wasn't running in time. Ask again."));
        } else if req.quiet() && profile != active {
            dropped.push(no(&format!("PZMM's profile is now {active}.")));
        } else if req.quiet() && out.last().is_some_and(|l| l.req == req) {
            // the same sorting edit twice in a row is one (a loud one, a backup say, is answered when it's done:
            // the game may act on its ok)
            dropped.push(Ack { id: id.into(), ok: true, why: String::new() });
        } else {
            out.push(Stamped { id: id.into(), profile: profile.into(), req, line: line.trim_end_matches('\r').into() });
        }
    }
    (out, dropped)
}

/// A mod ID or section name: a line of text.
fn named(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 200 && !s.chars().any(char::is_control)
}

/// A save folder's part: a name, not a path.
fn plain(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && s != "." && s != ".." && !s.contains(['/', '\\', ':'])
}

/// Every mod's category (`of`, from `category::categorize`) for the game's Group by category:
/// `categories.txt`, the sections in order on its first line (`order\t...`), then `<id>\t<category>` lines.
/// Only where PZMM - Client is.
pub fn categories_for_game(paths: &Paths, of: &std::collections::BTreeMap<String, String>) {
    let dir = crate::pz::bridge::dir(paths);
    if !dir.is_dir() {
        return;
    }
    let mut text = format!("order\t{}\n", super::category::ORDER.join("\t"));
    for (id, c) in of {
        text += &format!("{id}\t{c}\n");
    }
    write_for_game(&dir.join("categories.txt"), text);
}

/// An activity line shared with the game: `<secs>\t<kind>\t<text>` (kind: info, error, install, update, remove).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub at: u64,
    pub kind: String,
    pub text: String,
}

/// Each side's activity file keeps the newest this many lines.
const ACTIVITY: usize = 200;
const KINDS: [&str; 5] = ["info", "error", "install", "update", "remove"];

fn read_activity(file: &std::path::Path) -> Vec<Activity> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut f = l.trim_end_matches('\r').splitn(3, '\t');
            let at = f.next()?.trim().parse::<f64>().ok().filter(|a| a.is_finite() && *a >= 0.0)? as u64;
            let kind = f.next()?;
            let text = f.next()?.trim();
            (!text.is_empty()).then(|| Activity { at, kind: if KINDS.contains(&kind) { kind.into() } else { "info".into() }, text: text.into() })
        })
        .collect()
}

/// Add the app's lines that concern the game to `activity_app.txt` (oldest first, the newest kept), where
/// PZMM - Client is. Lines keep their own times, so the file never goes back in time.
pub fn activity_for_game(paths: &Paths, new: &[Activity]) {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let file = crate::pz::bridge::dir(paths).join("activity_app.txt");
    if new.is_empty() || !file.parent().is_some_and(|d| d.is_dir()) {
        return;
    }
    let mut lines = read_activity(&file);
    let one_line = |s: &str| s.replace(['\t', '\r', '\n'], " ");
    lines.extend(new.iter().map(|a| Activity { text: one_line(&a.text), ..a.clone() }));
    // stable: lines of one second keep the order they came in
    lines.sort_by_key(|a| a.at);
    let text: String = lines[lines.len().saturating_sub(ACTIVITY)..].iter().map(|a| format!("{}\t{}\t{}\n", a.at, a.kind, a.text)).collect();
    write_for_game(&file, text);
}

/// The game's own activity (`activity_game.txt`, which PZMM - Client rewrites), oldest first.
pub fn game_activity(paths: &Paths) -> Vec<Activity> {
    read_activity(&crate::pz::bridge::dir(paths).join("activity_game.txt"))
}

/// Check setup's findings for the PZMM - Client mod's Problems: `findings.txt`, `at=<secs>` then the UI's lines
/// (`<level>\t<mod or ->\t<text>`, then `\t<fix id>\t<fix label>` pairs). Only where PZMM - Client is.
pub fn findings_for_game(paths: &Paths, lines: &[String]) {
    let dir = crate::pz::bridge::dir(paths);
    if !dir.is_dir() {
        return;
    }
    let mut text = format!("at={}\n", crate::store::now());
    for l in lines.iter().take(500) {
        text += &l.replace(['\r', '\n'], " ");
        text.push('\n');
    }
    write_for_game(&dir.join("findings.txt"), text);
}

/// PZMM - Client's own options (its Settings tab), shared with the app's Settings: `client.txt`, `key=1` or `key=0`,
/// with each one's default when the file doesn't say.
pub const CLIENT_KEYS: [(&str, bool); 5] = [("logo", true), ("watermark", true), ("news", true), ("tips", true), ("large", false)];

/// Every `key=0|1` in `client.txt` (the game may have keys this app doesn't know yet).
fn client_file(paths: &Paths) -> std::collections::BTreeMap<String, bool> {
    let text = std::fs::read_to_string(crate::pz::bridge::dir(paths).join("client.txt")).unwrap_or_default();
    text.lines().filter_map(|l| l.trim().split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim() != "0")).collect()
}

/// PZMM - Client's options the app shows, the file's or their defaults. None before the mod has run here.
pub fn client_settings(paths: &Paths) -> Option<std::collections::BTreeMap<String, bool>> {
    let dir = dir(paths);
    if !dir.join("client.txt").exists() && !dir.join("game.json").exists() {
        return None;
    }
    let file = client_file(paths);
    Some(CLIENT_KEYS.iter().map(|(k, on)| (k.to_string(), file.get(*k).copied().unwrap_or(*on))).collect())
}

/// Change PZMM - Client's options the app knows (others in `set` are ignored; the file's other keys stay).
pub fn set_client_settings(paths: &Paths, set: &std::collections::BTreeMap<String, bool>) -> anyhow::Result<()> {
    let Some(shown) = client_settings(paths) else { anyhow::bail!("PZMM - Client hasn't run in the game yet") };
    let mut now = client_file(paths);
    now.extend(shown);
    for (k, v) in set.iter().filter(|(k, _)| CLIENT_KEYS.iter().any(|(c, _)| c == k)) {
        now.insert(k.clone(), *v);
    }
    let text: String = now.iter().map(|(k, v)| format!("{k}={}\n", u8::from(*v))).collect();
    Ok(crate::write_atomic(&crate::pz::bridge::dir(paths).join("client.txt"), text)?)
}

/// What the game last reported (`game.json`, written at the main menu and when a game starts).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GameReport {
    #[serde(default)]
    pub client: String,
    #[serde(default)]
    pub game: String,
    /// "menu" or "game".
    #[serde(default)]
    pub state: String,
    /// When, in seconds since 1970 (the game's os.time() may have a fraction).
    #[serde(default, deserialize_with = "whole_seconds")]
    pub at: u64,
    /// The mods the game loaded, in its order.
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub save: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    /// The Zomboid folder the game uses (`Core.getMyDocumentFolder()`).
    #[serde(default)]
    pub zomboid: String,
    /// The game's Zomboid folder isn't the one PZMM manages: what PZMM changes, the game doesn't see.
    #[serde(default, skip_deserializing)]
    pub elsewhere: bool,
}

fn whole_seconds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let t = f64::deserialize(d)?;
    Ok(if t.is_finite() && t > 0.0 { t as u64 } else { 0 })
}

/// The game's last report, if PZMM - Client has written one: in PZMM's Zomboid folder, or the usual one
/// when PZMM is set to another (the newer of the two).
pub fn game_report(paths: &Paths) -> Option<GameReport> {
    let read = |z: &std::path::Path| -> Option<GameReport> { serde_json::from_str(&std::fs::read_to_string(z.join("Lua").join("pzmm").join("game.json")).ok()?).ok() };
    // the Zomboid folder the game plays in (the active profile's own, if it has one)
    let played = dir(paths).parent().and_then(|l| l.parent()).map_or_else(|| paths.zomboid.clone(), std::path::Path::to_path_buf);
    // also PZMM's own Zomboid folder (a game started without the profile's folder) and the usual one
    let mut folders = vec![played.clone()];
    for z in [Some(paths.zomboid.clone()), if cfg!(test) { None } else { dirs::home_dir().map(|h| h.join("Zomboid")) }].into_iter().flatten() {
        if !folders.iter().any(|f| same_dir(f, &z)) {
            folders.push(z);
        }
    }
    let mut r = folders.iter().filter_map(|z| read(z)).max_by_key(|r| r.at)?;
    r.elsewhere = !r.zomboid.is_empty() && !same_dir(std::path::Path::new(&r.zomboid), &played);
    Some(r)
}

pub fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.to_string_lossy().trim_end_matches(['/', '\\']).eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['/', '\\'])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_792_000_000;

    /// `lines` stamped as the game does: now, for the Default profile.
    fn stamped(lines: &str) -> String {
        lines.lines().enumerate().map(|(i, l)| format!("{NOW}-{i}\tDefault\t{l}\n")).collect()
    }

    fn reqs(text: &str) -> Vec<Request> {
        parse(&stamped(text), Some(NOW), "Default").0.into_iter().map(|s| s.req).collect()
    }

    #[test]
    fn requests_are_checked_and_trimmed() {
        let text = "install\t123,456\r\ninstall\t12a\nbackup\tSurvivor/2026-10-14_09-28-10\nbackup\tSurvivor/../x\nbackup\tnoslash\ncheck\ncheck\nopen\tsaves\nopen\tC:/evil\nwipe\tall\n";
        assert_eq!(
            reqs(text),
            vec![
                Request::Install { ids: vec!["123".into(), "456".into()] },
                Request::Backup { mode: "Survivor".into(), name: "2026-10-14_09-28-10".into() },
                Request::Check,
                Request::Check,
                Request::Open { tab: "saves".into() },
            ]
        );
        // a sorting edit twice in a row is one; a loud request (the game may act on its ok) never is
        assert_eq!(reqs("place\tA\tMaps\nplace\tA\tMaps\nbackup\tS/a\nbackup\tS/a\n").len(), 3);
    }

    #[test]
    fn stale_and_other_profiles_requests_are_acked_not_done() {
        let text = format!(
            "{old}-1\tDefault\topen\tsaves\n{old}-2\tDefault\tplace\tA\tMaps\n{NOW}-3\tOther\tplace\tA\tMaps\n{NOW}-4\tOther\tcheck\nopen\tsaves\nx-5\tDefault\tcheck\n{NOW}-6\tDefault\tcheck\n",
            old = NOW - STALE - 1
        );
        let (ok, no) = parse(&text, Some(NOW), "Default");
        let ids: Vec<&str> = ok.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, [format!("{}-2", NOW - STALE - 1), format!("{NOW}-4"), format!("{NOW}-6")], "an old drag still counts; a loud request for another profile too");
        let no: Vec<(&str, bool)> = no.iter().map(|a| (a.id.as_str(), a.ok)).collect();
        assert_eq!(no, [(format!("{}-1", NOW - STALE - 1).as_str(), false), (format!("{NOW}-3").as_str(), false)]);
    }

    #[test]
    fn acks_keep_the_newest() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..ACKS + 5 {
            ack(&paths, &[Ack { id: format!("1-{i}"), ok: i % 2 == 0, why: "no\tway\n".into() }]);
        }
        let text = std::fs::read_to_string(dir.join("acks.txt")).unwrap();
        assert_eq!(text.lines().count(), ACKS);
        assert_eq!(text.lines().last(), Some(format!("1-{}\tok\tno way ", ACKS + 4).as_str()));
    }

    #[test]
    fn requests_are_taken_once() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        let now = crate::store::now();
        std::fs::write(dir.join("requests.txt"), format!("{now}-1\tDefault\tcheck\n{}-2\tDefault\tcheck\n", now - STALE - 1)).unwrap();
        assert_eq!(take_requests(&paths).into_iter().map(|s| s.req).collect::<Vec<_>>(), vec![Request::Check]);
        assert_eq!(take_requests(&paths), vec![]);
        assert!(!dir.join("requests.taken").exists());
        assert!(std::fs::read_to_string(dir.join("acks.txt")).unwrap().contains(&format!("{}-2\tno\t", now - STALE - 1)), "the stale one is acked");
    }

    #[test]
    fn queued_requests_wait_for_the_ui() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        let now = crate::store::now();
        let line = |l: &str| format!("{now}-1\tDefault\t{l}\n");
        assert_eq!(queue_requests(&paths), None, "nothing asked");
        std::fs::write(dir.join("requests.txt"), line("place\tA\tGameplay\t")).unwrap();
        assert_eq!(queue_requests(&paths), Some(false), "a drag doesn't call PZMM to the front");
        std::fs::write(dir.join("requests.txt"), format!("{}-1\tDefault\topen\tmods\n", now - STALE - 1)).unwrap();
        assert_eq!(queue_requests(&paths), Some(false), "nor does a stale request");
        std::fs::write(dir.join("requests.txt"), line("open\tsaves")).unwrap();
        assert_eq!(queue_requests(&paths), Some(true));
        assert!(!dir.join("requests.txt").exists(), "the game sees PZMM took it");
        std::fs::write(dir.join("requests.txt"), line("check")).unwrap();
        assert_eq!(take_requests(&paths).into_iter().map(|s| s.req).collect::<Vec<_>>(), vec![
            Request::Place { id: "A".into(), section: "Gameplay".into(), before: None },
            Request::Open { tab: "saves".into() },
            Request::Check
        ]);
        assert_eq!(take_requests(&paths), vec![], "handed out once");
        assert!(dir.join("requests.queue").exists(), "kept until they're answered");
        ack(&paths, &[Ack { id: format!("{now}-1"), ok: true, why: String::new() }]);
        assert!(!dir.join("requests.queue").exists());
        // queued in time, it's done however long the UI takes; what can't be read is answered all the same
        std::fs::write(dir.join("requests.queue"), format!("{}-9\tDefault\tcheck\n", now - STALE - 100)).unwrap();
        std::fs::write(dir.join("requests.txt"), format!("{now}-2\tDefault\twipe\tall\n{now}-3\tDefault\topen")).unwrap();
        assert_eq!(take_requests(&paths).into_iter().map(|s| s.req).collect::<Vec<_>>(), vec![Request::Check]);
        let acks = std::fs::read_to_string(dir.join("acks.txt")).unwrap();
        for id in [format!("{}-1", now - STALE - 1), format!("{now}-2"), format!("{now}-3")] {
            assert!(acks.contains(&format!("{id}\tno\t")), "{id} answered: {acks}");
        }
    }

    #[test]
    fn unanswered_requests_survive_a_restart() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        let now = crate::store::now();
        std::fs::write(dir.join("requests.txt"), format!("{now}-1\tDefault\tcheck\n{now}-2\tDefault\topen\tsaves\n")).unwrap();
        assert_eq!(take_requests(&paths).len(), 2);
        ack(&paths, &[Ack { id: format!("{now}-1"), ok: true, why: String::new() }]);
        assert_eq!(std::fs::read_to_string(dir.join("requests.queue")).unwrap(), format!("{now}-2\tDefault\topen\tsaves\n"), "the answered one is off the queue");
        // PZMM quits before answering the other: its next start hands it out again
        HANDED.lock().unwrap().retain(|(data, _, _)| *data != paths.data);
        assert_eq!(take_requests(&paths).into_iter().map(|s| s.id).collect::<Vec<_>>(), [format!("{now}-2")]);
    }

    #[test]
    fn a_write_for_the_game_waits_out_a_brief_lock() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let file = paths.data.join("x.txt");
        std::fs::create_dir_all(&paths.data).unwrap();
        // a folder in the way stands in for the game holding the file: freed after a few tries, the write lands
        std::fs::create_dir(&file).unwrap();
        let free = std::thread::spawn({
            let file = file.clone();
            move || {
                std::thread::sleep(std::time::Duration::from_millis(120));
                std::fs::remove_dir(&file).unwrap();
            }
        });
        write_for_game(&file, "new");
        free.join().unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
    }

    #[test]
    fn places_name_a_mod_and_a_section() {
        assert_eq!(
            reqs("place\tA\tFrameworks & libraries\tB\nplace\t\tGameplay\nplace\tA\t\nplace\tA\tMaps\n"),
            vec![
                Request::Place { id: "A".into(), section: "Frameworks & libraries".into(), before: Some("B".into()) },
                Request::Place { id: "A".into(), section: "Maps".into(), before: None },
            ]
        );
    }

    #[test]
    fn section_changes_are_checked() {
        assert_eq!(
            reqs("section\tadd\tMaps\tGameplay\nsection\trename\tMaps\t\nsection\tremove\tMaps\nsection\twipe\tMaps\nsection\tmove\tMaps\tGameplay\nsection\tmove\tMaps\n"),
            vec![
                Request::Section { op: "add".into(), name: "Maps".into(), to: Some("Gameplay".into()) },
                Request::Section { op: "remove".into(), name: "Maps".into(), to: None },
                Request::Section { op: "move".into(), name: "Maps".into(), to: Some("Gameplay".into()) },
                Request::Section { op: "move".into(), name: "Maps".into(), to: None },
            ]
        );
    }

    #[test]
    fn a_library_is_lines_of_sections_and_mods() {
        assert_eq!(
            reqs("library\t# --- Maps\t+A\t-B\t# ---\t+C\nlibrary\t# --- Maps\tC:/evil\n"),
            vec![Request::Library { lines: vec!["# --- Maps".into(), "+A".into(), "-B".into(), "# ---".into(), "+C".into()] }]
        );
    }

    #[test]
    fn categories_reach_the_game() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let of = [("A".to_string(), "Maps".to_string())].into_iter().collect();
        categories_for_game(&paths, &of);
        assert!(!paths.zomboid.join("Lua/pzmm/categories.txt").exists(), "no client mod: nothing written");
        std::fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        categories_for_game(&paths, &of);
        let text = std::fs::read_to_string(paths.zomboid.join("Lua/pzmm/categories.txt")).unwrap();
        assert!(text.starts_with("order\tFrameworks & libraries\tGameplay\t") && text.ends_with("\nA\tMaps\n"), "{text}");
    }

    #[test]
    fn a_runaway_file_is_cut_short() {
        let text = (0..200).map(|i| format!("install\t{i}\n")).collect::<String>();
        assert_eq!(reqs(&text).len(), MAX);
    }

    #[test]
    fn a_place_keeps_its_own_id_apart_from_the_mods() {
        let (reqs, _) = parse("100-1\tP\tplace\tModA\tMaps\t\n100-2\tP\tprofile\tOther one\n100-3\tP\tprofile\t\n", Some(100), "P");
        let json = serde_json::to_value(&reqs).unwrap();
        assert_eq!((json[0]["id"].as_str(), json[0]["mod"].as_str()), (Some("100-1"), Some("ModA")));
        assert_eq!(reqs[1].req, Request::Profile { name: "Other one".into() });
        assert_eq!(reqs.len(), 2, "a profile needs a name");
        let (t, _) = parse("100-1\tP\ttags\tModA\tKeep; Fun ;\n100-2\tP\ttags\tModB\n", Some(100), "P");
        assert_eq!(t.iter().map(|s| s.req.clone()).collect::<Vec<_>>(), vec![
            Request::Tags { id: "ModA".into(), tags: vec!["Keep".into(), "Fun".into()] },
            Request::Tags { id: "ModB".into(), tags: vec![] },
        ], "none clears them");
        assert!(t[0].req.quiet());
        let (f, _) = parse("100-1\tP\tfix\tonly42|A,B|turnOff\n100-2\tP\tfix\t\n", Some(100), "P");
        assert_eq!(f.iter().map(|s| s.req.clone()).collect::<Vec<_>>(), vec![Request::Fix { fix: "only42|A,B|turnOff".into() }]);
        let (r, _) = parse("100-1\tP\trestore\tSurvivor/Rising\tC:\\data\\b.zip\n100-2\tP\trestore\tSurvivor/..\tx.zip\n100-3\tP\trestore\tSurvivor/Rising\n", Some(100), "P");
        assert_eq!(r.iter().map(|s| s.req.clone()).collect::<Vec<_>>(), vec![Request::Restore { mode: "Survivor".into(), name: "Rising".into(), file: "C:\\data\\b.zip".into() }]);
        assert!(!reqs[1].req.quiet());
    }

    #[test]
    fn activity_goes_both_ways() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        let line = |at, text: &str| Activity { at, kind: "info".into(), text: text.into() };
        activity_for_game(&paths, &[line(20, "b\tc"), line(10, "a")]);
        activity_for_game(&paths, &(0..250).map(|i| line(30, &i.to_string())).collect::<Vec<_>>());
        let ours = read_activity(&dir.join("activity_app.txt"));
        assert_eq!((ours.len(), ours[0].text.as_str(), ours[199].text.as_str()), (200, "50", "249"), "oldest first, the newest kept");
        std::fs::write(dir.join("activity_game.txt"), "1792000000.4\tinstall\tDragged A\nbad line\n5\tweird\tB\n").unwrap();
        assert_eq!(game_activity(&paths), vec![Activity { at: 1792000000, kind: "install".into(), text: "Dragged A".into() }, line(5, "B")]);
        activity_for_game(&paths, &[line(1, "x")]);
        assert!(read_activity(&dir.join("activity_app.txt")).iter().all(|a| a.text != "x"), "sorted by time: older than the newest 200, it goes");
    }

    #[test]
    fn the_client_mods_options_are_shared() {
        let (_t, paths) = crate::store::tests::temp_paths();
        assert_eq!(client_settings(&paths), None, "the mod hasn't run");
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(client_settings(&paths), None, "the folder PZMM makes isn't the mod");
        std::fs::write(dir.join("game.json"), "{}").unwrap();
        let s = client_settings(&paths).unwrap();
        assert_eq!((s["logo"], s["tips"], s["large"]), (true, true, false), "the defaults");
        std::fs::write(dir.join("client.txt"), "tips=0\nlogo=1\nnewer=1\n").unwrap();
        let s = client_settings(&paths).unwrap();
        assert_eq!((s["tips"], s["logo"], s["news"]), (false, true, true));
        set_client_settings(&paths, &[("news".to_string(), false), ("evil".to_string(), true)].into()).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("client.txt")).unwrap(), "large=0\nlogo=1\nnewer=1\nnews=0\ntips=0\nwatermark=1\n", "a key the app doesn't know stays");
    }

    #[test]
    fn reads_what_the_client_mod_writes() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        let json = |z: &str| format!(r#"{{"v":1,"client":"0.2.0","game":"42.21.0","state":"menu","at":1792000000,"mods":["A","B"],"zomboid":{}}}"#, serde_json::to_string(z).unwrap());
        std::fs::write(dir.join("game.json"), json(&paths.zomboid.to_string_lossy())).unwrap();
        let r = game_report(&paths).unwrap();
        assert_eq!((r.state.as_str(), r.mods.len(), r.save, r.elsewhere), ("menu", 2, None, false));
        std::fs::write(dir.join("game.json"), json("D:/Elsewhere/Zomboid")).unwrap();
        assert!(game_report(&paths).unwrap().elsewhere);
        let r: GameReport = serde_json::from_str(r#"{"at":1792000000.75}"#).unwrap();
        assert_eq!(r.at, 1792000000, "os.time() may have a fraction");
    }
}
