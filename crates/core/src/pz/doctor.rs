//! Health check for the active profile and server configs ("doctor"): problems the
//! game reports badly or not at all (it skips a mod silently, or kicks players later).

use std::collections::{BTreeMap, HashMap, HashSet};

use super::{Paths, server};
use crate::store::Row;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// The game won't load something, or players can't join.
    Error,
    /// Likely to misbehave.
    Warning,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Finding {
    pub level: Level,
    /// The mod or file it's about.
    pub subject: String,
    /// In English (the CLI, diagnostics).
    pub text: String,
    /// The app's wording: doctor.<key> in its locale files, with the values for its {placeholders}.
    pub key: &'static str,
    pub vars: BTreeMap<&'static str, String>,
    /// What would fix it, for a button beside it.
    pub action: Option<Action>,
}

/// A fix the UI can offer: install a missing mod, or disable a clashing one.
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Install(String),
    Disable(String),
}

/// A finding worded by the English template (with {name} placeholders) and keyed for the app;
/// scripts/i18n-check.ts checks the template matches doctor.<key> in en.json.
fn finding(level: Level, subject: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    Finding { level, subject: subject.into(), text: crate::fill(template, vars), key, vars: vars.iter().cloned().collect(), action: None }
}

fn err(subject: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    finding(Level::Error, subject, key, template, vars)
}

fn warn(subject: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    finding(Level::Warning, subject, key, template, vars)
}

/// Check the enabled mods (in load order) for `build` (41/42), against everything
/// installed (`mods`), plus every server config.
pub fn check(paths: &Paths, enabled: &[String], mods: &BTreeMap<String, Row>, build: u8) -> Vec<Finding> {
    let mut out = check_folders();
    // ids the Steam subscriptions provide (the scan shows a local copy instead when both exist)
    let subscribed: HashSet<String> = super::workshop_dir()
        .map(|ws| super::workshop_mods(&ws).into_iter().map(|m| m.id).collect())
        .unwrap_or_default();
    let pos: HashMap<&str, usize> = enabled.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    for (i, id) in enabled.iter().enumerate() {
        let Some(m) = mods.get(id) else {
            out.push(err(id, "notInstalled", "enabled but not installed: the game skips it", &[]));
            continue;
        };
        for r in &m.requires {
            let req = [("req", r.clone())];
            match pos.get(r.as_str()) {
                None if mods.contains_key(r) => out.push(err(id, "needsDisabled", "needs {req}, which is disabled", &req)),
                None => out.push(Finding { action: Some(Action::Install(r.clone())), ..err(id, "needsMissing", "needs {req}, which isn't installed", &req) }),
                Some(&j) if j > i => out.push(err(id, "needsLater", "needs {req}, which loads after it", &req)),
                Some(_) => {}
            }
        }
        for x in &m.incompatible {
            if pos.contains_key(x.as_str()) {
                let other = [("other", x.clone())];
                out.push(Finding { action: Some(Action::Disable(x.clone())), ..err(id, "incompatible", "is incompatible with {other}, which is also enabled", &other) });
            }
        }
        for a in &m.load_after {
            if pos.get(a.as_str()).is_some_and(|&j| j > i) {
                out.push(warn(id, "loadAfter", "should load after {other}", &[("other", a.clone())]));
            }
        }
        for b in &m.load_before {
            if pos.get(b.as_str()).is_some_and(|&j| j < i) {
                out.push(warn(id, "loadBefore", "should load before {other}", &[("other", b.clone())]));
            }
        }
        if build >= 42 && !m.supports_42 {
            out.push(warn(id, "only41", "is built for Build 41 (no 42/ or common/ folder); on Build 42 it may error or do nothing", &[]));
        } else if build == 41 && !m.supports_41 {
            out.push(warn(id, "only42", "is built for Build 42 only (no root mod.info)", &[]));
        }
        if !m.source.starts_with("steam:") && subscribed.contains(id) {
            out.push(warn(id, "twoCopies", "is in Zomboid/mods and also subscribed in Steam: the Zomboid/mods copy wins and won't get Steam's updates", &[]));
        }
    }
    out.extend(check_servers(paths, mods, build));
    out
}

