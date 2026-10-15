//! What changed since you last played: the Workshop updates to your mods, with their change notes,
//! and a hint when one may break a save in progress.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{Paths, launcher, maps, saves};
use crate::sources::{self, steam_workshop::{self, ChangeNote}};
use crate::store;

/// Change-note pages fetched per call at most (the rest wait for "load more"): Steam serves one
/// item's notes per request.
const FETCH: usize = 40;
const DAY: u64 = 24 * 60 * 60;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct State {
    /// The last time PZMM started the game.
    last_launch: u64,
    /// Fetched change notes by Workshop item.
    notes: BTreeMap<String, Cached>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Cached {
    /// The item's Workshop update time when they were fetched.
    updated: u64,
    /// They reach back to this time (and include the first note at or before it).
    since: u64,
    notes: Vec<ChangeNote>,
}

fn file(paths: &Paths) -> PathBuf {
    paths.data.join("feed.json")
}

fn load(paths: &Paths) -> State {
    std::fs::read_to_string(file(paths)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(paths: &Paths, s: &State) -> Result<()> {
    std::fs::create_dir_all(&paths.data)?;
    Ok(crate::write_atomic(&file(paths), serde_json::to_string(s)?)?)
}

/// PZMM is starting the game now.
pub fn record_launch(paths: &Paths) -> Result<()> {
    let mut s = load(paths);
    s.last_launch = store::now();
    save(paths, &s)
}

/// When you last played (Unix seconds) and how PZMM knows: "launch" (PZMM started the game or
/// joined a server), "save" (a save's last write) or "" (no sign of play: 0).
pub fn last_played(paths: &Paths) -> (u64, &'static str) {
    let joined = launcher::state(paths).joins.values().flatten().map(|j| j.time).max().unwrap_or(0);
    // a game started without PZMM: PZMM - Client notes it in `played.txt`
    let launch = load(paths).last_launch.max(joined).max(game_number(paths, "played.txt"));
    let save = saves::list(paths).first().map_or(0, |s| s.modified);
    match (launch, save) {
        (0, 0) => (0, ""),
        _ if launch >= save => (launch, "launch"),
        _ => (save, "save"),
    }
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Entry {
    /// The Workshop item.
    pub item: String,
    pub title: String,
    /// The installed mods it holds.
    pub mods: Vec<String>,
    /// Its Workshop update time.
    pub updated: u64,
    /// Change notes since you last played, newest first.
    pub notes: Vec<ChangeNote>,
    /// The notes were fetched (false: over this call's limit, or Steam failed).
    pub loaded: bool,
    /// The mod's own changelog, when Steam has no notes for the update.
    pub bundled: Option<Changelog>,
    /// Why it may break a save in progress: "map" (it adds a map), "version" (a major version
    /// jump), "notes" (its notes talk about new games or wiped saves).
    pub risks: Vec<&'static str>,
    /// The version jump the notes show, e.g. "1.4.3" → "2.0".
    pub from: String,
    pub to: String,
    /// PZMM installed it and hasn't installed this update yet.
    pub pending: bool,
}

impl Entry {
    /// It has change notes with text, or the mod's own changelog.
    fn has_notes(&self) -> bool {
        self.notes.iter().any(|n| !n.text.trim().is_empty()) || self.bundled.as_ref().is_some_and(|b| !b.entries.is_empty())
    }
    /// Its notes are loaded and there's nothing in them (one not loaded yet may still have some).
    fn nothing_to_read(&self) -> bool {
        self.loaded && !self.has_notes()
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Feed {
    /// Updates after this time are shown: when you last played, else the last week.
    pub since: u64,
    /// How `since` was found: "launch", "save" or "" (the last week).
    pub basis: &'static str,
    /// Updates with notes, then those without; newest first within each.
    pub entries: Vec<Entry>,
    /// Entries whose notes aren't loaded yet.
    pub unloaded: usize,
    /// Why notes stopped loading (Steam refused or failed), if they did.
    pub error: String,
    /// The newest update seen in the game's What's new (PZMM - Client's `feedseen.txt`): older ones are seen.
    pub seen: u64,
}

/// A number PZMM - Client keeps in `Zomboid/Lua/pzmm/<name>` (0 when there's none).
fn game_number(paths: &Paths, name: &str) -> u64 {
    std::fs::read_to_string(crate::pz::bridge::dir(paths).join(name)).ok().and_then(|t| t.trim().parse::<f64>().ok()).filter(|n| n.is_finite() && *n > 0.0).map_or(0, |n| n as u64)
}

/// What's new was read up to the update at `at` (in the app): the game's badge clears too.
pub fn set_seen(paths: &Paths, at: u64) {
    let dir = crate::pz::bridge::dir(paths);
    if dir.is_dir() && at > game_number(paths, "feedseen.txt") {
        super::bridge::write_for_game(&dir.join("feedseen.txt"), at.to_string());
    }
}

/// The Workshop items among `mods` updated since you last played. With `notes`, also their
/// change notes: cached per item and update, at most [`FETCH`] new pages per call.
pub fn feed(paths: &Paths, mods: &BTreeMap<String, store::Row>, notes: bool) -> Result<Feed> {
    let (played, basis) = last_played(paths);
    let since = if played > 0 { played } else { store::now().saturating_sub(7 * DAY) };
    let mut items: BTreeMap<&str, Vec<&store::Row>> = BTreeMap::new();
    for row in mods.values() {
        if let Some(item) = sources::workshop_item(&row.source) {
            items.entry(item).or_default().push(row);
        }
    }
    let ids: Vec<&str> = items.keys().copied().collect();
    let mut updated = vec![];
    for chunk in ids.chunks(100) {
        updated.extend(steam_workshop::details(chunk)?.into_iter().filter(|d| d.time_updated > since));
    }
    updated.sort_by_key(|d| std::cmp::Reverse(d.time_updated));

    let index = store::load(paths)?;
    let mut state = load(paths);
    let (mut fetched, mut error) = (0, String::new());
    let mut out = Feed { since, basis, seen: game_number(paths, "feedseen.txt"), ..Default::default() };
    for d in updated {
        let rows = &items[d.id.as_str()];
        let cached = state.notes.get(&d.id).filter(|c| c.updated == d.time_updated && c.since <= since).map(|c| c.notes.clone());
        let all = match cached {
            Some(n) => Some(n),
            None if notes && fetched < FETCH && error.is_empty() => {
                fetched += 1;
                match steam_workshop::change_notes_back_to(&d.id, since) {
                    Ok(n) => {
                        state.notes.insert(d.id.clone(), Cached { updated: d.time_updated, since, notes: n.clone() });
                        Some(n)
                    }
                    Err(e) => {
                        error = format!("{e:#}");
                        None
                    }
                }
            }
            None => None,
        };
        let map = rows.iter().any(|r| [41, 42].iter().any(|b| !maps::folder_names(&r.dir, *b).is_empty()));
        let mut e = assess(all.unwrap_or_default(), since, map);
        e.loaded = state.notes.get(&d.id).is_some_and(|c| c.updated == d.time_updated && c.since <= since);
        if e.loaded && e.notes.is_empty() {
            e.bundled = rows.iter().find_map(|r| bundled_changelog(r));
        }
        out.unloaded += usize::from(!e.loaded);
        e.pending = rows.iter().any(|r| {
            index.get(&r.id).is_some_and(|i| i.source.starts_with("workshop:") && sources::installed_at(paths, &r.id, i) < d.time_updated)
        });
        e.mods = rows.iter().map(|r| r.id.clone()).collect();
        e.item = d.id;
        e.title = d.title;
        e.updated = d.time_updated;
        out.entries.push(e);
    }
    // updates with something to read (or not loaded yet) first, newest first within each
    out.entries.sort_by_key(Entry::nothing_to_read);
    out.error = error;
    // forget items you no longer have
    state.notes.retain(|k, _| items.contains_key(k.as_str()));
    save(paths, &state)?;
    for_game(paths, &out);
    Ok(out)
}

/// What's new for the PZMM - Client mod's in-game screen: `Zomboid/Lua/pzmm/feed.txt`, where that
/// folder is, on every feed load. Game Lua reads lines easily (no JSON): `since=`, `basis=`, `at=` (when it was
/// written), then per entry a tab-separated
/// `entry` line (item, updated, pending, risks, from, to, title, loaded), `mods`, and its `note`s (time, text)
/// or the mod's own changelog as `log`s (title, text); newlines in text as `\n`.
fn for_game(paths: &Paths, f: &Feed) {
    let dir = crate::pz::bridge::dir(paths);
    if dir.is_dir() {
        super::bridge::write_for_game(&dir.join("feed.txt"), game_text(f));
    }
}

fn game_text(f: &Feed) -> String {
    let flat = |s: &str| s.replace('\\', "\\\\").replace('\r', "").replace('\n', "\\n").replace('\t', " ");
    let mut out = format!("since={}\nbasis={}\nat={}\n", f.since, f.basis, store::now());
    for e in &f.entries {
        out += &format!("entry\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", e.item, e.updated, u8::from(e.pending), e.risks.join(","), flat(&e.from), flat(&e.to), flat(&e.title), u8::from(e.loaded));
        out += &format!("mods\t{}\n", e.mods.join(","));
        for n in &e.notes {
            out += &format!("note\t{}\t{}\n", n.time, flat(&n.text));
        }
        for l in e.bundled.iter().flat_map(|b| &b.entries) {
            out += &format!("log\t{}\t{}\n", flat(&l.title), flat(&l.text));
        }
    }
    out
}

/// An entry's notes and risks from `all` (newer than `since`, then possibly the one before).
fn assess(all: Vec<ChangeNote>, since: u64, map: bool) -> Entry {
    let (notes, before): (Vec<_>, Vec<_>) = all.into_iter().partition(|n| n.time > since);
    let to = notes.iter().find_map(|n| version_in(&n.text)).unwrap_or_default();
    let from = before.first().and_then(|n| version_in(&n.text)).unwrap_or_default();
    let mut risks = vec![];
    if map {
        risks.push("map");
    }
    if major_jump(&from, &to) {
        risks.push("version");
    }
    if notes.iter().any(|n| warns_saves(&n.text)) {
        risks.push("notes");
    }
    let jump = !from.is_empty() && !to.is_empty() && from != to;
    Entry { notes, risks, from: if jump { from } else { String::new() }, to: if jump { to } else { String::new() }, ..Default::default() }
}

/// The first version number in a note: `v1.4`, `2.0.3`, `Version 3.1` (at least one dot).
fn version_in(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for (i, c) in b.iter().enumerate() {
        let starts = c.is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric() || (matches!(b[i - 1], b'v' | b'V') && (i < 2 || !b[i - 2].is_ascii_alphanumeric())));
        if !starts {
            continue;
        }
        let len = b[i..].iter().take_while(|c| c.is_ascii_digit() || **c == b'.').count();
        let v = text[i..i + len].trim_end_matches('.');
        // a date (10.01.2025) or the game's build ("build 42.13") isn't the mod's version
        let date = v.split('.').any(|p| p.len() == 4);
        let build = text[..i].trim_end().to_ascii_lowercase().ends_with("build");
        if v.contains('.') && !v.contains("..") && !date && !build {
            return Some(v.to_string());
        }
    }
    None
}

/// The major version moved (for 0.x versions, the minor).
fn major_jump(from: &str, to: &str) -> bool {
    let parts = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (a, b) = (parts(from), parts(to));
    if from.is_empty() || to.is_empty() {
        return false;
    }
    if a[0] == 0 && b[0] == 0 { a.get(1) != b.get(1) } else { a[0] != b[0] }
}

/// The note says the update needs a new game or breaks saves.
fn warns_saves(text: &str) -> bool {
    const WORDS: &[&str] = &[
        "new save", "new game", "fresh save", "fresh game", "start a new", "save-breaking", "save breaking", "savebreaking", "break save", "breaks save",
        "break your save", "breaks existing", "wipe", "not save compatible", "not save-compatible", "not compatible with existing save",
    ];
    let t = text.to_lowercase();
    WORDS.iter().any(|w| t.contains(w))
}

/// A mod's own changelog, in the format in-game changelog mods (Mod Update and Alert System) read.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Changelog {
    /// Newest first.
    pub entries: Vec<LogEntry>,
    /// The links its ALERT_CONFIG offers: (title, https URL).
    pub links: Vec<(String, String)>,
    /// The entries are BBCode (`ChangeLog.bbcode`); else plain text or Markdown.
    pub bbcode: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct LogEntry {
    pub title: String,
    pub text: String,
}

/// The mod's `ChangeLog.txt`, else `.md`, else `.bbcode` (any case), in its version folder,
/// `common/` or the mod folder, each in `media/` or at the top.
fn bundled_changelog(row: &store::Row) -> Option<Changelog> {
    let dirs: Vec<PathBuf> = [row.info.parent().map(Path::to_path_buf), Some(row.dir.join("common")), Some(row.dir.clone())].into_iter().flatten().collect();
    for ext in ["txt", "md", "bbcode"] {
        let name = format!("changelog.{ext}");
        for d in &dirs {
            for sub in ["media", ""] {
                let Ok(rd) = std::fs::read_dir(d.join(sub)) else { continue };
                let found = rd.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(&name));
                if let Some(text) = found.and_then(|e| std::fs::read(e.path()).ok()) {
                    let log = parse_changelog(&super::decode_text(&text), ext);
                    return (!log.entries.is_empty()).then_some(log);
                }
            }
        }
    }
    None
}

/// `link1 = Title = https://...,` lines; only https links are kept.
fn alert_links(config: &str) -> Vec<(String, String)> {
    config
        .lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '=');
            let (_, title, url) = (parts.next()?, parts.next()?.trim(), parts.next()?.trim().trim_end_matches(',').trim());
            (url.starts_with("https://") && !title.is_empty()).then(|| (title.to_string(), url.to_string()))
        })
        .collect()
}

/// `[ Title ]` … `[ ------ ]` blocks (`ext` "txt"), `### Title ###` … `#` blocks ("md"), or the
/// whole file as one entry ("bbcode", titled by its first quote's first line); each with the
/// ALERT_CONFIG links.
fn parse_changelog(text: &str, ext: &str) -> Changelog {
    let mut out = Changelog { bbcode: ext == "bbcode", ..Default::default() };
    let mut entries: Vec<(String, Vec<&str>)> = vec![];
    if ext == "bbcode" {
        let lower = text.to_ascii_lowercase();
        let mut body = text.to_string();
        if let (Some(a), Some(b)) = (lower.find("[alert_config]"), lower.find("[/alert_config]"))
            && a < b
        {
            out.links = alert_links(&text[a + 14..b]);
            body = format!("{}{}", &text[..a], &text[b + 15..]);
        }
        let quote = body.to_ascii_lowercase().find("[quote]").map(|i| i + 7);
        let title = quote.map_or(&body[..], |i| &body[i..]).lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string();
        let text = body.replacen(&title, "", 1).trim().to_string();
        if !title.is_empty() {
            out.entries.push(LogEntry { title, text });
        }
        return out;
    }
    let mut current: Option<(String, Vec<&str>)> = None;
    let mut config: Option<Vec<&str>> = None; // inside an md ALERT_CONFIG comment
    let mut comment = false;
    for line in text.lines() {
        let l = line.trim();
        if ext == "md" {
            if let Some(c) = config.as_mut() {
                if l.starts_with("-->") {
                    out.links = alert_links(&c.join("\n"));
                    config = None;
                } else {
                    c.push(l);
                }
                continue;
            }
            if comment || l.starts_with("<!--") {
                if l.starts_with("<!-- ALERT_CONFIG") {
                    config = Some(vec![]);
                } else {
                    comment = !l.contains("-->");
                }
                continue;
            }
            if let Some(head) = l.strip_prefix("###").and_then(|h| h.strip_suffix("###")).filter(|_| l.len() > 6) {
                entries.extend(current.take());
                current = Some((head.trim().to_string(), vec![]));
            } else if l == "#" {
                entries.extend(current.take());
            } else if let Some((_, body)) = current.as_mut() {
                body.push(line);
            }
        } else if let Some(head) = l.strip_prefix("[ ").and_then(|l| l.strip_suffix(" ]")).map(str::trim).filter(|h| !h.contains(']')) {
            entries.extend(current.take());
            if !head.chars().all(|c| c == '-') {
                current = Some((head.to_string(), vec![]));
            }
        } else if let Some((_, body)) = current.as_mut() {
            body.push(line);
        }
    }
    entries.extend(current);
    for (title, body) in entries.into_iter().rev() {
        if title.eq_ignore_ascii_case("ALERT_CONFIG") {
            out.links = alert_links(&body.join("\n"));
        } else {
            out.entries.push(LogEntry { title, text: body.join("\n").trim().to_string() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_or_a_changelog_count_as_something_to_read() {
        let note = |text: &str| Entry { notes: vec![ChangeNote { time: 1, text: text.into() }], ..Default::default() };
        assert!(note("fixed things").has_notes());
        assert!(!note(" \n").has_notes());
        assert!(!Entry::default().has_notes());
        assert!(!Entry { bundled: Some(Changelog::default()), ..Default::default() }.has_notes(), "an empty changelog");
        assert!(!Entry::default().nothing_to_read(), "not loaded yet: it may have notes");
        assert!(Entry { loaded: true, ..Default::default() }.nothing_to_read());
    }

    #[test]
    fn the_client_mod_reads_whats_new_as_lines() {
        let f = Feed {
            since: 10,
            basis: "launch",
            entries: vec![Entry {
                item: "42".into(),
                title: "Cool\tMod".into(),
                mods: vec!["A".into(), "B".into()],
                updated: 20,
                notes: vec![ChangeNote { time: 15, text: "fixed\r\nthings \\o/".into() }],
                risks: vec!["map", "notes"],
                pending: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = game_text(&f);
        let (head, rest) = text.split_once("entry").unwrap();
        assert!(head.starts_with("since=10\nbasis=launch\nat="), "{head}");
        assert_eq!(format!("entry{rest}"), "entry\t42\t20\t1\tmap,notes\t\t\tCool Mod\t0\nmods\tA,B\nnote\t15\tfixed\\nthings \\\\o/\n");
    }

    #[test]
    fn notes_risks_and_versions() {
        let note = |time, text: &str| ChangeNote { time, text: text.into() };
        let e = assess(vec![note(30, "v2.0.1\n- needs a NEW GAME"), note(20, "Version 2.0"), note(10, "v1.4.3 hotfix")], 15, false);
        assert_eq!(e.notes.iter().map(|n| n.time).collect::<Vec<_>>(), [30, 20], "only the notes after you played");
        assert_eq!((e.from.as_str(), e.to.as_str()), ("1.4.3", "2.0.1"));
        assert_eq!(e.risks, ["version", "notes"]);

        let e = assess(vec![note(30, "1.5: tidy"), note(10, "1.4")], 15, true);
        assert_eq!((e.risks.clone(), e.from.as_str(), e.to.as_str()), (vec!["map"], "1.4", "1.5"), "a minor bump is no risk; maps are");
        assert!(major_jump("0.9", "0.10") && !major_jump("", "2.0") && !major_jump("3.1", "3.9"));
        assert_eq!(version_in("Update for B42.13: fixed 2 bugs"), None, "the game build is not the mod version");
        assert_eq!(version_in("Update 10.01.2025: fixes"), None, "nor is a date");
        assert_eq!(version_in("Works on build 42.13 now, v1.4.2"), Some("1.4.2".into()));
        assert_eq!(version_in("fixed bug2.0x and 1. items"), None);
    }

    #[test]
    fn reads_the_mods_own_changelog() {
        let entry = |title: &str, text: &str| LogEntry { title: title.into(), text: text.into() };
        let site = vec![("Site".to_string(), "https://steamcommunity.com/linkfilter/?u=https://example.com".to_string())];
        // the formats Mod Update and Alert System reads, as its own files have them
        let txt = "[ ALERT_CONFIG ]
link1 = Site = https://steamcommunity.com/linkfilter/?u=https://example.com,
link2 = Bad = http://x.y,
[ ------ ]

[ 1.0 ]
- first
[ ------ ]
[ 1.1 hotfix #1 ]
- Fixed the radio
[b]kept[/b]
- Faster
[ ------ ]
";
        let log = parse_changelog(txt, "txt");
        assert_eq!(log.entries, [entry("1.1 hotfix #1", "- Fixed the radio
[b]kept[/b]
- Faster"), entry("1.0", "- first")], "all entries, newest first");
        assert_eq!(log.links, site);
        assert!(!log.bbcode && parse_changelog("[ ALERT_CONFIG ]
x
[ ------ ]", "txt").entries.is_empty());

        let md = "### 1.0 ###
First.
#

<!-- Links need the filter.
-->

### 1.1 ###
- **Faster**
#
<!-- ALERT_CONFIG
link1 = Site = https://steamcommunity.com/linkfilter/?u=https://example.com,
-->";
        let log = parse_changelog(md, "md");
        assert_eq!(log.entries, [entry("1.1", "- **Faster**"), entry("1.0", "First.")]);
        assert_eq!(log.links, site);

        let bb = "[alert_config]
link1 = Site = https://steamcommunity.com/linkfilter/?u=https://example.com,
[/alert_config]

[quote]
2/15/2025
The [b]first[/b] line names it.
[/quote]";
        let log = parse_changelog(bb, "bbcode");
        assert_eq!(log.entries, [entry("2/15/2025", "[quote]

The [b]first[/b] line names it.
[/quote]")]);
        assert!(log.bbcode && log.links == site);

        // found in common/ (Build 42) before the mod folder; .txt before .md
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("common/media")).unwrap();
        std::fs::write(t.path().join("common/ChangeLog.md"), md).unwrap();
        std::fs::write(t.path().join("common/media/changelog.txt"), txt).unwrap();
        let row = store::Row { dir: t.path().into(), info: t.path().join("42/mod.info"), ..Default::default() };
        assert_eq!(bundled_changelog(&row).unwrap().entries[0].title, "1.1 hotfix #1");
    }

    #[test]
    fn last_played_from_launches_and_saves() {
        let (_t, paths) = store::tests::temp_paths();
        assert_eq!(last_played(&paths), (0, ""));
        let dir = paths.zomboid.join("Saves/Sandbox/a");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mods.txt"), "mods\n{\n}\n").unwrap();
        assert_eq!(last_played(&paths).1, "save");
        save(&paths, &State { last_launch: store::now() + 60, ..Default::default() }).unwrap();
        assert_eq!(last_played(&paths).1, "launch");
        // a game started from the game itself (PZMM - Client's played.txt; its os.time() may have a fraction)
        std::fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        std::fs::write(paths.zomboid.join("Lua/pzmm/played.txt"), format!("{}.5", store::now() + 120)).unwrap();
        assert_eq!(last_played(&paths), (store::now() + 120, "launch"));
    }

    #[test]
    fn whats_new_seen_is_shared_with_the_game() {
        let (_t, paths) = store::tests::temp_paths();
        set_seen(&paths, 50);
        assert_eq!(game_number(&paths, "feedseen.txt"), 0, "no PZMM - Client folder: nothing written");
        std::fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        set_seen(&paths, 50);
        set_seen(&paths, 40);
        assert_eq!(game_number(&paths, "feedseen.txt"), 50, "only ever forward");
    }
}
