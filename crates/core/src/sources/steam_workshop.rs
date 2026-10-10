//! Steam Workshop: anonymous downloads through SteamCMD, and the folder where the
//! Steam client keeps the user's own subscriptions.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::pz::Paths;

const STEAMCMD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/";

/// `steamapps/workshop/content/<app>` in whichever Steam library has it.
pub fn content_dir(app: &str) -> Option<PathBuf> {
    library_path(&format!("steamapps/workshop/content/{app}"))
}

/// `<library>/<rel>` for the first Steam library that has it, e.g. `steamapps/common/ProjectZomboid`.
pub fn library_path(rel: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
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
    for root in roots {
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
        }
    }
}

/// Browse sort orders, as Steam names them.
pub const SORTS: &[&str] = &["trend", "toprated", "totaluniquesubscribers", "mostrecent", "lastupdated"];

/// Search or browse the Workshop for `app`. `sort` is one of [`SORTS`] (ignored when
/// searching); `page` starts at 1; `tag` filters, e.g. "Build 42".
pub fn browse(app: &str, search: &str, sort: &str, page: u32, tag: Option<&str>) -> Result<Page> {
    let enc = |s: &str| -> String {
        s.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect()
    };
    let sort = if !search.trim().is_empty() { "textsearch" } else if SORTS.contains(&sort) { sort } else { "trend" };
    let mut url = format!(
        "https://steamcommunity.com/workshop/browse/?appid={app}&section=readytouseitems&browsesort={sort}&actualsort={sort}&p={}",
        page.max(1)
    );
    if sort == "trend" {
        url += "&days=7";
    }
    if sort == "textsearch" {
        url += &format!("&searchtext={}", enc(search.trim()));
    }
    if let Some(t) = tag {
        url += &format!("&requiredtags%5B%5D={}", enc(t));
    }
    parse_browse(&String::from_utf8_lossy(&super::http_get(&url)?))
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
                    *page = Some(Page { total: num(v, "total_count"), items: results.iter().map(Item::from_json).collect() });
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

fn post_form(url: &str, body: &str) -> Result<Value> {
    let bytes = super::polite(url, || {
        ureq::post(url)
            .header("User-Agent", "pzmm")
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
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let v = post_form("https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/", &id_list("itemcount", ids))?;
    Ok(v["response"]["publishedfiledetails"].as_array().into_iter().flatten().filter(|d| d["result"] == 1).map(Item::from_json).collect())
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
    let html = String::from_utf8_lossy(&super::http_get(&format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}"))?).into_owned();
    let req = parse_required(&html);
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(id.to_string(), req.clone());
    Ok(req)
}

fn parse_required(html: &str) -> Vec<String> {
    let Some(start) = html.find("id=\"RequiredItems\"") else { return vec![] };
    let block = &html[start..];
    let block = &block[..block.find("<!-- created by -->").or_else(|| block.find("rightSectionTopTitle")).unwrap_or(block.len().min(20_000))];
    let mut ids: Vec<String> = vec![];
    for part in block.split("filedetails/?id=").skip(1) {
        let id: String = part.chars().take_while(char::is_ascii_digit).collect();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn steamcmd(paths: &Paths) -> Result<PathBuf> {
    let dir = paths.data.join("steamcmd");
    let exe = dir.join(if cfg!(windows) { "steamcmd.exe" } else { "steamcmd.sh" });
    if !exe.exists() {
        crate::log!("downloading steamcmd...");
        std::fs::create_dir_all(&dir)?;
        if cfg!(windows) {
            super::unzip(&super::http_get(&format!("{STEAMCMD_URL}steamcmd.zip"))?, &dir)?;
        } else {
            let gz = super::http_get(&format!("{STEAMCMD_URL}steamcmd_linux.tar.gz"))?;
            tar::Archive::new(flate2::read::GzDecoder::new(&gz[..])).unpack(&dir)?;
        }
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

pub fn download(paths: &Paths, app: &str, ids: &[&str]) -> Result<Vec<(String, Option<PathBuf>)>> {
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
    // one SteamCMD at a time: two sessions fight over its folder
    static STEAMCMD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = STEAMCMD.lock().unwrap_or_else(|e| e.into_inner());
    crate::log!("downloading {} workshop item(s)...", ids.len());
    let mut child = cmd.spawn().with_context(|| format!("running {}", exe.display()))?;
    let reader = child.stdout.take().map(|out| {
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let l = line.to_lowercase();
                if l.contains("workshop") || l.contains("error") {
                    crate::log!("{}", line.trim());
                }
            }
        })
    });
    // ponytail: one overall limit; a per-item limit if huge downloads ever need longer
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60 * 60);
    while child.try_wait()?.is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            bail!("SteamCMD didn't finish within an hour and was stopped");
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    if let Some(r) = reader {
        let _ = r.join();
    }
    let content: &Path = &exe.parent().unwrap().join("steamapps/workshop/content").join(app);
    Ok(ids.iter().map(|id| (id.to_string(), Some(content.join(id)).filter(|d| d.is_dir()))).collect())
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
        assert!(download(&paths, "108600", &["+runscript"]).is_err(), "rejected before SteamCMD runs");
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
}
