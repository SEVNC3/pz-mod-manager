//! Where mods come from: Steam Workshop, GitHub, or a local zip/folder.

pub mod github;
pub mod steam_manifest;
pub mod steam_workshop;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::pz::{self, Paths};
use crate::store;

#[derive(Debug, PartialEq)]
pub enum Source {
    Workshop(String),
    GitHub(String),
    Local(PathBuf),
}

impl Source {
    /// Workshop id or URL, an existing local path, else GitHub `owner/repo[@ref]` or URL.
    pub fn parse(s: &str) -> Source {
        let s = s.trim();
        if steam_workshop::valid_id(s) {
            return Source::Workshop(s.into());
        }
        if s.contains("steamcommunity.com/") {
            let id = s.split(['?', '&']).find_map(|kv| kv.strip_prefix("id=")).unwrap_or("");
            return Source::Workshop(id.chars().take_while(char::is_ascii_digit).collect());
        }
        if Path::new(s).exists() {
            return Source::Local(PathBuf::from(s));
        }
        let s = s.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("github.com/");
        Source::GitHub(s.trim_end_matches('/').trim_end_matches(".git").into())
    }

    /// Inverse of the `source` string stored in the index.
    pub fn from_index(s: &str) -> Option<Source> {
        let (kind, v) = s.split_once(':')?;
        match kind {
            "workshop" if steam_workshop::valid_id(v) => Some(Source::Workshop(v.into())),
            "github" => Some(Source::GitHub(v.into())),
            "local" => Some(Source::Local(v.into())),
            _ => None,
        }
    }
}

/// Hosts that still answered HTTP 429 after the backoffs, and when: for a minute their requests
/// fail at once, so a batch (a collection's pages) doesn't wait out the backoffs item by item.
static REFUSED: std::sync::Mutex<Vec<(String, std::time::Instant)>> = std::sync::Mutex::new(vec![]);

fn host(url: &str) -> &str {
    url.split('/').nth(2).unwrap_or(url)
}

/// Run an HTTP request with Steam etiquette: at most one Steam request every 350 ms
/// (shared across threads), and on HTTP 429 back off (2 s, then 5 s) before giving up.
pub(crate) fn polite<T>(url: &str, mut send: impl FnMut() -> Result<T, ureq::Error>) -> Result<T> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static LAST_STEAM: Mutex<Option<Instant>> = Mutex::new(None);
    const LIMITED: &str = "Steam is limiting requests right now (HTTP 429). Wait a minute and try again.";
    let steam = url.contains("steamcommunity.com") || url.contains("steampowered.com");
    {
        let mut refused = REFUSED.lock().unwrap_or_else(|e| e.into_inner());
        refused.retain(|(_, at)| at.elapsed() < Duration::from_secs(60));
        if refused.iter().any(|(h, _)| h == host(url)) {
            bail!("{LIMITED}");
        }
    }
    for wait in [2, 5, 0] {
        if steam {
            let mut last = LAST_STEAM.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(gap) = last.and_then(|t| Duration::from_millis(350).checked_sub(t.elapsed())) {
                std::thread::sleep(gap);
            }
            *last = Some(Instant::now());
        }
        match send() {
            Ok(v) => return Ok(v),
            Err(ureq::Error::StatusCode(429)) if wait > 0 => std::thread::sleep(Duration::from_secs(wait)),
            Err(ureq::Error::StatusCode(429)) => {
                REFUSED.lock().unwrap_or_else(|e| e.into_inner()).push((host(url).into(), Instant::now()));
                bail!("{LIMITED}")
            }
            Err(e) => return Err(e).with_context(|| format!("requesting {url}")),
        }
    }
    unreachable!()
}

/// Longest wait for one web call (connecting: `CONNECT`); downloads get `DOWNLOAD`.
const CALL: std::time::Duration = std::time::Duration::from_secs(30);
const CONNECT: std::time::Duration = std::time::Duration::from_secs(10);
const DOWNLOAD: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The one HTTP agent every web call goes through, so a silent server never hangs PZMM
/// (ureq waits forever by default).
pub(crate) fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| ureq::Agent::config_builder().timeout_connect(Some(CONNECT)).timeout_global(Some(CALL)).build().into())
}

