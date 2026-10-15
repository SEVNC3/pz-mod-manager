//! Helpers for making mods: check a mod folder (mod.info, the Build 41 / 42 layout) or a
//! Workshop upload folder the way the game's uploader will, and build that upload folder
//! (`workshop.txt`, `preview.png`, `Contents/mods/<mod>/`) so the upload doesn't fail silently.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::doctor::{Finding, Level};
use super::{decode_text, is_version_dir};

/// The keys the game reads from mod.info (case-sensitive; `key=value`, no space before `=`).
const KEYS: &[&str] = &[
    "name", "id", "description", "poster", "icon", "require", "incompatible", "loadModAfter", "loadModBefore", "author", "modversion", "category", "url",
    "pack", "tiledef", "versionMin", "versionMax",
];
/// Keys that may appear more than once.
const REPEATABLE: &[&str] = &["poster", "pack", "tiledef", "description"];
/// Files the uploader refuses (pyramid.zip, the map pyramid, is the one allowed zip).
const BANNED: &[&str] = &["exe", "dll", "bat", "app", "dylib", "sh", "so", "zip"];
/// Folders the game reads under media/ (and that authors put beside it by mistake).
const MEDIA_DIRS: &[&str] = &["lua", "scripts", "textures", "sound", "models", "maps", "ui", "texturepacks", "anims_X", "AnimSets", "actiongroups", "shaders"];
const VISIBILITY: &[&str] = &["public", "friendsOnly", "private", "unlisted"];

fn finding(level: Level, file: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    Finding { level, subject: file.into(), text: crate::fill(template, vars), key, vars: vars.iter().cloned().collect(), action: None, hidden: false }
}

/// The game or the uploader refuses it, or skips it without a word.
fn bad(file: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    finding(Level::Error, file, key, template, vars)
}