/// Folders that are set (by an environment variable or in Settings) but wrong, and folder warnings.
fn check_folders() -> Vec<Finding> {
    let mut out = vec![];
    for f in super::folders::list(&crate::settings::load()) {
        let Some(p) = &f.path else { continue };
        let path = ("path", p.display().to_string());
        let folder = ("folder", f.label.to_string());
        let env = f.env.filter(|_| f.overridden).map(|e| ("env", e.to_string()));
        match (f.status, env) {
            ("missing", Some(env)) => out.push(err(f.label, "missingEnv", "{path} doesn't exist (set by {env})", &[path.clone(), env])),
            ("missing", None) if f.custom => out.push(err(f.label, "missingSet", "{path} doesn't exist (set in Settings)", std::slice::from_ref(&path))),
            ("missing", None) if f.key == "zomboid" => out.push(warn(f.label, "notYet", "{path} doesn't exist yet: start the game once", std::slice::from_ref(&path))),
            ("not_pz", Some(env)) => out.push(warn(f.label, "notPzEnv", "{path} doesn't look like the {folder} (set by {env})", &[path.clone(), folder, env])),
            ("not_pz", None) if f.custom => out.push(warn(f.label, "notPzSet", "{path} doesn't look like the {folder} (set in Settings)", &[path.clone(), folder])),
            ("not_pz", None) => out.push(warn(f.label, "notPz", "{path} doesn't look like the {folder}", &[path.clone(), folder])),
            _ => {}
        }
        out.extend(f.warnings.iter().map(|w| warn(f.label, "folderWarning", "{path}: {warning}", &[path.clone(), ("warning", w.clone())])));
    }
    out
}

/// [`mask_user`] with this account's name (its home folder's name).
pub fn mask_home_user(text: &str) -> String {
    let user = dirs::home_dir().and_then(|h| h.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
    mask_user(text, &user)
}

/// `text` with `user` (the account name) hidden wherever it's a folder in a path
/// (`C:\Users\<user>\...`, `/home/<user>/...`), for bug reports.
pub fn mask_user(text: &str, user: &str) -> String {
    if user.len() < 2 {
        return text.into();
    }
    // ASCII lowercasing keeps byte offsets identical
    let (hay, needle) = (text.to_ascii_lowercase(), user.to_ascii_lowercase());
    let (mut out, mut last, mut from) = (String::with_capacity(text.len()), 0, 0);
    while let Some(i) = hay[from..].find(&needle).map(|i| i + from) {
        let end = i + needle.len();
        let after = hay[end..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || "._-".contains(c)));
        if hay[..i].ends_with(['\\', '/']) && after {
            out.push_str(&text[last..i]);
            out.push_str("<user>");
            last = end;
        }
        from = end;
    }
    out + &text[last..]
}

