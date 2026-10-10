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
    /// Profile name -> kind (e.g. "viewpoint"). Untagged profiles are the default kind.
    #[serde(default)]
    kinds: std::collections::BTreeMap<String, String>,
    /// Kind -> the profile of that kind used last, so switching kinds returns to it.
    #[serde(default)]
    last: std::collections::BTreeMap<String, String>,
}

/// The default profile kind: plain Project Zomboid.
pub const DEFAULT_KIND: &str = "zomboid";

/// A profile's kind ("zomboid" unless tagged otherwise).
pub fn kind_of(paths: &Paths, name: &str) -> String {
    state(paths).kinds.get(name).cloned().unwrap_or_else(|| DEFAULT_KIND.into())
}

pub fn set_kind(paths: &Paths, name: &str, kind: &str) -> Result<()> {
    let mut s = state(paths);
    if kind == DEFAULT_KIND {
        s.kinds.remove(name);
    } else {
        s.kinds.insert(name.into(), kind.into());
    }
    save_state(paths, &s)
}

/// Record the active profile as the last one of its kind (done before switching kinds,
/// so switching back returns to it even if it was last applied before kinds existed).
pub fn remember_active(paths: &Paths) -> Result<()> {
    let mut s = state(paths);
    let name = active_name(paths);
    let kind = s.kinds.get(&name).cloned().unwrap_or_else(|| DEFAULT_KIND.into());
    s.last.insert(kind, name);
    save_state(paths, &s)
}

/// The profile of `kind` used most recently, if it still exists.
pub fn last_of_kind(paths: &Paths, kind: &str) -> Option<String> {
    let s = state(paths);
    let existing = list(paths);
    s.last
        .get(kind)
        .filter(|n| existing.contains(n))
        .cloned()
        .or_else(|| existing.into_iter().find(|n| s.kinds.get(n).map_or(DEFAULT_KIND, String::as_str) == kind))
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
    // lossy: a separator saved as ANSI by Notepad still loads (one odd character) rather than failing
    Ok(Profile::parse(name, &String::from_utf8_lossy(&fs::read(p)?)))
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
    let mut s = state(paths);
    s.kinds.remove(name);
    s.last.retain(|_, n| n != name);
    save_state(paths, &s)
}

fn state(paths: &Paths) -> State {
    let path = paths.data.join("state.json");
    let Ok(text) = fs::read_to_string(&path) else { return State::default() };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        // keep the broken file for inspection instead of silently overwriting it on the next save
        let _ = fs::copy(&path, paths.data.join("state.json.bad"));
        crate::log!("error: {} is damaged ({e}); kept a copy as state.json.bad and started fresh", path.display());
        State::default()
    })
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
    // a profile that exists but can't be read is an error: starting empty would replace it with the game's list
    let mut p = if dir(paths).join(&name).join("modlist.txt").exists() { load(paths, &name)? } else { Profile { name, lines: vec![] } };
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
    let mut s = state(paths);
    pz::write_default(&paths.default_txt(), &profile.enabled(), s.b41)?;
    let kind = s.kinds.get(&profile.name).cloned().unwrap_or_else(|| DEFAULT_KIND.into());
    s.last.insert(kind, profile.name.clone());
    save_state(paths, &State { active: profile.name.clone(), ..s })
}

/// A profile to share with friends or a server: its list plus where each mod comes
/// from on the Workshop, so the receiver can install what they're missing.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Shared {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    /// The profile in modlist.txt form (`+Id`, `-Id`, `# --- Separator`).
    pub list: String,
    /// Mod id -> Workshop item id, for mods that come from the Workshop.
    #[serde(default)]
    pub workshop: std::collections::BTreeMap<String, String>,
}