/// A page or API response (up to `CALL`).
pub(crate) fn http_get(url: &str) -> Result<Vec<u8>> {
    get(url, CALL)
}

/// A file download (zip, SteamCMD): up to `DOWNLOAD`.
pub(crate) fn download(url: &str) -> Result<Vec<u8>> {
    get(url, DOWNLOAD)
}

fn get(url: &str, limit: std::time::Duration) -> Result<Vec<u8>> {
    polite(url, || {
        let mut req = agent().get(url).config().timeout_global(Some(limit)).build().header("User-Agent", crate::USER_AGENT);
        if url.starts_with("https://api.github.com/")
            && let Some(token) = crate::settings::github_token()
        {
            req = req.header("Authorization", &format!("Bearer {token}"));
        }
        req.call()?.body_mut().with_config().limit(2 << 30).read_to_vec()
    })
}

pub(crate) fn unzip(bytes: &[u8], to: &Path) -> Result<()> {
    // zip's extract() refuses entries that escape `to` (../, absolute paths)
    zip::ZipArchive::new(std::io::Cursor::new(bytes))?.extract(to)?;
    Ok(())
}

/// A downloaded source ready to install. Temp folders are deleted when it's dropped.
pub struct Fetched {
    pub source: String,
    root: PathBuf,
    _temp: Option<tempfile::TempDir>,
    /// Already subscribed in Steam: use Steam's copy (Steam keeps it updated) instead of
    /// copying it into Zomboid/mods, where a copy would shadow newer Steam updates.
    subscribed: bool,
}

/// Set to cancel the SteamCMD download of the `fetch` in progress: items it finished still install.
pub static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Download (network and SteamCMD only; nothing in Zomboid/mods changes). Workshop items
/// go through one SteamCMD session; with `resolve`, collections are expanded and required
/// items added. Errors are logged per source so one bad entry doesn't stop the rest.
pub fn fetch(paths: &Paths, sources: &[Source], resolve: bool) -> Vec<Fetched> {
    STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    let mut out = vec![];
    let mut workshop: Vec<String> = sources.iter().filter_map(|s| if let Source::Workshop(i) = s { Some(i.clone()) } else { None }).collect();
    let mut unchecked = vec![];
    if resolve && !workshop.is_empty() {
        (workshop, unchecked) = expand_workshop(paths, workshop);
    }
    let mut tried: HashSet<String> = workshop.iter().cloned().collect();
    loop {
        fetch_workshop(paths, std::mem::take(&mut workshop), &mut out);
        if unchecked.is_empty() {
            break;
        }
        // Steam wouldn't show these items' pages: what their mod.info files require comes from
        // the mod ID index instead, and so on for what that adds
        let mut provided: HashSet<String> = store::scan(paths).unwrap_or_default().into_keys().collect();
        let mut needs = vec![];
        for f in &out {
            let page_unread = workshop_item(&f.source).is_some_and(|i| unchecked.iter().any(|u| u == i));
            for m in pz::find_mods(&f.root, 8) {
                if page_unread {
                    needs.extend(m.requires.iter().map(|r| (m.id.clone(), r.clone())));
                }
                provided.insert(m.id);
            }
        }
        let (items, unresolved) = from_index(&needs, &provided, &pz::mod_index::kept_by_id(paths), &tried);
        for (id, req, item) in &items {
            crate::say!("requiresFromInfo", "{id} requires {req}: adding workshop:{item} (from its mod.info, as Steam didn't show the item's page)", id = id, req = req, item = item);
        }
        for (id, req) in unresolved {
            crate::say!("requiresUnknown", "error: {id} requires {req}, and no Workshop item PZMM knows of provides it: find and install it yourself", id = id, req = req);
        }
        workshop = items.into_iter().map(|(.., item)| item).filter(|i| tried.insert(i.clone())).collect();
        if workshop.is_empty() {
            break;
        }
        unchecked = workshop.clone();
    }
    let mut report = |r: Result<Fetched>| match r {
        Ok(f) => out.push(f),
        Err(e) => crate::log!("error: {e:#}"),
    };
    for s in sources {
        match s {
            Source::Workshop(_) => {}
            Source::GitHub(spec) => report(github::download(spec).map(|t| Fetched {
                source: format!("github:{spec}"),
                root: t.path().to_path_buf(),
                _temp: Some(t),
                subscribed: false,
            })),
            Source::Local(p) => report(fetch_local(p)),
        }
    }
    out
}

