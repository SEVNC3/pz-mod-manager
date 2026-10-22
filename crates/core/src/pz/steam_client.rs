//! Steam's own server list without a Web API key: the running Steam client asked through the
//! game's `steam_api64.dll` (`ISteamMatchmakingServers::RequestInternetServerList`).
//!
//! It runs in a short-lived child process (PZMM's own exe with [`CHILD_ARG`]): the Steam API has to
//! see `SteamAppId` in its environment (setting it in the app, which runs many threads, isn't
//! sound), a fault inside Steam's DLL then can't take the app down, and Steam stops showing the
//! player as "playing Project Zomboid" the moment the child exits, even if `SteamAPI_Shutdown` hangs.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use super::launcher::Server;

/// The hidden argument that makes PZMM's exe list Steam's servers on stdout (JSON) and exit.
pub const CHILD_ARG: &str = "--steam-server-list";
/// The longest the child waits for Steam's list.
#[cfg(windows)]
const PUMP: Duration = Duration::from_secs(15);
/// How long the list's count stays the same before it's taken as complete.
#[cfg(windows)]
const SETTLED: Duration = Duration::from_secs(1);
/// The child's whole run, after which it's killed.
const CHILD: Duration = Duration::from_secs(30);

/// The hidden argument that makes PZMM's exe subscribe to (`sub`) or unsubscribe from (`unsub`) a Workshop item and exit.
pub const UGC_ARG: &str = "--steam-ugc";
/// The hidden argument that makes PZMM's exe print the account's subscribed Workshop items (a JSON array of ids) and exit.
pub const SUBSCRIBED_ARG: &str = "--steam-subscribed";
/// `ISteamUGC`'s accessors, newest first (an older game branch ships an older DLL).
#[cfg(windows)]
const UGC_VERSIONS: &[&[u8]] = &[b"SteamAPI_SteamUGC_v021\0", b"SteamAPI_SteamUGC_v020\0", b"SteamAPI_SteamUGC_v018\0", b"SteamAPI_SteamUGC_v017\0", b"SteamAPI_SteamUGC_v016\0", b"SteamAPI_SteamUGC_v015\0", b"SteamAPI_SteamUGC_v014\0"];

/// Steam's internet server list for PZ, from the running Steam client (the game's install `game` has the DLL).
pub fn list(game: &Path) -> Result<Vec<Server>> {
    parse(&helper(&[CHILD_ARG.as_ref(), game.as_os_str()], "Steam didn't send its server list in time", None)?)
}

/// Subscribe the signed-in Steam account to Workshop item `id` (or unsubscribe with `on` false),
/// through the running Steam client: Steam then downloads (or deletes) it as for any subscription.
pub fn subscribe(game: &Path, id: &str, on: bool) -> Result<()> {
    anyhow::ensure!(crate::sources::steam_workshop::valid_id(id), "not a Workshop item: {id}");
    helper(&[UGC_ARG.as_ref(), game.as_os_str(), (if on { "sub" } else { "unsub" }).as_ref(), id.as_ref()], "Steam didn't answer in time", None).map(|_| ())
}

/// The subscriptions helper's answer: the subscribed items, and whether they came from `GetItemState` (Steam listed none).
#[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Answer {
    pub ids: BTreeSet<String>,
    pub by_state: bool,
}

/// The signed-in Steam account's subscribed Workshop items for PZ, from the running Steam client. When Steam lists none,
/// each of `on_disk` (the items in its Workshop folder) is asked about too (`GetItemState`), and any it says is subscribed
/// is in the answer: an empty list can be a client that hasn't synced it yet. The ids go on the helper's stdin (thousands
/// of them would overflow a command line).
pub fn subscribed(game: &Path, on_disk: &BTreeSet<String>) -> Result<Answer> {
    let input: String = on_disk.iter().filter(|i| crate::sources::steam_workshop::valid_id(i)).map(|i| format!("{i}\n")).collect();
    parse_subscribed(&helper(&[SUBSCRIBED_ARG.as_ref(), game.as_os_str()], "Steam didn't answer in time", Some(&input))?)
}

