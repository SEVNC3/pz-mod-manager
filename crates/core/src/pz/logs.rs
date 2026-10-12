//! What went wrong in the last game or server run, and which mods it points at. The game's logs
//! (`console.txt`, `Logs/*_DebugLog.txt`, `server-console.txt`) carry three kinds of clue:
//! STACK TRACE frames (`function: render -- file: X.lua line # 178 | MOD: Mod Name`, or `| Vanilla`),
//! Build 42's `at Lua(Mod Name).func` on a log line, and paths into a mod's folder
//! (`.../108600/<item>/mods/<Folder>/...`, `Zomboid/mods/<Folder>/...`).

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::Paths;
use crate::store::Row;

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct ModErrors {
    /// The mod's name (as installed, else as the log has it).
    pub name: String,
    pub count: usize,
    /// The first offending frame or line, e.g. `render -- file: X.lua line # 178`.
    pub example: String,
    /// The installed mod it is, when PZMM can tell.
    pub id: Option<String>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct FirstError {
    /// 1-based line in the log.
    pub line: usize,
    /// The error and the lines that belong to it (a few at most).
    pub text: String,
    /// Installed mods it points at, first clue first.
    pub ids: Vec<String>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Report {
    pub file: String,
    pub first: Option<FirstError>,
    pub mods: Vec<ModErrors>,
}

/// The DebugLog's `[11-10-26 10:16:35.900] ` prefix, if any, removed.
fn unstamp(l: &str) -> &str {
    match l.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        Some((stamp, rest)) if stamp.len() < 30 && stamp.bytes().all(|b| b.is_ascii_digit() || b" -:.".contains(&b)) => rest,
        _ => l,
    }
}

/// A log line's level (`LOG`, `ERROR`, `WARN`...), None for continuation lines and trace frames.
fn level(l: &str) -> Option<&str> {
    let (head, _) = l.split_once(':')?;
    let head = head.trim();
    ["LOG", "ERROR", "WARN", "DEBUG", "TRACE", "INFO"].contains(&head).then_some(head)
}

/// Words that make an ERROR line a real failure, not the game's usual startup noise.
const FAILURE: &[&str] = &["Exception", "Error found in", "OutOfMemoryError", "attempted index", "tried to call nil", "non-table", "StackOverflow", "failed to load file"];

/// Mod clues on a line: a trace frame's mod, `Lua(Name)`, a mod folder in a path.
fn clues(l: &str) -> Vec<String> {
    let mut out = vec![];
    if let Some((_, name)) = l.split_once("| MOD:") {
        out.push(name.trim().to_string());
    }
    let mut rest = l;
    while let Some(i) = rest.find("Lua(") {
        rest = &rest[i + 4..];
        if let Some(end) = rest.find(')') {
            let name = &rest[..end];
            if !name.is_empty() && name != "Vanilla" {
                out.push(name.to_string());
            }
        }
    }
    let path = l.replace('\\', "/");
    let lower = path.to_ascii_lowercase();
    for marker in ["/108600/", "/zomboid/mods/"] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(marker).map(|i| i + from + marker.len()) {
            from = i;
            let parts: Vec<&str> = path[i..].splitn(4, '/').collect();
            let (folder, more) = match (marker, parts.as_slice()) {
                ("/108600/", [_, "mods", f, ..]) => (f.to_string(), parts.len() > 3),
                ("/108600/", [item, ..]) if item.bytes().all(|b| b.is_ascii_digit()) && !item.is_empty() => (format!("#{item}"), true),
                ("/108600/", _) => continue,
                (_, [f, ..]) => (f.to_string(), parts.len() > 1),
                _ => continue,
            };
            // a folder that ends the path runs into the rest of the line: cut it at a space or quote
            let folder = if more { folder } else { folder.split([' ', '"', '\'', ':', ',', ')']).next().unwrap_or_default().to_string() };
            if !folder.is_empty() && !out.contains(&folder) {
                out.push(folder);
            }
        }
    }
    out
}

/// The installed mod a clue names: by id, name or folder, or (`#<item>`) by Workshop item.
fn mod_of<'a>(clue: &str, mods: &'a BTreeMap<String, Row>) -> Option<&'a Row> {
    if let Some(item) = clue.strip_prefix('#') {
        return mods.values().find(|m| crate::sources::workshop_item(&m.source) == Some(item));
    }
    mods.get(clue).or_else(|| mods.values().find(|m| m.name == clue || m.dir.file_name().is_some_and(|n| n.to_string_lossy() == clue)))
}