/// Workshop `items` through Steam's own folder (subscriptions) or one SteamCMD session.
fn fetch_workshop(paths: &Paths, mut items: Vec<String>, out: &mut Vec<Fetched>) {
    let mut report = |r: Result<Fetched>| match r {
        Ok(f) => out.push(f),
        Err(e) => crate::log!("error: {e:#}"),
    };
    if let Some(ws) = pz::workshop_dir() {
        items.retain(|id| {
            let root = ws.join(id);
            if !steam_workshop::valid_id(id) || !root.is_dir() {
                return true;
            }
            report(Ok(Fetched { source: format!("steam:{id}"), root, _temp: None, subscribed: true }));
            false
        });
    }
    items.retain(|id| {
        let ok = steam_workshop::valid_id(id);
        if !ok {
            report(Err(anyhow::anyhow!("workshop:{id}: not a Workshop item id (a link needs ?id=…)")));
        }
        ok
    });
    if items.is_empty() {
        return;
    }
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    match steam_workshop::download(paths, pz::STEAM_APP, &refs, &STOP) {
        Ok(got) => {
            for (id, dir) in got {
                report(
                    dir.with_context(|| format!("workshop:{id}: download failed"))
                        .map(|root| Fetched { source: format!("workshop:{id}"), root, _temp: None, subscribed: false }),
                );
            }
        }
        Err(e) => crate::log!("error: {e:#}"),
    }
}

/// For each (mod, required mod id) in `needs` that nothing `provided` has: the most subscribed
/// Workshop item holding it in the mod ID index `by_id` (one per required id, not one of
/// `tried`), as (mod, required id, item); and those no known item provides.
type Found = (Vec<(String, String, String)>, Vec<(String, String)>);
fn from_index(needs: &[(String, String)], provided: &HashSet<String>, by_id: &std::collections::HashMap<String, Vec<String>>, tried: &HashSet<String>) -> Found {
    let (mut found, mut unknown) = (vec![], vec![]);
    let mut seen = HashSet::new();
    for (id, req) in needs {
        if provided.contains(req) || !seen.insert(req) {
            continue;
        }
        match by_id.get(req).and_then(|items| items.first()) {
            Some(item) if !tried.contains(item) => found.push((id.clone(), req.clone(), item.clone())),
            Some(_) => {} // that item came already and doesn't hold it (or failed, which was said)
            None => unknown.push((id.clone(), req.clone())),
        }
    }
    (found, unknown)
}

/// Copy fetched mods into Zomboid/mods. Returns the installed mod ids.
pub fn install_fetched(paths: &Paths, fetched: &[Fetched]) -> Vec<String> {
    let mut ids = vec![];
    for f in fetched {
        if f.subscribed {
            let found: Vec<String> = pz::find_mods(&f.root, 8).into_iter().map(|m| m.id).collect();
            crate::say!("subscribedCopy", "{source}: you're subscribed in Steam, which keeps it updated; using that copy ({ids})", source = f.source, ids = found.join(", "));
            // a copy PZMM installed before the subscription would shadow Steam's newer one (core 10)
            let item = format!("workshop:{}", f.source.trim_start_matches("steam:"));
            let ours: Vec<String> = store::load(paths).unwrap_or_default().into_iter().filter(|(_, e)| e.source == item).map(|(id, _)| id).collect();
            if !ours.is_empty() && let Err(e) = store::remove(paths, &ours, crate::settings::load().recycle) {
                crate::say!("oldCopyNotRemoved", "error: removing PZMM's older copy of {ids}: {error}", ids = ours.join(", "), error = format!("{e:#}"));
            }
            ids.extend(found);
            continue;
        }
        match store::install(paths, &f.root, &f.source) {
            Ok(v) => ids.extend(v),
            Err(e) => crate::log!("error: {e:#}"),
        }
    }
    let cap = crate::settings::load().backup_cap_mb << 20;
    if cap > 0 && let Err(e) = store::prune_backups(paths, cap) {
        crate::say!("pruneFailed", "error: pruning backups: {error}", error = format!("{e:#}"));
    }
    ids
}