/// The subscriptions helper's output: Steam's DLL lines, then its [`Answer`] as JSON.
fn parse_subscribed(out: &str) -> Result<Answer> {
    let a: Answer = serde_json::from_str(out.trim().lines().last().unwrap_or_default()).context("the Steam subscriptions helper's output")?;
    anyhow::ensure!(a.ids.iter().all(|i| crate::sources::steam_workshop::valid_id(i)), "the Steam subscriptions helper's output");
    Ok(a)
}

/// The item ids the subscriptions helper reads on its stdin: one per line (any whitespace).
fn parse_items(text: &str) -> Result<Vec<u64>> {
    text.split_whitespace().map(|i| i.parse::<u64>().context("a Workshop item id")).collect()
}

/// What PZMM knows of the account's subscriptions: Steam's set (`None`: not asked yet, or Steam wasn't there, so
/// folders are trusted as before), and the items unsubscribed through PZMM since (leftovers at once, before Steam is asked).
#[derive(Debug, Default)]
pub struct Subscriptions {
    set: Option<BTreeSet<String>>,
    dropped: BTreeSet<String>,
}

impl Subscriptions {
    pub const fn new() -> Self {
        Subscriptions { set: None, dropped: BTreeSet::new() }
    }
    /// Steam's answer: an item subscribed again (in Steam) stops being a leftover. An empty list while Steam's Workshop
    /// folder has items (`on_disk`) is a client that hasn't synced it yet, and so may be one that `GetItemState` answered
    /// for: each item on disk outside the answer must have been unsubscribed last in PZMM (`gone`: its history, or this
    /// run), or the answer is unknown and folders are trusted again. Whether it was taken as an answer.
    pub fn refreshed(&mut self, answer: Answer, on_disk: &BTreeSet<String>, gone: impl Fn(&str) -> bool) -> bool {
        let set = answer.ids;
        let unsure = set.is_empty() || answer.by_state;
        if unsure && on_disk.iter().any(|i| !set.contains(i) && !self.dropped.contains(i) && !gone(i)) {
            self.set = None;
            return false;
        }
        self.dropped.retain(|i| !set.contains(i));
        self.set = Some(set);
        true
    }
    /// PZMM just subscribed to (`on`) or unsubscribed from `item`.
    pub fn changed(&mut self, item: &str, on: bool) {
        if on {
            self.dropped.remove(item);
            if let Some(s) = self.set.as_mut() {
                s.insert(item.to_string());
            }
        } else {
            self.dropped.insert(item.to_string());
            if let Some(s) = self.set.as_mut() {
                s.remove(item);
            }
        }
    }
    /// A `steam:<item>` source still on disk whose item isn't subscribed any more.
    pub fn leftover(&self, source: &str) -> bool {
        source.strip_prefix("steam:").is_some_and(|i| self.dropped.contains(i) || self.set.as_ref().is_some_and(|s| !s.contains(i)))
    }
}

/// Run PZMM's exe as a Steam helper with `args`: its stdout, or its last error line.
fn helper(args: &[&std::ffi::OsStr], late: &str, input: Option<&str>) -> Result<String> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.args(args).env("SteamAppId", super::STEAM_APP).env("SteamGameId", super::STEAM_APP).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().context("starting the Steam list helper")?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        // on its own thread: the helper reads it all before it writes, but a pipe holds only so much
        let text = text.to_string();
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let mut out = child.stdout.take().context("the helper's output")?;
    let mut err = child.stderr.take().context("the helper's errors")?;
    // stderr too: Steam's DLL can write more than a pipe holds, and the helper would wait on it
    let errs = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    // drained on its own thread: thousands of servers overflow a pipe's buffer
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        out.read_to_string(&mut s).map(|_| s)
    });
    let end = Instant::now() + CHILD;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{late}");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let out = reader.join().map_err(|_| anyhow::anyhow!("reading the helper's output"))??;
    if !status.success() {
        let err = errs.join().unwrap_or_default();
        bail!("{}", err.trim().lines().last().unwrap_or("the Steam helper failed"));
    }
    Ok(out)
}

/// The helper's output: Steam's DLL prints its own lines first, the list is the last one.
fn parse(out: &str) -> Result<Vec<Server>> {
    serde_json::from_str(out.trim().lines().last().unwrap_or_default()).context("the Steam list helper's output")
}