/// Where errors start in `lines` (unstamped), and the errors per mod (most first). Vanilla-only
/// traces count as errors but point at no mod; ERROR lines without a failure word or a mod are noise.
fn scan(lines: &[&str], mods: &BTreeMap<String, Row>) -> (Vec<usize>, Vec<ModErrors>) {
    let mut by_mod: BTreeMap<String, ModErrors> = BTreeMap::new();
    let mut last_hit: HashMap<String, usize> = HashMap::new();
    let mut starts: Vec<usize> = vec![];
    let mut in_trace = false; // inside a STACK TRACE block that hasn't named a mod yet
    for (i, l) in lines.iter().enumerate() {
        let lvl = level(l);
        let mut found: Vec<String> = vec![];
        let mut from_trace = false;
        if l.contains("STACK TRACE") {
            in_trace = true;
            starts.push(i);
            continue;
        }
        if lvl.is_some() {
            in_trace = false;
        }
        if in_trace && l.trim_start().starts_with("function:") && l.contains("| MOD:") {
            found = clues(l);
            (in_trace, from_trace) = (false, true);
        } else if lvl == Some("ERROR") || (lvl.is_none() && !in_trace && (l.contains("Exception in thread") || l.contains("OutOfMemoryError"))) {
            found = clues(l);
            if !found.is_empty() || FAILURE.iter().any(|w| l.contains(w)) {
                starts.push(i);
            }
        }
        for clue in found.into_iter().take(1) {
            let m = mod_of(&clue, mods);
            let key = m.map_or(clue.clone(), |m| m.id.clone());
            // one error shows as a trace and an error line naming the same mod: count it once
            let again = last_hit.insert(key.clone(), i).is_some_and(|at| i - at <= 10) && !from_trace;
            let example = l.trim().trim_start_matches("function:").trim().split("| MOD:").next().unwrap_or_default().trim().to_string();
            let name = m.map_or(clue, |m| if m.name.is_empty() { m.id.clone() } else { m.name.clone() });
            let e = by_mod.entry(key).or_insert(ModErrors { name, count: 0, example, id: m.map(|m| m.id.clone()) });
            if !again {
                e.count += 1;
            }
        }
    }
    let mut out: Vec<ModErrors> = by_mod.into_values().collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    (starts, out)
}

/// The end (exclusive) of the error starting at `start`: its trace and Java stack, up to the next
/// ordinary line (`max` lines at most). Build 42 opens a STACK TRACE with a `LOG ... -----` line,
/// which belongs to it. A second STACK TRACE is the next error.
fn block_end(lines: &[&str], start: usize, max: usize) -> usize {
    let mut end = start + 1;
    let mut traced = lines[start].contains("STACK TRACE");
    while end < lines.len() && end - start < max {
        let l = lines[end];
        if l.contains("STACK TRACE") {
            if traced {
                // its opening `-----` line goes with it
                return if lines[end - 1].trim_end().ends_with("-----") && end - 1 > start { end - 1 } else { end };
            }
            traced = true;
        }
        let opens_trace = l.trim_end().ends_with("-----") && lines.get(end + 1).is_some_and(|n| n.contains("STACK TRACE"));
        if matches!(level(l), Some("LOG" | "WARN" | "DEBUG" | "INFO")) && !opens_trace {
            break;
        }
        end += 1;
    }
    end
}

/// Installed mods the lines point at, first clue first.
fn ids_in(lines: &[&str], mods: &BTreeMap<String, Row>) -> Vec<String> {
    let mut ids: Vec<String> = vec![];
    for l in lines {
        for c in clues(l) {
            if let Some(m) = mod_of(&c, mods).filter(|m| !ids.contains(&m.id)) {
                ids.push(m.id.clone());
            }
        }
    }
    ids
}

fn joined(block: &[&str]) -> String {
    block.iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
}

/// The first real error in a log and the errors per mod (most first).
fn analyse(text: &str, mods: &BTreeMap<String, Row>) -> (Option<FirstError>, Vec<ModErrors>) {
    let lines: Vec<&str> = text.lines().map(unstamp).collect();
    let (starts, out) = scan(&lines, mods);
    let first = starts.first().map(|&start| {
        let block = &lines[start..block_end(&lines, start, 12)];
        FirstError { line: start + 1, text: joined(block), ids: ids_in(block, mods) }
    });
    (first, out)
}