/// Download and install in one go (the CLI; the app splits the two to keep its lock short).
pub fn install(paths: &Paths, sources: &[Source], resolve: bool) -> Vec<String> {
    install_fetched(paths, &fetch(paths, sources, resolve))
}

/// The Workshop item a mod comes from: "steam:<id>" (a subscription) or "workshop:<id>" (PZMM's install).
pub fn workshop_item(source: &str) -> Option<&str> {
    source.strip_prefix("steam:").or_else(|| source.strip_prefix("workshop:")).filter(|i| !i.is_empty())
}

/// Workshop ids the game can already load: Steam subscriptions and PZMM installs.
pub fn workshop_available(paths: &Paths) -> HashSet<String> {
    let mut have: HashSet<String> = pz::workshop_dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    have.extend(store::load(paths).unwrap_or_default().into_values().filter_map(|e| e.source.strip_prefix("workshop:").map(String::from)));
    have
}

/// Replace collections with their items, then add required items (recursively) that the
/// game can't already load. Also returns the items whose pages Steam didn't show (said once in
/// the log), whose requirements weren't checked.
pub fn expand_workshop(paths: &Paths, ids: Vec<String>) -> (Vec<String>, Vec<String>) {
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let collections = steam_workshop::collections(&refs).unwrap_or_else(|e| {
        crate::say!("collectionsFailed", "error: couldn't check for Workshop collections: {error}", error = format!("{e:#}"));
        Default::default()
    });
    let mut queue = std::collections::VecDeque::new();
    for id in ids {
        match collections.get(&id) {
            Some(items) => {
                crate::say!("collection", "collection {id}: {count} item(s)", id = id, count = items.len());
                queue.extend(items.iter().cloned());
            }
            None => queue.push_back(id),
        }
    }
    let have = workshop_available(paths);
    let mut seen = HashSet::new();
    let (mut out, mut unchecked, mut error) = (vec![], vec![], None);
    while let Some(id) = queue.pop_front() {
        // a big collection reads hundreds of pages: Stop ends it with nothing to download
        if STOP.load(std::sync::atomic::Ordering::SeqCst) {
            return (vec![], vec![]);
        }
        if !seen.insert(id.clone()) || out.len() >= 500 {
            continue;
        }
        crate::progress(seen.len() - 1, seen.len() + queue.len());
        out.push(id.clone());
        match steam_workshop::required_items(&id) {
            Ok(req) => {
                for r in req.into_iter().filter(|r| !have.contains(r) && !seen.contains(r)) {
                    crate::say!("requiresAdding", "workshop:{id} requires workshop:{req}, adding it", id = id, req = r);
                    queue.push_back(r);
                }
            }
            Err(e) => {
                error.get_or_insert(format!("{e:#}"));
                unchecked.push(id);
            }
        }
    }
    if let Some(error) = error {
        crate::say!("requiresUnchecked", "error: couldn't check the Workshop pages of {count} item(s) for what they require ({error}): {ids}", count = unchecked.len(), error = error, ids = unchecked.join(", "));
    }
    (out, unchecked)
}

