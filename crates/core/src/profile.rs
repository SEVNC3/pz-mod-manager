//! Profiles: named mod setups stored as `<data>/profiles/<name>/modlist.txt`.
//!
//! ```text
//! # --- Maps
//! +Muldraugh_Expanded      enabled; line order is load order
//! -SomeMod                 known but disabled
//! ```
//! The active profile is applied to the game's mod list; changes made in-game are
//! adopted back on the next sync, so the two never drift.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::pz::{self, Paths};

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Mod { id: String, on: bool },
    Separator(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub name: String,
    pub lines: Vec<Line>,
}

impl Profile {
    /// Each mod appears once (first line wins); empty ids are dropped.
    pub fn parse(name: &str, text: &str) -> Profile {
        let mut seen = HashSet::new();
        let lines = text
            .lines()
            .map(str::trim)
            .filter_map(|l| {
                if let Some(s) = l.strip_prefix("# ---") {
                    Some(Line::Separator(s.trim().into()))
                } else if let Some(id) = l.strip_prefix('+') {
                    Some(Line::Mod { id: id.trim().into(), on: true })
                } else {
                    l.strip_prefix('-').map(|id| Line::Mod { id: id.trim().into(), on: false })
                }
            })
            .filter(|l| match l {
                Line::Mod { id, .. } => !id.is_empty() && seen.insert(id.clone()),
                Line::Separator(_) => true,
            })
            .collect();
        Profile { name: name.into(), lines }
    }

    /// A profile enabling `ids` in order (duplicates and empty ids dropped).
    pub fn from_ids(name: &str, ids: impl IntoIterator<Item = String>) -> Profile {
        let mut seen = HashSet::new();
        let lines = ids.into_iter().filter(|id| !id.is_empty() && seen.insert(id.clone())).map(|id| Line::Mod { id, on: true }).collect();
        Profile { name: name.into(), lines }
    }

    pub fn to_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| match l {
                Line::Mod { id, on } => format!("{}{id}\n", if *on { '+' } else { '-' }),
                Line::Separator(s) => format!("# --- {s}\n"),
            })
            .collect()
    }

    pub fn enabled(&self) -> Vec<String> {
        self.lines.iter().filter_map(|l| if let Line::Mod { id, on: true } = l { Some(id.clone()) } else { None }).collect()
    }

    /// Enable or disable; unknown ids are appended (so newly enabled mods load last).
    pub fn set(&mut self, id: &str, on: bool) {
        match self.lines.iter_mut().find(|l| matches!(l, Line::Mod { id: i, .. } if i == id)) {
            Some(Line::Mod { on: o, .. }) => *o = on,
            _ => self.lines.push(Line::Mod { id: id.into(), on }),
        }
    }

    pub fn forget(&mut self, id: &str) {
        self.lines.retain(|l| !matches!(l, Line::Mod { id: i, .. } if i == id));
    }

    /// Take on the game's current enabled list (it changes when players use the in-game
    /// mod menu). Separators and disabled entries keep their places; enabled slots are
    /// refilled in the game's order. Returns whether anything changed.
    pub fn adopt(&mut self, game: &[String]) -> bool {
        let mut seen = HashSet::new();
        let game: Vec<&String> = game.iter().filter(|i| !i.is_empty() && seen.insert(*i)).collect();
        if self.enabled().iter().eq(game.iter().copied()) {
            return false;
        }
        let known: HashSet<String> = self.lines.iter().filter_map(|l| if let Line::Mod { id, .. } = l { Some(id.clone()) } else { None }).collect();
        let mut order = game.iter().filter(|i| known.contains(**i));
        for line in &mut self.lines {
            if let Line::Mod { id, on } = line {
                *on = game.contains(&&*id);
                if *on {
                    *id = order.next().map(|s| s.to_string()).unwrap_or_default();
                }
            }
        }
        for id in game.iter().filter(|i| !known.contains(**i)) {
            self.lines.push(Line::Mod { id: id.to_string(), on: true });
        }
        true
    }
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    active: String,
    /// Force the B41 default.txt format; otherwise the file's format is kept.
    #[serde(default)]
    b41: Option<bool>,
}

fn dir(paths: &Paths) -> PathBuf {
    paths.data.join("profiles")
}

fn valid_name(name: &str) -> Result<()> {
    if !crate::safe_file_name(name) || name.starts_with('.') {
        bail!("invalid profile name: {name:?}");
    }
    Ok(())
}

/// Whether a profile with this name exists, ignoring case (Windows folders do).
pub fn exists(paths: &Paths, name: &str) -> bool {
    list(paths).iter().any(|n| n.eq_ignore_ascii_case(name))
}

pub fn list(paths: &Paths) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir(paths))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("modlist.txt").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub fn load(paths: &Paths, name: &str) -> Result<Profile> {
    valid_name(name)?;
    let p = dir(paths).join(name).join("modlist.txt");
    if !p.exists() {
        bail!("no profile named {name:?}");
    }
    Ok(Profile::parse(name, &fs::read_to_string(p)?))
}

pub fn save(paths: &Paths, profile: &Profile) -> Result<()> {
    valid_name(&profile.name)?;
    let d = dir(paths).join(&profile.name);
    fs::create_dir_all(&d)?;
    crate::write_atomic(&d.join("modlist.txt"), profile.to_text())?;
    Ok(())
}

