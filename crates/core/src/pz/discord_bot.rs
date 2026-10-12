//! A server's Discord bot: PZMM reads one channel with the server's bot and answers `!` commands
//! from members with the set role, while PZMM (or its tray) runs. No gateway connection: the channel
//! is read over Discord's REST API every 5 seconds per server, with PZMM's HTTP agent, backing off
//! (doubling up to 5 minutes) after errors and waiting as long as a rate limit asks. Each member's
//! roles are looked up once per 5 minutes. Replies are embeds that never ping anyone. The token
//! lives in the OS credential store; Discord gives bots message text only with the Message Content
//! intent on (Developer Portal, Bot). With relay on, general chat from the server's chat log goes to
//! the channel (one message per read), and plain messages there from members with the role go in game.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::host::{self, Server, Status};
use super::{Paths, autopilot, discord};
use crate::store::now;

pub const API: &str = "https://discord.com/api/v10";
const EVERY: Duration = Duration::from_secs(5);
const MAX_WAIT: Duration = Duration::from_secs(300);
const ROLE_CACHE: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Help,
    Status,
    Players,
    /// In this many minutes; None: with the server's usual warnings.
    Restart(Option<u32>),
    Save,
    Msg(String),
    UpdateCheck,
    /// A relayed chat message, as said in game (never typed: `parse` doesn't make it).
    Chat(String),
}

impl Command {
    /// As typed, for the activity log.
    fn shown(&self) -> String {
        match self {
            Command::Help => "!help".into(),
            Command::Status => "!status".into(),
            Command::Players => "!players".into(),
            Command::Restart(None) => "!restart".into(),
            Command::Restart(Some(m)) => format!("!restart {m}"),
            Command::Save => "!save".into(),
            Command::Msg(t) => format!("!msg {t}"),
            Command::UpdateCheck => "!update-check".into(),
            Command::Chat(t) => t.clone(),
        }
    }
}

/// `!command args` as a command; None when the message isn't one. A command with bad arguments is `Help`.
pub fn parse(text: &str) -> Option<Command> {
    let rest = text.trim().strip_prefix('!')?;
    let (cmd, arg) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(c, a)| (c, a.trim()));
    Some(match cmd.to_ascii_lowercase().as_str() {
        "help" => Command::Help,
        "status" => Command::Status,
        "players" => Command::Players,
        "restart" if arg.is_empty() => Command::Restart(None),
        "restart" => arg.parse().ok().filter(|m| *m <= 120).map_or(Command::Help, |m| Command::Restart(Some(m))),
        "save" => Command::Save,
        "msg" if sayable(arg) => Command::Msg(arg.into()),
        "msg" => Command::Help,
        "update-check" => Command::UpdateCheck,
        _ => return None,
    })
}

/// Text that fits in servermsg "...": one line, no quotes.
fn sayable(t: &str) -> bool {
    !t.is_empty() && t.len() <= 200 && !t.contains(['"', '\n', '\r'])
}

const HELP: &str = "`!status` · `!players` · `!restart [minutes]` (no minutes: with the usual warnings; 0: now) · `!save` · `!msg <text>` (to everyone in game, one line, no double quotes) · `!update-check`";

/// A Discord ID (a snowflake).
pub fn valid_id(id: &str) -> bool {
    (15..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

fn secret(name: &str) -> String {
    format!("server-discord-bot:{name}")
}

/// Keep the bot token (empty: forget it).
pub fn save_token(name: &str, token: &str) -> Result<()> {
    let token = token.trim();
    if !token.is_empty() && (token.len() < 50 || token.contains(char::is_whitespace) || token.matches('.').count() != 2) {
        bail!("that isn't a bot token (Developer Portal, your application, Bot, Reset Token)");
    }
    crate::settings::set_secret(&secret(name), token)
}

/// Discord asked to wait this many seconds.
#[derive(Debug)]
struct Limited(f64);

impl std::fmt::Display for Limited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Discord asked to slow down for {:.0} s", self.0)
    }
}

impl std::error::Error for Limited {}

