//! What a server's owner keeps about its players (`<data>/server-players/<server>.json`): a note
//! and tags per username, and who joined and left when (from the log lines `host::player_event`
//! reads while PZMM runs the server).

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::Paths;

/// The most joins and leaves kept per server.
const KEEP: usize = 2000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dossier {
    /// By username.
    pub players: BTreeMap<String, Player>,
    /// Oldest first.
    pub history: Vec<Seen>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Player {
    pub note: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    /// Unix seconds.
    pub time: u64,
    pub name: String,
    /// Joined (true) or left.
    pub joined: bool,
    /// "ban", "unban" or "kick" when PZMM sent that command (`joined` is false then); empty for a join or leave.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event: String,
}

fn file(paths: &Paths, server: &str) -> Result<PathBuf> {
    if !crate::safe_file_name(server) {
        bail!("invalid server name: {server:?}");
    }
    Ok(paths.data.join("server-players").join(format!("{server}.json")))
}

pub fn load(paths: &Paths, server: &str) -> Dossier {
    file(paths, server).and_then(|f| crate::read_json(&f)).unwrap_or_default()
}

fn update(paths: &Paths, server: &str, f: impl FnOnce(&mut Dossier)) -> Result<Dossier> {
    let to = file(paths, server)?;
    let _lock = crate::lock_data(paths)?;
    // one that can't be read is never written over
    let mut d: Dossier = crate::read_json(&to)?;
    f(&mut d);
    std::fs::create_dir_all(to.parent().unwrap_or(&paths.data))?;
    crate::write_atomic(&to, serde_json::to_string_pretty(&d)?)?;
    Ok(d)
}

/// Set a player's note and tags (trimmed, no blanks or repeats); both empty forgets them.
pub fn set_player(paths: &Paths, server: &str, name: &str, note: &str, tags: &[String]) -> Result<Dossier> {
    let mut clean: Vec<String> = vec![];
    for t in tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !clean.iter().any(|c| c.eq_ignore_ascii_case(t)) {
            clean.push(t.to_string());
        }
    }
    let p = Player { note: note.trim().to_string(), tags: clean };
    update(paths, server, |d| {
        if p == Player::default() {
            d.players.remove(name);
        } else {
            d.players.insert(name.to_string(), p);
        }
    })
}

/// Note joins (true) and leaves at `time`, keeping the newest [`KEEP`].
pub fn record(paths: &Paths, server: &str, time: u64, events: &[(bool, String)]) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    update(paths, server, |d| {
        d.history.extend(events.iter().map(|(joined, name)| Seen { time, name: name.clone(), joined: *joined, event: String::new() }));
        let extra = d.history.len().saturating_sub(KEEP);
        d.history.drain(..extra);
    })?;
    Ok(())
}

/// Note the ban, unban or kick a console command PZMM sent makes (`banuser "name"`...), at `time`.
pub fn note_command(paths: &Paths, server: &str, time: u64, cmd: &str) -> Result<()> {
    let Some((verb, rest)) = cmd.trim().split_once(' ') else { return Ok(()) };
    let event = match verb.to_ascii_lowercase().as_str() {
        "banuser" => "ban",
        "unbanuser" => "unban",
        "kickuser" => "kick",
        _ => return Ok(()),
    };
    let rest = rest.trim_start();
    let name = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or_default(),
        None => rest.split(' ').next().unwrap_or_default(),
    };
    if name.is_empty() {
        return Ok(());
    }
    update(paths, server, |d| {
        d.history.push(Seen { time, name: name.into(), joined: false, event: event.into() });
        let extra = d.history.len().saturating_sub(KEEP);
        d.history.drain(..extra);
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_tags_and_history_are_kept() {
        let (_t, paths) = crate::store::tests::temp_paths();
        assert_eq!(load(&paths, "Friends"), Dossier::default());
        let d = set_player(&paths, "Friends", "Kate", " Builds the walls ", &[" helper".into(), "Helper".into(), "".into(), "mod".into()]).unwrap();
        assert_eq!(d.players["Kate"], Player { note: "Builds the walls".into(), tags: vec!["helper".into(), "mod".into()] });
        record(&paths, "Friends", 10, &[(true, "Kate".into()), (true, "Bob".into())]).unwrap();
        record(&paths, "Friends", 20, &[(false, "Kate".into())]).unwrap();
        let d = load(&paths, "Friends");
        assert_eq!(d.history.iter().map(|s| (s.time, s.name.as_str(), s.joined)).collect::<Vec<_>>(), [(10, "Kate", true), (10, "Bob", true), (20, "Kate", false)]);
        assert_eq!(d.players.len(), 1, "history leaves notes alone");
        assert!(set_player(&paths, "Friends", "Kate", "", &[]).unwrap().players.is_empty(), "empty forgets");
        assert_eq!(load(&paths, "Other"), Dossier::default(), "per server");
        assert!(set_player(&paths, r"..\..\outside", "Kate", "x", &[]).is_err(), "a name never leaves the folder");
        let many: Vec<(bool, String)> = (0..KEEP + 5).map(|i| (true, format!("P{i}"))).collect();
        record(&paths, "Friends", 30, &many).unwrap();
        let d = load(&paths, "Friends");
        assert_eq!((d.history.len(), d.history[0].name.as_str()), (KEEP, "P5"), "the oldest go");
    }

    #[test]
    fn bans_unbans_and_kicks_are_noted() {
        let (_t, paths) = crate::store::tests::temp_paths();
        for cmd in [r#"banuser "Kate Smith" -ip"#, "UNBANUSER Bob", r#"kickuser "Al" -r "spam""#, "players", "servermsg \"banuser X\"", "banuser \"\""] {
            note_command(&paths, "Friends", 7, cmd).unwrap();
        }
        let d = load(&paths, "Friends");
        let got: Vec<(&str, &str, bool)> = d.history.iter().map(|s| (s.name.as_str(), s.event.as_str(), s.joined)).collect();
        assert_eq!(got, [("Kate Smith", "ban", false), ("Bob", "unban", false), ("Al", "kick", false)]);
        let json = serde_json::to_string(&d.history[0]).unwrap();
        assert!(json.contains(r#""event":"ban""#));
        record(&paths, "Friends", 8, &[(true, "Kate Smith".into())]).unwrap();
        assert!(!serde_json::to_string(&load(&paths, "Friends").history[3]).unwrap().contains("event"), "joins carry no event");
    }
}
