//! Steam Workshop: anonymous downloads through SteamCMD, and the folder where the
//! Steam client keeps the user's own subscriptions.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::pz::Paths;

const STEAMCMD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/";

/// `steamapps/workshop/content/<app>` in whichever Steam library has it.
pub fn content_dir(app: &str) -> Option<PathBuf> {
    library_path(&format!("steamapps/workshop/content/{app}"))
}

/// Where the Steam client may be installed, the registered one first.
pub fn steam_roots() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    #[allow(unused_mut)]
    let mut roots = vec![
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        if let Ok(p) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam").and_then(|k| k.get_value::<String, _>("SteamPath")) {
            roots.insert(0, PathBuf::from(p));
        }
    }
    roots
}

/// `<library>/<rel>` for the first Steam library that has it, e.g. `steamapps/common/ProjectZomboid`.
pub fn library_path(rel: &str) -> Option<PathBuf> {
    for root in steam_roots() {
        let vdf = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
        let libs = vdf.lines().filter_map(|l| {
            let v = l.trim().strip_prefix("\"path\"")?.trim().trim_matches('"');
            Some(PathBuf::from(v.replace("\\\\", "\\")))
        });
        for lib in std::iter::once(root.clone()).chain(libs) {
            let d = rel.split('/').fold(lib.clone(), |p, s| p.join(s));
            if d.is_dir() {
                return Some(d);
            }
        }
    }
    None
}

/// The SteamID64 of the account signed in to the running Steam client: Steam records its account
/// ID while it runs (`ActiveProcess\ActiveUser`, in the registry on Windows, `registry.vdf` elsewhere).
pub fn active_user() -> Option<String> {
    #[cfg(windows)]
    let id = {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam\ActiveProcess").and_then(|k| k.get_value::<u32, _>("ActiveUser")).ok()
    };
    #[cfg(not(windows))]
    let id = dirs::home_dir().and_then(|h| std::fs::read_to_string(h.join(".steam/registry.vdf")).ok()).and_then(|t| active_user_in(&t));
    id.filter(|i| *i != 0).map(steam_id64)
}

/// `"ActiveUser" "<account id>"` in Steam's registry.vdf.
pub fn active_user_in(vdf: &str) -> Option<u32> {
    vdf.lines().find_map(|l| {
        let parts: Vec<&str> = l.split('"').filter(|p| !p.trim().is_empty()).collect();
        match parts.as_slice() {
            [k, v] if k.eq_ignore_ascii_case("ActiveUser") => v.parse().ok(),
            _ => None,
        }
    })
}

/// A Steam account ID as the SteamID64 the Web API takes (individual accounts, public universe).
pub fn steam_id64(account: u32) -> String {
    (76_561_197_960_265_728u64 + u64::from(account)).to_string()
}

/// Whether the Steam client is running (Steam records its pid while it runs).
pub fn client_running() -> bool {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam\ActiveProcess")
            .and_then(|k| k.get_value::<u32, _>("pid"))
            .is_ok_and(|pid| pid != 0)
    }
    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .and_then(|h| std::fs::read_to_string(h.join(".steam/steam.pid")).ok())
            .and_then(|pid| pid.trim().parse::<u32>().ok())
            .is_some_and(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists())
    }
}

// --- Workshop web data (no API key needed) ---

/// One Workshop item as shown in the browser.
#[derive(Clone, Debug, Default, serde::Serialize, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub preview_url: String,
    pub description: String,
    pub subscriptions: u64,
    pub time_updated: u64,
    pub file_size: u64,
    pub tags: Vec<String>,
    /// Required items (or collection members) Steam lists for it.
    pub num_children: u64,
    /// Steam's 0-5 star rating and how many votes it's based on (browse results only).
    pub stars: u64,
    pub votes: u64,
    pub views: u64,
    pub favorited: u64,
    pub comments: u64,
    pub time_created: u64,
    /// The author's Steam id, and their name when the source has it (browse results do).
    pub creator: String,
    pub author: String,
    /// A collection rather than a mod.
    pub collection: bool,
    /// Its depot manifest (`hcontent_file`): the file list Steam serves. Too big for JS numbers.
    #[serde(skip)]
    pub hcontent: u64,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Page {
    pub total: u64,
    pub items: Vec<Item>,
}

