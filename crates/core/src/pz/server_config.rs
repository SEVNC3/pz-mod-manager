//! A server's settings as forms. The server writes a comment above each key of its `<name>.ini`
//! and `<name>_SandboxVars.lua` (42.21: the description, choices, `Min: Max: Default:`), so the forms
//! come from the files themselves and follow new versions. Sandbox presets are the game's own
//! (`media/lua/shared/Sandbox/*.lua` in the installed server) and the user's (`<data>/sandbox-presets`).
//! Nothing is written to a file that couldn't be read back; the file before PZMM's first change
//! stays as `.bak` and the one before the latest change as `.prev`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::Paths;

/// One setting in a form.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    /// The ini key, or the sandbox path ("ZombieLore.Speed").
    pub key: String,
    /// As shown: sandbox strings without their quotes.
    pub value: String,
    pub section: String,
    pub description: String,
    /// "bool", "number", "choice", "text" or "secret".
    pub kind: String,
    /// Sandbox choices: (value, label).
    pub choices: Vec<(String, String)>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub default: Option<String>,
    /// A mod's name for its option (from its sandbox-options.txt and translation); "" for none.
    #[serde(default)]
    pub label: String,
    /// The mod the option comes from; "" for the game's own (or a mod PZMM can't see).
    #[serde(default)]
    pub source: String,
}

/// `text` without a trailing "Default: X", "Min: X", "Max: X" (in any order) or "Default = Label";
/// returns what was found.
fn limits(text: &str) -> (String, Option<f64>, Option<f64>, Option<String>) {
    let mut t = text.trim().to_string();
    let (mut min, mut max, mut default) = (None, None, None);
    loop {
        let cut = ["Default: ", "Default = ", "Min: ", "Max: "].iter().filter_map(|k| t.rfind(k).map(|at| (at, *k))).max();
        let Some((at, k)) = cut else { break };
        let v = t[at + k.len()..].trim().to_string();
        match k {
            "Min: " => min = v.parse().ok(),
            "Max: " => max = v.parse().ok(),
            _ => default = Some(v),
        }
        t.truncate(at);
        t = t.trim_end().to_string();
    }
    (t.trim().to_string(), min, max, default)
}

// --- <name>.ini ---

/// Ini sections, by the first rule a key matches (substrings); the rest go in "other".
const SECTIONS: &[(&str, &[&str])] = &[
    ("network", &["Port", "UPnP", "RCON", "server_browser", "MaxPacketsPerSecond"]),
    ("anticheat", &["AntiCheat", "ClientCommandFilter", "ClientActionLogs", "PerkLogs"]),
    ("access", &["PublicName", "PublicDescription", "Public", "Password", "MaxPlayers", "Open", "MaxAccountsPerUser", "AllowCoop", "AllowNonAsciiUsername", "LoginQueue", "DenyLogin", "PingLimit", "ServerWelcomeMessage", "ResetID", "ServerPlayerID", "DoLuaChecksum", "SteamVAC", "SteamScoreboard"]),
    ("pvp", &["PVP", "Safety", "War"]),
    ("safehouses", &["afehouse", "afeHouse", "Safezone", "Sledgehammer"]),
    ("factions", &["Faction"]),
    ("chat", &["Chat", "Voice", "Discord", "Webhook", "Radio", "BadWord", "GoodWord", "Announce"]),
    ("backups", &["Backup", "SaveWorld"]),
    ("players", &["Spawn", "Sleep", "Respawn", "DisplayUserName", "ShowFirstAndLastName", "Disguise", "Knocked", "Sneak", "MouseOver", "HidePlayers", "PlayerBump", "Scoreboard", "HideAdmins", "ShowCoordinates", "MapRemotePlayerVisibility"]),
];

/// The keys the Mods tab edits (and validates); the form leaves them out.
const ON_MODS_TAB: [&str; 3] = ["Mods", "WorkshopItems", "Map"];

fn section(key: &str) -> &'static str {
    SECTIONS.iter().find(|(_, needles)| needles.iter().any(|n| key.contains(n))).map_or("other", |(s, _)| s)
}

/// The ini's settings with their comments: each `# ...` block directly above a `key=value` line.
pub fn ini_settings(text: &str) -> Vec<Setting> {
    let mut out = vec![];
    let mut comment: Vec<&str> = vec![];
    for line in text.lines() {
        let l = line.trim();
        if let Some(c) = l.strip_prefix('#') {
            comment.push(c.trim());
            continue;
        }
        let Some((key, value)) = l.split_once('=').filter(|(k, _)| !k.is_empty() && !k.contains(' ')) else {
            comment.clear();
            continue;
        };
        let (description, min, max, default) = limits(&comment.join(" "));
        comment.clear();
        if ON_MODS_TAB.contains(&key) {
            continue;
        }
        let kind = if key.contains("Password") || key == "DiscordToken" {
            "secret"
        } else if matches!(value, "true" | "false") {
            "bool"
        } else if min.is_some() || max.is_some() {
            "number"
        } else {
            "text"
        };
        out.push(Setting { key: key.into(), value: value.into(), section: section(key).into(), description, kind: kind.into(), min, max, default, ..Default::default() });
    }
    out
}

