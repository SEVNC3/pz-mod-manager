//! Desktop shell: exposes pzmm-core to the UI as Tauri commands.

use std::collections::BTreeMap;
use std::sync::Mutex;

use pzmm_core::keyed;
use pzmm_core::profile::{self, Line, Profile};
use pzmm_core::pz::{self, Paths, launcher};
use pzmm_core::settings::{self, Settings};
use pzmm_core::sources::{self, Source};
use pzmm_core::store;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

/// Every command reads and writes the same files; run them one at a time.
static LOCK: Mutex<()> = Mutex::new(());

/// The background threads (automation, Discord bots) tick under a read guard; moving the data
/// folder takes the write side so none of them writes to the old folder mid-move (and waits for
/// the automation rounds already on threads of their own: autopilot::wait_rounds).
static TICKING: std::sync::RwLock<()> = std::sync::RwLock::new(());

type Res<T> = Result<T, String>;


/// For commands that may change mods or profiles: takes the locks and drops the scan cache.
fn locked<T>(f: impl FnOnce(&Paths) -> anyhow::Result<T>) -> Res<T> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::detect();
    let _file = pzmm_core::lock_data(&paths).map_err(|e| format!("locking PZMM's data: {e}"))?;
    *SCAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
    f(&paths).map_err(|e| format!("{e:#}"))
}

/// For commands that read (row clicks, status): same locks, but reuses the scan cache. Only writes
/// that never change mods belong here: `active` saving in-game list changes it picks up, `save`.
/// Anything else that writes (profiles, map order, server files, presets) goes through `locked`.
fn reading<T>(f: impl FnOnce(&Paths) -> anyhow::Result<T>) -> Res<T> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::detect();
    let _file = pzmm_core::lock_data(&paths).map_err(|e| format!("locking PZMM's data: {e}"))?;
    f(&paths).map_err(|e| format!("{e:#}"))
}

/// `reading` for the UI's views (row clicks, panels): sync commands each hold one of the async
/// runtime's few threads while they wait, so rather than queue behind a long install they wait a
/// moment, then answer busy (the UI reloads its views when the install ends).
fn quick_read<T>(f: impl FnOnce(&Paths) -> anyhow::Result<T>) -> Res<T> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let _guard = loop {
        match LOCK.try_lock() {
            Ok(g) => break g,
            Err(std::sync::TryLockError::Poisoned(e)) => break e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) if std::time::Instant::now() < until => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(std::sync::TryLockError::WouldBlock) => return Err(keyed!("busy", "PZMM is busy installing or changing mods: try again when it's done")),
        }
    };
    let paths = Paths::detect();
    let _file = pzmm_core::lock_data(&paths).map_err(|e| format!("locking PZMM's data: {e}"))?;
    f(&paths).map_err(|e| format!("{e:#}"))
}

/// The last full mod scan. Scanning walks the Workshop folder and reads every mod.info,
/// and details/poster/conflicts run on each row click, so reads reuse it for a few
/// seconds; anything that changes mods goes through `locked`, which clears it.
static SCAN: Mutex<Option<(std::time::Instant, BTreeMap<String, store::Row>)>> = Mutex::new(None);

fn scan(paths: &Paths) -> anyhow::Result<BTreeMap<String, store::Row>> {
    let mut cache = SCAN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, mods)) = cache.as_ref()
        && at.elapsed() < std::time::Duration::from_secs(10)
    {
        return Ok(mods.clone());
    }
    let mods = store::scan(paths)?;
    *cache = Some((std::time::Instant::now(), mods.clone()));
    Ok(mods)
}

/// Run blocking work (downloads, Workshop requests, file walks) off the UI thread.
async fn blocking<T: Send + 'static, E: std::fmt::Display>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(move || f().map_err(|e| format!("{e:#}"))).await.map_err(|e| e.to_string())?
}

/// Download outside the lock (it can take minutes), then install under it.
fn fetch_then<T>(sources: &[Source], resolve: bool, then: impl FnOnce(&Paths, Vec<String>) -> anyhow::Result<T>) -> Res<T> {
    fetch_then_as(sources, resolve, false, then)
}

/// [`fetch_then`], as an `update` (see `store::update`).
fn fetch_then_as<T>(sources: &[Source], resolve: bool, update: bool, then: impl FnOnce(&Paths, Vec<String>) -> anyhow::Result<T>) -> Res<T> {
    let fetched = sources::fetch(&Paths::detect(), sources, resolve);
    let done = locked(|paths| {
        let ids = sources::install_fetched(paths, &fetched, update);
        then(paths, ids)
    });
    // installs and updates change what's installed: the game's categories follow
    if done.is_ok() && !sources.is_empty() {
        categories_for_game();
    }
    done
}

/// The game's Group by category uses PZMM's categories (Workshop tags), when PZMM - Client is there. In a
/// thread of its own: it asks Steam for the tags, outside the lock.
fn categories_for_game() {
    std::thread::spawn(|| {
        let paths = Paths::detect();
        if pz::bridge::dir(&paths).is_dir()
            && let Ok(mods) = reading(scan)
        {
            pz::bridge::categories_for_game(&paths, &pz::category::categorize(&mods));
        }
    });
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[allow(clippy::large_enum_variant)] // the UI's rows, sent as JSON: a smaller separator saves nothing
enum Row {
    Mod {
        id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        source: String,
        on: bool,
        #[serde(default)]
        missing: bool,
        #[serde(default)]
        requires: Vec<String>,
        #[serde(default)]
        incompatible: Vec<String>,
        #[serde(default)]
        load_after: Vec<String>,
        #[serde(default)]
        load_before: Vec<String>,
        /// Mod Load Order Sorter's loadFirst / loadLast: "on", "category" or "".
        #[serde(default)]
        load_first: String,
        #[serde(default)]
        load_last: String,
        /// The Sorter's rules file pins its place (loadAfter / loadBefore rules).
        #[serde(default)]
        pinned: bool,
        #[serde(default)]
        author: String,
        /// The mod's folders only support Build 41 / only Build 42.
        #[serde(default)]
        only41: bool,
        #[serde(default)]
        only42: bool,
        /// Languages it has texts in (`Translate/<LANG>` folders).
        #[serde(default)]
        languages: Vec<String>,
        /// The profile didn't mention it yet (installed since): appended, off.
        #[serde(default)]
        new: bool,
    },
    Sep {
        name: String,
    },
}

#[derive(Serialize)]
struct View {
    profile: String,
    profiles: Vec<String>,
    /// The library: what's installed, organised by the player.
    rows: Vec<Row>,
    /// The load order: enabled mod ids, as written to the game.
    order: Vec<String>,
    /// The game's mod list was changed in-game and the profile picked that up.
    adopted: bool,
    /// First run: this view created the first profile.
    created: bool,
    zomboid: String,
    workshop: Option<String>,
    /// The build being played: forced in Settings, else what the game's mod list looks like.
    build: Option<u8>,
    /// The build forced in Settings (None: follow the game's mod list).
    forced_build: Option<u8>,
    /// The game version of the last run ("42.19.2") and the Steam branch installed ("public").
    game_version: Option<String>,
    steam_branch: Option<String>,
    /// The game's language (`Translate` folder name, "" if unknown).
    language: String,
    /// Kind of the active profile: "zomboid" or "viewpoint".
    mode: String,
    /// Profile name -> kind, so the profile picker can show one mode's profiles.
    kinds: BTreeMap<String, String>,
    /// Profile name -> the server (its address) it was set up for.
    servers: BTreeMap<String, String>,
}

/// The game's version and Steam branch: they change only with a game update, so they're read once a
/// minute rather than on every view (finding the game folder probes Steam's libraries).
type GameInfo = (std::time::Instant, (Option<String>, Option<String>));
static GAME_INFO: Mutex<Option<GameInfo>> = Mutex::new(None);

fn game_info(paths: &Paths) -> (Option<String>, Option<String>) {
    let mut c = GAME_INFO.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, info)) = c.as_ref().filter(|(at, _)| at.elapsed() < std::time::Duration::from_secs(60)) {
        return info.clone();
    }
    let info = (pz::logs::game_version(&paths.zomboid), pz::zombiebuddy::game_dir().and_then(|g| pz::logs::steam_branch(&g)));
    *c = Some((std::time::Instant::now(), info.clone()));
    info
}

fn view(paths: &Paths) -> anyhow::Result<View> {
    let (game_version, steam_branch) = game_info(paths);
    let created = profile::list(paths).is_empty();
    let (p, adopted) = profile::sync(paths)?;
    let mut mods = scan(paths)?;
    // Mod Load Order Sorter's rules file adds to the mods' own rules
    let rules = pz::managers::rules(paths);
    for (id, m) in mods.iter_mut() {
        let Some(r) = rules.get(id) else { continue };
        for (to, from) in [(&mut m.load_after, &r.load_after), (&mut m.load_before, &r.load_before), (&mut m.incompatible, &r.incompatible)] {
            let new: Vec<String> = from.iter().filter(|x| !to.contains(x)).cloned().collect();
            to.extend(new);
        }
        for (to, from) in [(&mut m.load_first, &r.load_first), (&mut m.load_last, &r.load_last)] {
            if !from.is_empty() {
                *to = from.clone();
            }
        }
    }
    let pinned = |id: &str| rules.get(id).is_some_and(|r| !r.load_after.is_empty() || !r.load_before.is_empty());
    let row = |m: store::Row, on: bool, new: bool| Row::Mod {
        pinned: pinned(&m.id),
        id: m.id,
        name: m.name,
        source: m.source,
        on,
        missing: false,
        requires: m.requires,
        incompatible: m.incompatible,
        load_after: m.load_after,
        load_before: m.load_before,
        load_first: m.load_first,
        load_last: m.load_last,
        author: m.author,
        only41: m.supports_41 && !m.supports_42,
        only42: m.supports_42 && !m.supports_41,
        languages: m.languages,
        new,
    };
    let mut rows = vec![];
    for line in &p.lines {
        match line {
            Line::Separator(name) => rows.push(Row::Sep { name: name.clone() }),
            Line::Mod { id, on } => match mods.remove(id) {
                Some(m) => rows.push(row(m, *on, false)),
                None if *on => rows.push(Row::Mod {
                    id: id.clone(),
                    name: String::new(),
                    source: String::new(),
                    on: true,
                    missing: true,
                    requires: vec![],
                    incompatible: vec![],
                    load_after: vec![],
                    load_before: vec![],
                    load_first: String::new(),
                    load_last: String::new(),
                    pinned: false,
                    author: String::new(),
                    only41: false,
                    only42: false,
                    languages: vec![],
                    new: false,
                }),
                None => {} // disabled and gone: drop it
            },
        }
    }
    let mut rest: Vec<store::Row> = mods.into_values().collect();
    rest.sort_by_key(|m| (if m.name.is_empty() { &m.id } else { &m.name }).to_lowercase());
    // mods the profile doesn't mention yet go at the bottom as loose mods: a nameless separator
    // (a break) ends the last section so they don't look part of it
    let in_section = rows.iter().rev().find_map(|r| if let Row::Sep { name } = r { Some(!name.is_empty()) } else { None }).unwrap_or(false);
    if in_section && !rest.is_empty() {
        rows.push(Row::Sep { name: String::new() });
    }
    rows.extend(rest.into_iter().map(|m| row(m, false, true)));
    // a per-server profile is a Project Zomboid profile like any other
    let mode = Some(profile::kind_of(paths, &p.name)).filter(|k| k == vp::KIND).unwrap_or_else(|| profile::DEFAULT_KIND.into());
    Ok(View {
        order: p.enabled(),
        profile: p.name,
        profiles: profile::list(paths),
        rows,
        adopted,
        created,
        zomboid: paths.zomboid.display().to_string(),
        workshop: pz::workshop_dir().map(|d| d.display().to_string()),
        build: Some(profile::effective_build(paths)),
        forced_build: profile::build(paths),
        game_version,
        steam_branch,
        language: pz::game_language(paths),
        mode,
        kinds: profile::list(paths).into_iter().map(|n| { let kind = profile::kind_of(paths, &n); (n, kind) }).collect(),
        servers: profile::list(paths).into_iter().filter_map(|n| Some((n.clone(), profile::server_of(paths, &n)?))).collect(),
    })
}

fn active(paths: &Paths) -> anyhow::Result<Profile> {
    Ok(profile::sync(paths)?.0)
}

/// Save `p`, make it active and write it to the game: its mods and, from a fresh scan, its maps.
/// Everything that changes enabled mods or what they hold (installs, updates, rollbacks) ends here.
fn apply(paths: &Paths, p: &Profile) -> anyhow::Result<()> {
    profile::apply(paths, p, profile::maps_scan(scan(paths)).as_ref())
}

#[tauri::command(async)]
fn load() -> Res<View> {
    pz::files::forget();
    locked(view)
}

/// Save the library (order, on/off, separators) and the load order to the active profile and the game.
/// `profile` is the one the rows were drawn from: when another is active now, nothing is saved.
#[tauri::command(async)]
fn save(rows: Vec<Row>, order: Vec<String>, profile: String) -> Res<View> {
    // order and on/off only: the installed mods are as scanned, so the scan is kept
    reading(|paths| {
        let (mut p, adopted) = profile::sync(paths)?;
        if adopted {
            // the rows were drawn before the game's mod menu changed the list; saving them would undo that
            anyhow::bail!(keyed!("reloaded", "The mod list was changed in the game's mod menu, so PZMM reloaded it. Make your change again."));
        }
        if p.name != profile {
            anyhow::bail!(keyed!("profileChanged", "The active profile is now \"{now}\", so this change to \"{profile}\" wasn't saved.", now = p.name, profile = profile));
        }
        p.lines = rows
            .into_iter()
            .map(|r| match r {
                Row::Mod { id, on, .. } => Line::Mod { id, on },
                Row::Sep { name } => Line::Separator(name),
            })
            .collect();
        p.order = order;
        apply(paths, &p)?;
        view(paths)
    })
}

/// Enable `ids` in order, leaving off any that declares (or is declared) incompatible with
/// a mod already on: Workshop items often ship alternative versions of one mod.
fn enable_compatible(p: &mut Profile, mods: &BTreeMap<String, store::Row>, ids: &[String]) {
    let clashes = |a: &str, b: &str| mods.get(a).is_some_and(|m| m.incompatible.iter().any(|x| x == b));
    for id in ids {
        match p.enabled().into_iter().find(|o| o != id && (clashes(id, o) || clashes(o, id))) {
            Some(o) => pzmm_core::say!("leftOff", "{id} left off: it's incompatible with {other}. Enable it instead of {other} if you prefer it.", id = id, other = o),
            None => p.set(id, true),
        }
    }
}

#[tauri::command]
async fn install(sources: Vec<String>) -> Res<View> {
    blocking(move || {
        let parsed: Vec<Source> = sources.iter().map(|s| Source::parse(s)).collect();
        fetch_then(&parsed, true, |paths, ids| {
            if ids.is_empty() && !parsed.is_empty() {
                anyhow::bail!(pzmm_core::keyed!("nothingInstalled", "nothing was installed: see the activity log"));
            }
            let mut p = active(paths)?;
            let mods = scan(paths)?;
            profile::follow_case(&mut p, &mods);
            enable_compatible(&mut p, &mods, &ids);
            apply(paths, &p)?;
            view(paths)
        })
    })
    .await
}

#[tauri::command(async)]
fn remove(ids: Vec<String>) -> Res<View> {
    locked(|paths| {
        let refused = store::remove(paths, &ids, settings::load().recycle)?;
        let refused = store::remove_manual(paths, &refused)?; // added outside PZMM: to the Recycle Bin
        for id in &refused {
            pzmm_core::say!("notOurs", "{id}: not installed by PZMM (Workshop subscription or manual install); left alone", id = id);
        }
        let mut p = active(paths)?;
        ids.iter().filter(|i| !refused.contains(i)).for_each(|i| p.forget(i));
        apply(paths, &p)?;
        view(paths)
    })
}

#[tauri::command(async)]
fn rollback(id: String) -> Res<View> {
    locked(|paths| {
        store::rollback(paths, &id)?;
        pzmm_core::say!("rolledBack", "{id}: restored the previous version", id = id);
        // the old version may add other map folders, or spell its id in another case
        let mut p = active(paths)?;
        profile::follow_case(&mut p, &scan(paths)?);
        apply(paths, &p)?;
        view(paths)
    })
}

#[tauri::command(async)]
fn profile_new(name: String) -> Res<View> {
    locked(|paths| {
        if profile::exists(paths, &name) {
            anyhow::bail!(keyed!("profileExists", "a profile named \"{name}\" already exists", name = name));
        }
        // empty, for the same game as the active one (Duplicate copies a profile)
        let viewpoint = profile::kind_of(paths, &active(paths)?.name) == vp::KIND;
        // a new profile plays in Zomboid
        same_folder_while_playing(paths, &name, &paths.zomboid.join("Lua").join("pzmm"))?;
        let p = if viewpoint { vp::fresh(&name) } else { Profile::parse(&name, "") };
        profile::save(paths, &p)?;
        profile::set_kind(paths, &name, if viewpoint { vp::KIND } else { profile::DEFAULT_KIND })?;
        apply(paths, &p)?;
        pzmm_core::say!("profileNew", "Made the empty profile \"{name}\" and switched to it.", name = name);
        view(paths)
    })
}

/// The running game stays in its own folder: a change that moves PZMM's side to another (`to`) would leave it talking to a
/// folder PZMM no longer reads, and its Accept would write the old mods into the new profile.
fn same_folder_while_playing(paths: &Paths, name: &str, to: &std::path::Path) -> anyhow::Result<()> {
    if pz::host::game_running() && !pz::bridge::same_dir(to, &pz::bridge::game_dir(paths)) {
        anyhow::bail!(pzmm_core::keyed!("profileOtherFolder", "{name} plays in another game folder: quit the game first, then Play from PZMM", name = name));
    }
    Ok(())
}

#[tauri::command(async)]
fn profile_use(name: String) -> Res<View> {
    locked(|paths| {
        // (apply keeps what the game changed in the profile being left)
        let p = profile::load(paths, &name)?;
        // the running game stays in its own folder: switching to a profile that plays in another would leave it
        // talking to a folder PZMM no longer reads, and its Accept would write the old mods into the new profile
        same_folder_while_playing(paths, &p.name, &pz::bridge::dir_for(paths, &p.name))?;
        apply(paths, &p)?;
        pzmm_core::say!("profileUsed", "Switched to the profile \"{name}\".", name = p.name);
        view(paths)
    })
}

/// Rename profile `from` to `to` (any profile, the active one included).
#[tauri::command(async)]
fn profile_rename(from: String, to: String) -> Res<View> {
    locked(|paths| {
        profile::rename(paths, &from, to.trim())?;
        pzmm_core::say!("profileRenamed", "Renamed the profile \"{from}\" to \"{to}\".", from = from, to = to.trim());
        view(paths)
    })
}

/// Copy profile `from` as `to`, staying on the active profile.
#[tauri::command(async)]
fn profile_duplicate(from: String, to: String) -> Res<View> {
    locked(|paths| {
        profile::duplicate(paths, &from, to.trim())?;
        pzmm_core::say!("profileCopied", "Copied the profile \"{from}\" as \"{to}\".", from = from, to = to.trim());
        view(paths)
    })
}

/// Keep the active profile as it's saved now, before a change labelled `label` (as saved: in-game
/// changes are left for the next view to pick up, so it can say so).
#[tauri::command(async)]
fn snapshot_create(label: String) -> Res<()> {
    locked(|paths| profile::snapshot(paths, &profile::active_name(paths), &label))
}

/// The active profile's snapshots, newest first.
#[tauri::command(async)]
fn snapshot_list() -> Res<Vec<profile::SnapshotInfo>> {
    quick_read(|paths| Ok(profile::snapshots(paths, &profile::active_name(paths))))
}

/// Put the active profile back as snapshot `id` had it (its current state is snapshotted first).
#[tauri::command(async)]
fn snapshot_restore(id: String) -> Res<View> {
    locked(|paths| {
        let p = profile::restore(paths, &active(paths)?.name, &id)?;
        apply(paths, &p)?;
        view(paths)
    })
}

#[tauri::command(async)]
fn profile_delete(name: String) -> Res<View> {
    locked(|paths| {
        profile::delete(paths, &name)?;
        pzmm_core::say!("profileDeleted", "Deleted the profile \"{name}\".", name = name);
        view(paths)
    })
}

/// Server profiles you no longer use (see profile::unused_server_profiles), to ask before pruning them.
#[tauri::command(async)]
fn profiles_unused() -> Res<Vec<String>> {
    quick_read(|paths| Ok(profile::unused_server_profiles(paths)))
}

/// Delete the profiles in `names` that are still unused server profiles (as profile_delete).
#[tauri::command(async)]
fn profiles_prune(names: Vec<String>) -> Res<View> {
    locked(|paths| {
        let unused = profile::unused_server_profiles(paths);
        for name in names.iter().filter(|n| unused.contains(n)) {
            profile::delete(paths, name)?;
            pzmm_core::say!("profileDeleted", "Deleted the profile \"{name}\".", name = name);
        }
        view(paths)
    })
}

#[tauri::command(async)]
fn set_build(build: Option<u8>) -> Res<View> {
    locked(|paths| {
        profile::set_b41(paths, build.map(|b| b == 41))?;
        apply(paths, &active(paths)?)?;
        view(paths)
    })
}

#[derive(Serialize)]
struct Details {
    id: String,
    name: String,
    source: String,
    dir: String,
    /// mod.info fields (description, author, url, versionMin, require, ...)
    info: BTreeMap<String, String>,
    ours: bool,
    has_backup: bool,
    /// It adds maps on the build being played.
    maps: bool,
}

#[tauri::command(async)]
fn details(id: String) -> Res<Option<Details>> {
    quick_read(|paths| {
        let Some(m) = scan(paths)?.remove(&id) else { return Ok(None) };
        let ours = store::load(paths)?.contains_key(&id);
        Ok(Some(Details {
            info: pz::read_info(&m.info).into_iter().collect(),
            has_backup: store::has_backup(paths, &id),
            maps: !pz::maps::folder_names(&m.dir, profile::effective_build(paths)).is_empty(),
            dir: m.dir.display().to_string(),
            id: m.id,
            name: m.name,
            source: m.source,
            ours,
        }))
    })
}

/// The mod's poster image as raw bytes (empty if none).
#[tauri::command(async)]
fn poster(id: String) -> Res<tauri::ipc::Response> {
    quick_read(|paths| {
        let Some(m) = scan(paths)?.remove(&id) else { return Ok(vec![]) };
        let name = pz::read_info(&m.info).remove("poster").unwrap_or_else(|| "poster.png".into());
        let base = m.info.parent().unwrap_or(&m.dir);
        // poster names come from mod.info: stay inside the mod folder
        let file = [base.join(&name), m.dir.join(&name)].into_iter().find(|p| p.is_file() && p.starts_with(&m.dir) && !name.contains(".."));
        Ok(file.and_then(|f| std::fs::read(f).ok()).unwrap_or_default())
    })
    .map(tauri::ipc::Response::new)
}