/// One error as it repeats in a log: the same message and trace, wherever it happened.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct ErrorGroup {
    /// 1-based lines where it starts: the first and the last time.
    pub line: usize,
    pub last: usize,
    pub count: usize,
    /// The error, its Lua trace and Java stack (the first time).
    pub text: String,
    /// Installed mods it points at, first clue first.
    pub ids: Vec<String>,
}

/// An error block without what changes between repeats (the frame counter and time in the
/// `ERROR: General f:900 ...>` prefix).
fn signature(block: &[&str]) -> String {
    block.iter().map(|l| if level(l).is_some() { l.split_once("> ").map_or(*l, |(_, m)| m) } else { l }.trim()).collect::<Vec<_>>().join("\n")
}

/// Every error in `lines` (unstamped) from `starts`, the same ones grouped, in the order they
/// first happened; also the line ranges the errors cover.
fn groups(lines: &[&str], starts: &[usize], mods: &BTreeMap<String, Row>) -> (Vec<ErrorGroup>, Vec<Range<usize>>) {
    let (mut out, mut spans, mut seen) = (Vec::<ErrorGroup>::new(), Vec::<Range<usize>>::new(), HashMap::new());
    for &start in starts {
        if spans.last().is_some_and(|r| start < r.end) {
            continue; // an ERROR line inside the trace it belongs to
        }
        let end = block_end(lines, start, 40);
        spans.push(start..end);
        let block = &lines[start..end];
        match seen.entry(signature(block)) {
            Entry::Occupied(g) => {
                let g: &mut ErrorGroup = &mut out[*g.get()];
                (g.count, g.last) = (g.count + 1, start + 1);
            }
            Entry::Vacant(v) => {
                v.insert(out.len());
                out.push(ErrorGroup { line: start + 1, last: start + 1, count: 1, text: joined(block), ids: ids_in(block, mods) });
            }
        }
    }
    (out, spans)
}

/// A log to read in the app: its end, its errors grouped, and the errors per mod.
#[derive(Debug, serde::Serialize)]
pub struct View {
    pub file: String,
    /// Its size when read (the same size later: nothing new).
    pub size: u64,
    /// The last [`VIEW_LINES`] lines; `seq` is the 0-based line in the file. The lines of an
    /// error's trace count as errors.
    pub lines: Vec<super::host::Line>,
    pub groups: Vec<ErrorGroup>,
    pub mods: Vec<ModErrors>,
}

const VIEW_LINES: usize = 5000;

pub fn view(file: &Path, mods: &BTreeMap<String, Row>) -> anyhow::Result<View> {
    let bytes = std::fs::read(file)?;
    let text = String::from_utf8_lossy(&bytes);
    let raw: Vec<&str> = text.lines().collect();
    let lines: Vec<&str> = raw.iter().map(|l| unstamp(l)).collect();
    let (starts, by_mod) = scan(&lines, mods);
    let (groups, spans) = groups(&lines, &starts, mods);
    let skip = raw.len().saturating_sub(VIEW_LINES);
    let shown = (skip..raw.len())
        .map(|seq| {
            let (mut level, note) = super::host::classify(lines[seq]);
            if spans.get(spans.partition_point(|r| r.end <= seq)).is_some_and(|r| r.contains(&seq)) {
                level = super::host::Level::Error;
            }
            super::host::Line { seq: seq as u64, text: raw[seq].to_string(), level, note }
        })
        .collect();
    Ok(View { file: file.display().to_string(), size: bytes.len() as u64, lines: shown, groups, mods: by_mod })
}

