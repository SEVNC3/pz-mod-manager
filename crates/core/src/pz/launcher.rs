//! The launcher: list PZ servers (the Steam client, the Steam Web API, the game's saved list), read
//! each one over A2S, resolve its mod IDs to Workshop items, and keep favourites and the mod set
//! used at each join (`<data>/launcher.json`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::ToSocketAddrs;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::a2s;
use crate::pz::Paths;
use crate::store;

/// A server as the browser shows it: from the Steam list, then filled in by A2S.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    /// Where it answers queries, `ip:port`.
    pub addr: String,
    /// The game port, for `+connect`.
    pub port: u16,
    pub name: String,
    /// `Map=`, cut to 31 characters by the server.
    pub map: String,
    pub players: u32,
    pub max_players: u32,
    /// e.g. `;modded;pvp;VERSION:42.21`.
    pub keywords: String,
    /// "42.21", "41.78.16" or "" (from the keywords, else the `version` rule).
    pub build: String,
    pub pvp: bool,
    pub password: bool,
    /// Round trip in ms; None until it answered.
    pub ping: Option<u32>,
    /// The mod IDs the server lists: only the first ~127 characters of its `Mods=`.
    pub mods: Vec<String>,
    /// How many mods it runs (`modCount`).
    pub mod_count: u32,
    /// Plain text: chunks joined, colour and line markup turned into line breaks (the PZMM list taken out).
    pub description: String,
    /// The PZMM list in its description: its exact mods (see `tag`).
    pub tag: Option<super::tag::Tag>,
    /// Why a PZMM list it carries can't be used: "cut", "damaged" or "newer" (see `tag::decode`); empty otherwise.
    pub tag_error: String,
    /// The other rules, as sent.
    pub rules: BTreeMap<String, String>,
    /// Steam friends playing on it now (the Friends list only).
    pub friends: Vec<String>,
    /// It answered A2S.
    pub probed: bool,
    /// Why it didn't (empty when it did).
    pub error: String,
}

impl Server {
    /// `ip:port` to join.
    pub fn connect(&self) -> String {
        let host = self.addr.rsplit_once(':').map_or(self.addr.as_str(), |(h, _)| h);
        format!("{host}:{}", self.port)
    }

    pub(crate) fn keywords_said(&mut self) {
        let words: Vec<&str> = self.keywords.split(';').collect();
        self.pvp = words.contains(&"pvp");
        if let Some(v) = words.iter().find_map(|w| w.strip_prefix("VERSION:")) {
            self.build = v.into();
        }
    }

    fn take_info(&mut self, i: a2s::Info, rtt: Duration) {
        self.ping = Some(rtt.as_millis().min(9999) as u32);
        (self.name, self.map, self.players, self.max_players, self.password, self.keywords) = (i.name, i.map, i.players, i.max_players, i.password, i.keywords);
        if let Some(p) = i.game_port.filter(|p| *p != 0) {
            self.port = p;
        }
        self.keywords_said();
    }

    fn take_rules(&mut self, rules: Vec<(String, String)>) {
        let raw = raw_description(&rules);
        (self.tag, self.tag_error) = match super::tag::decode(&raw) {
            Some(Ok(t)) => (Some(t), String::new()),
            Some(Err(e)) => (None, e.into()),
            None => (None, String::new()),
        };
        self.description = plain_text(super::tag::strip(&raw));
        let mut rest: BTreeMap<String, String> = rules.into_iter().filter(|(k, _)| !k.starts_with("description")).collect();
        self.mods = super::server::split(rest.get("mods").map_or("", String::as_str));
        self.mod_count = rest.get("modCount").and_then(|c| c.parse().ok()).unwrap_or(self.mods.len() as u32);
        if let Some(pvp) = rest.get("pvp") {
            self.pvp = pvp == "1";
        }
        if self.build.is_empty() {
            self.build = rest.get("version").cloned().unwrap_or_default();
        }
        rest.remove("mods");
        self.rules = rest;
    }
}

/// The servers in a Steam Web API `GetServerList` reply.
fn parse_list(json: &str) -> Result<Vec<Server>> {
    let v: serde_json::Value = serde_json::from_str(json).context("Steam's server list isn't JSON")?;
    let num = |s: &serde_json::Value, k: &str| s[k].as_u64().unwrap_or(0);
    Ok(v["response"]["servers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let addr = s["addr"].as_str()?.to_string();
            let port = s["gameport"].as_u64().filter(|p| *p > 0 && *p <= 65535).map(|p| p as u16).or_else(|| addr.rsplit_once(':')?.1.parse().ok())?;
            let mut out = Server {
                addr,
                port,
                name: s["name"].as_str().unwrap_or_default().into(),
                map: s["map"].as_str().unwrap_or_default().into(),
                players: num(s, "players") as u32,
                max_players: num(s, "max_players") as u32,
                keywords: s["gametype"].as_str().unwrap_or_default().into(),
                ..Default::default()
            };
            out.keywords_said();
            Some(out)
        })
        .collect())
}

/// Every PZ server Steam lists, through the Steam Web API with the player's `key`.
pub fn list(key: &str) -> Result<Vec<Server>> {
    let url = format!("https://api.steampowered.com/IGameServersService/GetServerList/v1/?key={}&filter=%5Cappid%5C{}&limit=10000", valid_key(key)?, super::STEAM_APP);
    parse_list(&steam_api("https://api.steampowered.com/IGameServersService/GetServerList", &url)?)
}

/// A Steam Web API call with the player's `key` (`label`, not the URL, goes into errors: the URL holds the key).
fn steam_api(label: &str, url: &str) -> Result<String> {
    crate::sources::polite(label, || crate::sources::agent().get(url).header("User-Agent", crate::USER_AGENT).call()?.body_mut().with_config().limit(64 << 20).read_to_string()).map_err(|e| {
        match e.root_cause().downcast_ref::<ureq::Error>() {
            Some(ureq::Error::StatusCode(401 | 403)) => anyhow::anyhow!("Steam refused the Web API key, or the profile asked for isn't visible to it: check the key in Settings"),
            _ => anyhow::anyhow!("couldn't reach Steam: {}", e.root_cause()),
        }
    })
}

fn valid_key(key: &str) -> Result<&str> {
    let key = key.trim();
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric()) {
        bail!("the Steam Web API key isn't valid: it's 32 letters and digits");
    }
    Ok(key)
}

