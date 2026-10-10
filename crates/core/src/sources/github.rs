use anyhow::{Result, bail};
use tempfile::TempDir;

/// Download `owner/repo[@ref]` (default branch when no ref) and extract it to a temp dir.
pub fn download(spec: &str) -> Result<TempDir> {
    let (repo, git_ref) = spec.split_once('@').unwrap_or((spec, ""));
    let valid = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if !matches!(repo.split_once('/'), Some((o, r)) if valid(o) && valid(r)) {
        bail!("bad GitHub repo: {spec} (expected owner/repo)");
    }
    if git_ref.contains(['?', '#', ' ']) {
        bail!("bad GitHub ref: {git_ref}");
    }
    println!("downloading github:{spec}...");
    let url = format!("https://api.github.com/repos/{repo}/zipball{}", if git_ref.is_empty() { String::new() } else { format!("/{git_ref}") });
    let dir = tempfile::tempdir()?;
    super::unzip(&super::http_get(&url)?, dir.path())?;
    Ok(dir)
}