/// Likely a mistake.
fn iffy(file: &str, key: &'static str, template: &str, vars: &[(&'static str, String)]) -> Finding {
    finding(Level::Warning, file, key, template, vars)
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    out.sort();
    out
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.is_file() && p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

/// Is `dir` a Workshop upload folder (rather than a mod)?
pub fn is_upload_folder(dir: &Path) -> bool {
    dir.join("Contents").is_dir() || dir.join("workshop.txt").is_file()
}

/// Everything the game or the uploader would trip over in `dir`: a mod folder, an upload
/// folder (`workshop.txt`, `preview.png`, `Contents/mods/...`) or a folder of mods.
pub fn check(dir: &Path, tags: &[String]) -> Vec<Finding> {
    if !dir.is_dir() {
        return vec![bad(&dir.display().to_string(), "noFolder", "no such folder", &[])];
    }
    let mut out = vec![];
    if is_upload_folder(dir) {
        check_upload(dir, tags, &mut out);
    } else if find_file(dir, "mod.info").is_none() && !subdirs(dir).iter().any(|(n, _)| is_version_dir(n)) && dir.join("mods").is_dir() {
        // a Workshop item's own folder (<item>/mods/<mod>)
        for (_, m) in subdirs(&dir.join("mods")) {
            check_mod(dir, &m, &mut out);
        }
    } else {
        check_mod(dir, dir, &mut out);
    }
    out
}

/// One mod folder: its layout for Build 41 / 42, every mod.info in it, and files the uploader refuses.
fn check_mod(root: &Path, dir: &Path, out: &mut Vec<Finding>) {
    let here = rel(root, dir);
    let here = if here.is_empty() { ".".to_string() } else { here };
    let root_info = find_file(dir, "mod.info");
    let dirs = subdirs(dir);
    let versions: Vec<&(String, PathBuf)> = dirs.iter().filter(|(n, _)| is_version_dir(n)).collect();
    let numbered: Vec<&(String, PathBuf)> = versions.iter().copied().filter(|(n, _)| n != "common").collect();
    for (name, _) in &dirs {
        let lower = name.to_ascii_lowercase();
        if !is_version_dir(name) && (lower.contains("42") || lower.contains("41") || lower == "commons" || lower == "shared") && !MEDIA_DIRS.contains(&name.as_str()) && name != "media" {
            out.push(iffy(&here, "versionFolderName", "{folder}/ looks like a version folder, but the game only reads 42, 42.x and common/", &[("folder", name.clone())]));
        }
    }
    if root_info.is_none() && versions.is_empty() {
        out.push(bad(&here, "noModInfo", "no mod.info: the game won't see this folder as a mod", &[]));
        return;
    }
    if versions.is_empty() {
        out.push(iffy(&here, "b41Only", "Build 41 layout only, so Build 42 won't list it: add common/ and a 42/ folder with its own mod.info, and move media/ into one of them", &[]));
    } else {
        if !versions.iter().any(|(n, _)| n == "common") {
            out.push(iffy(&here, "noCommon", "no common/ folder: Build 42's layout has one beside the version folders (it can be empty), and some game versions skip a mod without it", &[]));
        }
        if numbered.is_empty() {
            out.push(iffy(&here, "noVersionFolder", "only common/: Build 42 normally reads mod.info from a version folder such as 42/", &[]));
        }
        for (name, path) in &numbered {
            if name.split('.').next().and_then(|m| m.parse::<u32>().ok()).is_some_and(|m| m < 42) {
                out.push(iffy(&rel(root, path), "versionBelow42", "version folders are a Build 42 feature: Build 41 never reads this one", &[]));
            } else if find_file(path, "mod.info").is_none() {
                out.push(bad(&rel(root, path), "versionNoInfo", "no mod.info here: Build 42 won't load this version folder", &[]));
            }
        }
        if let Some(f) = find_file(&dir.join("common"), "mod.info").filter(|_| !numbered.is_empty()) {
            out.push(iffy(&rel(root, &f), "infoInCommon", "the game ignores mod.info in common/: each version folder needs its own", &[]));
        }
        if root_info.is_none() && dir.join("media").is_dir() {
            out.push(bad(&rel(root, &dir.join("media")), "rootMedia", "Build 42 ignores media/ at the mod's top level: move it into common/ or a version folder", &[]));
        }
    }
    // media/ placement: lua/, scripts/... belong inside media/
    let mut places: Vec<&Path> = versions.iter().map(|(_, p)| p.as_path()).collect();
    if root_info.is_some() {
        places.push(dir);
    }
    for place in places {
        for (name, path) in subdirs(place) {
            if MEDIA_DIRS.iter().any(|m| m.eq_ignore_ascii_case(&name)) {
                out.push(bad(&rel(root, &path), "outsideMedia", "{folder}/ belongs inside media/: the game doesn't look for it here", &[("folder", name)]));
            }
        }
    }
    // every mod.info: the root (Build 41) and each version folder (Build 42), or common/'s when that's all
    let mut infos: Vec<(PathBuf, bool)> = root_info.iter().map(|p| (p.clone(), false)).collect();
    infos.extend(numbered.iter().filter_map(|(_, p)| find_file(p, "mod.info")).map(|p| (p, true)));
    if numbered.is_empty() {
        infos.extend(find_file(&dir.join("common"), "mod.info").map(|p| (p, true)));
    }
    for (info, b42) in infos {
        let text = decode_text(&fs::read(&info).unwrap_or_default());
        check_info(&text, &rel(root, &info), b42, &[info.parent().unwrap_or(dir), &dir.join("common"), dir], out);
    }
    banned_files(root, dir, out);
}

/// Files the Workshop uploader refuses, anywhere under `dir`.
fn banned_files(root: &Path, dir: &Path, out: &mut Vec<Finding>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            banned_files(root, &p, out);
        } else if is_banned(&p) {
            out.push(bad(&rel(root, &p), "fileType", "the Workshop uploader refuses .exe, .dll, .bat, .sh, .so, .dylib, .app and .zip files (pyramid.zip aside)", &[]));
        }
    }
}

fn is_banned(p: &Path) -> bool {
    let name = p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    name != "pyramid.zip" && p.extension().is_some_and(|x| BANNED.contains(&x.to_string_lossy().to_ascii_lowercase().as_str()))
}