/// When PZMM installed mod `id` (Unix seconds); v0.1 imports have no time, so their folder's age.
pub fn installed_at(paths: &Paths, id: &str, e: &store::Entry) -> u64 {
    if e.installed_at > 0 {
        return e.installed_at;
    }
    std::fs::metadata(paths.mods().join(id))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// Mods PZMM installed from the Workshop that have been updated there since.
/// (Steam keeps subscriptions current itself; GitHub sources aren't checked yet.)
pub fn check_updates(paths: &Paths) -> Result<Vec<String>> {
    let index = store::load(paths)?;
    let ours: Vec<(&String, &str, u64)> = index
        .iter()
        .filter_map(|(id, e)| Some((id, e.source.strip_prefix("workshop:")?, installed_at(paths, id, e))))
        .collect();
    let items: Vec<&str> = ours.iter().map(|o| o.1).collect::<HashSet<_>>().into_iter().collect();
    let updated: std::collections::HashMap<String, u64> =
        steam_workshop::details(&items)?.into_iter().map(|i| (i.id, i.time_updated)).collect();
    Ok(ours.into_iter().filter(|(_, item, at)| updated.get(*item).is_some_and(|t| t > at)).map(|(id, ..)| id.clone()).collect())
}

/// A Workshop item that may provide a missing mod.
#[derive(Debug, serde::Serialize)]
pub struct Candidate {
    #[serde(flatten)]
    pub item: steam_workshop::Item,
    /// Its description declares `Mod ID: <the id>`.
    pub confirmed: bool,
    /// The dependent mod's author lists it under "Required items".
    pub required_by_author: bool,
    /// Tagged for the build being played (or carries no build tag).
    pub fits_build: bool,
}

impl Candidate {
    /// Safe to install without asking: it declares the id and suits the build.
    pub fn sure(&self) -> bool {
        self.confirmed && self.fits_build
    }
}

/// Find Workshop items that provide `mod_id` for `build` (41/42): the "Required items" of
/// the Workshop item `needed_by` (if any) plus a Workshop search for the id. Best first.
pub fn find_mod(mod_id: &str, needed_by: Option<&str>, build: u8) -> Result<Vec<Candidate>> {
    // the author's list is a bonus: if Steam won't show the page, the search still works
    let required = needed_by
        .map(|item| {
            steam_workshop::required_items(item).unwrap_or_else(|e| {
                crate::say!("requiredItemsFailed", "couldn't read workshop:{item}'s required items: {error}", item = item, error = format!("{e:#}"));
                vec![]
            })
        })
        .unwrap_or_default();
    let mut ids = required.clone();
    let q = steam_workshop::Query { search: mod_id.into(), ..Default::default() };
    for i in steam_workshop::browse(pz::STEAM_APP, &q)?.items {
        if !ids.contains(&i.id) {
            ids.push(i.id);
        }
    }
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let mut out: Vec<Candidate> = steam_workshop::details(&refs)?
        .into_iter()
        .map(|item| {
            let builds: Vec<&String> = item.tags.iter().filter(|t| t.starts_with("Build ")).collect();
            Candidate {
                confirmed: pz::workshop_mod_ids(&item.description).iter().any(|m| m.eq_ignore_ascii_case(mod_id)),
                required_by_author: required.contains(&item.id),
                fits_build: builds.is_empty() || builds.iter().any(|t| **t == format!("Build {build}")),
                item,
            }
        })
        .collect();
    // confirmed matches for this build first (the author's own pick before others, then the
    // most subscribed, since translations mention the id too), then the rest in search order
    out.sort_by_key(|c| {
        (!c.sure(), !(c.required_by_author && c.confirmed), !c.confirmed, std::cmp::Reverse(c.confirmed as u64 * c.item.subscriptions))
    });
    Ok(out)
}

fn fetch_local(p: &Path) -> Result<Fetched> {
    let abs = std::path::absolute(p)?;
    let source = format!("local:{}", abs.display());
    if abs.is_dir() {
        return Ok(Fetched { source, root: abs, _temp: None, subscribed: false });
    }
    if abs.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let t = tempfile::tempdir()?;
        unzip(&std::fs::read(&abs)?, t.path())?;
        return Ok(Fetched { source, root: t.path().to_path_buf(), _temp: Some(t), subscribed: false });
    }
    bail!("{}: not a folder or .zip", abs.display())
}