/// In PZMM's exe before anything else: when started as the helper, list and say the exit code.
pub fn child() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let which = args.next()?;
    if which != CHILD_ARG && which != UGC_ARG && which != SUBSCRIBED_ARG {
        return None;
    }
    let game = std::path::PathBuf::from(args.next().unwrap_or_default());
    let got = if which == UGC_ARG {
        let (how, id) = (args.next().unwrap_or_default(), args.next().unwrap_or_default());
        id.to_str().and_then(|i| i.parse::<u64>().ok()).context("a Workshop item id").and_then(|id| ugc(&game, how == "sub", id)).map(|()| String::new())
    } else if which == SUBSCRIBED_ARG {
        let mut text = String::new();
        let items = std::io::stdin().read_to_string(&mut text).context("the item ids").and_then(|_| parse_items(&text));
        items.and_then(|items| subscribed_in_process(&game, &items)).and_then(|a| Ok(serde_json::to_string(&a)?))
    } else {
        in_process(&game).and_then(|l| Ok(serde_json::to_string(&l)?))
    };
    Some(match got {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("{e:#}");
            1
        }
    })
}

/// `servernetadr_t`: ports and IP in host byte order.
#[repr(C)]
struct NetAdr {
    connection_port: u16,
    query_port: u16,
    ip: u32,
}

/// `gameserveritem_t` (Steamworks `matchmakingtypes.h`, no `#pragma pack`: natural alignment,
/// 376 bytes on x64). The `bool`s are read as bytes.
#[allow(dead_code)] // the whole C layout, though only some fields are read
#[repr(C)]
struct Item {
    net: NetAdr,
    ping: i32,
    had_response: u8,
    do_not_refresh: u8,
    game_dir: [u8; 32],
    map: [u8; 32],
    description: [u8; 64],
    app_id: u32,
    players: i32,
    max_players: i32,
    bots: i32,
    password: u8,
    secure: u8,
    last_played: u32,
    server_version: i32,
    name: [u8; 64],
    tags: [u8; 128],
    steam_id: u64,
}

#[cfg(any(windows, test))]
fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned()
}

/// A listed server as the browser shows it (`addr` is where it answers queries). Steam clears
/// the details of one that didn't answer its ping.
#[cfg(any(windows, test))]
fn server(i: &Item) -> Option<Server> {
    if i.name[0] == 0 || i.net.ip == 0 || i.net.query_port == 0 {
        return None;
    }
    let ip = std::net::Ipv4Addr::from(i.net.ip);
    let mut s = Server {
        addr: format!("{ip}:{}", i.net.query_port),
        port: if i.net.connection_port == 0 { i.net.query_port } else { i.net.connection_port },
        name: text(&i.name),
        map: text(&i.map),
        players: i.players.max(0) as u32,
        max_players: i.max_players.max(0) as u32,
        keywords: text(&i.tags),
        password: i.password != 0,
        ..Default::default()
    };
    s.keywords_said();
    Some(s)
}

#[cfg(not(windows))]
fn ugc(_game: &Path, _on: bool, _id: u64) -> Result<()> {
    bail!("subscribing through the Steam client works on Windows only")
}

/// `RemoteStorageSubscribePublishedFileResult_t` / `...Unsubscribe...`: an `EResult`, then the item (8-byte aligned).
#[cfg(any(windows, test))] // only the Windows helper reads it
#[repr(C)]
struct UgcResult {
    result: i32,
    item: u64,
}