/// GET `path`, or POST `body` to it.
fn call(api: &str, token: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    let url = format!("{api}{path}");
    let auth = format!("Bot {token}");
    let a = crate::sources::agent();
    let res = match body {
        None => a.get(&url).config().http_status_as_error(false).build().header("Authorization", &auth).header("User-Agent", crate::USER_AGENT).call(),
        Some(b) => a.post(&url).config().http_status_as_error(false).build().header("Authorization", &auth).header("User-Agent", crate::USER_AGENT).header("Content-Type", "application/json").send(b.to_string()),
    };
    let mut res = res?;
    let status = res.status().as_u16();
    let v: Value = serde_json::from_str(&res.body_mut().read_to_string().unwrap_or_default()).unwrap_or(Value::Null);
    match status {
        200..=299 => Ok(v),
        429 => Err(Limited(v["retry_after"].as_f64().unwrap_or(5.0)).into()),
        401 => bail!("Discord refused the bot token"),
        403 => bail!("the bot isn't allowed in that channel (it needs View Channel, Send Messages and Read Message History)"),
        404 => bail!("Discord doesn't know that channel or member"),
        _ => bail!("Discord answered {status}: {}", v["message"].as_str().unwrap_or("")),
    }
}

/// One server's bot, in memory.
#[derive(Debug, Default)]
pub struct Bot {
    /// The settings it reads with (a change starts over).
    key: String,
    /// The newest message seen; None until the first read (messages from before are never run).
    last: Option<u64>,
    guild: String,
    /// Whether each member may use commands, and when that was looked up.
    allowed: BTreeMap<String, (bool, Instant)>,
    next: Option<Instant>,
    wait: Duration,
    error: String,
    /// Relay: the chat log and its last line posted (None until the first read, which posts nothing).
    chat: Option<(PathBuf, Option<u64>)>,
}

fn id(v: &Value) -> u64 {
    v["id"].as_str().and_then(|i| i.parse().ok()).unwrap_or(0)
}

/// Read the channel once: the commands to run from members with `role`, as (message id, user name,
/// command), oldest first; with `relay`, their plain messages too (as `Chat`).
fn poll(api: &str, token: &str, channel: &str, role: &str, relay: bool, bot: &mut Bot) -> Result<Vec<(String, String, Command)>> {
    let Some(last) = bot.last else {
        let newest = call(api, token, &format!("/channels/{channel}/messages?limit=1"), None)?;
        bot.last = Some(newest.as_array().and_then(|m| m.first()).map_or(0, id));
        return Ok(vec![]);
    };
    let got = call(api, token, &format!("/channels/{channel}/messages?after={last}&limit=50"), None)?;
    let mut msgs: Vec<&Value> = got.as_array().map(|m| m.iter().collect()).unwrap_or_default();
    msgs.sort_by_key(|m| id(m));
    let mut out = vec![];
    for m in msgs {
        let before = bot.last;
        bot.last = Some(id(m).max(bot.last.unwrap_or(0)));
        if m["author"]["bot"].as_bool() == Some(true) {
            continue;
        }
        let text = m["content"].as_str().unwrap_or_default().trim();
        let cmd = match parse(text) {
            Some(c) => c,
            // an unknown `!` word stays a command (ignored)
            None if relay && !text.starts_with('!') && sayable(text) => Command::Chat(text.into()),
            None => continue,
        };
        let user = m["author"]["id"].as_str().unwrap_or_default().to_string();
        match allowed(api, token, channel, role, &user, bot) {
            Ok(true) => {}
            Ok(false) => continue,
            // this message and those after it are read again next time; the ones before still run
            Err(e) => {
                bot.last = before;
                return if out.is_empty() { Err(e) } else { Ok(out) };
            }
        }
        let who = m["author"]["global_name"].as_str().or(m["author"]["username"].as_str()).unwrap_or("someone").to_string();
        let cmd = match cmd {
            Command::Chat(t) => Command::Chat(format!("[Discord] {}: {t}", who.replace('"', ""))),
            c => c,
        };
        out.push((m["id"].as_str().unwrap_or_default().to_string(), who, cmd));
    }
    Ok(out)
}

/// Whether `user` has `role` in the channel's server.
fn allowed(api: &str, token: &str, channel: &str, role: &str, user: &str, bot: &mut Bot) -> Result<bool> {
    if let Some((ok, at)) = bot.allowed.get(user)
        && at.elapsed() < ROLE_CACHE
    {
        return Ok(*ok);
    }
    if bot.guild.is_empty() {
        bot.guild = call(api, token, &format!("/channels/{channel}"), None)?["guild_id"].as_str().unwrap_or_default().to_string();
    }
    let member = if bot.guild.is_empty() {
        Value::Null
    } else {
        match call(api, token, &format!("/guilds/{}/members/{user}", bot.guild), None) {
            // they left the server
            Err(e) if e.to_string().starts_with("Discord doesn't know") => Value::Null,
            m => m?,
        }
    };
    let ok = member["roles"].as_array().is_some_and(|r| r.iter().any(|r| r == role));
    bot.allowed.insert(user.to_string(), (ok, Instant::now()));
    Ok(ok)
}