/// One mod.info's text (`file` for messages; `b42`: it's in a version folder; poster and icon
/// paths are looked up in `dirs`: its folder, common/, the mod's).
fn check_info(text: &str, file: &str, b42: bool, dirs: &[&Path], out: &mut Vec<Finding>) {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut id = String::new();
    for (n, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((raw_key, value)) = line.split_once('=') else {
            out.push(iffy(file, "badLine", "line {line} isn't key=value: the game skips it", &[("line", (n + 1).to_string())]));
            continue;
        };
        let (key, value) = (raw_key.trim(), value.trim());
        if raw_key != key {
            out.push(bad(file, "spaceBeforeEquals", "\"{key} =\": the game only reads {key}= with no space before the =", &[("key", key.into())]));
        }
        if !KEYS.contains(&key) {
            match KEYS.iter().find(|k| k.eq_ignore_ascii_case(key)) {
                Some(k) => out.push(bad(file, "wrongCase", "{key}= is ignored: keys are case-sensitive, write {right}=", &[("key", key.into()), ("right", k.to_string())])),
                None => out.push(iffy(file, "unknownKey", "the game ignores {key}=", &[("key", key.into())])),
            }
            continue;
        }
        if seen.insert(key.into(), value.into()).is_some() && !REPEATABLE.contains(&key) {
            out.push(iffy(file, "duplicateKey", "{key}= is set more than once: only one counts", &[("key", key.into())]));
        }
        match key {
            "id" => {
                id = value.to_string();
                if value.contains(['\\', '/', ',', ';', '"']) {
                    out.push(bad(file, "idChars", "id={id} has characters that break mod lists (\\ / , ; \")", &[("id", value.into())]));
                }
            }
            "poster" | "icon" => {
                let found = !value.contains("..") && dirs.iter().any(|d| d.join(value).is_file());
                if value.is_empty() || !found {
                    out.push(bad(file, "missingFile", "{key}={path}: no such file beside this mod.info (or in common/)", &[("key", key.into()), ("path", value.into())]));
                } else if !value.to_ascii_lowercase().ends_with(".png") {
                    out.push(iffy(file, "notPng", "{key}={path}: posters and icons should be PNG", &[("key", key.into()), ("path", value.into())]));
                }
            }
            "require" | "incompatible" | "loadModAfter" | "loadModBefore" => {
                if value.contains(';') {
                    out.push(bad(file, "separator", "{key}=: separate mod ids with commas, the game doesn't split on ;", &[("key", key.into())]));
                }
                for item in value.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
                    if b42 && !item.starts_with('\\') {
                        out.push(iffy(file, "needsBackslash", "{key}={item}: Build 42 writes mod ids as \\{item}", &[("key", key.into()), ("item", item.into())]));
                    } else if !b42 && item.starts_with('\\') {
                        out.push(iffy(file, "noBackslash41", "{key}={item}: Build 41 doesn't use the \\ before mod ids", &[("key", key.into()), ("item", item.into())]));
                    }
                }
            }
            "versionMin" | "versionMax" if !valid_version(value) => {
                out.push(bad(file, "badVersion", "{key}={value} isn't a version the game reads (like 42.0 or 41.78.16)", &[("key", key.into()), ("value", value.into())]));
            }
            "pack" if value.split(',').next().is_none_or(|f| f.trim().is_empty()) => out.push(bad(file, "packFormat", "pack= needs a file name", &[])),
            "tiledef" if value.split_whitespace().nth(1).is_none_or(|n| n.parse::<u32>().is_err()) => {
                out.push(bad(file, "tiledefFormat", "tiledef= needs a file name and a number", &[]));
            }
            "url" if !value.is_empty() && !value.starts_with("https://") && !value.starts_with("http://") => {
                out.push(iffy(file, "badUrl", "url= should start with https://", &[]));
            }
            _ => {}
        }
    }
    let get = |k: &str| seen.get(k).map(String::as_str).unwrap_or_default();
    if get("name").is_empty() {
        out.push(bad(file, "noName", "no name=: the mod shows up without a name", &[]));
    }
    if id.is_empty() {
        out.push(bad(file, "noId", "no id=: the game can't load it, and other mods can't require it", &[]));
    } else {
        for key in ["require", "incompatible"] {
            if get(key).split(',').any(|r| r.trim().trim_start_matches('\\') == id) {
                out.push(iffy(file, "listsItself", "it lists itself in {key}=", &[("key", key.into())]));
            }
        }
    }
    if get("description").is_empty() {
        out.push(iffy(file, "noDescription", "no description=: the in-game mod list shows none", &[]));
    }
    let (min, max) = (get("versionMin"), get("versionMax"));
    if valid_version(min) && valid_version(max) && version_parts(min) > version_parts(max) {
        out.push(bad(file, "minAboveMax", "versionMin is above versionMax: no game version can load it", &[]));
    }
    if b42 && valid_version(max) && version_parts(max) < vec![42] {
        out.push(bad(file, "maxBelow42", "versionMax is below 42 in a Build 42 folder: Build 42 won't load it", &[]));
    }
}

