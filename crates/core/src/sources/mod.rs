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
/// Every install and update comes through here, so held mods are left out here (see [`fetch_anyway`]).
pub fn fetch(paths: &Paths, sources: &[Source], resolve: bool) -> Vec<Fetched> {
    fetch_in(paths, sources, resolve, &held_sources(paths, &store::load(paths).unwrap_or_default()))
}

/// [`fetch`] with held mods too: their Update anyway.
pub fn fetch_anyway(paths: &Paths, sources: &[Source]) -> Vec<Fetched> {
    fetch_in(paths, sources, false, &HashSet::new())
}

/// The sources (`workshop:<id>`, `github:<spec>`) of the mods whose updates are held and that are in Zomboid/mods: one
/// whose folder is gone is installed again (that's no update).
pub fn held_sources(paths: &Paths, index: &store::Index) -> HashSet<String> {
    index.iter().filter(|(id, e)| e.held && paths.mods().join(id).is_dir()).map(|(_, e)| e.source.clone()).collect()
}

/// Workshop `items` but the `held` ones (said as left out).
fn not_held(held: &HashSet<String>, items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .filter(|i| {
            let out = held.contains(&format!("workshop:{i}"));
            if out {
                crate::say!("heldSkipped", "{source}: its updates are held, so it was left as it is", source = format!("workshop:{i}"));
            }
            !out
        })
        .collect()
}

/// The Workshop items of `sources` to fetch, collections opened by `expand` with `resolve` (see [`expand_workshop`]),
/// held ones left out after it; and the items whose pages weren't read.
fn workshop_wanted(sources: &[Source], resolve: bool, held: &HashSet<String>, expand: impl FnOnce(Vec<String>) -> (Vec<String>, Vec<String>)) -> (Vec<String>, Vec<String>) {
    let mut workshop: Vec<String> = sources.iter().filter_map(|s| if let Source::Workshop(i) = s { Some(i.clone()) } else { None }).collect();
    let mut unchecked = vec![];
    if resolve && !workshop.is_empty() {
        (workshop, unchecked) = expand(workshop);
    }
    // after the expansion: a collection can hold a held item too
    (not_held(held, workshop), unchecked)
}

/// The next round of required items the mod ID index found (`(mod, required id, item)`, see [`from_index`]): those not
/// `tried` yet, held ones left out.
fn required_round(found: Vec<(String, String, String)>, tried: &mut HashSet<String>, held: &HashSet<String>) -> Vec<String> {
    not_held(held, found.into_iter().map(|(.., item)| item).filter(|i| tried.insert(i.clone())).collect())
}