#[cfg(windows)]
fn ugc(game: &Path, on: bool, id: u64) -> Result<()> {
    use std::ffi::c_void;
    let dll = game.join("steam_api64.dll");
    if !dll.is_file() {
        bail!("the game's steam_api64.dll isn't in {}", game.display());
    }
    // SAFETY: the game's own Steam API DLL; every symbol is called with its documented flat signature
    unsafe {
        let lib = libloading::Library::new(&dll).with_context(|| format!("loading {}", dll.display()))?;
        init(&lib)?;
        let shutdown = lib.get::<unsafe extern "C" fn()>(b"SteamAPI_Shutdown\0")?;
        let got = (|| -> Result<()> {
            // the newest interface the game's DLL has (an older game branch ships an older DLL)
            let first = |names: &[&[u8]]| names.iter().find_map(|n| lib.get::<unsafe extern "C" fn() -> *mut c_void>(n).ok());
            let old = || anyhow::anyhow!("this game version's Steam API is too old to subscribe from PZMM: subscribe on the Workshop page");
            let ugc = first(UGC_VERSIONS).ok_or_else(old)?();
            let utils = first(&[b"SteamAPI_SteamUtils_v010\0", b"SteamAPI_SteamUtils_v009\0"]).ok_or_else(old)?();
            if ugc.is_null() || utils.is_null() {
                bail!("Steam's Workshop interface isn't there");
            }
            let name: &[u8] = if on { b"SteamAPI_ISteamUGC_SubscribeItem\0" } else { b"SteamAPI_ISteamUGC_UnsubscribeItem\0" };
            let call = lib.get::<unsafe extern "C" fn(*mut c_void, u64) -> u64>(name)?(ugc, id);
            let done = lib.get::<unsafe extern "C" fn(*mut c_void, u64, *mut bool) -> bool>(b"SteamAPI_ISteamUtils_IsAPICallCompleted\0")?;
            let result = lib.get::<unsafe extern "C" fn(*mut c_void, u64, *mut c_void, i32, i32, *mut bool) -> bool>(b"SteamAPI_ISteamUtils_GetAPICallResult\0")?;
            let run = lib.get::<unsafe extern "C" fn()>(b"SteamAPI_RunCallbacks\0")?;
            let end = Instant::now() + PUMP;
            let mut failed = false;
            while !done(utils, call, &mut failed) {
                if Instant::now() > end {
                    bail!("Steam didn't answer in time");
                }
                run();
                std::thread::sleep(Duration::from_millis(50));
            }
            let mut r = UgcResult { result: 0, item: 0 };
            // k_iSteamRemoteStorageCallbacks + 13 (subscribe) / + 15 (unsubscribe)
            let expected = if on { 1313 } else { 1315 };
            if !result(utils, call, (&mut r as *mut UgcResult).cast(), size_of::<UgcResult>() as i32, expected, &mut failed) || failed {
                bail!("Steam couldn't do it");
            }
            // EResult 1 is OK
            if r.result != 1 {
                bail!("Steam said no (EResult {})", r.result);
            }
            Ok(())
        })();
        shutdown();
        got
    }
}

#[cfg(not(windows))]
fn subscribed_in_process(_game: &Path, _items: &[u64]) -> Result<Answer> {
    bail!("asking the Steam client for subscriptions works on Windows only")
}

/// `ISteamUGC::GetSubscribedItems`: synchronous, from the client's own state. None listed: `GetItemState` of each of
/// `items` too, those with `k_EItemStateSubscribed` (1) in the answer.
#[cfg(windows)]
fn subscribed_in_process(game: &Path, items: &[u64]) -> Result<Answer> {
    use std::ffi::c_void;
    let dll = game.join("steam_api64.dll");
    if !dll.is_file() {
        bail!("the game's steam_api64.dll isn't in {}", game.display());
    }
    // SAFETY: the game's own Steam API DLL; every symbol is called with its documented flat signature. Newer SDKs add
    // `bIncludeLocallyDisabled` last (true: an item disabled locally in Steam is still subscribed, its folder still
    // there); an older DLL ignores it (x64 passes it in a register the callee never reads)
    unsafe {
        let lib = libloading::Library::new(&dll).with_context(|| format!("loading {}", dll.display()))?;
        init(&lib)?;
        let shutdown = lib.get::<unsafe extern "C" fn()>(b"SteamAPI_Shutdown\0")?;
        let got = (|| -> Result<Answer> {
            let ugc = UGC_VERSIONS.iter().find_map(|n| lib.get::<unsafe extern "C" fn() -> *mut c_void>(n).ok()).context("this game version's Steam API is too old")?();
            if ugc.is_null() {
                bail!("Steam's Workshop interface isn't there");
            }
            let count = lib.get::<unsafe extern "C" fn(*mut c_void, bool) -> u32>(b"SteamAPI_ISteamUGC_GetNumSubscribedItems\0")?(ugc, true);
            let mut ids = vec![0u64; count as usize];
            let got = lib.get::<unsafe extern "C" fn(*mut c_void, *mut u64, u32, bool) -> u32>(b"SteamAPI_ISteamUGC_GetSubscribedItems\0")?(ugc, ids.as_mut_ptr(), count, true);
            ids.truncate(got.min(count) as usize);
            let by_state = ids.is_empty() && !items.is_empty();
            if by_state {
                let state = lib.get::<unsafe extern "C" fn(*mut c_void, u64) -> u32>(b"SteamAPI_ISteamUGC_GetItemState\0")?;
                ids.extend(items.iter().copied().filter(|&i| state(ugc, i) & 1 != 0));
            }
            Ok(Answer { ids: ids.into_iter().map(|i| i.to_string()).collect(), by_state })
        })();
        shutdown();
        got
    }
}