/// Whether `value` fits `s` (a number in range, true/false, one line).
pub(crate) fn check_value(s: &Setting, value: &str) -> Result<()> {
    if value.contains(['\n', '\r']) {
        bail!("{} must be one line", s.key);
    }
    match s.kind.as_str() {
        "bool" if !matches!(value, "true" | "false") => bail!("{} is true or false", s.key),
        "number" | "choice" => {
            let n: f64 = value.trim().parse().ok().filter(|n: &f64| n.is_finite()).with_context(|| format!("{} must be a number", s.key))?;
            if s.min.is_some_and(|m| n < m) || s.max.is_some_and(|m| n > m) {
                bail!("{} must be between {} and {}", s.key, s.min.unwrap_or(f64::MIN), s.max.unwrap_or(f64::MAX));
            }
            if s.kind == "choice" && !s.choices.iter().any(|(v, _)| v == value.trim()) {
                bail!("{} has no choice {value}", s.key);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Keep `<file>.prev` (the file before this write) next to `.bak` (before PZMM's first).
pub(crate) fn keep_prev(path: &Path) -> Result<()> {
    crate::backup_once(path)?;
    let mut prev = path.as_os_str().to_owned();
    prev.push(".prev");
    fs::copy(path, PathBuf::from(prev)).with_context(|| format!("copying {}", path.display()))?;
    Ok(())
}

/// Change ini values (keys the file has, checked against their comments).
pub fn write_ini(path: &Path, changes: &[(String, String)]) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {} (nothing was changed)", path.display()))?;
    let settings = ini_settings(&text);
    for (k, v) in changes {
        let s = settings.iter().find(|s| &s.key == k).with_context(|| format!("{} has no setting {k}", path.display()))?;
        check_value(s, v)?;
    }
    keep_prev(path)?;
    let set: Vec<(&str, String)> = changes.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    super::server::set_values(path, &set)
}

// --- <name>_SandboxVars.lua and the presets ---

/// Where each value sits: its line.
struct Parsed {
    settings: Vec<Setting>,
    lines: Vec<usize>,
}

/// A Lua literal as shown: strings without quotes (and `\"`, `\\` unescaped). None: not a literal we write.
fn literal(v: &str) -> Option<(String, &'static str)> {
    if matches!(v, "true" | "false") {
        return Some((v.into(), "bool"));
    }
    if v.parse::<f64>().is_ok_and(f64::is_finite) {
        return Some((v.into(), "number"));
    }
    let inner = v.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next()? {
                c @ ('"' | '\\') => out.push(c),
                _ => return None,
            },
            '"' => return None,
            c => out.push(c),
        }
    }
    Some((out, "text"))
}

/// Read `SandboxVars = { ... }` (or a preset's `return { ... }`): `Key = value,` lines, nested
/// `Key = {` tables, `--` comments above each key. Any other line is an error: such a file isn't
/// written. `lenient` (presets, only read) skips values that aren't plain literals.
fn parse_lua(text: &str, lenient: bool) -> Result<Parsed> {
    let (mut settings, mut lines) = (vec![], vec![]);
    let mut path: Vec<&str> = vec![];
    let mut comment: Vec<&str> = vec![];
    let mut opened = false;
    let mut closed = false;
    for (n, raw) in text.lines().enumerate() {
        let l = raw.trim();
        let bad = || anyhow::anyhow!("line {}: can't read {l:?}", n + 1);
        if l.is_empty() {
            continue;
        }
        if let Some(c) = l.strip_prefix("--") {
            comment.push(c.trim());
            continue;
        }
        if closed {
            bail!(bad());
        }
        if !opened {
            if l == "SandboxVars = {" || l == "return {" {
                opened = true;
                comment.clear();
                continue;
            }
            bail!(bad());
        }
        if l == "}" || l == "}," {
            comment.clear();
            if path.pop().is_none() {
                closed = true;
            }
            continue;
        }
        let (key, rest) = l.split_once(" = ").ok_or_else(bad)?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            bail!(bad());
        }
        if rest == "{" {
            path.push(key);
            comment.clear();
            continue;
        }
        let Some((value, kind)) = literal(rest.strip_suffix(',').unwrap_or(rest).trim()) else {
            if lenient {
                comment.clear();
                continue;
            }
            bail!(bad());
        };
        let (mut text, mut choices) = (vec![], vec![]);
        for c in comment.drain(..) {
            match c.split_once(" = ").filter(|(v, _)| v.parse::<i64>().is_ok()) {
                Some((v, label)) => choices.push((v.to_string(), label.to_string())),
                None => text.push(c),
            }
        }
        let (description, min, max, default) = limits(&text.join(" "));
        let full = path.iter().chain([&key]).copied().collect::<Vec<_>>().join(".");
        let kind = if !choices.is_empty() { "choice" } else { kind };
        settings.push(Setting { section: path.first().copied().unwrap_or("").into(), key: full, value, description, kind: kind.into(), choices, min, max, default, ..Default::default() });
        lines.push(n);
    }
    if !closed || !path.is_empty() {
        bail!("the table never closes");
    }
    Ok(Parsed { settings, lines })
}

pub fn sandbox_settings(text: &str) -> Result<Vec<Setting>> {
    parse_lua(text, false).map(|p| p.settings)
}

/// `text` with `changes` (path, value as shown) made in place: every other line, comment and line
/// ending stays. Checked by reading the result back.
fn sandbox_set(text: &str, changes: &[(String, String)]) -> Result<String> {
    let p = parse_lua(text, false)?;
    let mut lines: Vec<String> = text.split_inclusive('\n').map(String::from).collect();
    for (key, value) in changes {
        let i = p.settings.iter().position(|s| &s.key == key).with_context(|| format!("the sandbox has no setting {key}"))?;
        let s = &p.settings[i];
        check_value(s, value)?;
        let lua = if s.kind == "text" { format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")) } else { value.trim().to_string() };
        let line = &mut lines[p.lines[i]];
        let ending = &line[line.trim_end_matches(['\r', '\n']).len()..];
        let indent = &line[..line.len() - line.trim_start().len()];
        let name = key.rsplit('.').next().unwrap_or(key);
        *line = format!("{indent}{name} = {lua},{ending}");
    }
    let out = lines.concat();
    let back = parse_lua(&out, false).context("the changed file didn't read back (nothing was written)")?;
    for (key, value) in changes {
        if !back.settings.iter().any(|s| &s.key == key && s.value == value.trim()) {
            bail!("{key} didn't read back as {value} (nothing was written)");
        }
    }
    Ok(out)
}

pub fn write_sandbox(path: &Path, changes: &[(String, String)]) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {} (nothing was changed)", path.display()))?;
    let new = sandbox_set(&text, changes)?;
    keep_prev(path)?;
    crate::write_atomic(path, new)?;
    Ok(())
}

/// The game's presets in the installed server, by name.
pub fn game_presets(install: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(install.join("media/lua/shared/Sandbox"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".lua").map(String::from))
        .filter(|n| n != "SandboxVars")
        .collect();
    out.sort();
    out
}

/// The presets PZMM saved before it wrote them where the game reads them (still offered).
fn old_dir(paths: &Paths) -> PathBuf {
    paths.data.join("sandbox-presets")
}

/// The player's presets, where the game's new-game and server settings screens read and save
/// them: `Zomboid/Sandbox Presets/<name>.cfg`.
fn own_dir(paths: &Paths) -> PathBuf {
    paths.zomboid.join("Sandbox Presets")
}

/// The player's presets: the game's own folder, and the ones PZMM kept in its data before.
pub fn own_presets(paths: &Paths) -> Vec<String> {
    let names = |dir: PathBuf, ext: &'static str| fs::read_dir(dir).into_iter().flatten().flatten().filter_map(move |e| e.file_name().to_str()?.strip_suffix(ext).map(String::from));
    let mut out: Vec<String> = names(own_dir(paths), ".cfg").chain(names(old_dir(paths), ".json")).collect();
    out.sort();
    out.dedup();
    out
}

/// A preset's values by sandbox path (its version key left out): the game's from `install`, else the user's.
pub fn preset(paths: &Paths, install: &Path, name: &str, own: bool) -> Result<BTreeMap<String, String>> {
    if !crate::safe_file_name(name) {
        bail!("invalid preset name {name:?}");
    }
    if own {
        let f = own_dir(paths).join(format!("{name}.cfg"));
        if f.is_file() {
            return Ok(cfg_values(&fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?));
        }
        let f = old_dir(paths).join(format!("{name}.json"));
        return Ok(serde_json::from_str(&fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?)?);
    }
    let f = install.join("media/lua/shared/Sandbox").join(format!("{name}.lua"));
    preset_values(&fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?)
}

/// A preset file's values by sandbox path (`return { ... }` or `SandboxVars = { ... }`; its
/// version key left out).
pub fn preset_values(text: &str) -> Result<BTreeMap<String, String>> {
    Ok(parse_lua(text, true)?.settings.into_iter().filter(|s| !s.key.eq_ignore_ascii_case("version")).map(|s| (s.key, s.value)).collect())
}

/// A game preset file's values (`Version=6`, then `<path>=<value>` lines; `#` comments).
fn cfg_values(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty() && !k.eq_ignore_ascii_case("version"))
        .collect()
}

