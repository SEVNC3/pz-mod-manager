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
                Some(m) => rows.push(Row::Mod { id, name: m.name, source: m.source, on, missing: false }),
                None if on => rows.push(Row::Mod { id, name: String::new(), source: String::new(), on, missing: true }),
                None => {} // disabled and gone: drop it
            },
        }
    }
    let mut rest: Vec<store::Row> = mods.into_values().collect();
    rest.sort_by_key(|m| (if m.name.is_empty() { &m.id } else { &m.name }).to_lowercase());
    rows.extend(rest.into_iter().map(|m| Row::Mod { id: m.id, name: m.name, source: m.source, on: false, missing: false }));
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
            let ids = sources::install(paths, &parsed);
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
            sources::install(paths, &all);
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
        .setup(|app| {
            let handle = app.handle().clone();
            pzmm_core::set_log_sink(move |line| {
                let _ = handle.emit("log", line.to_string());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load, save, install, update_all, remove, rollback, profile_new, profile_use, profile_delete, set_build,
            details, poster, open_path, open_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running PZMM");
}
