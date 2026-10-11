//! Project Viewpoint (Workshop 3809306528): a 3D first/third-person renderer for Build 42
//! that loads through ZombieBuddy. PZMM keeps "viewpoint" profiles apart from plain
//! Project Zomboid ones, so players can flip between the 2D and 3D game.
//!
//! Switching to Viewpoint is three steps so callers can download without holding locks:
//! [`needed`] (what to download), then installing those, then [`enter`].

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use super::{Paths, zombiebuddy};
use crate::profile::{self, Line, Profile};
use crate::sources::{self, Source};
use crate::store::Row;

/// The profile kind for Viewpoint mod lists.
pub const KIND: &str = "viewpoint";
pub const WORKSHOP_ID: &str = "3809306528";
pub const MOD_ID: &str = "Viewpoint";
/// What Viewpoint needs from the Workshop.
pub const WORKSHOP: [&str; 2] = [zombiebuddy::WORKSHOP_ID, WORKSHOP_ID];
/// Mods enabled at the top of every Viewpoint profile, in load order.
pub const MODS: [&str; 2] = [zombiebuddy::MOD_ID, MOD_ID];
/// Name of the profile made on the first switch.
pub const PROFILE: &str = "Viewpoint";
/// Name of the plain profile made when leaving Viewpoint with no plain profile to return to.
pub const PLAIN_PROFILE: &str = "Project Zomboid";

/// `base` with ZombieBuddy and Viewpoint enabled first; everything else keeps its place.
pub fn with_viewpoint(base: &Profile, name: &str) -> Profile {
    let mut lines: Vec<Line> = MODS.iter().map(|id| Line::Mod { id: id.to_string(), on: true }).collect();
    lines.extend(base.lines.iter().filter(|l| !matches!(l, Line::Mod { id, .. } if MODS.contains(&id.as_str()))).cloned());
    let order = MODS.iter().map(|id| id.to_string()).chain(base.enabled()).collect();
    Profile { name: name.into(), lines, order }
}

/// `base` without Viewpoint (ZombieBuddy stays: other Java mods may need it).
pub fn without_viewpoint(base: &Profile, name: &str) -> Profile {
    let lines = base.lines.iter().filter(|l| !matches!(l, Line::Mod { id, .. } if id == MOD_ID)).cloned().collect();
    Profile { name: name.into(), lines, order: base.enabled() }
}

/// `base`, or `base 2`, `base 3`... whichever profile name is free.
pub fn unique_profile_name(paths: &Paths, base: &str) -> String {
    if !profile::exists(paths, base) {
        return base.into();
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| !profile::exists(paths, n)).unwrap()
}

/// Workshop downloads Viewpoint still needs (items neither installed nor subscribed).
/// Fails on Build 41, which Viewpoint doesn't support.
pub fn needed(paths: &Paths, mods: &BTreeMap<String, Row>) -> Result<Vec<Source>> {
    if profile::effective_build(paths) == 41 {
        bail!("Project Viewpoint needs Build 42 (change it in Settings > Game build)");
    }
    let have = sources::workshop_available(paths);
    Ok(WORKSHOP
        .into_iter()
        .zip(MODS)
        .filter(|(item, id)| !mods.contains_key(*id) && !have.contains(*item))
        .map(|(item, _)| Source::Workshop(item.into()))
        .collect())
}

/// What switching to `kind` would change, for a confirmation step.
pub fn preview(paths: &Paths, kind: &str, mods: &BTreeMap<String, Row>) -> Result<Vec<String>> {
    let current = profile::sync(paths)?.0.name;
    if kind != KIND {
        return Ok(vec![match profile::last_of_kind(paths, profile::DEFAULT_KIND) {
            Some(n) => format!("switch to your \"{n}\" profile"),
            None => format!("make a \"{PLAIN_PROFILE}\" profile from \"{current}\" without Viewpoint, and switch to it"),
        }]);
    }
    let mut lines: Vec<String> = needed(paths, mods)?
        .into_iter()
        .filter_map(|s| if let Source::Workshop(item) = s { Some(item) } else { None })
        .map(|item| {
            let id = MODS[WORKSHOP.iter().position(|w| *w == item).unwrap_or(0)];
            format!("download {id} from the Steam Workshop (item {item})")
        })
        .collect();
    let (launcher, libs) = zombiebuddy::locate(mods);
    let Some(l) = launcher else { bail!("Project Zomboid's game folder wasn't found in your Steam libraries (set PZ_GAME_DIR)") };
    match libs {
        Some(s) => lines.extend(zombiebuddy::install(&l, &s, true)?),
        None => {
            lines.push(format!("copy ZombieBuddy's Java agent files to {}", l.display()));
            lines.push(format!("add {} to {}", zombiebuddy::launch_arg(), l.join("ProjectZomboid64.json").display()));
        }
    }
    lines.push(match profile::last_of_kind(paths, KIND) {
        Some(n) => format!("switch to your \"{n}\" profile, with ZombieBuddy and Viewpoint enabled at the top"),
        None => format!(
            "make a \"{}\" profile from \"{current}\" with ZombieBuddy and Viewpoint enabled at the top, and switch to it",
            unique_profile_name(paths, PROFILE)
        ),
    });
    Ok(lines)
}