/// The servers in the game's own list, `Zomboid/db/ServerListSteam.db` (the ones added on its Join
/// screen), read from a copy. The passwords and accounts the game keeps there are never read.
pub fn game_list(zomboid: &std::path::Path) -> Result<Vec<Server>> {
    let db = zomboid.join("db").join("ServerListSteam.db");
    if !db.is_file() {
        return Ok(vec![]);
    }
    let (_tmp, c) = super::saves::open_copy(&db)?;
    let mut q = c.prepare("SELECT name, ip, port, description, mods FROM server ORDER BY id")?;
    let rows = q.query_map([], |r| {
        let text = |i| r.get::<_, Option<String>>(i).map(Option::unwrap_or_default);
        Ok((text(0)?, text(1)?, r.get::<_, i64>(2)?, text(3)?, text(4)?))
    })?;
    let mut out = vec![];
    for row in rows {
        let (name, ip, port, description, mods) = row?;
        let Ok(port) = u16::try_from(port) else { continue };
        if ip.trim().is_empty() || port == 0 {
            continue;
        }
        out.push(Server { addr: format!("{}:{port}", ip.trim()), port, name, description: plain_text(&description), mods: super::server::split(&mods), ..Default::default() });
    }
    Ok(out)
}

/// What one source added to the merged list: how many servers, or why none.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Answer {
    /// "steam" (the Steam client), "web" (the Web API key) or "game" (the game's saved list).
    pub source: String,
    pub count: usize,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Listing {
    pub servers: Vec<Server>,
    pub sources: Vec<Answer>,
}

/// The sources' lists as one, each server once by its query address: the record that says the
/// most is kept, in the order first seen.
pub fn merge(lists: Vec<(&str, Result<Vec<Server>>)>) -> Listing {
    let said = |s: &Server| [!s.name.is_empty(), !s.map.is_empty(), !s.keywords.is_empty(), s.max_players > 0, !s.description.is_empty(), !s.mods.is_empty()].into_iter().filter(|b| *b).count();
    let mut out = Listing::default();
    let mut at: HashMap<String, usize> = HashMap::new();
    for (source, got) in lists {
        let (count, error) = match &got {
            Ok(l) => (l.len(), String::new()),
            Err(e) => (0, format!("{e:#}")),
        };
        out.sources.push(Answer { source: source.into(), count, error });
        for s in got.into_iter().flatten() {
            match at.get(&s.addr) {
                Some(&i) if said(&s) > said(&out.servers[i]) => out.servers[i] = s,
                Some(_) => {}
                None => {
                    at.insert(s.addr.clone(), out.servers.len());
                    out.servers.push(s);
                }
            }
        }
    }
    out
}

/// The servers friends play on, from `GetPlayerSummaries` players: those in this game with a
/// server address, each server once with its friends' names.
fn friend_servers(players: &[serde_json::Value]) -> Vec<Server> {
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in players.iter().filter(|p| p["gameid"].as_str() == Some(super::STEAM_APP)) {
        let Some(addr) = p["gameserverip"].as_str().filter(|a| !a.starts_with("0.0.0.0") && !a.ends_with(":0") && a.contains(':')) else { continue };
        by.entry(addr.to_string()).or_default().push(p["personaname"].as_str().unwrap_or_default().to_string());
    }
    by.into_iter()
        .filter_map(|(addr, friends)| {
            let port = addr.rsplit_once(':')?.1.parse().ok()?;
            Some(Server { name: addr.clone(), addr, port, friends, ..Default::default() })
        })
        .collect()
}

/// Servers the Steam friends of the account signed in to Steam now are playing on (needs the Web
/// API key; Steam shows a friend's server only when their game details are public).
pub fn friends(key: &str) -> Result<Vec<Server>> {
    let key = valid_key(key)?;
    let me = crate::sources::steam_workshop::active_user().context("Steam isn't running with an account signed in, so PZMM doesn't know whose friends to look for")?;
    let v: serde_json::Value = serde_json::from_str(&steam_api(
        "https://api.steampowered.com/ISteamUser/GetFriendList",
        &format!("https://api.steampowered.com/ISteamUser/GetFriendList/v1/?key={key}&steamid={me}&relationship=friend"),
    )?)?;
    let ids: Vec<&str> = v["friendslist"]["friends"].as_array().into_iter().flatten().filter_map(|f| f["steamid"].as_str()).collect();
    let mut players = vec![];
    for chunk in ids.chunks(100) {
        let v: serde_json::Value = serde_json::from_str(&steam_api(
            "https://api.steampowered.com/ISteamUser/GetPlayerSummaries",
            &format!("https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/?key={key}&steamids={}", chunk.join(",")),
        )?)?;
        players.extend(v["response"]["players"].as_array().cloned().unwrap_or_default());
    }
    Ok(friend_servers(&players))
}

/// The description as sent: one `description`, or `description:N/M` chunks joined (a
/// description changed while the server ran leaves the old set beside the new one until a
/// restart, and nothing says which is newer: the longest complete set wins).
fn raw_description(rules: &[(String, String)]) -> String {
    if let Some((_, d)) = rules.iter().find(|(k, _)| k == "description") {
        return d.clone();
    }
    let mut sets: BTreeMap<usize, BTreeMap<usize, &str>> = BTreeMap::new();
    for (k, v) in rules {
        let Some((n, m)) = k.strip_prefix("description:").and_then(|nm| nm.split_once('/')) else { continue };
        if let (Ok(n), Ok(m)) = (n.parse(), m.parse()) {
            sets.entry(m).or_default().insert(n, v.as_str());
        }
    }
    let best = sets.iter().rev().find(|(m, chunks)| (1..=**m).all(|n| chunks.contains_key(&n)));
    best.map_or(String::new(), |(_, chunks)| chunks.values().copied().collect())
}