/// A reply to message `to` ("": a plain message).
fn reply_body(server: &str, kind: &str, text: &str, to: &str) -> Value {
    let mut b = discord::event(server, kind, text, None);
    let o = b.as_object_mut().expect("an object");
    o.remove("username");
    if !to.is_empty() {
        o.insert("message_reference".into(), json!({ "message_id": to, "fail_if_not_exists": false }));
    }
    b
}

fn post(api: &str, token: &str, channel: &str, body: &Value) -> Result<()> {
    call(api, token, &format!("/channels/{channel}/messages"), Some(body)).map(|_| ())
}

/// Game text made plain for Discord: mentions (<@id>, <@&id>, <#id>) dropped, @everyone and @here
/// broken (a zero-width space after every @), and [text](url) links shown as written.
fn plain(t: &str) -> String {
    let mut out = String::new();
    let mut rest = t;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let r = &rest[i..];
        match r.find('>') {
            Some(j) if j > 2 && r[1..j].starts_with(['@', '#']) && r[1..j].bytes().all(|b| b.is_ascii_digit() || b"@#&!".contains(&b)) => rest = &r[j + 1..],
            _ => {
                out.push('<');
                rest = &r[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace('@', "@\u{200b}").replace("](", "] (")
}

/// Chat lines (name, text) as one message of "**name**: text" lines, within Discord's 2000 characters.
// ponytail: lines past the limit are counted, not posted; split into more messages if busy servers need them
fn batch(lines: &[(String, String)]) -> String {
    let mut out = String::new();
    for (i, (who, text)) in lines.iter().enumerate() {
        let line = format!("**{}**: {}\n", plain(who).replace('*', ""), plain(text));
        if out.chars().count() + line.chars().count() > 1950 {
            out += &format!("… {} more", lines.len() - i);
            break;
        }
        out += &line;
    }
    out.trim_end().to_string()
}

/// Post the general chat in `file` (the server's chat log) since the last read; the first read only
/// notes where it ends. A post that fails is tried again next time.
fn relay_out(api: &str, token: &str, channel: &str, file: Option<PathBuf>, bot: &mut Bot) -> Result<()> {
    let file = file.unwrap_or_default();
    let after = match &bot.chat {
        None => {
            bot.chat = Some((file.clone(), host::tail(&file, 1, 0).last().map(|l| l.seq)));
            return Ok(());
        }
        Some((f, at)) if *f == file => *at,
        Some(_) => None, // a new log: all of it
    };
    let lines: Vec<_> = host::tail(&file, 500, after.unwrap_or(0)).into_iter().filter(|l| Some(l.seq) > after).collect();
    let Some(end) = lines.last().map(|l| l.seq) else { return Ok(()) };
    // what players typed in general chat (not whispers, faction, admin or server messages)
    let chat: Vec<(String, String)> = lines
        .iter()
        .filter(|l| l.text.contains("Got message:"))
        .filter_map(|l| host::chat_message(&l.text))
        .filter(|(c, _, t)| ["General", "All"].contains(&c.as_str()) && !t.trim().is_empty())
        .map(|(_, a, t)| (a, t))
        .collect();
    if !chat.is_empty() {
        post(api, token, channel, &json!({ "content": batch(&chat), "allowed_mentions": { "parse": [] } }))?;
    }
    bot.chat = Some((file, Some(end)));
    Ok(())
}

/// Run a command on server `name`: the reply's embed (with the message it answers set by the caller).
pub fn run(paths: &Paths, name: &str, cmd: &Command) -> Value {
    let Some(s) = host::find(paths, name) else { return reply_body(name, "crashed", "PZMM doesn't manage this server any more.", "") };
    let status = host::state(&s).0;
    let up = status == Status::Running;
    let reply = |kind: &str, text: String| reply_body(name, kind, &text, "");
    let result: Result<Value> = (|| {
        Ok(match cmd {
            Command::Help => reply("status", HELP.into()),
            Command::Status => {
                let players = if up { host::players(paths, name).unwrap_or_default() } else { vec![] };
                let uptime = if up && s.started > 0 { now().saturating_sub(s.started) } else { 0 };
                let mut b = discord::status(name, status, &players, uptime, &s.address);
                b.as_object_mut().expect("an object").remove("username");
                b
            }
            _ if !up => bail!("The server isn't running."),
            Command::Players => {
                let p = host::players(paths, name)?;
                reply("status", if p.is_empty() { "Nobody is online.".into() } else { format!("Online ({}): {}", p.len(), p.join(", ")) })
            }
            Command::Restart(m) => {
                let m = m.unwrap_or_else(|| s.auto.warn_minutes.iter().max().copied().unwrap_or(0));
                autopilot::restart_in(name, m);
                reply("restarting", if m == 0 { "Restarting now.".into() } else { format!("Restarting in {m} {}; players get the warnings.", if m == 1 { "minute" } else { "minutes" }) })
            }
            Command::Save => {
                host::send(paths, name, "save")?;
                reply("status", "Saving the world.".into())
            }
            Command::Msg(text) => {
                host::send(paths, name, &format!("servermsg \"{text}\""))?;
                reply("status", "Sent to everyone in game.".into())
            }
            Command::Chat(_) => bail!("chat isn't a command"),
            Command::UpdateCheck => {
                let t = autopilot::check_updates(paths, name)?;
                reply("updated", if t.is_empty() { "Every Workshop mod is up to date.".into() } else { format!("{} {} updated since the server started: {}", t.len(), if t.len() == 1 { "mod" } else { "mods" }, t.join(", ")) })
            }
        })
    })();
    result.unwrap_or_else(|e| reply("crashed", format!("{e:#}")))
}

static BOTS: Mutex<BTreeMap<String, Bot>> = Mutex::new(BTreeMap::new());

/// The servers whose data folder another one uses too: they share its Logs folder, so a relay
/// couldn't tell their chat apart and stays off for them.
pub fn shared_logs(paths: &Paths, servers: &[Server]) -> BTreeSet<String> {
    let dir = |s: &Server| {
        let d = s.cachedir(paths);
        if cfg!(windows) { PathBuf::from(d.to_string_lossy().to_lowercase()) } else { d }
    };
    servers.iter().filter(|s| servers.iter().any(|o| o.name != s.name && dir(o) == dir(s))).map(|s| s.name.clone()).collect()
}

/// Read every bot's channel that's due and start its commands (each on a thread of its own, so a
/// restart doesn't hold up the others). Call it every second or so.
pub fn tick(paths: &Paths) {
    let servers = host::load(paths);
    let shared = shared_logs(paths, &servers);
    for s in servers.into_iter().filter(|s| s.external.is_none() && s.auto.discord.bot && valid_id(&s.auto.discord.channel) && valid_id(&s.auto.discord.role)) {
        let d = &s.auto.discord;
        let relay = d.relay && !shared.contains(&s.name);
        let key = format!("{}/{}/{relay}", d.channel, d.role);
        let mut bots = BOTS.lock().unwrap_or_else(|e| e.into_inner());
        let bot = bots.entry(s.name.clone()).or_default();
        if bot.key != key {
            *bot = Bot { key, ..Default::default() };
            if d.relay && !relay {
                crate::say!("discordRelayShared", "{name}: chat relay is off because another server uses the same data folder, so their chat would mix", name = s.name);
            }
        }
        // the credential store only when it's due
        if bot.next.is_some_and(|n| Instant::now() < n) {
            continue;
        }
        let Some(token) = crate::settings::secret(&secret(&s.name)) else { continue };
        // chat out first: a failed post leaves the channel unread, so no command is lost
        let out = if relay { relay_out(API, &token, &d.channel, host::game_log_file(paths, &s.name, "chat"), bot) } else { Ok(()) };
        match out.and_then(|_| poll(API, &token, &d.channel, &d.role, relay, bot)) {
            Ok(cmds) => {
                (bot.wait, bot.next) = (Duration::ZERO, Some(Instant::now() + EVERY));
                bot.error.clear();
                for (to, who, cmd) in cmds {
                    if let Command::Chat(line) = &cmd {
                        // nobody to tell while it's down
                        if host::state(&s).0 == Status::Running
                            && let Err(e) = host::send(paths, &s.name, &format!("servermsg \"{line}\""))
                        {
                            crate::log!("Discord bot ({}): {e:#}", s.name);
                        }
                        continue;
                    }
                    crate::say!("discordCommand", "{name}: {user} sent {command} on Discord", name = s.name, user = who, command = cmd.shown());
                    let paths = Paths { zomboid: paths.zomboid.clone(), data: paths.data.clone() };
                    let (name, channel, token) = (s.name.clone(), d.channel.clone(), token.clone());
                    std::thread::spawn(move || {
                        let mut body = run(&paths, &name, &cmd);
                        if let Some(o) = body.as_object_mut() {
                            o.insert("message_reference".into(), json!({ "message_id": to, "fail_if_not_exists": false }));
                            o.insert("allowed_mentions".into(), json!({ "parse": [] }));
                        }
                        if let Err(e) = post(API, &token, &channel, &body) {
                            crate::log!("Discord bot ({name}): {e:#}");
                        }
                    });
                }
            }
            Err(e) => {
                let limited = e.downcast_ref::<Limited>().map(|l| Duration::from_secs_f64(l.0.clamp(1.0, 3600.0)));
                bot.wait = limited.unwrap_or_else(|| (bot.wait * 2).clamp(Duration::from_secs(10), MAX_WAIT));
                bot.next = Some(Instant::now() + bot.wait);
                let text = format!("{e:#}");
                if bot.error != text && limited.is_none() {
                    crate::log!("Discord bot ({}): {text}", s.name);
                }
                bot.error = text;
            }
        }
    }
}

/// Post a test message to the saved channel.
pub fn test(name: &str, channel: &str) -> Result<()> {
    let Some(token) = crate::settings::secret(&secret(name)) else { bail!("save a bot token for {name} first") };
    if !valid_id(channel) {
        bail!("set the channel's ID first (and save)");
    }
    post(API, &token, channel, &reply_body(name, "status", &format!("PZMM reads commands here for {name}. {HELP}"), ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_sharing_a_data_folder_share_no_relay() {
        let paths = Paths { zomboid: PathBuf::from("/z/Zomboid"), data: PathBuf::from("/z/pzmm") };
        let at = |name: &str, dir: Option<&str>| Server { name: name.into(), cachedir: dir.map(PathBuf::from), ..Default::default() };
        let servers = [at("a", None), at("b", Some("/z/Zomboid")), at("c", Some("/z/own")), at("d", Some("/z/other"))];
        assert_eq!(shared_logs(&paths, &servers), BTreeSet::from(["a".to_string(), "b".to_string()]));
        assert!(shared_logs(&paths, &servers[2..]).is_empty(), "each its own folder");
    }

    #[test]
    fn commands() {
        assert_eq!(parse("!status"), Some(Command::Status));
        assert_eq!(parse("  !Players "), Some(Command::Players));
        assert_eq!(parse("!restart"), Some(Command::Restart(None)));
        assert_eq!(parse("!restart 10"), Some(Command::Restart(Some(10))));
        assert_eq!(parse("!restart soon"), Some(Command::Help));
        assert_eq!(parse("!restart 500"), Some(Command::Help));
        assert_eq!(parse("!msg Back in 5"), Some(Command::Msg("Back in 5".into())));
        assert_eq!(parse("!msg say \"hi\""), Some(Command::Help), "it would break out of servermsg");
        assert_eq!(parse("!update-check"), Some(Command::UpdateCheck));
        assert_eq!(parse("hello !status"), None);
        assert_eq!(parse("!dance"), None);
        assert!(valid_id("123456789012345678") && !valid_id("12ab") && !valid_id(""));
        assert!(save_token("S", "short").is_err());
        let r = reply_body("Friends", "status", "Sent.", "42");
        assert_eq!((r.get("username"), &r["message_reference"]["message_id"], &r["allowed_mentions"]["parse"]), (None, &json!("42"), &json!([])));
    }

    #[test]
    fn reads_the_channel_and_checks_roles_on_a_local_server() {
        let msgs = json!([
            { "id": "104", "content": "!save", "author": { "id": "9", "bot": true } },
            { "id": "103", "content": "just chatting", "author": { "id": "7" } },
            { "id": "102", "content": "!status", "author": { "id": "7", "username": "guest" } },
            { "id": "101", "content": "!restart 5", "author": { "id": "8", "username": "admin", "global_name": "Admin" } }
        ]);
        let (api, seen) = crate::sources::fake_http(vec![
            ("GET /channels/1/messages?limit=1", 200, json!([{ "id": "100" }]).to_string()),
            ("GET /channels/1/messages?after=100", 200, msgs.to_string()),
            ("GET /channels/1/messages?after=104", 200, "[]".into()),
            ("GET /channels/1 ", 200, json!({ "id": "1", "guild_id": "5" }).to_string()),
            ("GET /guilds/5/members/8", 200, json!({ "roles": ["3", "66"] }).to_string()),
            ("GET /guilds/5/members/7", 200, json!({ "roles": ["3"] }).to_string()),
            ("GET /channels/2/", 429, json!({ "retry_after": 2.5 }).to_string()),
            ("GET /channels/3/", 401, "{}".into()),
            ("GET /channels/4/messages?after=200", 200, json!([
                { "id": "201", "content": "!save", "author": { "id": "11" } },
                { "id": "202", "content": "!status", "author": { "id": "8", "username": "admin" } },
                { "id": "203", "content": "!players", "author": { "id": "13" } }
            ]).to_string()),
            ("GET /channels/4 ", 200, json!({ "id": "4", "guild_id": "5" }).to_string()),
            ("GET /guilds/5/members/13", 500, "{}".into()),
        ]);
        let mut bot = Bot::default();
        assert!(poll(&api, "tok", "1", "66", false, &mut bot).unwrap().is_empty(), "the first read only notes where the channel is");
        assert_eq!(bot.last, Some(100));
        let got = poll(&api, "tok", "1", "66", false, &mut bot).unwrap();
        assert_eq!(got, [("101".to_string(), "Admin".to_string(), Command::Restart(Some(5)))], "only members with the role; never bots");
        assert_eq!(bot.last, Some(104));
        assert!(poll(&api, "tok", "1", "66", false, &mut bot).unwrap().is_empty());
        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen.iter().filter(|(l, _)| l.starts_with("GET /channels/1 ")).count(), 1, "the server is looked up once");
            assert_eq!(seen.iter().filter(|(l, _)| l.contains("/members/")).count(), 2, "each member once");
        }
        post(&api, "tok", "1", &reply_body("Friends", "status", "Saving.", "101")).unwrap_err(); // no route: 404
        assert!(seen.lock().unwrap().last().unwrap().1.contains("\"message_id\":\"101\""));

        let e = poll(&api, "tok", "2", "66", false, &mut Bot::default()).unwrap_err();
        assert_eq!(e.downcast_ref::<Limited>().map(|l| l.0), Some(2.5));
        let e = poll(&api, "tok", "3", "66", false, &mut Bot::default()).unwrap_err();
        assert!(e.to_string().contains("token"), "{e}");

        // 11 left the server (404); looking 13 up fails: 202 still runs, 203 is read again next time
        let mut bot = Bot { last: Some(200), ..Default::default() };
        let got = poll(&api, "tok", "4", "66", false, &mut bot).unwrap();
        assert_eq!((got.len(), got[0].0.as_str(), bot.last), (1, "202", Some(202)));
        let mut bot = Bot { last: Some(202), ..Default::default() };
        assert!(poll(&api, "tok", "4", "66", false, &mut bot).is_err(), "nothing read after 202");
    }

    #[test]
    fn relayed_chat_is_plain_and_batched() {
        assert_eq!(plain("hey @everyone <@123> <@&45> <#67> [free](http://x.example) <3 a<b>"), "hey @\u{200b}everyone    [free] (http://x.example) <3 a<b>");
        assert_eq!(plain("@here"), "@\u{200b}here");
        assert_eq!(batch(&[("B*ob".into(), "hi".into()), ("Ann".into(), "yo".into())]), "**Bob**: hi\n**Ann**: yo");
        let busy: Vec<(String, String)> = (0..100).map(|i| (format!("p{i}"), "x".repeat(40))).collect();
        let b = batch(&busy);
        assert!(b.chars().count() <= 2000 && b.starts_with("**p0**: ") && b.ends_with(" more"), "{b}");
        let (posted, more) = (b.lines().count() - 1, b.rsplit("… ").next().unwrap().trim_end_matches(" more").parse::<usize>().unwrap());
        assert_eq!(posted + more, 100, "every line posted or counted");
    }

    #[test]
    fn relays_chat_both_ways() {
        let (api, seen) = crate::sources::fake_http(vec![
            ("GET /channels/6/messages?after=300", 200, json!([
                { "id": "301", "content": "**Bob**: hi", "author": { "id": "9", "bot": true } },
                { "id": "302", "content": "hello all", "author": { "id": "8", "global_name": "Ad\"min" } },
                { "id": "303", "content": "me too", "author": { "id": "7" } },
                { "id": "304", "content": "!dance", "author": { "id": "8" } },
                { "id": "305", "content": "say \"hi\"", "author": { "id": "8" } },
                { "id": "306", "content": "!status", "author": { "id": "8" } }
            ]).to_string()),
            ("GET /channels/6 ", 200, json!({ "id": "6", "guild_id": "5" }).to_string()),
            ("GET /guilds/5/members/8", 200, json!({ "roles": ["66"] }).to_string()),
            ("GET /guilds/5/members/7", 200, json!({ "roles": [] }).to_string()),
            ("POST /channels/6/messages", 200, "{}".into()),
            ("POST /channels/7/messages", 429, json!({ "retry_after": 1.0 }).to_string()),
        ]);
        let got = poll(&api, "tok", "6", "66", true, &mut Bot { last: Some(300), ..Default::default() }).unwrap();
        let cmds: Vec<_> = got.into_iter().map(|(_, _, c)| c).collect();
        assert_eq!(cmds, [Command::Chat("[Discord] Admin: hello all".into()), Command::Status], "the role's plain messages; never bots, `!` words or quotes");
        let got = poll(&api, "tok", "6", "66", false, &mut Bot { last: Some(300), ..Default::default() }).unwrap();
        assert_eq!(got.len(), 1, "relay off: commands only");

        let (_t, paths) = crate::store::tests::temp_paths();
        std::fs::create_dir_all(&paths.data).unwrap();
        let log = paths.data.join("2026-10-12_10-00_chat.txt");
        let line = |chat: &str, who: &str, text: &str| format!("[12-10-26 10:00:00.000][info] Got message:ChatMessage{{chat={chat}, author='{who}', text='{text}'}}.\n");
        std::fs::write(&log, line("General", "Old", "before PZMM looked")).unwrap();
        let mut bot = Bot::default();
        relay_out(&api, "tok", "6", Some(log.clone()), &mut bot).unwrap();
        let posts = || seen.lock().unwrap().iter().filter(|(l, _)| l.starts_with("POST")).map(|(_, b)| b.clone()).collect::<Vec<_>>();
        assert!(posts().is_empty(), "the first read only notes where the log ends");
        let more = [line("General", "Bob", "hey @everyone <@123> [free](http://x.example)"), line("Faction", "Bob", "secret"), line("Whisper", "Ann", "psst"), "[12-10-26 10:00:01.000][info] General chat has id = 0.\n".into(), line("General", "Ann", "it's fine")].concat();
        std::fs::write(&log, line("General", "Old", "before PZMM looked") + &more).unwrap();
        relay_out(&api, "tok", "6", Some(log.clone()), &mut bot).unwrap();
        relay_out(&api, "tok", "6", Some(log.clone()), &mut bot).unwrap();
        let p = posts();
        assert_eq!(p.len(), 1, "one message per read, nothing twice");
        let body: Value = serde_json::from_str(&p[0]).unwrap();
        assert_eq!(body["content"], "**Bob**: hey @\u{200b}everyone  [free] (http://x.example)\n**Ann**: it's fine");
        assert_eq!(body["allowed_mentions"]["parse"], json!([]));

        // rate limited: the lines stay for next time
        std::fs::write(&log, line("General", "Old", "before PZMM looked") + &more + &line("General", "Cy", "later")).unwrap();
        let before = bot.chat.clone();
        assert!(relay_out(&api, "tok", "7", Some(log.clone()), &mut bot).unwrap_err().downcast_ref::<Limited>().is_some());
        assert_eq!(bot.chat, before);
        // a new log (the server restarted): all of it
        let new = paths.data.join("2026-10-12_11-00_chat.txt");
        std::fs::write(&new, line("General", "Dee", "back")).unwrap();
        relay_out(&api, "tok", "6", Some(new), &mut bot).unwrap();
        assert!(posts().last().unwrap().contains("**Dee**: back"));
    }
}
