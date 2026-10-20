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

/// Per host, shared by every thread: no request before the time Steam's last HTTP 429 asked for, and
/// how many 429s in a row (each doubles the wait), so concurrent commands back off together.
type Cooldowns = std::collections::HashMap<String, (std::time::Instant, u32)>;
static COOL: std::sync::Mutex<Option<Cooldowns>> = std::sync::Mutex::new(None);

thread_local! {
    /// The Retry-After (seconds) of the 429 just seen on this thread (see [`status_ok`]).
    static RETRY_AFTER: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// A cooldown up to this is waited out; a longer one fails at once (a batch doesn't sit out each item).
const WAIT_OUT: std::time::Duration = std::time::Duration::from_secs(10);

/// How long `url`'s host is still cooling down after a 429.
pub(crate) fn cooling(url: &str) -> Option<std::time::Duration> {
    let c = COOL.lock().unwrap_or_else(|e| e.into_inner());
    c.as_ref()?.get(host(url)).and_then(|(until, _)| until.checked_duration_since(std::time::Instant::now()))
}

/// A reply read with `http_status_as_error(false)`, as ureq would give it, keeping a 429's Retry-After for [`polite`].
pub(crate) fn status_ok(r: ureq::http::Response<ureq::Body>) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let s = r.status().as_u16();
    if s == 429 {
        RETRY_AFTER.set(r.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok()));
    }
    if s >= 400 { Err(ureq::Error::StatusCode(s)) } else { Ok(r) }
}

fn host(url: &str) -> &str {
    url.split('/').nth(2).unwrap_or(url)
}

/// Run an HTTP request with Steam etiquette: Steam's API at most every 350 ms and its web pages at most
/// every second (each shared across threads), and after an HTTP 429 the host's cooldown (see [`COOL`]):
/// Retry-After when Steam gives one, else 2, 4, 8... s (up to 2 minutes), waited out when short.
pub(crate) fn polite<T>(url: &str, mut send: impl FnMut() -> Result<T, ureq::Error>) -> Result<T> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static LAST_API: Mutex<Option<Instant>> = Mutex::new(None);
    static LAST_PAGE: Mutex<Option<Instant>> = Mutex::new(None);
    const LIMITED: &str = "Steam is limiting requests right now (HTTP 429). Wait a minute and try again.";
    let lane = if url.contains("steampowered.com") {
        Some((&LAST_API, Duration::from_millis(350)))
    } else if url.contains("steamcommunity.com") {
        Some((&LAST_PAGE, Duration::from_secs(1)))
    } else {
        None
    };
    for _ in 0..3 {
        if let Some(left) = cooling(url) {
            if left > WAIT_OUT {
                bail!("{LIMITED}");
            }
            crate::say!("steamWait", "Steam is limiting requests: trying again in {seconds} s", seconds = left.as_secs_f32().ceil() as u64);
            std::thread::sleep(left);
        }
        if let Some((last, every)) = lane {
            let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(gap) = last.and_then(|t| every.checked_sub(t.elapsed())) {
                std::thread::sleep(gap);
            }
            *last = Some(Instant::now());
        }
        RETRY_AFTER.set(None);
        match send() {
            Ok(v) => {
                // a cooldown that's over starts again from 2 s; one another thread's 429 started meanwhile stays
                if let Some(c) = COOL.lock().unwrap_or_else(|e| e.into_inner()).as_mut()
                    && c.get(host(url)).is_some_and(|(until, _)| *until <= Instant::now())
                {
                    c.remove(host(url));
                }
                return Ok(v);
            }
            Err(ureq::Error::StatusCode(429)) => {
                let mut c = COOL.lock().unwrap_or_else(|e| e.into_inner());
                let e = c.get_or_insert_with(Default::default).entry(host(url).to_string()).or_insert((Instant::now(), 0));
                e.1 += 1;
                let secs = RETRY_AFTER.take().unwrap_or(1 << e.1.min(7)).min(300);
                e.0 = e.0.max(Instant::now() + Duration::from_secs(secs));
            }
            Err(e) => return Err(e).with_context(|| format!("requesting {url}")),
        }
    }
    bail!("{LIMITED}")
}

/// Longest wait for one web call (connecting: `CONNECT`); downloads get `DOWNLOAD`.
const CALL: std::time::Duration = std::time::Duration::from_secs(30);
const CONNECT: std::time::Duration = std::time::Duration::from_secs(10);
const DOWNLOAD: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The one HTTP agent every web call goes through, so a silent server never hangs PZMM
/// (ureq waits forever by default). It trusts the computer's own certificates (see [`roots`]), and a
/// certificate it can't trust is said plainly (see [`intercepted`]).
pub(crate) fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| agent_with(roots()))
}

