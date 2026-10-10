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

/// Run blocking work (downloads, Workshop requests, file walks) off the UI thread.
async fn blocking<T: Send + 'static, E: std::fmt::Display>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(move || f().map_err(|e| format!("{e:#}"))).await.map_err(|e| e.to_string())?
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
        #[serde(default)]
        incompatible: Vec<String>,
        #[serde(default)]
        load_after: Vec<String>,
        #[serde(default)]
        load_before: Vec<String>,
        /// The mod's folders only support Build 41 / only Build 42.
        #[serde(default)]
        only41: bool,
        #[serde(default)]
        only42: bool,
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
    /// First run: this view created the first profile.
    created: bool,
    zomboid: String,
    workshop: Option<String>,
    build: Option<u8>,
    /// Kind of the active profile: "zomboid" or "viewpoint".
    mode: String,
    /// Profile name -> kind, so the profile picker can show one mode's profiles.
    kinds: BTreeMap<String, String>,
}

fn view(paths: &Paths) -> anyhow::Result<View> {
    let created = profile::list(paths).is_empty();
    let (p, adopted) = profile::sync(paths)?;
    let mut mods = scan(paths)?;
    let row = |m: store::Row, on: bool| Row::Mod {
        id: m.id,
        name: m.name,
        source: m.source,
        on,
        missing: false,
        requires: m.requires,
        incompatible: m.incompatible,
        load_after: m.load_after,
        load_before: m.load_before,
        only41: m.supports_41 && !m.supports_42,
        only42: m.supports_42 && !m.supports_41,
    };
    let mut rows = vec![];
    for line in &p.lines {
        match line {
            Line::Separator(name) => rows.push(Row::Sep { name: name.clone() }),
            Line::Mod { id, on } => match mods.remove(id) {
                Some(m) => rows.push(row(m, *on)),
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
                    only41: false,
                    only42: false,
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
    rows.extend(rest.into_iter().map(|m| row(m, false)));
    let mode = profile::kind_of(paths, &p.name);
    Ok(View {
        profile: p.name,
        profiles: profile::list(paths),
        rows,
        adopted,
        created,
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

#[tauri::command(async)]
fn load() -> Res<View> {
    locked(view)
}

/// Save the whole list (order, on/off, separators) to the active profile and the game.
#[tauri::command(async)]
fn save(rows: Vec<Row>) -> Res<View> {
    locked(|paths| {
        let (mut p, adopted) = profile::sync(paths)?;
        if adopted {
            // the rows were drawn before the game's mod menu changed the list; saving them would undo that
            anyhow::bail!("The mod list was changed in the game's mod menu, so PZMM reloaded it. Make your change again.");
        }
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

/// Enable `ids` in order, leaving off any that declares (or is declared) incompatible with
/// a mod already on: Workshop items often ship alternative versions of one mod.
fn enable_compatible(p: &mut Profile, mods: &BTreeMap<String, store::Row>, ids: &[String]) {
    let clashes = |a: &str, b: &str| mods.get(a).is_some_and(|m| m.incompatible.iter().any(|x| x == b));
    for id in ids {
        match p.enabled().into_iter().find(|o| o != id && (clashes(id, o) || clashes(o, id))) {
            Some(o) => pzmm_core::log!("{id} left off: it's incompatible with {o}. Enable it instead of {o} if you prefer it."),
            None => p.set(id, true),
        }
    }
}

#[tauri::command]
async fn install(sources: Vec<String>) -> Res<View> {
    blocking(move || {
        let parsed: Vec<Source> = sources.iter().map(|s| Source::parse(s)).collect();
        fetch_then(&parsed, true, |paths, ids| {
            if !ids.is_empty() {
                let mut p = active(paths)?;
                enable_compatible(&mut p, &scan(paths)?, &ids);
                profile::apply(paths, &p)?;
            }
            view(paths)
        })
    })
    .await
}

#[tauri::command]
async fn update_all() -> Res<View> {
    blocking(|| {
        let mut seen = std::collections::HashSet::new();
        let all: Vec<Source> = reading(store::load)?
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
}

#[tauri::command(async)]
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

#[tauri::command(async)]
fn rollback(id: String) -> Res<View> {
    locked(|paths| {
        store::rollback(paths, &id)?;
        pzmm_core::log!("{id}: restored the previous version");
        view(paths)
    })
}

#[tauri::command(async)]
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

#[tauri::command(async)]
fn profile_use(name: String) -> Res<View> {
    locked(|paths| {
        profile::apply(paths, &profile::load(paths, &name)?)?;
        view(paths)
    })
}

#[tauri::command(async)]
fn profile_delete(name: String) -> Res<View> {
    locked(|paths| {
        profile::delete(paths, &name)?;
        view(paths)
    })
}

#[tauri::command(async)]
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

#[tauri::command(async)]
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
#[tauri::command(async)]
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
        let page = sources::steam_workshop::item_page(&id)?;
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

#[tauri::command]
async fn workshop_comments(creator: String, id: String, start: u32) -> Res<sources::steam_workshop::Comments> {
    blocking(move || sources::steam_workshop::comments(&creator, &id, start)).await
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

#[tauri::command(async)]
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
#[tauri::command(async)]
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
    blocking(|| {
        let need_mod = reading(|paths| Ok(zb_paths(paths)?.1.is_none()))?;
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
}

#[tauri::command(async)]
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
    blocking(move || {
        let hint = scan(&Paths::detect()).ok().and_then(|m| m.get(&needed_by).and_then(workshop_item).map(String::from));
        let build = profile::effective_build(&Paths::detect());
        sources::find_mod(&mod_id, hint.as_deref(), build)
    })
    .await
}

/// Install every requirement of enabled mods that isn't installed, when the Workshop
/// has a confirmed match (its description declares the mod id). Returns ids still missing.
#[tauri::command]
async fn install_missing() -> Res<(View, Vec<String>)> {
    blocking(|| {
        // (missing id, Workshop item of a mod that needs it), read under the lock
        let (wanted, build) = reading(|paths| {
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
        let needed: Vec<String> = wanted.iter().map(|(id, _)| id.clone()).collect();
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
            profile::apply(paths, &p)?;
            for id in &unresolved {
                pzmm_core::log!("{id}: no Workshop item clearly provides it. Use Download in the mod's details to pick one.");
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

/// Re-download specific mods PZMM installed.
#[tauri::command]
async fn update_mods(ids: Vec<String>) -> Res<View> {
    blocking(move || {
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
}

#[tauri::command(async)]
fn conflicts(id: String) -> Res<Vec<pz::files::Conflict>> {
    // collect paths under the lock, walk their files after it (other commands wait on the lock)
    let found = reading(|paths| {
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
}

/// Every mod's category (asks Steam for Workshop tags, so outside the lock).
#[tauri::command]
async fn categories() -> Res<Categories> {
    blocking(|| -> Res<_> {
        let mods = reading(scan)?;
        Ok(Categories { order: pz::category::ORDER, of: pz::category::categorize(&mods) })
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
        Ok(pz::files::overlaps(&order))
    })
    .await
}

#[derive(Serialize)]
struct ServersAndSaves {
    servers: Vec<String>,
    server_dir: String,
    saves: Vec<pz::saves::Save>,
}

#[tauri::command(async)]
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
#[tauri::command(async)]
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
    blocking(move || {
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
}

/// Start the dedicated server with config `name`.
#[tauri::command(async)]
fn server_start(name: String) -> Res<String> {
    reading(|paths| {
        let script = pz::server::start(paths, &name)?;
        pzmm_core::log!("Started the {name} server ({}).", script.display());
        Ok(script.display().to_string())
    })
}

/// Make a profile from a save's mod list and use it.
#[tauri::command(async)]
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

/// The active profile packaged for sharing.
#[tauri::command(async)]
fn profile_export() -> Res<profile::Shared> {
    reading(|paths| profile::export(paths, &scan(paths)?))
}

/// Install what a shared profile needs from the Workshop, then save and use it.
#[tauri::command]
async fn profile_import(shared: profile::Shared) -> Res<View> {
    blocking(move || {
        let needs = reading(|paths| Ok(profile::import_needs(&shared, &scan(paths)?)))?;
        if !needs.is_empty() {
            pzmm_core::log!("installing {} Workshop item(s) the shared profile uses", needs.len());
        }
        fetch_then(&needs, false, |paths, _| {
            let p = profile::import(paths, &shared)?;
            pzmm_core::log!("Imported the profile as \"{}\".", p.name);
            view(paths)
        })
    })
    .await
}

#[derive(Serialize)]
struct ErrorsByMod {
    #[serde(flatten)]
    errors: pz::logs::ModErrors,
    /// The mod id, when an installed mod has that display name.
    id: Option<String>,
}

/// Lua errors in the last game session's console.txt, per mod.
#[tauri::command(async)]
fn console_errors() -> Res<Vec<ErrorsByMod>> {
    reading(|paths| {
        let mods = scan(paths)?;
        Ok(pz::logs::console_errors(&paths.zomboid)
            .into_iter()
            .map(|e| {
                let id = mods.values().find(|m| m.name == e.name || m.id == e.name).map(|m| m.id.clone());
                ErrorsByMod { errors: e, id }
            })
            .collect())
    })
}

/// Health check of the active profile and server configs.
#[tauri::command(async)]
fn doctor() -> Res<Vec<pz::doctor::Finding>> {
    reading(|paths| Ok(pz::doctor::check(paths, &active(paths)?.enabled(), &scan(paths)?, profile::effective_build(paths))))
}

/// What switching to `mode` would do, for the confirmation panel.
#[tauri::command(async)]
fn mode_preview(mode: String) -> Res<Vec<String>> {
    reading(|paths| vp::preview(paths, &mode, &scan(paths)?))
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
        pzmm_core::log!("error: not a folder: {path}");
        return Err(format!("not a folder: {path}"));
    }
    app.opener().open_path(&path, None::<&str>).map_err(|e| {
        pzmm_core::log!("error: couldn't open {path}: {e}");
        e.to_string()
    })
}

#[tauri::command(async)]
fn open_url(app: AppHandle, url: String) -> Res<()> {
    if !(url.starts_with("https://") || url == format!("steam://rungameid/{}", pz::STEAM_APP)) {
        pzmm_core::log!("error: PZMM only opens https links: {url}");
        return Err(format!("refusing to open {url}"));
    }
    // Workshop pages open in the Steam client when it's running (Subscribe works there);
    // otherwise in the browser, since a steam:// link would start Steam
    let workshop = ["https://steamcommunity.com/sharedfiles/", "https://steamcommunity.com/workshop/"].iter().any(|p| url.starts_with(p));
    let url = if workshop && sources::steam_workshop::client_running() { format!("steam://openurl/{url}") } else { url };
    app.opener().open_url(&url, None::<&str>).map_err(|e| {
        pzmm_core::log!("error: couldn't open {url}: {e} (is Steam installed?)");
        e.to_string()
    })
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
            details, poster, open_path, open_url, workshop_browse, workshop_info, workshop_changes, workshop_comments, workshop_author, zb_status, zb_preview, zb_install, zb_uninstall,
            find_requirement, install_missing, check_updates, update_mods, conflicts, conflict_summary, mod_stats, categories, servers_and_saves, server_export, server_import, server_start, save_use,
            mode_preview, mode_switch, doctor, console_errors, profile_export, profile_import
        ])
        .run(tauri::generate_context!())
        .expect("error while running PZMM");
}

#[cfg(test)]
mod tests {
    use super::Row;

    /// The UI saves slim rows: everything but kind, id/name and on is optional.
    #[test]
    fn slim_rows_deserialize() {
        let rows: Vec<Row> = serde_json::from_str(r#"[{"kind":"sep","name":"Maps"},{"kind":"mod","id":"A","on":true}]"#).unwrap();
        assert!(matches!(&rows[0], Row::Sep { name } if name == "Maps"));
        assert!(matches!(&rows[1], Row::Mod { id, on: true, missing: false, .. } if id == "A"));
    }
}