#[derive(Serialize)]
struct BrowseItem {
    #[serde(flatten)]
    item: sources::steam_workshop::Item,
    /// Already loadable: subscribed in Steam or installed by PZMM.
    have: bool,
}

#[derive(Serialize)]
struct Browse {
    total: u64,
    items: Vec<BrowseItem>,
}

fn with_have(items: Vec<sources::steam_workshop::Item>) -> Vec<BrowseItem> {
    let have = sources::workshop_available(&Paths::detect());
    items.into_iter().map(|item| BrowseItem { have: have.contains(&item.id), item }).collect()
}

/// Details of many items, in their order (the API takes 100 at a time).
fn details_of(ids: &[String]) -> anyhow::Result<Vec<sources::steam_workshop::Item>> {
    let mut out = vec![];
    for chunk in ids.chunks(100) {
        let refs: Vec<&str> = chunk.iter().map(String::as_str).collect();
        let mut got = sources::steam_workshop::details(&refs)?;
        got.sort_by_key(|i| chunk.iter().position(|c| *c == i.id));
        out.extend(got);
    }
    Ok(out)
}

#[tauri::command]
async fn workshop_browse(query: sources::steam_workshop::Query) -> Res<Browse> {
    blocking(move || -> anyhow::Result<_> {
        let res = sources::steam_workshop::browse(pz::STEAM_APP, &query)?;
        Ok(Browse { total: res.total, items: with_have(res.items) })
    })
    .await
}

#[derive(Serialize)]
struct WorkshopItemFull {
    item: Option<BrowseItem>,
    screenshots: Vec<String>,
    videos: Vec<String>,
    required: Vec<BrowseItem>,
    /// For a collection: its items, in order.
    members: Vec<BrowseItem>,
}

/// Everything for the item panel: full description and counts, screenshots, required items
/// and, for a collection, its members.
#[tauri::command]
async fn workshop_info(id: String) -> Res<WorkshopItemFull> {
    blocking(move || -> anyhow::Result<_> {
        let item = sources::steam_workshop::details(&[id.as_str()])?.into_iter().next();
        // the page only adds screenshots, videos and required items: without it the panel still has the details
        let page = sources::steam_workshop::item_page(&id).unwrap_or_default();
        let members = if item.as_ref().is_some_and(|i| i.collection) {
            sources::steam_workshop::collections(&[id.as_str()])?.remove(&id).unwrap_or_default()
        } else {
            vec![]
        };
        Ok(WorkshopItemFull {
            item: with_have(item.into_iter().collect()).pop(),
            screenshots: page.screenshots,
            videos: page.videos,
            required: with_have(details_of(&page.required)?),
            members: with_have(details_of(&members)?),
        })
    })
    .await
}

#[tauri::command]
async fn workshop_changes(id: String, page: u32) -> Res<Vec<sources::steam_workshop::ChangeNote>> {
    blocking(move || sources::steam_workshop::change_notes(&id, page)).await
}

/// The Workshop change notes since the installed version of mod `id` (PZMM's Workshop installs only).
#[tauri::command]
async fn mod_changes(id: String) -> Res<Vec<sources::steam_workshop::ChangeNote>> {
    blocking(move || -> Res<_> {
        let found = reading(|paths| {
            let index = store::load(paths)?;
            Ok(index.get(&id).and_then(|e| Some((e.source.strip_prefix("workshop:")?.to_string(), sources::installed_at(paths, &id, e)))))
        })?;
        match found {
            Some((item, since)) => sources::steam_workshop::change_notes_since(&item, since).map_err(|e| format!("{e:#}")),
            None => Ok(vec![]),
        }
    })
    .await
}

/// Your Workshop mods updated since you last played; with `notes`, their change notes too.
#[tauri::command]
async fn feed(notes: bool) -> Res<pz::feed::Feed> {
    blocking(move || -> Res<_> {
        let mods = reading(scan)?;
        // the network part runs outside the lock: only the feed's own cache is written
        pz::feed::feed(&Paths::detect(), &mods, notes).map_err(|e| format!("{e:#}"))
    })
    .await
}

/// Project Zomboid's official news on Steam (kept an hour).
#[tauri::command]
async fn news() -> Res<pz::news::News> {
    blocking(|| pz::news::news(&Paths::detect())).await
}

#[tauri::command]
async fn workshop_comments(creator: String, id: String, start: u32) -> Res<sources::steam_workshop::Comments> {
    blocking(move || sources::steam_workshop::comments(&creator, &id, start)).await
}

/// Workshop items by id (titles, preview images...), in the order asked.
#[tauri::command]
async fn workshop_items(ids: Vec<String>) -> Res<Vec<sources::steam_workshop::Item>> {
    blocking(move || details_of(&ids)).await
}

/// One page of an author's Project Zomboid items.
#[tauri::command]
async fn workshop_author(creator: String, page: u32) -> Res<Browse> {
    blocking(move || -> anyhow::Result<_> {
        let (total, ids) = sources::steam_workshop::author_items(pz::STEAM_APP, &creator, page)?;
        Ok(Browse { total, items: with_have(details_of(&ids)?) })
    })
    .await
}

// --- making mods: checks and the Workshop upload folder ---

use pzmm_core::pz::authoring;

#[derive(Serialize)]
struct AuthoringReport {
    /// The folder is a Workshop upload folder (else a mod, or a folder of mods).
    upload: bool,
    findings: Vec<pz::doctor::Finding>,
    /// A starting workshop.txt for it.
    draft: authoring::Upload,
    /// The Workshop tags the game offers (empty when the game wasn't found).
    tags: Vec<String>,
    /// Where the game looks for upload folders (Zomboid/Workshop): where the folder picker starts.
    workshop: String,
    has_poster: bool,
}

#[tauri::command]
async fn authoring_check(dir: String) -> Res<AuthoringReport> {
    blocking(move || -> anyhow::Result<_> {
        let dir = std::path::PathBuf::from(dir);
        let tags = authoring::allowed_tags(zb::game_dir().as_deref());
        let workshop = Paths::detect().zomboid.join("Workshop");
        let existing = dir.file_name().map(|n| workshop.join(n));
        Ok(AuthoringReport {
            upload: authoring::is_upload_folder(&dir),
            findings: authoring::check(&dir, &tags),
            draft: authoring::draft(&dir, existing.as_deref()),
            tags,
            workshop: workshop.display().to_string(),
            has_poster: authoring::poster(&dir).is_some(),
        })
    })
    .await
}

/// The poster of the mod in `dir` as raw bytes (empty if none), for the Workshop preview.
#[tauri::command(async)]
fn authoring_poster(dir: String) -> tauri::ipc::Response {
    tauri::ipc::Response::new(authoring::poster(std::path::Path::new(&dir)).and_then(|p| std::fs::read(p).ok()).unwrap_or_default())
}

#[derive(Serialize)]
struct Built {
    folder: String,
    /// Files left out (hidden files, types the uploader refuses).
    skipped: Vec<String>,
    findings: Vec<pz::doctor::Finding>,
}

/// Make or refresh the upload folder for the mod in `dir` inside `parent` (a folder the player picked).
#[tauri::command]
async fn authoring_build(dir: String, parent: String, upload: authoring::Upload, preview: Option<Vec<u8>>) -> Res<Built> {
    blocking(move || -> anyhow::Result<_> {
        let (folder, skipped) = authoring::build(std::path::Path::new(&dir), std::path::Path::new(&parent), &upload, preview.as_deref())?;
        let findings = authoring::check(&folder, &authoring::allowed_tags(zb::game_dir().as_deref()));
        Ok(Built { folder: folder.display().to_string(), skipped, findings })
    })
    .await
}

// --- ZombieBuddy (Java mods) ---

use pzmm_core::pz::zombiebuddy as zb;

#[derive(Serialize)]
struct ZbStatus {
    game_dir: Option<String>,
    /// The ZombieBuddy mod is available (subscribed or installed).
    have_mod: bool,
    files_current: bool,
    launcher_patched: bool,
    enabled: bool,
    /// Enabled mods whose mod.info requires ZombieBuddy.
    required_by: Vec<String>,
    /// Other Java mod loaders installed for the game (Leaf, Avrix).
    loaders: Vec<pz::javamods::Loader>,
}

/// (launcher folder, ZombieBuddy's libs folder, every mod)
type ZbPaths = (Option<std::path::PathBuf>, Option<std::path::PathBuf>, BTreeMap<String, store::Row>);

fn zb_paths(paths: &Paths) -> anyhow::Result<ZbPaths> {
    let mods = scan(paths)?;
    let (launcher, libs) = zb::locate(&mods);
    Ok((launcher, libs, mods))
}

#[tauri::command(async)]
fn zb_status() -> Res<ZbStatus> {
    quick_read(|paths| {
        let (launcher, libs, mods) = zb_paths(paths)?;
        let enabled = active(paths)?.enabled();
        let required_by = zb::required_by(&enabled, &mods);
        Ok(ZbStatus {
            game_dir: launcher.as_ref().map(|d| d.display().to_string()),
            have_mod: libs.is_some(),
            files_current: launcher.as_deref().zip(libs.as_deref()).is_some_and(|(l, s)| zb::files_current(l, s)),
            launcher_patched: launcher.as_deref().is_some_and(zb::launcher_patched),
            enabled: enabled.iter().any(|i| i == zb::MOD_ID),
            required_by,
            loaders: zb::game_dir().map(|g| pz::javamods::other_loaders(&g, pz::javamods::steam_launch_options)).unwrap_or_default(),
        })
    })
}

/// Setting up ZombieBuddy enables it in the active profile, but from the Viewpoint card (`viewpoint`)
/// only when that's the Viewpoint profile: a plain one stays as it is.
fn zb_enables(paths: &Paths, viewpoint: bool) -> bool {
    !viewpoint || profile::kind_of(paths, &profile::active_name(paths)) == vp::KIND
}

/// What setting up (or removing) ZombieBuddy would change, for the confirmation dialog.
#[tauri::command(async)]
fn zb_preview(remove: bool, viewpoint: bool) -> Res<Vec<String>> {
    quick_read(|paths| {
        let (launcher, libs, _) = zb_paths(paths)?;
        let Some(l) = launcher else { anyhow::bail!("Project Zomboid's game folder wasn't found in your Steam libraries") };
        if remove {
            return zb::uninstall(&l, true);
        }
        let mut lines = match libs {
            Some(s) => zb::install(&l, &s, true)?,
            None => vec![
                keyed!("zbDownload", "download ZombieBuddy (Workshop item {item}) into Zomboid/mods", item = zb::WORKSHOP_ID),
                zb::copy_agent(&l),
                zb::add_flag(&l),
            ],
        };
        if zb_enables(paths, viewpoint) {
            lines.push(keyed!("zbEnable", "enable ZombieBuddy in the active profile"));
        }
        Ok(lines)
    })
}

#[tauri::command]
async fn zb_install(viewpoint: bool) -> Res<View> {
    blocking(move || {
        let need_mod = reading(|paths| Ok(zb_paths(paths)?.1.is_none()))?;
        let fetch: Vec<Source> = if need_mod { vec![Source::Workshop(zb::WORKSHOP_ID.into())] } else { vec![] };
        fetch_then(&fetch, false, |paths, _| {
            let (launcher, libs, _) = zb_paths(paths)?;
            let Some(l) = launcher else { anyhow::bail!("Project Zomboid's game folder wasn't found in your Steam libraries") };
            let Some(s) = libs else { anyhow::bail!("couldn't get the ZombieBuddy mod from the Workshop") };
            for c in zb::install(&l, &s, false)? {
                pzmm_core::log!("{}", pzmm_core::unkeyed(&c));
            }
            if zb_enables(paths, viewpoint) {
                let mut p = active(paths)?;
                p.set(zb::MOD_ID, true);
                apply(paths, &p)?;
            }
            pzmm_core::say!("zbSetUp", "ZombieBuddy is set up. Java mods ask for your approval the first time they load.");
            view(paths)
        })
    })
    .await
}

/// The enabled Java mods ZombieBuddy would load: hash, signature, its saved answer, scan.
/// Hashing and scanning the jars runs outside the lock (each jar is read once per change, see javamods).
#[tauri::command]
async fn java_mods() -> Res<Vec<pz::javamods::JavaMod>> {
    use pz::javamods as jm;
    blocking(|| {
        let (mods, ids) = quick_read(|paths| Ok((scan(paths)?, active(paths)?.enabled()))).map_err(anyhow::Error::msg)?;
        let authors = jm::known_authors(&mods);
        let approvals = jm::zb_config_dir().map(|d| jm::read_approvals(&d.join("mod_approvals.json"))).unwrap_or_default();
        let enabled: Vec<&store::Row> = ids.iter().filter_map(|id| mods.get(id)).collect();
        let mut out: Vec<jm::JavaMod> =
            enabled.iter().filter(|m| !(m.id == zb::MOD_ID && zb::official_source(&m.source))).filter_map(|m| jm::inspect(m, &authors, &approvals)).collect();
        // and what Leaf and Avrix load
        out.extend(jm::loader_mods(zb::game_dir().as_deref(), enabled));
        Ok::<_, anyhow::Error>(out)
    })
    .await
}

/// Subscribe to (or unsubscribe from) a Workshop item through the running Steam client.
/// `mod_id`/`name`: the mod it is, for the download history (else the item's id).
#[tauri::command]
async fn workshop_subscribe(id: String, on: bool, mod_id: Option<String>, name: Option<String>) -> Res<View> {
    blocking(move || workshop_subscribe_now(&id, on, mod_id, name)).await
}

fn workshop_subscribe_now(id: &str, on: bool, mod_id: Option<String>, name: Option<String>) -> Res<View> {
    let game = zb::game_dir().ok_or("Project Zomboid's game folder wasn't found in your Steam libraries")?;
    pz::steam_client::subscribe(&game, id, on).map_err(|e| format!("{e:#}"))?;
    pzmm_core::history::add(&Paths::detect(), mod_id.as_deref().unwrap_or(id), name.as_deref().unwrap_or(""), &format!("steam:{id}"), if on { "subscribed" } else { "unsubscribed" });
    if on {
        pzmm_core::say!("subscribed", "Subscribed to {id} in Steam: Steam downloads it and keeps it updated", id = id);
    } else {
        pzmm_core::say!("unsubscribed", "Unsubscribed from {id} in Steam: Steam removes its files", id = id);
    }
    locked(view)
}

#[tauri::command(async)]
fn zb_uninstall() -> Res<View> {
    locked(|paths| {
        let Some(l) = zb_paths(paths)?.0 else { anyhow::bail!("Project Zomboid's game folder wasn't found in your Steam libraries") };
        for c in zb::uninstall(&l, false)? {
            pzmm_core::log!("{}", pzmm_core::unkeyed(&c));
        }
        view(paths)
    })
}

// --- missing requirements ---

/// Workshop items that may provide `mod_id`, which the installed mod `needed_by` requires.
#[tauri::command]
async fn find_requirement(mod_id: String, needed_by: String) -> Res<Vec<sources::Candidate>> {
    blocking(move || {
        let (hint, build) = reading(|paths| Ok((scan(paths).ok().and_then(|m| m.get(&needed_by).and_then(|r| sources::workshop_item(&r.source)).map(String::from)), profile::effective_build(paths)))).map_err(anyhow::Error::msg)?;
        sources::find_mod(&mod_id, hint.as_deref(), build)
    })
    .await
}

/// Install every enabled mod, and requirement of one, that isn't installed, when the Workshop
/// has a confirmed match (its description declares the mod id). Returns ids still missing.
#[tauri::command]
async fn install_missing() -> Res<(View, Vec<String>)> {
    blocking(|| {
        // (missing id, Workshop item of a mod that needs it), read under the lock
        let (wanted, build) = reading(|paths| {
            let mods = scan(paths)?;
            let mut wanted: Vec<(String, Option<String>)> = vec![];
            for id in active(paths)?.enabled() {
                let Some(m) = mods.get(&id) else {
                    // enabled but not installed: it's missing too
                    if !wanted.iter().any(|(w, _)| *w == id) {
                        wanted.push((id, None));
                    }
                    continue;
                };
                for r in &m.requires {
                    if !mods.contains_key(r) && !wanted.iter().any(|(w, _)| w == r) {
                        wanted.push((r.clone(), sources::workshop_item(&m.source).map(String::from)));
                    }
                }
            }
            Ok((wanted, profile::effective_build(paths)))
        })?;
        // look up and download without holding the lock
        let (mut items, mut unresolved) = (vec![], vec![]);
        let needed: Vec<String> = wanted.iter().map(|(id, _)| id.clone()).collect();
        for (id, hint) in wanted {
            match sources::find_mod(&id, hint.as_deref(), build) {
                Ok(c) if c.first().is_some_and(|c| c.sure()) => {
                    pzmm_core::say!("found", "{id}: found on the Workshop as \"{title}\" ({item})", id = id, title = c[0].item.title, item = c[0].item.id);
                    items.push(Source::Workshop(c[0].item.id.clone()));
                }
                Ok(_) => unresolved.push(id),
                Err(e) => {
                    pzmm_core::say!("lookupFailed", "error: looking up {id}: {error}", id = id, error = format!("{e:#}"));
                    unresolved.push(id);
                }
            }
        }
        fetch_then(&items, true, |paths, _| {
            // only what was missing and what that needs: an item's other mods are often optional add-ons
            let mods = scan(paths)?;
            let mut p = active(paths)?;
            let (mut todo, mut seen) = (needed, std::collections::HashSet::new());
            let mut enable = vec![];
            while let Some(id) = todo.pop() {
                if let Some(m) = mods.get(&id)
                    && seen.insert(id.clone())
                {
                    todo.extend(m.requires.iter().cloned());
                    enable.push(id);
                }
            }
            enable.reverse();
            enable_compatible(&mut p, &mods, &enable);
            apply(paths, &p)?;
            for id in &unresolved {
                pzmm_core::say!("noClearMatch", "{id}: no Workshop item clearly provides it. Use Download in the mod's details to pick one.", id = id);
            }
            Ok((view(paths)?, unresolved))
        })
    })
    .await
}

// --- updates, conflicts, servers, saves ---

#[tauri::command]
async fn check_updates() -> Res<Vec<String>> {
    blocking(|| sources::check_updates(&Paths::detect())).await
}

/// Re-download specific mods PZMM installed. Also returns the ids that did install (failures are only logged).
#[tauri::command]
async fn update_mods(ids: Vec<String>) -> Res<(View, Vec<String>)> {
    blocking(move || {
        let index = locked(store::load)?;
        let mut seen = std::collections::HashSet::new();
        let todo: Vec<Source> = ids
            .iter()
            .filter_map(|id| index.get(id))
            .filter(|e| seen.insert(e.source.clone()))
            .filter_map(|e| Source::from_index(&e.source))
            .collect();
        fetch_then_as(&todo, false, true, |paths, installed| {
            // an update can add or rename map folders, or change a mod id's case
            let mut p = active(paths)?;
            profile::follow_case(&mut p, &scan(paths)?);
            apply(paths, &p)?;
            Ok((view(paths)?, installed))
        })
    })
    .await
}

#[tauri::command(async)]
fn conflicts(id: String) -> Res<Vec<pz::files::Conflict>> {
    // collect paths under the lock, walk their files after it (other commands wait on the lock)
    let found = quick_read(|paths| {
        let mods = scan(paths)?;
        let enabled = active(paths)?.enabled();
        let Some(mine) = mods.get(&id) else { return Ok(None) };
        let my_pos = enabled.iter().position(|e| *e == id).unwrap_or(usize::MAX);
        let others: Vec<(String, std::path::PathBuf, usize)> = enabled
            .iter()
            .enumerate()
            .filter(|(_, e)| **e != id)
            .filter_map(|(pos, e)| mods.get(e).map(|m| (e.clone(), m.dir.clone(), pos)))
            .collect();
        Ok(Some((mine.dir.clone(), my_pos, others)))
    })?;
    Ok(found.map(|(dir, pos, others)| pz::files::conflicts(&dir, pos, &others)).unwrap_or_default())
}

/// Folder size and newest file time of every mod (walks all their files, so it's
/// fetched separately from the list).
#[tauri::command]
async fn mod_stats() -> Res<BTreeMap<String, pz::files::Stats>> {
    blocking(|| -> Res<_> {
        let mods = reading(scan)?;
        Ok(mods.into_iter().map(|(id, m)| (id, pz::files::stats(&m.dir))).collect())
    })
    .await
}

#[derive(Serialize)]
struct Categories {
    /// Section names in list order.
    order: &'static [&'static str],
    /// Mod id -> section name.
    of: BTreeMap<String, String>,
    /// Mod id -> what kind of mod it is (framework, patch, map, ui, translation).
    kinds: BTreeMap<String, Vec<&'static str>>,
}

/// Every mod's category (asks Steam for Workshop tags, so outside the lock).
#[tauri::command]
async fn categories() -> Res<Categories> {
    blocking(|| -> Res<_> {
        let mods = reading(scan)?;
        let of = pz::category::categorize(&mods);
        pz::bridge::categories_for_game(&Paths::detect(), &of);
        Ok(Categories { order: pz::category::ORDER, of, kinds: pz::category::kinds(&mods) })
    })
    .await
}

/// For every enabled mod: which enabled mods it overrides, and is overridden by.
#[tauri::command]
async fn conflict_summary() -> Res<std::collections::HashMap<String, pz::files::Overlap>> {
    blocking(|| -> Res<_> {
        let order: Vec<(String, std::path::PathBuf)> = reading(|paths| {
            let mods = scan(paths)?;
            Ok(active(paths)?.enabled().into_iter().filter_map(|id| mods.get(&id).map(|m| (id, m.dir.clone()))).collect())
        })?;
        let found = pz::files::overlaps(&order);
        pz::bridge::overlaps_for_game(&Paths::detect(), &found);
        Ok(found)
    })
    .await
}

#[derive(Serialize)]
struct ServersAndSaves {
    servers: Vec<String>,
    server_dir: String,
    saves: Vec<pz::saves::Save>,
    /// Saves that are gone but have backups: (mode, name).
    deleted: Vec<(String, String)>,
    /// The active profile's own game folder, whose saves these are (None: the Zomboid folder's).
    own_folder: Option<String>,
}

/// The folders the active profile plays in: its own game folder when it has one (`-cachedir`),
/// where the game keeps that profile's saves.
fn played(paths: &Paths) -> Paths {
    let own = profile::launch_of(paths, &profile::active_name(paths)).cache_dir;
    Paths { zomboid: own.unwrap_or_else(|| paths.zomboid.clone()), data: paths.data.clone() }
}

#[tauri::command(async)]
fn servers_and_saves() -> Res<ServersAndSaves> {
    quick_read(|paths| {
        let played = played(paths);
        Ok(ServersAndSaves {
            servers: pz::server::list(paths),
            server_dir: pz::server::dir(paths).display().to_string(),
            saves: pz::saves::list(&played),
            // gone from the folder played (its backups are kept under that folder's tag)
            deleted: pz::saves::deleted_with_backups(&played),
            own_folder: (played.zomboid != paths.zomboid).then(|| played.zomboid.display().to_string()),
        })
    })
}