/// `values` as the game writes a preset (its `ConfigFile` format), with the game's sandbox `version`.
fn cfg_text(values: &BTreeMap<String, String>, version: u32) -> String {
    let mut out = format!("Version={version}\n");
    for (k, v) in values.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("version")) {
        out.push_str(&format!("{k}={v}\n"));
    }
    out
}

/// The installed game's sandbox version (`Version = 6` in its Apocalypse preset); 6 when it can't be read.
pub fn sandbox_version(install: Option<&Path>) -> u32 {
    install
        .and_then(|i| fs::read_to_string(i.join("media/lua/shared/Sandbox/Apocalypse.lua")).ok())
        .and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("Version = ")?.trim_end_matches(',').trim().parse().ok()))
        .unwrap_or(6)
}

/// Save a preset where the game reads them, so its new-game screen and server settings offer it too.
pub fn save_preset(paths: &Paths, name: &str, values: &BTreeMap<String, String>, version: u32) -> Result<()> {
    if !crate::safe_file_name(name) {
        bail!("invalid preset name {name:?}");
    }
    if let Some((k, _)) = values.iter().find(|(k, v)| k.contains(['=', '\n', '\r']) || v.contains(['\n', '\r'])) {
        bail!("the game's preset files can't hold the value of {k}");
    }
    fs::create_dir_all(own_dir(paths))?;
    crate::write_atomic(&own_dir(paths).join(format!("{name}.cfg")), cfg_text(values, version))?;
    Ok(())
}

// --- mods' sandbox-options.txt ---

/// One option a mod adds to the sandbox (`option Page.Name { type = ..., ... }` in its
/// `media/sandbox-options.txt`), with the words its Sandbox translation gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModOption {
    pub key: String,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub default: Option<String>,
    pub label: String,
    pub tooltip: String,
    pub page: String,
    pub choices: Vec<(String, String)>,
}