pub fn delete(paths: &Paths, name: &str) -> Result<()> {
    valid_name(name)?;
    if active_name(paths) == name {
        bail!("{name:?} is the active profile; switch to another first");
    }
    fs::remove_dir_all(dir(paths).join(name))?;
    Ok(())
}

fn state(paths: &Paths) -> State {
    fs::read_to_string(paths.data.join("state.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_state(paths: &Paths, s: &State) -> Result<()> {
    fs::create_dir_all(&paths.data)?;
    crate::write_atomic(&paths.data.join("state.json"), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

fn active_name(paths: &Paths) -> String {
    Some(state(paths).active).filter(|s| !s.is_empty()).unwrap_or_else(|| "Default".into())
}

/// Forced default.txt format (41 or 42), or None to keep the file's own.
pub fn build(paths: &Paths) -> Option<u8> {
    state(paths).b41.map(|b| if b { 41 } else { 42 })
}

/// The build being played: the forced format, else what default.txt looks like (B42 if unknown).
pub fn effective_build(paths: &Paths) -> u8 {
    build(paths).unwrap_or(if pz::read_default(&paths.default_txt()).b41 { 41 } else { 42 })
}

pub fn set_b41(paths: &Paths, b41: Option<bool>) -> Result<()> {
    save_state(paths, &State { b41, ..state(paths) })
}

/// Load the active profile (creating "Default" from the game's list on first run) and
/// adopt any changes made in-game. Returns the profile and whether it adopted changes.
pub fn sync(paths: &Paths) -> Result<(Profile, bool)> {
    let name = active_name(paths);
    let mut p = load(paths, &name).unwrap_or_else(|_| Profile { name, lines: vec![] });
    // a missing or unreadable default.txt (game running, odd encoding) says nothing about
    // what's enabled: adopting it would switch the whole profile off
    let changed = pz::try_read_default(&paths.default_txt()).is_some_and(|game| p.adopt(&game.mods));
    if changed || !dir(paths).join(&p.name).exists() {
        save(paths, &p)?;
    }
    Ok((p, changed))
}

/// Save `profile`, make it active and write it to the game's mod list.
pub fn apply(paths: &Paths, profile: &Profile) -> Result<()> {
    save(paths, profile)?;
    let s = state(paths);
    pz::write_default(&paths.default_txt(), &profile.enabled(), s.b41)?;
    save_state(paths, &State { active: profile.name.clone(), ..s })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_paths;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn text_round_trip() {
        let text = "# --- Maps\n+A\n-B\n+C\n";
        assert_eq!(Profile::parse("x", text).to_text(), text);
    }

    #[test]
    fn adopt_keeps_separators_and_disabled() {
        let mut p = Profile::parse("x", "# --- Maps\n+A\n-B\n+C\n");
        assert!(!p.adopt(&ids(&["A", "C"])));
        // in-game: C moved before A, B enabled, D added, duplicates ignored
        assert!(p.adopt(&ids(&["C", "A", "B", "D", "A"])));
        assert_eq!(p.to_text(), "# --- Maps\n+C\n+A\n+B\n+D\n");
        assert!(p.adopt(&ids(&["B"])));
        assert_eq!(p.to_text(), "# --- Maps\n-C\n-A\n+B\n-D\n");
    }

    #[test]
    fn unreadable_default_txt_keeps_the_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A", "B"]))).unwrap();
        // the game has it locked / it was deleted / it isn't UTF-8
        fs::write(paths.default_txt(), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), ids(&["A", "B"]));
        fs::remove_file(paths.default_txt()).unwrap();
        assert_eq!(sync(&paths).unwrap().0.enabled(), ids(&["A", "B"]));
    }

    #[test]
    fn duplicates_and_empty_ids_are_dropped() {
        assert_eq!(Profile::parse("x", "+A\n+\n-A\n+B\n").to_text(), "+A\n+B\n");
        assert_eq!(Profile::from_ids("x", ids(&["A", "", "A", "B"])).enabled(), ids(&["A", "B"]));
        let mut p = Profile::parse("x", "+A\n");
        p.adopt(&ids(&["A", "", "A"]));
        assert_eq!(p.to_text(), "+A\n");
    }

    #[test]
    fn sync_and_apply() {
        let (_t, paths) = temp_paths();
        pz::write_default(&paths.default_txt(), &ids(&["A", "B"]), None).unwrap();
        let (mut p, _) = sync(&paths).unwrap();
        assert_eq!((p.name.as_str(), p.enabled()), ("Default", ids(&["A", "B"])));

        p.set("A", false);
        p.set("C", true);
        apply(&paths, &p).unwrap();
        assert_eq!(pz::read_default(&paths.default_txt()).mods, ids(&["B", "C"]));

        let solo = Profile { name: "Solo".into(), lines: vec![Line::Mod { id: "A".into(), on: true }] };
        apply(&paths, &solo).unwrap();
        assert_eq!(sync(&paths).unwrap().0, solo);
        assert_eq!(list(&paths), ids(&["Default", "Solo"]));
        assert!(delete(&paths, "Solo").is_err());
        assert!(load(&paths, "../x").is_err());
    }
}