/// The `Map=` list writing to `Server/<name>.ini` would set (empty when no enabled mod adds a
/// map): in the profile's arranged order, else as the config has them.
#[tauri::command(async)]
fn server_maps(name: String) -> Res<Vec<String>> {
    quick_read(|paths| {
        let mut order = profile::map_order(paths, &profile::active_name(paths));
        if order.is_empty() {
            order = pz::server::read(&pz::server::path(paths, &name)?).map(|c| c.maps).unwrap_or_default();
        }
        Ok(pz::server::map_order(&order, &pz::server::mod_maps(&active(paths)?.enabled(), &scan(paths)?, pz::server::build(paths, &name))))
    })
}

/// Write the active profile into `Server/<name>.ini`, its maps in `maps` order (empty: as the config has them). Returns notes for the user.
#[tauri::command(async)]
fn server_export(name: String, maps: Vec<String>) -> Res<Vec<String>> {
    locked(|paths| {
        let notes = pz::server::export(paths, &name, &active(paths)?.enabled(), &scan(paths)?, Some(maps.as_slice()).filter(|m| !m.is_empty()))?;
        if !maps.is_empty() {
            profile::set_map_order(paths, &profile::active_name(paths), &maps)?;
        }
        notes.iter().for_each(|n| pzmm_core::log!("{n}"));
        Ok(notes)
    })
}

/// Problems with what writing the active profile to `<name>.ini` would set: the check Hosting's Mods page runs.
#[tauri::command]
async fn server_check(name: String, maps: Vec<String>) -> Res<Vec<pz::server::Problem>> {
    blocking(move || {
        let lines = reading(|paths| pz::server::planned(paths, &name, &active(paths)?.enabled(), &scan(paths)?, Some(maps.as_slice()).filter(|m| !m.is_empty()))).map_err(anyhow::Error::msg)?;
        check_lines(&lines).map(|r| r.problems)
    })
    .await
}

/// What writing the active profile to `Server/<name>.ini` would change (see `server_export`).
#[tauri::command(async)]
fn server_preview(name: String, maps: Vec<String>) -> Res<pz::server::Preview> {
    quick_read(|paths| pz::server::preview(paths, &name, &active(paths)?.enabled(), &scan(paths)?, Some(maps.as_slice()).filter(|m| !m.is_empty())))
}

/// Make a profile from a server config (installing missing Workshop items) and use it.
#[tauri::command]
async fn server_import(name: String) -> Res<View> {
    blocking(move || {
        let (cfg, missing) = locked(|paths| {
            let cfg = pz::server::read(&pz::server::path(paths, &name)?)?;
            let have = sources::workshop_available(paths);
            let missing: Vec<Source> = cfg.workshop.iter().filter(|w| !have.contains(*w)).map(|w| Source::Workshop(w.clone())).collect();
            Ok((cfg, missing))
        })?;
        if !missing.is_empty() {
            pzmm_core::say!("serverItems", "installing {count} Workshop item(s) the server uses", count = missing.len());
        }
        fetch_then(&missing, false, |paths, _| {
            apply(paths, &profile::for_server(paths, &name, &format!("ini:{name}"), cfg.mods, &cfg.maps)?)?;
            view(paths)
        })
    })
    .await
}

// --- hosting dedicated servers (pz::host). Outside `locked`: servers.json takes PZMM's data lock
// itself only while it writes, and starting, stopping and installing take minutes ---

#[derive(Serialize)]
struct HostView {
    servers: Vec<pz::host::Card>,
    /// Memory installed, and the starting -Xmx it suggests (MB).
    ram_mb: u64,
    memory_mb: u32,
    /// Where a new server installs by default, per build.
    install_b42: String,
    install_41: String,
    zomboid: String,
    /// A dedicated server already installed (Steam's tool or the setting), for adopting configs.
    server_dir: Option<String>,
}

#[tauri::command(async)]
fn host_view() -> Res<HostView> {
    let paths = Paths::detect();
    let ram = pz::host::total_ram_mb();
    Ok(HostView {
        servers: pz::host::cards(&paths),
        ram_mb: ram,
        memory_mb: pz::host::default_memory_mb(ram),
        install_b42: pz::host::default_install_dir(&paths, "b42").display().to_string(),
        install_41: pz::host::default_install_dir(&paths, pz::host::LEGACY).display().to_string(),
        zomboid: paths.zomboid.display().to_string(),
        server_dir: pz::folders::path("server", &settings::load()).filter(|d| pz::host::installed(d)).map(|d| d.display().to_string()),
    })
}

/// Which of `ports` are free: (port, UDP) pairs; RCON is TCP.
#[tauri::command(async)]
fn host_ports(ports: Vec<(u16, bool)>) -> Vec<bool> {
    ports.into_iter().map(|(p, udp)| pz::host::port_free(p, udp)).collect()
}

/// The setup wizard's last step: install (SteamCMD, when the folder has no server), write the ini,
/// then the first start.
#[tauri::command]
async fn host_create(new: pz::host::NewServer) -> Res<()> {
    blocking(move || {
        SERVER_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
        let paths = Paths::detect();
        let s = pz::host::create(&paths, &new, &SERVER_STOP)?;
        pzmm_core::say!("serverCreated", "Set up {name}: {file}", name = s.name, file = s.ini(&paths).display());
        pz::host::start(&paths, &s.name, None)
    })
    .await
}

/// What Check setup finds in server `name`'s config, asked before it starts.
#[tauri::command(async)]
fn host_preflight(name: String) -> Res<Vec<pz::doctor::Finding>> {
    quick_read(|paths| Ok(pz::doctor::check_server(paths, &name, &scan(paths)?)))
}

#[tauri::command]
async fn host_start(name: String) -> Res<()> {
    blocking(move || pz::host::start(&Paths::detect(), &name, None)).await
}

/// `save`, `quit`, then wait (Java is ended after 90 s).
#[tauri::command]
async fn host_stop(name: String) -> Res<()> {
    blocking(move || {
        pz::autopilot::stop_asked(&name); // a restart under way mustn't start it again
        pz::host::stop(&Paths::detect(), &name, std::time::Duration::from_secs(90))?;
        pzmm_core::say!("serverStopped", "Stopped {name}.", name = name);
        Ok::<_, anyhow::Error>(())
    })
    .await
}

#[tauri::command]
async fn host_restart(name: String) -> Res<()> {
    blocking(move || {
        // a backup first when Workshop updates are waiting
        pz::autopilot::restart_asked(&Paths::detect(), &name)
    })
    .await
}

#[derive(Serialize)]
struct HostLog {
    /// PZMM started it: `lines` are new since `after`, and more come as events. Else the end of its server-console.txt.
    attached: bool,
    lines: Vec<pz::host::Line>,
}

#[tauri::command(async)]
fn host_log(name: String, after: u64) -> HostLog {
    let (attached, lines) = pz::host::log(&Paths::detect(), &name, after);
    HostLog { attached, lines }
}

/// A console command: None when it went to the console (the reply shows in the log), else RCON's reply.
#[tauri::command]
async fn host_send(name: String, command: String) -> Res<Option<String>> {
    blocking(move || {
        let paths = Paths::detect();
        let reply = pz::host::send(&paths, &name, &command)?;
        // bans, unbans and kicks go in the players' history
        if let Err(e) = pz::dossier::note_command(&paths, &name, store::now(), &command) {
            pzmm_core::log!("{name}: couldn't note it in the players' history: {e:#}");
        }
        Ok::<_, anyhow::Error>(reply)
    })
    .await
}

#[tauri::command]
async fn host_players(name: String) -> Res<Vec<String>> {
    blocking(move || pz::host::players(&Paths::detect(), &name)).await
}

/// A server's mod lines as its ini has them.
#[tauri::command(async)]
fn host_lines(name: String) -> Res<pz::server::ServerMods> {
    quick_read(|paths| pz::server::read(&pz::server::path(paths, &name)?))
}

/// The lines a profile's enabled mods make (its arranged maps), and notes about mods left out.
#[tauri::command(async)]
fn host_lines_from_profile(profile: String, build: u8) -> Res<(pz::server::ServerMods, Vec<String>)> {
    quick_read(|paths| {
        let p = profile::load(paths, &profile)?;
        Ok(pz::server::lines_for(&p.enabled(), &scan(paths)?, build, &profile::map_order(paths, &profile)))
    })
}

#[tauri::command]
async fn host_lines_from_collection(input: String, build: u8) -> Res<(pz::server::ServerMods, Vec<String>)> {
    blocking(move || pz::host::lines_from_collection(&Paths::detect(), &input, build, &std::sync::atomic::AtomicBool::new(false))).await
}

/// A server's mod lines checked, with what the list shows for each entry.
#[derive(Serialize)]
struct LineReport {
    problems: Vec<pz::server::Problem>,
    /// Workshop item -> its title (the listed items and the required ones the list lacks).
    titles: BTreeMap<String, String>,
    /// Mod ID -> (its name, the Workshop item it comes from), as far as they're known.
    mods: BTreeMap<String, (String, String)>,
    /// Items listed items require that the list lacks (and what those require): (needed by, item).
    required: Vec<(String, String)>,
}

/// Problems with `lines` (see `server::check`), plus the Workshop items they require that aren't
/// listed; asks the Workshop whether each item exists and what it's called.
fn check_lines(lines: &pz::server::ServerMods) -> anyhow::Result<LineReport> {
    use sources::steam_workshop;
    let installed = reading(scan).map_err(anyhow::Error::msg)?;
    let ids: Vec<&str> = lines.workshop.iter().map(String::as_str).filter(|w| steam_workshop::valid_id(w)).collect();
    let details = ids.chunks(100).map(steam_workshop::details).collect::<anyhow::Result<Vec<_>>>().map(|d| d.concat()).ok();
    let mut problems = pz::server::check(lines, &installed, details.as_deref());
    let details = details.unwrap_or_default();
    // only items Steam says have required items cost a page read
    let parents: Vec<String> = details.iter().filter(|d| d.num_children > 0).map(|d| d.id.clone()).collect();
    let required = pz::host::missing_required(&lines.workshop, &parents, |id| steam_workshop::required_items(id).unwrap_or_default());
    let more: Vec<&str> = required.iter().map(|(_, r)| r.as_str()).collect();
    let extra = more.chunks(100).map(steam_workshop::details).collect::<anyhow::Result<Vec<_>>>().map(|d| d.concat()).unwrap_or_default();
    let titles: BTreeMap<String, String> = details.iter().chain(&extra).map(|d| (d.id.clone(), d.title.trim().to_string())).collect();
    for (by, item) in &required {
        let title = titles.get(item).filter(|t| !t.is_empty()).map_or_else(|| item.clone(), |t| format!("{t} ({item})"));
        problems.push(pz::server::Problem { error: false, key: "requiredItem", id: by.clone(), item: title.clone(), text: format!("Workshop item {by} requires {title}, which isn't in WorkshopItems") });
    }
    let mods = lines
        .mods
        .iter()
        .map(|m| {
            let known = match installed.get(m) {
                Some(r) => (r.name.clone(), sources::workshop_item(&r.source).unwrap_or_default().to_string()),
                None => (String::new(), details.iter().find(|d| pz::workshop_mod_ids(&d.description).contains(m)).map(|d| d.id.clone()).unwrap_or_default()),
            };
            (m.clone(), known)
        })
        .collect();
    Ok(LineReport { problems, titles, mods, required })
}

#[tauri::command]
async fn host_check(lines: pz::server::ServerMods) -> Res<LineReport> {
    blocking(move || check_lines(&lines)).await
}

/// `lines` with the Workshop items in `input` (links or IDs) added and what they require (see `host::lines_with`).
#[tauri::command]
async fn host_lines_with(lines: pz::server::ServerMods, input: String, build: u8, first: bool) -> Res<(pz::server::ServerMods, Vec<String>)> {
    blocking(move || pz::host::lines_with(&Paths::detect(), &lines, &input, build, first, &std::sync::atomic::AtomicBool::new(false))).await
}

#[derive(Serialize)]
struct People {
    /// From its database (an error when there's none yet).
    accounts: Option<pz::saves::Accounts>,
    accounts_error: String,
    /// The characters in its world's players.db.
    characters: Vec<pz::saves::Player>,
}

/// A server's accounts, bans and characters, from its own files (read from copies).
#[tauri::command]
async fn host_people(name: String) -> Res<People> {
    blocking(move || {
        let paths = Paths::detect();
        let dir = server_of(&paths, &name)?.cachedir(&paths);
        let (accounts, accounts_error) = match pz::saves::accounts(&dir.join("db").join(format!("{name}.db"))) {
            Ok(a) => (Some(a), String::new()),
            Err(e) => (None, format!("{e:#}")),
        };
        let characters = pz::saves::players(&dir.join("Saves/Multiplayer").join(&name)).unwrap_or_default();
        Ok::<_, anyhow::Error>(People { accounts, accounts_error, characters })
    })
    .await
}

/// The items and vehicles a server's admin commands can give: from its own install's scripts, else the game's.
#[tauri::command]
async fn host_catalog(name: String) -> Res<std::sync::Arc<pz::catalog::Catalog>> {
    blocking(move || {
        let paths = Paths::detect();
        let own = pz::host::find(&paths, &name).map(|s| s.install_dir).filter(|d| d.join("media/scripts").is_dir());
        let dir = own.or_else(game_files).ok_or_else(|| anyhow::anyhow!("the game's folder wasn't found (Settings, Folders)"))?;
        pz::catalog::of(&dir)
    })
    .await
}

/// Add a server run elsewhere, reached over RCON (the login is tried first).
#[tauri::command]
async fn host_remote_add(remote: pz::host::Remote, password: String) -> Res<()> {
    blocking(move || {
        pz::host::add_remote(&Paths::detect(), &remote, &password)?;
        pzmm_core::say!("remoteAdded", "Added {name}: {host}:{port}", name = remote.name, host = remote.host, port = remote.port);
        Ok::<_, anyhow::Error>(())
    })
    .await
}