fn agent_with(roots: ureq::tls::RootCerts) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT))
        .timeout_global(Some(CALL))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(roots).build())
        .middleware(plain_certs)
        .build()
        .into()
}

/// The computer's trusted certificates, so antivirus, VPN and company networks that inspect HTTPS with
/// a root of their own work; PZMM's bundled Mozilla roots when they can't be loaded, or with
/// `PZMM_BUNDLED_ROOTS=1`.
fn roots() -> ureq::tls::RootCerts {
    if std::env::var_os("PZMM_BUNDLED_ROOTS").is_some_and(|v| v == "1") {
        return ureq::tls::RootCerts::WebPki;
    }
    match rustls_platform_verifier::Verifier::new(std::sync::Arc::new(rustls::crypto::ring::default_provider())) {
        Ok(_) => ureq::tls::RootCerts::PlatformVerifier,
        Err(e) => {
            crate::log!("error: couldn't load this computer's trusted certificates, using PZMM's own: {e}");
            ureq::tls::RootCerts::WebPki
        }
    }
}

/// Said instead of a certificate error (the app shows it in the player's language: `errors.certIntercepted`).
pub(crate) const CERT_INTERCEPTED: &str = "Something on your computer is intercepting secure connections (usually antivirus, firewall or VPN). Turn off its HTTPS scanning or add PZMM as an exception.";

fn plain_certs(req: ureq::http::Request<ureq::SendBody>, next: ureq::middleware::MiddlewareNext) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    next.handle(req).map_err(intercepted)
}