/// Start the Steam API from the game's DLL (the app id comes from the environment, see [`helper`]).
#[cfg(windows)]
unsafe fn init(lib: &libloading::Library) -> Result<()> {
    // SAFETY: the caller's DLL; the documented flat signatures
    unsafe {
        if let Ok(init) = lib.get::<unsafe extern "C" fn(*mut [u8; 1024]) -> i32>(b"SteamAPI_InitFlat\0") {
            let mut msg = [0u8; 1024];
            if init(&mut msg) != 0 {
                bail!("Steam isn't available: {}", text(&msg));
            }
        } else if !lib.get::<unsafe extern "C" fn() -> bool>(b"SteamAPI_Init\0")?() {
            bail!("Steam isn't available: is it running and signed in?");
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn in_process(_game: &Path) -> Result<Vec<Server>> {
    bail!("asking the Steam client for servers works on Windows only")
}

#[cfg(windows)]
fn in_process(game: &Path) -> Result<Vec<Server>> {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// `ISteamMatchmakingServerListResponse`: a C++ object is a pointer to its vtable (three
    /// virtual methods, `this` first; x64 has one calling convention).
    #[repr(C)]
    struct VTable {
        responded: unsafe extern "C" fn(*mut Response, *mut c_void, i32),
        failed: unsafe extern "C" fn(*mut Response, *mut c_void, i32),
        complete: unsafe extern "C" fn(*mut Response, *mut c_void, i32),
    }
    #[repr(C)]
    struct Response {
        vtable: &'static VTable,
        done: AtomicBool,
    }
    unsafe extern "C" fn nothing(_: *mut Response, _: *mut c_void, _: i32) {}
    unsafe extern "C" fn complete(this: *mut Response, _: *mut c_void, _: i32) {
        // SAFETY: `this` is the Response handed to Steam, alive until the request is released
        unsafe { (*this).done.store(true, Ordering::Release) }
    }
    static VTABLE: VTable = VTable { responded: nothing, failed: nothing, complete };

    let dll = game.join("steam_api64.dll");
    if !dll.is_file() {
        bail!("the game's steam_api64.dll isn't in {}", game.display());
    }
    // SAFETY: the game's own Steam API DLL; every symbol is called with its documented flat signature
    unsafe {
        let lib = libloading::Library::new(&dll).with_context(|| format!("loading {}", dll.display()))?;
        init(&lib)?;
        // from here Steam shows the player in the game: keep it to the one request
        let shutdown = lib.get::<unsafe extern "C" fn()>(b"SteamAPI_Shutdown\0")?;
        let got = (|| -> Result<Vec<Server>> {
            let servers = lib.get::<unsafe extern "C" fn() -> *mut c_void>(b"SteamAPI_SteamMatchmakingServers_v002\0")?();
            if servers.is_null() {
                bail!("Steam's matchmaking servers interface isn't there");
            }
            let request = lib.get::<unsafe extern "C" fn(*mut c_void, u32, *mut *mut c_void, u32, *mut Response) -> *mut c_void>(b"SteamAPI_ISteamMatchmakingServers_RequestInternetServerList\0")?;
            let run = lib.get::<unsafe extern "C" fn()>(b"SteamAPI_RunCallbacks\0")?;
            let count = lib.get::<unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32>(b"SteamAPI_ISteamMatchmakingServers_GetServerCount\0")?;
            let details = lib.get::<unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> *const Item>(b"SteamAPI_ISteamMatchmakingServers_GetServerDetails\0")?;
            let release = lib.get::<unsafe extern "C" fn(*mut c_void, *mut c_void)>(b"SteamAPI_ISteamMatchmakingServers_ReleaseRequest\0")?;
            let mut response = Box::new(Response { vtable: &VTABLE, done: AtomicBool::new(false) });
            let app = super::STEAM_APP.parse().unwrap_or(0);
            let req = request(servers, app, std::ptr::null_mut(), 0, &mut *response);
            if req.is_null() {
                bail!("Steam didn't take the server list request");
            }
            // Steam's backend sends every server's details within a second or so; Steam then pings each
            // one itself, which takes minutes. The list is read once its count settles.
            let (end, mut seen, mut settled) = (Instant::now() + PUMP, 0, Instant::now());
            while !response.done.load(Ordering::Acquire) && Instant::now() < end {
                run();
                std::thread::sleep(Duration::from_millis(50));
                let n = count(servers, req);
                if n != seen {
                    (seen, settled) = (n, Instant::now());
                } else if n > 0 && settled.elapsed() > SETTLED {
                    break;
                }
            }
            let out = (0..count(servers, req)).filter_map(|i| details(servers, req, i).as_ref()).filter_map(server).collect();
            release(servers, req);
            drop(response);
            Ok(out)
        })();
        shutdown();
        got
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribed_output() {
        let out = "Setting breakpad minidump AppID = 108600\n{\"ids\":[\"2392709985\",\"3818460225\"],\"by_state\":false}\n";
        assert_eq!(parse_subscribed(out).unwrap(), listed(&["2392709985", "3818460225"]));
        assert!(parse_subscribed("{\"ids\":[],\"by_state\":false}").unwrap().ids.is_empty());
        assert!(parse_subscribed("").is_err() && parse_subscribed("{\"ids\":[\"../x\"],\"by_state\":false}").is_err());
        // what it reads on stdin: thousands of ids, more than a command line holds
        let many: String = (0..5000u64).map(|i| format!("{}\n", 3_000_000_000 + i)).collect();
        assert!(many.len() > 32_767);
        let items = parse_items(&many).unwrap();
        assert_eq!((items.len(), items[4999]), (5000, 3_000_004_999));
        assert!(parse_items("").unwrap().is_empty() && parse_items("12\n../x\n").is_err());
    }

    fn listed(ids: &[&str]) -> Answer {
        Answer { ids: ids.iter().map(|s| s.to_string()).collect(), by_state: false }
    }

    /// Steam lists nothing (not synced yet), but `GetItemState` says item 42 is subscribed (resubscribed in Steam's own
    /// client, so PZMM's history still says unsubscribed): Remove leaves 42 and takes only the real leftover, 43.
    #[test]
    fn an_item_state_subscribed_stops_remove() {
        let (t, paths) = crate::store::tests::temp_paths();
        let ws = t.path().join("ws");
        for (item, id) in [("42", "ModA"), ("43", "ModB")] {
            std::fs::create_dir_all(ws.join(item).join("mods").join(id)).unwrap();
            std::fs::write(ws.join(item).join("mods").join(id).join("mod.info"), format!("id={id}\n")).unwrap();
        }
        let on_disk = BTreeSet::from(["42".to_string(), "43".to_string()]);
        let mut s = Subscriptions::new();
        // the helper's fake answer: 42 only, from its item state
        assert!(s.refreshed(parse_subscribed("{\"ids\":[\"42\"],\"by_state\":true}").unwrap(), &on_disk, |_| true));
        let ids = ["ModA".to_string(), "ModB".to_string()];
        let (gone, refused, _) = crate::store::remove_leftovers(&paths, Some(&ws), &ids, false, Some(|src: &str| s.leftover(src))).unwrap();
        assert_eq!((gone, refused), (vec!["ModB".to_string()], vec!["ModA".to_string()]));
        assert!(ws.join("42").is_dir() && !ws.join("43").exists());
        // the items outside an item-state answer must be unsubscribed last in PZMM too: 43 isn't, so it's unknown
        let mut s = Subscriptions::new();
        assert!(!s.refreshed(Answer { ids: BTreeSet::from(["42".to_string()]), by_state: true }, &on_disk, |_| false));
        assert!(!s.leftover("steam:43"));
    }

    #[test]
    fn leftovers() {
        let mut s = Subscriptions::new();
        // never asked: folders are trusted
        assert!(!s.leftover("steam:1") && !s.leftover("workshop:2"));
        let disk = BTreeSet::from(["1".to_string(), "2".to_string()]);
        assert!(s.refreshed(listed(&["1"]), &disk, |_| false));
        assert!(!s.leftover("steam:1") && s.leftover("steam:2") && !s.leftover("workshop:2") && !s.leftover("manual"));
        // unsubscribed in PZMM: a leftover at once, even before Steam was ever asked
        let mut fresh = Subscriptions::new();
        fresh.changed("7", false);
        assert!(fresh.leftover("steam:7"));
        s.changed("1", false);
        assert!(s.leftover("steam:1"));
        // subscribed again, here or in Steam
        s.changed("1", true);
        assert!(!s.leftover("steam:1"));
        fresh.refreshed(listed(&["7"]), &disk, |_| false);
        assert!(!fresh.leftover("steam:7"));
        // an empty answer with Workshop folders on disk: unknown, not "every item is a leftover"
        assert!(!s.refreshed(listed(&[]), &disk, |_| false));
        assert!(!s.leftover("steam:2"), "folders trusted again");
        // nor when PZMM unsubscribed only some of them last
        assert!(!s.refreshed(listed(&[]), &disk, |i| i == "1"));
        // with no folders, or every item on disk unsubscribed in PZMM last, nothing subscribed is a real answer
        assert!(s.refreshed(listed(&[]), &BTreeSet::new(), |_| false));
        assert!(s.refreshed(listed(&[]), &disk, |_| true) && s.leftover("steam:1") && s.leftover("steam:2"));
        let mut this_run = Subscriptions::new();
        this_run.changed("1", false);
        // unsubscribed this run (1) and in PZMM's history (2): every item, so the empty answer stands
        assert!(this_run.refreshed(listed(&[]), &disk, |i| i == "2") && this_run.leftover("steam:1"));
    }

    #[test]
    fn ugc_result_layout() {
        assert_eq!((size_of_val(&UgcResult { result: 0, item: 0 }), std::mem::offset_of!(UgcResult, item)), (16, 8));
    }

    #[test]
    fn gameserveritem_layout() {
        use std::mem::{offset_of, size_of};
        // natural alignment on x64, as the Steamworks headers declare it
        assert_eq!(size_of::<NetAdr>(), 8);
        assert_eq!((offset_of!(Item, ping), offset_of!(Item, had_response), offset_of!(Item, game_dir), offset_of!(Item, map), offset_of!(Item, description)), (8, 12, 14, 46, 78));
        assert_eq!((offset_of!(Item, app_id), offset_of!(Item, password), offset_of!(Item, last_played), offset_of!(Item, name), offset_of!(Item, tags), offset_of!(Item, steam_id)), (144, 160, 164, 172, 236, 368));
        assert_eq!(size_of::<Item>(), 376);

        let mut i: Item = unsafe { std::mem::zeroed() };
        assert!(server(&i).is_none(), "no details");
        i.net = NetAdr { connection_port: 16262, query_port: 16261, ip: u32::from(std::net::Ipv4Addr::new(203, 0, 113, 5)) };
        (i.players, i.max_players, i.password) = (7, 32, 1);
        i.name[..4].copy_from_slice(b"Test");
        i.tags[..23].copy_from_slice(b";modded;pvp;VERSION:42.");
        let s = server(&i).unwrap();
        assert_eq!((s.addr.as_str(), s.connect(), s.name.as_str(), s.players, s.max_players, s.password, s.pvp), ("203.0.113.5:16261", "203.0.113.5:16262".to_string(), "Test", 7, 32, true, true));

        // what the helper prints: Steam's own line, then the list
        let out = format!("Setting breakpad minidump AppID = 108600
{}
", serde_json::to_string(&[&s]).unwrap());
        assert_eq!(parse(&out).unwrap(), [s]);
        assert!(parse("").is_err());
    }

    /// One live run against the Steam client on this machine (shows the account as playing PZ for a few seconds).
    #[test]
    #[ignore = "needs Steam running, PZ_GAME_DIR, and SteamAppId=108600 SteamGameId=108600 set as the app's helper sets them"]
    fn live() {
        let game = std::path::PathBuf::from(std::env::var("PZ_GAME_DIR").expect("PZ_GAME_DIR"));
        let start = Instant::now();
        let l = in_process(&game).unwrap();
        eprintln!("{} servers in {:?}; first: {:?}", l.len(), start.elapsed(), l.iter().take(3).map(|s| (&s.addr, s.port, &s.name, s.players, &s.keywords)).collect::<Vec<_>>());
        assert!(!l.is_empty());
    }
}