#[tauri::command(async)]
fn host_remote_forget(name: String) -> Res<()> {
    pz::host::forget_remote(&Paths::detect(), &name).map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
fn host_preview(name: String, lines: pz::server::ServerMods) -> Res<pz::server::Preview> {
    quick_read(|paths| pz::server::preview_lines(&pz::server::path(paths, &name)?, &lines))
}

/// Write a server's mod lines (the file before PZMM's first change stays as .ini.bak).
#[tauri::command(async)]
fn host_write(name: String, lines: pz::server::ServerMods) -> Res<()> {
    locked(|paths| {
        if let Some(bad) = lines.workshop.iter().find(|w| !sources::steam_workshop::valid_id(w)) {
            anyhow::bail!("{bad} isn't a Workshop ID: a bad ID crashes the server");
        }
        let b41 = pz::server::build(paths, &name) == 41;
        let file = pz::server::path(paths, &name)?;
        pz::server::write(&file, &lines.mods, &lines.workshop, &lines.maps, b41)?;
        pzmm_core::say!("serverWritten", "Wrote {mods} mods and {items} Workshop items to {file}", mods = lines.mods.len(), items = lines.workshop.len(), file = file.display());
        Ok(())
    })
}

#[derive(Serialize)]
struct TagView {
    /// The PZMM list to put at the end of the description: the longest form that fits ("" when none does).
    tag: String,
    /// "full", "items" (Workshop items and times only) or "none", with the config's own description.
    form: &'static str,
    /// The description's length with the list, and the most A2S carries.
    length: usize,
    limit: usize,
}

/// The PZMM list for `lines` (with the Workshop's update times now), fitted to server `name`'s
/// description; `write` puts it in the config's `PublicDescription=` too.
#[tauri::command]
async fn host_tag(name: String, lines: pz::server::ServerMods, write: bool) -> Res<TagView> {
    blocking(move || {
        let (ini, sides) = reading(|paths| Ok((pz::server::path(paths, &name)?, pz::host::find(paths, &name).map(|s| s.sides).unwrap_or_default()))).map_err(anyhow::Error::msg)?;
        let t = pz::host::tag_for(&lines, &sides)?;
        let own = std::fs::read_to_string(&ini).map(|text| pz::server::value(&text, "PublicDescription").to_string()).unwrap_or_default();
        let (desc, form) = pz::tag::with_tag(&own, &t);
        if write {
            locked(|_| pz::host::set_tag(&ini, Some(&t))).map_err(anyhow::Error::msg)?;
        }
        let tag = if form == "none" { String::new() } else { pz::tag::part(&desc).to_string() };
        Ok::<_, anyhow::Error>(TagView { tag, form, length: desc.chars().count(), limit: pz::tag::LIMIT })
    })
    .await
}

/// Mark a server's mods "server" (only it needs them) or "optional"; the rest are needed on both sides.
#[tauri::command(async)]
fn host_sides_set(name: String, sides: BTreeMap<String, String>) -> Res<()> {
    pz::host::set_sides(&Paths::detect(), &name, sides).map_err(|e| format!("{e:#}"))
}

/// Whether a server's starts put the PZMM list in its description (off takes it out now).
#[tauri::command(async)]
fn host_tag_set(name: String, on: bool) -> Res<()> {
    // not under `locked`: it saves servers.json, which takes the data lock itself
    pz::host::set_tag_on(&Paths::detect(), &name, on).map_err(|e| format!("{e:#}"))
}

/// Manage a config in the Zomboid folder, run from the server installed in `install_dir` (on the build installed there).
#[tauri::command(async)]
fn host_adopt(name: String, install_dir: String) -> Res<()> {
    pz::host::adopt(&Paths::detect(), &name, std::path::Path::new(&install_dir)).map_err(|e| format!("{e:#}"))?;
    pzmm_core::say!("serverAdopted", "PZMM now manages {name}.", name = name);
    Ok(())
}

#[tauri::command(async)]
fn host_forget(name: String) -> Res<()> {
    let paths = Paths::detect();
    if pz::host::find(&paths, &name).is_some_and(|s| !matches!(pz::host::state(&s).0, pz::host::Status::Stopped | pz::host::Status::Crashed)) {
        return Err(keyed!("stopFirst", "Stop {name} first.", name = name));
    }
    pz::host::forget(&paths, &name).map_err(|e| format!("{e:#}"))?;
    pzmm_core::say!("serverForgotten", "PZMM no longer manages {name}; its files stay.", name = name);
    Ok(())
}

// --- automation, backups, settings, Discord, servers run elsewhere (queue items 12-18) ---

/// A server's record, or a bare one for a config in the Zomboid folder PZMM doesn't manage.
fn server_of(paths: &Paths, name: &str) -> anyhow::Result<pz::host::Server> {
    pz::server::path(paths, name)?; // the name's rules
    // one run elsewhere: PZMM's copy of its files (see pz::ftp)
    let cachedir = pz::host::remote(paths, name).map(|_| pz::ftp::mirror(paths, name));
    Ok(pz::host::find(paths, name).unwrap_or_else(|| pz::host::Server { name: name.into(), cachedir, ..Default::default() }))
}

/// Save a server's automation (the Discord webhook and bot token state stays as it is).
#[tauri::command(async)]
fn host_auto_set(name: String, auto: pz::autopilot::Auto) -> Res<()> {
    pz::autopilot::check(&auto).map_err(|e| format!("{e:#}"))?;
    pz::host::set_auto(&Paths::detect(), &name, |a| {
        let discord = std::mem::take(&mut a.discord);
        let d = &auto.discord;
        *a = pz::autopilot::Auto { discord: pz::discord::Settings { events: d.events.clone(), channel: d.channel.clone(), role: d.role.clone(), relay: d.relay, ..discord }, ..auto };
    })
    .map_err(|e| format!("{e:#}"))?;
    pzmm_core::say!("serverAutoSaved", "Saved {name}'s automation.", name = name);
    Ok(())
}

/// Check the server's Workshop items now: the titles of those updated since it started.
#[tauri::command]
async fn host_check_updates(name: String) -> Res<Vec<String>> {
    blocking(move || pz::autopilot::check_updates(&Paths::detect(), &name)).await
}

#[derive(Serialize)]
struct BackupView {
    backups: Vec<pz::server_backup::Backup>,
    /// Zero-byte map files in the save.
    empty_cells: Vec<String>,
    /// The server's own backups: BackupsOnStart, BackupsCount and where they go.
    game_on_start: bool,
    game_count: String,
    game_dir: std::path::PathBuf,
}

#[tauri::command(async)]
fn host_backups(name: String) -> Res<BackupView> {
    quick_read(|paths| {
        let s = server_of(paths, &name)?;
        let ini = std::fs::read_to_string(s.ini(paths)).unwrap_or_default();
        let dir = s.cachedir(paths);
        Ok(BackupView {
            backups: pz::server_backup::list(paths, &name),
            empty_cells: pz::server_backup::empty_cells(&dir.join("Saves/Multiplayer").join(&name)),
            game_on_start: pz::server::value(&ini, "BackupsOnStart") != "false",
            game_count: pz::server::value(&ini, "BackupsCount").to_string(),
            game_dir: dir.join("backups"),
        })
    })
}

/// Back up now (a running server saves first).
#[tauri::command]
async fn host_backup(name: String) -> Res<()> {
    blocking(move || {
        let paths = Paths::detect();
        let s = server_of(&paths, &name)?;
        if pz::host::state(&s).0 == pz::host::Status::Running && pz::host::send(&paths, &name, "save").is_ok() {
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        pz::server_backup::create(&paths, &s, "manual").map(|_| ())
    })
    .await
}

/// Restore a backup: the server is stopped first (saving), and the state before is backed up.
#[tauri::command]
async fn host_restore(name: String, file: String) -> Res<()> {
    blocking(move || {
        let paths = Paths::detect();
        let s = server_of(&paths, &name)?;
        pz::autopilot::stop_asked(&name); // a restart under way mustn't start it again
        if s.external.is_none() && pz::host::state(&s).0 != pz::host::Status::Stopped {
            pz::host::stop(&paths, &name, std::time::Duration::from_secs(90))?;
        }
        pz::server_backup::restore(&paths, &s, std::path::Path::new(&file))
    })
    .await
}

/// A server's zone backups (its `map_meta.bin` alone), newest first.
#[tauri::command(async)]
fn host_zones(name: String) -> Res<Vec<pz::server_backup::Backup>> {
    quick_read(|paths| Ok(pz::server_backup::zones_list(paths, &name)))
}

/// Back up a server's safehouses, zones and factions now (a running server saves first).
#[tauri::command]
async fn host_zones_save(name: String) -> Res<pz::server_backup::Backup> {
    blocking(move || {
        let paths = Paths::detect();
        let s = server_of(&paths, &name)?;
        if pz::host::state(&s).0 == pz::host::Status::Running && pz::host::send(&paths, &name, "save").is_ok() {
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        pz::server_backup::zones_save(&paths, &s, "manual")
    })
    .await
}

/// Put a zone backup back: the server is stopped first (saving), and the current zones backed up.
#[tauri::command]
async fn host_zones_restore(name: String, file: String) -> Res<()> {
    blocking(move || {
        let paths = Paths::detect();
        let s = server_of(&paths, &name)?;
        pz::autopilot::stop_asked(&name); // a restart under way mustn't start it again
        if s.external.is_none() && pz::host::state(&s).0 != pz::host::Status::Stopped {
            pz::host::stop(&paths, &name, std::time::Duration::from_secs(90))?;
        }
        pz::server_backup::zones_restore(&paths, &s, std::path::Path::new(&file))
    })
    .await
}

/// What the owner keeps about a server's players: notes, tags, joins and leaves.
#[tauri::command(async)]
fn host_dossier(name: String) -> Res<pz::dossier::Dossier> {
    Ok(pz::dossier::load(&Paths::detect(), &name))
}

#[tauri::command(async)]
fn host_player_set(name: String, player: String, note: String, tags: Vec<String>) -> Res<pz::dossier::Dossier> {
    pz::dossier::set_player(&Paths::detect(), &name, &player, &note, &tags).map_err(|e| format!("{e:#}"))
}

#[derive(Serialize)]
struct WorldGrid {
    /// Tiles per cell (300 in Build 41, 256 in 42).
    size: u32,
    /// The game's own cells (vanilla and water) and its map mods', as the Maps grid draws them.
    cells: Vec<pz::maps::GridCell>,
    /// Its map mods installed here (their world maps drawn), bottom first: from the last in Map= to the first.
    maps: Vec<String>,
}

/// The world's cells for a server's player map, from its own install's maps and the map mods in
/// its Mods= that are installed here.
#[tauri::command]
async fn host_world(name: String) -> Res<WorldGrid> {
    blocking(move || {
        let paths = Paths::detect();
        let s = server_of(&paths, &name)?;
        let build = s.build_number();
        let mut cache = pz::maps::Cache::load(&paths);
        let game = (!s.install_dir.as_os_str().is_empty()).then_some(s.install_dir.as_path());
        let vanilla = pz::maps::vanilla(game, build, &mut cache).1.unwrap_or_default();
        let water = match game {
            Some(g) if !vanilla.is_empty() => cache.water(g, build),
            _ => vec![],
        };
        let lines = pz::server::read(&s.ini(&paths)).unwrap_or_default();
        // best effort: while an install holds the mods, the map shows the game's own cells
        let on: BTreeMap<String, store::Row> = quick_read(scan).unwrap_or_default().into_iter().filter(|(id, _)| lines.mods.contains(id)).collect();
        let mut maps = pz::maps::installed(&on, build, &mut cache);
        let _ = cache.save(&paths);
        let rank = |m: &pz::maps::MapMod| m.maps.iter().filter_map(|f| lines.maps.iter().position(|l| *l == f.folder)).min().unwrap_or(usize::MAX);
        maps.sort_by_key(|m| std::cmp::Reverse(rank(m)));
        let cells = pz::maps::grid(&vanilla, &water, &maps.iter().collect::<Vec<_>>());
        Ok::<_, anyhow::Error>(WorldGrid { size: if build == 41 { 300 } else { 256 }, cells, maps: maps.into_iter().map(|m| m.id).collect() })
    })
    .await
}

#[derive(Serialize)]
struct ConfigView {
    ini: Vec<pz::server_config::Setting>,
    /// The sandbox form, or why it can't be shown (the file isn't there yet, or PZMM can't read it).
    sandbox: Vec<pz::server_config::Setting>,
    sandbox_error: String,
    regions: Vec<pz::server_config::Region>,
    regions_error: String,
    /// Spawn regions the server's map mods offer that it doesn't list yet.
    spawn_offers: Vec<pz::server_config::Region>,
    /// The files with an earlier version to go back to (`server_config::FILES`).
    prev: Vec<String>,
    presets: Vec<String>,
    own_presets: Vec<String>,
}

fn sibling(paths: &Paths, name: &str, suffix: &str) -> anyhow::Result<std::path::PathBuf> {
    Ok(pz::server::path(paths, name)?.with_file_name(format!("{name}{suffix}")))
}

#[tauri::command(async)]
fn host_config(name: String) -> Res<ConfigView> {
    quick_read(|paths| {
        use pz::server_config as c;
        let s = server_of(paths, &name)?;
        let ini = std::fs::read_to_string(s.ini(paths)).map_err(|e| anyhow::anyhow!("reading {}: {e}", s.ini(paths).display()))?;
        let read = |suffix: &str| std::fs::read_to_string(sibling(paths, &name, suffix)?).map_err(|e| anyhow::anyhow!("{e}"));
        let (mut sandbox, sandbox_error) = match read("_SandboxVars.lua").and_then(|t| c::sandbox_settings(&t)) {
            Ok(v) => (v, String::new()),
            Err(e) => (vec![], format!("{e:#}")),
        };
        let (regions, regions_error) = match read("_spawnregions.lua").and_then(|t| c::spawn_regions(&t)) {
            Ok(v) => (v, String::new()),
            Err(e) => (vec![], format!("{e:#}")),
        };
        // the server's mods: their sandbox options' names and limits, their maps' spawn regions
        let lines = pz::server::read(&s.ini(paths)).unwrap_or_default();
        let roots = pz::host::mod_roots(&s, &lines.mods, &scan(paths).unwrap_or_default());
        let options: Vec<(String, Vec<c::ModOption>)> = roots
            .iter()
            .map(|(_, mod_name, rs)| (mod_name.clone(), rs.iter().filter_map(|r| std::fs::read(r.join("media/sandbox-options.txt")).ok().map(|b| c::mod_options(&pz::decode_text(&b), &c::sandbox_words(r)))).flatten().collect()))
            .collect();
        c::label_mod_settings(&mut sandbox, &options);
        let all_roots: Vec<std::path::PathBuf> = roots.into_iter().flat_map(|r| r.2).collect();
        let spawn_offers = if regions_error.is_empty() { c::map_spawn_regions(&all_roots, &lines.maps, &regions) } else { vec![] };
        let prev = c::FILES.iter().filter(|f| sibling(paths, &name, f).is_ok_and(|p| c::has_prev(&p))).map(|f| f.to_string()).collect();
        Ok(ConfigView { ini: c::ini_settings(&ini), sandbox, sandbox_error, regions, regions_error, spawn_offers, prev, presets: c::game_presets(&s.install_dir), own_presets: c::own_presets(paths) })
    })
}

/// One of a server's settings files (see `server_config::FILES`), as text ("" when it isn't there yet).
#[tauri::command(async)]
fn host_text(name: String, file: String) -> Res<String> {
    quick_read(|paths| {
        if !pz::server_config::FILES.contains(&file.as_str()) {
            anyhow::bail!("{file} isn't a server settings file");
        }
        Ok(std::fs::read(sibling(paths, &name, &file)?).map(|b| pz::decode_text(&b)).unwrap_or_default())
    })
}

#[tauri::command(async)]
fn host_text_write(name: String, file: String, text: String) -> Res<()> {
    locked(|paths| {
        if !pz::server_config::FILES.contains(&file.as_str()) {
            anyhow::bail!("{file} isn't a server settings file");
        }
        settings_saved(&name, &sibling(paths, &name, &file)?, |f| pz::server_config::write_text(f, &text))
    })
}

/// Put a settings file back as it was before its latest change.
#[tauri::command(async)]
fn host_text_restore(name: String, file: String) -> Res<()> {
    locked(|paths| {
        if !pz::server_config::FILES.contains(&file.as_str()) {
            anyhow::bail!("{file} isn't a server settings file");
        }
        settings_saved(&name, &sibling(paths, &name, &file)?, pz::server_config::restore_prev)
    })
}

#[tauri::command(async)]
fn host_ini_write(name: String, changes: Vec<(String, String)>) -> Res<()> {
    locked(|paths| settings_saved(&name, &pz::server::path(paths, &name)?, |f| pz::server_config::write_ini(f, &changes)))
}

#[tauri::command(async)]
fn host_sandbox_write(name: String, changes: Vec<(String, String)>) -> Res<()> {
    locked(|paths| settings_saved(&name, &sibling(paths, &name, "_SandboxVars.lua")?, |f| pz::server_config::write_sandbox(f, &changes)))
}

#[tauri::command(async)]
fn host_spawn_write(name: String, regions: Vec<pz::server_config::Region>) -> Res<()> {
    locked(|paths| settings_saved(&name, &sibling(paths, &name, "_spawnregions.lua")?, |f| pz::server_config::write_spawn_regions(f, &regions)))
}

/// Write one of server `name`'s settings files with `write`, and say so in the activity log.
fn settings_saved(name: &str, file: &std::path::Path, write: impl FnOnce(&std::path::Path) -> anyhow::Result<()>) -> anyhow::Result<()> {
    write(file)?;
    pzmm_core::say!("serverSettingsSaved", "Saved {name}'s settings: {file}", name = name, file = file.display());
    Ok(())
}

/// A preset's sandbox values: the game's (from the server's install) or the user's own.
#[tauri::command(async)]
fn host_preset(name: String, preset: String, own: bool) -> Res<BTreeMap<String, String>> {
    quick_read(|paths| pz::server_config::preset(paths, &server_of(paths, &name)?.install_dir, &preset, own))
}

#[tauri::command(async)]
fn host_preset_save(preset: String, values: BTreeMap<String, String>) -> Res<()> {
    locked(|paths| pz::server_config::save_preset(paths, &preset, &values, pz::server_config::sandbox_version(game_files().as_deref())))
}

/// Keep a server's Discord webhook in the credential store (empty: forget it).
#[tauri::command(async)]
fn host_discord_set(name: String, url: String) -> Res<()> {
    let paths = Paths::detect();
    pz::discord::save(&name, &url).and_then(|_| pz::host::set_auto(&paths, &name, |a| a.discord.saved = !url.trim().is_empty())).map_err(|e| format!("{e:#}"))?;
    if url.trim().is_empty() {
        pzmm_core::say!("serverDiscordForgot", "Removed {name}'s Discord webhook.", name = name);
    } else {
        pzmm_core::say!("serverDiscordSaved", "Saved {name}'s Discord webhook.", name = name);
    }
    Ok(())
}

#[tauri::command]
async fn host_discord_test(name: String) -> Res<()> {
    blocking(move || pz::discord::test(&name)).await
}

/// Keep a server's Discord bot token in the credential store (empty: forget it).
#[tauri::command(async)]
fn host_discord_bot_set(name: String, token: String) -> Res<()> {
    let paths = Paths::detect();
    let on = !token.trim().is_empty();
    pz::discord_bot::save_token(&name, &token).and_then(|_| pz::host::set_auto(&paths, &name, |a| a.discord.bot = on)).map_err(|e| format!("{e:#}"))?;
    if on {
        pzmm_core::say!("serverBotSaved", "Saved {name}'s Discord bot token.", name = name);
    } else {
        pzmm_core::say!("serverBotForgot", "Removed {name}'s Discord bot token.", name = name);
    }
    Ok(())
}

#[tauri::command]
async fn host_discord_bot_test(name: String) -> Res<()> {
    blocking(move || {
        let channel = pz::host::find(&Paths::detect(), &name).map(|s| s.auto.discord.channel).unwrap_or_default();
        pz::discord_bot::test(&name, &channel)
    })
    .await
}

/// The address players join at ("": none), e.g. a playit.gg tunnel's.
#[tauri::command(async)]
fn host_address_set(name: String, address: String) -> Res<()> {
    pz::host::set_address(&Paths::detect(), &name, &address).map_err(|e| format!("{e:#}")) // takes the data lock itself
}

// --- a rented server's files over FTP ---

fn ftp_of(paths: &Paths, name: &str) -> anyhow::Result<(pz::ftp::Ftp, String)> {
    use anyhow::Context;
    let f = pz::host::remote(paths, name).and_then(|r| r.ftp).with_context(|| format!("set up {name}'s file access first"))?;
    let pw = pz::ftp::password(name).with_context(|| format!("no FTP password is saved for {name}: connect again"))?;
    Ok((f, pw))
}

/// Log in with `ftp` (an empty password keeps the saved one), keep the settings, and list the folder
/// it names (or where the login lands).
#[tauri::command]
async fn host_ftp_set(name: String, ftp: pz::ftp::Ftp, password: String) -> Res<(String, Vec<pz::ftp::Entry>)> {
    blocking(move || {
        let paths = Paths::detect();
        let pw = if password.is_empty() { pz::ftp::password(&name).unwrap_or_default() } else { password.clone() };
        let listed = pz::ftp::browse(&ftp, &pw, &ftp.folder)?;
        if !password.is_empty() {
            pz::ftp::save_password(&name, &password)?;
        }
        pz::host::set_ftp(&paths, &name, |f| *f = Some(pz::ftp::Ftp { pulled: f.as_ref().map_or(0, |o| o.pulled), ..ftp }))?;
        Ok::<_, anyhow::Error>(listed)
    })
    .await
}

#[tauri::command]
async fn host_ftp_browse(name: String, dir: String) -> Res<(String, Vec<pz::ftp::Entry>)> {
    blocking(move || {
        let (f, pw) = ftp_of(&Paths::detect(), &name)?;
        pz::ftp::browse(&f, &pw, &dir)
    })
    .await
}

/// Forget the file access (and its password).
#[tauri::command(async)]
fn host_ftp_forget(name: String) -> Res<()> {
    pz::ftp::save_password(&name, "").and_then(|_| pz::host::set_ftp(&Paths::detect(), &name, |f| *f = None)).map_err(|e| format!("{e:#}"))
}

/// Get the settings files into PZMM's copy.
#[tauri::command]
async fn host_ftp_pull(name: String) -> Res<usize> {
    blocking(move || {
        let paths = Paths::detect();
        let (f, pw) = ftp_of(&paths, &name)?;
        let got = pz::ftp::pull_settings(&paths, &name, &f, &pw)?;
        let now = store::now();
        pz::host::set_ftp(&paths, &name, |f| f.iter_mut().for_each(|f| f.pulled = now))?;
        pzmm_core::say!("ftpGot", "Got {name}'s settings files from its host", name = name);
        Ok::<_, anyhow::Error>(got.len())
    })
    .await
}

/// Send the settings files back (the host's copies are backed up first).
#[tauri::command]
async fn host_ftp_push(name: String) -> Res<Option<std::path::PathBuf>> {
    blocking(move || {
        let paths = Paths::detect();
        let (f, pw) = ftp_of(&paths, &name)?;
        pz::ftp::push_settings(&paths, &name, &f, &pw)
    })
    .await
}

static FTP_STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Get the world into a backup.
#[tauri::command]
async fn host_ftp_world(name: String) -> Res<pz::server_backup::Backup> {
    FTP_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    blocking(move || {
        let paths = Paths::detect();
        let (f, pw) = ftp_of(&paths, &name)?;
        pz::ftp::pull_world(&paths, &name, &f, &pw, &FTP_STOP)
    })
    .await
}

#[tauri::command]
fn host_ftp_stop() {
    FTP_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Servers LinuxGSM or Docker runs, in a folder the user picked.
#[tauri::command(async)]
fn host_detect(folder: String) -> Vec<pz::server_detect::Found> {
    pz::server_detect::detect(std::path::Path::new(&folder))
}

#[tauri::command(async)]
fn host_add_external(found: pz::server_detect::Found) -> Res<()> {
    pz::host::add_external(&Paths::detect(), &found).map(|_| ()).map_err(|e| format!("{e:#}"))
}

/// Update a stopped server's files (SteamCMD validates every file), on `build`'s branch.
#[tauri::command]
async fn host_update(name: String, build: String) -> Res<()> {
    blocking(move || {
        SERVER_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
        pz::host::update_server(&Paths::detect(), &name, &build, &SERVER_STOP)
    })
    .await
}

/// Call off the restart countdown; false when none runs.
#[tauri::command]
async fn host_abort_restart(name: String) -> Res<bool> {
    blocking(move || pz::autopilot::abort_restart(&Paths::detect(), &name)).await
}

#[tauri::command(async)]
fn host_history(name: String) -> Vec<pz::autopilot::Event> {
    pz::autopilot::history(&Paths::detect(), &name)
}

#[tauri::command(async)]
fn host_diagnostics(name: String) -> String {
    pz::host::diagnostics(&Paths::detect(), &name)
}

/// The newest chat ("chat") or perk ("perk") log of a server's data folder, its lines in short.
#[tauri::command(async)]
fn host_game_log(name: String, kind: String) -> (Option<std::path::PathBuf>, Vec<String>) {
    pz::host::game_log(&Paths::detect(), &name, &kind)
}

#[tauri::command(async)]
fn host_clone(from: String, to: String) -> Res<()> {
    pz::host::clone_server(&Paths::detect(), &from, to.trim()).map(|_| ()).map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
fn host_coop_worlds() -> Vec<String> {
    let paths = Paths::detect();
    pz::host::coop_worlds(&paths, &played(&paths).zomboid)
}

#[tauri::command]
async fn host_migrate(name: String, world: String) -> Res<()> {
    blocking(move || {
        let paths = Paths::detect();
        pz::host::migrate(&paths, &name, &world, &played(&paths).zomboid)
    })
    .await
}

/// Write a start script for the server into `dir`; returns its path.
#[tauri::command(async)]
fn host_bundle(name: String, dir: String) -> Res<String> {
    pz::host::bundle(&Paths::detect(), &name, std::path::Path::new(&dir)).map(|f| f.display().to_string()).map_err(|e| format!("{e:#}"))
}

/// Back up and reset a stopped server's world; returns the files removed.
#[tauri::command]
async fn host_wipe(name: String, keep_characters: bool, keep_zones: bool) -> Res<usize> {
    blocking(move || {
        let paths = Paths::detect();
        pz::server_backup::wipe(&paths, &server_of(&paths, &name)?, keep_characters, keep_zones)
    })
    .await
}

/// How many of the world's map chunks a reset would take (inside the areas, or outside them all), of how many.
#[tauri::command(async)]
fn host_chunk_plan(name: String, areas: Vec<pz::server_backup::Area>, inside: bool) -> Res<(usize, usize)> {
    quick_read(|paths| {
        let s = server_of(paths, &name)?;
        let all = pz::server_backup::chunks(&s.cachedir(paths).join("Saves/Multiplayer").join(&name));
        Ok((if inside && areas.is_empty() { 0 } else { pz::server_backup::to_reset(&all, &areas, inside).len() }, all.len()))
    })
}

#[tauri::command]
async fn host_chunk_reset(name: String, areas: Vec<pz::server_backup::Area>, inside: bool) -> Res<usize> {
    blocking(move || {
        let paths = Paths::detect();
        pz::server_backup::reset_chunks(&paths, &server_of(&paths, &name)?, &areas, inside)
    })
    .await
}

/// Copy a backup into `dir`; returns the copy's path.
#[tauri::command(async)]
fn host_backup_export(name: String, file: String, dir: String) -> Res<String> {
    pz::server_backup::export(&Paths::detect(), &name, std::path::Path::new(&file), std::path::Path::new(&dir)).map(|f| f.display().to_string()).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn host_backup_import(name: String, file: String) -> Res<()> {
    blocking(move || {
        let paths = Paths::detect();
        pz::server_backup::import(&paths, &server_of(&paths, &name)?, std::path::Path::new(&file)).map(|_| ())
    })
    .await
}

#[derive(Serialize)]
struct ItemState {
    id: String,
    title: String,
    /// On the Workshop, and in the server's downloads (None: PZMM can't see them), Unix seconds.
    updated: u64,
    downloaded: Option<u64>,
}

/// A server's Workshop items: when each was last updated, and the copy its install downloaded.
#[tauri::command]
async fn host_workshop_status(name: String) -> Res<Vec<ItemState>> {
    blocking(move || {
        let paths = Paths::detect();
        let s = server_of(&paths, &name)?;
        let lines = pz::server::read(&s.ini(&paths))?;
        let ids: Vec<&str> = lines.workshop.iter().map(String::as_str).filter(|w| sources::steam_workshop::valid_id(w)).collect();
        let details = ids.chunks(100).map(sources::steam_workshop::details).collect::<anyhow::Result<Vec<_>>>()?.concat();
        let have = pz::server_detect::downloaded(&s.install_dir);
        Ok::<_, anyhow::Error>(details.into_iter().map(|d| ItemState { downloaded: have.get(&d.id).copied(), title: if d.title.is_empty() { d.id.clone() } else { d.title }, id: d.id, updated: d.time_updated }).collect())
    })
    .await
}

// --- the tray: PZMM keeps running with the window closed (servers' automation goes on) ---

/// The tray menu's words and the first-close notice, in the UI's language (English until it sends them).
static TRAY_WORDS: std::sync::Mutex<Option<(String, String, String)>> = std::sync::Mutex::new(None);

fn tray_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem};
    let (open, quit, _) = TRAY_WORDS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| ("Open PZMM".into(), "Quit PZMM".into(), String::new()));
    Menu::with_items(app, &[&MenuItem::with_id(app, "open", open, true, None::<&str>)?, &MenuItem::with_id(app, "quit", quit, true, None::<&str>)?])
}

fn show_window(app: &AppHandle) {
    // still loading behind the splash: lifting it shows the window (it hides the splash first)
    if app.get_webview_window("splash").is_some_and(|s| s.is_visible().unwrap_or(false)) {
        return;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// The first close to the tray says where PZMM went (once: a file in PZMM's data notes it).
fn tell_tray(app: &AppHandle) {
    let told = Paths::detect().data.join("tray-told");
    if told.exists() {
        return;
    }
    let text = TRAY_WORDS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|w| w.2.clone()).filter(|t| !t.is_empty());
    let text = text.unwrap_or_else(|| "PZMM is still running in the tray. Open it or quit from the tray icon.".into());
    if notify(app.clone(), text).is_ok() {
        let _ = std::fs::write(told, "");
    }
}

/// Quit from the tray: with servers PZMM runs, the window asks first (as closing it does).
fn quit_app(app: &AppHandle) {
    let running = pz::host::attached_running();
    if running.is_empty() {
        app.exit(0);
    } else {
        show_window(app);
        let _ = app.emit("servers-running", running);
    }
}

/// Show the tray icon, or take it away.
fn tray(app: &AppHandle, on: bool) -> tauri::Result<()> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    if !on {
        let _ = app.remove_tray_by_id("main");
        return Ok(());
    }
    if app.tray_by_id("main").is_some() {
        return Ok(());
    }
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("PZMM")
        .menu(&tray_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show_window(app),
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|t, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_window(t.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

/// The argument autostart starts PZMM with: straight to the tray.
const MINIMISED: &str = "--minimised";

/// A desktop notification (the UI words it and checks the setting).
#[tauri::command]
fn notify(app: AppHandle, text: String) -> Res<()> {
    use tauri_plugin_notification::NotificationExt;
    app.notification().builder().title("PZMM").body(text).show().map_err(|e| e.to_string())
}

/// With favourites' notifications on, look at them every two minutes and say in the activity log
/// which came online with room (the UI turns the line into a notification too).
/// The game's requests (PZMM - Client), taken off it as they come: PZMM comes to the front (at most every
/// few seconds) and the UI acts on them. Every 5 s it tells the game it's running and which profile is
/// active (app.txt), and while it's in front a change to the game's mod list (default.txt) reloads it.
/// PZMM's version, for app.txt written outside the watcher.
static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn watch_game(app: &AppHandle) {
    let version = app.package_info().version.to_string();
    let _ = VERSION.set(version.clone());
    let (mut there, mut running, mut heard_in) = (false, false, None::<std::path::PathBuf>);
    let (mut tick, mut fronted, mut listed, mut acted) = (0u32, None::<std::time::Instant>, None, None);
    // the bridge folder written to, and the one before it (a profile with its own game folder moves it) since when
    let (mut bridge, mut earlier) = (None::<std::path::PathBuf>, None::<(std::path::PathBuf, std::time::Instant)>);
    loop {
        // a round that panics (a bug) is logged by the panic hook, and the next one runs
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let paths = Paths::detect();
            if tick % 5 == 0 {
                profile::app_for_game(&paths, &version);
            }
            tick = tick.wrapping_add(1);
            // at the start, and when the bridge moves: everything the game reads, written there (the feed follows on its
            // next load, Check setup's findings with the UI's next check)
            let now_in = pz::bridge::dir(&paths);
            let appeared = !there && now_in.is_dir();
            there = now_in.is_dir();
            if bridge.as_ref() != Some(&now_in) || appeared {
                // first: a write that panics isn't tried again every second
                earlier = bridge.replace(now_in.clone()).map(|d| (d, std::time::Instant::now()));
                profile::app_for_game(&paths, &version);
                profile::profiles_for_game(&paths);
                profile::library_for_game(&paths, &profile::peek(&paths));
                pz::saves::backups_for_game(&played(&paths));
                pz::launcher::servers_for_game(&paths, &pz::launcher::state(&paths));
                if let Ok(index) = store::load(&paths) {
                    store::installed_for_game(&paths, &index);
                }
                store::tags_for_game(&paths, &store::tags(&paths));
                categories_for_game();
                // the UI's part: Check setup's findings and the feed, written there again
                let _ = app.emit("game-bridge", ());
            }
            // a game may still be open in the folder left: heard for 10 more minutes
            let left = earlier.as_ref().filter(|(d, at)| *d != now_in && at.elapsed() < std::time::Duration::from_secs(600)).and_then(|(d, _)| pz::bridge::queue_requests_in(&paths, d));
            // a game started from Steam while the profile has its own folder plays in another: while its report is
            // fresh (10 minutes), its requests are heard there and it's told PZMM is running (the active folder's app.txt)
            let report = pz::bridge::game_report(&paths).filter(|g| !g.zomboid.is_empty());
            let away = report.as_ref().map(|g| (g.at, std::path::Path::new(&g.zomboid).join("Lua").join("pzmm"))).filter(|(_, d)| d.is_dir() && !pz::bridge::same_dir(d, &now_in));
            if away.is_some() && tick % 5 == 0 {
                running = pz::host::game_running();
            }
            let elsewhere = away.filter(|(at, _)| pzmm_core::store::now().saturating_sub(*at) < 600 || running).map(|(_, d)| d);
            if let Some(d) = elsewhere.as_ref()
                && heard_in.as_ref() != Some(d)
                && let Some(z) = d.parent().and_then(|l| l.parent())
            {
                heard_in = Some(d.clone());
                pz::saves::backups_for_game(&Paths { zomboid: z.to_path_buf(), data: paths.data.clone() });
            }
            let heard = elsewhere.as_ref().and_then(|d| {
                // just after app_for_game's write (every 5 s)
                if tick % 5 == 1
                    && let Ok(text) = std::fs::read_to_string(now_in.join("app.txt"))
                {
                    let text = if d.join("requests.queue").exists() { text.replace("busy=0", "busy=1") } else { text };
                    pz::bridge::write_for_game(&d.join("app.txt"), text);
                    pz::bridge::mirror(&now_in, d);
                }
                pz::bridge::queue_requests_in(&paths, d)
            });
            if let Some(front) = pz::bridge::queue_requests(&paths).into_iter().chain(left).chain(heard).reduce(|a, b| a || b) {
                if front && fronted.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(5)) {
                    fronted = Some(std::time::Instant::now());
                    show_window(app);
                }
                let _ = app.emit("game-requests", ());
            }
            let changed = std::fs::metadata(paths.default_txt()).and_then(|m| m.modified()).ok();
            if listed.is_some() && changed != listed && app.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false)) {
                let _ = app.emit("game-list", ());
            }
            listed = changed;
            // the game's own activity: the UI shows its new lines
            let did = std::fs::metadata(pz::bridge::dir(&paths).join("activity_game.txt")).and_then(|m| m.modified()).ok();
            if did.is_some() && did != acted {
                let _ = app.emit("game-activity", ());
            }
            acted = did;
        }));
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn watch_favourites() {
    let mut before = None;
    loop {
        if settings::load().notify.iter().any(|n| n == "favourites") {
            let favourites = reading(|paths| Ok(launcher::state(paths).favourites)).unwrap_or_default();
            let probed = launcher::probe_all(favourites, true);
            let (up, new) = launcher::newly_joinable(before.as_ref(), &probed);
            let names: Vec<&str> = new.iter().map(|s| s.name.as_str()).collect();
            if !names.is_empty() {
                pzmm_core::say!("favouriteUp", "Online with room to join: {names}", names = names.join(", "));
            }
            before = Some(up);
        } else {
            before = None;
        }
        std::thread::sleep(std::time::Duration::from_secs(120));
    }
}

/// The tray menu's words in the UI's language.
#[tauri::command]
fn tray_words(app: AppHandle, open: String, quit: String, hidden: String) -> Res<()> {
    *TRAY_WORDS.lock().unwrap_or_else(|e| e.into_inner()) = Some((open, quit, hidden));
    if let Some(t) = app.tray_by_id("main") {
        t.set_menu(Some(tray_menu(&app).map_err(|e| e.to_string())?)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The page has loaded: the splash goes and the main window shows (once; a start in the tray had no splash).
#[tauri::command]
fn splash_done(app: AppHandle) {
    PAGE_UP.store(true, std::sync::atomic::Ordering::Relaxed);
    lift_splash(&app);
}

/// The splash goes and the main window shows (the page's call, or the fallback when it never comes).
fn lift_splash(app: &AppHandle) {
    if let Some(s) = app.get_webview_window("splash") {
        // hidden first: closing takes a moment, and the splash stays on top of the window showing under it
        let _ = s.hide();
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.center(); // where the splash was, every launch
        }
        show_window(app);
        let _ = s.close();
    }
}

/// The page has mounted (it lifted the splash), so it can answer "close-ask": a page that never did can't.
static PAGE_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Download history: installs, updates, removals and subscriptions, newest first.
#[tauri::command(async)]
fn history_list() -> Vec<pzmm_core::history::Record> {
    pzmm_core::history::list(&Paths::detect())
}

/// Forget history records: those of `ids`, or all.
#[tauri::command(async)]
fn history_clear(ids: Option<Vec<String>>) -> Vec<pzmm_core::history::Record> {
    pzmm_core::history::clear(&Paths::detect(), ids.as_deref())
}

/// The window's close button, answered: hide to the tray (turning it on), or quit.
#[tauri::command]
fn app_close(app: AppHandle, to_tray: bool) -> Res<()> {
    if !to_tray {
        quit_app(&app);
        return Ok(());
    }
    tray(&app, true).map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    tell_tray(&app);
    Ok(())
}

/// Quit after the "servers are running" question: stop them first, or leave them running
/// (PZMM finds them again next time by their process id, and reaches them over RCON).
#[tauri::command]
async fn app_quit(app: AppHandle, stop: bool) -> Res<()> {
    blocking(move || {
        if stop {
            let paths = Paths::detect();
            for name in pz::host::attached_running() {
                pz::autopilot::stop_asked(&name); // a restart under way mustn't start it again
                pz::host::stop(&paths, &name, std::time::Duration::from_secs(90))?;
            }
        }
        app.exit(0);
        Ok::<_, anyhow::Error>(())
    })
    .await
}

/// Make a profile from a save's mod list and use it.
#[tauri::command(async)]
fn save_use(mode: String, name: String) -> Res<View> {
    locked(|paths| {
        let save = pz::saves::list(&played(paths)).into_iter().find(|s| s.mode == mode && s.name == name);
        let Some(save) = save else { anyhow::bail!("save {mode}/{name} not found") };
        apply(paths, &profile::for_save(paths, &profile::active_name(paths), &format!("Save {name}"), save.mods, &save.maps)?)?;
        view(paths)
    })
}

// --- save tools (queue item 34) ---

#[derive(Serialize)]
struct SaveDetails {
    backups: Vec<pz::server_backup::Backup>,
    broken: pz::saves::Broken,
    /// How many files each reset would remove (see `saves::RESETS`).
    resets: BTreeMap<&'static str, usize>,
}

/// A save's backups and broken files (walks the save, so off the UI thread).
#[tauri::command]
async fn save_details(mode: String, name: String, game: Option<bool>) -> Res<SaveDetails> {
    blocking(move || {
        // for a request from the game: the folder it plays in
        let paths = if game == Some(true) { game_played(&Paths::detect()) } else { played(&Paths::detect()) };
        let dir = pz::saves::dir(&paths, &mode, &name)?;
        Ok::<_, anyhow::Error>(SaveDetails {
            backups: pz::saves::backups(&paths, &mode, &name)?,
            broken: pz::saves::broken(&dir),
            resets: pz::saves::resets(&dir),
        })
    })
    .await
}

fn game_closed() -> anyhow::Result<()> {
    if pz::host::game_running() {
        anyhow::bail!("the game is running: quit it first, so it doesn't write over the save");
    }
    Ok(())
}

/// What the game asked PZMM to do since the last look (PZMM - Client's requests.txt), taken: each is
/// handled once. The UI asks before acting on any that change something.
#[tauri::command]
async fn game_requests() -> Res<Vec<pz::bridge::Stamped>> {
    Ok(pz::bridge::take_requests(&Paths::detect()))
}

/// Tell the game what became of its requests: its sorting edits settle on these.
#[tauri::command]
async fn game_ack(acks: Vec<pz::bridge::Ack>) -> Res<()> {
    pz::bridge::ack(&Paths::detect(), &acks);
    Ok(())
}

/// The app's activity lines that concern the game (PZMM - Client's activity_app.txt).
#[tauri::command]
async fn game_activity_add(lines: Vec<pz::bridge::Activity>) -> Res<()> {
    pz::bridge::activity_for_game(&Paths::detect(), &lines);
    Ok(())
}

/// The UI is (or no longer is) handling the game's requests: app.txt says busy, at once.
#[tauri::command]
async fn game_busy(on: bool) -> Res<()> {
    pz::bridge::GAME_BUSY.store(on, std::sync::atomic::Ordering::Relaxed);
    let paths = Paths::detect();
    profile::app_for_game(&paths, &VERSION.get().cloned().unwrap_or_default());
    Ok(())
}

/// The app's accent and theme, for the in-game manager to match (app.txt).
#[tauri::command]
async fn game_look(accent: String, theme: String) -> Res<()> {
    let ok = |s: &str, max: usize| s.len() <= max && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '#');
    if ok(&accent, 7) && ok(&theme, 5) {
        *pz::bridge::LOOK.lock().unwrap_or_else(|e| e.into_inner()) = (accent, theme);
        profile::app_for_game(&Paths::detect(), &VERSION.get().cloned().unwrap_or_default());
    }
    Ok(())
}

/// The game's own activity lines (PZMM - Client's activity_game.txt), oldest first.
#[tauri::command]
async fn game_activity() -> Res<Vec<pz::bridge::Activity>> {
    Ok(pz::bridge::game_activity(&Paths::detect()))
}

/// Check setup's findings, as the UI words them, for the game's Problems (PZMM - Client's findings.txt).
#[tauri::command]
async fn game_findings(lines: Vec<String>) -> Res<()> {
    pz::bridge::findings_for_game(&Paths::detect(), &lines);
    Ok(())
}

/// PZMM - Client's options (its Settings tab), shared: None before the mod has run in the game.
#[tauri::command]
async fn game_settings() -> Res<Option<std::collections::BTreeMap<String, bool>>> {
    Ok(pz::bridge::client_settings(&Paths::detect()))
}

#[tauri::command]
async fn game_settings_set(set: std::collections::BTreeMap<String, bool>) -> Res<()> {
    pz::bridge::set_client_settings(&Paths::detect(), &set).map_err(|e| format!("{e:#}"))
}

/// What's new was read up to the update at `at`: the game's badge clears too (PZMM - Client's feedseen.txt).
#[tauri::command]
async fn feed_seen(at: u64) -> Res<()> {
    pz::feed::set_seen(&Paths::detect(), at);
    Ok(())
}

/// Launch arguments as a shell splits them: a "quoted span" (with spaces) stays one argument, quotes dropped.
fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (vec![], String::new(), false, false);
    for c in s.chars() {
        match c {
            '"' => (quoted, any) = (!quoted, true),
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            c => (cur.push(c), any = true).0,
        }
    }
    if any {
        out.push(cur);
    }
    out
}

/// What the game last reported (PZMM - Client's game.json): the mods it loaded, at the menu or in a game.
#[tauri::command]
async fn game_report() -> Res<Option<pz::bridge::GameReport>> {
    Ok(pz::bridge::game_report(&Paths::detect()))
}

/// Back a save up now; a `slot` name keeps it until it's deleted.
#[tauri::command]
async fn save_backup(mode: String, name: String, slot: String) -> Res<()> {
    blocking(move || pz::saves::back_up(&played(&Paths::detect()), &mode, &name, "manual", &slot, settings::load().save_backup_keep).map(|_| ())).await
}

/// The Zomboid folder the game asking is in: the one its fresh report names (a game started from Steam while the
/// profile has its own), else the one the active profile plays in.
fn game_played(paths: &Paths) -> Paths {
    let fresh = pz::bridge::game_report(paths).filter(|g| !g.zomboid.is_empty() && pzmm_core::store::now().saturating_sub(g.at) < 6 * 3600);
    match fresh.map(|g| std::path::PathBuf::from(g.zomboid)).filter(|z| z.join("Lua").join("pzmm").is_dir()) {
        Some(zomboid) => Paths { zomboid, data: paths.data.clone() },
        None => played(paths),
    }
}

/// A restore the game asked for: the game is running, so it must be at its main menu (no save open).
#[tauri::command]
async fn save_restore_from_game(mode: String, name: String, file: String) -> Res<()> {
    blocking(move || {
        let paths = Paths::detect();
        if pz::bridge::game_report(&paths).is_none_or(|g| g.state != "menu") {
            anyhow::bail!("the game has a save open: go back to its main menu first");
        }
        pz::saves::restore(&game_played(&paths), &mode, &name, std::path::Path::new(&file), settings::load().save_backup_keep)
    })
    .await
}

/// A backup the game asked for: like a restore, only at its main menu, so the save isn't zipped while the game writes it.
#[tauri::command]
async fn save_backup_from_game(mode: String, name: String) -> Res<()> {
    blocking(move || {
        let paths = Paths::detect();
        if pz::bridge::game_report(&paths).is_none_or(|g| g.state != "menu") {
            anyhow::bail!("the game has a save open: go back to its main menu first");
        }
        pz::saves::back_up(&game_played(&paths), &mode, &name, "manual", "", settings::load().save_backup_keep).map(|_| ())
    })
    .await
}

#[tauri::command]
async fn save_restore(mode: String, name: String, file: String) -> Res<()> {
    blocking(move || {
        game_closed()?;
        pz::saves::restore(&played(&Paths::detect()), &mode, &name, std::path::Path::new(&file), settings::load().save_backup_keep)
    })
    .await
}

/// Give a backup a slot name (`copy`: as a copy, the backup stays as it is).
#[tauri::command]
async fn save_backup_name(mode: String, name: String, file: String, slot: String, copy: bool) -> Res<()> {
    blocking(move || pz::saves::name_backup(&played(&Paths::detect()), &mode, &name, std::path::Path::new(&file), &slot, copy).map(|_| ())).await
}

/// The save's screenshot, or the one in backup `file` ("": the save's own), as raw bytes (empty if none).
#[tauri::command]
async fn save_thumb(mode: String, name: String, file: String) -> Res<tauri::ipc::Response> {
    blocking(move || pz::saves::thumb(&played(&Paths::detect()), &mode, &name, (!file.is_empty()).then(|| std::path::Path::new(&file)))).await.map(tauri::ipc::Response::new)
}

/// Back the save up, then remove the files of one reset (`saves::RESETS`). How many went.
#[tauri::command]
async fn save_reset(mode: String, name: String, what: String) -> Res<usize> {
    blocking(move || {
        game_closed()?;
        pz::saves::reset(&played(&Paths::detect()), &mode, &name, &what, settings::load().save_backup_keep)
    })
    .await
}

/// Copy a save as `<name>_modded` with the active profile's mods and maps. The copy's name.
#[tauri::command(async)]
fn save_clone(mode: String, name: String) -> Res<String> {
    game_closed().map_err(|e| e.to_string())?;
    locked(|paths| {
        let p = active(paths)?;
        let maps = profile::game_maps(paths, &p, &scan(paths)?, profile::effective_build(paths));
        pz::saves::clone_with_mods(&played(paths), &mode, &name, &p.enabled(), &maps, profile::effective_build(paths) == 41)
    })
}

#[tauri::command(async)]
fn save_backup_delete(mode: String, name: String, file: String) -> Res<()> {
    pz::saves::delete_backup(&played(&Paths::detect()), &mode, &name, std::path::Path::new(&file)).map_err(|e| format!("{e:#}"))
}

/// Back the save up, then remove its zero-byte cell files (the game rebuilds them). How many went.
#[tauri::command]
async fn save_repair(mode: String, name: String) -> Res<usize> {
    blocking(move || {
        game_closed()?;
        pz::saves::repair(&played(&Paths::detect()), &mode, &name, settings::load().save_backup_keep)
    })
    .await
}

/// Give a save the active profile's mods and the maps they add (the UI shows the difference first).
#[tauri::command(async)]
fn save_set_mods(mode: String, name: String) -> Res<()> {
    game_closed().map_err(|e| e.to_string())?;
    locked(|paths| {
        let p = active(paths)?;
        let maps = profile::game_maps(paths, &p, &scan(paths)?, profile::effective_build(paths));
        pz::saves::set_mods(&played(paths), &mode, &name, &p.enabled(), &maps, profile::effective_build(paths) == 41)
    })
}

/// A save's databases and their tables, with row counts (read from copies).
#[tauri::command]
async fn save_databases(mode: String, name: String) -> Res<Vec<(String, Vec<(String, u64)>)>> {
    blocking(move || pz::saves::dir(&played(&Paths::detect()), &mode, &name).map(|d| pz::saves::databases(&d))).await
}

/// 100 rows of a table in one of a save's databases, from `offset`.
#[tauri::command]
async fn save_table(mode: String, name: String, file: String, table: String, offset: u64) -> Res<pz::saves::Table> {
    blocking(move || pz::saves::table(&pz::saves::dir(&played(&Paths::detect()), &mode, &name)?, &file, &table, offset, 100)).await
}

#[tauri::command]
async fn save_players(mode: String, name: String) -> Res<Vec<pz::saves::Player>> {
    blocking(move || pz::saves::players(&pz::saves::dir(&played(&Paths::detect()), &mode, &name)?)).await
}

// --- a save's sandbox settings (queue item 35) ---

/// The installed game's folder that holds projectzomboid.jar and media/.
fn game_files() -> Option<std::path::PathBuf> {
    pz::zombiebuddy::game_dir().and_then(|g| pz::zombiebuddy::launcher_dir(&g))
}

/// A save's `map_sand.bin` as the server sandbox form, with the game's presets and the user's.
#[tauri::command]
async fn save_sandbox(mode: String, name: String) -> Res<ConfigView> {
    blocking(move || {
        let paths = played(&Paths::detect());
        let file = pz::saves::dir(&paths, &mode, &name)?.join("map_sand.bin");
        let game = game_files();
        let (sandbox, sandbox_error) = match pz::sandbox::form(game.as_deref(), &file) {
            Ok(v) => (v, String::new()),
            Err(e) => (vec![], format!("{e:#}")),
        };
        let presets = game.as_deref().map(pz::server_config::game_presets).unwrap_or_default();
        Ok::<_, anyhow::Error>(ConfigView { ini: vec![], sandbox, sandbox_error, regions: vec![], regions_error: String::new(), spawn_offers: vec![], prev: vec![], presets, own_presets: pz::server_config::own_presets(&paths) })
    })
    .await
}

#[tauri::command]
async fn save_sandbox_write(mode: String, name: String, changes: Vec<(String, String)>) -> Res<()> {
    blocking(move || {
        game_closed()?;
        let file = pz::saves::dir(&played(&Paths::detect()), &mode, &name)?.join("map_sand.bin");
        pz::sandbox::write(game_files().as_deref(), &file, &changes)
    })
    .await
}

/// A preset's sandbox values: the installed game's or the user's own.
#[tauri::command(async)]
fn save_sandbox_preset(preset: String, own: bool) -> Res<BTreeMap<String, String>> {
    quick_read(|paths| pz::server_config::preset(paths, &game_files().unwrap_or_default(), &preset, own))
}

// --- performance (queue item 38) ---

#[derive(Serialize)]
struct PerfView {
    ram_mb: u64,
    /// ProjectZomboid64.json (None: the game wasn't found).
    launcher: Option<String>,
    memory_mb: Option<u32>,
    /// PZ_Optimization's heap option set -Xmx.
    memory_by_pzopt: bool,
    suggested_mb: u32,
    /// Enabled mods: how many, their size on disk, and the largest (id, name, bytes).
    mods: usize,
    mods_size: u64,
    largest: Vec<(String, String, u64)>,
    pzopt: pz::perf::PzOpt,
    game_running: bool,
    /// The collector and the other JVM options (None: the game wasn't found).
    jvm: Option<pz::perf::Jvm>,
}

/// The official PZ_Optimization Workshop copy's version folder (its installer is there).
fn pzopt_copy(paths: &Paths) -> anyhow::Result<Option<std::path::PathBuf>> {
    let mods = scan(paths)?;
    Ok(mods.get(pz::perf::PZOPT_ID).filter(|m| sources::workshop_item(&m.source) == Some(pz::perf::PZOPT_ITEM)).and_then(|m| m.info.parent().map(|p| p.to_path_buf())))
}

/// Whether the game is running on this machine.
#[tauri::command(async)]
fn game_running() -> bool {
    pz::host::game_running()
}

/// Memory, PZ_Optimization and what the mod list weighs (walks the enabled mods' files).
#[tauri::command]
async fn perf_view() -> Res<PerfView> {
    blocking(|| {
        let (enabled, copy) = reading(|paths| {
            let mods = scan(paths)?;
            let enabled: Vec<(String, String, std::path::PathBuf)> = active(paths)?.enabled().into_iter().filter_map(|id| mods.get(&id).map(|m| (id, m.name.clone(), m.dir.clone()))).collect();
            Ok((enabled, pzopt_copy(paths)?))
        })?;
        let mut sizes: Vec<(String, String, u64)> = enabled.into_iter().map(|(id, name, dir)| (id, name, pz::files::stats(&dir).size)).collect();
        sizes.sort_by_key(|m| std::cmp::Reverse(m.2));
        let game = game_files();
        let (memory_mb, memory_by_pzopt) = game.as_deref().and_then(|g| pz::perf::memory(g).ok()).unwrap_or_default();
        let ram_mb = pz::host::total_ram_mb();
        Ok::<_, String>(PerfView {
            ram_mb,
            launcher: game.as_ref().map(|g| g.join("ProjectZomboid64.json").display().to_string()),
            memory_mb,
            memory_by_pzopt,
            suggested_mb: pz::perf::suggest_mb(ram_mb, sizes.len()),
            mods: sizes.len(),
            mods_size: sizes.iter().map(|m| m.2).sum(),
            largest: sizes.into_iter().take(5).collect(),
            pzopt: game.as_deref().map(|g| pz::perf::pzopt(g, copy.as_deref())).unwrap_or_default(),
            game_running: pz::host::game_running(),
            jvm: game.as_deref().and_then(|g| pz::perf::jvm(g).ok()),
        })
    })
    .await
}

/// Set the game's -Xmx.
#[tauri::command(async)]
fn perf_memory_set(mb: u32) -> Res<()> {
    let game = game_files().ok_or("the game wasn't found: set its folder in Settings")?;
    pz::perf::set_memory(&game, mb).map_err(|e| format!("{e:#}"))
}

/// Set the game's garbage collector and its other JVM options.
#[tauri::command(async)]
fn perf_jvm_set(gc: String, args: Vec<String>) -> Res<()> {
    let game = game_files().ok_or("the game wasn't found: set its folder in Settings")?;
    pz::perf::set_jvm(&game, &gc, &args).map_err(|e| format!("{e:#}"))
}

/// Install (or update) or uninstall PZ_Optimization with the official Workshop copy's installer.
#[tauri::command]
async fn pzopt_run(uninstall: bool) -> Res<String> {
    blocking(move || {
        game_closed()?;
        let game = game_files().ok_or_else(|| anyhow::anyhow!("the game wasn't found: set its folder in Settings"))?;
        let copy = reading(pzopt_copy).map_err(anyhow::Error::msg)?.ok_or_else(|| anyhow::anyhow!("PZ_Optimization isn't installed from the Workshop"))?;
        let out = pz::perf::pzopt_run(&game, &copy, uninstall)?;
        pzmm_core::log!("{out}");
        Ok::<_, anyhow::Error>(out)
    })
    .await
}

// --- maps ---

use pzmm_core::pz::maps::{self, MapMod};

/// A Workshop item PZMM read maps from (by the scan or a check), for search and sorting.
#[derive(Serialize)]
struct MapItem {
    title: String,
    author: String,
    subscriptions: u64,
    size: u64,
    /// Checked by hand: listed even with the Workshop source off.
    manual: bool,
    /// Last updated and first published (Unix seconds).
    updated: u64,
    created: u64,
    tags: Vec<String>,
    /// How many mods it holds on this build (a mod pack holds several).
    mods: usize,
}

#[derive(Serialize)]
struct MapsView {
    /// The build cells are read for (B41 and B42 cells differ in size).
    build: u8,
    /// The game's map folders: a `lots=` naming one is always met.
    vanilla_folders: Vec<String>,
    /// The game's cells, or None when the install isn't found or isn't `build`.
    vanilla: Option<Vec<maps::Cell>>,
    /// The game's cells that are mostly river (from its world map).
    water: Vec<maps::Cell>,
    /// Installed mods that add maps.
    mods: Vec<MapMod>,
    /// Workshop maps read without installing (and not installed since): one per mod id.
    checked: Vec<MapMod>,
    /// Those Workshop items (and installed ones the scan saw), by item id.
    items: BTreeMap<String, MapItem>,
    /// Mod ids several Workshop items provide (reuploads, packs), with those items.
    providers: BTreeMap<String, Vec<String>>,
}

/// Every map's cells (walks map folders the cache doesn't know yet, so outside the lock).
#[tauri::command]
async fn map_view() -> Res<MapsView> {
    blocking(|| -> Res<_> {
        let (mods, build, checked) = reading(|paths| Ok((scan(paths)?, profile::effective_build(paths), maps::checked(paths))))?;
        let paths = Paths::detect();
        let mut cache = maps::Cache::load(&paths);
        let installed = maps::installed(&mods, build, &mut cache);
        let game = pz::folders::path("game", &settings::load());
        let (vanilla_folders, vanilla) = maps::vanilla(game.as_deref(), build, &mut cache);
        let water = match (&game, &vanilla) {
            (Some(g), Some(_)) => cache.water(g, build),
            _ => vec![],
        };
        let _ = cache.save(&paths); // only a cache: the next view reads the folders again
        let (out, providers) = maps::workshop_maps(&checked, build, &mods);
        let items = checked
            .into_iter()
            // read, or read before and kept while a newer version won't read
            .filter(|(_, c)| c.failed.is_empty() || !c.builds.is_empty())
            .map(|(id, c)| {
                let mods = c.mods.get(&build).copied().unwrap_or_default();
                (id, MapItem { title: c.title, author: c.author, subscriptions: c.subscriptions, size: c.size, manual: c.manual, updated: c.updated, created: c.created, tags: c.tags, mods })
            })
            .collect();
        Ok(MapsView { build, vanilla_folders, vanilla, water, mods: installed, checked: out, items, providers })
    })
    .await
}

/// What the in-game map draws for the game's own maps (`id` empty) or an installed map mod, to
/// draw under the grid; empty when it ships no world map.
#[tauri::command]
async fn map_world(id: String) -> Res<maps::WorldMap> {
    blocking(move || -> Res<_> {
        if id.is_empty() {
            return Ok(pz::folders::path("game", &settings::load()).map(|g| maps::world_maps(&[g])).unwrap_or_default());
        }
        let (mods, build) = reading(|paths| Ok((scan(paths)?, profile::effective_build(paths))))?;
        Ok(mods.get(&id).map(|m| maps::mod_world(&m.dir, build)).unwrap_or_default())
    })
    .await
}

/// The grid for the shown maps (the UI sends back what `map_view` gave it).
#[tauri::command(async)]
fn map_grid(vanilla: Vec<maps::Cell>, water: Vec<maps::Cell>, shown: Vec<MapMod>) -> Vec<maps::GridCell> {
    maps::grid(&vanilla, &water, &shown.iter().collect::<Vec<_>>())
}

/// The most maps from `all` that fit together around the `fixed` (enabled) ones; ties go to
/// the higher `ranks` entry.
#[tauri::command(async)]
fn map_quick_select(all: Vec<MapMod>, fixed: Vec<MapMod>, ranks: BTreeMap<String, u64>) -> Vec<String> {
    maps::quick_select(&all, &fixed, |m| ranks.get(&m.id).copied().unwrap_or(0))
}

/// The `Map=` list for `folders`, keeping the arrangement in `order` and, for maps it lacks,
/// where the profile's order starts (see `profile::map_start`). Reads only: opening the Maps
/// tab changes nothing.
#[tauri::command(async)]
fn map_line(order: Vec<String>, folders: Vec<String>) -> Res<Vec<String>> {
    quick_read(|paths| {
        let start = profile::map_start(paths, &profile::peek(paths), &scan(paths)?, profile::effective_build(paths));
        Ok(pz::server::map_order(&pz::server::merge_order(&start, &order), &folders))
    })
}

/// Keep a `Map=` list the player arranged with the active profile (its enabled maps only:
/// previewed ones never), and give the game the enabled maps in that order (default.txt's `maps` block).
/// Refused when `profile` (the one the list was arranged for) isn't active any more, or when the
/// game's mod menu changed the list since the Maps tab read it.
#[tauri::command(async)]
fn map_order_save(maps: Vec<String>, profile: String) -> Res<()> {
    locked(|paths| {
        let (p, adopted) = profile::sync(paths)?;
        if adopted {
            anyhow::bail!("maps.err.reloaded");
        }
        if p.name != profile {
            anyhow::bail!("maps.err.otherProfile");
        }
        let on = pz::server::mod_maps(&p.enabled(), &scan(paths)?, profile::effective_build(paths));
        let maps: Vec<String> = maps.into_iter().filter(|m| on.contains(m) || maps::is_vanilla(m)).collect();
        if profile::set_map_order(paths, &p.name, &maps)? {
            apply(paths, &p)?;
        }
        Ok(())
    })
}

/// Clipboard text for the shown maps: links, mod ids, requirements (looked up on the Workshop
/// when PZMM doesn't know them) and the server lines, worded with `labels` (the UI's language).
#[tauri::command]
async fn map_export(mut shown: Vec<MapMod>, map_line: Vec<String>, labels: maps::ExportLabels) -> Res<String> {
    blocking(move || -> Res<_> {
        let (mods, build, checked, order) = reading(|paths| Ok((scan(paths)?, profile::effective_build(paths), maps::checked(paths), profile::peek(paths).enabled())))?;
        let known = maps::workshop_maps(&checked, build, &mods).0;
        // listed, and in Mods=, in the profile's load order (requirements still first), then as shown
        shown.sort_by_key(|m| order.iter().position(|i| *i == m.id).unwrap_or(usize::MAX));
        let mut find = |id: &str, needed_by: Option<&str>| match sources::find_mod(id, needed_by, build) {
            Ok(c) => c.into_iter().next().filter(|c| c.sure()).map(|c| (c.item.title, c.item.id)),
            Err(e) => {
                pzmm_core::say!("lookupFailed", "error: looking up {id}: {error}", id = id, error = format!("{e:#}"));
                None
            }
        };
        Ok(maps::export(&shown.iter().collect::<Vec<_>>(), &mods, &known, &map_line, build == 41, &labels, &mut find))
    })
    .await
}
/// Read a Workshop item's cells from its file list, without installing it; with `download`,
/// SteamCMD downloads it when Steam won't give the list (the download is deleted).
#[tauri::command]
async fn map_check(id: String, download: bool) -> Res<Vec<MapMod>> {
    blocking(move || -> Res<_> {
        if !sources::steam_workshop::valid_id(&id) {
            return Err(maps::ui_error(&maps::Fail::NotAnId.into()));
        }
        let (have, build) = reading(|paths| Ok((sources::workshop_available(paths).contains(&id), profile::effective_build(paths))))?;
        if have {
            return Err(maps::ui_error(&maps::Fail::Installed.into()));
        }
        MAP_CHECK_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
        let found = maps::check(&Paths::detect(), &id, download, &MAP_CHECK_STOP).map_err(|e| maps::ui_error(&e))?.builds.remove(&build).unwrap_or_default();
        if found.is_empty() {
            pzmm_core::say!("noMaps", "workshop:{id} adds no maps for Build {build}", id = id, build = build);
        }
        Ok(found)
    })
    .await
}

static MAP_CHECK_STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
fn map_check_stop() {
    MAP_CHECK_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

static MAP_SCAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MAP_SCAN_STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A one-at-a-time flag held while this lives: cleared when it drops, errors and panics included.
struct Running(&'static std::sync::atomic::AtomicBool);

impl Running {
    /// None while another holds `flag`.
    fn take(flag: &'static std::sync::atomic::AtomicBool) -> Option<Running> {
        (!flag.swap(true, std::sync::atomic::Ordering::SeqCst)).then_some(Running(flag))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Read every Map-tagged Workshop item, both builds (see `maps::scan_workshop`), sending
/// "maps-scan" progress events. One scan at a time.
#[tauri::command]
async fn map_scan(app: AppHandle) -> Res<maps::Progress> {
    let Some(_running) = Running::take(&MAP_SCAN) else { return Err(maps::ui_error(&maps::Fail::ScanRunning.into())) };
    MAP_SCAN_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    blocking(move || {
        maps::scan_workshop(&Paths::detect(), &MAP_SCAN_STOP, &mut |p| {
            let _ = app.emit("maps-scan", p);
        })
        .map_err(|e| maps::ui_error(&e))
    })
    .await
}

#[tauri::command]
fn map_scan_stop() {
    MAP_SCAN_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command(async)]
fn map_presets() -> Res<BTreeMap<String, Vec<String>>> {
    quick_read(|paths| Ok(maps::presets(paths)))
}

/// Save a named set of maps; no `ids` deletes it.
#[tauri::command(async)]
fn map_preset_set(name: String, ids: Option<Vec<String>>) -> Res<BTreeMap<String, Vec<String>>> {
    locked(|paths| {
        maps::set_preset(paths, &name, ids)?;
        Ok(maps::presets(paths))
    })
}

/// The picture drawn under Build `build`'s grid, if one is set.
#[tauri::command(async)]
fn map_background(build: u8) -> Res<Option<maps::Background>> {
    quick_read(|paths| Ok(maps::background(paths, build)))
}

/// Set (or with no `bg`, forget) Build `build`'s background picture.
#[tauri::command(async)]
fn map_background_set(build: u8, bg: Option<maps::Background>) -> Res<()> {
    locked(|paths| maps::set_background(paths, build, bg))
}

/// A background picture's bytes (PNG or JPEG only).
#[tauri::command(async)]
fn map_background_image(path: String) -> Res<tauri::ipc::Response> {
    let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase());
    if !matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg")) {
        return Err(format!("not a PNG or JPEG: {path}"));
    }
    std::fs::read(&path).map(tauri::ipc::Response::new).map_err(|e| format!("{path}: {e}"))
}

/// A PNG or JPEG picked in the system dialog.
#[tauri::command]
async fn pick_image(app: AppHandle) -> Res<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || -> anyhow::Result<_> {
        Ok(app.dialog().file().add_filter("PNG, JPEG", &["png", "jpg", "jpeg"]).blocking_pick_file().and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()))
    })
    .await
}

/// A server's maps against `yours` (map folders): what each has that the other doesn't, and which of
/// its maps overlap. The server is config `name`, or `theirs` (a public server's mods and maps) on Build `build`.
#[tauri::command]
async fn map_server_compare(name: String, yours: Vec<String>, theirs: Option<pz::server::ServerMods>, build: Option<u8>) -> Res<maps::Compare> {
    blocking(move || -> Res<_> {
        let (server, mods, build, checked) = reading(|paths| {
            let (server, build) = match theirs {
                Some(s) => (s, build.unwrap_or_else(|| profile::effective_build(paths))),
                None => (pz::server::read(&pz::server::path(paths, &name)?)?, pz::server::build(paths, &name)),
            };
            Ok((server, scan(paths)?, build, maps::checked(paths)))
        })?;
        let paths = Paths::detect();
        let mut cache = maps::Cache::load(&paths);
        let mut all = maps::installed(&mods, build, &mut cache);
        all.extend(checked.into_values().flat_map(|mut c| c.builds.remove(&build).unwrap_or_default()));
        let game = pz::folders::path("game", &settings::load());
        let vanilla = maps::vanilla(game.as_deref(), build, &mut cache).0;
        Ok(maps::compare(&server, &yours, &vanilla, &all))
    })
    .await
}

// --- Project Zomboid / Project Viewpoint ---

use pzmm_core::pz::viewpoint as vp;

/// The active profile packaged for sharing.
#[tauri::command(async)]
fn profile_export() -> Res<profile::Shared> {
    quick_read(|paths| profile::export(paths, &scan(paths)?))
}

/// Install what a shared profile needs from the Workshop, then save and use it. A Viewpoint
/// profile also gets Viewpoint set up, as switching to Viewpoint does.
#[tauri::command]
async fn profile_import(shared: profile::Shared) -> Res<View> {
    blocking(move || {
        let viewpoint = shared.kind == vp::KIND;
        let needs = reading(|paths| {
            let mods = scan(paths)?;
            let mut needs = profile::import_needs(&shared, &mods);
            if viewpoint {
                needs.extend(vp::needed(paths, &mods)?.into_iter().filter(|s| !needs.contains(s)).collect::<Vec<_>>());
            }
            Ok(needs)
        })?;
        if !needs.is_empty() {
            pzmm_core::say!("sharedItems", "installing {count} Workshop item(s) the shared profile uses", count = needs.len());
        }
        fetch_then(&needs, false, |paths, _| {
            if viewpoint {
                vp::set_up(&scan(paths)?)?;
            }
            let p = profile::import(paths, &shared)?;
            pzmm_core::say!("imported", "Imported the profile as \"{name}\".", name = p.name);
            view(paths)
        })
    })
    .await
}

/// Install a Workshop collection (its items and what they require) and make it a profile of
/// its own: every mod it holds on the build you play, required ones first.
/// What pasted Workshop collections (or items: several links or IDs at once) hold on the build you
/// play, read without installing: for choosing what to import.
#[tauri::command]
async fn collection_preview(input: String) -> Res<pz::collection::Pack> {
    blocking(move || {
        let paths = Paths::detect();
        let (title, items) = pz::collection::items(&paths, &input)?;
        let build = reading(|paths| Ok(profile::effective_build(paths))).map_err(anyhow::Error::msg)?;
        pz::collection::read(&paths, &title, &items, build, &[], &std::sync::atomic::AtomicBool::new(false))
    })
    .await
}

/// Install Workshop `items` (from `collection_preview`) PZMM doesn't have and make them a profile of
/// its own, `title`: their mods on the build you play but those in `skip` (of alternative versions,
/// the first one left), required ones first.
#[tauri::command]
async fn collection_profile(title: String, items: Vec<String>, skip: Vec<String>) -> Res<View> {
    blocking(move || -> Res<View> {
        let needs: Vec<Source> = reading(|paths| {
            let have = sources::workshop_available(paths);
            Ok(items.iter().filter(|i| !have.contains(*i)).map(|i| Source::Workshop(i.clone())).collect())
        })?;
        if !needs.is_empty() {
            fetch_then(&needs, false, |_, _| Ok(()))?;
        }
        let build = reading(|paths| Ok(profile::effective_build(paths)))?;
        let mut pack = pz::collection::read(&Paths::detect(), &title, &items, build, &skip, &std::sync::atomic::AtomicBool::new(false)).map_err(|e| format!("{e:#}"))?;
        locked(|paths| {
            // imported now: the sandbox presets it ships become the player's
            pz::collection::save_presets(paths, &mut pack);
            for n in &pack.notes {
                pzmm_core::log!("{n}");
            }
            let list = pack.mods.iter().map(|m| format!("+{m}\n")).collect();
            let name = if title.trim().is_empty() { "Collection".to_string() } else { title.clone() };
            let p = profile::import(paths, &profile::Shared { name, kind: String::new(), list, order: pack.mods.clone(), workshop: pack.item_of.clone(), maps: pack.maps.clone() })?;
            pzmm_core::say!("imported", "Imported the profile as \"{name}\".", name = p.name);
            view(paths)
        })
    })
    .await
}

/// The lists the game's mod menu and the Workshop mod managers keep: presets, favourites, hidden mods.
#[derive(Serialize)]
struct Lists {
    presets: Vec<pz::managers::Preset>,
    favourites: Vec<String>,
    hidden: Vec<String>,
    /// The player's own tags, by mod id (PZMM's, shared by every profile).
    tags: store::Tags,
}

fn lists_of(paths: &Paths) -> anyhow::Result<Lists> {
    Ok(Lists { presets: pz::managers::presets(paths), favourites: pz::managers::favourites(paths), hidden: pz::managers::hidden(paths), tags: store::tags(paths) })
}

#[tauri::command(async)]
fn lists() -> Res<Lists> {
    quick_read(lists_of)
}

/// Save a preset in the game's or a Workshop mod manager's file (see `managers::save_preset`).
#[tauri::command(async)]
fn preset_save(preset: pz::managers::Preset) -> Res<Lists> {
    locked(|paths| {
        pz::managers::save_preset(paths, &preset)?;
        lists_of(paths)
    })
}

/// Make another manager's preset a profile: install its Workshop items PZMM doesn't have
/// (server presets list them), then save and use it.
#[tauri::command]
async fn preset_import(preset: pz::managers::Preset) -> Res<View> {
    blocking(move || {
        let needs: Vec<Source> = reading(|paths| {
            let have = sources::workshop_available(paths);
            Ok(preset.workshop.iter().filter(|w| sources::steam_workshop::valid_id(w) && !have.contains(*w)).map(|w| Source::Workshop(w.clone())).collect())
        })?;
        fetch_then(&needs, false, |paths, _| {
            let list = preset.mods.iter().map(|m| format!("+{m}\n")).collect();
            let p = profile::import(paths, &profile::Shared { name: preset.name.clone(), kind: String::new(), list, order: preset.mods.clone(), workshop: Default::default(), maps: vec![] })?;
            pzmm_core::say!("imported", "Imported the profile as \"{name}\".", name = p.name);
            view(paths)
        })
    })
    .await
}

/// Star (`kind` "favourite") or hide ("hidden") a mod, in the game's and the mod managers' lists.
#[tauri::command(async)]
fn mod_mark(id: String, kind: String, on: bool) -> Res<Lists> {
    locked(|paths| {
        match kind.as_str() {
            "favourite" => pz::managers::set_favourite(paths, &id, on)?,
            "hidden" => pz::managers::set_hidden(paths, &id, on)?,
            _ => anyhow::bail!("unknown mark {kind:?}"),
        }
        lists_of(paths)
    })
}

/// Set a mod's own tags (none removes them).
#[tauri::command(async)]
fn mod_tags(id: String, tags: Vec<String>) -> Res<Lists> {
    locked(|paths| {
        store::set_tags(paths, &id, &tags)?;
        lists_of(paths)
    })
}

/// A mod's rule in Mod Load Order Sorter's rules file (empty when it has none).
#[tauri::command(async)]
fn mod_rule(id: String) -> Res<pz::managers::Rule> {
    quick_read(|paths| Ok(pz::managers::rules(paths).remove(&id).unwrap_or_default()))
}

/// Replace a mod's rule in the Sorter's rules file (an empty one removes it).
#[tauri::command(async)]
fn mod_rule_set(id: String, rule: pz::managers::Rule) -> Res<View> {
    locked(|paths| {
        pz::managers::set_rule(paths, &id, rule)?;
        view(paths)
    })
}

/// Pin a mod's place for Autosort and Mod Load Order Sorter: it loads after `after` and before
/// `before` (both empty: unpinned). Written to the Sorter's rules file.
#[tauri::command(async)]
fn mod_pin(id: String, after: Vec<String>, before: Vec<String>) -> Res<View> {
    locked(|paths| {
        let mut rule = pz::managers::rules(paths).remove(&id).unwrap_or_default();
        (rule.load_after, rule.load_before) = (after, before);
        pz::managers::set_rule(paths, &id, rule)?;
        view(paths)
    })
}

/// The last game and server logs: the first real error and the errors per mod.
#[tauri::command(async)]
fn log_reports() -> Res<Vec<pz::logs::Report>> {
    quick_read(|paths| Ok(pz::logs::reports(paths, &scan(paths)?)))
}

#[derive(Serialize)]
struct GameLog {
    /// The logs there are to read, newest kind first.
    files: Vec<String>,
    /// `file` (else the first), read again; None when its size is still `known`.
    view: Option<pz::logs::View>,
}

/// The game and server logs, for the log viewer. Only the logs PZMM lists can be read.
#[tauri::command]
async fn game_log(file: Option<String>, known: u64) -> Res<GameLog> {
    // the log is read and parsed outside the lock (it follows every 2 s)
    blocking(move || {
        let (files, mods) = quick_read(|paths| {
            let files = pz::logs::viewable(paths);
            let changed = files.iter().find(|f| file.as_deref() == Some(&*f.to_string_lossy())).or(files.first()).is_some_and(|f| std::fs::metadata(f).map_or(0, |m| m.len()) != known);
            Ok((files, if changed { Some(scan(paths)?) } else { None }))
        })
        .map_err(anyhow::Error::msg)?;
        let pick = files.iter().find(|f| file.as_deref() == Some(&*f.to_string_lossy())).or(files.first());
        let view = match (pick, mods) {
            (Some(f), Some(mods)) => Some(pz::logs::view(f, &mods)?),
            _ => None,
        };
        Ok::<_, anyhow::Error>(GameLog { files: files.iter().map(|f| f.display().to_string()).collect(), view })
    })
    .await
}

/// Stop reporting the scans' findings about a file (by its SHA-256) until it changes.
#[tauri::command(async)]
fn scan_dismiss(sha: String) -> Res<()> {
    locked(|paths| pz::javamods::dismiss(paths, &sha))
}

/// Health check of the active profile and server configs; `online` also asks the Workshop about
/// the enabled Workshop mods. Only the profile and scan are read under the lock: the check reads
/// every enabled mod's files (seconds on a big list), and other commands mustn't wait on it.
#[tauri::command]
async fn doctor(online: Option<bool>) -> Res<Vec<pz::doctor::Finding>> {
    blocking(move || {
        let (enabled, mods, build) = quick_read(|paths| Ok((active(paths)?.enabled(), scan(paths)?, profile::effective_build(paths))))?;
        let mut out = pz::doctor::check(&Paths::detect(), &enabled, &mods, build);
        if online == Some(true) {
            out.extend(pz::doctor::check_workshop(&Paths::detect(), &enabled, &mods));
        }
        pz::doctor::mark_hidden(&Paths::detect(), &mods, &mut out);
        Ok::<_, String>(out)
    })
    .await
}

/// Forget ZombieBuddy's "no" for a Java file (by its SHA-256): PZMM allows it on the next Play.
#[tauri::command(async)]
fn zb_unblock(sha: String) -> Res<()> {
    let file = pz::javamods::zb_config_dir().ok_or("no home folder")?.join("mod_approvals.json");
    pz::javamods::unblock(&file, &sha).map_err(|e| format!("{e:#}"))
}

/// Hide a Check setup finding ("It's fine"), or show it again; it comes back by itself when its mod changes.
#[tauri::command(async)]
fn finding_hide(key: String, subject: String, vars: BTreeMap<String, String>, on: bool) -> Res<()> {
    let mods = quick_read(scan)?;
    pz::doctor::hide(&Paths::detect(), &mods, &key, &subject, vars, on).map_err(|e| format!("{e:#}"))
}

/// What switching to `mode` would do, for the confirmation panel.
#[tauri::command(async)]
fn mode_preview(mode: String) -> Res<Vec<String>> {
    quick_read(|paths| vp::preview(paths, &mode, &scan(paths)?))
}

/// Switch between plain Project Zomboid and Project Viewpoint profiles, setting up
/// Viewpoint (downloads, ZombieBuddy agent, profile) the first time.
#[tauri::command]
async fn mode_switch(mode: String) -> Res<View> {
    blocking(move || {
        if mode != vp::KIND {
            return locked(|paths| {
                vp::leave(paths)?;
                view(paths)
            });
        }
        let missing = reading(|paths| vp::needed(paths, &scan(paths)?))?;
        fetch_then(&missing, false, |paths, _| {
            vp::enter(paths, &scan(paths)?)?;
            view(paths)
        })
    })
    .await
}

#[tauri::command(async)]
fn open_path(app: AppHandle, path: String) -> Res<()> {
    // folders only: a file path here could launch a program
    if !std::path::Path::new(&path).is_dir() {
        pzmm_core::say!("notAFolder", "error: not a folder: {path}", path = path);
        return Err(format!("not a folder: {path}"));
    }
    app.opener().open_path(&path, None::<&str>).map_err(|e| {
        pzmm_core::say!("openFailed", "error: couldn't open {path}: {error}", path = path, error = e);
        e.to_string()
    })
}

#[tauri::command(async)]
fn open_url(app: AppHandle, url: String) -> Res<()> {
    if !(url.starts_with("https://") || url == format!("steam://rungameid/{}", pz::STEAM_APP)) {
        pzmm_core::say!("httpsOnly", "error: PZMM only opens https links: {url}", url = url);
        return Err(format!("refusing to open {url}"));
    }
    // Workshop pages open in the Steam client when it's running (Subscribe works there), unless
    // Settings say browser; otherwise in the browser, since a steam:// link would start Steam
    let workshop = ["https://steamcommunity.com/sharedfiles/", "https://steamcommunity.com/workshop/"].iter().any(|p| url.starts_with(p));
    let steam = workshop && settings::load().workshop_links != "browser" && sources::steam_workshop::client_running();
    let url = if steam { format!("steam://openurl/{url}") } else { url };
    app.opener().open_url(&url, None::<&str>).map_err(|e| {
        pzmm_core::say!("openUrlFailed", "error: couldn't open {url}: {error} (is Steam installed?)", url = url, error = e);
        e.to_string()
    })
}

/// The Steam link that starts the game, with `args` (Steam asks once before passing them on).
fn launch_url(args: &str) -> String {
    let args = args.split_whitespace().collect::<Vec<_>>().join(" ");
    if args.is_empty() {
        return format!("steam://rungameid/{}", pz::STEAM_APP);
    }
    let enc: String = args.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.=".contains(&b) { char::from(b).to_string() } else { format!("%{b:02X}") }).collect();
    format!("steam://run/{}//{enc}/", pz::STEAM_APP)
}

/// Start the game with `args` (Settings' and the active profile's come first): through Steam, or
/// a copy outside Steam (GOG) through its own launcher. The Paths the game plays in (the
/// profile's own folder, if it has one).
fn start_game(app: &AppHandle, args: &str) -> anyhow::Result<Paths> {
    // the game emptied its mod list (after an update) or it was changed outside PZMM: the active
    // profile goes back in first, or the game starts without its mods. Busy (an install) answers
    // at once rather than leaving Play waiting with no word.
    quick_read(|paths| {
        let p = active(paths)?;
        if profile::game_differs(paths, &p) {
            *SCAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
            apply(paths, &p)?;
            pzmm_core::say!("listRestored", "Put the {count} mod(s) of \"{name}\" back in the game's mod list before starting it.", count = p.enabled().len(), name = p.name);
        }
        // a first start of Build 42 empties the list unless its marker is there (apply above wrote it already)
        else if profile::effective_build(paths) >= 42 {
            profile::keep_through_game_reset(paths);
        }
        // Java mods turned on here are allowed in ZombieBuddy too, so it doesn't ask again in the game
        // (best effort: a scan that fails here doesn't stop the game starting)
        let mods = scan(paths).unwrap_or_default();
        let java: Vec<&store::Row> = p.enabled().iter().filter_map(|id| mods.get(id)).filter(|m| !(m.id == zb::MOD_ID && zb::official_source(&m.source))).collect();
        // only where ZombieBuddy keeps its settings already (no folder made for players who don't use it),
        // and not for a profile set up from a server's list or a share code: its Java mods are someone else's pick
        let mine = profile::picked_by_player(paths, &p.name);
        if let Some(file) = pz::javamods::zb_config_dir().filter(|d| d.is_dir() && mine).map(|d| d.join("mod_approvals.json")) {
            match pz::javamods::approve(&file, &java, &pz::javamods::exceptions(paths)) {
                Ok(ids) if !ids.is_empty() => pzmm_core::say!("zbApproved", "Allowed {count} Java mod(s) in ZombieBuddy, so it won't ask in the game: {ids}", count = ids.len(), ids = ids.join(", ")),
                Ok(_) => {}
                Err(e) => pzmm_core::say!("zbApproveFailed", "error: couldn't allow the Java mods in ZombieBuddy ({error}): it asks in the game", error = format!("{e:#}")),
            }
        }
        Ok(())
    })
    .map_err(anyhow::Error::msg)?;
    let paths = Paths::detect();
    // for "What's new" (playing or joining): if it can't be noted, the newest save's time stands in
    let _ = pz::feed::record_launch(&paths);
    let (own, folder) = profile::launch_args(&paths, &profile::active_name(&paths))?;
    let args = format!("{} {own} {args}", settings::load().launch_args);
    let game = pz::zombiebuddy::game_dir().filter(|g| g.is_dir() && !pz::folders::steam_copy(g));
    if let Some(exe) = game.as_ref().and_then(|g| ["ProjectZomboid64.exe", "ProjectZomboid64", "projectzomboid.sh"].iter().map(|e| g.join(e)).find(|e| e.is_file())) {
        if let Err(e) = std::process::Command::new(&exe).args(split_args(&args)).current_dir(exe.parent().unwrap_or(std::path::Path::new("."))).spawn() {
            pzmm_core::say!("launchFailed", "error: couldn't start the game ({url}): {error} (is Steam installed?)", url = exe.display(), error = e);
            anyhow::bail!("couldn't start {}: {e}", exe.display());
        }
    } else {
        let url = launch_url(&args);
        if let Err(e) = app.opener().open_url(&url, None::<&str>) {
            // without the arguments: they can hold a server's password
            pzmm_core::say!("launchFailed", "error: couldn't start the game ({url}): {error} (is Steam installed?)", url = launch_url(""), error = e);
            anyhow::bail!("couldn't start the game: {e} (is Steam installed?)");
        }
    }
    Ok(Paths { zomboid: folder.unwrap_or(paths.zomboid), data: paths.data })
}

/// Start the game with the extra arguments from Settings and the active profile.
#[tauri::command(async)]
fn launch_game(app: AppHandle) -> Res<()> {
    let s = settings::load();
    let played = start_game(&app, "").map_err(|e| format!("{e:#}"))?;
    // save backups on launch, while playing and on exit
    std::thread::spawn(move || pz::saves::session(&played, &s));
    Ok(())
}

#[derive(Serialize)]
struct ProfileLaunch {
    profile: String,
    launch: profile::Launch,
}

/// The active profile's launch options.
#[tauri::command(async)]
fn profile_launch() -> Res<ProfileLaunch> {
    quick_read(|paths| {
        let name = profile::active_name(paths);
        Ok(ProfileLaunch { launch: profile::launch_of(paths, &name), profile: name })
    })
}

/// Set the active profile's extra arguments, and whether it plays in a folder of its own.
#[tauri::command(async)]
fn profile_launch_set(args: String, own_folder: bool) -> Res<ProfileLaunch> {
    locked(|paths| {
        let name = profile::active_name(paths);
        let was = profile::launch_of(paths, &name);
        // its own folder, switched off, stays its: on again, it's the same folder (its saves)
        let own = was.cache_dir.clone().or(was.kept);
        let (cache_dir, kept) = if own_folder { (Some(own.unwrap_or_else(|| profile::own_game_folder(paths, &name))), None) } else { (None, own) };
        if cache_dir != was.cache_dir {
            same_folder_while_playing(paths, &name, &cache_dir.clone().unwrap_or_else(|| paths.zomboid.clone()).join("Lua").join("pzmm"))?;
        }
        profile::set_launch(paths, &name, &profile::Launch { args, cache_dir, kept })
    })?;
    profile_launch()
}

// --- the launcher: find servers, set up their mods, join ---

/// Every PZ server, kept ten minutes (Home lists them at start-up, so the Servers tab opens at once): Steam's list and the game's saved one, merged. Steam's list
/// comes from the Web API key when one is saved, else (or when the key fails) from the Steam
/// client: both give the same list, and the client shows the player in the game for a moment.
#[tauri::command]
async fn servers_list(fresh: bool) -> Res<launcher::Listing> {
    static LIST: Mutex<Option<(std::time::Instant, launcher::Listing)>> = Mutex::new(None);
    // one load at a time: a second ask waits for the first and takes its list (one Steam helper)
    static LOADING: Mutex<()> = Mutex::new(());
    blocking(move || {
        let asked = std::time::Instant::now();
        let _loading = LOADING.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, l)) = LIST.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
            && (*at >= asked || !fresh && at.elapsed() < std::time::Duration::from_secs(600))
        {
            return Ok(l.clone());
        }
        let l = std::thread::scope(|s| {
            let game = s.spawn(|| launcher::game_list(&Paths::detect().zomboid));
            let mut lists = vec![];
            if let Some(key) = settings::secret(settings::STEAM_KEY) {
                lists.push(("web", launcher::list(&key)));
            }
            // the Steam client helper loads the game's Windows DLL
            if cfg!(windows) && !matches!(lists.first(), Some((_, Ok(_)))) {
                let game = pz::folders::path("game", &settings::load()).ok_or_else(|| anyhow::anyhow!("the game's install wasn't found, so the Steam client can't be asked"));
                lists.push(("steam", game.and_then(|g| pz::steam_client::list(&g))));
            }
            lists.push(("game", game.join().unwrap_or_else(|_| Err(anyhow::anyhow!("reading the game's list failed")))));
            launcher::merge(lists)
        });
        for a in l.sources.iter().filter(|a| !a.error.is_empty()) {
            let source = match a.source.as_str() {
                "steam" => "Steam",
                "web" => "Steam Web API",
                _ => "ServerListSteam.db",
            };
            pzmm_core::say!("serverSourceFailed", "error: couldn't list servers from {source}: {error}", source = source, error = a.error);
        }
        *LIST.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), l.clone()));
        Ok::<_, anyhow::Error>(l)
    })
    .await
}

