//! What PZMM does for the servers it runs while it's open: Workshop update checks, restarts with
//! in-game warnings (`servermsg`), daily restart times, backups on a timer, restarts after a crash,
//! and the Discord webhook. [`tick`] runs every few seconds (the app's background thread, or
//! `pzmm server start` in the CLI). A server re-downloads outdated Workshop items when it starts
//! (verified on 42.21), so an update needs only `save`, `quit` and a start.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::host::{self, Server, Status};
use super::{Paths, discord, server, server_backup};
use crate::sources::steam_workshop;
use crate::store::now;

/// A server's automation, kept in servers.json.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Auto {
    /// What a Workshop update does: "always" (restart after the warnings), "empty" (restart once
    /// nobody's online), "scheduled" (at the next restart time) or "notify" (only say so).
    pub updates: String,
    /// Minutes between Workshop update checks; 0: none.
    pub check_minutes: u32,
    /// The warnings players get before a restart, in minutes.
    pub warn_minutes: Vec<u32>,
    /// Daily restarts, "HH:MM" in local time.
    pub restarts: Vec<String>,
    /// Hours a server runs before it restarts (counted from its start); 0: none.
    pub restart_hours: u32,
    /// Minutes between `save`s; 0: only the server's own saves.
    pub save_minutes: u32,
    /// Messages players get on a timer.
    pub broadcasts: Vec<Broadcast>,
    /// Hours between backups; 0: none on a timer.
    pub backup_hours: u32,
    /// PZMM backs up before each start (the server's own BackupsOnStart is separate).
    pub backup_on_start: bool,
    /// Backups kept, and the MB they may take together (0: no cap).
    pub backup_keep: u32,
    pub backup_max_mb: u64,
    /// Restarts after a crash per hour; 0: crash watch off.
    pub crash_restarts: u32,
    pub discord: discord::Settings,
}

impl Default for Auto {
    fn default() -> Self {
        Auto {
            updates: "always".into(),
            check_minutes: 30,
            warn_minutes: vec![15, 5, 1],
            restarts: vec![],
            restart_hours: 0,
            save_minutes: 0,
            broadcasts: vec![],
            backup_hours: 0,
            backup_on_start: false,
            backup_keep: 10,
            backup_max_mb: 0,
            crash_restarts: 3,
            discord: Default::default(),
        }
    }
}

/// A message posted to players (`servermsg`) every `minutes`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Broadcast {
    pub minutes: u32,
    pub text: String,
}

/// Check settings before saving them.
pub fn check(a: &Auto) -> Result<()> {
    if !["always", "empty", "scheduled", "notify"].contains(&a.updates.as_str()) {
        anyhow::bail!("unknown update choice {:?}", a.updates);
    }
    if let Some(t) = a.restarts.iter().find(|t| clock(t).is_none()) {
        anyhow::bail!("{t} isn't a time (HH:MM)");
    }
    if a.warn_minutes.iter().any(|m| *m == 0 || *m > 120) {
        anyhow::bail!("warnings are 1 to 120 minutes before the restart");
    }
    if a.restart_hours > 168 || a.save_minutes > 1440 {
        anyhow::bail!("restarts are at most every 168 hours and saves every 1440 minutes");
    }
    let d = &a.discord;
    if [&d.channel, &d.role].iter().any(|id| !id.is_empty() && !super::discord_bot::valid_id(id)) {
        anyhow::bail!("a Discord channel or role ID is a long number (in Discord, turn on Developer Mode, then right-click the channel or role and Copy ID)");
    }
    for b in &a.broadcasts {
        if b.minutes == 0 || b.minutes > 1440 {
            anyhow::bail!("a message goes out every 1 to 1440 minutes");
        }
        // it goes in servermsg "...": one line, no quotes
        if b.text.trim().is_empty() || b.text.contains(['"', '\n', '\r']) {
            anyhow::bail!("a message is one line of text without double quotes");
        }
    }
    Ok(())
}

/// "HH:MM" as minutes after midnight.
fn clock(t: &str) -> Option<u64> {
    let (h, m) = t.trim().split_once(':')?;
    let (h, m): (u64, u64) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// The next daily restart after `now` (Unix seconds), for `times` in a zone `offset` seconds from UTC.
pub fn next_restart(times: &[String], now: u64, offset: i64) -> Option<u64> {
    let local = now as i64 + offset;
    let midnight = local - local.rem_euclid(86400);
    times
        .iter()
        .filter_map(|t| clock(t))
        .map(|m| {
            let at = midnight + m as i64 * 60;
            if at <= local { at + 86400 } else { at }
        })
        .min()
        .map(|at| (at - offset) as u64)
}

/// The next planned restart: the next daily time, or `restart_hours` after the server's start
/// (`started`, 0: not running), whichever comes first.
pub fn next_planned(a: &Auto, started: u64, now: u64, offset: i64) -> Option<u64> {
    let every = (a.restart_hours > 0 && started > 0).then(|| {
        let period = a.restart_hours as u64 * 3600;
        started + period * (now.saturating_sub(started) / period + 1)
    });
    [next_restart(&a.restarts, now, offset), every].into_iter().flatten().min()
}

/// Seconds the local time is ahead of UTC, now (daylight saving included).
pub fn utc_offset() -> i64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct TimeZoneInformation {
            bias: i32,
            standard_name: [u16; 32],
            standard_date: [u16; 8],
            standard_bias: i32,
            daylight_name: [u16; 32],
            daylight_date: [u16; 8],
            daylight_bias: i32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetTimeZoneInformation(tz: *mut TimeZoneInformation) -> u32;
        }
        // SAFETY: an all-integer struct laid out as TIME_ZONE_INFORMATION, which the call fills in
        let mut tz: TimeZoneInformation = unsafe { std::mem::zeroed() };
        let mode = unsafe { GetTimeZoneInformation(&mut tz) };
        let bias = tz.bias + if mode == 2 { tz.daylight_bias } else if mode == 1 { tz.standard_bias } else { 0 };
        -(bias as i64) * 60
    }
    #[cfg(not(windows))]
    {
        // "+0100"
        let out = std::process::Command::new("date").arg("+%z").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let (sign, hm) = out.split_at(out.len().min(1));
        let n: i64 = hm.parse().unwrap_or(0);
        (if sign == "-" { -1 } else { 1 }) * ((n / 100) * 3600 + (n % 100) * 60)
    }
}