/// A local HTTP server for tests: answers each request with the first route whose prefix matches
/// its request line (`"GET /x"`), and keeps every request line and body it got.
#[cfg(test)]
pub(crate) type Seen = std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>;

#[cfg(test)]
pub(crate) fn fake_http(routes: Vec<(&'static str, u16, String)>) -> (String, Seen) {
    use std::io::{BufRead, BufReader, Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let seen = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let log = seen.clone();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let (mut first, mut len) = (String::new(), 0);
            if r.read_line(&mut first).is_err() || first.is_empty() {
                continue;
            }
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let first = first.trim().to_string();
            let (status, reply) = routes.iter().find(|(p, ..)| first.starts_with(p)).map_or((404, "{}".to_string()), |(_, st, b)| (*st, b.clone()));
            log.lock().unwrap().push((first, String::from_utf8(body).unwrap()));
            let mut s = s;
            let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
        }
    });
    (base, seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_calls_time_out() {
        let t = agent().config().timeouts();
        assert_eq!((t.connect, t.global), (Some(CONNECT), Some(CALL)), "every call through the shared agent is bounded");
        // a server that accepts and never answers
        let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", silent.local_addr().unwrap());
        let start = std::time::Instant::now();
        assert!(get(&url, std::time::Duration::from_millis(300)).is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "gave up after the limit, not never");
    }

    #[test]
    fn a_host_refusing_requests_fails_fast() {
        let (base, seen) = fake_http(vec![]);
        REFUSED.lock().unwrap().push((host(&base).into(), std::time::Instant::now()));
        let e = http_get(&format!("{base}/page")).unwrap_err().to_string();
        assert!(e.contains("HTTP 429") && seen.lock().unwrap().is_empty(), "{e}");
    }

    #[test]
    fn requirements_found_through_the_mod_index() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let by_id = [("tsarslib".to_string(), s(&["2392709985", "111"])), ("Taken".to_string(), s(&["222"]))].into();
        let needs = [("TMC_Trolley", "tsarslib"), ("Other", "tsarslib"), ("TMC_Trolley", "Have"), ("A", "Taken"), ("A", "Nowhere")].map(|(a, b)| (a.to_string(), b.to_string()));
        let provided = HashSet::from(["Have".to_string()]);
        let tried = HashSet::from(["222".to_string()]);
        let (found, unknown) = from_index(&needs, &provided, &by_id, &tried);
        assert_eq!(found, [("TMC_Trolley".to_string(), "tsarslib".to_string(), "2392709985".to_string())]);
        assert_eq!(unknown, [("A".to_string(), "Nowhere".to_string())]);
    }

    #[test]
    fn unzip_refuses_escaping_paths() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("Good/mod.info", opts).unwrap();
            z.write_all(b"id=Good\n").unwrap();
            z.start_file("../escaped.txt", opts).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        let t = tempfile::tempdir().unwrap();
        let out = t.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let _ = unzip(buf.get_ref(), &out); // may error on the bad entry; must not write outside
        assert!(!t.path().join("escaped.txt").exists());
    }

    #[test]
    fn parses_sources() {
        use Source::*;
        assert_eq!(Source::parse("2169435993"), Workshop("2169435993".into()));
        assert_eq!(Source::parse("https://steamcommunity.com/sharedfiles/filedetails/?id=2169435993&searchtext="), Workshop("2169435993".into()));
        assert_eq!(Source::parse("https://github.com/owner/repo.git"), GitHub("owner/repo".into()));
        assert_eq!(Source::parse("owner/repo@v1"), GitHub("owner/repo@v1".into()));
        assert_eq!(Source::from_index("github:owner/repo@v1"), Some(GitHub("owner/repo@v1".into())));
    }
}