fn num(v: &Value, key: &str) -> u64 {
    match &v[key] {
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

impl Item {
    fn from_json(v: &Value) -> Item {
        let desc = if v["short_description"].is_string() { text(v, "short_description") } else { text(v, "description") };
        Item {
            id: text(v, "publishedfileid"),
            title: text(v, "title"),
            preview_url: text(v, "preview_url"),
            description: desc,
            subscriptions: num(v, "subscriptions"),
            time_updated: num(v, "time_updated"),
            file_size: num(v, "file_size"),
            tags: v["tags"].as_array().into_iter().flatten().filter_map(|t| t["tag"].as_str().map(String::from)).collect(),
            num_children: num(v, "num_children"),
            stars: num(v, "star_rating"),
            votes: num(v, "total_votes"),
            views: num(v, "views"),
            favorited: num(v, "favorited"),
            comments: num(v, "num_comments_public"),
            time_created: num(v, "time_created"),
            creator: text(v, "creator"),
            author: String::new(),
            collection: num(v, "file_type") == 2,
            hcontent: num(v, "hcontent_file"),
        }
    }
}

/// Browse sort orders, as Steam names them. Collections only sort by the first five; "textsearch" is relevance.
const SORTS: &[&str] = &["textsearch", "trend", "toprated", "mostrecent", "lastupdated", "totaluniquesubscribers", "num_parent_collections"];

/// One Workshop browse request: everything Steam's browse page filters on anonymously.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Query {
    pub search: String,
    /// Where the words must appear: 0 anywhere, 1 title, 2 description.
    pub scope: u8,
    /// One of [`SORTS`]; "textsearch" (relevance) only applies with a search.
    pub sort: String,
    /// Popularity window for the "trend" sort, in days (1, 7, 30, 90, 180, 365; -1 all time).
    pub days: i32,
    /// From 1.
    pub page: u32,
    /// 30 or 50 (Steam ignores other sizes).
    pub per_page: u32,
    /// Tags every result has, and tags none has (e.g. "Build 42", "Map").
    pub required: Vec<String>,
    pub excluded: Vec<String>,
    /// Collections instead of mods.
    pub collections: bool,
    /// Only what depends on (or, with `collections`, contains) this item.
    pub child: String,
    /// Only items other items require (the libraries).
    pub libraries: bool,
    /// Only items the author marked incompatible.
    pub incompatible: bool,
    /// Unix-time ranges; 0 = open.
    pub created_from: u64,
    pub created_to: u64,
    pub updated_from: u64,
    pub updated_to: u64,
}

fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The browse page URL for `q`.
fn browse_url(app: &str, q: &Query) -> String {
    let search = q.search.trim();
    let mut sort = if SORTS.contains(&q.sort.as_str()) && (q.sort != "textsearch" || !search.is_empty()) { q.sort.as_str() } else { "trend" };
    if q.collections && !["trend", "toprated", "mostrecent", "lastupdated", "textsearch"].contains(&sort) {
        sort = "trend"; // other sorts return nothing for collections
    }
    let section = if q.collections { "collections" } else { "readytouseitems" };
    let mut url = format!("https://steamcommunity.com/workshop/browse/?appid={app}&section={section}&browsesort={sort}&actualsort={sort}&p={}", q.page.max(1));
    if sort == "trend" {
        url += &format!("&days={}", if q.days == 0 { 7 } else { q.days });
    }
    if !search.is_empty() {
        url += &format!("&searchtext={}", enc(search));
        if q.scope > 0 {
            url += &format!("&search_text_target={}", q.scope.min(2));
        }
    }
    if q.per_page == 50 {
        url += "&num_per_page=50";
    }
    for tag in &q.required {
        url += &format!("&requiredtags%5B%5D={}", enc(tag));
    }
    for tag in &q.excluded {
        url += &format!("&excludedtags%5B%5D={}", enc(tag));
    }
    if valid_id(&q.child) {
        url += &format!("&childpublishedfileid={}", q.child);
    }
    if q.libraries {
        url += "&special_filter=6";
    }
    if q.incompatible {
        url += "&requiredflags%5B%5D=incompatible";
    }
    for (key, v) in [
        ("created_date_range_filter_start", q.created_from),
        ("created_date_range_filter_end", q.created_to),
        ("updated_date_range_filter_start", q.updated_from),
        ("updated_date_range_filter_end", q.updated_to),
    ] {
        if v > 0 {
            url += &format!("&{key}={v}");
        }
    }
    url
}

/// Search or browse the Workshop for `app`.
pub fn browse(app: &str, q: &Query) -> Result<Page> {
    parse_browse(&String::from_utf8_lossy(&cached_get(&browse_url(app, q))?))
}

/// Steam renders the browse page server-side and embeds its data as JSON
/// (`<script id="valve-ssr-data">`), with nested JSON strings inside.
fn parse_browse(html: &str) -> Result<Page> {
    let start = html.find("id=\"valve-ssr-data\"").and_then(|i| html[i..].find('>').map(|j| i + j + 1));
    let Some(start) = start else { bail!("Steam returned a page PZMM can't read (no result data)") };
    let end = html[start..].find("</script>").map(|j| start + j).unwrap_or(html.len());
    let data: Value = serde_json::from_str(&html[start..end]).context("reading Steam's result data")?;
    fn find(v: &Value, page: &mut Option<Page>) {
        if page.is_some() {
            return;
        }
        match v {
            Value::String(s) if s.starts_with('{') || s.starts_with('[') => {
                if let Ok(inner) = serde_json::from_str::<Value>(s) {
                    find(&inner, page);
                }
            }
            Value::Object(o) => {
                if let Some(Value::Array(results)) = o.get("results")
                    && results.first().is_some_and(|r| r.get("publishedfileid").is_some())
                {
                    // authors' names come alongside, keyed by Steam id
                    let names: std::collections::HashMap<&str, &str> = o
                        .get("creator_player_link_details")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|p| Some((p["public_data"]["steamid"].as_str()?, p["public_data"]["persona_name"].as_str()?)))
                        .collect();
                    let items = results
                        .iter()
                        .map(|r| {
                            let mut i = Item::from_json(r);
                            i.author = names.get(i.creator.as_str()).map(|n| n.to_string()).unwrap_or_default();
                            i
                        })
                        .collect();
                    *page = Some(Page { total: num(v, "total_count"), items });
                    return;
                }
                o.values().for_each(|x| find(x, page));
            }
            Value::Array(a) => a.iter().for_each(|x| find(x, page)),
            _ => {}
        }
    }
    let mut page = None;
    find(&data, &mut page);
    Ok(page.unwrap_or_default())
}