/// Why a countdown runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Why {
    #[default]
    Updates,
    Schedule,
    /// Asked for by an admin (a Discord command).
    Admin,
}

/// What [`Watch::step`] asks for.
#[derive(Debug, PartialEq)]
pub enum Do {
    /// Tell players the restart is this many minutes away.
    Warn(u64),
    Restart,
    Backup,
    Check,
    Save,
    /// Post this message to players.
    Say(String),
}

/// One server's automation state (in memory: it runs while PZMM is open).
#[derive(Debug, Default)]
pub struct Watch {
    /// Workshop items updated since the server started (their titles).
    pub updated: Vec<String>,
    /// When the countdown ends (Unix seconds), and why.
    pub restart_at: Option<u64>,
    pub why: Why,
    warned: Vec<u32>,
    pub last_check: u64,
    pub last_backup: u64,
    /// A planned restart called off (it isn't started again), and an update restart called off
    /// (none until more updates come).
    skip: Option<u64>,
    hold: bool,
    last_save: u64,
    /// When each broadcast last went out.
    said: Vec<u64>,
    /// CPU (percent of the machine) and memory (bytes) samples, oldest first, and the process's
    /// CPU time at the last sample (100 ns units) with when it was read.
    samples: std::collections::VecDeque<(f32, u64)>,
    cpu_at: Option<(std::time::Instant, u64)>,
    /// Crash restarts in the last hour, and the process whose crash was handled.
    crashes: Vec<u64>,
    handled: Option<u32>,
    // for Discord: what was last seen
    pid: Option<u32>,
    status: Option<Status>,
    seq: u64,
    online: BTreeSet<String>,
    status_posted: u64,
    status_text: String,
    last_players_ask: u64,
    players: Option<usize>,
    /// A stop or restore asked for while a restart may be under way: it doesn't start the server again.
    stop_asked: bool,
}

impl Watch {
    /// The next steps at `now` for a running server: start a countdown (a planned restart within the
    /// warning lead, or updates by the server's choice; `players` is who's online when known),
    /// warn, restart, back up, check for updates, save, post messages.
    pub fn step(&mut self, a: &Auto, now: u64, next_planned: Option<u64>, players: Option<usize>) -> Vec<Do> {
        let mut out = vec![];
        // first: a restart (or a warning that fails) ends the round, and the backup mustn't be lost with it
        if a.backup_hours > 0 && now >= self.last_backup + a.backup_hours as u64 * 3600 {
            out.push(Do::Backup);
            self.last_backup = now;
        }
        let lead = a.warn_minutes.iter().max().copied().unwrap_or(0) as u64 * 60;
        if self.restart_at.is_none() {
            if let Some(at) = next_planned.filter(|at| *at <= now + lead && self.skip != Some(*at)) {
                (self.restart_at, self.why) = (Some(at), Why::Schedule);
            } else if !self.updated.is_empty() && !self.hold {
                match a.updates.as_str() {
                    "always" => (self.restart_at, self.why) = (Some(now + lead), Why::Updates),
                    // nobody to warn
                    "empty" if players == Some(0) => (self.restart_at, self.why) = (Some(now), Why::Updates),
                    _ => {}
                }
            }
        }
        if let Some(at) = self.restart_at {
            let due: Vec<u32> = a.warn_minutes.iter().copied().filter(|m| !self.warned.contains(m) && now < at && now + *m as u64 * 60 >= at).collect();
            // ticks may skip a warning: say only the latest, with the real time left
            if !due.is_empty() {
                out.push(Do::Warn((at - now).div_ceil(60)));
            }
            self.warned.extend(due);
            if now >= at {
                out.push(Do::Restart);
                self.restart_at = None;
                self.warned.clear();
            }
        }
        if a.check_minutes > 0 && now >= self.last_check + a.check_minutes as u64 * 60 {
            out.push(Do::Check);
            self.last_check = now;
        }
        // saves and messages count from the first time they're seen on
        if a.save_minutes > 0 {
            if self.last_save == 0 {
                self.last_save = now;
            } else if now >= self.last_save + a.save_minutes as u64 * 60 {
                out.push(Do::Save);
                self.last_save = now;
            }
        }
        self.said.resize(a.broadcasts.len(), 0);
        for (b, said) in a.broadcasts.iter().zip(self.said.iter_mut()) {
            if *said == 0 {
                *said = now;
            } else if now >= *said + b.minutes as u64 * 60 {
                out.push(Do::Say(b.text.clone()));
                *said = now;
            }
        }
        out
    }

    /// Call off the countdown: a planned restart is skipped, an update restart waits for more
    /// updates (or the Restart button). False when none runs.
    fn abort(&mut self) -> bool {
        let Some(at) = self.restart_at.take() else { return false };
        match self.why {
            Why::Schedule => self.skip = Some(at),
            Why::Updates => self.hold = true,
            Why::Admin => {}
        }
        self.warned.clear();
        true
    }