fn valid_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    !v.is_empty() && parts.len() <= 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn version_parts(v: &str) -> Vec<u32> {
    v.split('.').filter_map(|p| p.parse().ok()).collect()
}

/// An upload folder as the uploader checks it, then each mod in it.
fn check_upload(dir: &Path, tags: &[String], out: &mut Vec<Finding>) {
    if let Some(txt) = find_file(dir, "workshop.txt") {
        let w = read_workshop_txt(&txt);
        if w.title.trim().is_empty() {
            out.push(bad("workshop.txt", "noTitle", "no title=: the uploader won't go on without one", &[]));
        } else if w.title.chars().count() > 128 {
            out.push(bad("workshop.txt", "titleLong", "the title is over 128 characters: Steam cuts it", &[]));
        }
        if w.description.chars().count() > 8000 {
            out.push(bad("workshop.txt", "descriptionLong", "the description is over 8000 characters: Steam refuses it", &[]));
        }
        if !w.visibility.is_empty() && !VISIBILITY.contains(&w.visibility.as_str()) {
            out.push(bad("workshop.txt", "badVisibility", "visibility={value} isn't one the game knows (public, friendsOnly, private, unlisted)", &[("value", w.visibility.clone())]));
        }
        if !w.id.is_empty() && !crate::sources::steam_workshop::valid_id(&w.id) {
            out.push(bad("workshop.txt", "badId", "id={value} isn't a Workshop item number", &[("value", w.id.clone())]));
        }
        for tag in w.tags.iter().filter(|t| !tags.is_empty() && !tags.contains(t)) {
            out.push(iffy("workshop.txt", "unknownTag", "{tag} isn't one of the game's Workshop tags", &[("tag", tag.clone())]));
        }
    }
    match find_file(dir, "preview.png") {
        None => out.push(bad("preview.png", "noPreview", "no preview.png: the Workshop needs a 256×256 PNG", &[])),
        Some(p) => {
            let bytes = fs::read(&p).unwrap_or_default();
            match png_size(&bytes) {
                None => out.push(bad("preview.png", "previewFormat", "not a PNG the uploader can read", &[])),
                Some((256, 256)) => {}
                Some((w, h)) => out.push(bad("preview.png", "previewSize", "{size} pixels: it must be exactly 256×256", &[("size", format!("{w}×{h}"))])),
            }
            if bytes.len() > 1_024_000 {
                out.push(bad("preview.png", "previewBig", "over 1000 KB: the uploader refuses it", &[]));
            }
        }
    }
    let contents = dir.join("Contents");
    if !contents.is_dir() {
        out.push(bad("Contents", "noContents", "no Contents/ folder: the uploader needs your mod in Contents/mods/", &[]));
        return;
    }
    let entries: Vec<_> = fs::read_dir(&contents).into_iter().flatten().flatten().collect();
    if entries.is_empty() {
        out.push(bad("Contents", "emptyContents", "Contents/ is empty", &[]));
    }
    for e in &entries {
        let name = e.file_name().to_string_lossy().into_owned();
        if !e.file_type().is_ok_and(|t| t.is_dir()) {
            out.push(bad(&format!("Contents/{name}"), "fileInContents", "Contents/ may only hold folders", &[]));
        } else if !["buildings", "creative", "mods"].contains(&name.as_str()) {
            out.push(bad(&format!("Contents/{name}"), "folderInContents", "Contents/ may only hold buildings/, creative/ and mods/", &[]));
        } else if name != "mods" {
            banned_files(dir, &e.path(), out);
        }
    }
    let mods = contents.join("mods");
    if mods.is_dir() {
        let entries: Vec<_> = fs::read_dir(&mods).into_iter().flatten().flatten().collect();
        if entries.is_empty() {
            out.push(bad("Contents/mods", "emptyMods", "Contents/mods/ has no mod folder", &[]));
        }
        for e in entries {
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                check_mod(dir, &e.path(), out);
            } else {
                out.push(bad(&rel(dir, &e.path()), "fileInMods", "Contents/mods/ may only hold mod folders", &[]));
            }
        }
    }
}