/// Steam answers kept for a few minutes: paging back and forth, reopening an item or a big
/// collection's details would otherwise ask Steam again and run into its rate limit (HTTP 429).
fn cached(key: String, fetch: impl FnOnce() -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    use std::time::{Duration, Instant};
    const KEEP: Duration = Duration::from_secs(5 * 60);
    // ponytail: in memory for the session only; persist to disk if restarts still hit 429s
    type Answers = std::collections::HashMap<String, (Instant, Vec<u8>)>;
    static CACHE: std::sync::Mutex<Option<Answers>> = std::sync::Mutex::new(None);
    if let Some((_, hit)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|c| c.get(&key)).filter(|(at, _)| at.elapsed() < KEEP) {
        return Ok(hit.clone());
    }
    let got = fetch()?;
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let c = c.get_or_insert_with(Default::default);
    c.retain(|_, (at, _)| at.elapsed() < KEEP);
    if c.len() >= 64 && let Some(oldest) = c.iter().min_by_key(|(_, (at, _))| *at).map(|(k, _)| k.clone()) {
        c.remove(&oldest);
    }
    c.insert(key, (Instant::now(), got.clone()));
    Ok(got)
}

fn cached_get(url: &str) -> Result<Vec<u8>> {
    cached(url.to_string(), || super::http_get(url))
}

