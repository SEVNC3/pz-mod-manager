//! Steam's own server list without a Web API key: the running Steam client asked through the
//! game's `steam_api64.dll` (`ISteamMatchmakingServers::RequestInternetServerList`).
//!
//! It runs in a short-lived child process (PZMM's own exe with [`CHILD_ARG`]): the Steam API has to
//! see `SteamAppId` in its environment (setting it in the app, which runs many threads, isn't
//! sound), a fault inside Steam's DLL then can't take the app down, and Steam stops showing the
//! player as "playing Project Zomboid" the moment the child exits, even if `SteamAPI_Shutdown` hangs.

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

/// Steam's internet server list for PZ, from the running Steam client (the game's install `game` has the DLL).
pub fn list(game: &Path) -> Result<Vec<Server>> {
    parse(&helper(&[CHILD_ARG.as_ref(), game.as_os_str()], "Steam didn't send its server list in time")?)
}

/// Subscribe the signed-in Steam account to Workshop item `id` (or unsubscribe with `on` false),
/// through the running Steam client: Steam then downloads (or deletes) it as for any subscription.
pub fn subscribe(game: &Path, id: &str, on: bool) -> Result<()> {
    anyhow::ensure!(crate::sources::steam_workshop::valid_id(id), "not a Workshop item: {id}");
    helper(&[UGC_ARG.as_ref(), game.as_os_str(), (if on { "sub" } else { "unsub" }).as_ref(), id.as_ref()], "Steam didn't answer in time").map(|_| ())
}

/// Run PZMM's exe as a Steam helper with `args`: its stdout, or its last error line.
fn helper(args: &[&std::ffi::OsStr], late: &str) -> Result<String> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.args(args).env("SteamAppId", super::STEAM_APP).env("SteamGameId", super::STEAM_APP).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().context("starting the Steam list helper")?;
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
    if which != CHILD_ARG && which != UGC_ARG {
        return None;
    }
    let game = std::path::PathBuf::from(args.next().unwrap_or_default());
    let got = if which == UGC_ARG {
        let (how, id) = (args.next().unwrap_or_default(), args.next().unwrap_or_default());
        id.to_str().and_then(|i| i.parse::<u64>().ok()).context("a Workshop item id").and_then(|id| ugc(&game, how == "sub", id)).map(|()| String::new())
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
            let ugc = first(&[b"SteamAPI_SteamUGC_v021\0", b"SteamAPI_SteamUGC_v020\0", b"SteamAPI_SteamUGC_v018\0", b"SteamAPI_SteamUGC_v017\0", b"SteamAPI_SteamUGC_v016\0", b"SteamAPI_SteamUGC_v015\0", b"SteamAPI_SteamUGC_v014\0"]).ok_or_else(old)?();
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