/// The servers Steam friends play on now (needs the Steam Web API key).
#[tauri::command]
async fn servers_friends() -> Res<Vec<launcher::Server>> {
    blocking(|| {
        let Some(key) = settings::secret(settings::STEAM_KEY) else { anyhow::bail!("no Steam Web API key: add one in Settings") };
        launcher::friends(&key)
    })
    .await
}

/// Ask each server for its info and rules (cached five minutes unless `fresh`).
#[tauri::command]
async fn servers_probe(servers: Vec<launcher::Server>, fresh: bool) -> Res<Vec<launcher::Server>> {
    blocking(move || Ok::<_, anyhow::Error>(launcher::probe_all(servers, fresh))).await
}

/// Country codes by IP for the server browser; nothing unless the player turned countries on.
#[tauri::command]
async fn server_countries(ips: Vec<String>) -> Res<BTreeMap<String, String>> {
    if !settings::load().countries {
        return Ok(BTreeMap::new());
    }
    blocking(move || pz::geoip::countries(&Paths::detect(), &ips)).await
}

#[tauri::command]
async fn server_players(addr: String) -> Res<Vec<pz::a2s::Player>> {
    blocking(move || launcher::players(&addr)).await
}

#[derive(Serialize)]
struct ServerMods {
    mods: Vec<launcher::Resolved>,
    /// Its `Map=` as far as A2S shows it, Muldraugh last.
    maps: Vec<String>,
    /// The build you play (41 or 42).
    build: u8,
    /// The server's PZMM list: "full" (its exact mods, load order and maps), "items" (its exact
    /// Workshop items; mods matched by ID) or "" (none).
    list: &'static str,
    /// The list's Workshop items, with the update times it was made with.
    items: Vec<launcher::ListItem>,
    /// Mods its list marks only the server needs, and those players may leave out.
    server_only: Vec<String>,
    optional: Vec<String>,
}