/// A PNG's width and height, from its header.
fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    Some((u32::from_be_bytes(b[16..20].try_into().ok()?), u32::from_be_bytes(b[20..24].try_into().ok()?)))
}

/// What `workshop.txt` holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Upload {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    /// public, friendsOnly, private or unlisted.
    pub visibility: String,
    /// The Workshop item, once uploaded ("" for a new one).
    pub id: String,
}

fn read_workshop_txt(path: &Path) -> Upload {
    let text = decode_text(&fs::read(path).unwrap_or_default());
    let mut w = Upload::default();
    let mut desc = vec![];
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "title" => w.title = v.to_string(),
            "description" => desc.push(v),
            "tags" => w.tags = v.split(';').map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect(),
            "visibility" => w.visibility = v.trim().to_string(),
            "id" => w.id = v.trim().to_string(),
            _ => {}
        }
    }
    w.description = desc.join("\n");
    w
}

fn workshop_txt(w: &Upload) -> String {
    let mut s = String::from("version=1\n");
    if !w.id.trim().is_empty() {
        s += &format!("id={}\n", w.id.trim());
    }
    s += &format!("title={}\n", w.title.replace(['\r', '\n'], " ").trim());
    for line in w.description.lines() {
        s += &format!("description={}\n", line.trim_end_matches('\r'));
    }
    s += &format!("tags={}\n", w.tags.join(";"));
    s += &format!("visibility={}\n", if w.visibility.is_empty() { "public" } else { &w.visibility });
    s
}

/// The Workshop tags the game offers (`media/WorkshopTags.txt` in the game folder).
pub fn allowed_tags(game: Option<&Path>) -> Vec<String> {
    game.and_then(|g| fs::read_to_string(g.join("media/WorkshopTags.txt")).ok())
        .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
        .unwrap_or_default()
}

/// A starting `workshop.txt` for the mod in `dir`: from the upload folder's own if `existing`
/// has one, else from the mod's mod.info (newest version folder first) and its layout.
pub fn draft(dir: &Path, existing: Option<&Path>) -> Upload {
    if let Some(txt) = existing.and_then(|e| find_file(e, "workshop.txt")) {
        return read_workshop_txt(&txt);
    }
    let found = super::find_mods(dir, 2).into_iter().next();
    let info = found.as_ref().map(|m| super::read_info(&m.info)).unwrap_or_default();
    let mut tags = vec![];
    if found.as_ref().is_some_and(|m| m.supports_41) {
        tags.push("Build 41".to_string());
    }
    if found.as_ref().is_some_and(|m| m.supports_42) {
        tags.push("Build 42".to_string());
    }
    let description = info.get("description").cloned().unwrap_or_default().replace("<LINE>", "\n").replace("<BR>", "\n");
    Upload { title: info.get("name").cloned().unwrap_or_default(), description, tags, visibility: "public".into(), id: String::new() }
}