/// A mod's English Sandbox words: B42's `Translate/EN/Sandbox.json`, or B41's `Sandbox_EN.txt`
/// (`Sandbox_EN = { Key = "Text", }`), under `root/media/lua/shared`.
pub fn sandbox_words(root: &Path) -> BTreeMap<String, String> {
    let dir = root.join("media/lua/shared/Translate/EN");
    let mut out = BTreeMap::new();
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_ascii_lowercase();
        let text = || fs::read(e.path()).map(|b| super::decode_text(&b)).unwrap_or_default();
        if n == "sandbox.json" {
            if let Ok(m) = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&text()) {
                out.extend(m.into_iter().filter_map(|(k, v)| Some((k, v.as_str()?.to_string()))));
            }
        } else if n == "sandbox_en.txt" {
            for l in text().lines() {
                if let Some((k, v)) = l.split_once('=')
                    && let Some(v) = v.trim().trim_end_matches(',').trim().strip_prefix('"').and_then(|v| v.strip_suffix('"'))
                {
                    out.insert(k.trim().to_string(), v.to_string());
                }
            }
        }
    }
    out
}

/// The options in a `sandbox-options.txt`, named with `words` (see [`sandbox_words`]).
pub fn mod_options(text: &str, words: &BTreeMap<String, String>) -> Vec<ModOption> {
    let word = |k: &str| words.get(&format!("Sandbox_{k}")).cloned();
    let mut out = vec![];
    let mut rest = text;
    while let Some(at) = rest.find("option ") {
        rest = &rest[at + 7..];
        let key: String = rest.chars().take_while(|c| !c.is_whitespace() && *c != '{' && *c != '=').collect();
        let (Some(open), Some(close)) = (rest.find('{'), rest.find('}')) else { break };
        if close < open || key.is_empty() {
            continue;
        }
        let mut p: BTreeMap<&str, &str> = BTreeMap::new();
        for part in rest[open + 1..close].split([',', '\n']) {
            if let Some((k, v)) = part.split_once('=') {
                p.insert(k.trim(), v.trim());
            }
        }
        rest = &rest[close..];
        let num = |k: &str| p.get(k).and_then(|v| v.parse::<f64>().ok());
        let translation = p.get("translation").copied().unwrap_or(&key);
        let mut o = ModOption {
            label: word(translation).unwrap_or_else(|| key.clone()),
            tooltip: word(&format!("{translation}_tooltip")).unwrap_or_default(),
            page: p.get("page").map(|pg| word(pg).unwrap_or_else(|| pg.to_string())).unwrap_or_default(),
            default: p.get("default").map(|v| v.to_string()),
            key: key.clone(),
            ..Default::default()
        };
        match p.get("type").copied().unwrap_or("") {
            "boolean" => o.kind = "bool".into(),
            "integer" | "double" => (o.kind, o.min, o.max) = ("number".into(), num("min"), num("max")),
            "enum" => {
                let values = p.get("valueTranslation").copied().unwrap_or(translation);
                o.kind = "choice".into();
                o.choices = (1..=num("numValues").unwrap_or(0.0) as u32).map(|i| (i.to_string(), word(&format!("{values}_option{i}")).unwrap_or_else(|| i.to_string()))).collect();
            }
            _ => o.kind = "text".into(),
        }
        out.push(o);
    }
    out
}

/// Name the sandbox settings mods add: the option's label, tooltip, limits, choices and page
/// from `options` (by mod name), and the mod as their source. The rest stay the game's.
pub fn label_mod_settings(settings: &mut [Setting], options: &[(String, Vec<ModOption>)]) {
    for s in settings.iter_mut() {
        let Some((m, o)) = options.iter().find_map(|(m, opts)| opts.iter().find(|o| o.key == s.key).map(|o| (m, o))) else { continue };
        s.source = m.clone();
        s.label = o.label.clone();
        if !o.tooltip.is_empty() {
            s.description = o.tooltip.clone();
        }
        if !o.page.is_empty() {
            s.section = o.page.clone();
        }
        s.default = o.default.clone().or(s.default.take());
        // the file's literal decides what can be written; the option adds its limits and choices
        if s.kind == "number" && o.kind == "choice" {
            (s.kind, s.choices) = ("choice".into(), o.choices.clone());
        } else if s.kind == "number" {
            (s.min, s.max) = (o.min.or(s.min), o.max.or(s.max));
        }
    }
}

// --- map mods' spawn regions ---

/// The spawn regions map mods offer for the maps on a server's `Map=` line, from the folders
/// holding their `media/` (`roots`), leaving out what `have` already lists: each map folder's
/// `spawnpoints.lua` (as the game's own maps list theirs), else the entries of a
/// `spawnregions.lua` it ships.
pub fn map_spawn_regions(roots: &[PathBuf], maps: &[String], have: &[Region]) -> Vec<Region> {
    let mut out: Vec<Region> = vec![];
    for root in roots {
        for map in maps {
            let dir = root.join("media/maps").join(map);
            let found = if dir.join("spawnpoints.lua").is_file() {
                let title = super::maps::read_folder(&dir).title.replace(['"', '\\'], "");
                vec![Region { name: title, file: format!("media/maps/{map}/spawnpoints.lua"), server_file: false }]
            } else {
                fs::read(dir.join("spawnregions.lua")).ok().and_then(|b| spawn_regions(&super::decode_text(&b)).ok()).unwrap_or_default()
            };
            for r in found {
                if !have.iter().chain(&out).any(|h| h.file == r.file || h.name == r.name) {
                    out.push(r);
                }
            }
        }
    }
    out
}

