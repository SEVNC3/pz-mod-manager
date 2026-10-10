//! Desktop shell: exposes pzmm-core to the UI as Tauri commands.

use std::collections::BTreeMap;
use std::sync::Mutex;

use pzmm_core::profile::{self, Line, Profile};
use pzmm_core::pz::{self, Paths};
use pzmm_core::sources::{self, Source};
use pzmm_core::store;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

/// Every command reads and writes the same files; run them one at a time.
static LOCK: Mutex<()> = Mutex::new(());

type Res<T> = Result<T, String>;

/// For commands that may change mods or profiles: takes the locks and drops the scan cache.
fn locked<T>(f: impl FnOnce(&Paths) -> anyhow::Result<T>) -> Res<T> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::detect();
    let _file = pzmm_core::lock_data(&paths).map_err(|e| format!("locking PZMM's data: {e}"))?;
    *SCAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
    f(&paths).map_err(|e| format!("{e:#}"))
}

/// For read-only commands (row clicks, status): same locks, but reuses the scan cache.
fn reading<T>(f: impl FnOnce(&Paths) -> anyhow::Result<T>) -> Res<T> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

/// Download outside the lock (it can take minutes), then install under it.
fn fetch_then<T>(sources: &[Source], resolve: bool, then: impl FnOnce(&Paths, Vec<String>) -> anyhow::Result<T>) -> Res<T> {
    let fetched = sources::fetch(&Paths::detect(), sources, resolve);
    locked(|paths| {
        let ids = sources::install_fetched(paths, &fetched);
        then(paths, ids)
    })
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
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
    },
    Sep {
        name: String,
    },
}

#[derive(Serialize)]
struct View {
    profile: String,
    profiles: Vec<String>,
    rows: Vec<Row>,
    /// The game's mod list was changed in-game and the profile picked that up.
    adopted: bool,
    zomboid: String,
    workshop: Option<String>,
    build: Option<u8>,
    /// Kind of the active profile: "zomboid" or "viewpoint".
    mode: String,
    /// Profile name -> kind, so the profile picker can show one mode's profiles.
    kinds: BTreeMap<String, String>,
}

fn view(paths: &Paths) -> anyhow::Result<View> {
    let (p, adopted) = profile::sync(paths)?;
    let mut mods = scan(paths)?;
    let mut rows = vec![];
    for line in p.lines {
        match line {
            Line::Separator(name) => rows.push(Row::Sep { name }),
            Line::Mod { id, on } => match mods.remove(&id) {
                Some(m) => rows.push(Row::Mod { id, name: m.name, source: m.source, on, missing: false, requires: m.requires }),
                None if on => rows.push(Row::Mod { id, name: String::new(), source: String::new(), on, missing: true, requires: vec![] }),
                None => {} // disabled and gone: drop it
            },
        }
    }
    let mut rest: Vec<store::Row> = mods.into_values().collect();
    rest.sort_by_key(|m| (if m.name.is_empty() { &m.id } else { &m.name }).to_lowercase());
    rows.extend(rest.into_iter().map(|m| Row::Mod { id: m.id, name: m.name, source: m.source, on: false, missing: false, requires: m.requires }));
    let mode = profile::kind_of(paths, &p.name);
    Ok(View {
        profile: p.name,
        profiles: profile::list(paths),
        rows,
        adopted,
        zomboid: paths.zomboid.display().to_string(),
        workshop: sources::steam_workshop::content_dir(pz::STEAM_APP).map(|d| d.display().to_string()),
        build: profile::build(paths),
        mode,
        kinds: profile::list(paths).into_iter().map(|n| { let kind = profile::kind_of(paths, &n); (n, kind) }).collect(),
    })
}

fn active(paths: &Paths) -> anyhow::Result<Profile> {
    Ok(profile::sync(paths)?.0)
}

#[tauri::command]
fn load() -> Res<View> {
    locked(view)
}

/// Save the whole list (order, on/off, separators) to the active profile and the game.
#[tauri::command]
fn save(rows: Vec<Row>) -> Res<View> {
    locked(|paths| {
        let mut p = active(paths)?;
        p.lines = rows
            .into_iter()
            .map(|r| match r {
                Row::Mod { id, on, .. } => Line::Mod { id, on },
                Row::Sep { name } => Line::Separator(name),
            })
            .collect();
        profile::apply(paths, &p)?;
        view(paths)
    })
}