/// PZ's rich text as plain text: `\n` and `<LINE>`/`<BR>` become line breaks, other tags
/// (`<RGB:1,0,0>`, `<RED>`, `<SIZE:large>`, `<CENTRE>`...) go; lines trimmed, at most one blank line in a row.
fn plain_text(s: &str) -> String {
    let s = s.replace("\\n", "\n");
    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after.find('>').filter(|&e| e <= 60);
        let tag = end.map(|e| &after[..e]).filter(|t| {
            let name = t.split(':').next().unwrap_or("");
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') && name.as_bytes()[0].is_ascii_uppercase()
        });
        match (tag, end) {
            (Some(t), Some(e)) => {
                if matches!(t, "LINE" | "BR") {
                    out.push('\n');
                }
                rest = &after[e + 1..];
            }
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    let mut lines: Vec<&str> = vec![];
    for l in out.lines().map(str::trim) {
        if !(l.is_empty() && lines.last().is_none_or(|p| p.is_empty())) {
            lines.push(l);
        }
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// The mod IDs in a `mods` rule (`Mods=` as written, B42's `\` prefix optional).
/// The `Map=` folders from A2S's map field. It's cut at 31 characters, so a last entry that
/// may be cut short is left out. Muldraugh goes last (the game wants it there).
pub fn map_folders(map: &str) -> Vec<String> {
    let mut maps: Vec<&str> = map.split(';').map(str::trim).filter(|m| !m.is_empty()).collect();
    if map.chars().count() >= 31 && !map.ends_with(';') {
        maps.pop();
    }
    let mut out: Vec<String> = maps.iter().filter(|m| !super::server::is_base_map(m)).map(|m| m.to_string()).collect();
    out.push(super::server::BASE_MAP.into());
    out
}

const TIMEOUT: Duration = Duration::from_millis(1500);
const FRESH: Duration = Duration::from_secs(300);
/// How many servers are asked at once.
const CONCURRENCY: usize = 32;

/// Answers by address, with when they came.
type Answers = Mutex<Option<HashMap<String, (Instant, Server)>>>;
static CACHE: Answers = Mutex::new(None);

/// Ask one server (A2S_INFO, then A2S_RULES). `known`: what the
/// list says, kept where A2S says nothing. Answers are cached for 5 minutes unless `fresh`.
pub fn probe(known: Server, fresh: bool) -> Server {
    let key = known.addr.clone();
    if !fresh && let Some((at, s)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).get(&key)
        && at.elapsed() < FRESH
    {
        return s.clone();
    }
    let mut s = known;
    let res = (|| -> Result<()> {
        let addr = s.addr.to_socket_addrs().context("not an address")?.next().context("not an address")?;
        if s.port == 0 {
            s.port = addr.port();
        }
        // `addr` stays as given (a host name too): favourites and history know it by that
        let c = a2s::Client::new(addr, TIMEOUT)?;
        let (info, rtt) = c.info()?;
        s.take_info(info, rtt);
        s.probed = true;
        s.take_rules(c.rules()?);
        Ok(())
    })();
    s.error = res.err().map(|e| format!("{e:#}")).unwrap_or_default();
    if !s.error.is_empty() {
        // what an earlier answer said isn't true now
        (s.probed, s.ping) = (false, None);
    }
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, (Instant::now(), s.clone()));
    s
}

/// `probe` each, `CONCURRENCY` at a time, in the order given.
pub fn probe_all(servers: Vec<Server>, fresh: bool) -> Vec<Server> {
    let todo = Mutex::new(servers.into_iter().enumerate().collect::<Vec<_>>().into_iter());
    let done = Mutex::new(vec![]);
    std::thread::scope(|scope| {
        for _ in 0..CONCURRENCY {
            scope.spawn(|| {
                while let Some((i, s)) = todo.lock().unwrap_or_else(|e| e.into_inner()).next() {
                    let s = probe(s, fresh);
                    done.lock().unwrap_or_else(|e| e.into_inner()).push((i, s));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(|e| e.into_inner());
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, s)| s).collect()
}

/// Servers that became joinable (answering, with a free slot) since `before`, the addresses
/// joinable last time (None on the first look, which only notes them); and the joinable ones now.
pub fn newly_joinable<'a>(before: Option<&HashSet<String>>, servers: &'a [Server]) -> (HashSet<String>, Vec<&'a Server>) {
    let up: Vec<&Server> = servers.iter().filter(|s| s.probed && (s.max_players == 0 || s.players < s.max_players)).collect();
    let new = before.map(|b| up.iter().filter(|s| !b.contains(&s.addr)).copied().collect()).unwrap_or_default();
    (up.iter().map(|s| s.addr.clone()).collect(), new)
}

/// Who's on (A2S_PLAYER), asked when the player opens the list.
pub fn players(addr: &str) -> Result<Vec<a2s::Player>> {
    let addr = addr.to_socket_addrs().context("not an address")?.next().context("not an address")?;
    a2s::Client::new(addr, TIMEOUT)?.players()
}

// --- mod IDs -> Workshop items ---

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Resolved {
    pub id: String,
    /// "installed"; "found" (an item that declares this mod ID, or the map index); "likely" (the
    /// best Workshop search match, which doesn't declare it); "unknown".
    pub status: String,
    /// Its Workshop item ("" for a mod installed from elsewhere, or not found).
    pub item: String,
    pub title: String,
    /// The item's size and Workshop update time (Unix seconds), when known.
    pub size: u64,
    pub updated: u64,
    /// Its Workshop preview image ("" when not known).
    pub preview: String,
}

/// A Workshop lookup kept in `<data>/server-mods.json` (mod ID -> item; "" when nothing matched).
#[derive(Clone, Serialize, Deserialize)]
struct Lookup {
    item: String,
    sure: bool,
    at: u64,
}

/// Lookups are kept a week; misses a day.
fn fresh(l: &Lookup, now: u64) -> bool {
    now.saturating_sub(l.at) < if l.item.is_empty() { 86_400 } else { 7 * 86_400 }
}

/// A Workshop search for a mod ID: its best item, and whether that item declares the ID.
pub type Search<'a> = dyn FnMut(&str) -> Result<Option<(String, bool)>> + 'a;

/// Resolve the server's mod IDs on `build`: installed mods first, then the map index, then the
/// mod ID index, then the Workshop (`search`: mod ID -> its best candidate and whether it's
/// sure; cached a week). With `within` (a PZMM list's items) only those items count and nothing
/// is searched: a mod no index places stays "found" without an item (see `add_details`).
pub fn resolve(
    paths: &Paths,
    ids: &[String],
    build: u8,
    installed: &BTreeMap<String, store::Row>,
    search: &mut Search<'_>,
    within: &[String],
) -> Vec<Resolved> {
    let file = paths.data.join("server-mods.json");
    let mut cache: BTreeMap<String, Lookup> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let now = store::now();
    let maps: HashMap<String, String> = super::maps::checked(paths)
        .into_iter()
        .flat_map(|(item, c)| c.builds.get(&build).into_iter().flatten().map(|m| (m.id.clone(), item.clone())).collect::<Vec<_>>())
        .collect();
    let by_id = super::mod_index::kept_by_id(paths);
    let candidates = |id: &String| -> Vec<&String> { by_id.get(id).into_iter().flatten().filter(|i| within.is_empty() || within.contains(i)).collect() };
    // an ID several items hold: the item most of the server's other IDs could come from (a
    // pack, or the library its mods share), else the most subscribed
    let mut shared: HashMap<&String, usize> = HashMap::new();
    for id in ids {
        for i in candidates(id) {
            *shared.entry(i).or_default() += 1;
        }
    }
    let mut changed = false;
    let out = ids
        .iter()
        .map(|id| {
            let r = |status: &str, item: &str| Resolved { id: id.clone(), status: status.into(), item: item.into(), ..Default::default() };
            if let Some(m) = installed.get(id) {
                return r("installed", crate::sources::workshop_item(&m.source).unwrap_or(""));
            }
            if let Some(item) = maps.get(id).filter(|i| within.is_empty() || within.contains(i)) {
                return r("found", item);
            }
            let c = candidates(id);
            if let Some(item) = c.iter().enumerate().max_by_key(|(n, i)| (shared[*i], std::cmp::Reverse(*n))).map(|(_, i)| *i) {
                return r("found", item);
            }
            if !within.is_empty() {
                return r("found", "");
            }
            let hit = cache.get(id).filter(|l| fresh(l, now)).cloned();
            let hit = hit.or_else(|| match search(id) {
                Ok(found) => {
                    let (item, sure) = found.unwrap_or_default();
                    let l = Lookup { item, sure, at: now };
                    cache.insert(id.clone(), l.clone());
                    changed = true;
                    Some(l)
                }
                Err(e) => {
                    crate::say!("lookupFailed", "error: looking up {id}: {error}", id = id, error = format!("{e:#}"));
                    None
                }
            });
            match hit {
                Some(l) if !l.item.is_empty() => r(if l.sure { "found" } else { "likely" }, &l.item),
                _ => r("unknown", ""),
            }
        })
        .collect();
    if changed && let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::create_dir_all(&paths.data).and_then(|_| crate::write_atomic(&file, json));
    }
    out
}

/// Fill in title, size and update time from the items' Workshop details (one request); a mod
/// found without an item takes the one whose description names it.
/// An item's Build tags allow `build` (no Build tag: it doesn't say, so it does).
pub fn fits_build(item: &crate::sources::steam_workshop::Item, build: u8) -> bool {
    let builds: Vec<&String> = item.tags.iter().filter(|t| t.starts_with("Build ")).collect();
    builds.is_empty() || builds.iter().any(|t| **t == format!("Build {build}"))
}

/// A mod ID several items hold went to the item most of the server's mods share, which the mod index picks without
/// knowing builds: when that item's Build tags rule out the server's `build`, the most subscribed other holder that fits
/// takes its place (`fetch` reads the details of those not in `details` yet, which it extends). Only outside a server's
/// own list (`within` empty): that list names its items.
pub fn fit_build(
    paths: &Paths,
    list: &mut [Resolved],
    build: u8,
    within: &[String],
    details: &mut Vec<crate::sources::steam_workshop::Item>,
    fetch: impl Fn(&[&str]) -> Vec<crate::sources::steam_workshop::Item>,
) {
    if !within.is_empty() {
        return;
    }
    let wrong = |item: &str, details: &[crate::sources::steam_workshop::Item]| details.iter().find(|d| d.id == item).is_some_and(|d| !fits_build(d, build));
    let by_id = super::mod_index::kept_by_id(paths);
    let others = |r: &Resolved| -> Vec<String> { by_id.get(&r.id).into_iter().flatten().filter(|i| **i != r.item).cloned().collect() };
    let mut ask: Vec<String> = list.iter().filter(|r| r.status == "found" && wrong(&r.item, details)).flat_map(&others).filter(|i| !details.iter().any(|d| d.id == *i)).collect();
    ask.sort();
    ask.dedup();
    if !ask.is_empty() {
        details.extend(fetch(&ask.iter().map(String::as_str).collect::<Vec<_>>()));
    }
    for r in list.iter_mut().filter(|r| r.status == "found") {
        if wrong(&r.item, details)
            && let Some(better) = others(r).into_iter().find(|i| details.iter().any(|d| d.id == *i && fits_build(d, build)))
        {
            r.item = better;
        }
    }
}

pub fn add_details(list: &mut [Resolved], details: &[crate::sources::steam_workshop::Item]) {
    for r in list {
        if r.status == "found" && r.item.is_empty()
            && let Some(d) = details.iter().find(|d| super::workshop_mod_ids(&d.description).contains(&r.id))
        {
            r.item = d.id.clone();
        }
        if let Some(d) = details.iter().find(|d| d.id == r.item) {
            (r.title, r.size, r.updated, r.preview) = (d.title.clone(), d.file_size, d.time_updated, d.preview_url.clone());
        }
    }
}

/// One of a PZMM list's Workshop items: what the server runs and where the Workshop is now.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ListItem {
    pub item: String,
    pub title: String,
    pub size: u64,
    /// Its Workshop update time now, and when the server's list was made (0: unknown).
    pub updated: u64,
    pub pinned: u64,
    pub installed: bool,
    pub preview: String,
}

/// A PZMM list's items with their details; `installed`: the mods installed now.
pub fn list_items(tag: &super::tag::Tag, details: &[crate::sources::steam_workshop::Item], installed: &BTreeMap<String, store::Row>) -> Vec<ListItem> {
    tag.items
        .iter()
        .map(|(item, pinned)| {
            let d = details.iter().find(|d| &d.id == item);
            ListItem {
                item: item.clone(),
                title: d.map(|d| d.title.clone()).unwrap_or_default(),
                size: d.map_or(0, |d| d.file_size),
                updated: d.map_or(0, |d| d.time_updated),
                pinned: *pinned,
                installed: installed.values().any(|r| crate::sources::workshop_item(&r.source) == Some(item.as_str())),
                preview: d.map(|d| d.preview_url.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

/// "Server <name>" as a valid profile name.
pub fn profile_name(server: &str) -> String {
    let clean = crate::clean_file_name(server);
    let short: String = clean.chars().take(48).collect();
    let name = format!("Server {}", short.trim_end_matches(['.', ' ']));
    if crate::safe_file_name(&name) { name } else { "Server".into() }
}

/// The game's arguments to join: `+connect ip:port`, and `+password` when there is one.
pub fn join_args(connect: &str, password: &str) -> Result<String> {
    // a value starting with - or + would read as another of the game's arguments (a link can carry one)
    let arg_like = |v: &str| v.starts_with(['-', '+']) || v.contains(char::is_control);
    if connect.is_empty() || connect.contains(char::is_whitespace) || arg_like(connect) {
        bail!("not a server address: {connect:?}");
    }
    if password.contains(char::is_whitespace) || arg_like(password) {
        bail!("PZMM can't pass this password to the game (it has spaces, or starts with - or +): join, then type it in the game");
    }
    Ok(if password.is_empty() { format!("+connect {connect}") } else { format!("+connect {connect} +password {password}") })
}

// --- favourites and join history ---

/// One join: the exact mod set used.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Join {
    pub time: u64,
    /// The load order.
    pub mods: Vec<String>,
    pub maps: Vec<String>,
    /// Workshop item -> its Workshop update time then (0 if unknown).
    pub items: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub favourites: Vec<Server>,
    /// Servers joined (by `addr`), newest first, as last seen.
    pub history: Vec<Server>,
    /// By `addr`: its joins, newest first.
    pub joins: BTreeMap<String, Vec<Join>>,
}

/// Joins kept per server, and servers in the history.
const KEEP: usize = 10;
const HISTORY: usize = 50;

fn state_file(paths: &Paths) -> std::path::PathBuf {
    paths.data.join("launcher.json")
}

/// The favourites and history (a damaged file is kept aside, see `read_json`; an unreadable one reads as none).
pub fn state(paths: &Paths) -> State {
    crate::read_json(&state_file(paths)).unwrap_or_default()
}

fn save_state(paths: &Paths, s: &State) -> Result<()> {
    std::fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&state_file(paths), serde_json::to_string_pretty(s)?)?;
    servers_for_game(paths, s);
    Ok(())
}

/// The servers you joined for the PZMM - Client mod's Servers tab: `Zomboid/Lua/pzmm/servers.txt`, newest first,
/// `<name>\t<ip:game port>\t<last joined>` lines. Only where PZMM - Client is.
pub fn servers_for_game(paths: &Paths, s: &State) {
    let dir = crate::pz::bridge::dir(paths);
    if !dir.is_dir() {
        return;
    }
    let one = |t: &str| t.replace(['\t', '\r', '\n'], " ");
    let text: String = s
        .history
        .iter()
        .map(|v| {
            let ip = v.addr.rsplit_once(':').map_or(v.addr.as_str(), |(ip, _)| ip);
            let last = s.joins.get(&v.addr).and_then(|j| j.first()).map_or(0, |j| j.time);
            format!("{}\t{ip}:{}\t{last}\n", one(&v.name), v.port)
        })
        .collect();
    super::bridge::write_for_game(&dir.join("servers.txt"), text);
}

/// What's kept of a server: what the list shows (not its rules or description).
fn slim(s: &Server) -> Server {
    Server { description: String::new(), rules: BTreeMap::new(), tag: None, tag_error: String::new(), ping: None, probed: false, error: String::new(), friends: vec![], ..s.clone() }
}

pub fn set_favourite(paths: &Paths, server: &Server, on: bool) -> Result<State> {
    let mut s: State = crate::read_json(&state_file(paths))?;
    s.favourites.retain(|f| f.addr != server.addr);
    if on {
        s.favourites.push(slim(server));
    }
    save_state(paths, &s)?;
    Ok(s)
}

/// Note a join in the history; `join` is the mod set used, `None` when it wasn't the server's own
/// (joined as is, or a vanilla server), so "Use this setup" never offers an unrelated profile.
pub fn record_join(paths: &Paths, server: &Server, join: Option<Join>) -> Result<()> {
    let mut s: State = crate::read_json(&state_file(paths))?;
    s.history.retain(|h| h.addr != server.addr);
    s.history.insert(0, slim(server));
    s.history.truncate(HISTORY);
    if let Some(join) = join {
        let joins = s.joins.entry(server.addr.clone()).or_default();
        joins.insert(0, join);
        joins.truncate(KEEP);
    }
    save_state(paths, &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steams_server_list() {
        // the shape IGameServersService/GetServerList returns (no key here: a recorded-style reply)
        let json = r#"{"response":{"servers":[
            {"addr":"203.0.113.5:16261","gameport":16261,"steamid":"90294480967390208","name":"Test B42","appid":108600,"gamedir":"zomboid","version":"1.0.0.0","product":"zomboid","region":255,"players":12,"max_players":45,"bots":0,"map":"Muldraugh, KY","secure":true,"dedicated":true,"os":"l","gametype":";modded;pvp;VERSION:42.21"},
            {"addr":"198.51.100.7:27015","gameport":16300,"name":"Old B41","players":0,"max_players":16,"map":"Muldraugh, KY","gametype":""},
            {"name":"no address"}]}}"#;
        let l = parse_list(json).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].connect(), l[0].build.as_str(), l[0].pvp, l[0].players), ("203.0.113.5:16261".into(), "42.21", true, 12));
        assert_eq!((l[1].connect(), l[1].build.as_str(), l[1].pvp), ("198.51.100.7:16300".into(), "", false));
        assert!(parse_list(r#"{"response":{}}"#).unwrap().is_empty(), "no servers");
        assert!(parse_list("<html>").is_err());
        assert!(list("not a key!").is_err(), "a bad key never reaches Steam");
    }

    #[test]
    fn merged_sources() {
        let s = |addr: &str, name: &str, map: &str| Server { addr: addr.into(), port: 16261, name: name.into(), map: map.into(), ..Default::default() };
        let l = merge(vec![
            ("game", Ok(vec![s("203.0.113.5:16261", "Ours", ""), s("203.0.113.9:16261", "Only saved", "")])),
            ("steam", Ok(vec![s("203.0.113.5:16261", "Ours", "Muldraugh, KY"), s("198.51.100.7:16261", "Other", "Riverside, KY"), s("203.0.113.9:16261", "", "")])),
            ("web", Err(anyhow::anyhow!("no key"))),
        ]);
        let got: Vec<(&str, &str)> = l.servers.iter().map(|s| (s.addr.as_str(), s.map.as_str())).collect();
        assert_eq!(got, [("203.0.113.5:16261", "Muldraugh, KY"), ("203.0.113.9:16261", ""), ("198.51.100.7:16261", "Riverside, KY")], "each once, the fuller record kept");
        assert_eq!(l.servers[1].name, "Only saved", "a poorer duplicate never replaces");
        let answers: Vec<(&str, usize, bool)> = l.sources.iter().map(|a| (a.source.as_str(), a.count, a.error.is_empty())).collect();
        assert_eq!(answers, [("game", 2, true), ("steam", 3, true), ("web", 0, false)]);
    }

    #[test]
    fn the_games_own_list_and_friends_servers() {
        let t = tempfile::tempdir().unwrap();
        assert!(game_list(t.path()).unwrap().is_empty(), "no list yet");
        std::fs::create_dir_all(t.path().join("db")).unwrap();
        let c = rusqlite::Connection::open(t.path().join("db/ServerListSteam.db")).unwrap();
        // the game's own schema (zombie/savefile/AccountDBHelper)
        c.execute_batch("CREATE TABLE server (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, ip TEXT NOT NULL, port INTEGER NOT NULL, serverPassword TEXT, description TEXT, mods TEXT, icon BLOB, banner BLOB, panelBackground BLOB, screenBackground BLOB, lastOnline TEXT, lastDataUpdate TEXT);
            INSERT INTO server (name, ip, port, serverPassword, description, mods) VALUES ('Our place', '203.0.113.5', 16261, 'secret', 'Hi <LINE> all', 'ModA;\\ModB');
            INSERT INTO server (name, ip, port) VALUES ('No port', '203.0.113.6', 0);").unwrap();
        drop(c);
        let l = game_list(t.path()).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].addr.as_str(), l[0].connect(), l[0].name.as_str(), l[0].description.as_str(), l[0].mods.clone()), ("203.0.113.5:16261", "203.0.113.5:16261".to_string(), "Our place", "Hi\nall", vec!["ModA".to_string(), "ModB".into()]));
        assert!(!format!("{l:?}").contains("secret"), "saved passwords are never read");

        // who's signed in to Steam: its account ID as a SteamID64
        use crate::sources::steam_workshop::{active_user_in, steam_id64};
        assert_eq!(active_user_in("\"Registry\"
{
	\"ActiveProcess\"
	{
		\"pid\"		\"123\"
		\"ActiveUser\"		\"22202\"
	}
}
"), Some(22202));
        assert_eq!(steam_id64(22202), "76561197960287930");
        // GetPlayerSummaries players: in PZ on a server, in PZ in the menu, in another game
        let players: Vec<serde_json::Value> = serde_json::from_str(r#"[
            {"steamid":"1","personaname":"Kate","gameid":"108600","gameserverip":"203.0.113.5:16261"},
            {"steamid":"2","personaname":"Sam","gameid":"108600","gameserverip":"203.0.113.5:16261"},
            {"steamid":"3","personaname":"Ann","gameid":"108600"},
            {"steamid":"4","personaname":"Bo","gameid":"108600","gameserverip":"0.0.0.0:0"},
            {"steamid":"5","personaname":"Cy","gameid":"730","gameserverip":"198.51.100.1:27015"}]"#).unwrap();
        let f = friend_servers(&players);
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].connect(), f[0].friends.clone()), ("203.0.113.5:16261".to_string(), vec!["Kate".to_string(), "Sam".into()]));
        assert!(friends("not a key!").is_err());
    }

    #[test]
    fn rules_from_a_capture() {
        let mut s = Server { addr: "127.0.0.1:16261".into(), keywords: "hidden;modded;pvp;VERSION:42.21".into(), ..Default::default() };
        s.keywords_said();
        s.take_rules(a2s::parse_rules(&a2s::tests::fixture("rules")[4..]).unwrap());
        assert_eq!(s.mods, ["SpawnSelector", "BrandenburgEkronIrvingtonSpawnLocations", "beek_muldraugh_firedept", "pzmm_fake_mod_01", "pzmm_fake_mod_02"]);
        assert_eq!((s.mod_count, s.build.as_str(), s.pvp), (48, "42.21", true));
        // two sets: the stale 6-chunk one and the 16-chunk one set later; the longer complete set wins
        assert_eq!((s.description.chars().count(), &s.description[..4]), (15 * 127 + 100, "LIVE"));
        assert!(!s.rules.keys().any(|k| k.starts_with("description") || k == "mods"));
        assert_eq!(s.rules.get("open").map(String::as_str), Some("1"));
    }

    #[test]
    fn description_markup_becomes_plain_text() {
        let rules = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let d = plain_text(&raw_description(&rules(&[
            ("description:2/2", "<LINE> <RED> Wiped 16.09 <LINE> <LINE> <LINE> a <3 b <not a tag>"),
            ("description:1/2", "<RGB:1,0.55,0> [GER] Last Resort <SIZE:large>PvPvE\\n \\n Discord: x"),
        ])));
        assert_eq!(d, "[GER] Last Resort PvPvE\n\nDiscord: x\nWiped 16.09\n\na <3 b <not a tag>");
        assert_eq!(raw_description(&rules(&[("description", "Sunday Drivers Season 12")])), "Sunday Drivers Season 12");
        assert_eq!(raw_description(&rules(&[("description:1/2", "half")])), "", "an incomplete set isn't shown");
    }

    #[test]
    fn mod_and_map_lines() {
        assert_eq!(super::super::server::split(r"\Common Sense;\ SpawnSelector;;plain "), ["Common Sense", "SpawnSelector", "plain"]);
        // cut at 31 characters: the last entry may be cut short
        assert_eq!(map_folders("RavenCreek;Grapeseed;BedfordFal"), ["RavenCreek", "Grapeseed", "Muldraugh, KY"]);
        assert_eq!(map_folders("Muldraugh_FireDept;Muldraugh, K"), ["Muldraugh_FireDept", "Muldraugh, KY"]);
        assert_eq!(map_folders("Muldraugh, KY"), ["Muldraugh, KY"]);
        assert_eq!(map_folders("AZSpawn;Muldraugh, KY;"), ["AZSpawn", "Muldraugh, KY"]);
    }

    #[test]
    fn join_arguments() {
        assert_eq!(join_args("203.0.113.5:16261", "").unwrap(), "+connect 203.0.113.5:16261");
        assert_eq!(join_args("203.0.113.5:16261", "pw1").unwrap(), "+connect 203.0.113.5:16261 +password pw1");
        assert!(join_args("203.0.113.5:16261", "two words").is_err());
        assert!(join_args("203.0.113.5:16261", "-cachedir=\\\\host\\share").is_err(), "no smuggled arguments");
        assert!(join_args("203.0.113.5:16261", "+connect").is_err() && join_args("203.0.113.5:16261", "a\u{7}").is_err());
        assert!(join_args("-debug", "").is_err());
        assert!(join_args("203.0.113.5:16261", "p-w+1").is_ok());
        assert!(join_args("", "").is_err());
        assert_eq!(profile_name("[ID|PZ] KOMPLEK: BOOMER?"), "Server [ID PZ] KOMPLEK BOOMER");
        assert_eq!(profile_name("..."), "Server");
        assert_eq!(profile_name(&"x".repeat(100)).len(), 55);
    }

    fn row(id: &str, source: &str, requires: &[&str]) -> (String, store::Row) {
        let r = store::Row {
            id: id.into(),
            name: id.into(),
            source: source.into(),
            dir: Default::default(),
            info: Default::default(),
            requires: requires.iter().map(|s| s.to_string()).collect(),
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: true,
            supports_42: true,
            ..Default::default()
        };
        (id.into(), r)
    }

    #[test]
    fn resolves_installed_then_cache_then_workshop() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let installed: BTreeMap<String, store::Row> = [row("Lib", "steam:111", &[]), row("Local", "local:C:/x", &[])].into();
        let ids: Vec<String> = ["Lib", "Local", "Sure", "Maybe", "Nothing", "Broken"].map(String::from).into();
        let mut asked = vec![];
        let mut search = |id: &str| -> Result<Option<(String, bool)>> {
            asked.push(id.to_string());
            match id {
                "Sure" => Ok(Some(("222".into(), true))),
                "Maybe" => Ok(Some(("333".into(), false))),
                "Broken" => bail!("offline"),
                _ => Ok(None),
            }
        };
        let r = resolve(&paths, &ids, 42, &installed, &mut search, &[]);
        let got: Vec<(&str, &str)> = r.iter().map(|r| (r.status.as_str(), r.item.as_str())).collect();
        assert_eq!(got, [("installed", "111"), ("installed", ""), ("found", "222"), ("likely", "333"), ("unknown", ""), ("unknown", "")]);
        assert_eq!(asked, ["Sure", "Maybe", "Nothing", "Broken"]);
        // the second time only the failed lookup goes to the Workshop
        let mut again = vec![];
        let r2 = resolve(&paths, &ids, 42, &installed, &mut |id| {
            again.push(id.to_string());
            Ok(None)
        }, &[]);
        assert_eq!(again, ["Broken"]);
        assert_eq!(r2[2].item, "222");

        let mut list = r;
        add_details(&mut list, &[crate::sources::steam_workshop::Item { id: "222".into(), title: "Sure Mod".into(), file_size: 5, time_updated: 9, preview_url: "p.jpg".into(), ..Default::default() }]);
        assert_eq!((list[2].title.as_str(), list[2].size, list[2].updated, list[2].preview.as_str()), ("Sure Mod", 5, 9, "p.jpg"));
    }

    #[test]
    fn the_mod_id_index_and_a_pzmm_list() {
        let (_t, paths) = crate::store::tests::temp_paths();
        // "Shared" is in a pack (500) and a popular single item (400); the pack also holds "Other"
        std::fs::create_dir_all(&paths.data).unwrap();
        std::fs::write(
            paths.data.join("mod-index.json"),
            r#"{"at":1,"items":{"400":{"u":1,"s":9000,"i":["Shared"]},"500":{"u":1,"s":10,"i":["Shared","Other"]},"600":{"u":1,"s":5,"i":["Lone"]}}}"#,
        )
        .unwrap();
        let ids: Vec<String> = ["Shared", "Other", "Lone", "Nobody"].map(String::from).into();
        let asked = std::cell::RefCell::new(vec![]);
        let mut search = |id: &str| -> Result<Option<(String, bool)>> {
            asked.borrow_mut().push(id.to_string());
            Ok(None)
        };
        let got = |r: Vec<Resolved>| r.into_iter().map(|r| (r.status, r.item)).collect::<Vec<_>>();
        let pair = |s: &str, i: &str| (s.to_string(), i.to_string());
        let r = got(resolve(&paths, &ids, 42, &BTreeMap::new(), &mut search, &[]));
        assert_eq!(r, [pair("found", "500"), pair("found", "500"), pair("found", "600"), pair("unknown", "")], "the pack the server's other mods share wins");
        assert_eq!(*asked.borrow(), ["Nobody"], "only what no index knows is searched");
        // a PZMM list: only its items count, nothing is searched, and a description places the rest
        asked.borrow_mut().clear();
        let mut r = resolve(&paths, &ids, 42, &BTreeMap::new(), &mut search, &["400".into(), "700".into()]);
        assert!(asked.borrow().is_empty());
        let d = |id: &str, desc: &str| crate::sources::steam_workshop::Item { id: id.into(), description: desc.into(), ..Default::default() };
        add_details(&mut r, &[d("700", "Mod ID: Nobody")]);
        assert_eq!(got(r), [pair("found", "400"), pair("found", ""), pair("found", ""), pair("found", "700")]);
    }

    #[test]
    fn a_pzmm_list_in_the_rules() {
        let t = super::super::tag::Tag { items: vec![("111".into(), 5)], mods: vec!["A".into(), "B".into()], maps: vec!["Town".into(), "Muldraugh, KY".into()], ..Default::default() };
        let (desc, _) = super::super::tag::with_tag(r"<RED> Friends \n Discord: x", &t);
        let chunks: Vec<(String, String)> = desc.as_bytes().chunks(127).enumerate().map(|(i, c)| (format!("description:{}/{}", i + 1, desc.len().div_ceil(127)), String::from_utf8(c.to_vec()).unwrap())).collect();
        let mut s = Server::default();
        s.take_rules(chunks.clone());
        assert_eq!((s.tag.as_ref(), s.description.as_str(), s.tag_error.as_str()), (Some(&t), "Friends\nDiscord: x", ""));
        // the last chunk missing (cut at A2S's limit): no list, and why
        let mut cut = Server::default();
        cut.take_rules(vec![("description".into(), desc[..desc.len() - 10].into())]);
        assert_eq!((cut.tag, cut.tag_error.as_str()), (None, "cut"));
        let items = list_items(&t, &[crate::sources::steam_workshop::Item { id: "111".into(), title: "Pack".into(), time_updated: 9, ..Default::default() }], &[row("A", "steam:111", &[])].into());
        assert_eq!(items, [ListItem { item: "111".into(), title: "Pack".into(), size: 0, updated: 9, pinned: 5, installed: true, preview: String::new() }]);
        assert!(slim(&s).tag.is_none());
    }

    #[test]
    fn favourites_coming_online() {
        let srv = |addr: &str, probed: bool, players: u32| Server { addr: addr.into(), probed, players, max_players: 8, ..Default::default() };
        let first = [srv("a:1", true, 1), srv("b:1", false, 0)];
        let (up, new) = newly_joinable(None, &first);
        assert!(new.is_empty(), "the first look only notes who's up");
        let now = [srv("a:1", true, 2), srv("b:1", true, 0), srv("c:1", true, 8)];
        let (up, new) = newly_joinable(Some(&up), &now);
        assert_eq!(new.iter().map(|s| s.addr.as_str()).collect::<Vec<_>>(), ["b:1"], "c is full");
        // going down and back up tells again
        let (down, back) = ([srv("b:1", false, 0)], [srv("b:1", true, 0)]);
        let (up, _) = newly_joinable(Some(&up), &down);
        assert_eq!(newly_joinable(Some(&up), &back).1.len(), 1);
    }

    #[test]
    fn favourites_and_join_history() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let srv = Server { addr: "203.0.113.5:16261".into(), port: 16261, name: "A".into(), description: "long".into(), ping: Some(30), ..Default::default() };
        assert_eq!(set_favourite(&paths, &srv, true).unwrap().favourites, [Server { description: String::new(), ping: None, ..srv.clone() }]);
        assert!(set_favourite(&paths, &srv, false).unwrap().favourites.is_empty());
        for t in 0..12 {
            record_join(&paths, &srv, Some(Join { time: t, mods: vec!["A".into()], ..Default::default() })).unwrap();
        }
        record_join(&paths, &srv, None).unwrap();
        let s = state(&paths);
        assert_eq!(s.history.len(), 1);
        let joins = &s.joins["203.0.113.5:16261"];
        assert_eq!((joins.len(), joins[0].time), (10, 11), "newest first, ten kept");
        // the game's copy (PZMM - Client's servers.txt), once its folder is there
        std::fs::create_dir_all(paths.zomboid.join("Lua/pzmm")).unwrap();
        record_join(&paths, &srv, None).unwrap();
        assert_eq!(std::fs::read_to_string(paths.zomboid.join("Lua/pzmm/servers.txt")).unwrap(), "A\t203.0.113.5:16261\t11\n");
    }

    /// Real servers, politely (one pass, cached): `PZMM_A2S_LIVE=ip:port,ip:port cargo test -p pzmm-core live_servers -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_servers() {
        let addrs = std::env::var("PZMM_A2S_LIVE").unwrap_or_default();
        let list: Vec<Server> = addrs.split(',').filter(|a| !a.is_empty()).map(|a| Server { addr: a.into(), ..Default::default() }).collect();
        for s in probe_all(list, true) {
            println!("{} | {} | {:?} ms | {} | {}/{} | mods {}/{} {:?} | pw {} | desc {} chars | err {:?}", s.addr, s.name, s.ping, s.build, s.players, s.max_players, s.mods.len(), s.mod_count, s.mods.first(), s.password, s.description.len(), s.error);
            let t = s.tag.as_ref();
            println!("  PZMM list: {:?} | items {:?} first {:?} | mods {:?} last {:?} | maps {:?} | description {:?}", s.tag_error, t.map(|t| t.items.len()), t.and_then(|t| t.items.first()), t.map(|t| t.mods.len()), t.and_then(|t| t.mods.last()), t.map(|t| &t.maps), s.description);
            if s.probed && s.players > 0 {
                let p = players(&s.addr).map(|p| (p.len(), p.iter().map(|p| p.duration).fold(0.0, f32::max)));
                println!("  players: {:?} (count, longest seconds)", p.map_err(|e| e.to_string()));
            }
        }
    }

    /// A server that answers A2S on localhost (no challenge, as PZ does there).
    #[test]
    fn probes_and_caches() {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = sock.local_addr().unwrap().to_string();
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let n = asked.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1500];
            while let Ok((len, from)) = sock.recv_from(&mut buf) {
                n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let reply = if buf[4] == b'T' { a2s::tests::fixture("info") } else { a2s::tests::fixture("rules") };
                let _ = len;
                sock.send_to(&reply, from).unwrap();
            }
        });
        let known = Server { addr: addr.clone(), name: "from the list".into(), ..Default::default() };
        let got = probe_all(vec![known.clone(), Server { addr: "127.0.0.1:1".into(), ..Default::default() }], false);
        assert_eq!((got[0].name.as_str(), got[0].port, got[0].mod_count, got[0].probed), ("pzmmtest", 16261, 48, true));
        assert!(got[0].ping.is_some());
        assert!(!got[1].probed && !got[1].error.is_empty(), "{:?}", got[1].error);
        let before = asked.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(probe(known.clone(), false), got[0], "cached");
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), before, "no new queries");
        probe(known, true);
        assert!(asked.load(std::sync::atomic::Ordering::SeqCst) > before, "fresh asks again");
    }
}