// --- the files as text ---

/// The server's files the text editor opens, after its name.
pub const FILES: [&str; 4] = [".ini", "_SandboxVars.lua", "_spawnregions.lua", "_spawnpoints.lua"];

fn prev_of(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".prev");
    PathBuf::from(p)
}

pub fn has_prev(path: &Path) -> bool {
    prev_of(path).is_file()
}

/// Write a settings file as typed (the one before stays as `.prev`). A sandbox or spawn regions
/// file PZMM could read is refused when the new text doesn't read: the server would likely stop on it.
pub fn write_text(path: &Path, text: &str) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let check = |t: &str| -> Result<()> {
        if name.ends_with("_SandboxVars.lua") {
            sandbox_settings(t).map(|_| ())
        } else if name.ends_with("_spawnregions.lua") {
            spawn_regions(t).map(|_| ())
        } else {
            Ok(())
        }
    };
    if let Ok(old) = fs::read_to_string(path)
        && check(&old).is_ok()
    {
        check(text).context("PZMM can't read the new text, so the server likely can't either (nothing was written)")?;
    }
    if path.exists() {
        keep_prev(path)?;
    }
    crate::write_atomic(path, text)?;
    Ok(())
}

/// Put back the file as it was before the latest change. The current one becomes the previous one,
/// so a second restore undoes the first.
pub fn restore_prev(path: &Path) -> Result<()> {
    let prev = prev_of(path);
    let old = fs::read(&prev).with_context(|| format!("there's no earlier version of {}", path.display()))?;
    let now = fs::read(path).ok();
    crate::write_atomic(path, old)?;
    if let Some(now) = now {
        crate::write_atomic(&prev, now)?;
    }
    Ok(())
}

// --- <name>_spawnregions.lua ---

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    /// A map's spawnpoints.lua (`file`), or the server's own (`serverfile`).
    pub file: String,
    #[serde(default)]
    pub server_file: bool,
}

/// `{ name = "...", file = "..." },` entries in `function SpawnRegions() return { ... } end`.
/// Commented-out entries are skipped; any other line is an error.
pub fn spawn_regions(text: &str) -> Result<Vec<Region>> {
    let mut out = vec![];
    for (n, raw) in text.lines().enumerate() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("--") || ["function SpawnRegions()", "return {", "}", "end"].contains(&l) {
            continue;
        }
        let bad = || anyhow::anyhow!("line {}: can't read {l:?}", n + 1);
        let inner = l.strip_prefix('{').and_then(|r| r.trim_end_matches(',').trim().strip_suffix('}')).ok_or_else(bad)?;
        let mut region = Region { name: String::new(), file: String::new(), server_file: false };
        for part in inner.split("\", ") {
            let (k, v) = part.trim().split_once(" = \"").ok_or_else(bad)?;
            let v = v.trim_end_matches('"').to_string();
            match k {
                "name" => region.name = v,
                "file" => region.file = v,
                "serverfile" => (region.file, region.server_file) = (v, true),
                _ => bail!(bad()),
            }
        }
        if region.name.is_empty() || region.file.is_empty() {
            bail!(bad());
        }
        out.push(region);
    }
    Ok(out)
}

fn spawn_text(regions: &[Region]) -> Result<String> {
    let mut out = String::from("function SpawnRegions()\r\n\treturn {\r\n");
    for r in regions {
        if [&r.name, &r.file].iter().any(|v| v.trim().is_empty() || v.contains(['"', '\\', '\n', '\r'])) {
            bail!("a spawn region needs a name and a file, without quotes or backslashes");
        }
        let key = if r.server_file { "serverfile" } else { "file" };
        out += &format!("\t\t{{ name = \"{}\", {key} = \"{}\" }},\r\n", r.name.trim(), r.file.trim());
    }
    out += "\t}\r\nend\r\n";
    Ok(out)
}