/// A certificate PZMM can't trust (rustls' "invalid peer certificate: UnknownIssuer"...) as [`CERT_INTERCEPTED`],
/// with the original after it for bug reports; any other error as it is.
fn intercepted(e: ureq::Error) -> ureq::Error {
    let text = e.to_string();
    if text.contains("invalid peer certificate") {
        ureq::Error::Io(std::io::Error::other(format!("{CERT_INTERCEPTED} ({text})")))
    } else {
        e
    }
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
        let mut req = agent().get(url).config().timeout_global(Some(limit)).http_status_as_error(false).build().header("User-Agent", crate::USER_AGENT);
        if url.starts_with("https://api.github.com/")
            && let Some(token) = crate::settings::github_token()
        {
            req = req.header("Authorization", &format!("Bearer {token}"));
        }
        status_ok(req.call()?)?.body_mut().with_config().limit(2 << 30).read_to_vec()
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
        let mut provided: HashSet<String> = store::ids(paths).unwrap_or_default();
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
            // an unsubscribe can leave the item's folder behind, empty: that's no subscription
            if !steam_workshop::valid_id(id) || pz::find_mods(&root, 8).is_empty() {
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

/// Copy fetched mods into Zomboid/mods (as an `update`: see [`store::update`]). Returns the installed mod ids.
pub fn install_fetched(paths: &Paths, fetched: &[Fetched], update: bool) -> Vec<String> {
    let mut ids = vec![];
    for f in fetched {
        if f.subscribed {
            let found: Vec<String> = pz::find_mods(&f.root, 8).into_iter().map(|m| m.id).collect();
            // nothing left in Steam's folder: PZMM's own copy stays
            if found.is_empty() {
                crate::log!("error: {}: no mod.info found", f.source);
                continue;
            }
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
        match if update { store::update(paths, &f.root, &f.source) } else { store::install(paths, &f.root, &f.source) } {
            Ok(v) => ids.extend(v),
            Err(e) => crate::log!("error: {e:#}"),
        }
    }
    let cap = crate::settings::load().backup_cap_mb << 20;
    if cap > 0 && let Err(e) = store::prune_backups(paths, cap, &ids) {
        crate::say!("pruneFailed", "error: pruning backups: {error}", error = format!("{e:#}"));
    }
    ids
}

/// Download and install in one go (the CLI; the app splits the two to keep its lock short).
pub fn install(paths: &Paths, sources: &[Source], resolve: bool) -> Vec<String> {
    install_fetched(paths, &fetch(paths, sources, resolve), false)
}

/// [`install`] as an update of what PZMM installed.
pub fn update(paths: &Paths, sources: &[Source]) -> Vec<String> {
    install_fetched(paths, &fetch(paths, sources, false), true)
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
        .filter(|e| std::fs::read_dir(e.path().join("mods")).is_ok_and(|mut d| d.next().is_some()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    // only copies still in Zomboid/mods (a folder deleted by hand isn't available)
    let index = store::load(paths).unwrap_or_default();
    have.extend(index.into_iter().filter(|(id, _)| paths.mods().join(id).is_dir()).filter_map(|(_, e)| e.source.strip_prefix("workshop:").map(String::from)));
    have
}

/// Replace collections with their items, then add required items (recursively) that the
/// game can't already load. Also returns the items whose pages Steam didn't show (said once in
/// the log), whose requirements weren't checked.
pub fn expand_workshop(paths: &Paths, ids: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut queue = std::collections::VecDeque::new();
    // collections inside collections are opened too (a few levels deep)
    let (mut pending, mut opened) = (ids, HashSet::new());
    for depth in 0..4 {
        let refs: Vec<&str> = pending.iter().map(String::as_str).collect();
        let collections = if depth == 3 {
            Default::default()
        } else {
            steam_workshop::collections(&refs).unwrap_or_else(|e| {
                crate::say!("collectionsFailed", "error: couldn't check for Workshop collections: {error}", error = format!("{e:#}"));
                Default::default()
            })
        };
        let mut inner = vec![];
        for id in std::mem::take(&mut pending) {
            match collections.get(&id) {
                Some(items) if opened.insert(id.clone()) => {
                    crate::say!("collection", "collection {id}: {count} item(s)", id = id, count = items.len());
                    inner.extend(items.iter().cloned());
                }
                Some(_) => {}
                None => queue.push_back(id),
            }
        }
        if inner.is_empty() {
            break;
        }
        pending = inner;
    }
    let have = workshop_available(paths);
    let mut seen = HashSet::new();
    let (mut out, mut unchecked, mut error, mut dropped) = (vec![], vec![], None, 0);
    while let Some(id) = queue.pop_front() {
        // a big collection reads hundreds of pages: Stop ends it with nothing to download
        if STOP.load(std::sync::atomic::Ordering::SeqCst) {
            return (vec![], vec![]);
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        if out.len() >= 500 {
            dropped += 1;
            continue;
        }
        crate::progress(seen.len() - 1, seen.len() + queue.len());
        out.push(id.clone());
        // this and the next 99 in one details request: only the ones with required items get a page read
        steam_workshop::warm(&std::iter::once(id.as_str()).chain(queue.iter().map(String::as_str)).take(100).collect::<Vec<_>>());
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
    if dropped > 0 {
        crate::say!("tooManyItems", "error: {count} more Workshop item(s) left out: at most 500 are added at once", count = dropped);
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
        items.chunks(100).map(steam_workshop::details_fresh).collect::<Result<Vec<_>>>()?.concat().into_iter().map(|i| (i.id, i.time_updated)).collect();
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
            // a reply "retry-after:N" is sent as that header instead
            let extra = reply.strip_prefix("retry-after:").map(|n| format!("Retry-After: {n}\r\n")).unwrap_or_default();
            let _ = write!(s, "HTTP/1.1 {status} X\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
        }
    });
    (base, seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An HTTPS server on localhost with a certificate from tests/tls's test CA (its key thrown away), answering "ok".
    fn test_server() -> String {
        use rustls::pki_types::pem::PemObject;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tls");
        let certs: Vec<CertificateDer> = CertificateDer::pem_file_iter(dir.join("server.pem")).unwrap().map(|c| c.unwrap()).collect();
        let key = PrivateKeyDer::from_pem_file(dir.join("server.pkcs8.key")).unwrap();
        let config = std::sync::Arc::new(
            rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .unwrap(),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let config = config.clone();
                std::thread::spawn(move || {
                    use std::io::{Read, Write};
                    let mut tls = rustls::StreamOwned::new(rustls::ServerConnection::new(config).unwrap(), stream);
                    let (mut head, mut b) = (Vec::new(), [0u8; 1]);
                    while !head.ends_with(b"\r\n\r\n") && tls.read(&mut b).is_ok_and(|n| n == 1) {
                        head.push(b[0]);
                    }
                    let _ = tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
                    let _ = tls.flush();
                });
            }
        });
        format!("https://localhost:{port}/")
    }

    #[test]
    fn certificates_are_checked_against_the_computers_and_untrusted_ones_said_plainly() {
        let url = test_server();
        let read = |a: &ureq::Agent| a.get(&url).call().and_then(|mut r| r.body_mut().read_to_string());
        // the server itself works, with its CA trusted
        let ca = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tls/ca.pem")).unwrap();
        let ca = ureq::tls::Certificate::from_pem(&ca).unwrap();
        assert_eq!(read(&agent_with(ureq::tls::RootCerts::new_with_certs(&[ca]))).unwrap(), "ok");
        // the computer's certificates: on CI the test CA is installed (PZMM_TEST_CA_TRUSTED=1) and it connects, as an
        // antivirus' root would; anywhere else it isn't, and the error says so plainly
        let platform = read(&agent_with(roots()));
        if std::env::var_os("PZMM_TEST_CA_TRUSTED").is_some() {
            assert_eq!(platform.unwrap(), "ok");
        } else {
            let e = platform.unwrap_err().to_string();
            assert!(e.contains(CERT_INTERCEPTED), "{e}");
        }
        // PZMM's own bundled roots never trust it
        assert!(read(&agent_with(ureq::tls::RootCerts::WebPki)).unwrap_err().to_string().contains(CERT_INTERCEPTED));
    }

    #[test]
    fn web_calls_time_out() {
        let t = agent().config().timeouts();
        // certificate errors are said plainly, the rest left alone
        let cert = intercepted(ureq::Error::Rustls(rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer))).to_string();
        assert!(cert.contains(CERT_INTERCEPTED) && cert.contains("UnknownIssuer"), "{cert}");
        assert_eq!(intercepted(ureq::Error::StatusCode(404)).to_string(), "http status: 404");
        assert_eq!((t.connect, t.global), (Some(CONNECT), Some(CALL)), "every call through the shared agent is bounded");
        // a server that accepts and never answers
        let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", silent.local_addr().unwrap());
        let start = std::time::Instant::now();
        assert!(get(&url, std::time::Duration::from_millis(300)).is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "gave up after the limit, not never");
    }

    #[test]
    fn a_host_cooling_down_fails_fast() {
        let (base, seen) = fake_http(vec![]);
        COOL.lock().unwrap().get_or_insert_with(Default::default).insert(host(&base).into(), (std::time::Instant::now() + std::time::Duration::from_secs(60), 3));
        let e = http_get(&format!("{base}/page")).unwrap_err().to_string();
        assert!(e.contains("HTTP 429") && seen.lock().unwrap().is_empty(), "{e}");
    }

    #[test]
    fn a_success_clears_only_a_cooldown_that_is_over() {
        use std::time::{Duration, Instant};
        let url = "http://cooldown-kept.invalid/x";
        let cool = |until: Instant| COOL.lock().unwrap().get_or_insert_with(Default::default).insert(host(url).into(), (until, 3));
        // another thread's 429 while this request was out
        polite(url, || Ok(cool(Instant::now() + Duration::from_secs(60)))).unwrap();
        assert!(cooling(url).is_some_and(|left| left > Duration::from_secs(50)), "an active cooldown stays");
        cool(Instant::now());
        polite(url, || Ok(())).unwrap();
        assert!(COOL.lock().unwrap().as_ref().unwrap().get(host(url)).is_none(), "one that's over is cleared");
    }

    #[test]
    fn a_429_cools_the_host_down_for_every_thread_by_its_retry_after() {
        let (base, seen) = fake_http(vec![("GET /busy", 429, "retry-after:30".into())]);
        // Retry-After 30 s: past what's waited out, so the first 429 gives up and the host stays cooling
        let e = http_get(&format!("{base}/busy")).unwrap_err().to_string();
        assert!(e.contains("HTTP 429"), "{e}");
        assert_eq!(seen.lock().unwrap().len(), 1, "one request, then the cooldown");
        let left = cooling(&base).unwrap();
        assert!(left > std::time::Duration::from_secs(25), "{left:?}");
        // another thread asking the same host doesn't reach it
        let url = format!("{base}/other");
        assert!(std::thread::spawn(move || http_get(&url)).join().unwrap().is_err());
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_short_cooldown_is_waited_out_and_doubles_without_retry_after() {
        let (base, seen) = fake_http(vec![("GET /busy", 429, "{}".into())]);
        let start = std::time::Instant::now();
        assert!(http_get(&format!("{base}/busy")).is_err());
        // 2 s, then 4 s waited out, then 8 s left on the host
        assert_eq!(seen.lock().unwrap().len(), 3);
        assert!(start.elapsed() >= std::time::Duration::from_secs(6), "{:?}", start.elapsed());
        assert!(cooling(&base).unwrap() > std::time::Duration::from_secs(6));
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
    fn workshop_available_needs_the_folder() {
        let (_t, paths) = store::tests::temp_paths();
        std::fs::create_dir_all(paths.mods().join("Here")).unwrap();
        let entry = |item: &str| store::Entry { source: format!("workshop:{item}"), installed_at: 1 };
        store::save(&paths, &[("Here".to_string(), entry("90000000001")), ("Gone".to_string(), entry("90000000002"))].into()).unwrap();
        let have = workshop_available(&paths);
        assert!(have.contains("90000000001") && !have.contains("90000000002"));
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