/// The server's mods resolved to Workshop items: from its PZMM list when it carries one, else its
/// listed mod IDs through what's installed, the map and mod ID indexes, then a Workshop search.
#[tauri::command]
async fn server_mods(server: launcher::Server) -> Res<ServerMods> {
    blocking(move || {
        let (mods, build) = reading(|paths| Ok((scan(paths)?, profile::effective_build(paths)))).map_err(anyhow::Error::msg)?;
        let server_build: u8 = if server.build.starts_with("41") {
            41
        } else if server.build.starts_with("42") {
            42
        } else {
            build
        };
        let tag = server.tag.as_ref();
        let full = tag.filter(|t| !t.mods.is_empty());
        let within: Vec<String> = tag.iter().flat_map(|t| t.items.iter().map(|(i, _)| i.clone())).collect();
        let ids = full.map_or(&server.mods, |t| &t.mods);
        let mut found = launcher::resolve(&Paths::detect(), ids, server_build, &mods, &mut |id| {
            let c = sources::find_mod(id, None, server_build)?;
            Ok(c.into_iter().next().map(|c| (c.item.id.clone(), c.sure())))
        }, &within);
        let mut items: Vec<&str> = found.iter().map(|r| r.item.as_str()).filter(|i| !i.is_empty()).chain(within.iter().map(String::as_str)).collect();
        items.sort();
        items.dedup();
        let details = items.chunks(100).map(sources::steam_workshop::details).collect::<anyhow::Result<Vec<_>>>().map(|d| d.concat());
        let mut details = details.unwrap_or_else(|e| {
            pzmm_core::say!("detailsFailed", "error: couldn't read the Workshop details of the server's mods: {error}", error = format!("{e:#}"));
            vec![]
        });
        // an item picked for a mod ID that's for the other build: one that fits the server's takes its place
        launcher::fit_build(&Paths::detect(), &mut found, server_build, &within, &mut details, |ids| {
            ids.chunks(100).map(sources::steam_workshop::details).collect::<anyhow::Result<Vec<_>>>().map(|d| d.concat()).unwrap_or_default()
        });
        launcher::add_details(&mut found, &details);
        let maps = full.map_or_else(|| launcher::map_folders(&server.map), |t| t.maps.clone());
        let list = if full.is_some() { "full" } else if tag.is_some() { "items" } else { "" };
        let items = tag.map(|t| launcher::list_items(t, &details, &mods)).unwrap_or_default();
        let (server_only, optional) = tag.map(|t| (t.server_only.clone(), t.optional.clone())).unwrap_or_default();
        Ok::<_, anyhow::Error>(ServerMods { mods: found, maps, build, list, items, server_only, optional })
    })
    .await
}