/// The logs of the last runs: the game's `console.txt` (or its newest DebugLog), and the
/// `server-console.txt` and `coop-console.txt` of servers (in Zomboid and PZMM's servers' folders).
pub fn files(paths: &Paths) -> Vec<PathBuf> {
    let z = &paths.zomboid;
    let mut out = vec![];
    // the active profile's own game folder first, when it plays in one
    let own = crate::profile::launch_of(paths, &crate::profile::active_name(paths)).cache_dir;
    for dir in own.iter().chain([z]) {
        let console = dir.join("console.txt");
        if console.is_file() {
            out.push(console);
        } else {
            out.extend(debug_logs(dir).pop());
        }
    }
    let dirs = std::iter::once(z.clone()).chain(super::host::load(paths).into_iter().map(|s| s.cachedir(paths)));
    for d in dirs {
        for f in ["server-console.txt", "coop-console.txt"] {
            let p = d.join(f);
            if p.is_file() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// The game's `Logs/*_DebugLog.txt`, oldest first (they're named by date and time).
fn debug_logs(zomboid: &Path) -> Vec<PathBuf> {
    let mut logs: Vec<PathBuf> = std::fs::read_dir(zomboid.join("Logs"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_DebugLog.txt")))
        .collect();
    logs.sort();
    logs
}

/// The logs the app's viewer offers: those of [`files`], then the game's five newest DebugLogs.
pub fn viewable(paths: &Paths) -> Vec<PathBuf> {
    let mut out = files(paths);
    for l in debug_logs(&paths.zomboid).into_iter().rev().take(5) {
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// Every log of [`files`], analysed; logs with nothing wrong are left out.
pub fn reports(paths: &Paths, mods: &BTreeMap<String, Row>) -> Vec<Report> {
    files(paths)
        .into_iter()
        .filter_map(|f| {
            let bytes = std::fs::read(&f).ok()?;
            let (first, errs) = analyse(&String::from_utf8_lossy(&bytes), mods);
            (first.is_some() || !errs.is_empty()).then(|| Report { file: f.display().to_string(), first, mods: errs })
        })
        .collect()
}

/// The game version of the last run (`version=42.21.0 ...` in console.txt), e.g. `42.21.0`.
pub fn game_version(zomboid: &Path) -> Option<String> {
    let text = String::from_utf8_lossy(&std::fs::read(zomboid.join("console.txt")).ok()?).into_owned();
    text.lines().find_map(|l| {
        let v = l.split_once("> version=")?.1.split_whitespace().next()?;
        (v.contains('.') && v.bytes().all(|b| b.is_ascii_digit() || b == b'.')).then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, name: &str, dir: &str, source: &str) -> Row {
        Row {
            id: id.into(),
            name: name.into(),
            source: source.into(),
            dir: PathBuf::from(dir),
            info: PathBuf::from(dir).join("mod.info"),
            requires: vec![],
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: true,
            supports_42: true,
            ..Default::default()
        }
    }

    /// Lines in the game's own format (Build 41 traces, Build 42 headers and DebugLog stamps),
    /// with the usual startup noise before the first real error.
    const LOG: &str = "\
LOG  : General      f:0> version=42.21.0 4a0e9546ec demo=false
ERROR: General      f:0 at AngelCodeFont.parseFnt              > AngelCodeFont failed to load page 0 texture X:/Steam/steamapps/common/ProjectZomboid/media/mainfont_0.png
WARN : Lua          f:0 at Lua(Vanilla).corpseStorageCheck     > require(\"ISUI/ISInventoryPaneContextMenu\") failed
-----------------------------------------
STACK TRACE
-----------------------------------------
function: render -- file: RasFarmingMenu.lua line # 178 | MOD: ra's Farming & Canning
function: onTick -- file: Other.lua line # 3 | MOD: Other Mod
ERROR: General      f:0 at ExceptionLogger.logException> Exception thrown java.lang.RuntimeException: attempted index: size of non-table: null at KahluaThread.tableget(KahluaThread.java:1667)
\tat se.krka.kahlua.vm.KahluaThread.tableget(KahluaThread.java:1667)
LOG  : General      f:1> back to normal
STACK TRACE
-----------------------------------------
function: update -- file: ISSearchManager.lua line # 1255 | Vanilla
function: render -- file: RasFarmingMenu.lua line # 180 | MOD: ra's Farming & Canning
[11-10-26 10:20:01.100] ERROR: Lua          f:900 at Lua(Better Sorting).sortItems > Object tried to call nil in sortItems
[11-10-26 10:20:02.100] ERROR: General      f:901 at LuaManager.RunLua > Error found in LUA file: X:/Steam/steamapps/workshop/content/108600/2000001/mods/LootMod/42/media/lua/client/Loot.lua
LOG  : General a normal line -- file: nothing | MOD: not a trace
";

    #[test]
    fn finds_the_first_real_error_and_its_mods() {
        let mods = BTreeMap::from([
            ("RasFarming".to_string(), row("RasFarming", "ra's Farming & Canning", "/ws/1/mods/RasFarming", "steam:1")),
            ("BetterSorting".to_string(), row("BetterSorting", "Better Sorting", "/ws/3/mods/BetterSorting", "steam:3")),
            ("Loot".to_string(), row("Loot", "Loot", "/ws/2000001/mods/LootMod", "steam:2000001")),
        ]);
        let (first, errs) = analyse(LOG, &mods);
        let first = first.unwrap();
        assert_eq!(first.line, 5, "the font noise isn't an error: {first:?}");
        assert!(first.text.starts_with("STACK TRACE") && first.text.contains("Exception thrown"), "{}", first.text);
        assert_eq!(first.ids, ["RasFarming"], "the installed mod behind its first frame");

        let got: Vec<(&str, usize, Option<&str>)> = errs.iter().map(|e| (e.name.as_str(), e.count, e.id.as_deref())).collect();
        assert_eq!(got, [("ra's Farming & Canning", 2, Some("RasFarming")), ("Better Sorting", 1, Some("BetterSorting")), ("Loot", 1, Some("Loot"))]);
        assert_eq!(errs[0].example, "render -- file: RasFarmingMenu.lua line # 178");
        assert_eq!(game_version_of(LOG).as_deref(), Some("42.21.0"));
    }

    #[test]
    fn groups_repeated_errors_for_the_viewer() {
        let mods = BTreeMap::from([("RasFarming".to_string(), row("RasFarming", "ra's Farming & Canning", "/ws/1/mods/RasFarming", "steam:1"))]);
        // the first trace again, later in the session (another frame counter and time)
        let again = "STACK TRACE\n-----------------------------------------\nfunction: render -- file: RasFarmingMenu.lua line # 178 | MOD: ra's Farming & Canning\nfunction: onTick -- file: Other.lua line # 3 | MOD: Other Mod\n[11-10-26 11:00:00.000] ERROR: General      f:7000 at ExceptionLogger.logException> Exception thrown java.lang.RuntimeException: attempted index: size of non-table: null at KahluaThread.tableget(KahluaThread.java:1667)\n\tat se.krka.kahlua.vm.KahluaThread.tableget(KahluaThread.java:1667)\nLOG  : General      f:7001> fine\n";
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("console.txt");
        std::fs::write(&file, format!("{LOG}{again}")).unwrap();
        let v = view(&file, &mods).unwrap();
        let got: Vec<(usize, usize, usize, &[String])> = v.groups.iter().map(|g| (g.line, g.last, g.count, g.ids.as_slice())).collect();
        assert_eq!(got.len(), 2, "{:#?}", v.groups);
        assert_eq!(got[0], (5, 19, 2, &["RasFarming".to_string()][..]), "the same trace twice is one group");
        assert!(v.groups[0].text.ends_with("KahluaThread.java:1667)") && v.groups[1].text.starts_with("STACK TRACE"), "{:#?}", v.groups);
        assert!(v.groups[1].text.contains("tried to call nil"), "the error lines after a trace go with it");
        let level = |seq: u64| v.lines.iter().find(|l| l.seq == seq).map(|l| l.level);
        assert_eq!(level(6), Some(super::super::host::Level::Error), "a trace's lines are errors");
        assert_eq!(level(0), Some(super::super::host::Level::Info));
        assert_eq!(v.lines.len(), LOG.lines().count() + again.lines().count());
        assert_eq!(v.mods[0].count, 3);
    }

    #[test]
    fn reads_the_profiles_own_game_folder_first() {
        use crate::profile;
        let (_t, paths) = crate::store::tests::temp_paths();
        profile::apply(&paths, &profile::Profile::from_ids("Own", []), None).unwrap();
        std::fs::write(paths.zomboid.join("console.txt"), "x").unwrap();
        let own = profile::own_game_folder(&paths, "Own");
        profile::set_launch(&paths, "Own", &profile::Launch { args: String::new(), cache_dir: Some(own.clone()) }).unwrap();
        std::fs::write(own.join("console.txt"), "y").unwrap();
        assert_eq!(files(&paths)[..2], [own.join("console.txt"), paths.zomboid.join("console.txt")]);
    }

    fn game_version_of(text: &str) -> Option<String> {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("console.txt"), text).unwrap();
        game_version(t.path())
    }

    #[test]
    fn reads_mod_clues() {
        assert_eq!(clues(r"loading C:\Steam\steamapps\workshop\content\108600\123\mods\My Mod\media\x.lua"), ["My Mod"]);
        assert_eq!(clues("file C:/Users/x/Zomboid/mods/Local/media/lua/a.lua:3"), ["Local"]);
        assert_eq!(clues("Workshop item 108600/456 failed"), Vec::<String>::new(), "a bare number after a space isn't a path");
        assert_eq!(clues("content/108600/456/ isn't installed"), ["#456"]);
        assert_eq!(clues("at Lua(Vanilla).x > fine"), Vec::<String>::new());
        assert_eq!(unstamp("[11-10-26 10:16:35.900] LOG  : x"), "LOG  : x");
    }
}