fn fetch_in(paths: &Paths, sources: &[Source], resolve: bool, held: &HashSet<String>) -> Vec<Fetched> {
    STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    let left_in = |source: String| {
        let out = held.contains(&source);
        if out {
            crate::say!("heldSkipped", "{source}: its updates are held, so it was left as it is", source = source);
        }
        !out
    };
    let mut out = vec![];
    let (mut workshop, mut unchecked) = workshop_wanted(sources, resolve, held, |ids| expand_workshop(paths, ids));
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
        workshop = required_round(items, &mut tried, held);
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
            Source::GitHub(spec) if !left_in(format!("github:{spec}")) => {}
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

/// Hold (`on`) or allow the updates of `ids` (rows of `mods`): a Subscribed one becomes PZMM's held copy of the version
/// in Steam's Workshop folder `ws` (see [`hold_subscribed`], `unsubscribe` the app's own), the rest are held or allowed
/// as they are, and feed.txt follows. A conversion that fails doesn't stop the others. Returns the ids whose hold
/// changed and what failed.
pub fn hold(
    paths: &Paths,
    mods: &std::collections::BTreeMap<String, store::Row>,
    ws: Option<&Path>,
    ids: &[String],
    on: bool,
    mut unsubscribe: impl FnMut(&str) -> Result<()>,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut items: Vec<&str> = vec![];
    for id in ids {
        if let Some(item) = mods.get(id).and_then(|m| m.source.strip_prefix("steam:")).filter(|_| on)
            && !items.contains(&item)
        {
            items.push(item);
        }
    }
    let mut failed = vec![];
    for item in items {
        let Some(root) = ws.map(|w| w.join(item)) else {
            failed.push(format!("workshop:{item}: Steam's Workshop folder wasn't found"));
            continue;
        };
        match hold_subscribed(paths, item, &root, &mut unsubscribe) {
            Ok(held) => crate::say!("holdCopied", "{ids}: PZMM keeps its own copy of the version you have, held, and unsubscribed it in Steam", ids = held.join(", ")),
            Err(e) => {
                crate::say!("holdFailed", "error: holding workshop:{item}: {error}", item = item, error = format!("{e:#}"));
                failed.push(format!("workshop:{item}: {e:#}"));
            }
        }
    }
    let changed = store::set_held(paths, ids, on)?;
    // before the game's ack (it reads them again then): held.txt and installed.txt are written by now, feed.txt here
    pz::feed::pending_for_game(paths);
    Ok((changed, failed))
}

/// Hold a Steam subscription's updates. Steam keeps subscriptions updated itself, so the item's files as they are now
/// (Steam's folder `root`, not a fresh download) are copied in as PZMM's own `workshop:<item>`, recorded held, and
/// only then is the item unsubscribed (`unsubscribe`: the app's own). A copy that fails unsubscribes nothing, and
/// what it did copy goes again. The copy keeps the time Steam updated it (so a newer version still shows), and
/// an unsubscribe that fails is said with the copy kept. Mods PZMM already has (an item held before, its subscription
/// back since) are left exactly as they are: only the ones it lacks are copied, and only those go on a failure.
/// Returns the mod ids newly held.
pub fn hold_subscribed(paths: &Paths, item: &str, root: &Path, unsubscribe: impl FnOnce(&str) -> Result<()>) -> Result<Vec<String>> {
    let source = format!("workshop:{item}");
    let before = store::load(paths)?;
    let ids = store::install_new(paths, root, &source)?;
    // never unsubscribed unless every mod of the item has PZMM's copy on disk now: copied just now, or there before
    let index = store::load(paths)?;
    let copied = |id: &str| index.iter().any(|(k, e)| k.eq_ignore_ascii_case(id) && e.source == source && paths.mods().join(k).is_dir());
    if !pz::find_mods(root, 8).iter().all(|m| copied(&m.id)) {
        // why each one couldn't be copied was said; a part copy would only shadow the subscription. Only mods new to
        // the index go: one it had (held, its folder gone) keeps its entry and backup
        let added: Vec<String> = ids.iter().filter(|id| !before.keys().any(|k| k.eq_ignore_ascii_case(id))).cloned().collect();
        if let Err(e) = store::remove(paths, &added, false) {
            crate::log!("error: workshop:{item}: removing the part copy: {e:#}");
        }
        bail!("not every mod could be copied, so it stays subscribed");
    }
    // held before the unsubscribe: no scan takes the copy for a leftover of the subscription (see `store::scan`)
    let at = steam_updated(item, root);
    let mut index = store::load(paths)?;
    for (_, e) in index.iter_mut().filter(|(id, _)| ids.contains(id)) {
        e.held = true;
        e.installed_at = at.unwrap_or(e.installed_at);
    }
    store::save(paths, &index)?;
    unsubscribe(item).context("PZMM's held copy is made, but Steam is still subscribed (is Steam running?): unsubscribe it in Steam, or Steam's copy keeps updating alongside")?;
    Ok(ids)
}

/// When Steam last updated subscribed item `item` in its folder `root`: its library's appworkshop_108600.acf
/// (`<library>/steamapps/workshop/content/108600/<item>`), else the newest file's time.
fn steam_updated(item: &str, root: &Path) -> Option<u64> {
    let acf = root.ancestors().nth(5).and_then(|lib| pz::server_detect::downloaded(lib).get(item).copied());
    acf.or_else(|| Some(pz::files::stats(root).modified).filter(|t| *t > 0))
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

/// What [`check_updates`] found.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Updates {
    /// Mods with a newer version to install.
    pub outdated: Vec<String>,
    /// Held mods the Workshop has a newer version of, and when it came out (seconds since 1970): not updated.
    pub held: std::collections::BTreeMap<String, u64>,
}

/// Mods PZMM installed from the Workshop that have been updated there since.
/// (Steam keeps subscriptions current itself; GitHub sources aren't checked yet.)
pub fn check_updates(paths: &Paths) -> Result<Updates> {
    let index = store::load(paths)?;
    let items: Vec<&str> = index.values().filter_map(|e| e.source.strip_prefix("workshop:")).collect::<HashSet<_>>().into_iter().collect();
    let updated: std::collections::HashMap<String, u64> =
        items.chunks(100).map(steam_workshop::details_fresh).collect::<Result<Vec<_>>>()?.concat().into_iter().map(|i| (i.id, i.time_updated)).collect();
    Ok(updates_in(paths, &index, &updated))
}

/// [`check_updates`] from each Workshop item's last update (`updated`, by item id).
fn updates_in(paths: &Paths, index: &store::Index, updated: &std::collections::HashMap<String, u64>) -> Updates {
    let mut out = Updates::default();
    for (id, e) in index {
        let Some(&t) = e.source.strip_prefix("workshop:").and_then(|item| updated.get(item)) else { continue };
        if t <= installed_at(paths, id, e) {
            continue;
        }
        if e.held {
            out.held.insert(id.clone(), t);
        } else {
            out.outdated.push(id.clone());
        }
    }
    out
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
        let entry = |item: &str| store::Entry { source: format!("workshop:{item}"), installed_at: 1, held: false };
        store::save(&paths, &[("Here".to_string(), entry("90000000001")), ("Gone".to_string(), entry("90000000002"))].into()).unwrap();
        let have = workshop_available(&paths);
        assert!(have.contains("90000000001") && !have.contains("90000000002"));
    }

    #[test]
    fn held_mods_are_not_outdated_but_their_newer_time_is_kept() {
        let (_t, paths) = store::tests::temp_paths();
        let entry = |item: &str, held| store::Entry { source: format!("workshop:{item}"), installed_at: 100, held };
        let index: store::Index = [("Free".to_string(), entry("1", false)), ("Held".to_string(), entry("2", true)), ("Current".to_string(), entry("3", true))].into();
        let updated = [("1".to_string(), 200), ("2".to_string(), 300), ("3".to_string(), 50)].into();
        assert_eq!(updates_in(&paths, &index, &updated), Updates { outdated: vec!["Free".into()], held: [("Held".to_string(), 300)].into() });
        // only held mods on disk: a held one whose folder is gone installs again
        assert!(held_sources(&paths, &index).is_empty());
        std::fs::create_dir_all(paths.mods().join("Held")).unwrap();
        assert_eq!(held_sources(&paths, &index), HashSet::from(["workshop:2".to_string()]));
    }

    #[test]
    fn a_subscription_is_held_by_copying_then_recording_then_unsubscribing() {
        let (t, paths) = store::tests::temp_paths();
        let steam = t.path().join("steam/77");
        let steam_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
        for id in ["A", "B"] {
            std::fs::create_dir_all(steam.join(format!("mods/{id}"))).unwrap();
            std::fs::write(steam.join(format!("mods/{id}/mod.info")), format!("id={id}\n")).unwrap();
            std::fs::File::options().write(true).open(steam.join(format!("mods/{id}/mod.info"))).unwrap().set_modified(steam_time).unwrap();
        }
        let mut called = vec![];
        let ids = hold_subscribed(&paths, "77", &steam, |item| {
            // by now the copy is in and recorded held
            let index = store::load(&paths).unwrap();
            assert!(["A", "B"].iter().all(|id| paths.mods().join(id).join("mod.info").exists() && index[*id].held && index[*id].source == "workshop:77"));
            called.push(item.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!((ids, called), (vec!["A".to_string(), "B".to_string()], vec!["77".to_string()]));
        // Steam's time, not now: a newer Workshop version still shows as held-with-newer
        assert_eq!(store::load(&paths).unwrap()["A"].installed_at, 1_500_000_000);
        // a copy that fails (a hand-installed B is in the way) unsubscribes nothing and leaves no part copy
        let (_t2, paths2) = store::tests::temp_paths();
        std::fs::create_dir_all(paths2.mods().join("B")).unwrap();
        std::fs::write(paths2.mods().join("B/mod.info"), "id=B\n").unwrap();
        let mut asked = false;
        assert!(hold_subscribed(&paths2, "77", &steam, |_| {
            asked = true;
            Ok(())
        }).is_err());
        assert!(!asked && !paths2.mods().join("A").exists() && store::load(&paths2).unwrap().is_empty());
        // and an unsubscribe that fails is said, the held copy kept
        let (_t3, paths3) = store::tests::temp_paths();
        let e = hold_subscribed(&paths3, "77", &steam, |_| anyhow::bail!("Steam isn't running")).unwrap_err();
        assert!(format!("{e:#}").contains("Steam is still subscribed"), "{e:#}");
        assert!(store::load(&paths3).unwrap()["A"].held);
    }

    #[test]
    fn a_held_item_in_an_expanded_collection_is_left_out() {
        let held = HashSet::from(["workshop:2".to_string()]);
        // the collection 9 opened (as expand_workshop would) into its items, one of them held, and one unread
        let expand = |ids: Vec<String>| {
            assert_eq!(ids, ["9"]);
            (vec!["1".to_string(), "2".to_string(), "3".to_string()], vec!["3".to_string()])
        };
        let (items, unchecked) = workshop_wanted(&[Source::Workshop("9".into()), Source::GitHub("o/r".into())], true, &held, expand);
        assert_eq!((items, unchecked), (vec!["1".to_string(), "3".to_string()], vec!["3".to_string()]));
        // no resolve: what was asked, held ones out
        let (items, _) = workshop_wanted(&[Source::Workshop("2".into()), Source::Workshop("4".into())], false, &held, |_| unreachable!());
        assert_eq!(items, ["4"]);
    }

    #[test]
    fn a_held_requirement_is_left_out_while_its_folder_is_there() {
        let (_t, paths) = store::tests::temp_paths();
        let index: store::Index = [("Req".to_string(), store::Entry { source: "workshop:5".into(), installed_at: 1, held: true })].into();
        // the mod ID index found what an item requires: the held item 5, a new item 6, and item 1 already fetched
        let found = || [("A", "req", "5"), ("A", "other", "6"), ("B", "x", "1")].map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).to_vec();
        let round = || required_round(found(), &mut HashSet::from(["1".to_string()]), &held_sources(&paths, &index));
        assert_eq!(round(), ["5", "6"], "missing from Zomboid/mods: not held there, so it installs");
        std::fs::create_dir_all(paths.mods().join("Req")).unwrap();
        assert_eq!(round(), ["6"], "on disk and held: not downloaded");
    }

    /// Steam's folder of item `item` with mods `ids`, each mod.info naming its `version`.
    fn steam_item(root: &Path, item: &str, ids: &[&str], version: &str) -> PathBuf {
        let dir = root.join(item);
        for id in ids {
            std::fs::create_dir_all(dir.join(format!("mods/{id}"))).unwrap();
            std::fs::write(dir.join(format!("mods/{id}/mod.info")), format!("id={id}\nname={version}\n")).unwrap();
        }
        dir
    }

    #[test]
    fn holding_again_leaves_the_held_copies_alone() {
        let (t, paths) = store::tests::temp_paths();
        let v1 = steam_item(&t.path().join("v1"), "77", &["A"], "v1");
        hold_subscribed(&paths, "77", &v1, |_| Ok(())).unwrap();
        // subscribed again in Steam, where the item now has a newer A, and B and C
        let v2 = steam_item(&t.path().join("v2"), "77", &["A", "B", "C"], "v2");
        let a = || std::fs::read_to_string(paths.mods().join("A/mod.info")).unwrap();
        // a part copy (a hand-installed B in the way): only C, which it added, goes; A stays as it was
        std::fs::create_dir_all(paths.mods().join("B")).unwrap();
        std::fs::write(paths.mods().join("B/mod.info"), "id=B\n").unwrap();
        assert!(hold_subscribed(&paths, "77", &v2, |_| Ok(())).is_err());
        let index = store::load(&paths).unwrap();
        assert!(a().contains("v1") && index["A"].held && !index.contains_key("C") && !paths.mods().join("C").exists());
        // the way clear: B and C are copied and held, A still the held v1
        std::fs::remove_dir_all(paths.mods().join("B")).unwrap();
        let mut ids = hold_subscribed(&paths, "77", &v2, |_| Ok(())).unwrap();
        ids.sort();
        assert_eq!(ids, ["B", "C"]);
        let index = store::load(&paths).unwrap();
        assert!(a().contains("v1") && ["A", "B", "C"].iter().all(|id| index[*id].held));
        // a held mod whose folder is gone is copied again, never counted as there
        std::fs::remove_dir_all(paths.mods().join("A")).unwrap();
        let mut called = false;
        assert_eq!(hold_subscribed(&paths, "77", &v2, |_| {
            called = true;
            Ok(())
        }).unwrap(), ["A"]);
        assert!(called && a().contains("v2") && store::load(&paths).unwrap()["A"].held);
        // and when it can't be (its staging spot is taken by a file): its entry stays, and nothing is unsubscribed
        std::fs::remove_dir_all(paths.mods().join("A")).unwrap();
        std::fs::create_dir_all(paths.zomboid.join(".pzmm-staging")).unwrap();
        std::fs::write(paths.zomboid.join(".pzmm-staging/A"), "x").unwrap();
        let mut asked = false;
        assert!(hold_subscribed(&paths, "77", &v2, |_| {
            asked = true;
            Ok(())
        }).is_err());
        assert!(!asked && store::load(&paths).unwrap()["A"].held, "its entry stays");
    }

    #[test]
    fn a_failed_conversion_doesnt_stop_the_others() {
        let (t, paths) = store::tests::temp_paths();
        let ws = t.path().join("ws");
        steam_item(&ws, "71", &["Ok"], "v1");
        steam_item(&ws, "72", &["Blocked"], "v1");
        std::fs::create_dir_all(paths.mods().join("Blocked")).unwrap();
        std::fs::write(paths.mods().join("Blocked/mod.info"), "id=Blocked\n").unwrap();
        // and a mod PZMM installed, held as it is
        let gh = t.path().join("gh");
        std::fs::create_dir_all(gh.join("Mine")).unwrap();
        std::fs::write(gh.join("Mine/mod.info"), "id=Mine\n").unwrap();
        store::install(&paths, &gh, "github:o/r").unwrap();
        let row = |id: &str, source: &str| (id.to_string(), store::Row { id: id.into(), source: source.into(), ..Default::default() });
        let mods = [row("Ok", "steam:71"), row("Blocked", "steam:72"), row("Mine", "github:o/r")].into();
        let mut unsubscribed = vec![];
        let ids = ["Ok".to_string(), "Blocked".to_string(), "Mine".to_string()];
        let (changed, failed) = hold(&paths, &mods, Some(&ws), &ids, true, |i| {
            unsubscribed.push(i.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!((changed, unsubscribed), (vec!["Mine".to_string()], vec!["71".to_string()]), "Ok was held by its conversion");
        assert!(failed.len() == 1 && failed[0].starts_with("workshop:72"), "{failed:?}");
        let index = store::load(&paths).unwrap();
        assert!(index["Ok"].held && index["Mine"].held && !index.contains_key("Blocked"));
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