    /// Add a sample: `cpu` is the process's CPU time (100 ns units) read at `at`, over `cores` cores.
    fn sample(&mut self, at: std::time::Instant, cpu: Option<u64>, memory: u64, cores: u32) {
        let pct = match (self.cpu_at, cpu) {
            (Some((t0, c0)), Some(c)) if at > t0 => (c.saturating_sub(c0) as f64 / 1e5 / at.duration_since(t0).as_secs_f64() / cores.max(1) as f64).min(100.0) as f32,
            _ => 0.0,
        };
        // the first reading has nothing to compare with
        if self.cpu_at.is_some() || cpu.is_none() {
            if self.samples.len() >= SAMPLES {
                self.samples.pop_front();
            }
            self.samples.push_back((pct, memory));
        }
        self.cpu_at = cpu.map(|c| (at, c));
    }

    /// Whether a crash may be restarted: fewer than `limit` restarts in the last hour (it counts this one).
    fn crash_allowed(&mut self, now: u64, limit: u32) -> bool {
        self.crashes.retain(|t| now.saturating_sub(*t) < 3600);
        let ok = (self.crashes.len() as u32) < limit;
        if ok {
            self.crashes.push(now);
        }
        ok
    }
}

/// Samples kept per server: 15 minutes at one per tick (5 s).
const SAMPLES: usize = 180;

/// What players read before a restart.
pub fn warning(minutes: u64, why: Why) -> String {
    let when = if minutes == 1 { "1 minute".to_string() } else { format!("{minutes} minutes") };
    match why {
        Why::Updates => format!("The server restarts in {when} to update mods. Find a safe spot."),
        Why::Schedule => format!("The server restarts in {when} (scheduled restart). Find a safe spot."),
        Why::Admin => format!("The server restarts in {when}. Find a safe spot."),
    }
}

/// The titles of the items updated on the Workshop after the server started.
fn updated_since(items: &[steam_workshop::Item], started: u64) -> Vec<String> {
    items.iter().filter(|i| i.time_updated > started).map(|i| if i.title.is_empty() { i.id.clone() } else { i.title.clone() }).collect()
}

static WATCH: Mutex<BTreeMap<String, Watch>> = Mutex::new(BTreeMap::new());

fn with<T>(name: &str, f: impl FnOnce(&mut Watch) -> T) -> T {
    f(WATCH.lock().unwrap_or_else(|e| e.into_inner()).entry(name.into()).or_default())
}

/// For the server card: the updated items waiting for a restart, and when the countdown ends (0: none).
pub fn pending(name: &str) -> (Vec<String>, u64) {
    WATCH.lock().unwrap_or_else(|e| e.into_inner()).get(name).map_or((vec![], 0), |w| (w.updated.clone(), w.restart_at.unwrap_or(0)))
}

/// For the server card: CPU (percent) and memory (bytes) over the last minutes, oldest first.
pub fn samples(name: &str) -> Vec<(f32, u64)> {
    WATCH.lock().unwrap_or_else(|e| e.into_inner()).get(name).map_or(vec![], |w| w.samples.iter().copied().collect())
}

/// Call off a running restart countdown and tell the players. False when none runs.
pub fn abort_restart(paths: &Paths, name: &str) -> Result<bool> {
    if !with(name, Watch::abort) {
        return Ok(false);
    }
    crate::say!("serverRestartAborted", "{name}: the restart was called off", name = name);
    record(paths, name, "aborted", "");
    let _ = host::send(paths, name, "servermsg \"The restart was called off.\"");
    Ok(true)
}

/// Start a restart countdown of `minutes` (0: at the next tick); players get the server's warnings
/// that fit in it. Replaces a countdown already running.
pub fn restart_in(name: &str, minutes: u32) {
    let at = now() + minutes as u64 * 60;
    with(name, |w| {
        (w.restart_at, w.why) = (Some(at), Why::Admin);
        w.warned.clear();
    });
}

// --- the last 24 hours, kept in <data>/server-history.json ---

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub server: String,
    /// Unix seconds.
    pub time: u64,
    /// "mods" (Workshop updates: their titles), "restart" (the updates it took), "aborted",
    /// "crash" or "server" (its files updated: the build).
    pub kind: String,
    pub text: String,
}

fn history_file(paths: &Paths) -> std::path::PathBuf {
    paths.data.join("server-history.json")
}

fn read_history(paths: &Paths) -> Result<Vec<Event>> {
    crate::read_json(&history_file(paths))
}

/// Note what happened to a server; entries older than a day go.
pub fn record(paths: &Paths, server: &str, kind: &str, text: &str) {
    let result = (|| -> Result<()> {
        let _lock = crate::lock_data(paths)?;
        let cut = now().saturating_sub(86400);
        let mut all: Vec<Event> = read_history(paths)?.into_iter().filter(|e| e.time >= cut).collect();
        all.push(Event { server: server.into(), time: now(), kind: kind.into(), text: text.into() });
        crate::write_atomic(&history_file(paths), serde_json::to_string(&all)?)?;
        Ok(())
    })();
    if let Err(e) = result {
        crate::log!("{server}: couldn't note {kind} in the history: {e:#}");
    }
}

/// What happened to a server in the last 24 hours, newest first.
pub fn history(paths: &Paths, server: &str) -> Vec<Event> {
    let cut = now().saturating_sub(86400);
    let mut out: Vec<Event> = read_history(paths).unwrap_or_default().into_iter().filter(|e| e.server == server && e.time >= cut).collect();
    out.reverse();
    out
}