/// Read the Workshop items updated since the mod ID index was made, in the background (at most
/// once a day, and only on top of a seed: a full read takes hours).
#[tauri::command(async)]
fn mod_index_refresh() {
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let paths = Paths::detect();
    let at = pz::mod_index::index(&paths).at;
    if at == 0 || store::now().saturating_sub(at) < 86_400 || RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        match pz::mod_index::scan(&paths, &std::sync::atomic::AtomicBool::new(false), &mut |_| {}) {
            Ok(p) => pzmm_core::say!("modIndexRead", "Read the mod IDs of {count} Workshop item(s) updated since the last look", count = p.done),
            Err(e) => pzmm_core::say!("modIndexFailed", "error: couldn't update the mod ID index: {error}", error = format!("{e:#}")),
        }
        RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

/// Download `items` (and what they require), then make "Server <name>" from the server's `mods`
/// (those installed, after what they require) and `maps`, and switch to it. Other profiles stay as they are.
#[tauri::command]
async fn server_setup(server: launcher::Server, items: Vec<String>, mods: Vec<String>, maps: Vec<String>) -> Res<View> {
    blocking(move || {
        let sources: Vec<Source> = items.into_iter().filter(|i| sources::steam_workshop::valid_id(i)).map(Source::Workshop).collect();
        fetch_then(&sources, true, |paths, _| {
            let installed = scan(paths)?;
            let leaving = profile::kind_of(paths, &profile::active_name(paths)) == vp::KIND;
            let p = profile::for_server(paths, &server.name, &server.addr, pz::requirements_first(&mods, |id| Some(installed.get(id)?.requires.clone())).0, &maps)?;
            let name = p.name.clone();
            profile::apply(paths, &p, Some(&installed))?;
            pzmm_core::say!("serverProfile", "Switched to \"{name}\" with {count} mod(s) for {server}.", name = name, count = p.enabled().len(), server = server.name);
            if leaving && !p.enabled().iter().any(|m| m == vp::MOD_ID) {
                pzmm_core::say!("leavingViewpoint", "Leaving Project Viewpoint for this server");
            }
            let missing: Vec<&str> = mods.iter().filter(|m| !installed.contains_key(*m)).map(String::as_str).collect();
            if !missing.is_empty() {
                pzmm_core::say!("serverMissing", "Not installed, so not in the profile: {ids}", ids = missing.join(", "));
            }
            view(paths)
        })
    })
    .await
}

/// Start the game joining `server` with the active profile, and note the exact mod set used.
/// `password` empty: the remembered one, if any; `remember` keeps a typed one.
#[tauri::command]
async fn server_join(app: AppHandle, server: launcher::Server, password: String, remember: bool) -> Res<()> {
    blocking(move || {
        let key = settings::server_password(&server.connect());
        let pw = if password.is_empty() { settings::secret(&key).unwrap_or_default() } else { password.clone() };
        let args = launcher::join_args(&server.connect(), &pw)?;
        if remember && !password.is_empty() {
            settings::set_secret(&key, &password)?;
        }
        // the join's mod set: the load order, the maps, and each Workshop item's update time now
        // kept only when the active profile is the server's own (not joined as is, nor a vanilla server)
        let (order, maps, items, own, viewpoint) = reading(|paths| {
            let p = active(paths)?;
            let mods = scan(paths)?;
            let mut items: Vec<String> = p.enabled().iter().filter_map(|id| sources::workshop_item(&mods.get(id)?.source).map(String::from)).collect();
            items.dedup();
            let own = profile::is_for_server(paths, &p.name, &server.addr);
            Ok((p.enabled(), profile::map_order(paths, &p.name), items, own, profile::kind_of(paths, &p.name) == vp::KIND))
        })
        .map_err(anyhow::Error::msg)?;
        let refs: Vec<&str> = items.iter().map(String::as_str).collect();
        let times: BTreeMap<String, u64> = if own { sources::steam_workshop::details(&refs).unwrap_or_default().into_iter().map(|i| (i.id, i.time_updated)).collect() } else { BTreeMap::new() };
        let items = items.into_iter().map(|i| (i.clone(), times.get(&i).copied().unwrap_or(0))).collect();
        let profile = locked(|paths| {
            launcher::record_join(paths, &server, own.then(|| launcher::Join { time: store::now(), mods: order, maps, items }))?;
            Ok(profile::active_name(paths))
        })
        .map_err(anyhow::Error::msg)?;
        // the server's mod list replaces the profile's in game
        if viewpoint && !server.mods.iter().any(|m| m == vp::MOD_ID) {
            pzmm_core::say!("leavingViewpoint", "Leaving Project Viewpoint for this server");
        }
        pzmm_core::say!("joining", "Starting the game to join {server} with the profile \"{profile}\".", server = server.name, profile = profile);
        start_game(&app, &args).map(|_| ())
    })
    .await
}

#[derive(Serialize)]
struct LauncherView {
    #[serde(flatten)]
    state: launcher::State,
    /// A Steam Web API key is saved (the value never leaves the backend).
    key_saved: bool,
    /// Countries are on (Settings `countries`).
    countries: bool,
}

#[tauri::command(async)]
fn launcher_view() -> Res<LauncherView> {
    quick_read(|paths| Ok(LauncherView { state: launcher::state(paths), key_saved: settings::secret(settings::STEAM_KEY).is_some(), countries: settings::load().countries }))
}

/// Turn the server browser's countries on or off (on: it asks api.country.is, see pz::geoip).
#[tauri::command(async)]
fn server_countries_set(on: bool) -> Res<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    settings::save(&Settings { countries: on, ..settings::load() }).map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
fn server_favourite(server: launcher::Server, on: bool) -> Res<launcher::State> {
    locked(|paths| launcher::set_favourite(paths, &server, on))
}

/// Whether a join password is remembered for `connect` (`ip:port`).
#[tauri::command(async)]
fn server_password_saved(connect: String) -> bool {
    settings::secret(&settings::server_password(&connect)).is_some()
}

#[tauri::command(async)]
fn server_password_forget(connect: String) -> Res<()> {
    settings::set_secret(&settings::server_password(&connect), "").map_err(|e| format!("{e:#}"))
}

/// The pzmm:// links PZMM was started with (the UI shows the server and asks before joining).
static LINKS: Mutex<Vec<String>> = Mutex::new(vec![]);

#[tauri::command]
fn join_links() -> Vec<String> {
    std::mem::take(&mut *LINKS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Save the Steam Web API key in the credential store (empty: forget it).
#[tauri::command(async)]
fn steam_key_set(key: String) -> Res<()> {
    if !key.trim().bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("A Steam Web API key is letters and digits only.".into());
    }
    settings::set_secret(settings::STEAM_KEY, key.trim()).map_err(|e| format!("{e:#}"))
}

// --- settings ---

#[derive(Serialize)]
struct SettingsView {
    settings: Settings,
    folders: Vec<pz::folders::Folder>,
    /// A GitHub token is saved in the credential store (its value never leaves the backend).
    token_saved: bool,
    /// GITHUB_TOKEN is set, so it's used instead.
    token_env: bool,
    /// A Steam Web API key is saved.
    steam_key_saved: bool,
    backups_size: u64,
}

fn settings_view() -> anyhow::Result<SettingsView> {
    let s = settings::load();
    Ok(SettingsView {
        folders: pz::folders::list(&s),
        token_saved: settings::token_saved(),
        token_env: std::env::var("GITHUB_TOKEN").is_ok_and(|t| !t.trim().is_empty()),
        steam_key_saved: settings::secret(settings::STEAM_KEY).is_some(),
        backups_size: store::backups_size(&Paths::detect()),
        settings: s,
    })
}

#[tauri::command]
async fn settings_get() -> Res<SettingsView> {
    blocking(settings_view).await
}

/// Save everything but the folders (they go through `folder_set`).
#[tauri::command(async)]
fn settings_set(app: AppHandle, settings: Settings) -> Res<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // the folders and the countries switch have commands of their own: the page's copy may be stale
    let disk = settings::load();
    let (on, autostart) = (settings.tray, settings.autostart);
    settings::save(&Settings { folders: disk.folders, countries: disk.countries, ..settings }).map_err(|e| format!("{e:#}"))?;
    {
        use tauri_plugin_autostart::ManagerExt;
        let launch = app.autolaunch();
        if launch.is_enabled().unwrap_or(false) != autostart {
            if autostart { launch.enable() } else { launch.disable() }.map_err(|e| format!("starting with the system: {e}"))?;
        }
    }
    tray(&app, on).map_err(|e| e.to_string())
}

/// Set a folder (empty: back to autodetect); PZMM data can move along with it.
#[tauri::command]
async fn folder_set(key: String, path: String, move_data: bool) -> Res<SettingsView> {
    blocking(move || {
        // the in-process locks only: the core takes the data lock itself where a move allows it
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *GAME_INFO.lock().unwrap_or_else(|e| e.into_inner()) = None; // the game or Zomboid folder may move
        let _paused = TICKING.write().unwrap_or_else(|e| e.into_inner());
        *SCAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let path = Some(path.trim()).filter(|p| !p.is_empty()).map(std::path::PathBuf::from);
        pz::folders::set(&key, path, move_data)?;
        // backups.txt names each backup by its full path: after a move, where they are now
        pz::saves::backups_for_game(&played(&Paths::detect()));
        settings_view()
    })
    .await
}

#[tauri::command]
async fn pick_folder(app: AppHandle, start: String) -> Res<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || -> anyhow::Result<_> {
        let mut dialog = app.dialog().file();
        if std::path::Path::new(&start).is_dir() {
            dialog = dialog.set_directory(&start);
        }
        Ok(dialog.blocking_pick_folder().and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()))
    })
    .await
}