/// After [`needed`] items are installed: set up the ZombieBuddy agent and switch to the
/// Viewpoint profile (made from the current one the first time). Returns that profile.
pub fn enter(paths: &Paths, mods: &BTreeMap<String, Row>) -> Result<Profile> {
    if let Some(id) = MODS.iter().find(|id| !mods.contains_key(**id)) {
        bail!("{id} isn't installed; the download may have failed (see the log)");
    }
    let (launcher, libs) = zombiebuddy::locate(mods);
    let Some(l) = launcher else { bail!("Project Zomboid's game folder wasn't found in your Steam libraries (set PZ_GAME_DIR)") };
    let Some(s) = libs else { bail!("ZombieBuddy's Java agent files weren't found in its Workshop item") };
    for change in zombiebuddy::install(&l, &s, false)? {
        crate::log!("{change}");
    }
    profile::remember_active(paths)?;
    let p = match profile::last_of_kind(paths, KIND) {
        Some(n) => with_viewpoint(&profile::load(paths, &n)?, &n),
        None => with_viewpoint(&profile::sync(paths)?.0, &unique_profile_name(paths, PROFILE)),
    };
    profile::set_kind(paths, &p.name, KIND)?;
    profile::apply(paths, &p, Some(mods))?;
    crate::say!(
        "switchedViewpoint",
        "Switched to Project Viewpoint (profile \"{name}\"). The first time the game starts, ZombieBuddy asks you to approve Viewpoint's Java code.",
        name = p.name
    );
    Ok(p)
}

/// Back to plain Project Zomboid: the last plain profile, without Viewpoint enabled.
pub fn leave(paths: &Paths) -> Result<Profile> {
    profile::remember_active(paths)?;
    let p = match profile::last_of_kind(paths, profile::DEFAULT_KIND) {
        Some(n) => without_viewpoint(&profile::load(paths, &n)?, &n),
        None => without_viewpoint(&profile::sync(paths)?.0, &unique_profile_name(paths, PLAIN_PROFILE)),
    };
    profile::set_kind(paths, &p.name, profile::DEFAULT_KIND)?;
    profile::apply(paths, &p, profile::maps_scan(crate::store::scan(paths)).as_ref())?;
    crate::say!("switchedZomboid", "Switched to Project Zomboid (profile \"{name}\").", name = p.name);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_paths;

    #[test]
    fn viewpoint_mods_go_first() {
        let base = Profile::parse("Default", "# --- Maps\n+A\n-Viewpoint\n+ZombieBuddy\n");
        let vp = with_viewpoint(&base, "Viewpoint");
        assert_eq!(vp.to_text(), "+ZombieBuddy\n+Viewpoint\n# --- Maps\n+A\n");
        assert_eq!(vp.enabled(), ["ZombieBuddy", "Viewpoint", "A"]);
        let plain = without_viewpoint(&vp, "Plain");
        assert_eq!((plain.to_text().as_str(), plain.enabled()), ("+ZombieBuddy\n# --- Maps\n+A\n", vec!["ZombieBuddy".to_string(), "A".into()]));
    }

    #[test]
    fn leaving_returns_to_the_last_plain_profile() {
        let (_t, paths) = temp_paths();
        profile::apply(&paths, &Profile::from_ids("Default", ["A".to_string()]), None).unwrap();
        let vp = with_viewpoint(&Profile::from_ids("x", ["A".to_string()]), PROFILE);
        profile::set_kind(&paths, PROFILE, KIND).unwrap();
        profile::apply(&paths, &vp, None).unwrap();
        assert_eq!(leave(&paths).unwrap().name, "Default");
        assert_eq!(profile::kind_of(&paths, PROFILE), KIND);
        assert_eq!(unique_profile_name(&paths, "Default"), "Default 2");
    }
}