fn post_form(url: &str, body: &str) -> Result<Value> {
    // not cached: update checks and installs need what Steam says now
    let bytes = super::polite(url, || {
        super::agent()
            .post(url)
            .header("User-Agent", crate::USER_AGENT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body)?
            .body_mut()
            .with_config()
            .limit(64 << 20)
            .read_to_vec()
    })?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn id_list(key: &str, ids: &[&str]) -> String {
    let mut body = format!("{key}={}", ids.len());
    for (i, id) in ids.iter().enumerate() {
        body += &format!("&publishedfileids%5B{i}%5D={id}");
    }
    body
}

/// Current details (title, last update, ...) for items by id.
pub fn details(ids: &[&str]) -> Result<Vec<Item>> {
    Ok(lookup(ids)?.0)
}

/// Items that aren't plainly public, by id: "gone" (Steam doesn't serve it: removed, or made
/// private), "banned" (taken down by Steam), "hidden" (friends only or private), "unlisted".
pub type Status = Vec<(String, &'static str)>;

/// [`details`], and the items' [`Status`].
pub fn lookup(ids: &[&str]) -> Result<(Vec<Item>, Status)> {
    if ids.is_empty() {
        return Ok(Default::default());
    }
    let v = post_form("https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/", &id_list("itemcount", ids))?;
    Ok(parse_details(&v))
}

fn parse_details(v: &Value) -> (Vec<Item>, Status) {
    let (mut items, mut status) = (vec![], vec![]);
    for d in v["response"]["publishedfiledetails"].as_array().into_iter().flatten() {
        let kind = if d["result"] != 1 {
            "gone"
        } else if d["banned"] == true || num(d, "banned") != 0 {
            "banned"
        } else {
            match num(d, "visibility") {
                0 => "",
                3 => "unlisted",
                _ => "hidden",
            }
        };
        if !kind.is_empty() {
            status.push((text(d, "publishedfileid"), kind));
        }
        if d["result"] == 1 {
            items.push(Item::from_json(d));
        }
    }
    (items, status)
}

/// Items whose Workshop update time moved past what `seen` recorded; records the new
/// times. The first sighting of an item only records it (no baseline yet).
pub fn newly_updated(seen: &mut std::collections::HashMap<String, u64>, items: Vec<Item>) -> Vec<Item> {
    items.into_iter().filter(|i| seen.insert(i.id.clone(), i.time_updated).is_some_and(|before| before < i.time_updated)).collect()
}

/// Members of each id that is a collection: `{collection id: [item ids]}`. Plain items are absent.
pub fn collections(ids: &[&str]) -> Result<std::collections::HashMap<String, Vec<String>>> {
    if ids.is_empty() {
        return Ok(Default::default());
    }
    let v = post_form("https://api.steampowered.com/ISteamRemoteStorage/GetCollectionDetails/v1/", &id_list("collectioncount", ids))?;
    Ok(v["response"]["collectiondetails"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["result"] == 1)
        .map(|c| {
            let kids = c["children"].as_array().into_iter().flatten().filter_map(|k| k["publishedfileid"].as_str().map(String::from)).collect();
            (text(c, "publishedfileid"), kids)
        })
        .collect())
}

/// The "Required items" an author lists on the item's page (the keyless API doesn't expose them).
pub fn required_items(id: &str) -> Result<Vec<String>> {
    // pages rarely change within a session; caching keeps big collections from tripping 429s
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, Vec<String>>>> = std::sync::Mutex::new(None);
    if let Some(hit) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|c| c.get(id)) {
        return Ok(hit.clone());
    }
    let html = String::from_utf8_lossy(&cached_get(&format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}"))?).into_owned();
    let req = parse_required(&html);
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(id.to_string(), req.clone());
    Ok(req)
}

/// What an item's Workshop page adds to the API's details.
#[derive(Debug, Default, serde::Serialize, PartialEq)]
pub struct ItemPage {
    /// Full-size screenshot URLs, in order.
    pub screenshots: Vec<String>,
    /// YouTube video ids.
    pub videos: Vec<String>,
    pub required: Vec<String>,
}

/// Screenshots, videos and required items from an item's page.
pub fn item_page(id: &str) -> Result<ItemPage> {
    if !valid_id(id) {
        bail!("not a Workshop id: {id}");
    }
    let html = String::from_utf8_lossy(&cached_get(&format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}"))?).into_owned();
    Ok(parse_item_page(&html))
}

fn parse_item_page(html: &str) -> ItemPage {
    let mut screenshots = vec![];
    if let Some(start) = html.find("var rgFullScreenshotURLs") {
        let block = &html[start..];
        let block = &block[..block.find("];").unwrap_or(block.len())];
        for part in block.split("'url': '").skip(1) {
            if let Some(end) = part.find('\'') {
                screenshots.push(part[..end].replace("&amp;", "&"));
            }
        }
    }
    let mut videos: Vec<String> = vec![];
    // the gallery strip shows each video as its YouTube thumbnail
    for part in html.split("movie_thumb\" src=\"https://img.youtube.com/vi/").skip(1) {
        let v: String = part.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        if v.len() == 11 && !videos.contains(&v) {
            videos.push(v);
        }
    }
    ItemPage { screenshots, videos, required: parse_required(html) }
}

/// Plain text from a scrap of Steam HTML: line breaks kept, tags dropped, entities decoded.
pub(crate) fn html_text(s: &str) -> String {
    let s = s.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ").replace("&amp;", "&").trim().to_string()
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ChangeNote {
    pub time: u64,
    pub text: String,
}

/// One page (from 1) of an item's change notes, newest first.
pub fn change_notes(id: &str, page: u32) -> Result<Vec<ChangeNote>> {
    if !valid_id(id) {
        bail!("not a Workshop id: {id}");
    }
    let url = format!("https://steamcommunity.com/sharedfiles/filedetails/changelog/{id}?p={}", page.max(1));
    Ok(parse_change_notes(&String::from_utf8_lossy(&cached_get(&url)?)))
}

/// The change notes newer than `since` (Unix seconds: the installed version), newest first.
pub fn change_notes_since(id: &str, since: u64) -> Result<Vec<ChangeNote>> {
    Ok(change_notes_back_to(id, since)?.into_iter().filter(|n| n.time > since).collect())
}

/// The change notes newer than `since`, newest first, then the first one at or before it (the
/// version you had), if there is one.
pub fn change_notes_back_to(id: &str, since: u64) -> Result<Vec<ChangeNote>> {
    notes_back_to(|page| change_notes(id, page), since)
}

/// Pages (10 notes each, newest first) until one reaches back to `since`; at most 5.
fn notes_back_to(mut page: impl FnMut(u32) -> Result<Vec<ChangeNote>>, since: u64) -> Result<Vec<ChangeNote>> {
    let mut out = vec![];
    for p in 1..=5 {
        let notes = page(p)?;
        let done = notes.len() < 10 || notes.iter().any(|n| n.time <= since);
        let older = notes.iter().position(|n| n.time <= since).map_or(notes.len(), |i| i + 1);
        out.extend(notes.into_iter().take(older));
        if done {
            break;
        }
    }
    Ok(out)
}

fn parse_change_notes(html: &str) -> Vec<ChangeNote> {
    html.split("class=\"changelog headline\"")
        .skip(1)
        .filter_map(|block| {
            let p = block.find("<p id=\"")? + 7;
            let time: u64 = block[p..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
            let body = &block[block[p..].find('>')? + p + 1..];
            Some(ChangeNote { time, text: html_text(&body[..body.find("</p>").unwrap_or(body.len())]) })
        })
        .collect()
}

#[derive(Debug, Default, serde::Serialize, PartialEq)]
pub struct Comment {
    pub author: String,
    pub time: u64,
    pub text: String,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Comments {
    pub total: u64,
    pub comments: Vec<Comment>,
}

/// Public comments on an item, newest first, 20 from `start`.
pub fn comments(creator: &str, id: &str, start: u32) -> Result<Comments> {
    if !valid_id(id) || creator.is_empty() || !creator.chars().all(|c| c.is_ascii_digit()) {
        bail!("not a Workshop item: {creator}/{id}");
    }
    let url = format!("https://steamcommunity.com/comment/PublishedFile_Public/render/{creator}/{id}/");
    let v = post_form(&url, &format!("start={start}&count=20&feature2=-1"))?;
    Ok(Comments { total: num(&v, "total_count"), comments: parse_comments(v["comments_html"].as_str().unwrap_or_default()) })
}

fn parse_comments(html: &str) -> Vec<Comment> {
    html.split("class=\"commentthread_comment ")
        .skip(1)
        .filter_map(|c| {
            let author = c.split("<bdi>").nth(1)?.split("</bdi>").next()?;
            let time = c.split("data-timestamp=\"").nth(1)?.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0);
            let text = c.split("commentthread_comment_text\"").nth(1)?;
            let text = &text[text.find('>')? + 1..];
            Some(Comment { author: html_text(author), time, text: html_text(&text[..text.find("</div>").unwrap_or(text.len())]) })
        })
        .collect()
}

/// One page (from 1) of an author's items for `app`: the author's total and this page's ids.
pub fn author_items(app: &str, creator: &str, page: u32) -> Result<(u64, Vec<String>)> {
    if creator.is_empty() || !creator.chars().all(|c| c.is_ascii_digit()) {
        bail!("not a Steam id: {creator}");
    }
    let url = format!("https://steamcommunity.com/profiles/{creator}/myworkshopfiles/?appid={app}&section=readytouseitems&p={}", page.max(1));
    Ok(parse_author_items(&String::from_utf8_lossy(&cached_get(&url)?)))
}

fn parse_author_items(html: &str) -> (u64, Vec<String>) {
    let total = html
        .split("workshopBrowsePagingInfo\">")
        .nth(1)
        .and_then(|s| s.split(" entries").next())
        .and_then(|s| s.rsplit(' ').next())
        .and_then(|n| n.replace(',', "").parse().ok())
        .unwrap_or(0);
    let items = html.split("workshopBrowseItems").nth(1).unwrap_or_default();
    let mut ids: Vec<String> = vec![];
    for part in items.split("filedetails/?id=").skip(1) {
        let id: String = part.chars().take_while(char::is_ascii_digit).collect();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    (total, ids)
}

fn parse_required(html: &str) -> Vec<String> {
    let Some(start) = html.find("id=\"RequiredItems\"") else { return vec![] };
    let block = &html[start..];
    let block = &block[..block.find("<!-- created by -->").or_else(|| block.find("rightSectionTopTitle")).unwrap_or_else(|| {
        // no end marker: cap at ~20 KB, on a character boundary (descriptions are often Cyrillic or CJK)
        let mut n = block.len().min(20_000);
        while !block.is_char_boundary(n) {
            n -= 1;
        }
        n
    })];
    let mut ids: Vec<String> = vec![];
    for part in block.split("filedetails/?id=").skip(1) {
        let id: String = part.chars().take_while(char::is_ascii_digit).collect();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// SteamCMD in PZMM's data folder, downloaded the first time it's needed (one download at a time,
/// unpacked aside and moved in, so a failed one leaves no half program and the next try starts over).
pub(crate) fn steamcmd(paths: &Paths) -> Result<PathBuf> {
    static GETTING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = GETTING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = paths.data.join("steamcmd");
    let name = if cfg!(windows) { "steamcmd.exe" } else { "steamcmd.sh" };
    let exe = dir.join(name);
    if !exe.exists() {
        crate::say!("downloadingSteamcmd", "downloading steamcmd...");
        let new = paths.data.join("steamcmd.pzmm-new");
        if new.exists() {
            std::fs::remove_dir_all(&new)?;
        }
        std::fs::create_dir_all(&new)?;
        if cfg!(windows) {
            super::unzip(&super::download(&format!("{STEAMCMD_URL}steamcmd.zip"))?, &new)?;
        } else {
            let gz = super::download(&format!("{STEAMCMD_URL}steamcmd_linux.tar.gz"))?;
            tar::Archive::new(flate2::read::GzDecoder::new(&gz[..])).unpack(&new)?;
        }
        anyhow::ensure!(new.join(name).is_file(), "the SteamCMD download has no {name}");
        std::fs::create_dir_all(&dir)?;
        // the program last: it being there means SteamCMD is complete
        let mut parts: Vec<_> = std::fs::read_dir(&new)?.flatten().map(|e| e.file_name()).collect();
        parts.sort_by_key(|n| *n == *name);
        for n in parts {
            let to = dir.join(&n);
            if to.is_dir() {
                std::fs::remove_dir_all(&to)?;
            }
            std::fs::rename(new.join(&n), to)?;
        }
        let _ = std::fs::remove_dir(&new);
    }
    Ok(exe)
}

/// Download items anonymously. Returns each id with its content folder, or None if
/// SteamCMD didn't produce one (its exit code is unreliable, so we check the folder).
/// Workshop ids are numbers. Anything else (from a server .ini, a pasted link) must never
/// reach SteamCMD's command line or a path.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

/// Where SteamCMD keeps item `id`: its finished folder, and its partial download.
pub fn steamcmd_dirs(paths: &Paths, app: &str, id: &str) -> [PathBuf; 2] {
    let w = paths.data.join("steamcmd/steamapps/workshop");
    [w.join("content").join(app).join(id), w.join("downloads").join(app).join(id)]
}

/// One SteamCMD at a time: two sessions fight over its folder.
pub(crate) static STEAMCMD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// How long SteamCMD may go without finishing an item before it's stopped.
const ITEM_LIMIT: Duration = Duration::from_secs(60 * 60);

/// Setting `stop` ends SteamCMD early; items it finished by then still count.
pub fn download(paths: &Paths, app: &str, ids: &[&str], stop: &AtomicBool) -> Result<Vec<(String, Option<PathBuf>)>> {
    if let Some(bad) = ids.iter().find(|i| !valid_id(i)) {
        bail!("{bad:?} isn't a Workshop item id");
    }
    let exe = steamcmd(paths)?;
    let mut cmd = Command::new(&exe);
    cmd.arg("+login").arg("anonymous");
    for id in ids {
        cmd.args(["+workshop_download_item", app, id, "validate"]);
    }
    cmd.arg("+quit").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _one = STEAMCMD.lock().unwrap_or_else(|e| e.into_inner());
    crate::say!("downloadingItems", "downloading {count} workshop item(s)...", count = ids.len());
    crate::progress(0, ids.len());
    let mut child = cmd.spawn().with_context(|| format!("running {}", exe.display()))?;
    let out = child.stdout.take().context("SteamCMD's output")?;
    let child = std::cell::RefCell::new(child);
    let (done, stopped) = finished_items(
        out,
        ITEM_LIMIT,
        stop,
        |n| crate::progress(n, ids.len()),
        || matches!(child.borrow_mut().try_wait(), Ok(Some(_))),
        || {
            let _ = child.borrow_mut().kill();
        },
    );
    let _ = child.borrow_mut().wait();
    let got: Vec<(String, Option<PathBuf>)> =
        ids.iter().map(|id| (id.to_string(), Some(steamcmd_dirs(paths, app, id)[0].clone()).filter(|d| done.contains(*id) && d.is_dir()))).collect();
    if stopped {
        let late = got.iter().filter(|(_, d)| d.is_none()).map(|(id, _)| id.as_str()).collect::<Vec<_>>().join(", ");
        if stop.load(Relaxed) {
            crate::say!("steamcmdStopped", "error: SteamCMD was stopped (you stopped it); not downloaded: {ids}", ids = late);
        } else {
            crate::say!("steamcmdTimedOut", "error: SteamCMD was stopped (no download finished within an hour); not downloaded: {ids}", ids = late);
        }
    }
    Ok(got)
}

/// The ids SteamCMD's output `out` reports downloaded, read until it ends; `done` hears how many
/// so far. `kill` runs when no item finishes within `limit` (a stuck download) or `stop` is
/// set; the bool says it ran. It also ends once `exited` says SteamCMD has: a process SteamCMD
/// left behind can hold its output open, and waiting for that to end held up every install after.
fn finished_items(out: impl Read + Send + 'static, limit: Duration, stop: &AtomicBool, mut done_so_far: impl FnMut(usize), mut exited: impl FnMut() -> bool, kill: impl FnOnce()) -> (HashSet<String>, bool) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let l = line.to_lowercase();
            if l.contains("workshop") || l.contains("error") {
                crate::log!("{}", line.trim());
            }
            // "Success. Downloaded item 123 to ..."
            if let Some(rest) = line.split("Downloaded item ").nth(1) {
                let _ = tx.send(rest.chars().take_while(char::is_ascii_digit).collect::<String>());
            }
        }
    });
    let (mut done, mut since) = (HashSet::new(), Instant::now());
    loop {
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(id) => {
                done.insert(id);
                done_so_far(done.len());
                since = Instant::now();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if exited() => {
                // what it printed before exiting is still in the pipe
                done.extend(std::iter::from_fn(|| rx.recv_timeout(Duration::from_secs(1)).ok()));
                done_so_far(done.len());
                return (done, false);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if since.elapsed() < limit && !stop.load(Relaxed) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                kill();
                done.extend(rx); // the output ends with the process: items finished just before count
                return (done, true);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return (done, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_browse_ssr_data() {
        // results sit inside a JSON string inside the SSR JSON, like Steam's page
        let inner = serde_json::json!({"queries": [{"state": {"data": {"total_count": 429, "results": [
            {"publishedfileid": "3810071828", "title": "Immersive Snow", "preview_url": "https://x/p.png",
             "short_description": "Footprints", "subscriptions": 8123, "time_updated": 1790000000,
             "file_size": "1048576", "tags": [{"tag": "Build 42"}], "num_children": 1,
             "star_rating": 4, "total_votes": 767}
        ]}}}]});
        let html = format!(
            "<html><script type=\"application/json\" id=\"valve-ssr-data\" nonce=\"n\">{}</script></html>",
            serde_json::json!({"renderContext": {"queryData": inner.to_string()}})
        );
        let page = parse_browse(&html).unwrap();
        assert_eq!(page.total, 429);
        assert_eq!(page.items[0].id, "3810071828");
        assert_eq!((page.items[0].file_size, page.items[0].tags.clone()), (1_048_576, vec!["Build 42".to_string()]));
        assert_eq!((page.items[0].stars, page.items[0].votes), (4, 767));
        assert!(parse_browse("<html>maintenance</html>").is_err());
    }

    #[test]
    fn tells_which_items_are_not_public() {
        // as GetPublishedFileDetails answers: removed or private items come back as result 9
        let v = serde_json::json!({"response": {"publishedfiledetails": [
            {"publishedfileid": "1", "result": 1, "visibility": 0, "banned": 0, "title": "Fine"},
            {"publishedfileid": "2", "result": 9},
            {"publishedfileid": "3", "result": 1, "visibility": 3, "banned": 0},
            {"publishedfileid": "4", "result": 1, "visibility": 1, "banned": 0},
            {"publishedfileid": "5", "result": 1, "visibility": 0, "banned": 1},
        ]}});
        let (items, status) = parse_details(&v);
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["1", "3", "4", "5"]);
        assert_eq!(status, [("2".to_string(), "gone"), ("3".into(), "unlisted"), ("4".into(), "hidden"), ("5".into(), "banned")]);
    }

    #[test]
    fn reports_updates_after_the_first_look() {
        let item = |t| Item { id: "1".into(), time_updated: t, ..Default::default() };
        let mut seen = Default::default();
        assert!(newly_updated(&mut seen, vec![item(100)]).is_empty(), "first look is the baseline");
        assert!(newly_updated(&mut seen, vec![item(100)]).is_empty());
        assert_eq!(newly_updated(&mut seen, vec![item(200)]).len(), 1);
        assert!(newly_updated(&mut seen, vec![item(200)]).is_empty());
    }

    #[test]
    fn workshop_ids_are_digits_only() {
        assert!(valid_id("3619862853"));
        for bad in ["", "+runscript", "../..", "12 3", "1;2", "123456789012345678901"] {
            assert!(!valid_id(bad), "{bad:?}");
        }
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: t.path().into(), data: t.path().into() };
        assert!(download(&paths, "108600", &["+runscript"], &AtomicBool::new(false)).is_err(), "rejected before SteamCMD runs");
    }

    #[test]
    fn a_stuck_batch_keeps_what_finished() {
        let never = AtomicBool::new(false);
        let out = "Downloading item 1 ...
Success. Downloaded item 1 to \"x\" (5 bytes)
Success. Downloaded item 22 to \"y\"
";
        let (done, stopped) = finished_items(std::io::Cursor::new(out), Duration::from_secs(60), &never, |_| {}, || false, || panic!("not stuck"));
        assert_eq!((done, stopped), (["1".to_string(), "22".to_string()].into(), false));

        // item 1 finishes, then nothing: stopped after `limit`, item 1 kept
        let (r, mut w) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut w, b"Success. Downloaded item 1 to \"x\"
Downloading item 2 ...
").unwrap();
        let (done, stopped) = finished_items(r, Duration::from_millis(500), &never, |_| {}, || false, move || drop(w));
        assert_eq!((done, stopped), (["1".to_string()].into(), true));

        let (r, w) = std::io::pipe().unwrap();
        let (done, stopped) = finished_items(r, Duration::from_secs(60), &AtomicBool::new(true), |_| {}, || false, move || drop(w));
        assert!(done.is_empty() && stopped, "Stop ends it at once");

        // SteamCMD exits but something it left keeps its output open: ends anyway, item 3 kept
        let (r, mut w) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut w, b"Success. Downloaded item 3 to \"z\"
Unloading Steam API...OK
").unwrap();
        let (done, stopped) = finished_items(r, Duration::from_secs(60), &never, |_| {}, || true, || panic!("not stuck"));
        assert_eq!((done, stopped), (["3".to_string()].into(), false));
        drop(w);
    }

    #[test]
    fn parses_required_items() {
        let html = r#"<div class="requiredItemsContainer" id="RequiredItems">
            <a href="https://steamcommunity.com/workshop/filedetails/?id=3619862853" target="_blank"><div class="requiredItem">ZombieBuddy</div></a>
            <a href="https://steamcommunity.com/workshop/filedetails/?id=2169435993"><div class="requiredItem">Mod Options</div></a>
            </div></div><!-- created by --><a href="https://steamcommunity.com/sharedfiles/filedetails/?id=999">other</a>"#;
        assert_eq!(parse_required(html), ["3619862853", "2169435993"]);
        assert!(parse_required("<html>no requirements</html>").is_empty());
    }

    #[test]
    fn browse_urls() {
        let q = Query { sort: "trend".into(), days: 30, required: vec!["Build 42".into()], excluded: vec!["Map".into()], per_page: 50, ..Default::default() };
        let u = browse_url("108600", &q);
        assert!(u.contains("section=readytouseitems&browsesort=trend&actualsort=trend&p=1&days=30"));
        assert!(u.contains("&num_per_page=50&requiredtags%5B%5D=Build%2042&excludedtags%5B%5D=Map"));
        let c = browse_url("108600", &Query { collections: true, sort: "totaluniquesubscribers".into(), child: "123".into(), ..Default::default() });
        assert!(c.contains("section=collections&browsesort=trend") && c.contains("&days=7&childpublishedfileid=123"));
        let s = browse_url("108600", &Query { search: "more traits".into(), sort: "textsearch".into(), scope: 1, created_from: 5, libraries: true, ..Default::default() });
        assert!(s.contains("browsesort=textsearch") && s.contains("searchtext=more%20traits&search_text_target=1"));
        assert!(s.contains("&special_filter=6") && s.contains("&created_date_range_filter_start=5"));
        // a search keeps the chosen sort; relevance without a search falls back to trend
        let t = browse_url("108600", &Query { search: "x".into(), sort: "trend".into(), days: 30, ..Default::default() });
        assert!(t.contains("browsesort=trend") && t.contains("&days=30&searchtext=x"));
        assert!(browse_url("108600", &Query { sort: "textsearch".into(), ..Default::default() }).contains("browsesort=trend"));
    }

    #[test]
    fn item_pages() {
        let html = r#"<script>var rgFullScreenshotURLs = [
            { 'previewid' : '1', 'url': 'https://images.steamusercontent.com/ugc/1/A/?imw=5000&amp;ima=fit' },
            { 'previewid' : '2', 'url': 'https://images.steamusercontent.com/ugc/2/B/' },
            ];</script><div class="highlight_strip_item highlight_strip_movie"><img class="movie_thumb" src="https://img.youtube.com/vi/dQw4w9WgXcQ/default.jpg"></div>"#;
        let page = parse_item_page(html);
        assert_eq!(page.screenshots, ["https://images.steamusercontent.com/ugc/1/A/?imw=5000&ima=fit", "https://images.steamusercontent.com/ugc/2/B/"]);
        assert_eq!(page.videos, ["dQw4w9WgXcQ"]);
    }

    #[test]
    fn change_notes_and_comments() {
        let notes = r#"<div class="changelog headline"> Update: 17 Oct </div><div></div><p id="1666019194">v1.4.4<br>- Hotfix &amp; tidy</p>
            <div class="changelog headline"> Update: 12 Oct </div><p id="1665625756">v1.4.3</p>"#;
        assert_eq!(
            parse_change_notes(notes),
            [ChangeNote { time: 1666019194, text: "v1.4.4\n- Hotfix & tidy".into() }, ChangeNote { time: 1665625756, text: "v1.4.3".into() }]
        );
        let html = r#"<div class="commentthread_comment responsive_body_text " id="comment_1"><a class="commentthread_author_link"><bdi>Spiffo</bdi></a>
            <div class="commentthread_comment_timestamp" data-timestamp="1791647977"> now</div>
            <div class="commentthread_comment_text" id="comment_content_1"> Works on 42.21<br>thanks! </div></div>"#;
        assert_eq!(parse_comments(html), [Comment { author: "Spiffo".into(), time: 1791647977, text: "Works on 42.21\nthanks!".into() }]);
    }

    #[test]
    fn change_notes_since_the_installed_version() {
        // pages of 10, newest first: times 100, 99, ... 76
        let note = |time| ChangeNote { time, text: format!("v{time}") };
        let asked = std::cell::RefCell::new(vec![]);
        let pages = |p: u32| {
            asked.borrow_mut().push(p);
            Ok((0..10).map(|i| 100 - (p as u64 - 1) * 10 - i).filter(|t| *t > 75).map(note).collect())
        };
        let got = notes_back_to(pages, 88).unwrap();
        assert_eq!(got.iter().map(|n| n.time).collect::<Vec<_>>(), (88..=100).rev().collect::<Vec<_>>(), "newer than the install, then the install");
        assert_eq!(asked.take(), [1, 2], "stops at the page that reaches the install");
        assert_eq!(notes_back_to(pages, 0).unwrap().len(), 25, "a short page is the last");
        assert_eq!(asked.take(), [1, 2, 3]);
        assert_eq!(notes_back_to(|_| Ok(vec![note(5), note(4)]), 5).unwrap(), [note(5)], "the installed version itself, nothing older");
    }

    #[test]
    fn author_pages() {
        let html = r#"<div class="workshopBrowsePagingInfo">Showing 1-9 of 1,204 entries</div><a href="x/filedetails/?id=5">nav</a>
            <div class="workshopBrowseItems"><a href="https://steamcommunity.com/sharedfiles/filedetails/?id=11"><img></a>
            <a href="https://steamcommunity.com/sharedfiles/filedetails/?id=11">t</a><a href="https://steamcommunity.com/sharedfiles/filedetails/?id=22">u</a></div>"#;
        assert_eq!(parse_author_items(html), (1204, vec!["11".to_string(), "22".to_string()]));
    }
}