/// Zips (or a folder) to install, picked in the system dialog.
#[tauri::command]
async fn pick_install(app: AppHandle, folder: bool) -> Res<Vec<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || -> anyhow::Result<_> {
        let dialog = app.dialog().file();
        let picked = if folder { dialog.blocking_pick_folder().into_iter().collect() } else { dialog.add_filter("Zip", &["zip"]).blocking_pick_files().unwrap_or_default() };
        Ok(picked.into_iter().filter_map(|p| p.into_path().ok()).map(|p| p.display().to_string()).collect())
    })
    .await
}

/// Cancel the SteamCMD download in progress: what it finished still installs.
#[tauri::command]
fn install_stop() {
    sources::STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Set to stop a dedicated server's SteamCMD install or update (apart from mod downloads').
static SERVER_STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
fn host_install_stop() {
    SERVER_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// `text` with the Windows (or Linux) user name hidden in paths, like the diagnostics.
#[tauri::command]
fn mask_user(text: String) -> String {
    pz::doctor::mask_home_user(&text)
}

/// Save the GitHub token in the OS credential store (empty: forget it).
#[tauri::command(async)]
fn github_token_set(token: String) -> Res<()> {
    settings::set_github_token(&token).map_err(|e| format!("{e:#}"))
}

/// Delete every backup; returns what's left (0).
#[tauri::command(async)]
fn backups_clear() -> Res<u64> {
    locked(|paths| {
        store::clear_backups(paths)?;
        pzmm_core::say!("backupsDeleted", "Deleted all backups.");
        Ok(store::backups_size(paths))
    })
}

/// A bug report's worth of facts, with the user name masked in paths.
#[tauri::command(async)]
fn diagnostics(app: AppHandle) -> Res<String> {
    use std::fmt::Write;
    quick_read(|paths| {
        let mut out = format!("PZMM {} on {} {}\n", app.package_info().version, std::env::consts::OS, std::env::consts::ARCH);
        for f in pz::folders::list(&settings::load()) {
            let by = match f.env {
                Some(env) if f.overridden => format!(", from {env}"),
                _ if f.custom => ", set in Settings".into(),
                _ => String::new(),
            };
            let at = f.path.map_or("not found".into(), |p| p.display().to_string());
            let warnings: String = f.warnings.iter().map(|w| format!("; {w}")).collect();
            writeln!(out, "{}: {at} ({}{by}){warnings}", f.label, f.status)?;
        }
        let p = active(paths)?;
        let mods = scan(paths)?;
        let build = profile::effective_build(paths);
        writeln!(out, "Profile: {} ({}), Build {build}", p.name, profile::kind_of(paths, &p.name))?;
        writeln!(out, "Mods: {} installed, {} enabled", mods.len(), p.enabled().len())?;
        let need_zb = zb::required_by(&p.enabled(), &mods);
        writeln!(out, "ZombieBuddy: needed by {}, agent {}", need_zb.len(), if zb::agent_ready(&mods) { "set up" } else { "not set up" })?;
        if let Some(server) = profile::server_of(paths, &p.name) {
            writeln!(out, "Profile made for server: {server}")?;
        }
        writeln!(out, "Servers joined: {}", launcher::state(paths).history.len())?;
        // the PZMM - Client Workshop mod leaves what the game last reported here
        let game = std::fs::read_to_string(paths.zomboid.join("Lua/pzmm/game.json")).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        match game {
            Some(g) => writeln!(out, "PZMM - Client {}: game {}, {} ({} mods loaded{}), at {}", g["client"].as_str().unwrap_or("?"), g["game"].as_str().unwrap_or("?"),
                g["state"].as_str().unwrap_or("?"), g["mods"].as_array().map_or(0, |m| m.len()), g["server"].as_str().map(|s| format!(", on {s}")).unwrap_or_default(), g["at"])?,
            None => writeln!(out, "PZMM - Client: not seen")?,
        }
        for s in pz::host::load(paths) {
            let run_by = s.external.as_ref().map(|e| format!(", run by {}", e.kind)).unwrap_or_default();
            writeln!(out, "Hosted server {}: {} ({:?}{run_by}), data in {}", s.name, s.build, pz::host::state(&s).0, s.cachedir(paths).display())?;
        }
        let findings = pz::doctor::check(paths, &p.enabled(), &mods, build);
        writeln!(out, "Check setup: {} finding(s)", findings.len())?;
        for f in findings {
            writeln!(out, "- {:?} {}: {}", f.level, f.subject, f.text)?;
        }
        Ok(pz::doctor::mask_home_user(&out))
    })
}

/// The other profiles that enable any of `ids`, with how many of them (Remove says so before it asks).
#[tauri::command(async)]
fn profiles_using(ids: Vec<String>) -> Res<BTreeMap<String, usize>> {
    quick_read(|paths| {
        let active = profile::active_name(paths);
        Ok(profile::list(paths)
            .into_iter()
            .filter(|p| *p != active)
            .filter_map(|p| {
                let on = profile::load(paths, &p).ok()?.enabled();
                let n = ids.iter().filter(|i| on.contains(i)).count();
                (n > 0).then_some((p, n))
            })
            .collect())
    })
}

/// Updates install over the running copy: without `/D=` the installer can fall back to its
/// default folder, leaving shortcuts on the old version (NSIS takes `/D=` last and unquoted).
fn updater() -> tauri_plugin_updater::Builder {
    let b = tauri_plugin_updater::Builder::new();
    match std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.display().to_string())) {
        Some(dir) if cfg!(windows) => b.installer_arg(format!("/D={dir}")),
        _ => b,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_window(app)))
        // pzmm://join/... links (Discord announcements): after single-instance, which forwards them
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // size only: restoring visibility, position or maximised would show the window behind the splash
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::SIZE)
                .with_denylist(&["splash"])
                .build(),
        )
        .plugin(updater().build())
        .plugin(tauri_plugin_process::init())
        // signing in starts PZMM in the tray (Settings: autostart)
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![MINIMISED])))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            pzmm_core::set_log_sink(move |line| {
                let _ = handle.emit("log", line);
            });
            let handle = app.handle().clone();
            pzmm_core::set_progress_sink(move |done, total| {
                let _ = handle.emit("progress", (done, total));
            });
            let handle = app.handle().clone();
            pz::host::set_event_sink(move |e| {
                let _ = handle.emit("server", e);
            });
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                // an AppImage has no installer to register pzmm:// links; a dev build takes them over
                // from the installed PZMM so the game's links reach it (reinstalling gives them back)
                #[cfg(any(target_os = "linux", all(windows, debug_assertions)))]
                if let Err(e) = app.deep_link().register_all() {
                    pzmm_core::log!("pzmm:// links: {e}");
                }
                // a link that started PZMM waits for the UI to ask; later ones arrive as events
                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    *LINKS.lock().unwrap_or_else(|e| e.into_inner()) = urls.iter().map(|u| u.to_string()).collect();
                }
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |e| {
                    let _ = handle.emit("join-link", e.urls().iter().map(|u| u.to_string()).collect::<Vec<_>>());
                });
            }
            pzmm_core::sweep_temp(&Paths::detect());
            let minimised = std::env::args().any(|a| a == MINIMISED);
            if (settings::load().tray || minimised)
                && let Err(e) = tray(app.handle(), true)
            {
                pzmm_core::log!("tray icon: {e}");
            }
            // the main window starts hidden: a small splash of its own, centred on the screen, shows while it
            // loads (the page calls splash_done; after 10 s it goes anyway). Started in the tray: neither shows.
            if !minimised {
                if let Err(e) = tauri::WebviewWindowBuilder::new(app, "splash", tauri::WebviewUrl::App("splash.html".into()))
                    .title("PZMM")
                    .inner_size(240.0, 240.0)
                    .decorations(false)
                    .transparent(true)
                    .shadow(false)
                    .resizable(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .center()
                    .build()
                {
                    pzmm_core::log!("splash window: {e}");
                    show_window(app.handle());
                }
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    lift_splash(&handle);
                });
            }
            std::thread::spawn(watch_favourites);
            let handle = app.handle().clone();
            std::thread::spawn(move || watch_game(&handle));
            // the built-in mod ID and map indexes parse once (a moment, seconds in a debug build): now,
            // rather than on the first server's mods
            std::thread::spawn(|| {
                let paths = Paths::detect();
                pz::mod_index::kept_by_id(&paths);
                pz::maps::checked(&paths);
            });
            // a panic (a bug) goes in the activity log too, not only to a console nobody sees
            let default = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                pzmm_core::log!("error: PZMM hit a bug: {info}");
                default(info);
            }));
            // a round that panics is logged (above) and the next one runs
            let round = |f: fn(&Paths)| {
                let _t = TICKING.read().unwrap_or_else(|e| e.into_inner());
                let _ = std::panic::catch_unwind(|| f(&Paths::detect()));
            };
            // schedules, update checks, crash watch and Discord, while PZMM is open
            std::thread::spawn(move || {
                loop {
                    round(pz::autopilot::tick);
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
            });
            // Discord bots: each reads its channel when due (every 5 s, longer after errors)
            std::thread::spawn(move || {
                loop {
                    round(pz::discord_bot::tick);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // the splash closed early (Alt+F4): the main window shows anyway (splash_done closes it the same way)
            if window.label() == "splash" && matches!(event, tauri::WindowEvent::Destroyed) {
                show_window(window.app_handle());
                return;
            }
            // servers PZMM runs: ask before quitting (the UI answers with app_quit); the splash just closes
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                // the page asks: quit or the tray (it answers with app_close)
                let s = settings::load();
                if s.close_ask.unwrap_or(!s.tray) && PAGE_UP.load(std::sync::atomic::Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.emit("close-ask", ());
                    return;
                }
                // Settings say closing goes to the tray: hide the window (the icon brings it back), automation goes on
                // (no tray icon to bring it back, where the system has none: closing quits instead)
                if s.tray && tray(window.app_handle(), true).is_ok() && window.app_handle().tray_by_id("main").is_some() {
                    api.prevent_close();
                    let _ = window.hide();
                    tell_tray(window.app_handle());
                    return;
                }
                let running = pz::host::attached_running();
                if !running.is_empty() {
                    api.prevent_close();
                    let _ = window.emit("servers-running", running);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load, save, install, remove, rollback, profile_new, profile_use, profile_delete, profiles_unused, profiles_prune, profile_rename, profile_duplicate, snapshot_create, snapshot_list, snapshot_restore, set_build,
            details, poster, open_path, open_url, workshop_browse, workshop_info, workshop_changes, mod_changes, feed, news, authoring_check, authoring_poster, authoring_build, workshop_comments, workshop_author, workshop_items, zb_status, zb_preview, zb_install, zb_uninstall, java_mods, workshop_subscribe,
            find_requirement, install_missing, check_updates, update_mods, conflicts, conflict_summary, mod_stats, categories, servers_and_saves, server_maps, server_preview, server_check, server_export, server_import, save_use,
            perf_view, game_running, perf_memory_set, perf_jvm_set, profile_launch, profile_launch_set, pzopt_run, save_sandbox, save_sandbox_write, save_sandbox_preset, save_details, save_backup, save_restore, save_backup_delete, save_repair, save_set_mods, save_players,
            game_requests, game_ack, game_busy, game_look, game_activity_add, game_activity, game_findings, save_restore_from_game, save_backup_from_game, feed_seen, game_settings, game_settings_set, game_report, save_backup_name, save_thumb, save_reset, save_clone, save_databases, save_table,
            host_view, host_ports, host_create, host_preflight, host_start, host_stop, host_restart, host_log, host_send, host_players, host_lines, host_lines_from_profile,
            host_lines_from_collection, host_check, host_lines_with, host_people, host_catalog, host_remote_add, host_remote_forget, host_preview, host_write, host_adopt, host_forget, app_quit, app_close, splash_done, history_list, history_clear, finding_hide, zb_unblock,
            host_auto_set, host_tag, host_tag_set, host_check_updates, host_backups, host_backup, host_restore, host_config, host_ini_write, host_sandbox_write, host_spawn_write, host_preset, host_preset_save,
            host_discord_set, host_discord_test, host_discord_bot_set, host_discord_bot_test, host_address_set, host_ftp_set, host_ftp_browse, host_ftp_forget, host_ftp_pull, host_ftp_push, host_ftp_world,
            host_ftp_stop, host_detect, host_add_external, host_sides_set, host_zones, host_zones_save, host_zones_restore, host_dossier, host_player_set, host_world,
            host_text, host_text_write, host_text_restore, host_update, host_abort_restart, host_history, host_diagnostics, host_game_log, host_clone, host_coop_worlds,
            host_migrate, host_bundle, host_wipe, host_chunk_plan, host_chunk_reset, host_backup_export, host_backup_import, host_workshop_status, tray_words, notify,
            map_view, map_grid, map_world, map_quick_select, map_line, map_order_save, map_export, map_check, map_check_stop, map_scan, map_scan_stop, map_presets, map_preset_set, map_background, map_background_set, map_background_image, pick_image, map_server_compare,
            mode_preview, mode_switch, doctor, scan_dismiss, log_reports, game_log, profile_export, profile_import, collection_preview, collection_profile, lists, preset_save, preset_import, mod_mark, mod_pin, mod_tags, mod_rule, mod_rule_set,
            servers_list, servers_friends, servers_probe, server_countries, server_countries_set, server_players, server_mods, mod_index_refresh, server_setup, server_join, launcher_view, server_favourite, server_password_saved, server_password_forget, steam_key_set, join_links,
            launch_game, settings_get, settings_set, folder_set, pick_folder, pick_install, install_stop, host_install_stop, mask_user, github_token_set, backups_clear, diagnostics, profiles_using
        ])
        .run(tauri::generate_context!())
        .expect("error while running PZMM");
}

#[cfg(test)]
mod tests {
    #[test]
    fn launch_arguments_keep_their_quoted_spans() {
        assert_eq!(super::split_args(r#" -nosteam  -cachedir="D:\PZ Data" -debug"#), ["-nosteam", r"-cachedir=D:\PZ Data", "-debug"]);
        assert_eq!(super::split_args(r#""" x"#), ["", "x"], "an empty quoted argument stays");
    }

    use super::{Row, Running, launch_url};

    #[test]
    fn a_scan_flag_always_clears() {
        static FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let held = Running::take(&FLAG).unwrap();
        assert!(Running::take(&FLAG).is_none(), "one at a time");
        drop(held);
        // a scan that panics (or errors) still lets the next one start
        let _ = std::panic::catch_unwind(|| {
            let _held = Running::take(&FLAG).unwrap();
            panic!("scan blew up");
        });
        assert!(Running::take(&FLAG).is_some());
    }

    #[test]
    fn launch_args_go_in_the_steam_link() {
        assert_eq!(launch_url("  "), "steam://rungameid/108600");
        assert_eq!(launch_url(" -debug  -nosteam "), "steam://run/108600//-debug%20-nosteam/");
        assert_eq!(launch_url("-x a/b&c"), "steam://run/108600//-x%20a%2Fb%26c/");
        // joining a server: the game gets +connect (and +password)
        let join = pzmm_core::pz::launcher::join_args("203.0.113.5:16261", "pw1").unwrap();
        assert_eq!(launch_url(&format!("-debug {join}")), "steam://run/108600//-debug%20%2Bconnect%20203.0.113.5%3A16261%20%2Bpassword%20pw1/");
    }

    /// A mod installed under one profile shows in every other one, off, after its own rows.
    #[test]
    fn unlisted_mods_show_off_in_every_profile() {
        use pzmm_core::profile::{self, Line, Profile};
        let root = std::env::temp_dir().join(format!("pzmm-view-{}", std::process::id()));
        let paths = pzmm_core::pz::Paths { zomboid: root.join("Zomboid"), data: root.join("data") };
        for id in ["A", "New"] {
            let d = paths.mods().join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("mod.info"), format!("name={id}
id={id}
")).unwrap();
        }
        let other = Profile { name: "Other".into(), lines: vec![Line::Separator("Sec".into()), Line::Mod { id: "A".into(), on: true }], order: vec!["A".into()] };
        profile::apply(&paths, &other, None).unwrap();
        // a Viewpoint profile lists them the same way
        profile::set_kind(&paths, "Other", pzmm_core::pz::viewpoint::KIND).unwrap();
        let v = super::view(&paths).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(v.mode, pzmm_core::pz::viewpoint::KIND);
        // Steam's own Workshop folder (if any) adds its mods too: only ours are checked
        let rows: Vec<(String, bool)> = v.rows.iter().filter_map(|r| match r { Row::Sep { name } => Some((format!("# {name}"), false)), Row::Mod { id, on, .. } => ["A", "New"].contains(&id.as_str()).then(|| (id.clone(), *on)) }).collect();
        assert_eq!(rows, [("# Sec".into(), false), ("A".into(), true), ("# ".into(), false), ("New".into(), false)]);
        assert_eq!(v.order, ["A"]);
        let new: Vec<&str> = v.rows.iter().filter_map(|r| match r { Row::Mod { id, new: true, .. } => Some(id.as_str()), _ => None }).collect();
        assert!(new.contains(&"New") && !new.contains(&"A"), "only the appended mods are new");
    }

    /// The UI saves slim rows: everything but kind, id/name and on is optional.
    #[test]
    fn slim_rows_deserialize() {
        let rows: Vec<Row> = serde_json::from_str(r#"[{"kind":"sep","name":"Maps"},{"kind":"mod","id":"A","on":true}]"#).unwrap();
        assert!(matches!(&rows[0], Row::Sep { name } if name == "Maps"));
        assert!(matches!(&rows[1], Row::Mod { id, on: true, missing: false, .. } if id == "A"));
    }
}