/// Look at the server's Workshop items now; returns the titles of those updated since it started.
pub fn check_updates(paths: &Paths, name: &str) -> Result<Vec<String>> {
    let s = host::find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    let lines = server::read(&s.ini(paths))?;
    let ids: Vec<&str> = lines.workshop.iter().map(String::as_str).collect();
    let mut items = vec![];
    for chunk in ids.chunks(100) {
        items.extend(steam_workshop::details(chunk)?);
    }
    let started = if s.started > 0 && s.pid.is_some() { s.started } else { now() };
    let updated = updated_since(&items, started);
    let new: Vec<String> = with(name, |w| {
        let new: Vec<String> = updated.iter().filter(|t| !w.updated.contains(t)).cloned().collect();
        w.hold &= new.is_empty();
        w.updated = updated.clone();
        w.last_check = now();
        new
    });
    if !new.is_empty() {
        crate::say!("serverModsUpdated", "{name}: {count} Workshop mod(s) updated ({titles}); the server picks them up when it restarts", name = name, count = new.len(), titles = new.join(", "));
        record(paths, name, "mods", &new.join(", "));
        notify(&s, "updated", &format!("{} {} updated on the Workshop: {}. Players can't join until the server restarts.", new.len(), if new.len() == 1 { "mod" } else { "mods" }, new.join(", ")), None);
    }
    Ok(updated)
}

/// Stop a server, back it up (when updates are waiting) and start it. The Restart button and the countdowns use it.
pub fn restart(paths: &Paths, name: &str) -> Result<()> {
    let s = host::find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
    notify(&s, "restarting", "Restarting now.", None);
    let updates = pending(name).0;
    host::stop(paths, name, Duration::from_secs(90))?;
    // a Stop or Restore came in meanwhile: the server stays stopped
    if with(name, |w| std::mem::take(&mut w.stop_asked)) {
        return Ok(());
    }
    // taken stopped, so the world in it is whole
    if !updates.is_empty() {
        server_backup::create(paths, &s, "update")?;
    }
    host::start(paths, name, None)?;
    with(name, |w| {
        w.updated.clear();
        w.restart_at = None;
        w.hold = false;
    });
    record(paths, name, "restart", &updates.join(", "));
    Ok(())
}

/// A Stop or Restore of `name` is coming: a restart under way leaves it stopped.
/// The Restart button: a Stop from before it doesn't count, one during it does.
pub fn restart_asked(paths: &Paths, name: &str) -> Result<()> {
    with(name, |w| w.stop_asked = false);
    restart(paths, name)
}

pub fn stop_asked(name: &str) {
    with(name, |w| w.stop_asked = true);
}

/// The errors Check setup finds in server `name`'s config (those the Start button asks about), as
/// one line; empty when there are none or the mods can't be read. Planned and update restarts
/// don't start a config with errors.
fn config_errors(paths: &Paths, name: &str) -> String {
    let Ok(mods) = crate::store::rows(paths) else { return String::new() };
    let errors: Vec<String> = super::doctor::check_server(paths, name, &mods).into_iter().filter(|f| f.level == super::doctor::Level::Error).map(|f| f.text).collect();
    errors.join(" ")
}

/// Post `event` to the server's Discord webhook when it has one and the event is on.
fn notify(s: &Server, event: &str, text: &str, lines: Option<&[String]>) {
    let d = &s.auto.discord;
    if !d.saved || !d.events.iter().any(|e| e == event) {
        return;
    }
    let Some(url) = discord::url(&s.name) else { return };
    if let Err(e) = discord::post(&url, &discord::event(&s.name, event, text, lines)) {
        crate::log!("Discord ({}): {e:#}", s.name);
    }
}

/// One round for every server PZMM runs: crash watch, Discord, countdowns, backups and update checks.
/// Each server on a thread of its own, so one restarting or backing up doesn't hold up the others.
pub fn tick(paths: &Paths) {
    let offset = utc_offset();
    for s in host::load(paths).into_iter().filter(|s| s.external.is_none()) {
        let paths = Paths { zomboid: paths.zomboid.clone(), data: paths.data.clone() };
        let round = Round::start();
        std::thread::spawn(move || {
            let _round = round;
            tick_guarded(&paths, &s, offset)
        });
    }
}

/// Rounds started by [`tick`] and not yet done: moving the data folder waits for them.
static ROUNDS: AtomicUsize = AtomicUsize::new(0);