/// Package the active profile for sharing (`mods`: the current scan, for Workshop ids).
pub fn export(paths: &Paths, mods: &std::collections::BTreeMap<String, crate::store::Row>) -> Result<Shared> {
    let p = sync(paths)?.0;
    let workshop = p
        .lines
        .iter()
        .filter_map(|l| if let Line::Mod { id, .. } = l { Some(id) } else { None })
        .filter_map(|id| {
            let src = &mods.get(id)?.source;
            let item = src.strip_prefix("steam:").or_else(|| src.strip_prefix("workshop:"))?;
            Some((id.clone(), item.to_string()))
        })
        .collect();
    Ok(Shared { kind: kind_of(paths, &p.name), list: p.to_text(), name: p.name, workshop })
}

/// Workshop items a shared profile needs that aren't installed (`mods`: the current scan).
pub fn import_needs(shared: &Shared, mods: &std::collections::BTreeMap<String, crate::store::Row>) -> Vec<crate::sources::Source> {
    let mut items: Vec<&String> = shared
        .workshop
        .iter()
        .filter(|(id, item)| !mods.contains_key(*id) && crate::sources::steam_workshop::valid_id(item))
        .map(|(_, item)| item)
        .collect();
    items.sort();
    items.dedup();
    items.into_iter().map(|i| crate::sources::Source::Workshop(i.clone())).collect()
}

/// Save a shared profile under a free name (`<name>`, `<name> 2`...), keep its kind, and
/// switch to it. Returns the profile.
pub fn import(paths: &Paths, shared: &Shared) -> Result<Profile> {
    let base = if crate::safe_file_name(shared.name.trim()) { shared.name.trim().to_string() } else { "Shared".to_string() };
    let name = if exists(paths, &base) { (2..).map(|i| format!("{base} {i}")).find(|n| !exists(paths, n)).unwrap() } else { base };
    let p = Profile::parse(&name, &shared.list);
    if !shared.kind.is_empty() && crate::safe_file_name(&shared.kind) {
        set_kind(paths, &name, &shared.kind)?;
    }
    apply(paths, &p)?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_round_trip() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Friends", "# --- Core\n+A\n-B\n")).unwrap();
        let shared = export(&paths, &Default::default()).unwrap();
        assert_eq!(shared.list, "# --- Core\n+A\n-B\n");
        let back: Shared = serde_json::from_str(&serde_json::to_string(&shared).unwrap()).unwrap();
        let p = import(&paths, &back).unwrap();
        assert_eq!((p.name.as_str(), p.to_text().as_str()), ("Friends 2", "# --- Core\n+A\n-B\n"));
        let needs = import_needs(
            &Shared { workshop: [("X".into(), "123".into()), ("Y".into(), "+bad".into())].into(), ..back },
            &Default::default(),
        );
        assert_eq!(needs, [crate::sources::Source::Workshop("123".into())]);
    }
    use crate::store::tests::temp_paths;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn unreadable_profile_is_not_replaced() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::parse("Default", "# --- Maps\n+A\n-B\n")).unwrap();
        // Notepad re-saved it as ANSI: the separator is no longer valid UTF-8
        let file = dir(&paths).join("Default").join("modlist.txt");
        fs::write(&file, b"# --- V\xe9g\n+A\n-B\n").unwrap();
        let (p, _) = sync(&paths).unwrap();
        assert_eq!(p.lines.len(), 3, "separator and disabled entry kept: {:?}", p.lines);
        assert!(matches!(&p.lines[2], Line::Mod { id, on: false } if id == "B"));
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
    fn kinds_remember_their_last_profile() {
        let (_t, paths) = temp_paths();
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        set_kind(&paths, "3D", "viewpoint").unwrap();
        apply(&paths, &Profile::from_ids("3D", ids(&["Viewpoint"]))).unwrap();
        assert_eq!(kind_of(&paths, "3D"), "viewpoint");
        assert_eq!(kind_of(&paths, "Default"), DEFAULT_KIND);
        assert_eq!(last_of_kind(&paths, DEFAULT_KIND).as_deref(), Some("Default"));
        assert_eq!(last_of_kind(&paths, "viewpoint").as_deref(), Some("3D"));
        apply(&paths, &Profile::from_ids("Default", ids(&["A"]))).unwrap();
        delete(&paths, "3D").unwrap();
        assert_eq!(last_of_kind(&paths, "viewpoint"), None);
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