/// Server configs whose Mods=, WorkshopItems= and Map= disagree.
fn check_servers(paths: &Paths, mods: &BTreeMap<String, Row>, build: u8) -> Vec<Finding> {
    let mut out = vec![];
    for name in server::list(paths) {
        let Ok(file) = server::path(paths, &name) else { continue };
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(cfg) = server::read(&file) else { continue };
        let subject = format!("{name}.ini");
        let workshop: HashSet<&str> = cfg.workshop.iter().map(String::as_str).collect();
        let mut needed_items = HashSet::new();
        for id in &cfg.mods {
            match mods.get(id) {
                Some(m) => {
                    if let Some(item) = crate::sources::workshop_item(&m.source) {
                        needed_items.insert(item.to_string());
                        if !workshop.contains(item) {
                            let vars = [("id", id.clone()), ("item", item.to_string())];
                            out.push(err(&subject, "itemNotListed", "Mods= has {id}, but its Workshop item {item} isn't in WorkshopItems=: the server won't download it", &vars));
                        }
                    }
                }
                None => out.push(warn(&subject, "serverModMissing", "Mods= has {id}, which isn't installed here, so PZMM can't check it", &[("id", id.clone())])),
            }
        }
        for item in &cfg.workshop {
            if !needed_items.contains(item) && mods.values().any(|m| m.source.ends_with(&format!(":{item}"))) {
                out.push(warn(&subject, "itemUnused", "WorkshopItems= has {item}, but none of its mods are in Mods=", &[("item", item.clone())]));
            }
        }
        let mods_line = text.lines().find_map(|l| l.strip_prefix("Mods=")).unwrap_or("");
        if build >= 42 && !cfg.mods.is_empty() && !mods_line.contains('\\') {
            out.push(err(&subject, "backslash", "Mods= entries need a leading backslash on Build 42 (\\ModId)", &[]));
        }
        let map_line = text.lines().find_map(|l| l.strip_prefix("Map=")).unwrap_or("");
        let maps: Vec<&str> = map_line.split(';').map(str::trim).filter(|m| !m.is_empty()).collect();
        let mod_maps = server::mod_maps(&cfg.mods, mods, build);
        for mm in &mod_maps {
            if !maps.contains(&mm.as_str()) {
                out.push(warn(&subject, "mapNotListed", "a mod adds the map {map}, but Map= doesn't list it", &[("map", mm.clone())]));
            }
        }
        if let Some(base) = maps.iter().position(|m| server::is_base_map(m))
            && maps[base + 1..].iter().any(|m| mod_maps.iter().any(|mm| mm == m))
        {
            out.push(warn(&subject, "mapsAfterBase", "Map= lists mod maps after Muldraugh, KY: mod maps must come first", &[]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_paths;
    use std::fs;
    use std::path::PathBuf;

    fn row(id: &str, dir: PathBuf, source: &str, requires: &[&str]) -> Row {
        let versioned = dir.join("42").is_dir();
        Row {
            id: id.into(),
            name: id.into(),
            source: source.into(),
            info: dir.join("mod.info"),
            dir,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: !versioned,
            supports_42: versioned,
        }
    }

    #[test]
    fn masks_the_user_name_in_paths() {
        let text = r"Zomboid: C:\Users\Sam\Zomboid, data: /home/sam/.local/share/pzmm, Sam's profile, C:\Users\Samuel\x, D:\sam";
        assert_eq!(
            mask_user(text, "Sam"),
            r"Zomboid: C:\Users\<user>\Zomboid, data: /home/<user>/.local/share/pzmm, Sam's profile, C:\Users\Samuel\x, D:\<user>"
        );
        assert_eq!(mask_user(r"C:\Users\A\x", "A"), r"C:\Users\A\x", "one letter is too short to mask safely");
    }

    #[test]
    fn finds_setup_problems() {
        let (t, paths) = temp_paths();
        let b41 = t.path().join("Old");
        fs::create_dir_all(&b41).unwrap();
        fs::write(b41.join("mod.info"), "id=Old\n").unwrap();
        let b42 = t.path().join("New");
        fs::create_dir_all(b42.join("42")).unwrap();
        let mods = BTreeMap::from([
            ("Old".to_string(), row("Old", b41, "steam:1", &[])),
            ("New".to_string(), row("New", b42.clone(), "steam:2", &["Lib"])),
            ("Lib".to_string(), row("Lib", b42, "steam:3", &[])),
        ]);
        let enabled = vec!["New".to_string(), "Old".to_string(), "Gone".to_string()];
        let found = check(&paths, &enabled, &mods, 42);
        let texts: Vec<String> = found.iter().map(|f| format!("{}: {}", f.subject, f.text)).collect();
        assert!(texts.iter().any(|t| t.starts_with("New: needs Lib, which is disabled")), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("Old: is built for Build 41")), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("Gone: enabled but not installed")), "{texts:?}");
        // fixes the UI can offer: install what's missing, disable the other of an incompatible pair
        let mut clash = mods.clone();
        clash.insert("Old".into(), Row { requires: vec!["Lib2".into()], incompatible: vec!["New".into()], ..mods["Old"].clone() });
        let actions: Vec<_> = check(&paths, &["New".into(), "Lib".into(), "Old".into()], &clash, 41).into_iter().filter_map(|f| f.action).collect();
        assert_eq!(actions, [Action::Install("Lib2".into()), Action::Disable("New".into())]);

        fs::create_dir_all(paths.zomboid.join("Server")).unwrap();
        fs::write(paths.zomboid.join("Server/srv.ini"), "Mods=New;Lib\nWorkshopItems=2\nMap=Muldraugh, KY\n").unwrap();
        let found = check(&paths, &[], &mods, 42);
        let texts: Vec<String> = found.iter().map(|f| f.text.clone()).collect();
        assert!(texts.iter().any(|t| t.contains("Lib, but its Workshop item 3 isn't in WorkshopItems=")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("leading backslash")), "{texts:?}");
    }
}
