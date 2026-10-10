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
    pub fn parse(name: &str, text: &str) -> Profile {
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
            .collect();
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
        let game: Vec<&String> = game.iter().filter(|i| seen.insert(*i)).collect();
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
    if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || name.starts_with('.') {
        bail!("invalid profile name: {name:?}");
    }
    Ok(())
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
    fs::write(d.join("modlist.txt"), profile.to_text())?;
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
    fs::write(paths.data.join("state.json"), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

fn active_name(paths: &Paths) -> String {
    Some(state(paths).active).filter(|s| !s.is_empty()).unwrap_or_else(|| "Default".into())
}

pub fn set_b41(paths: &Paths, b41: Option<bool>) -> Result<()> {
    save_state(paths, &State { b41, ..state(paths) })
}

/// Load the active profile (creating "Default" from the game's list on first run) and
/// adopt any changes made in-game. Returns the profile and whether it adopted changes.
pub fn sync(paths: &Paths) -> Result<(Profile, bool)> {
    let name = active_name(paths);
    let game = pz::read_default(&paths.default_txt()).mods;
    let mut p = load(paths, &name).unwrap_or_else(|_| Profile { name, lines: vec![] });
    let changed = p.adopt(&game);
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
