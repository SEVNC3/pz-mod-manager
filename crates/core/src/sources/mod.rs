//! Where mods come from: Steam Workshop, GitHub, or a local zip/folder.

pub mod github;
pub mod steam_workshop;

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

/// Download and install. Workshop items go through one SteamCMD session. Errors are
/// printed per source so one bad entry doesn't stop the rest. Returns installed ids.
pub fn install(paths: &Paths, sources: &[Source]) -> Vec<String> {
    let mut ids = vec![];
    let mut report = |r: Result<Vec<String>>| match r {
        Ok(v) => ids.extend(v),
        Err(e) => eprintln!("error: {e:#}"),
    };
    let workshop: Vec<&str> = sources.iter().filter_map(|s| if let Source::Workshop(i) = s { Some(i.as_str()) } else { None }).collect();
    if !workshop.is_empty() {
        match steam_workshop::download(paths, pz::STEAM_APP, &workshop) {
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