/// Counted from before its thread starts (so a move can't slip in between) until it ends, panics included.
struct Round;
impl Round {
    fn start() -> Round {
        ROUNDS.fetch_add(1, Ordering::SeqCst);
        Round
    }
}
impl Drop for Round {
    fn drop(&mut self) {
        ROUNDS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Wait until no round is running (the caller keeps new ones from starting, as the app's data move does).
pub fn wait_rounds() {
    while ROUNDS.load(Ordering::SeqCst) > 0 {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// One round for server `name` only, on this thread (the CLI's `server watch`).
pub fn tick_only(paths: &Paths, name: &str) {
    if let Some(s) = host::load(paths).into_iter().find(|s| s.external.is_none() && s.name == name) {
        tick_guarded(paths, &s, utc_offset());
    }
}

/// The servers whose round is running: a restart or backup can outlast the next tick.
static BUSY: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Server `s`'s round, unless its last one is still running.
fn tick_guarded(paths: &Paths, s: &Server, offset: i64) {
    struct Done(String);
    impl Drop for Done {
        // also when the round panics
        fn drop(&mut self) {
            BUSY.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
        }
    }
    if !BUSY.lock().unwrap_or_else(|e| e.into_inner()).insert(s.name.clone()) {
        return;
    }
    let _done = Done(s.name.clone());
    if let Err(e) = tick_one(paths, s, now(), offset) {
        crate::log!("{}: {e:#}", s.name);
    }
}

fn tick_one(paths: &Paths, s: &Server, now: u64, offset: i64) -> Result<()> {
    let name = s.name.as_str();
    let a = &s.auto;
    let (status, pid) = host::state(s);

    // crash watch
    if let Some((crashed_pid, lines)) = host::crashed(name)
        && with(name, |w| w.handled != Some(crashed_pid))
    {
        let again = with(name, |w| {
            w.handled = Some(crashed_pid);
            a.crash_restarts > 0 && w.crash_allowed(now, a.crash_restarts)
        });
        let text = if again {
            crate::say!("serverCrashRestart", "{name} crashed: PZMM restarts it", name = name);
            "The server crashed. PZMM is restarting it.".to_string()
        } else {
            if a.crash_restarts > 0 {
                crate::say!("serverCrashGaveUp", "{name} crashed {count} times within an hour: PZMM stopped restarting it", name = name, count = a.crash_restarts);
            } else {
                crate::say!("serverCrashed", "{name} crashed", name = name);
            }
            "The server crashed and PZMM isn't restarting it.".to_string()
        };
        notify(s, "crashed", &text, Some(&lines));
        record(paths, name, "crash", "");
        if again {
            host::start(paths, name, None)?;
        }
        return Ok(());
    }

    // Discord: status changes, players joining and leaving, the status message
    let mut events = vec![];
    with(name, |w| {
        if w.pid != pid {
            (w.pid, w.seq) = (pid, 0);
            w.online.clear();
            w.updated.clear();
            // a countdown ends with the run it was for: it mustn't fire as the next one starts
            w.restart_at = None;
            w.warned.clear();
        }
        match (w.status, status) {
            (Some(Status::Starting), Status::Running) => events.push(("started", "The server is up.".to_string())),
            (Some(Status::Running | Status::Stopping), Status::Stopped) => events.push(("stopped", "The server stopped.".to_string())),
            _ => {}
        }
        w.status = Some(status);
    });
    let (attached, lines) = if host::attached(name) { host::log(paths, name, with(name, |w| w.seq)) } else { (false, vec![]) };
    if attached {
        let mut seen = vec![];
        with(name, |w| {
            for l in &lines {
                w.seq = w.seq.max(l.seq);
                match host::player_event(&l.text) {
                    Some((true, who)) if w.online.insert(who.clone()) => {
                        events.push(("joined", format!("{who} joined.")));
                        seen.push((true, who));
                    }
                    Some((false, who)) if w.online.remove(&who) => {
                        events.push(("left", format!("{who} left.")));
                        seen.push((false, who));
                    }
                    _ => {}
                }
            }
        });
        if let Err(e) = super::dossier::record(paths, name, now, &seen) {
            crate::log!("{name}: couldn't note who joined and left: {e:#}");
        }
    }
    for (event, text) in &events {
        notify(s, event, text, None);
    }
    if a.discord.saved && a.discord.events.iter().any(|e| e == "status") {
        post_status(paths, s, status, now);
    }
    if status != Status::Running {
        return Ok(());
    }
    if let Some(pid) = pid {
        let (cpu, memory) = (host::cpu_time(pid), host::java_memory(pid).unwrap_or(0));
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
        with(name, |w| w.sample(std::time::Instant::now(), cpu, memory, cores));
    }

    // countdowns, backups, update checks
    if with(name, |w| w.last_backup == 0) {
        let newest = server_backup::list(paths, name).first().map_or(now, |b| b.time);
        with(name, |w| w.last_backup = newest);
    }
    let ask_players = with(name, |w| a.updates == "empty" && !w.updated.is_empty() && w.restart_at.is_none() && now >= w.last_players_ask + 60);
    if ask_players {
        let n = host::players(paths, name).ok().map(|p| p.len());
        with(name, |w| (w.players, w.last_players_ask) = (n, now));
    }
    let todo = with(name, |w| {
        let n = w.players.take();
        w.step(a, now, next_planned(a, s.started, now, offset), n)
    });
    // a restart is decided: a Stop from before (one that failed) no longer counts, one from now on does
    if todo.iter().any(|d| matches!(d, Do::Restart)) {
        with(name, |w| w.stop_asked = false);
    }
    for d in todo {
        match d {
            Do::Warn(m) => {
                let why = with(name, |w| w.why);
                if m == a.warn_minutes.iter().max().copied().unwrap_or(0) as u64 {
                    notify(s, "restarting", &format!("Restarting in {m} minutes."), None);
                }
                host::send(paths, name, &format!("servermsg \"{}\"", warning(m, why)))?;
            }
            Do::Restart => {
                // a planned or update restart is checked before stopping: a config with errors isn't
                // started, and the server keeps running. An admin's own restart isn't second-guessed.
                let why = with(name, |w| w.why);
                let errors = if why == Why::Admin { String::new() } else { config_errors(paths, name) };
                if !errors.is_empty() {
                    if why == Why::Updates {
                        with(name, |w| w.hold = true); // waits for more updates or the Restart button
                    }
                    crate::say!("serverRestartBroken", "error: {name}: not restarting, as Check setup finds errors in its config (it keeps running): {errors}", name = name, errors = errors);
                    record(paths, name, "aborted", &errors);
                    notify(s, "restarting", &format!("Not restarting: the server's config has errors. {errors}"), None);
                    let _ = host::send(paths, name, "servermsg \"The restart was called off.\"");
                    return Ok(());
                }
                crate::say!("serverAutoRestart", "{name}: restarting (saving first)", name = name);
                restart(paths, name)?;
                return Ok(());
            }
            Do::Backup => {
                let _ = host::send(paths, name, "save");
                std::thread::sleep(Duration::from_secs(5));
                // the restart queued after it still happens (players were warned)
                if let Err(e) = server_backup::create(paths, s, "scheduled") {
                    crate::log!("error: {name}: the scheduled backup failed: {e:#}");
                }
            }
            Do::Check => {
                check_updates(paths, name)?;
            }
            Do::Save => {
                host::send(paths, name, "save")?;
            }
            Do::Say(text) => {
                host::send(paths, name, &format!("servermsg \"{text}\""))?;
            }
        }
    }
    Ok(())
}

/// Keep the status message current (edited in place; a new one when it's gone). At most once a minute,
/// sooner when something changed.
fn post_status(paths: &Paths, s: &Server, status: Status, now: u64) {
    let players: Vec<String> = with(&s.name, |w| w.online.iter().cloned().collect());
    let up = if status == Status::Running && s.started > 0 { now.saturating_sub(s.started) } else { 0 };
    let payload = discord::status(&s.name, status, &players, up, &s.address);
    let text = serde_json::to_string(&(status, &players)).unwrap_or_default();
    let due = with(&s.name, |w| (w.status_text != text || now >= w.status_posted + 60).then(|| (w.status_text = text, w.status_posted = now)).is_some());
    if !due {
        return;
    }
    let Some(url) = discord::url(&s.name) else { return };
    let id = s.auto.discord.message_id.clone();
    let edited = !id.is_empty() && discord::edit(&url, &id, &payload).unwrap_or(false);
    if !edited && let Ok(new) = discord::post(&url, &payload) {
        let _ = host::set_auto(paths, &s.name, |a| a.discord.message_id = new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(updates: &str) -> Auto {
        Auto { updates: updates.into(), check_minutes: 0, ..Default::default() }
    }

    #[test]
    fn one_round_per_server_at_a_time() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let s = Server { name: "pzmmbusy".into(), ..Default::default() };
        BUSY.lock().unwrap().insert(s.name.clone());
        tick_guarded(&paths, &s, 0);
        assert!(BUSY.lock().unwrap().contains(&s.name), "the round still running keeps it");
        BUSY.lock().unwrap().remove(&s.name);
        tick_guarded(&paths, &s, 0);
        assert!(!BUSY.lock().unwrap().contains(&s.name), "freed when done");
    }

    #[test]
    fn a_data_move_waits_for_running_rounds() {
        let round = Round::start();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            drop(round);
        });
        let start = std::time::Instant::now();
        wait_rounds();
        assert!(start.elapsed() >= Duration::from_millis(150));
        t.join().unwrap();
    }

    #[test]
    fn unattended_restarts_check_the_config() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let dir = paths.zomboid.join("Server");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(paths.mods()).unwrap();
        std::fs::write(dir.join("srv.ini"), "Mods=\\A\n").unwrap();
        assert_eq!(config_errors(&paths, "srv"), "", "warnings don't stop a restart");
        std::fs::write(dir.join("srv.ini"), "Mods=A\n").unwrap();
        assert!(config_errors(&paths, "srv").contains("backslash"));
    }

    #[test]
    fn an_update_warns_then_restarts() {
        let a = auto("always");
        let mut w = Watch { updated: vec!["Hydro".into()], ..Default::default() };
        let t0 = 1_000_000;
        assert_eq!(w.step(&a, t0, None, Some(3)), [Do::Warn(15)]);
        assert_eq!(w.restart_at, Some(t0 + 900));
        assert_eq!(w.step(&a, t0 + 30, None, None), [], "each warning once");
        assert_eq!(w.step(&a, t0 + 600, None, None), [Do::Warn(5)]);
        // a slow tick skips the 1-minute mark: one warning, with the real time left
        assert_eq!(w.step(&a, t0 + 875, None, None), [Do::Warn(1)]);
        assert_eq!(w.step(&a, t0 + 900, None, None), [Do::Restart]);
        assert_eq!(w.restart_at, None);
        w.updated.clear(); // the restart picks them up
        assert_eq!(w.step(&a, t0 + 960, None, None), []);
    }

    #[test]
    fn update_choices() {
        let t0 = 1_000_000;
        let pending = || Watch { updated: vec!["Hydro".into()], ..Default::default() };
        // only when empty: waits while anyone is on, then restarts without warnings
        let mut w = pending();
        assert_eq!(w.step(&auto("empty"), t0, None, Some(2)), []);
        assert_eq!(w.step(&auto("empty"), t0 + 60, None, None), [], "unknown player count: wait");
        assert_eq!(w.step(&auto("empty"), t0 + 120, None, Some(0)), [Do::Restart]);
        // notify and scheduled never restart by themselves
        for choice in ["notify", "scheduled"] {
            let mut w = pending();
            assert_eq!(w.step(&auto(choice), t0, None, Some(0)), [], "{choice}");
        }
        // scheduled: the daily restart takes them, with its warnings
        let mut w = pending();
        assert_eq!(w.step(&auto("scheduled"), t0, Some(t0 + 3600), Some(0)), []);
        assert_eq!(w.step(&auto("scheduled"), t0 + 2700, Some(t0 + 3600), Some(0)), [Do::Warn(15)]);
        assert_eq!(w.why, Why::Schedule);
        assert_eq!(w.step(&auto("scheduled"), t0 + 3600, Some(t0 + 3600), None), [Do::Restart]);
    }

    #[test]
    fn timers_back_up_and_check() {
        let a = Auto { backup_hours: 6, check_minutes: 30, ..Default::default() };
        let mut w = Watch { last_backup: 1000, last_check: 1000, ..Default::default() };
        assert_eq!(w.step(&a, 1000 + 1799, None, None), []);
        assert_eq!(w.step(&a, 1000 + 1800, None, None), [Do::Check]);
        assert_eq!(w.step(&a, 1000 + 6 * 3600, None, None), [Do::Backup, Do::Check]);
        assert_eq!(w.step(&a, 1000 + 6 * 3600 + 60, None, None), []);
    }

    #[test]
    fn a_due_backup_comes_before_a_restart() {
        let a = Auto { backup_hours: 6, check_minutes: 0, ..Default::default() };
        let mut w = Watch { last_backup: 1000, restart_at: Some(1000 + 6 * 3600), why: Why::Admin, ..Default::default() };
        // the restart ends the round: the backup must already have run
        assert_eq!(w.step(&a, 1000 + 6 * 3600, None, None), [Do::Backup, Do::Restart]);
    }

    #[test]
    fn next_daily_restart() {
        let day = 1_790_000_000 - 1_790_000_000 % 86400; // a UTC midnight
        let times = vec!["06:00".to_string(), "18:30".to_string()];
        assert_eq!(next_restart(&times, day + 3600, 0), Some(day + 6 * 3600));
        assert_eq!(next_restart(&times, day + 6 * 3600, 0), Some(day + 18 * 3600 + 1800), "a time that's now is next day's");
        assert_eq!(next_restart(&times, day + 20 * 3600, 0), Some(day + 86400 + 6 * 3600), "tomorrow");
        // UTC+2: 06:00 local is 04:00 UTC
        assert_eq!(next_restart(&times, day + 3600, 7200), Some(day + 4 * 3600));
        // UTC-5 just after UTC midnight: it's still 19:xx yesterday locally
        assert_eq!(next_restart(&times, day + 60, -5 * 3600), Some(day + 11 * 3600));
        assert_eq!(next_restart(&[], day, 0), None);
        assert_eq!(next_restart(&["25:00".into(), "x".into()], day, 0), None);
        assert!(check(&Auto { restarts: vec!["7:5".into()], ..Default::default() }).is_ok());
        assert!(check(&Auto { restarts: vec!["24:00".into()], ..Default::default() }).is_err());
        assert!(check(&Auto { updates: "sometimes".into(), ..Default::default() }).is_err());
    }

    #[test]
    fn every_few_hours_aborts_saves_and_messages() {
        let t0 = 1_000_000;
        let a = Auto { restart_hours: 6, check_minutes: 0, ..Default::default() };
        assert_eq!(next_planned(&a, t0, t0 + 60, 0), Some(t0 + 6 * 3600));
        assert_eq!(next_planned(&a, t0, t0 + 6 * 3600 + 5, 0), Some(t0 + 12 * 3600), "a skipped one: the next period");
        assert_eq!(next_planned(&a, 0, t0, 0), None, "not running");
        let daily = Auto { restarts: vec!["00:00".into()], ..a.clone() };
        let day = 1_790_000_000 - 1_790_000_000 % 86400;
        assert_eq!(next_planned(&daily, day - 3600, day - 60, 0), Some(day), "the daily one comes first");

        // a planned restart called off stays off; the next one runs
        let mut w = Watch::default();
        let at = t0 + 6 * 3600;
        assert_eq!(w.step(&a, at - 900, Some(at), None), [Do::Warn(15)]);
        assert!(w.abort() && !w.abort());
        assert_eq!(w.step(&a, at - 300, Some(at), None), [], "no new countdown for the same restart");
        assert_eq!(w.step(&a, at, Some(at), None), []);
        assert_eq!(w.step(&a, at + 6 * 3600 - 900, Some(at + 6 * 3600), None), [Do::Warn(15)]);
        // an update restart called off waits for more updates
        let mut w = Watch { updated: vec!["Hydro".into()], ..Default::default() };
        let u = auto("always");
        assert_eq!(w.step(&u, t0, None, None), [Do::Warn(15)]);
        assert!(w.abort());
        assert_eq!(w.step(&u, t0 + 60, None, None), []);
        w.hold = false; // check_updates found another
        assert_eq!(w.step(&u, t0 + 120, None, None), [Do::Warn(15)]);

        let s = Auto { save_minutes: 10, broadcasts: vec![Broadcast { minutes: 30, text: "Join our Discord".into() }], check_minutes: 0, ..Default::default() };
        let mut w = Watch::default();
        assert_eq!(w.step(&s, t0, None, None), [], "they count from now");
        assert_eq!(w.step(&s, t0 + 600, None, None), [Do::Save]);
        assert_eq!(w.step(&s, t0 + 1800, None, None), [Do::Save, Do::Say("Join our Discord".into())]);
        assert!(check(&s).is_ok());
        for bad in ["say \"hi\"", "two\nlines", " "] {
            assert!(check(&Auto { broadcasts: vec![Broadcast { minutes: 5, text: bad.into() }], ..Default::default() }).is_err(), "{bad}");
        }
    }

    #[test]
    fn cpu_samples() {
        let mut w = Watch::default();
        let t = std::time::Instant::now();
        w.sample(t, Some(0), 100, 4);
        assert!(w.samples.is_empty(), "nothing to compare the first reading with");
        // 2 s of CPU in 1 s over 4 cores: half the machine
        w.sample(t + Duration::from_secs(1), Some(20_000_000), 200, 4);
        assert_eq!(w.samples.back(), Some(&(50.0, 200)));
        for i in 0..200 {
            w.sample(t + Duration::from_secs(2 + i), Some(20_000_000), 300, 4);
        }
        assert_eq!(w.samples.len(), SAMPLES);
        assert_eq!(w.samples.back(), Some(&(0.0, 300)));
    }

    #[test]
    fn the_history_keeps_a_day() {
        let (_t, paths) = crate::store::tests::temp_paths();
        let old = Event { server: "Friends".into(), time: now() - 90_000, kind: "mods".into(), text: "Old".into() };
        std::fs::create_dir_all(&paths.data).unwrap();
        std::fs::write(history_file(&paths), serde_json::to_string(&[old]).unwrap()).unwrap();
        record(&paths, "Friends", "mods", "Hydro");
        record(&paths, "Other", "crash", "");
        record(&paths, "Friends", "restart", "Hydro");
        let h = history(&paths, "Friends");
        assert_eq!(h.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["restart", "mods"], "newest first, a day only, this server only");
        assert_eq!(read_history(&paths).unwrap().len(), 3, "the old one is gone from the file");
    }

    #[test]
    fn crash_restarts_are_limited() {
        let mut w = Watch::default();
        let t0 = 1_000_000;
        assert!(w.crash_allowed(t0, 2) && w.crash_allowed(t0 + 60, 2));
        assert!(!w.crash_allowed(t0 + 120, 2), "a third within the hour: stop");
        assert!(w.crash_allowed(t0 + 3600, 2), "the first one has aged out");
        assert!(!Watch::default().crash_allowed(t0, 0));
    }

    #[test]
    fn updated_items_since_the_start() {
        let item = |id: &str, title: &str, t| steam_workshop::Item { id: id.into(), title: title.into(), time_updated: t, ..Default::default() };
        let items = [item("1", "Hydro", 500), item("2", "", 1500), item("3", "Old", 900)];
        assert_eq!(updated_since(&items, 1000), ["2"]);
        assert_eq!(warning(1, Why::Updates), "The server restarts in 1 minute to update mods. Find a safe spot.");
        assert!(warning(5, Why::Schedule).contains("5 minutes (scheduled restart)"));
    }

    #[test]
    fn settings_default_when_missing() {
        let a: Auto = serde_json::from_str(r#"{"updates": "empty"}"#).unwrap();
        assert_eq!((a.updates.as_str(), a.warn_minutes.as_slice(), a.crash_restarts), ("empty", &[15, 5, 1][..], 3));
    }

    /// The real server in `.temp/pzserver`, everything it writes under `.temp/s1b/it`: start, back up,
    /// a shortened countdown in game, then the restart (save, quit, start) and a stop.
    #[test]
    #[ignore = "runs the dedicated server installed in .temp/pzserver (about three minutes)"]
    fn real_server_backs_up_warns_and_restarts() {
        let temp = std::path::absolute(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.temp")).unwrap();
        let (install, scratch) = (temp.join("pzserver"), temp.join("s1b").join("it"));
        assert!(host::installed(&install), "no server in {}", install.display());
        let _ = std::fs::remove_dir_all(&scratch);
        let paths = Paths { zomboid: scratch.join("Zomboid"), data: scratch.join("data") };
        let s = host::tests::scratch_server(&paths, "pzmmauto", install, scratch.join("cache"));
        let wait = |want: Status, secs: u64| {
            let until = std::time::Instant::now() + Duration::from_secs(secs);
            loop {
                let st = host::state(&host::find(&paths, "pzmmauto").unwrap()).0;
                if st == want || std::time::Instant::now() > until {
                    return st;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        };
        host::start(&paths, "pzmmauto", Some(&host::generate_password())).unwrap();
        assert_eq!(wait(Status::Running, 180), Status::Running);

        let b = server_backup::create(&paths, &s, "test").unwrap();
        let names: Vec<String> = {
            let mut z = zip::ZipArchive::new(std::fs::File::open(&b.file).unwrap()).unwrap();
            (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().to_string()).collect()
        };
        assert!(names.iter().any(|n| n == "Server/pzmmauto.ini") && names.iter().any(|n| n.starts_with("Saves/Multiplayer/pzmmauto/")), "{names:?}");
        assert!(server_backup::empty_cells(&s.cachedir(&paths).join("Saves/Multiplayer/pzmmauto")).is_empty());

        // a one-minute countdown, ticked by hand
        let a = Auto { warn_minutes: vec![1], check_minutes: 0, ..Default::default() };
        let mut w = Watch { updated: vec!["Hydro".into()], ..Default::default() };
        let t0 = now();
        let todo = w.step(&a, t0, None, Some(0));
        assert_eq!(todo, [Do::Warn(1)]);
        host::send(&paths, "pzmmauto", &format!("servermsg \"{}\"", warning(1, w.why))).unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let log = host::log(&paths, "pzmmauto", 0).1;
        assert!(log.iter().any(|l| l.text.contains("servermsg") && l.text.contains("restarts in 1 minute")), "the server took the message");
        assert_eq!(w.step(&a, t0 + 60, None, None), [Do::Restart]);
        let pid = host::find(&paths, "pzmmauto").unwrap().pid.unwrap();
        restart(&paths, "pzmmauto").unwrap();
        assert!(host::java_memory(pid).is_none(), "the old java has gone");
        assert_eq!(wait(Status::Running, 180), Status::Running, "up again");
        let log = host::log(&paths, "pzmmauto", 0).1;
        assert!(log.iter().any(|l| l.text.contains("SERVER STARTED")));

        let pid = host::find(&paths, "pzmmauto").unwrap().pid.unwrap();
        host::stop(&paths, "pzmmauto", Duration::from_secs(60)).unwrap();
        assert!(host::java_memory(pid).is_none(), "java has gone");
    }
}
