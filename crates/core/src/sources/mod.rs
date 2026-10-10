//! Where mods come from: Steam Workshop, GitHub, or a local zip/folder.

pub mod github;
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
        if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
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
            "workshop" => Some(Source::Workshop(v.into())),
            "github" => Some(Source::GitHub(v.into())),
            "local" => Some(Source::Local(v.into())),
            _ => None,
        }
    }
}

pub(crate) fn http_get(url: &str) -> Result<Vec<u8>> {
    let mut req = ureq::get(url).header("User-Agent", "pzmm");
    if url.starts_with("https://api.github.com/")
        && let Ok(token) = std::env::var("GITHUB_TOKEN") {
            req = req.header("Authorization", &format!("Bearer {token}"));
        }
    let mut res = req.call().with_context(|| format!("GET {url}"))?;
    Ok(res.body_mut().with_config().limit(2 << 30).read_to_vec()?)
}

pub(crate) fn unzip(bytes: &[u8], to: &Path) -> Result<()> {
    // zip's extract() refuses entries that escape `to` (../, absolute paths)
    zip::ZipArchive::new(std::io::Cursor::new(bytes))?.extract(to)?;
    Ok(())
}

/// Download and install. Workshop items go through one SteamCMD session. With `resolve`,
/// Workshop collections are expanded and required items are added. Errors are logged per
/// source so one bad entry doesn't stop the rest. Returns installed mod ids.
pub fn install(paths: &Paths, sources: &[Source], resolve: bool) -> Vec<String> {
    let mut ids = vec![];
    let mut report = |r: Result<Vec<String>>| match r {
        Ok(v) => ids.extend(v),
        Err(e) => crate::log!("error: {e:#}"),
    };
    let mut workshop: Vec<String> = sources.iter().filter_map(|s| if let Source::Workshop(i) = s { Some(i.clone()) } else { None }).collect();
    if resolve && !workshop.is_empty() {
        workshop = expand_workshop(paths, workshop);
    }
    if !workshop.is_empty() {
        let refs: Vec<&str> = workshop.iter().map(String::as_str).collect();
        match steam_workshop::download(paths, pz::STEAM_APP, &refs) {
            Ok(got) => {
                for (id, dir) in got {
                    report(dir.with_context(|| format!("workshop:{id}: download failed")).and_then(|d| store::install(paths, &d, &format!("workshop:{id}"))));
                }
            }
            Err(e) => report(Err(e)),
        }
    }
    for s in sources {
        match s {
            Source::Workshop(_) => {}
            Source::GitHub(spec) => report(github::download(spec).and_then(|t| store::install(paths, t.path(), &format!("github:{spec}")))),
            Source::Local(p) => report(install_local(paths, p)),
        }
    }
    ids
}

/// Workshop ids the game can already load: Steam subscriptions and PZMM installs.
pub fn workshop_available(paths: &Paths) -> HashSet<String> {
    let mut have: HashSet<String> = steam_workshop::content_dir(pz::STEAM_APP)
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
/// game can't already load. Network failures are logged and skipped.
fn expand_workshop(paths: &Paths, ids: Vec<String>) -> Vec<String> {
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let collections = steam_workshop::collections(&refs).unwrap_or_else(|e| {
        crate::log!("error: couldn't check for Workshop collections: {e:#}");
        Default::default()
    });
    let mut queue = std::collections::VecDeque::new();
    for id in ids {
        match collections.get(&id) {
            Some(items) => {
                crate::log!("collection {id}: {} item(s)", items.len());
                queue.extend(items.iter().cloned());
            }
            None => queue.push_back(id),
        }
    }
    let have = workshop_available(paths);
    let mut seen = HashSet::new();
    let mut out = vec![];
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) || out.len() >= 500 {
            continue;
        }
        out.push(id.clone());
        match steam_workshop::required_items(&id) {
            Ok(req) => {
                for r in req.into_iter().filter(|r| !have.contains(r) && !seen.contains(r)) {
                    crate::log!("workshop:{id} requires workshop:{r}, adding it");
                    queue.push_back(r);
                }
            }
            Err(e) => crate::log!("error: couldn't check what workshop:{id} requires: {e:#}"),
        }
    }
    out
}

/// Mods PZMM installed from the Workshop that have been updated there since.
/// (Steam keeps subscriptions current itself; GitHub sources aren't checked yet.)
pub fn check_updates(paths: &Paths) -> Result<Vec<String>> {
    let index = store::load(paths)?;
    let ours: Vec<(&String, &str, u64)> = index
        .iter()
        .filter_map(|(id, e)| {
            let item = e.source.strip_prefix("workshop:")?;
            // v0.1 imports have no install time: fall back to the mod folder's age
            let at = if e.installed_at > 0 {
                e.installed_at
            } else {
                std::fs::metadata(paths.mods().join(id))
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs())
            };
            Some((id, item, at))
        })
        .collect();
    let items: Vec<&str> = ours.iter().map(|o| o.1).collect::<HashSet<_>>().into_iter().collect();
    let updated: std::collections::HashMap<String, u64> =
        steam_workshop::details(&items)?.into_iter().map(|i| (i.id, i.time_updated)).collect();
    Ok(ours.into_iter().filter(|(_, item, at)| updated.get(*item).is_some_and(|t| t > at)).map(|(id, ..)| id.clone()).collect())
}

fn install_local(paths: &Paths, p: &Path) -> Result<Vec<String>> {
    let abs = std::path::absolute(p)?;
    let source = format!("local:{}", abs.display());
    if abs.is_dir() {
        return store::install(paths, &abs, &source);
    }
    if abs.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let t = tempfile::tempdir()?;
        unzip(&std::fs::read(&abs)?, t.path())?;
        return store::install(paths, t.path(), &source);
    }
    bail!("{}: not a folder or .zip", abs.display())
}

#[cfg(test)]
mod tests {
    use super::*;

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