pub fn write_spawn_regions(path: &Path, regions: &[Region]) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {} (nothing was changed)", path.display()))?;
    spawn_regions(&text).context("PZMM can't read this file, so it leaves it as it is")?;
    let new = spawn_text(regions)?;
    if spawn_regions(&new)? != regions.iter().map(|r| Region { name: r.name.trim().into(), file: r.file.trim().into(), server_file: r.server_file }).collect::<Vec<_>>() {
        bail!("the spawn regions didn't read back (nothing was written)");
    }
    keep_prev(path)?;
    crate::write_atomic(path, new)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // from the ini a real 42.21 server wrote (CRLF, as it writes them)
    const INI: &str = "# Players can hurt and kill other players\r\nPVP=true\r\n\r\nChatStreams=s,r,a,w,y,sh,f,all\r\n\r\n# The time it takes for a player to enter and leave PVP mode Min: 0 Max: 1000 Default: 2\r\nSafetyToggleTimer=2\r\n\r\n# Min: 0 Max: 65535 Default: 16262\r\nUDPPort=16262\r\n\r\n# Enter the mod loading ID here. It can be found in \\Steam\\steamapps\\workshop\\modID\\mods\\modName\\info.txt\r\nMods=\r\n\r\n# WARNING: Server player counts above 32 will potentially result in poor map streaming and desync. Please advance with caution. Min: 1 Max: 254 Default: 32\r\nMaxPlayers=32\r\n\r\n# Removal happens when map chunks are loaded.\r\n# Zero means they will never disappear Min: 0 Max: 365 Default: 0\r\nHoursForWorldItemRemoval=0\r\n\r\nRCONPassword=s3cret\r\n";

    #[test]
    fn ini_comments_become_the_form() {
        let s = ini_settings(INI);
        let keys: Vec<&str> = s.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["PVP", "ChatStreams", "SafetyToggleTimer", "UDPPort", "MaxPlayers", "HoursForWorldItemRemoval", "RCONPassword"], "Mods is on the Mods tab");
        assert_eq!((s[0].kind.as_str(), s[0].section.as_str(), s[0].description.as_str()), ("bool", "pvp", "Players can hurt and kill other players"));
        assert_eq!((s[1].kind.as_str(), s[1].description.as_str(), s[1].section.as_str()), ("text", "", "chat"));
        let t = &s[2];
        assert_eq!((t.kind.as_str(), t.min, t.max, t.default.as_deref()), ("number", Some(0.0), Some(1000.0), Some("2")));
        assert_eq!(t.description, "The time it takes for a player to enter and leave PVP mode");
        assert_eq!((s[3].description.as_str(), s[3].section.as_str()), ("", "network"));
        assert_eq!((s[4].section.as_str(), s[4].max), ("access", Some(254.0)));
        assert_eq!(s[5].description, "Removal happens when map chunks are loaded. Zero means they will never disappear", "comment lines join");
        assert_eq!((s[6].kind.as_str(), s[6].section.as_str()), ("secret", "network"));
    }

    #[test]
    fn ini_writes_are_checked() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Friends.ini");
        fs::write(&p, INI).unwrap();
        for bad in [("SafetyToggleTimer", "1001"), ("PVP", "yes"), ("Nope", "1"), ("ChatStreams", "a\nPVP=false"), ("MaxPlayers", "lots")] {
            assert!(write_ini(&p, &[(bad.0.into(), bad.1.into())]).is_err(), "{bad:?}");
        }
        assert_eq!(fs::read_to_string(&p).unwrap(), INI, "nothing written");
        write_ini(&p, &[("MaxPlayers".into(), "8".into()), ("PVP".into(), "false".into())]).unwrap();
        let s = ini_settings(&fs::read_to_string(&p).unwrap());
        assert_eq!(s.iter().find(|s| s.key == "MaxPlayers").unwrap().value, "8");
        assert_eq!(s.iter().find(|s| s.key == "PVP").unwrap().value, "false");
        assert_eq!(s.len(), 7, "comments kept");
        assert_eq!(fs::read_to_string(t.path().join("Friends.ini.prev")).unwrap(), INI);
        assert!(t.path().join("Friends.ini.bak").exists());
    }

    // from a real 42.21 server's SandboxVars.lua (cut down)
    const SANDBOX: &str = "SandboxVars = {\r\n    VERSION = 6,\r\n    -- Changing this also sets the \"Population Multiplier\" in Advanced Zombie Options. Default = Normal\r\n    -- 1 = Insane\r\n    -- 2 = Very High\r\n    -- 3 = High\r\n    -- 4 = Normal\r\n    Zombies = 4,\r\n    -- Controls whether some randomization is applied to zombie distribution.\r\n    ZombieVoronoiNoise = true,\r\n    WorldItemRemovalList = \"Base.Hat, Base.Glasses\",\r\n    -- The minimum amount of gasoline that can spawn in gas pumps. Check the \"Advanced\" box below to use a custom amount. Min: 0.00 Max: 1.00 Default: 0.00\r\n    GeneratorFuelMin = 0.0,\r\n    ZombieLore = {\r\n        -- How fast zombies move. Default = Random\r\n        -- 1 = Sprinters\r\n        -- 2 = Fast Shamblers\r\n        -- 3 = Shamblers\r\n        -- 4 = Random\r\n        Speed = 4,\r\n    },\r\n}\r\n";

    #[test]
    fn sandbox_reads_and_writes_in_place() {
        let s = sandbox_settings(SANDBOX).unwrap();
        let keys: Vec<&str> = s.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["VERSION", "Zombies", "ZombieVoronoiNoise", "WorldItemRemovalList", "GeneratorFuelMin", "ZombieLore.Speed"]);
        let z = &s[1];
        assert_eq!((z.kind.as_str(), z.default.as_deref(), z.choices.len()), ("choice", Some("Normal"), 4));
        assert_eq!(z.description, "Changing this also sets the \"Population Multiplier\" in Advanced Zombie Options.");
        assert_eq!(z.choices[0], ("1".to_string(), "Insane".to_string()));
        assert_eq!((s[3].kind.as_str(), s[3].value.as_str()), ("text", "Base.Hat, Base.Glasses"));
        assert_eq!((s[4].kind.as_str(), s[4].max, s[4].default.as_deref()), ("number", Some(1.0), Some("0.00")));
        assert_eq!((s[5].section.as_str(), s[5].kind.as_str()), ("ZombieLore", "choice"));

        assert_eq!(sandbox_set(SANDBOX, &[]).unwrap(), SANDBOX, "no changes: the same bytes");
        let out = sandbox_set(SANDBOX, &[("ZombieLore.Speed".into(), "1".into()), ("WorldItemRemovalList".into(), "Base.\"Hat\"".into()), ("ZombieVoronoiNoise".into(), "false".into())]).unwrap();
        assert!(out.contains("        Speed = 1,\r\n") && out.contains("    WorldItemRemovalList = \"Base.\\\"Hat\\\"\",\r\n"), "{out}");
        assert_eq!(out.lines().count(), SANDBOX.lines().count());
        let back = sandbox_settings(&out).unwrap();
        assert_eq!(back.iter().find(|s| s.key == "WorldItemRemovalList").unwrap().value, "Base.\"Hat\"");
        assert_eq!(back.iter().map(|s| &s.description).collect::<Vec<_>>(), s.iter().map(|s| &s.description).collect::<Vec<_>>(), "comments kept");
        for bad in [("ZombieLore.Speed", "9"), ("GeneratorFuelMin", "2"), ("Zombies", "x"), ("ZombieVoronoiNoise", "1"), ("Nope", "1")] {
            assert!(sandbox_set(SANDBOX, &[(bad.0.into(), bad.1.into())]).is_err(), "{bad:?}");
        }
        for broken in ["SandboxVars = {\n    Zombies = 4,\n", "SandboxVars = {\n    Zombies = os.exit(),\n}\n", "x = 1\n", "SandboxVars = {\n}\nprint(1)\n"] {
            assert!(sandbox_settings(broken).is_err(), "{broken}");
        }
    }

    #[test]
    fn presets_from_the_game_and_your_own() {
        let (t, paths) = crate::store::tests::temp_paths();
        let install = t.path().join("srv");
        let dir = install.join("media/lua/shared/Sandbox");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Apocalypse.lua"), "return {\r\n    Version = 6,\r\n    Zombies = 4,\r\n    PopulationMultiplier = tonumber(ZombiePopulationMultiplier.VeryHigh),\r\n    ZombieLore = {\r\n        Speed = 2,\r\n    },\r\n}\r\n").unwrap();
        fs::write(dir.join("SandboxVars.lua"), "SandboxVars = require \"Sandbox/Apocalypse\"\r\n").unwrap();
        assert_eq!(game_presets(&install), ["Apocalypse"]);
        let p = preset(&paths, &install, "Apocalypse", false).unwrap();
        assert_eq!(p, BTreeMap::from([("Zombies".to_string(), "4".to_string()), ("ZombieLore.Speed".to_string(), "2".to_string())]));
        assert_eq!(sandbox_version(Some(&install)), 6);
        save_preset(&paths, "Our server", &p, 6).unwrap();
        // the game's own format, in the folder its new-game screen reads
        assert_eq!(fs::read_to_string(paths.zomboid.join("Sandbox Presets/Our server.cfg")).unwrap(), "Version=6\nZombieLore.Speed=2\nZombies=4\n");
        // one the game saved, and one PZMM kept in its data before
        fs::write(paths.zomboid.join("Sandbox Presets/Mine.cfg"), "Version=6\r\n# Zombie speed\r\nZombieLore.Speed=3\r\nStartMonth=7\r\n").unwrap();
        fs::create_dir_all(paths.data.join("sandbox-presets")).unwrap();
        fs::write(paths.data.join("sandbox-presets/Old.json"), r#"{"Zombies":"2"}"#).unwrap();
        assert_eq!(own_presets(&paths), ["Mine", "Old", "Our server"]);
        assert_eq!(preset(&paths, &install, "Our server", true).unwrap(), p);
        assert_eq!(preset(&paths, &install, "Mine", true).unwrap(), BTreeMap::from([("ZombieLore.Speed".to_string(), "3".to_string()), ("StartMonth".to_string(), "7".to_string())]));
        assert_eq!(preset(&paths, &install, "Old", true).unwrap()["Zombies"], "2");
        assert!(save_preset(&paths, "../x", &p, 6).is_err());
        assert!(save_preset(&paths, "x", &BTreeMap::from([("A".to_string(), "1\nB=2".to_string())]), 6).is_err());
    }

    #[test]
    fn mod_options_name_their_settings() {
        // shaped like a B42 mod's sandbox-options.txt and Sandbox.json
        let text = "VERSION = 1,\n\noption Hydro.Speed\n{\n    type = integer,\n    min = 1,\n    max = 10,\n    default = 5,\n    page = Hydro,\n    translation = Hydro_Speed,\n}\n\noption Hydro.Mode = {\n    type = enum, numValues = 2, default = 1,\n    page = Hydro, translation = Hydro_Mode, valueTranslation = Hydro_Modes,\n}\noption Hydro.On\n{\n    type = boolean, default = true,\n}\n";
        let words = BTreeMap::from([("Sandbox_Hydro", "Hydrocraft"), ("Sandbox_Hydro_Speed", "Growth speed"), ("Sandbox_Hydro_Speed_tooltip", "How fast plants grow."), ("Sandbox_Hydro_Modes_option1", "Easy")].map(|(k, v)| (k.to_string(), v.to_string())));
        let o = mod_options(text, &words);
        assert_eq!(o.iter().map(|o| (o.key.as_str(), o.kind.as_str())).collect::<Vec<_>>(), [("Hydro.Speed", "number"), ("Hydro.Mode", "choice"), ("Hydro.On", "bool")]);
        assert_eq!((o[0].label.as_str(), o[0].tooltip.as_str(), o[0].page.as_str(), o[0].min, o[0].max), ("Growth speed", "How fast plants grow.", "Hydrocraft", Some(1.0), Some(10.0)));
        assert_eq!(o[1].choices, [("1".to_string(), "Easy".to_string()), ("2".to_string(), "2".to_string())]);
        assert_eq!(o[2].label, "Hydro.On", "no translation: its key");

        let mut s = sandbox_settings("SandboxVars = {\n    Zombies = 4,\n    Hydro = {\n        Speed = 5,\n        Mode = 1,\n    },\n}\n").unwrap();
        label_mod_settings(&mut s, &[("Hydrocraft".into(), o)]);
        assert_eq!((s[0].source.as_str(), s[0].label.as_str()), ("", ""), "the game's own");
        assert_eq!((s[1].source.as_str(), s[1].label.as_str(), s[1].section.as_str(), s[1].max), ("Hydrocraft", "Growth speed", "Hydrocraft", Some(10.0)));
        assert_eq!((s[2].kind.as_str(), s[2].choices.len()), ("choice", 2));

        let t = tempfile::tempdir().unwrap();
        let tr = t.path().join("media/lua/shared/Translate/EN");
        fs::create_dir_all(&tr).unwrap();
        fs::write(tr.join("sandbox.json"), r#"{"Sandbox_A": "Label A"}"#).unwrap();
        fs::write(tr.join("Sandbox_EN.txt"), "Sandbox_EN = {\n    Sandbox_B = \"Label B\",\n}\n").unwrap();
        let w = sandbox_words(t.path());
        assert_eq!((w["Sandbox_A"].as_str(), w["Sandbox_B"].as_str()), ("Label A", "Label B"));
    }

    #[test]
    fn map_mods_offer_their_spawn_points() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        let w = |p: PathBuf, text: &str| {
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        };
        w(a.join("media/maps/Raven Creek/spawnpoints.lua"), "function SpawnPoints() end");
        w(a.join("media/maps/Raven Creek/map.info"), "title=Raven Creek \"RC\"\nlots=Muldraugh, KY\n");
        w(b.join("media/maps/Hilltop/spawnregions.lua"), "function SpawnRegions()\n\treturn {\n\t\t{ name = \"Hilltop\", file = \"media/maps/Hilltop/spawnpoints.lua\" },\n\t}\nend\n");
        w(b.join("media/maps/Other/spawnpoints.lua"), "x");
        let have = [Region { name: "Muldraugh, KY".into(), file: "media/maps/Muldraugh, KY/spawnpoints.lua".into(), server_file: false }];
        let maps = ["Raven Creek".to_string(), "Hilltop".into(), "Muldraugh, KY".into()];
        let got = map_spawn_regions(&[a, b], &maps, &have);
        assert_eq!(got.iter().map(|r| (r.name.as_str(), r.file.as_str())).collect::<Vec<_>>(), [("Raven Creek RC", "media/maps/Raven Creek/spawnpoints.lua"), ("Hilltop", "media/maps/Hilltop/spawnpoints.lua")]);
        assert!(map_spawn_regions(&[t.path().join("a")], &maps, &got).is_empty(), "already there");
    }

    #[test]
    fn text_edits_keep_the_previous_file() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Friends_SandboxVars.lua");
        fs::write(&p, SANDBOX).unwrap();
        assert!(write_text(&p, "SandboxVars = {\n    Zombies = \n").is_err(), "PZMM read it before: it must still read");
        assert_eq!(fs::read_to_string(&p).unwrap(), SANDBOX);
        let changed = SANDBOX.replace("Zombies = 4", "Zombies = 2");
        write_text(&p, &changed).unwrap();
        assert!(has_prev(&p));
        restore_prev(&p).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), SANDBOX);
        restore_prev(&p).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), changed, "a second restore undoes the first");
        let ini = t.path().join("Friends.ini");
        write_text(&ini, "PVP=true\n").unwrap();
        assert!(!has_prev(&ini), "a new file has no earlier version");
        assert!(restore_prev(&ini).is_err());
    }

    #[test]
    fn spawn_regions_round_trip() {
        // as a real 42.21 server writes it
        let text = "function SpawnRegions()\r\n\treturn {\r\n\t\t{ name = \"Muldraugh, KY\", file = \"media/maps/Muldraugh, KY/spawnpoints.lua\" },\r\n\t\t{ name = \"Riverside, KY\", file = \"media/maps/Riverside, KY/spawnpoints.lua\" },\r\n\t\t-- Uncomment the line below to add a custom spawnpoint for this server.\r\n--\t\t{ name = \"Twiggy's Bar\", serverfile = \"pzmmit_spawnpoints.lua\" },\r\n\t}\r\nend\r\n";
        let r = spawn_regions(text).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].name.as_str(), r[0].file.as_str(), r[0].server_file), ("Muldraugh, KY", "media/maps/Muldraugh, KY/spawnpoints.lua", false));
        let mut more = r.clone();
        more.push(Region { name: "Twiggy's Bar".into(), file: "pzmmit_spawnpoints.lua".into(), server_file: true });
        assert_eq!(spawn_regions(&spawn_text(&more).unwrap()).unwrap(), more);
        assert!(spawn_text(&[Region { name: "a\"b".into(), file: "f".into(), server_file: false }]).is_err());
        assert!(spawn_regions("function SpawnRegions()\n\treturn getSpawns()\nend\n").is_err(), "code PZMM doesn't read");

        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x_spawnregions.lua");
        fs::write(&p, "function SpawnRegions()\n\treturn custom()\nend\n").unwrap();
        assert!(write_spawn_regions(&p, &r).is_err(), "never writes a file it couldn't read");
        fs::write(&p, text).unwrap();
        write_spawn_regions(&p, &more).unwrap();
        assert_eq!(spawn_regions(&fs::read_to_string(&p).unwrap()).unwrap(), more);
    }
}
