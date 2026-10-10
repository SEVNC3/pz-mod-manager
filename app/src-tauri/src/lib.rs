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

fn locked<T>(f: impl FnOnce(&Paths) -> anyhow::Result<T>) -> Res<T> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f(&Paths::detect()).map_err(|e| format!("{e:#}"))
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
}

fn view(paths: &Paths) -> anyhow::Result<View> {
    let (p, adopted) = profile::sync(paths)?;
    let mut mods = store::scan(paths)?;
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
    Ok(View {
        profile: p.name,
        profiles: profile::list(paths),
        rows,
        adopted,
        zomboid: paths.zomboid.display().to_string(),
        workshop: sources::steam_workshop::content_dir(pz::STEAM_APP).map(|d| d.display().to_string()),
        build: profile::build(paths),
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
        locked(|paths| {
            let parsed: Vec<Source> = sources.iter().map(|s| Source::parse(s)).collect();
            let ids = sources::install(paths, &parsed, true);
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
        locked(|paths| {
            let mut seen = std::collections::HashSet::new();
            let all: Vec<Source> = store::load(paths)?
                .into_values()
                .filter(|e| seen.insert(e.source.clone()))
                .filter_map(|e| Source::from_index(&e.source))
                .collect();
            if all.is_empty() {
                pzmm_core::log!("nothing to update: no mods were installed through PZMM");
            }
            sources::install(paths, &all, false);
            view(paths)
        })
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
        if profile::list(paths).contains(&name) {
            anyhow::bail!("a profile named {name:?} already exists");
        }
        let lines = active(paths)?.lines;
        profile::apply(paths, &Profile { name, lines })?;
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
    locked(|paths| {
        let Some(m) = store::scan(paths)?.remove(&id) else { return Ok(None) };
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
    locked(|paths| {
        let Some(m) = store::scan(paths)?.remove(&id) else { return Ok(vec![]) };
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
    let mods = store::scan(paths)?;
    let launcher = zb::game_dir().and_then(|g| zb::launcher_dir(&g));
    let libs = mods.get(zb::MOD_ID).and_then(|m| zb::libs_dir(&m.dir));
    Ok((launcher, libs, mods))
}

#[tauri::command]
fn zb_status() -> Res<ZbStatus> {
    locked(|paths| {
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
    locked(|paths| {
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
        locked(|paths| {
            let (launcher, mut libs, _) = zb_paths(paths)?;
            let Some(l) = launcher else { anyhow::bail!("Project Zomboid's game folder wasn't found in your Steam libraries") };
            if libs.is_none() {
                sources::install(paths, &[Source::Workshop(zb::WORKSHOP_ID.into())], false);
                libs = zb_paths(paths)?.1;
            }
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
        locked(|paths| {
            let index = store::load(paths)?;
            let mut seen = std::collections::HashSet::new();
            let todo: Vec<Source> = ids
                .iter()
                .filter_map(|id| index.get(id))
                .filter(|e| seen.insert(e.source.clone()))
                .filter_map(|e| Source::from_index(&e.source))
                .collect();
            sources::install(paths, &todo, false);
            view(paths)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn conflicts(id: String) -> Res<Vec<pz::files::Conflict>> {
    locked(|paths| {
        let mods = store::scan(paths)?;
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

#[derive(Serialize)]
struct ServersAndSaves {
    servers: Vec<String>,
    server_dir: String,
    saves: Vec<pz::saves::Save>,
}

#[tauri::command]
fn servers_and_saves() -> Res<ServersAndSaves> {
    locked(|paths| {
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
    locked(|paths| {
        let notes = pz::server::export(paths, &name, &active(paths)?.enabled(), &store::scan(paths)?)?;
        notes.iter().for_each(|n| pzmm_core::log!("{n}"));
        Ok(notes)
    })
}

/// Make a profile from a server config (installing missing Workshop items) and use it.
#[tauri::command]
async fn server_import(name: String) -> Res<View> {
    tauri::async_runtime::spawn_blocking(move || {
        locked(|paths| {
            let cfg = pz::server::read(&pz::server::path(paths, &name)?)?;
            let have = sources::workshop_available(paths);
            let missing: Vec<Source> = cfg.workshop.iter().filter(|w| !have.contains(*w)).map(|w| Source::Workshop(w.clone())).collect();
            if !missing.is_empty() {
                pzmm_core::log!("installing {} Workshop item(s) the server uses", missing.len());
                sources::install(paths, &missing, false);
            }
            let lines = cfg.mods.into_iter().map(|id| Line::Mod { id, on: true }).collect();
            profile::apply(paths, &Profile { name: format!("Server {name}"), lines })?;
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
        let lines = save.mods.into_iter().map(|id| Line::Mod { id, on: true }).collect();
        profile::apply(paths, &Profile { name: format!("Save {name}"), lines })?;
        view(paths)
    })
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
            check_updates, update_mods, conflicts, servers_and_saves, server_export, server_import, save_use
        ])
        .run(tauri::generate_context!())
        .expect("error while running PZMM");
}
