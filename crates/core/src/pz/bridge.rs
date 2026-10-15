//! The game's side of the conversation with PZMM - Client: what the game asks PZMM to do
//! (`Zomboid/Lua/pzmm/requests.txt`, a line per request) and what it last reported (`game.json`).
//! Game Lua can only write under `Zomboid/Lua`, and any mod can write there, so every request is
//! checked here and the app asks you before it acts on one that changes anything. While PZMM runs it
//! takes them off the game within a second and comes to the front; when it doesn't, the game opens a
//! pzmm://game link (through a web page: the game opens only https) that starts it.

use serde::{Deserialize, Serialize};

use super::Paths;

/// One request from the game.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum Request {
    /// Install these Workshop items.
    Install { ids: Vec<String> },
    /// Back up a save (`mode/name`, as the game names its save folders).
    Backup { mode: String, name: String },
    /// Run Check setup.
    Check,
    /// Show one of the app's tabs.
    Open { tab: String },
}

/// The tabs a request may open (the app's `tab` values).
const TABS: [&str; 10] = ["home", "mods", "feed", "workshop", "maps", "servers", "saves", "zb", "authoring", "settings"];
/// At most this many requests are taken at once; the rest of a runaway file is dropped.
const MAX: usize = 20;

/// The watcher and the UI both move requests: one at a time.
static QUEUE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Move the game's new requests into the app's queue (`requests.queue`), at once while PZMM runs: the
/// game sees `requests.txt` gone, so it knows PZMM is up and doesn't start it. Returns whether there were any.
pub fn queue_requests(paths: &Paths) -> bool {
    let _one = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = paths.zomboid.join("Lua").join("pzmm");
    // moved aside first, then read: a line the game appends meanwhile starts a new file, not a lost one
    let taken = dir.join("requests.taken");
    if std::fs::rename(dir.join("requests.txt"), &taken).is_err() {
        return false;
    }
    let text = std::fs::read_to_string(&taken).unwrap_or_default();
    let _ = std::fs::remove_file(&taken);
    use std::io::Write;
    let queued = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("requests.queue")).and_then(|mut f| f.write_all(text.as_bytes()));
    queued.is_ok()
}

/// The game's requests, oldest first, taken: the files are removed so each is handled once.
pub fn take_requests(paths: &Paths) -> Vec<Request> {
    queue_requests(paths);
    let _one = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let queue = paths.zomboid.join("Lua").join("pzmm").join("requests.queue");
    let text = std::fs::read_to_string(&queue).unwrap_or_default();
    let _ = std::fs::remove_file(&queue);
    parse(&text)
}

fn parse(text: &str) -> Vec<Request> {
    let mut out: Vec<Request> = vec![];
    for line in text.lines() {
        let mut parts = line.trim_end_matches('\r').splitn(2, '\t');
        let (action, arg) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").trim());
        let req = match action {
            "install" => {
                let ids: Vec<String> = arg.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
                // Workshop IDs are digits; anything else in the list spoils the request
                (!ids.is_empty() && ids.len() <= 200 && ids.iter().all(|i| i.len() <= 20 && i.bytes().all(|b| b.is_ascii_digit()))).then_some(Request::Install { ids })
            }
            "backup" => arg.split_once('/').filter(|(m, n)| plain(m) && plain(n)).map(|(m, n)| Request::Backup { mode: m.into(), name: n.into() }),
            "check" => Some(Request::Check),
            "open" => TABS.contains(&arg).then(|| Request::Open { tab: arg.into() }),
            _ => None,
        };
        // the same request twice in a row is one
        if let Some(r) = req
            && out.last() != Some(&r)
        {
            out.push(r);
        }
        if out.len() == MAX {
            break;
        }
    }
    out
}

/// A save folder's part: a name, not a path.
fn plain(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && s != "." && s != ".." && !s.contains(['/', '\\', ':'])
}

/// What the game last reported (`game.json`, written at the main menu and when a game starts).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GameReport {
    #[serde(default)]
    pub client: String,
    #[serde(default)]
    pub game: String,
    /// "menu" or "game".
    #[serde(default)]
    pub state: String,
    /// When, in seconds since 1970.
    #[serde(default)]
    pub at: u64,
    /// The mods the game loaded, in its order.
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub save: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

/// The game's last report, if PZMM - Client has written one.
pub fn game_report(paths: &Paths) -> Option<GameReport> {
    let text = std::fs::read_to_string(paths.zomboid.join("Lua").join("pzmm").join("game.json")).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_checked_and_trimmed() {
        let text = "install\t123,456\r\ninstall\t12a\nbackup\tSurvivor/2026-10-14_09-28-10\nbackup\tSurvivor/../x\nbackup\tnoslash\ncheck\ncheck\nopen\tsaves\nopen\tC:/evil\nwipe\tall\n";
        assert_eq!(
            parse(text),
            vec![
                Request::Install { ids: vec!["123".into(), "456".into()] },
                Request::Backup { mode: "Survivor".into(), name: "2026-10-14_09-28-10".into() },
                Request::Check,
                Request::Open { tab: "saves".into() },
            ]
        );
    }

    #[test]
    fn requests_are_taken_once() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("requests.txt"), "check\n").unwrap();
        assert_eq!(take_requests(&paths), vec![Request::Check]);
        assert_eq!(take_requests(&paths), vec![]);
        assert!(!dir.join("requests.taken").exists());
    }

    #[test]
    fn queued_requests_wait_for_the_ui() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Lua/pzmm");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!queue_requests(&paths), "nothing asked");
        std::fs::write(dir.join("requests.txt"), "open\tsaves\n").unwrap();
        assert!(queue_requests(&paths));
        assert!(!dir.join("requests.txt").exists(), "the game sees PZMM took it");
        std::fs::write(dir.join("requests.txt"), "check\n").unwrap();
        assert_eq!(take_requests(&paths), vec![Request::Open { tab: "saves".into() }, Request::Check]);
        assert!(!dir.join("requests.queue").exists());
    }

    #[test]
    fn a_runaway_file_is_cut_short() {
        let text = (0..100).map(|i| format!("install\t{i}\n")).collect::<String>();
        assert_eq!(parse(&text).len(), MAX);
    }

    #[test]
    fn reads_what_the_client_mod_writes() {
        let r: GameReport = serde_json::from_str(r#"{"v":1,"client":"0.2.0","game":"42.21.0","state":"menu","at":1792000000,"mods":["A","B"]}"#).unwrap();
        assert_eq!((r.state.as_str(), r.mods.len(), r.save), ("menu", 2, None));
    }
}
