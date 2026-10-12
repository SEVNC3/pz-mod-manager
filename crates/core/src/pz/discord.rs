//! A server's Discord webhook: its URL lives in the OS credential store (never in servers.json);
//! events are posted as embeds, and a status message is edited in place.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::host::Status;

/// What servers.json keeps (no secrets).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// A webhook is in the credential store.
    pub saved: bool,
    /// The events posted: started, stopped, crashed, restarting, updated, joined, left, status.
    pub events: Vec<String>,
    /// The status message being edited.
    pub message_id: String,
    /// Bot mode (see `discord_bot`): a bot token is in the credential store; the channel it reads
    /// commands in and the role allowed to use them (Discord IDs).
    pub bot: bool,
    pub channel: String,
    pub role: String,
    /// The bot also relays general chat both ways between the game and the channel.
    pub relay: bool,
}

/// A Discord webhook URL: `https://discord.com/api/webhooks/<id>/<token>` (or discordapp.com, ptb., canary.).
fn valid_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.split('?').next().unwrap_or("");
    let hosts = ["discord.com", "discordapp.com", "ptb.discord.com", "canary.discord.com"];
    let parts: Vec<&str> = path.split('/').collect();
    hosts.contains(&host)
        && parts.len() == 4
        && parts[..2] == ["api", "webhooks"]
        && parts[2].bytes().all(|b| b.is_ascii_digit())
        && !parts[2].is_empty()
        && !parts[3].is_empty()
        && parts[3].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn secret(name: &str) -> String {
    format!("server-discord:{name}")
}

/// Keep the webhook (empty: forget it).
pub fn save(name: &str, url: &str) -> Result<()> {
    let url = url.trim();
    if !url.is_empty() && !valid_url(url) {
        bail!("that isn't a Discord webhook link (https://discord.com/api/webhooks/…)");
    }
    crate::settings::set_secret(&secret(name), url).context("the webhook")
}

/// The saved webhook. Unit tests never read the credential store.
pub fn url(name: &str) -> Option<String> {
    crate::settings::secret(&secret(name))
}

const RED: u32 = 0x8a0303;
const GREEN: u32 = 0x2e7d32;
const GREY: u32 = 0x607d8b;
const AMBER: u32 = 0xc77700;

fn payload(embed: Value) -> Value {
    // player names never ping anyone
    json!({ "username": "PZMM", "allowed_mentions": { "parse": [] }, "embeds": [embed] })
}

/// An event's message; `lines` (a crash's last log lines) go in a code block, cut to fit Discord's 4096.
pub fn event(server: &str, event: &str, text: &str, lines: Option<&[String]>) -> Value {
    let color = match event {
        "crashed" => RED,
        "started" | "joined" => GREEN,
        "restarting" | "updated" => AMBER,
        _ => GREY,
    };
    let mut description = text.to_string();
    if let Some(lines) = lines.filter(|l| !l.is_empty()) {
        let mut log = lines.join("\n").replace("```", "'''");
        let room = 4000 - description.len().min(3900);
        if log.len() > room {
            let cut = log.len() - room;
            let at = (cut..log.len()).find(|i| log.is_char_boundary(*i)).unwrap_or(log.len());
            log = log[at..].to_string();
        }
        description += &format!("\n```\n{log}\n```");
    }
    payload(json!({ "title": server, "description": description, "color": color }))
}

/// The status message: up or down, uptime, who's on, the address players join at ("": none set).
pub fn status(server: &str, status: Status, players: &[String], uptime_secs: u64, address: &str) -> Value {
    let (state, color) = match status {
        Status::Running => ("Online", GREEN),
        Status::Starting => ("Starting", AMBER),
        Status::Stopping => ("Stopping", AMBER),
        Status::Crashed => ("Crashed", RED),
        Status::Stopped => ("Offline", GREY),
    };
    let mut fields = vec![json!({ "name": "Status", "value": state, "inline": true })];
    if status == Status::Running {
        let (h, m) = (uptime_secs / 3600, uptime_secs / 60 % 60);
        fields.push(json!({ "name": "Uptime", "value": format!("{h} h {m} min"), "inline": true }));
        if !address.is_empty() {
            fields.push(json!({ "name": "Join at", "value": address, "inline": true }));
        }
        let mut who = players.join(", ");
        if who.len() > 1000 {
            who = format!("{}…", &who[..who.char_indices().take_while(|(i, _)| *i < 1000).last().map_or(0, |(i, _)| i)]);
        }
        fields.push(json!({ "name": format!("Players ({})", players.len()), "value": if who.is_empty() { "Nobody online".into() } else { who } }));
    }
    payload(json!({ "title": server, "color": color, "fields": fields }))
}

/// `url` with `extra` added to its query (a webhook link may carry `?thread_id=`).
fn with_query(base: &str, query: Option<&str>, extra: Option<&str>) -> String {
    let q: Vec<&str> = query.into_iter().chain(extra).filter(|q| !q.is_empty()).collect();
    if q.is_empty() { base.to_string() } else { format!("{base}?{}", q.join("&")) }
}