/// The poster the mod's mod.info names (newest version folder first), for the preview image.
pub fn poster(dir: &Path) -> Option<PathBuf> {
    let m = super::find_mods(dir, 2).into_iter().next()?;
    let name = super::read_info(&m.info).remove("poster").filter(|n| !n.contains(".."))?;
    let base = m.info.parent()?;
    [base.join(&name), m.dir.join("common").join(&name), m.dir.join(&name)].into_iter().find(|p| p.is_file())
}

/// Make or refresh the upload folder `<parent>/<mod folder name>/`: `workshop.txt` from `w`,
/// `preview.png` from `preview` (kept as it is when None), and `Contents/mods/<mod>/` copied
/// fresh from `mod_dir`, without hidden files and files the uploader refuses (returned, by path).
pub fn build(mod_dir: &Path, parent: &Path, w: &Upload, preview: Option<&[u8]>) -> Result<(PathBuf, Vec<String>)> {
    // typed or pasted (quotes and all): a relative path would land wherever PZMM runs from
    let full = |p: &Path| -> Result<PathBuf> {
        let p = PathBuf::from(p.to_string_lossy().trim().trim_matches('"'));
        if !p.is_absolute() {
            bail!(crate::keyed!("notFullPath", "{path} isn't a full path (like D:\\Games\\Zomboid)", path = p.display()));
        }
        Ok(std::path::absolute(p)?)
    };
    let (mod_dir, parent) = (full(mod_dir)?, full(parent)?);
    let parent = parent.as_path();
    let Some(name) = mod_dir.file_name().map(|n| n.to_string_lossy().into_owned()) else { bail!("pick a mod folder") };
    let dest = parent.join(&name);
    // as written and as they really are (a junction or link can lead one into the other)
    let real = fs::canonicalize(&mod_dir).ok().zip(fs::canonicalize(parent).ok().map(|p| p.join(&name)));
    let nested = |a: &Path, b: &Path| a.starts_with(b) || b.starts_with(a);
    if nested(&dest, &mod_dir) || real.is_some_and(|(m, d)| nested(&d, &m)) {
        bail!("the upload folder can't be inside the mod folder, or the other way round: pick another place for it");
    }
    if is_upload_folder(&mod_dir) {
        bail!("{} is already an upload folder: pick the mod folder itself", mod_dir.display());
    }
    if dest.exists() && fs::read_dir(&dest)?.next().is_some() && !is_upload_folder(&dest) {
        bail!("{} already exists and isn't an upload folder: pick another place", dest.display());
    }
    if let Some(p) = preview
        && png_size(p) != Some((256, 256))
    {
        bail!("the preview must be a 256×256 PNG");
    }
    let target = dest.join("Contents/mods").join(&name);
    if target.exists() {
        // the last build's copy: to the Recycle Bin (tests delete it), in case it held work
        if cfg!(test) {
            fs::remove_dir_all(&target)?;
        } else {
            trash::delete(&target).with_context(|| format!("moving {} to the Recycle Bin", target.display()))?;
            crate::say!("uploadCopyRecycled", "Moved the previous copy in {folder} to the Recycle Bin.", folder = target.display());
        }
    }
    let mut skipped = vec![];
    copy_clean(&mod_dir, &target, &mod_dir, &mut skipped)?;
    let mut w = w.clone();
    if w.id.trim().is_empty() {
        // a rebuild keeps the item id the game wrote after the first upload
        w.id = find_file(&dest, "workshop.txt").map(|t| read_workshop_txt(&t).id).unwrap_or_default();
    }
    crate::write_atomic(&dest.join("workshop.txt"), workshop_txt(&w))?;
    if let Some(p) = preview {
        crate::write_atomic(&dest.join("preview.png"), p)?;
    }
    Ok((dest, skipped))
}