#[tauri::command]
async fn install(sources: Vec<String>, enable: bool) -> Res<View> {
    tauri::async_runtime::spawn_blocking(move || {
        let parsed: Vec<Source> = sources.iter().map(|s| Source::parse(s)).collect();
        fetch_then(&parsed, true, |paths, ids| {
            if enable && !ids.is_empty() {
                let mut p = active(paths)?;
                ids.iter().for_each(|i| p.set(i, true));
                profile::apply(paths, &p)?;
            }
            view(paths)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn update_all() -> Res<View> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut seen = std::collections::HashSet::new();
        let all: Vec<Source> = store::load(&Paths::detect())
            .map_err(|e| format!("{e:#}"))?
            .into_values()
            .filter(|e| seen.insert(e.source.clone()))
            .filter_map(|e| Source::from_index(&e.source))
            .collect();
        if all.is_empty() {
            pzmm_core::log!("nothing to update: no mods were installed through PZMM");
        }
        fetch_then(&all, false, |paths, _| view(paths))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn remove(ids: Vec<String>) -> Res<View> {
    locked(|paths| {
        let refused = store::remove(paths, &ids)?;
        for id in &refused {
            pzmm_core::log!("{id}: not installed by PZMM (Workshop subscription or manual install); left alone");
        }
        let mut p = active(paths)?;
        ids.iter().filter(|i| !refused.contains(i)).for_each(|i| p.forget(i));
        profile::apply(paths, &p)?;
        view(paths)
    })
}

#[tauri::command]
fn rollback(id: String) -> Res<View> {
    locked(|paths| {
        store::rollback(paths, &id)?;
        pzmm_core::log!("{id}: restored the previous version");
        view(paths)
    })
}

#[tauri::command]
fn profile_new(name: String) -> Res<View> {
    locked(|paths| {
        if profile::exists(paths, &name) {
            anyhow::bail!("a profile named {name:?} already exists");
        }
        let current = active(paths)?;
        // a copy of a Viewpoint profile is a Viewpoint profile too
        profile::set_kind(paths, &name, &profile::kind_of(paths, &current.name))?;
        profile::apply(paths, &Profile { name, lines: current.lines })?;
        view(paths)
    })
}

#[tauri::command]
fn profile_use(name: String) -> Res<View> {
    locked(|paths| {
        profile::apply(paths, &profile::load(paths, &name)?)?;
        view(paths)
    })
}

#[tauri::command]
fn profile_delete(name: String) -> Res<View> {
    locked(|paths| {
        profile::delete(paths, &name)?;
        view(paths)
    })
}

#[tauri::command]
fn set_build(build: Option<u8>) -> Res<View> {
    locked(|paths| {
        profile::set_b41(paths, build.map(|b| b == 41))?;
        profile::apply(paths, &active(paths)?)?;
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
}

#[tauri::command]
fn details(id: String) -> Res<Option<Details>> {
    reading(|paths| {
        let Some(m) = scan(paths)?.remove(&id) else { return Ok(None) };
        let ours = store::load(paths)?.contains_key(&id);
        Ok(Some(Details {
            info: pz::read_info(&m.info).into_iter().collect(),
            has_backup: store::has_backup(paths, &id),
            dir: m.dir.display().to_string(),
            id: m.id,
            name: m.name,
            source: m.source,
            ours,
        }))
    })
}

/// The mod's poster image as raw bytes (empty if none).
#[tauri::command]
fn poster(id: String) -> Res<tauri::ipc::Response> {
    reading(|paths| {
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

#[tauri::command]
async fn workshop_browse(search: String, sort: String, page: u32, build: Option<u8>) -> Res<Browse> {
    tauri::async_runtime::spawn_blocking(move || {
        let tag = build.map(|b| format!("Build {b}"));
        let res = sources::steam_workshop::browse(pz::STEAM_APP, &search, &sort, page, tag.as_deref()).map_err(|e| format!("{e:#}"))?;
        let have = sources::workshop_available(&Paths::detect());
        Ok(Browse {
            total: res.total,
            items: res.items.into_iter().map(|item| BrowseItem { have: have.contains(&item.id), item }).collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
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
}

/// (launcher folder, ZombieBuddy's libs folder, every mod)
type ZbPaths = (Option<std::path::PathBuf>, Option<std::path::PathBuf>, BTreeMap<String, store::Row>);

fn zb_paths(paths: &Paths) -> anyhow::Result<ZbPaths> {
    let mods = scan(paths)?;
    let (launcher, libs) = zb::locate(&mods);
    Ok((launcher, libs, mods))
}

#[tauri::command]
fn zb_status() -> Res<ZbStatus> {
    reading(|paths| {
        let (launcher, libs, mods) = zb_paths(paths)?;
        let enabled = active(paths)?.enabled();
        let required_by = enabled
            .iter()
            .filter(|id| mods.get(*id).is_some_and(|m| m.requires.iter().any(|r| r == zb::MOD_ID)))
            .cloned()
            .collect();
        Ok(ZbStatus {
            game_dir: launcher.as_ref().map(|d| d.display().to_string()),
            have_mod: libs.is_some(),
            files_current: launcher.as_deref().zip(libs.as_deref()).is_some_and(|(l, s)| zb::files_current(l, s)),
            launcher_patched: launcher.as_deref().is_some_and(zb::launcher_patched),
            enabled: enabled.iter().any(|i| i == zb::MOD_ID),
            required_by,
        })
    })
}

/// What setting up (or removing) ZombieBuddy would change, for the confirmation dialog.
#[tauri::command]
fn zb_preview(remove: bool) -> Res<Vec<String>> {
    reading(|paths| {
        let (launcher, libs, _) = zb_paths(paths)?;
        let Some(l) = launcher else { anyhow::bail!("Project Zomboid's game folder wasn't found in your Steam libraries") };
        if remove {
            return zb::uninstall(&l, true);
        }
        let mut lines = match libs {
            Some(s) => zb::install(&l, &s, true)?,
            None => vec![
                format!("download ZombieBuddy (Workshop item {}) into Zomboid/mods", zb::WORKSHOP_ID),
                format!("copy its Java agent files to {}", l.display()),
                format!("add {} to {}", zb::launch_arg(), l.join("ProjectZomboid64.json").display()),
            ],
        };
        lines.push("enable ZombieBuddy in the active profile".into());
        Ok(lines)
    })
}

#[tauri::command]
async fn zb_install() -> Res<View> {
    tauri::async_runtime::spawn_blocking(|| {
        let need_mod = locked(|paths| Ok(zb_paths(paths)?.1.is_none()))?;
        let fetch: Vec<Source> = if need_mod { vec![Source::Workshop(zb::WORKSHOP_ID.into())] } else { vec![] };
        fetch_then(&fetch, false, |paths, _| {
            let (launcher, libs, _) = zb_paths(paths)?;
            let Some(l) = launcher else { anyhow::bail!("Project Zomboid's game folder wasn't found in your Steam libraries") };
            let Some(s) = libs else { anyhow::bail!("couldn't get the ZombieBuddy mod from the Workshop") };
            for c in zb::install(&l, &s, false)? {
                pzmm_core::log!("{c}");
            }
            let mut p = active(paths)?;
            p.set(zb::MOD_ID, true);
            profile::apply(paths, &p)?;
            pzmm_core::log!("ZombieBuddy is set up. Java mods ask for your approval the first time they load.");
            view(paths)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn zb_uninstall() -> Res<View> {
    locked(|paths| {
        let Some(l) = zb_paths(paths)?.0 else { anyhow::bail!("Project Zomboid's game folder wasn't found in your Steam libraries") };
        for c in zb::uninstall(&l, false)? {
            pzmm_core::log!("{c}");
        }
        view(paths)
    })
}

// --- missing requirements ---

/// The Workshop item a mod came from, if any (`steam:<id>` or `workshop:<id>`).
fn workshop_item(row: &store::Row) -> Option<&str> {
    row.source.strip_prefix("steam:").or_else(|| row.source.strip_prefix("workshop:"))
}

/// Workshop items that may provide `mod_id`, which the installed mod `needed_by` requires.
#[tauri::command]
async fn find_requirement(mod_id: String, needed_by: String) -> Res<Vec<sources::Candidate>> {
    tauri::async_runtime::spawn_blocking(move || {
        let hint = scan(&Paths::detect()).ok().and_then(|m| m.get(&needed_by).and_then(workshop_item).map(String::from));
        let build = profile::effective_build(&Paths::detect());
        sources::find_mod(&mod_id, hint.as_deref(), build).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Install every requirement of enabled mods that isn't installed, when the Workshop
/// has a confirmed match (its description declares the mod id). Returns ids still missing.
#[tauri::command]
async fn install_missing() -> Res<(View, Vec<String>)> {
    tauri::async_runtime::spawn_blocking(|| {
        // (missing id, Workshop item of a mod that needs it), read under the lock
        let (wanted, build) = locked(|paths| {
            let mods = scan(paths)?;
            let mut wanted: Vec<(String, Option<String>)> = vec![];
            for id in active(paths)?.enabled() {
                let Some(m) = mods.get(&id) else { continue };
                for r in &m.requires {
                    if !mods.contains_key(r) && !wanted.iter().any(|(w, _)| w == r) {
                        wanted.push((r.clone(), workshop_item(m).map(String::from)));
                    }
                }
            }
            Ok((wanted, profile::effective_build(paths)))
        })?;
        // look up and download without holding the lock
        let (mut items, mut unresolved) = (vec![], vec![]);
        for (id, hint) in wanted {
            match sources::find_mod(&id, hint.as_deref(), build) {
                Ok(c) if c.first().is_some_and(|c| c.sure()) => {
                    pzmm_core::log!("{id}: found on the Workshop as \"{}\" ({})", c[0].item.title, c[0].item.id);
                    items.push(Source::Workshop(c[0].item.id.clone()));
                }
                Ok(_) => unresolved.push(id),
                Err(e) => {
                    pzmm_core::log!("error: looking up {id}: {e:#}");
                    unresolved.push(id);
                }
            }
        }
        fetch_then(&items, true, |paths, ids| {
            let mut p = active(paths)?;
            ids.iter().for_each(|id| p.set(id, true));
            profile::apply(paths, &p)?;
            for id in &unresolved {
                pzmm_core::log!("{id}: no Workshop item clearly provides it. Use Download in the mod's details to pick one.");
            }
            Ok((view(paths)?, unresolved))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// --- updates, conflicts, servers, saves ---

#[tauri::command]
async fn check_updates() -> Res<Vec<String>> {
    tauri::async_runtime::spawn_blocking(|| sources::check_updates(&Paths::detect()).map_err(|e| format!("{e:#}")))
        .await
        .map_err(|e| e.to_string())?
}

/// Re-download specific mods PZMM installed.
#[tauri::command]
async fn update_mods(ids: Vec<String>) -> Res<View> {
    tauri::async_runtime::spawn_blocking(move || {
        let index = locked(store::load)?;
        let mut seen = std::collections::HashSet::new();
        let todo: Vec<Source> = ids
            .iter()
            .filter_map(|id| index.get(id))
            .filter(|e| seen.insert(e.source.clone()))
            .filter_map(|e| Source::from_index(&e.source))
            .collect();
        fetch_then(&todo, false, |paths, _| view(paths))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn conflicts(id: String) -> Res<Vec<pz::files::Conflict>> {
    reading(|paths| {
        let mods = scan(paths)?;
        let enabled = active(paths)?.enabled();
        let Some(mine) = mods.get(&id) else { return Ok(vec![]) };
        let my_pos = enabled.iter().position(|e| *e == id).unwrap_or(usize::MAX);
        let others: Vec<(String, std::path::PathBuf, usize)> = enabled
            .iter()
            .enumerate()
            .filter(|(_, e)| **e != id)
            .filter_map(|(pos, e)| mods.get(e).map(|m| (e.clone(), m.dir.clone(), pos)))
            .collect();
        Ok(pz::files::conflicts(&mine.dir, my_pos, &others))
    })
}

/// For every enabled mod: which enabled mods it overrides, and is overridden by.
#[tauri::command]
async fn conflict_summary() -> Res<std::collections::HashMap<String, pz::files::Overlap>> {
    tauri::async_runtime::spawn_blocking(|| {
        reading(|paths| {
            let mods = scan(paths)?;
            let order: Vec<(String, std::path::PathBuf)> =
                active(paths)?.enabled().into_iter().filter_map(|id| mods.get(&id).map(|m| (id, m.dir.clone()))).collect();
            Ok(pz::files::overlaps(&order))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct ServersAndSaves {
    servers: Vec<String>,
    server_dir: String,
    saves: Vec<pz::saves::Save>,
}

#[tauri::command]
fn servers_and_saves() -> Res<ServersAndSaves> {
    reading(|paths| {
        Ok(ServersAndSaves {
            servers: pz::server::list(paths),
            server_dir: pz::server::dir(paths).display().to_string(),
            saves: pz::saves::list(paths),
        })
    })
}

/// Write the active profile into `Server/<name>.ini`. Returns notes for the user.
#[tauri::command]
fn server_export(name: String) -> Res<Vec<String>> {
    reading(|paths| {
        let notes = pz::server::export(paths, &name, &active(paths)?.enabled(), &scan(paths)?)?;
        notes.iter().for_each(|n| pzmm_core::log!("{n}"));
        Ok(notes)
    })
}

/// Make a profile from a server config (installing missing Workshop items) and use it.
#[tauri::command]
async fn server_import(name: String) -> Res<View> {
    tauri::async_runtime::spawn_blocking(move || {
        let (cfg, missing) = locked(|paths| {
            let cfg = pz::server::read(&pz::server::path(paths, &name)?)?;
            let have = sources::workshop_available(paths);
            let missing: Vec<Source> = cfg.workshop.iter().filter(|w| !have.contains(*w)).map(|w| Source::Workshop(w.clone())).collect();
            Ok((cfg, missing))
        })?;
        if !missing.is_empty() {
            pzmm_core::log!("installing {} Workshop item(s) the server uses", missing.len());
        }
        fetch_then(&missing, false, |paths, _| {
            profile::apply(paths, &Profile::from_ids(&format!("Server {name}"), cfg.mods))?;
            view(paths)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Make a profile from a save's mod list and use it.
#[tauri::command]
fn save_use(mode: String, name: String) -> Res<View> {
    locked(|paths| {
        let save = pz::saves::list(paths).into_iter().find(|s| s.mode == mode && s.name == name);
        let Some(save) = save else { anyhow::bail!("save {mode}/{name} not found") };
        profile::apply(paths, &Profile::from_ids(&format!("Save {name}"), save.mods))?;
        view(paths)
    })
}

// --- Project Zomboid / Project Viewpoint ---

use pzmm_core::pz::viewpoint as vp;

/// What switching to `mode` would do, for the confirmation panel.
#[tauri::command]
fn mode_preview(mode: String) -> Res<Vec<String>> {
    reading(|paths| vp::preview(paths, &mode, &scan(paths)?))
}

/// Switch between plain Project Zomboid and Project Viewpoint profiles, setting up
/// Viewpoint (downloads, ZombieBuddy agent, profile) the first time.
#[tauri::command]
async fn mode_switch(mode: String) -> Res<View> {
    tauri::async_runtime::spawn_blocking(move || {
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
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn open_path(app: AppHandle, path: String) -> Res<()> {
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Res<()> {
    if !(url.starts_with("https://") || url == format!("steam://rungameid/{}", pz::STEAM_APP)) {
        return Err(format!("refusing to open {url}"));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            pzmm_core::set_log_sink(move |line| {
                let _ = handle.emit("log", line.to_string());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load, save, install, update_all, remove, rollback, profile_new, profile_use, profile_delete, set_build,
            details, poster, open_path, open_url, workshop_browse, zb_status, zb_preview, zb_install, zb_uninstall,
            find_requirement, install_missing, check_updates, update_mods, conflicts, conflict_summary, servers_and_saves, server_export, server_import, save_use,
            mode_preview, mode_switch
        ])
        .run(tauri::generate_context!())
        .expect("error while running PZMM");
}