/// Post a message; returns its id.
pub fn post(url: &str, body: &Value) -> Result<String> {
    let (base, query) = url.split_once('?').map_or((url, None), |(b, q)| (b, Some(q)));
    let mut res = crate::sources::agent()
        .post(&with_query(base, query, Some("wait=true")))
        .header("Content-Type", "application/json")
        .header("User-Agent", crate::USER_AGENT)
        .send(body.to_string())?;
    let v: Value = serde_json::from_str(&res.body_mut().read_to_string()?)?;
    Ok(v["id"].as_str().unwrap_or_default().to_string())
}

/// Edit message `id`; false when it's gone (deleted in Discord).
pub fn edit(url: &str, id: &str, body: &Value) -> Result<bool> {
    let (base, query) = url.split_once('?').map_or((url, None), |(b, q)| (b, Some(q)));
    let r = crate::sources::agent()
        .patch(&with_query(&format!("{base}/messages/{id}"), query, None))
        .header("Content-Type", "application/json")
        .header("User-Agent", crate::USER_AGENT)
        .send(body.to_string());
    match r {
        Ok(_) => Ok(true),
        Err(ureq::Error::StatusCode(404)) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Post a test message to the server's saved webhook.
pub fn test(server: &str) -> Result<()> {
    let Some(url) = url(server) else { bail!("save a webhook for {server} first") };
    post(&url, &event(server, "test", "PZMM can post here.", None)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};

    #[test]
    fn webhook_links() {
        assert!(valid_url("https://discord.com/api/webhooks/123456/abc-DEF_9"));
        assert!(valid_url("https://canary.discord.com/api/webhooks/1/x?thread_id=5"));
        for bad in ["http://discord.com/api/webhooks/1/x", "https://discord.com.evil.io/api/webhooks/1/x", "https://discord.com/api/webhooks/x/y", "https://discord.com/api/webhooks/1", "https://example.com/api/webhooks/1/x", ""] {
            assert!(!valid_url(bad), "{bad}");
        }
        assert_eq!(url("Friends"), None, "tests never read the credential store");
    }

    #[test]
    fn payloads() {
        let e = event("Friends", "crashed", "The server crashed.", Some(&["line 1".into(), "Server Terminated.".into()]));
        assert_eq!(e["allowed_mentions"]["parse"], json!([]));
        assert_eq!(e["embeds"][0]["title"], "Friends");
        assert_eq!(e["embeds"][0]["color"], RED);
        assert_eq!(e["embeds"][0]["description"], "The server crashed.\n```\nline 1\nServer Terminated.\n```");
        // a long log keeps its end and fits
        let long: Vec<String> = (0..400).map(|i| format!("ERROR: line {i} ```")).collect();
        let d = event("S", "crashed", "x", Some(&long))["embeds"][0]["description"].as_str().unwrap().to_string();
        assert!(d.chars().count() <= 4096 && d.contains("line 399") && !d.contains("line 0 ") && d.matches("```").count() == 2, "{}", d.len());

        let s = status("Friends", Status::Running, &["Alice".into(), "Bob".into()], 3720, "");
        let fields = &s["embeds"][0]["fields"];
        assert_eq!(fields[0]["value"], "Online");
        assert_eq!(fields[1]["value"], "1 h 2 min");
        assert_eq!((fields[2]["name"].as_str(), fields[2]["value"].as_str()), (Some("Players (2)"), Some("Alice, Bob")));
        let off = status("Friends", Status::Stopped, &[], 0, "play.example.net:16261");
        assert_eq!(off["embeds"][0]["fields"].as_array().unwrap().len(), 1);
        let on = status("Friends", Status::Running, &[], 60, "play.example.net:16261");
        assert_eq!(on["embeds"][0]["fields"][2]["value"], "play.example.net:16261");
        assert_eq!(with_query("https://d/api/webhooks/1/x", Some("thread_id=5"), Some("wait=true")), "https://d/api/webhooks/1/x?thread_id=5&wait=true");
    }

    /// One HTTP exchange on a local port: returns the request line and body; answers `reply`.
    fn listener(reply: &'static str, status: &'static str) -> (String, std::thread::JoinHandle<(String, String)>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/api/webhooks/1/tok", l.local_addr().unwrap().port());
        let h = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let (mut first, mut len) = (String::new(), 0);
            r.read_line(&mut first).unwrap();
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let mut s = s;
            write!(s, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).unwrap();
            (first.trim().to_string(), String::from_utf8(body).unwrap())
        });
        (url, h)
    }

    #[test]
    fn posts_and_edits_on_a_local_listener() {
        let (url, h) = listener(r#"{"id":"777"}"#, "200 OK");
        let body = event("Friends", "started", "The server is up.", None);
        assert_eq!(post(&url, &body).unwrap(), "777");
        let (line, got) = h.join().unwrap();
        assert_eq!(line, "POST /api/webhooks/1/tok?wait=true HTTP/1.1");
        assert_eq!(serde_json::from_str::<Value>(&got).unwrap(), body);

        let (url, h) = listener("{}", "200 OK");
        assert!(edit(&url, "777", &status("Friends", Status::Stopped, &[], 0, "")).unwrap());
        assert_eq!(h.join().unwrap().0, "PATCH /api/webhooks/1/tok/messages/777 HTTP/1.1");
        let (url, h) = listener(r#"{"message":"Unknown Message"}"#, "404 Not Found");
        assert!(!edit(&url, "777", &json!({})).unwrap(), "deleted in Discord: post a new one");
        h.join().unwrap();
    }
}