fn copy_clean(from: &Path, to: &Path, root: &Path, skipped: &mut Vec<String>) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let junk = name.starts_with('.') || ["thumbs.db", "desktop.ini"].contains(&name.to_ascii_lowercase().as_str());
        let kind = e.file_type()?;
        if junk || (kind.is_file() && is_banned(&p)) {
            skipped.push(rel(root, &p));
        } else if kind.is_dir() {
            copy_clean(&p, &to.join(&name), root, skipped)?;
        } else if kind.is_file() {
            fs::copy(&p, to.join(&name))?;
        } else {
            // a symlink or junction: listed as left out, never dropped quietly
            skipped.push(rel(root, &p));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG256: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR\0\0\x01\0\0\0\x01\0\x08\x02\0\0\0";

    fn write(root: &Path, rel: &str, text: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn keys(f: &[Finding]) -> Vec<(&str, &str)> {
        f.iter().map(|f| (f.key, f.subject.as_str())).collect()
    }

    #[test]
    fn mod_info_problems() {
        let mut out = vec![];
        let text = "name = Thing\nid=My;Mod\nRequire=\\A\nposter=missing.png\nrequire=A;\\B\nversionMin=42.x\nversionMax=41\nicon=../x.png\nfoo=1\njust words\n";
        let t = tempfile::tempdir().unwrap();
        check_info(text, "42/mod.info", true, &[t.path()], &mut out);
        assert_eq!(
            out.iter().map(|f| f.key).collect::<Vec<_>>(),
            [
                "spaceBeforeEquals", "idChars", "wrongCase", "missingFile", "separator", "needsBackslash", "badVersion", "missingFile", "unknownKey", "badLine",
                "noDescription", "maxBelow42"
            ]
        );
        assert_eq!(out[2].text, "Require= is ignored: keys are case-sensitive, write require=");

        let mut out = vec![];
        write(t.path(), "common/poster.png", PNG256);
        let ok = "name=Thing\nid=Thing\ndescription=Hi\nposter=poster.png\nrequire=\\Other\nversionMin=42.0\n";
        check_info(ok, "42/mod.info", true, &[&t.path().join("42"), &t.path().join("common")], &mut out);
        assert_eq!(keys(&out), [], "a poster in common/ counts");
        check_info("name=A\nid=A\ndescription=x\nrequire=\\B,A\n", "mod.info", false, &[t.path()], &mut out);
        assert_eq!(out.iter().map(|f| f.key).collect::<Vec<_>>(), ["noBackslash41", "listsItself"]);
    }

    #[test]
    fn a_missing_folder_says_so() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(keys(&check(&t.path().join("nope"), &[])).iter().map(|k| k.0).collect::<Vec<_>>(), ["noFolder"]);
    }

    #[test]
    fn layout_from_41_to_42() {
        let t = tempfile::tempdir().unwrap();
        let info = b"name=M\nid=M\ndescription=d\n";
        // Build 41 only
        write(t.path(), "A/mod.info", info);
        write(t.path(), "A/media/lua/client/a.lua", b"");
        assert_eq!(keys(&check(&t.path().join("A"), &[])), [("b41Only", ".")]);
        // a half-done move to Build 42
        write(t.path(), "B/42/mod.info", b"name=M\nid=N\ndescription=d\n");
        write(t.path(), "B/42.10/lua/x.lua", b"");
        write(t.path(), "B/common/mod.info", info);
        write(t.path(), "B/media/scripts/s.txt", b"");
        write(t.path(), "B/B42/readme.txt", b"");
        write(t.path(), "B/42/media/tool.exe", b"");
        let f = check(&t.path().join("B"), &[]);
        let k = keys(&f);
        for want in [
            ("versionFolderName", "."),
            ("versionNoInfo", "42.10"),
            ("infoInCommon", "common/mod.info"),
            ("rootMedia", "media"),
            ("outsideMedia", "42.10/lua"),
            ("fileType", "42/media/tool.exe"),
        ] {
            assert!(k.contains(&want), "{want:?} in {k:?}");
        }
        assert!(!k.iter().any(|(key, _)| ["noModInfo", "noCommon"].contains(key)));
        // no common/ at all
        write(t.path(), "C/42/mod.info", info);
        assert_eq!(keys(&check(&t.path().join("C"), &[])), [("noCommon", ".")]);
    }

    #[test]
    fn builds_and_checks_an_upload_folder() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("dev/MyMod");
        write(&src, "common/media/lua/client/a.lua", b"print(1)");
        write(&src, "42/mod.info", b"name=My Mod\nid=MyMod\ndescription=Does things\nposter=poster.png\n");
        write(&src, "42/poster.png", PNG256);
        write(&src, ".git/HEAD", b"x");
        write(&src, "build.bat", b"x");
        let out = t.path().join("Workshop");
        let w = draft(&src, None);
        assert_eq!((w.title.as_str(), w.tags.clone()), ("My Mod", vec!["Build 42".to_string()]));
        assert!(poster(&src).is_some());

        let (dest, skipped) = build(&src, &out, &Upload { description: "line 1\nline 2".into(), ..w.clone() }, Some(PNG256)).unwrap();
        assert_eq!(dest, out.join("MyMod"));
        assert_eq!(skipped, [".git", "build.bat"]);
        assert!(dest.join("Contents/mods/MyMod/42/mod.info").is_file() && !dest.join("Contents/mods/MyMod/.git").exists());
        let txt = fs::read_to_string(dest.join("workshop.txt")).unwrap();
        assert_eq!(txt, "version=1\ntitle=My Mod\ndescription=line 1\ndescription=line 2\ntags=Build 42\nvisibility=public\n");
        assert_eq!(keys(&check(&dest, &["Build 42".into()])), []);

        // the game adds id= after the first upload; a rebuild keeps it and replaces the files
        fs::write(dest.join("workshop.txt"), format!("{txt}id=123456\n")).unwrap();
        fs::remove_file(src.join("common/media/lua/client/a.lua")).unwrap();
        build(&src, &out, &w, None).unwrap();
        assert_eq!(read_workshop_txt(&dest.join("workshop.txt")).id, "123456");
        assert!(!dest.join("Contents/mods/MyMod/common/media/lua/client/a.lua").exists(), "fresh copy");
        assert_eq!(draft(&src, Some(&dest)).id, "123456");

        // what the uploader refuses
        write(&dest, "Contents/readme.txt", b"");
        write(&dest, "Contents/maps/x/y", b"");
        fs::write(dest.join("preview.png"), b"nope").unwrap();
        let mut bad = read_workshop_txt(&dest.join("workshop.txt"));
        bad.visibility = "everyone".into();
        bad.tags = vec!["Nope".into()];
        fs::write(dest.join("workshop.txt"), workshop_txt(&bad)).unwrap();
        let found = check(&dest, &["Build 42".into()]);
        let k = keys(&found);
        assert_eq!(
            k,
            [
                ("badVisibility", "workshop.txt"),
                ("unknownTag", "workshop.txt"),
                ("previewFormat", "preview.png"),
                ("folderInContents", "Contents/maps"),
                ("fileInContents", "Contents/readme.txt")
            ]
        );

        assert!(build(&src, &src, &w, None).is_err(), "never inside the mod itself");
        assert!(build(&src, &t.path().join("dev"), &w, None).is_err(), "never over the mod");
        assert!(build(&src, &out, &w, Some(b"not png")).is_err());
        assert!(build(Path::new("dev/MyMod"), &out, &w, None).unwrap_err().to_string().contains("isn't a full path"), "never next to wherever PZMM runs");
        assert_eq!(build(Path::new(&format!(" \"{}\" ", src.display())), &out, &w, None).unwrap().0, dest, "pasted with quotes");
        // nor through a junction that leads into the mod
        #[cfg(windows)]
        {
            let link = t.path().join("link");
            assert!(std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(t.path().join("dev").join("MyMod")).output().unwrap().status.success());
            assert!(build(&src, &link, &w, None).is_err(), "never inside the mod, however it's reached");
        }
    }
}
