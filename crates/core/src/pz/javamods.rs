//! Java mods that ZombieBuddy loads (mod.info `javaJarFile=`): the jar's SHA-256, its ZombieBuddy
//! signature (`<jar>.zbs`, checked against the known authors' keys), what ZombieBuddy already
//! decided about this exact jar, and a scan of its class files for calls a game mod rarely
//! needs (starting programs, the network, files, loading more code, private data).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use super::doctor::{Finding, Level};
use crate::store::Row;

#[derive(Debug, serde::Serialize)]
pub struct JavaMod {
    pub id: String,
    pub name: String,
    /// `javaJarFile=` as mod.info has it.
    pub jar: String,
    /// Lowercase hex; None when the jar isn't there.
    pub sha256: Option<String>,
    /// "unsigned", "valid", "invalid" (changed after signing), "unknown_key" (ZombieBuddy looks
    /// the key up on the signer's Steam profile) or "missing" (no jar).
    pub signature: &'static str,
    /// The signer's SteamID64 and, when ZombieBuddy's authors list knows them, their name.
    pub signer: Option<String>,
    pub signer_name: Option<String>,
    /// ZombieBuddy's saved answer for this exact jar: Some(true) loads it, Some(false) blocks it,
    /// None: it asks on the next launch.
    pub approved: Option<bool>,
    /// What the scan found: warnings to look at before allowing it, notes worth knowing.
    pub risks: Vec<Finding>,    /// What loads it: "zombiebuddy", "leaf" or "avrix" (signatures and approvals are ZombieBuddy's only).
    pub loader: &'static str,
}

/// SteamID64 → (name, Ed25519 public keys) from ZombieBuddy's `authors.json` files.
type Authors = HashMap<String, (String, Vec<Vec<u8>>)>;
/// ZombieBuddy's saved decisions: jar SHA-256 → allowed.
type Approvals = HashMap<String, bool>;

fn unhex(s: &str) -> Option<Vec<u8>> {
    s.len().is_multiple_of(2).then_some(())?;
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Add the authors from an `authors.json` (`{"authors": [{"id", "name", "keys": [hex]}]}`).
fn read_authors(file: &Path, out: &mut Authors) {
    let Ok(v) = std::fs::read_to_string(file).map(|t| serde_json::from_str::<serde_json::Value>(&t)) else { return };
    let Ok(v) = v else { return };
    for a in v["authors"].as_array().into_iter().flatten() {
        let id = a["id"].as_u64().map(|i| i.to_string()).or_else(|| a["id"].as_str().map(String::from));
        let Some(id) = id else { continue };
        let e = out.entry(id).or_insert_with(|| (a["name"].as_str().unwrap_or_default().to_string(), vec![]));
        e.1.extend(a["keys"].as_array().into_iter().flatten().filter_map(|k| unhex(k.as_str()?.trim())).filter(|k| k.len() == 32));
    }
}

/// ZombieBuddy's `mod_approvals.json` (`{"mods": [{"jar_hash", "decision"}]}`).
pub fn read_approvals(file: &Path) -> Approvals {
    let v: serde_json::Value = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["jar_hash"].as_str()?.to_ascii_lowercase(), m["decision"].as_bool()?)))
        .collect()
}

/// Allow, in ZombieBuddy's `mod_approvals.json`, the current jar of each mod in `mods` that it has
/// no answer for: turning a Java mod on in PZMM is the player's yes, so ZombieBuddy doesn't ask again
/// at launch. A "no" the player gave in ZombieBuddy stands, a jar whose scan warns or that loads native
/// code, or whose mod folder's scan warns (unless the player trusted the jar, see [`exceptions`]), is
/// left for ZombieBuddy to ask about, and so is one with no
/// Workshop item (ZombieBuddy's own entries always carry one). The ids allowed.
pub fn approve(file: &Path, mods: &[&Row], trusted: &HashSet<String>) -> anyhow::Result<Vec<String>> {
    let mut v: serde_json::Value = match std::fs::read_to_string(file) {
        Ok(t) => serde_json::from_str(&t)?, // unreadable: an error rather than saving over the player's answers
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({"formatVersion": 2, "mods": []}),
        Err(e) => return Err(anyhow::Error::from(e).context(format!("reading {}", file.display()))),
    };
    let known = read_approvals(file);
    let mut added = vec![];
    for m in mods {
        let Some((_, jar)) = jar_of(m) else { continue };
        let Some(workshop_id) = crate::sources::workshop_item(&m.source).and_then(|i| i.parse::<u64>().ok()) else { continue };
        let Some((sha, risks)) = hash_and_scan(&m.id, &jar) else { continue };
        let flagged = || risks.iter().any(|r| r.level == Level::Warning || r.key == "javaNative") || scan_folder(m, trusted).iter().any(|f| f.level == Level::Warning);
        if known.contains_key(&sha) || (!trusted.contains(&sha) && flagged()) {
            continue;
        }
        let Some(list) = v["mods"].as_array_mut() else { anyhow::bail!("{} has no mods list", file.display()) };
        list.push(serde_json::json!({"id": m.id, "workshop_id": workshop_id, "jar_hash": sha, "decision": true}));
        added.push(m.id.clone());
    }
    if !added.is_empty() {
        std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")))?;
        crate::write_atomic(file, serde_json::to_vec_pretty(&v)?)?;
    }
    Ok(added)
}

/// Forget ZombieBuddy's "no" for the jar with this SHA-256, so it asks (or PZMM allows it) again.
pub fn unblock(file: &Path, sha: &str) -> anyhow::Result<()> {
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    if let Some(list) = v["mods"].as_array_mut() {
        list.retain(|m| !(m["jar_hash"].as_str().is_some_and(|h| h.eq_ignore_ascii_case(sha)) && m["decision"] == false));
    }
    Ok(crate::write_atomic(file, serde_json::to_vec_pretty(&v)?)?)
}

/// ZombieBuddy's settings folder (`~/.zombie_buddy`).
pub fn zb_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".zombie_buddy"))
}

/// The authors ZombieBuddy knows: the list the official mod ships and the copy it keeps updated.
pub fn known_authors(mods: &BTreeMap<String, Row>) -> Authors {
    let mut out = Authors::new();
    if let Some(zb) = mods.get(super::zombiebuddy::MOD_ID).filter(|m| super::zombiebuddy::official_source(&m.source)) {
        read_authors(&zb.dir.join("authors.json"), &mut out);
    }
    if let Some(d) = zb_config_dir() {
        read_authors(&d.join("authors.json"), &mut out);
    }
    out
}

/// The mod's `javaJarFile=` and where it points (relative to its mod.info, as ZombieBuddy reads it;
/// on Build 42 it may sit in `common/mod.info` while the jar path is relative to the version folder).
fn jar_of(m: &Row) -> Option<(String, PathBuf)> {
    let base = m.info.parent()?;
    let rel = [m.info.clone(), m.dir.join("common").join("mod.info")]
        .iter()
        .find_map(|i| super::read_info(i).remove("javaJarFile").filter(|s| !s.is_empty()))?;
    // a mod.info in common/ names a jar inside a version folder (42/, 42.13/...): the newest that has it
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&m.dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("42"))).collect();
    let number = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().split('.').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>()).unwrap_or_default();
    versions.sort_by_key(number);
    let found = std::iter::once(base.to_path_buf()).chain(versions.into_iter().rev()).chain([m.dir.clone()]).map(|d| d.join(&rel)).find(|p| p.is_file());
    Some((rel.clone(), found.unwrap_or_else(|| base.join(&rel))))
}

/// A `.zbs` file's signer and signature (`SteamID64:<17 digits>`, `Signature:<128 hex>`).
fn read_zbs(file: &Path) -> Option<(String, Vec<u8>)> {
    let text = std::fs::read_to_string(file).ok()?;
    let field = |k: &str| text.lines().find_map(|l| l.trim().strip_prefix(k).map(str::to_string));
    let sid = field("SteamID64:").filter(|s| s.len() == 17 && s.bytes().all(|b| b.is_ascii_digit()))?;
    let sig = unhex(&field("Signature:")?).filter(|s| s.len() == 64)?;
    Some((sid, sig))
}

/// A jar's SHA-256 and scan (findings about mod `id`), kept by path, size and modified time: the
/// Java mods view and Check setup look at every enabled jar each time. None: it can't be read.
fn hash_and_scan(id: &str, file: &Path) -> Option<(String, Vec<Finding>)> {
    type Key = (PathBuf, String, u64, Option<std::time::SystemTime>);
    // ponytail: never pruned; an updated jar leaves its old entry behind (a few per update)
    static CACHE: std::sync::Mutex<BTreeMap<Key, (String, Vec<Finding>)>> = std::sync::Mutex::new(BTreeMap::new());
    let meta = std::fs::metadata(file).ok()?;
    let key = (file.to_path_buf(), id.to_string(), meta.len(), meta.modified().ok());
    if let Some(hit) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Some(hit.clone());
    }
    let bytes = std::fs::read(file).ok()?;
    let got = (sha256(&bytes), scan_jar(id, &bytes));
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).insert(key, got.clone());
    Some(got)
}

/// Check one Java mod. `None` when the mod has no `javaJarFile=`.
pub fn inspect(m: &Row, authors: &Authors, approvals: &Approvals) -> Option<JavaMod> {
    let (rel, jar) = jar_of(m)?;
    let mut out = JavaMod {
        id: m.id.clone(),
        name: m.name.clone(),
        jar: rel.clone(),
        sha256: None,
        signature: "missing",
        signer: None,
        signer_name: None,
        approved: None,
        risks: vec![],
        loader: "zombiebuddy",
    };
    let Some((sha, risks)) = hash_and_scan(&m.id, &jar) else { return Some(out) };
    out.approved = approvals.get(&sha).copied();
    out.signature = "unsigned";
    let zbs = PathBuf::from(format!("{}.zbs", jar.display()));
    if zbs.exists() {
        out.signature = "invalid";
        if let Some((sid, sig)) = read_zbs(&zbs) {
            let payload = format!("ZBS:{sid}:{sha}");
            out.signature = match authors.get(&sid) {
                Some((_, keys)) if keys.iter().any(|k| ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, k).verify(payload.as_bytes(), &sig).is_ok()) => "valid",
                Some((_, keys)) if !keys.is_empty() => "invalid",
                _ => "unknown_key",
            };
            out.signer_name = authors.get(&sid).map(|a| a.0.clone()).filter(|n| !n.is_empty());
            out.signer = Some(sid);
        }
    }
    out.sha256 = Some(sha);
    out.risks = risks;
    Some(out)
}

/// Doctor findings for an enabled Java mod: what stops it loading, and the scan's warnings (unless
/// the player dismissed them for this jar, see [`exceptions`]).
pub fn findings(j: &JavaMod, except: &HashSet<String>) -> Vec<Finding> {
    use super::doctor::{err, warn};
    let id = j.id.as_str();
    let mut out = vec![];
    if j.sha256.is_none() {
        out.push(err(id, "javaNoJar", "mod.info names the Java file {file}, which isn't there: its Java part won't load", &[("file", j.jar.clone())]));
        return out;
    }
    if j.signature == "invalid" {
        out.push(err(id, "javaBadSignature", "its Java file was changed after its author signed it: ZombieBuddy won't load it", &[]));
    }
    if j.approved == Some(false) {
        out.push(warn(id, "javaBlocked", "ZombieBuddy blocks this version of its Java file (you said no before), so it may not work", &[("sha", j.sha256.clone().unwrap_or_default())]));
    }
    if j.signature == "valid" {
        return out; // a known author signed this exact file: their code, not a stranger's
    }
    let sha = j.sha256.as_deref().unwrap_or_default();
    out.extend(dismissable(j.risks.iter().filter(|r| r.level == Level::Warning).cloned().collect(), sha, except));
    out
}

/// Look for calls and strings in the jar's class files (and jars inside it) that a game mod
/// rarely needs. Warnings are worth a look before allowing the jar; notes are common in legit mods.
fn scan_jar(id: &str, bytes: &[u8]) -> Vec<Finding> {
    let mut seen: BTreeMap<&'static str, Finding> = BTreeMap::new(); // one finding per kind, the first hit
    scan_zip(id, Cursor::new(bytes), 0, &mut seen);
    let mut out: Vec<Finding> = seen.into_values().collect();
    out.sort_by_key(|f| f.level != Level::Warning);
    out
}

fn scan_zip<R: Read + Seek>(id: &str, r: R, depth: usize, seen: &mut BTreeMap<&'static str, Finding>) {
    let Ok(mut z) = zip::ZipArchive::new(r) else {
        hit(seen, id, Kind::BadClass, "(not a valid jar)".into());
        return;
    };
    for i in 0..z.len() {
        let Ok(mut f) = z.by_index(i) else { continue };
        let Ok(name) = f.name().map(|n| n.to_string()) else { continue };
        let lower = name.to_ascii_lowercase();
        let mut data = vec![];
        if lower.ends_with(".class") || lower.ends_with(".jar") {
            // ponytail: a whole entry in memory; mod jars are small (a zip bomb only costs this check)
            if f.read_to_end(&mut data).is_err() {
                continue;
            }
        }
        if lower.ends_with(".class") {
            scan_class(id, &name, &data, seen);
        } else if lower.ends_with(".jar") {
            hit(seen, id, Kind::NestedJar, name.clone());
            if depth < 2 {
                scan_zip(id, Cursor::new(data), depth + 1, seen);
            }
        } else if [".dll", ".so", ".dylib", ".exe", ".bat", ".cmd", ".ps1", ".sh", ".vbs"].iter().any(|e| lower.ends_with(e)) {
            hit(seen, id, Kind::NativeFile, name);
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Process,
    Shell,
    Endpoint,
    Private,
    LoadsCode,
    LaunchFlags,
    NativeFile,
    BadClass,
    Network,
    Files,
    Native,
    Crypto,
    NestedJar,
}

/// What to look for. Classes and `Class.method` references match exactly or by prefix (also when
/// named in a string, as reflection does); strings match case-insensitively anywhere.
const CLASS_RULES: &[(&str, Kind)] = &[
    ("java/lang/ProcessBuilder", Kind::Process),
    ("java/lang/Runtime.exec", Kind::Process),
    ("java/lang/ClassLoader.defineClass", Kind::LoadsCode),
    ("java/security/SecureClassLoader.defineClass", Kind::LoadsCode),
    ("java/lang/invoke/MethodHandles$Lookup.defineClass", Kind::LoadsCode),
    ("java/lang/invoke/MethodHandles$Lookup.defineHiddenClass", Kind::LoadsCode),
    ("java/net/URLClassLoader", Kind::LoadsCode),
    ("javax/script/ScriptEngineManager", Kind::LoadsCode),
    ("java/net/Socket", Kind::Network),
    ("java/net/ServerSocket", Kind::Network),
    ("java/net/DatagramSocket", Kind::Network),
    ("java/net/URL.openConnection", Kind::Network),
    ("java/net/URL.openStream", Kind::Network),
    ("java/net/URLConnection", Kind::Network),
    ("java/net/HttpURLConnection", Kind::Network),
    ("java/net/http/HttpClient", Kind::Network),
    ("java/nio/channels/SocketChannel", Kind::Network),
    ("java/nio/channels/ServerSocketChannel", Kind::Network),
    ("com/sun/net/httpserver/HttpServer", Kind::Network),
    ("java/io/FileOutputStream", Kind::Files),
    ("java/io/FileWriter", Kind::Files),
    ("java/io/RandomAccessFile", Kind::Files),
    ("java/io/File.delete", Kind::Files),
    ("java/io/File.renameTo", Kind::Files),
    ("java/nio/file/Files.write", Kind::Files),
    ("java/nio/file/Files.newOutputStream", Kind::Files),
    ("java/nio/file/Files.newBufferedWriter", Kind::Files),
    ("java/nio/file/Files.delete", Kind::Files),
    ("java/nio/file/Files.copy", Kind::Files),
    ("java/nio/file/Files.move", Kind::Files),
    ("java/lang/System.load", Kind::Native),
    ("java/lang/Runtime.load", Kind::Native),
    ("javax/crypto/Cipher", Kind::Crypto),
];

const STRING_RULES: &[(&str, Kind)] = &[
    ("discord.com/api/webhooks", Kind::Endpoint),
    ("discordapp.com/api/webhooks", Kind::Endpoint),
    ("api.telegram.org", Kind::Endpoint),
    ("pastebin.com/raw", Kind::Endpoint),
    ("grabify", Kind::Endpoint),
    ("iplogger", Kind::Endpoint),
    ("ngrok", Kind::Endpoint),
    ("webhook.site", Kind::Endpoint),
    ("cmd.exe", Kind::Shell),
    ("powershell", Kind::Shell),
    ("wscript", Kind::Shell),
    ("cscript", Kind::Shell),
    ("mshta", Kind::Shell),
    ("rundll32", Kind::Shell),
    ("regsvr32", Kind::Shell),
    ("certutil", Kind::Shell),
    ("bitsadmin", Kind::Shell),
    ("curl.exe", Kind::Shell),
    ("/bin/sh", Kind::Shell),
    ("/bin/bash", Kind::Shell),
    ("/dev/tcp/", Kind::Shell),
    ("nc -e", Kind::Shell),
    ("\\login data", Kind::Private),
    ("/login data", Kind::Private),
    ("user data\\default", Kind::Private),
    ("local storage\\leveldb", Kind::Private),
    ("local storage/leveldb", Kind::Private),
    ("cookies.sqlite", Kind::Private),
    ("logins.json", Kind::Private),
    ("key4.db", Kind::Private),
    ("wallet.dat", Kind::Private),
    ("google\\chrome", Kind::Private),
    ("google/chrome", Kind::Private),
    ("mozilla\\firefox", Kind::Private),
    (".mozilla/firefox", Kind::Private),
    ("/.ssh", Kind::Private),
    ("\\.ssh", Kind::Private),
    ("\\steam\\config", Kind::Private),
    ("loginusers.vdf", Kind::Private),
    ("start menu\\programs\\startup", Kind::Private),
    ("currentversion\\run", Kind::Private),
    ("-javaagent:", Kind::LaunchFlags),
    ("-agentlib:", Kind::LaunchFlags),
    ("-agentpath:", Kind::LaunchFlags),
    ("-xbootclasspath", Kind::LaunchFlags),
    ("java_tool_options", Kind::LaunchFlags),
];

/// Record the first hit of a kind, worded for the player.
fn hit(seen: &mut BTreeMap<&'static str, Finding>, id: &str, kind: Kind, what: String) {
    use super::doctor::{note, warn};
    let v = [("what", what)];
    let f = match kind {
        Kind::Process => warn(id, "javaProcess", "its Java code can start other programs ({what})", &v),
        Kind::Shell => warn(id, "javaShell", "its Java code names a command shell or download tool ({what})", &v),
        Kind::Endpoint => warn(id, "javaEndpoint", "its Java code names {what}, a common drop for stolen data", &v),
        Kind::Private => warn(id, "javaPrivate", "its Java code names {what}, where browsers, Steam, Discord or Windows keep private data", &v),
        Kind::LoadsCode => warn(id, "javaLoadsCode", "its Java code can load more code while the game runs ({what})", &v),
        Kind::LaunchFlags => warn(id, "javaLaunchFlags", "its Java code names Java launch options ({what})", &v),
        Kind::NativeFile => warn(id, "javaNativeFile", "its Java file carries a program or native library ({what})", &v),
        Kind::BadClass => warn(id, "javaBadClass", "its Java file has code PZMM can't read ({what}): it may be hidden on purpose", &v),
        Kind::Network => note(id, "javaNetwork", "its Java code uses the network ({what})", &v),
        Kind::Files => note(id, "javaFiles", "its Java code writes, moves or deletes files ({what})", &v),
        Kind::Native => note(id, "javaNative", "its Java code loads native libraries ({what})", &v),
        Kind::Crypto => note(id, "javaCrypto", "its Java code encrypts or decrypts data ({what})", &v),
        Kind::NestedJar => note(id, "javaNestedJar", "its Java file contains another one ({what}), scanned too", &v),
    };
    seen.entry(f.key).or_insert(f);
}

/// The constant pool of a class file: the classes and members it references and its strings.
fn scan_class(id: &str, name: &str, b: &[u8], seen: &mut BTreeMap<&'static str, Finding>) {
    let Some((utf8, refs, strings)) = constant_pool(b) else {
        hit(seen, id, Kind::BadClass, name.into());
        return;
    };
    let text = |i: u16| utf8.get(&i).map(String::as_str).unwrap_or_default();
    let mut names: Vec<String> = vec![];
    for r in &refs {
        match *r {
            Ref::Class(n) => names.push(text(n).to_string()),
            Ref::Member(class, member) => names.push(format!("{}.{}", text(class), text(member))),
        }
    }
    // a class named in a string (reflection, Class.forName) counts as a reference too
    names.extend(strings.iter().map(|&s| text(s).replace('.', "/")));
    for n in &names {
        if let Some((rule, kind)) = CLASS_RULES.iter().find(|(r, _)| n == r || n.starts_with(r)) {
            let shown = rule.replace('/', ".");
            hit(seen, id, *kind, shown);
        }
    }
    for &s in &strings {
        let lower = text(s).to_ascii_lowercase();
        if let Some((rule, kind)) = STRING_RULES.iter().find(|(r, _)| lower.contains(r)) {
            hit(seen, id, *kind, rule.to_string());
        }
    }
}

enum Ref {
    /// CONSTANT_Class: its name's index.
    Class(u16),
    /// Field/Method refs: (the owner class's name index, the member name index).
    Member(u16, u16),
}

/// (Utf8 entries by index, class and member references, String constants' Utf8 indexes); None
/// when it isn't a readable class file.
#[allow(clippy::type_complexity)]
fn constant_pool(b: &[u8]) -> Option<(HashMap<u16, String>, Vec<Ref>, Vec<u16>)> {
    if b.get(..4)? != [0xCA, 0xFE, 0xBA, 0xBE] {
        return None;
    }
    let u16_at = |i: usize| Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]));
    let count = u16_at(8)?;
    let (mut utf8, mut classes, mut nat, mut members, mut strings) = (HashMap::new(), HashMap::new(), HashMap::new(), vec![], vec![]);
    let (mut i, mut idx) = (10usize, 1u16);
    while idx < count {
        let tag = *b.get(i)?;
        let len = match tag {
            1 => {
                let n = u16_at(i + 1)? as usize;
                utf8.insert(idx, String::from_utf8_lossy(b.get(i + 3..i + 3 + n)?).into_owned());
                3 + n
            }
            7 => {
                classes.insert(idx, u16_at(i + 1)?);
                3
            }
            8 => {
                strings.push(u16_at(i + 1)?);
                3
            }
            9..=11 => {
                members.push((u16_at(i + 1)?, u16_at(i + 3)?));
                5
            }
            12 => {
                nat.insert(idx, u16_at(i + 1)?);
                5
            }
            3 | 4 | 17 | 18 => 5,
            5 | 6 => 9,
            15 => 4,
            16 | 19 | 20 => 3,
            _ => return None,
        };
        i += len;
        idx += if tag == 5 || tag == 6 { 2 } else { 1 }; // longs and doubles take two slots
    }
    let mut refs: Vec<Ref> = classes.values().map(|&n| Ref::Class(n)).collect();
    refs.extend(members.into_iter().filter_map(|(c, t)| Some(Ref::Member(*classes.get(&c)?, *nat.get(&t)?))));
    Some((utf8, refs, strings))
}

// --- beyond declared jars: every Java and native file, the game folder, its launcher, mod Lua ---

/// Files a game mod rarely ships outside a declared jar: Java code, native libraries, programs and scripts.
/// Loose scripts (build.ps1, run.sh) are left out: nothing runs them unless a program already does.
const CODE_FILES: &[&str] = &["jar", "class", "dll", "so", "dylib", "exe"];

/// SHA-256 of `bytes`, lowercase hex.
pub fn sha256(bytes: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}

fn exceptions_file(paths: &super::Paths) -> PathBuf {
    paths.data.join("scan-exceptions.json")
}

/// The files the player dismissed scan findings for, by SHA-256: a changed file is scanned again
/// (a damaged list is kept aside, see `read_json`; an unreadable one reads as none).
pub fn exceptions(paths: &super::Paths) -> HashSet<String> {
    crate::read_json(&exceptions_file(paths)).unwrap_or_default()
}

/// Stop reporting scan findings about the file with this SHA-256.
pub fn dismiss(paths: &super::Paths, sha: &str) -> anyhow::Result<()> {
    anyhow::ensure!(sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()), "not a SHA-256: {sha}");
    // unreadable: an error rather than saving over the list
    let mut all: Vec<String> = crate::read_json::<HashSet<String>>(&exceptions_file(paths))?.into_iter().collect();
    all.push(sha.to_ascii_lowercase());
    all.sort();
    all.dedup();
    std::fs::create_dir_all(&paths.data)?;
    Ok(crate::write_atomic(&exceptions_file(paths), serde_json::to_vec(&all)?)?)
}

/// Scan findings about one file (by its SHA-256): none when the player dismissed it, else each
/// offers to.
pub(crate) fn dismissable(found: Vec<Finding>, sha: &str, except: &HashSet<String>) -> Vec<Finding> {
    if except.contains(sha) {
        return vec![];
    }
    found.into_iter().map(|f| Finding { action: Some(super::doctor::Action::Dismiss(sha.to_string())), ..f }).collect()
}

/// Java and native files anywhere in an enabled mod's folder besides the jars its mod.info files
/// declare (`javaJarFile=`, any build's): the game never loads them itself, but a Java mod or an
/// agent can. Jars are scanned like declared ones.
pub fn scan_folder(m: &Row, except: &HashSet<String>) -> Vec<Finding> {
    fn walk(d: &Path, depth: usize, out: &mut Vec<PathBuf>, declared: &mut Vec<String>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                if depth < 8 {
                    walk(&p, depth + 1, out, declared);
                }
            } else if p.extension().is_some_and(|x| CODE_FILES.iter().any(|c| x.eq_ignore_ascii_case(c))) {
                out.push(p);
            } else if e.file_name().eq_ignore_ascii_case("mod.info") {
                declared.extend(super::read_info(&p).remove("javaJarFile").map(|j| j.trim().trim_start_matches("./").replace('\\', "/").to_ascii_lowercase()).filter(|j| !j.is_empty()));
            }
        }
    }
    let (mut files, mut declared) = (vec![], vec![]);
    walk(&m.dir, 0, &mut files, &mut declared);
    let mut out = vec![];
    for f in &files {
        let what = f.strip_prefix(&m.dir).unwrap_or(f).to_string_lossy().replace('\\', "/");
        let lower = format!("/{}", what.to_ascii_lowercase());
        if declared.iter().any(|d| lower.ends_with(&format!("/{d}"))) {
            continue; // inspect() checks those
        }
        let Ok(bytes) = std::fs::read(f) else { continue };
        let mut found = vec![super::doctor::warn(&m.id, "modCodeFile", "its folder carries {what}, Java or native code its mod.info doesn't declare: the game doesn't load it, but other code can", &[("what", what.clone())])];
        if lower.ends_with(".jar") {
            found.extend(scan_jar(&m.id, &bytes).into_iter().filter(|r| r.level == Level::Warning));
        }
        out.extend(dismissable(found, &sha256(&bytes), except));
    }
    out
}

/// Calls in game Lua that a mod has no use for, and what they mean.
const LUA_CALLS: &[(&str, &str)] = &[("os.execute", "luaProcess"), ("io.popen", "luaProcess"), ("getUrlInputStream", "luaDownload")];

/// What one of a mod's Lua files (`file`, from `media/lua`) names that a game mod has no use for:
/// data drops, shells and download tools, private files, running programs, downloads, files
/// reached outside the game's folders. One finding per kind.
pub fn scan_lua(id: &str, file: &str, text: &str) -> Vec<Finding> {
    use super::doctor::warn;
    let lower = text.to_ascii_lowercase();
    let mut seen: BTreeMap<&'static str, Finding> = BTreeMap::new();
    let mut add = |f: Finding| {
        seen.entry(f.key).or_insert(f);
    };
    let vars = |what: &str| [("file", file.to_string()), ("what", what.to_string())];
    for (rule, kind) in STRING_RULES {
        // a whole word: Lua's own names run into them (`viewScript` holds "wscript")
        if !lower.match_indices(rule).any(|(i, _)| !lower[..i].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_')) {
            continue;
        }
        match kind {
            Kind::Endpoint => add(warn(id, "luaEndpoint", "its Lua ({file}) names {what}, a common drop for stolen data", &vars(rule))),
            Kind::Shell => add(warn(id, "luaShell", "its Lua ({file}) names a command shell or download tool ({what})", &vars(rule))),
            Kind::Private => add(warn(id, "luaPrivate", "its Lua ({file}) names {what}, where browsers, Steam, Discord or Windows keep private data", &vars(rule))),
            _ => {}
        }
    }
    for (call, key) in LUA_CALLS {
        if text.contains(call) {
            match *key {
                "luaProcess" => add(warn(id, "luaProcess", "its Lua ({file}) tries to run programs ({what})", &vars(call))),
                _ => add(warn(id, "luaDownload", "its Lua ({file}) downloads from the internet ({what})", &vars(call))),
            }
        }
    }
    // the game's file functions reach Zomboid's own folders; `../` climbs out of them
    let escapes = text.lines().find(|l| ["FileWriter(", "FileReader("].iter().any(|c| l.contains(c)) && ["\"../", "'../", "\"..\\\\", "'..\\\\", "/../", "\\\\..\\\\"].iter().any(|p| l.contains(p)));
    if let Some(l) = escapes {
        add(warn(id, "luaFileEscape", "its Lua ({file}) reaches for files outside the game's folders ({what})", &vars(l.trim())));
    }
    seen.into_values().collect()
}

/// The game folder and how it starts: Java agents and boot class path overrides in
/// ProjectZomboid64.json, Java options set for every program (`env`: the set ones of
/// JAVA_TOOL_OPTIONS, _JAVA_OPTIONS, JDK_JAVA_OPTIONS) and, on Build 42 (one `projectzomboid.jar`),
/// classpath entries, jars and loose class files that aren't the game's. `zb_libs`: the official
/// ZombieBuddy mod's agent files, which the game folder's copy should match.
pub fn scan_game(game: &Path, zb_libs: Option<&Path>, env: &[(&str, String)], except: &HashSet<String>) -> Vec<Finding> {
    use super::doctor::warn;
    let base = super::zombiebuddy::launcher_dir(game).unwrap_or_else(|| game.to_path_buf());
    let folder = base.file_name().map_or("game".into(), |n| n.to_string_lossy().into_owned());
    let mut out = vec![];
    for (name, value) in env {
        let f = warn(name, "javaEnvOptions", "is set ({what}): every Java program on this PC starts with it, the game too", &[("what", value.clone())]);
        out.extend(dismissable(vec![f], &sha256(format!("{name}={value}").as_bytes()), except));
    }
    let b42 = base.join("projectzomboid.jar").is_file();
    if let Ok(text) = std::fs::read(base.join("ProjectZomboid64.json")) {
        let v: serde_json::Value = serde_json::from_slice(&text).unwrap_or_default();
        let mut args = vec![];
        vm_args(&v, &mut args);
        let mut found = launch_flags("ProjectZomboid64.json", args.iter().map(String::as_str));
        if b42 {
            for c in v["classpath"].as_array().into_iter().flatten().filter_map(|c| c.as_str()).filter(|c| !matches!(*c, "." | "projectzomboid.jar")) {
                found.push(warn("ProjectZomboid64.json", "launcherClasspath", "puts {what} on the game's classpath, which isn't part of the game", &[("what", c.to_string())]));
            }
        }
        out.extend(dismissable(found, &sha256(&text), except));
    }
    // the scripts that start it by hand (ProjectZomboid64.bat, projectzomboid.sh)
    let mut dirs = vec![game.to_path_buf()];
    dirs.extend(Some(base.clone()).filter(|b| b != game));
    for e in dirs.iter().flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten()) {
        let name = e.file_name().to_string_lossy().into_owned();
        if [".bat", ".cmd", ".sh"].iter().any(|x| name.to_ascii_lowercase().ends_with(x))
            && let Ok(bytes) = std::fs::read(e.path())
        {
            out.extend(dismissable(launch_flags(&name, String::from_utf8_lossy(&bytes).split_whitespace()), &sha256(&bytes), except));
        }
    }
    if !b42 {
        return out; // Build 41 is loose class files and library jars itself
    }
    for e in std::fs::read_dir(&base).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.to_ascii_lowercase().ends_with(".jar") && !matches!(name.as_str(), "projectzomboid.jar" | "ZombieBuddy.jar") {
            let Ok(bytes) = std::fs::read(e.path()) else { continue };
            let f = warn(&folder, "gameJar", "has {what}, a Java file that isn't the game's", &[("what", name)]);
            out.extend(dismissable(vec![f], &sha256(&bytes), except));
        }
    }
    // Avrix plugins: the loader runs every jar there
    for n in file_names(&base.join("plugins")).into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".jar")) {
        let Ok(bytes) = std::fs::read(base.join("plugins").join(&n)) else { continue };
        let found = scan_jar(&format!("plugins/{n}"), &bytes).into_iter().filter(|r| r.level == Level::Warning).collect();
        out.extend(dismissable(found, &sha256(&bytes), except));
    }
    // the classpath starts with `.`: a class file here overrides the game's own
    let mut classes = vec![];
    loose_classes(&base, 0, &mut classes);
    for c in classes.iter().take(5) {
        let Ok(bytes) = std::fs::read(c) else { continue };
        let what = c.strip_prefix(&base).unwrap_or(c).to_string_lossy().replace('\\', "/");
        let f = warn(&folder, "gameClass", "has a loose Java class, {what}: the game runs it instead of its own code", &[("what", what)]);
        out.extend(dismissable(vec![f], &sha256(&bytes), except));
    }
    if let Some(libs) = zb_libs
        && base.join("ZombieBuddy.jar").is_file()
        && !super::zombiebuddy::files_current(&base, libs)
    {
        out.push(warn(&folder, "zbChanged", "its ZombieBuddy files aren't the ZombieBuddy mod's (outdated, or replaced): set it up again in the Java mods tab", &[]));
    }
    out
}

/// Java agents other than ZombieBuddy's, and boot class path overrides, among a launcher's options.
fn launch_flags<'a>(subject: &str, args: impl IntoIterator<Item = &'a str>) -> Vec<Finding> {
    use super::doctor::warn;
    let mut out = vec![];
    for a in args.into_iter().map(|a| a.trim_matches(['"', '\'', ','])).filter(|a| !super::zombiebuddy::is_zb_arg(a)) {
        let what = [("what", a.to_string())];
        if ["-javaagent:", "-agentlib:", "-agentpath:"].iter().any(|p| a.starts_with(p)) {
            out.push(warn(subject, "launcherAgent", "starts the game with the Java agent {what}: only ZombieBuddy's is expected", &what));
        } else if a.starts_with("-Xbootclasspath") {
            out.push(warn(subject, "launcherBootPath", "starts the game with {what}, which puts code ahead of Java's own", &what));
        }
    }
    out
}

/// Every `vmArgs` string in ProjectZomboid64.json, the per-OS sections' too.
fn vm_args(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                match x.as_array().filter(|_| k == "vmArgs") {
                    Some(a) => out.extend(a.iter().filter_map(|s| s.as_str().map(String::from))),
                    None => vm_args(x, out),
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| vm_args(x, out)),
        _ => {}
    }
}

/// `.class` files in the game folder outside its Java runtime and data folders.
fn loose_classes(d: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    const SKIP: &[&str] = &["jre", "jre64", "media", "mods", "workshop", "launcher", "license", "steamapps"];
    for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            if depth < 8 && !(depth == 0 && SKIP.contains(&name.as_str())) {
                loose_classes(&p, depth + 1, out);
            }
        } else if name.ends_with(".class") {
            out.push(p);
        }
    }
}

// --- other Java mod loaders: Leaf (github.com/LeafPZ, a Fabric Loader fork) and Avrix (github.com/Brov3r/Avrix) ---

/// A Java mod loader other than ZombieBuddy, installed in the game folder.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Loader {
    /// "leaf" or "avrix".
    pub name: &'static str,
    /// From its jar's name ("" when it doesn't say).
    pub version: String,
    /// What starts it with the game: "steam" (Project Zomboid's Steam launch options) or the
    /// launcher config or script in the game folder that names it; None when nothing does.
    pub started_by: Option<String>,
}

/// `<prefix><version>.jar` -> version.
fn jar_version(name: &str, prefix: &str) -> Option<String> {
    name.strip_prefix(prefix)?.strip_suffix(".jar").filter(|v| !v.ends_with("-sources")).map(String::from)
}

fn file_names(d: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

/// The Java loaders besides ZombieBuddy installed for the game in `game`. Leaf's installer puts
/// its jars in `.leaf/lib` (`loader-<v>.jar`, or `loader-proxy-<v>.jar`, which fetches the
/// loader) and starts it with `-javaagent:` in the Steam launch options or a launcher config of
/// its own; Avrix is `Avrix-Loader-<v>.jar` in the game folder, started with `java -jar` from a
/// script. `steam_options`: the game's Steam launch options (see [`steam_launch_options`]), asked
/// only when Leaf is there.
pub fn other_loaders(game: &Path, steam_options: impl FnOnce() -> String) -> Vec<Loader> {
    let base = super::zombiebuddy::launcher_dir(game).unwrap_or_else(|| game.to_path_buf());
    let mut dirs = vec![game.to_path_buf()];
    dirs.extend(Some(base.clone()).filter(|b| b != game));
    // the first launcher config or script in the game folder that names `needle`
    let named_in = |needle: &str| {
        dirs.iter().flat_map(|d| file_names(d).into_iter().map(move |n| (d.join(&n), n))).find_map(|(p, n)| {
            let lower = n.to_ascii_lowercase();
            let script = [".json", ".bat", ".cmd", ".sh"].iter().any(|x| lower.ends_with(x));
            (script && std::fs::read(&p).is_ok_and(|b| String::from_utf8_lossy(&b).contains(needle))).then_some(n)
        })
    };
    let mut out = vec![];
    let lib = file_names(&base.join(".leaf/lib"));
    if !lib.is_empty() {
        let version = lib.iter().find_map(|n| jar_version(n, "loader-").filter(|v| !v.starts_with("proxy-"))).or_else(|| lib.iter().find_map(|n| jar_version(n, "loader-proxy-")));
        let started_by = if steam_options().contains(".leaf") { Some("steam".into()) } else { named_in(".leaf") };
        out.push(Loader { name: "leaf", version: version.unwrap_or_default(), started_by });
    }
    if let Some(version) = dirs.iter().flat_map(|d| file_names(d)).find_map(|n| jar_version(&n, "Avrix-Loader-")) {
        out.push(Loader { name: "avrix", version, started_by: named_in("Avrix-Loader") });
    }
    out
}

/// A jar another loader loads (see [`loader_mods`]): its hash and scan, no ZombieBuddy signature.
fn loader_mod(loader: &'static str, id: &str, name: &str, jar: String, file: &Path) -> JavaMod {
    let got = hash_and_scan(id, file);
    JavaMod {
        id: id.to_string(),
        name: name.to_string(),
        jar,
        signature: if got.is_some() { "unsigned" } else { "missing" },
        sha256: got.as_ref().map(|g| g.0.clone()),
        signer: None,
        signer_name: None,
        approved: None,
        risks: got.map(|g| g.1).unwrap_or_default(),
        loader,
    }
}

/// A Leaf mod's jars: `media/java/*.jar` in the mod folder or a version folder (as Leaf looks), in a
/// mod without ZombieBuddy's `javaJarFile=` (those are inspect()'s).
fn leaf_jars(m: &Row) -> Vec<PathBuf> {
    if jar_of(m).is_some() {
        return vec![];
    }
    let mut roots = vec![m.dir.clone()];
    roots.extend(std::fs::read_dir(&m.dir).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()));
    roots.iter().map(|r| r.join("media/java")).flat_map(|java| file_names(&java).into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".jar")).map(move |n| java.join(n))).collect()
}

/// The jars Leaf and Avrix load: Leaf mods' among the enabled mods (see [`leaf_jars`]), and Avrix
/// plugins (`plugins/*.jar` in the game folder).
pub fn loader_mods<'a>(game: Option<&Path>, enabled: impl IntoIterator<Item = &'a Row>) -> Vec<JavaMod> {
    let mut out = vec![];
    for m in enabled {
        for p in leaf_jars(m) {
            let rel = p.strip_prefix(&m.dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            out.push(loader_mod("leaf", &m.id, &m.name, rel, &p));
        }
    }
    if let Some(game) = game {
        let plugins = super::zombiebuddy::launcher_dir(game).unwrap_or_else(|| game.to_path_buf()).join("plugins");
        for n in file_names(&plugins).into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".jar")) {
            out.push(loader_mod("avrix", &n, &n, format!("plugins/{n}"), &plugins.join(&n)));
        }
    }
    out
}

/// Check setup: enabled mods with Leaf jars while Leaf (`leaf`, from [`other_loaders`], asked only
/// when there are such mods) is missing, or installed with nothing starting it: their Java won't load.
pub fn leaf_findings<'a>(enabled: impl IntoIterator<Item = &'a Row>, leaf: impl FnOnce() -> Option<Loader>) -> Vec<Finding> {
    use super::doctor::warn;
    let with_jars: Vec<&Row> = enabled.into_iter().filter(|m| !leaf_jars(m).is_empty()).collect();
    if with_jars.is_empty() {
        return vec![];
    }
    let leaf = leaf();
    with_jars
        .into_iter()
        .filter_map(|m| match &leaf {
            None => Some(warn(&m.id, "javaNoLeaf", "its Java code loads through Leaf, which isn't installed for the game, so that part of it won't run", &[])),
            Some(l) if l.started_by.is_none() => Some(warn(&m.id, "javaLeafOff", "its Java code loads through Leaf, which is installed, but nothing starts Leaf with the game (Steam launch options or a launcher config)", &[])),
            Some(_) => None,
        })
        .collect()
}

/// Project Zomboid's launch options in a Steam `localconfig.vdf` (its `"108600"` app block).
pub fn launch_options_in(vdf: &str) -> Option<String> {
    for (i, _) in vdf.match_indices("\"108600\"") {
        let rest = vdf[i + 8..].trim_start();
        if !rest.starts_with('{') {
            continue;
        }
        // the app's block: up to its closing brace (keys and values are quoted, so braces in them don't count)
        let (mut depth, mut quoted, mut end) = (0, false, rest.len());
        let mut chars = rest.char_indices();
        while let Some((j, c)) = chars.next() {
            match c {
                '\\' if quoted => {
                    chars.next();
                }
                '"' => quoted = !quoted,
                '{' if !quoted => depth += 1,
                '}' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        end = j;
                        break;
                    }
                }
                _ => {}
            }
        }
        let block = &rest[..end];
        let Some(value) = block.split_once("\"LaunchOptions\"").and_then(|(_, v)| v.trim_start().strip_prefix('"')) else { continue };
        let mut out = String::new();
        let mut chars = value.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.extend(chars.next()),
                '"' => return Some(out),
                c => out.push(c),
            }
        }
    }
    None
}

/// Project Zomboid's Steam launch options (read-only): the signed-in account's, or with Steam
/// closed, every account's on this PC. Only the game's `LaunchOptions` value is kept.
pub fn steam_launch_options() -> String {
    if cfg!(test) {
        return String::new();
    }
    // userdata folders are named by account ID: the SteamID64 less the individual-account base
    let me = crate::sources::steam_workshop::active_user().and_then(|id| id.parse::<u64>().ok()?.checked_sub(76_561_197_960_265_728)).map(|a| a.to_string());
    let mut out = vec![];
    for users in crate::sources::steam_workshop::steam_roots().iter().map(|r| r.join("userdata")) {
        for e in std::fs::read_dir(users).into_iter().flatten().flatten().filter(|e| me.as_ref().is_none_or(|me| e.file_name().to_string_lossy() == me.as_str())) {
            if let Some(o) = std::fs::read(e.path().join("config/localconfig.vdf")).ok().and_then(|b| launch_options_in(&String::from_utf8_lossy(&b))) {
                out.push(o);
            }
        }
    }
    out.join(" ")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_common_mod_info_finds_the_jar_in_the_version_folder() {
        // Viewpoint's layout: common/mod.info names media/java/..., the jar sits in 42/
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("Viewpoint");
        std::fs::create_dir_all(dir.join("common")).unwrap();
        std::fs::create_dir_all(dir.join("42/media/java/client")).unwrap();
        std::fs::write(dir.join("common/mod.info"), "id=Viewpoint
javaJarFile=media/java/client/Viewpoint.jar
").unwrap();
        std::fs::write(dir.join("42/media/java/client/Viewpoint.jar"), b"PK").unwrap();
        let row = Row { id: "Viewpoint".into(), dir: dir.clone(), info: dir.join("common/mod.info"), ..Default::default() };
        assert_eq!(jar_of(&row).unwrap().1, dir.join("42").join("media/java/client/Viewpoint.jar"));
    }

    /// A minimal class file whose constant pool references `class.method` and holds `string`.
    fn class_file(class: &str, method: &str, string: &str) -> Vec<u8> {
        let mut b = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 61];
        let utf = |s: &str| [vec![1], (s.len() as u16).to_be_bytes().to_vec(), s.as_bytes().to_vec()].concat();
        let pool = [
            utf(class),                 // 1
            vec![7, 0, 1],              // 2 Class -> 1
            utf(method),                // 3
            utf("()V"),                 // 4
            vec![12, 0, 3, 0, 4],       // 5 NameAndType
            vec![10, 0, 2, 0, 5],       // 6 Methodref
            utf(string),                // 7
            vec![8, 0, 7],              // 8 String -> 7
            vec![5, 0, 0, 0, 0, 0, 0, 0, 1], // 9-10 Long
        ];
        b.extend(11u16.to_be_bytes());
        pool.iter().for_each(|e| b.extend(e));
        b
    }

    pub fn jar(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(vec![]));
        for (name, data) in entries {
            z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    fn keys(found: &[Finding]) -> Vec<(&str, &str)> {
        found.iter().map(|f| (f.key, f.vars["what"].as_str())).collect()
    }

    #[test]
    fn finds_leaf_and_avrix_and_their_jars() {
        let t = tempfile::tempdir().unwrap();
        let game = t.path().join("game");
        let put = |p: PathBuf, b: &[u8]| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b).unwrap();
        };
        assert!(other_loaders(&game, String::new).is_empty());
        put(game.join(".leaf/lib/loader-proxy-0.2.1.jar"), b"x");
        put(game.join("Avrix-Loader-2.2.0.jar"), b"x");
        put(game.join("Avrix-Loader-2.2.0-sources.jar"), b"x");
        let found = other_loaders(&game, String::new);
        assert_eq!(found, [Loader { name: "leaf", version: "0.2.1".into(), started_by: None }, Loader { name: "avrix", version: "2.2.0".into(), started_by: None }]);
        // started by the Steam launch options, a launcher config or a script
        put(game.join(".leaf/lib/loader-1.7.2.jar"), b"x");
        put(game.join("ProjectZomboid64Leaf.json"), br#"{"classpath": [".leaf/lib/loader-1.7.2.jar"]}"#);
        put(game.join("avrix.bat"), b"jre64\\bin\\java -jar Avrix-Loader-2.2.0.jar");
        let found = other_loaders(&game, String::new);
        assert_eq!((found[0].version.as_str(), found[0].started_by.as_deref()), ("1.7.2", Some("ProjectZomboid64Leaf.json")));
        assert_eq!(found[1].started_by.as_deref(), Some("avrix.bat"));
        assert_eq!(other_loaders(&game, || "-javaagent:.leaf/lib/loader-proxy-0.2.1.jar --".into())[0].started_by.as_deref(), Some("steam"));

        // a Leaf mod's jars (B42 version folder) and an Avrix plugin get the scan
        let dir = t.path().join("LeafMod");
        put(dir.join("42/media/java/leafmod.jar"), &jar(&[("a/A.class", class_file("java/lang/Runtime", "exec", "x"))]));
        put(game.join("plugins/Plugin.jar"), &jar(&[("a/B.class", class_file("zombie/Lua/LuaManager", "call", "x"))]));
        let row = Row { id: "LeafMod".into(), name: "Leaf Mod".into(), dir: dir.clone(), info: dir.join("42/mod.info"), ..Default::default() };
        let mods = loader_mods(Some(&game), [&row]);
        let got: Vec<_> = mods.iter().map(|j| (j.loader, j.id.as_str(), j.jar.as_str(), j.risks.iter().map(|r| r.key).collect::<Vec<_>>())).collect();
        assert_eq!(got, [("leaf", "LeafMod", "42/media/java/leafmod.jar", vec!["javaProcess"]), ("avrix", "Plugin.jar", "plugins/Plugin.jar", vec![])]);

        // Steam's config is read only when Leaf is there
        assert!(other_loaders(&t.path().join("bare"), || unreachable!("no Leaf: Steam's config stays unread")).is_empty());
        // Check setup: a Leaf mod's Java won't run without Leaf, or with nothing starting it
        let keys = |l: Option<Loader>| leaf_findings([&row], || l).iter().map(|f| f.key).collect::<Vec<_>>();
        assert_eq!(keys(None), ["javaNoLeaf"]);
        assert_eq!(keys(Some(Loader { name: "leaf", version: String::new(), started_by: None })), ["javaLeafOff"]);
        assert!(keys(Some(found[0].clone())).is_empty());
        let plain = Row { id: "Plain".into(), dir: t.path().join("Plain"), ..Default::default() };
        assert!(leaf_findings([&plain], || unreachable!("no Leaf mods: nothing to ask")).is_empty());
    }

    #[test]
    fn reads_the_games_steam_launch_options() {
        let vdf = "\"UserLocalConfigStore\"\n{\n\t\"apps\"\n\t{\n\t\t\"108600\"\n\t\t{\n\t\t\t\"LastPlayed\"\t\t\"1\"\n\t\t\t\"cloud\"\n\t\t\t{\n\t\t\t\t\"x\"\t\t\"{\"\n\t\t\t}\n\t\t\t\"LaunchOptions\"\t\t\"-javaagent:.leaf/lib/loader-proxy.jar -- \\\"a b\\\"\"\n\t\t}\n\t\t\"108601\"\n\t\t{\n\t\t\t\"LaunchOptions\"\t\t\"-other\"\n\t\t}\n\t}\n}\n";
        assert_eq!(launch_options_in(vdf).as_deref(), Some("-javaagent:.leaf/lib/loader-proxy.jar -- \"a b\""));
        // another app's options aren't the game's
        assert_eq!(launch_options_in("\"108600\"\n{\n\"LastPlayed\" \"1\"\n}\n\"9\"\n{\n\"LaunchOptions\" \"-x\"\n}"), None);
        assert_eq!(launch_options_in("\"Licenses\" \"108600\""), None);
    }

    #[test]
    fn flags_dangerous_calls_and_strings() {
        let clean = jar(&[("a/Patch.class", class_file("zombie/Lua/LuaManager", "call", "Hello"))]);
        assert!(scan_jar("M", &clean).is_empty());

        let inner = jar(&[("x/Y.class", class_file("java/net/URL", "openConnection", "https://discord.com/api/webhooks/1/abc"))]);
        let bad = jar(&[
            ("a/A.class", class_file("java/lang/Runtime", "exec", "cmd.exe /c del")),
            ("a/B.class", class_file("java/lang/Class", "forName", "java.lang.ProcessBuilder")),
            ("lib/inner.jar", inner),
            ("native/x.dll", vec![0]),
            ("a/C.class", b"not a class".to_vec()),
        ]);
        let found = scan_jar("M", &bad);
        let k = keys(&found);
        assert_eq!(found[0].level, Level::Warning, "warnings first: {k:?}");
        assert!(k.contains(&("javaProcess", "java.lang.Runtime.exec")), "{k:?}");
        assert!(k.contains(&("javaShell", "cmd.exe")), "{k:?}");
        assert!(k.contains(&("javaEndpoint", "discord.com/api/webhooks")), "nested jars are scanned: {k:?}");
        assert!(k.contains(&("javaNetwork", "java.net.URL.openConnection")), "{k:?}");
        assert!(k.contains(&("javaNativeFile", "native/x.dll")), "{k:?}");
        assert!(k.contains(&("javaBadClass", "a/C.class")), "{k:?}");
        assert!(k.contains(&("javaNestedJar", "lib/inner.jar")), "{k:?}");
        assert_eq!(found.iter().filter(|f| f.key == "javaProcess").count(), 1, "one finding per kind");
        assert_eq!(found.iter().find(|f| f.key == "javaNetwork").unwrap().level, Level::Note);
    }

    #[test]
    fn hashes_and_verifies_signatures() {
        use ring::signature::KeyPair;
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("Mod");
        std::fs::create_dir_all(dir.join("42/media/java")).unwrap();
        std::fs::write(dir.join("42/mod.info"), "id=Mod\nname=A mod\njavaJarFile=media/java/Mod.jar\njavaPkgName=a\n").unwrap();
        let bytes = jar(&[("a/A.class", class_file("zombie/X", "y", "z"))]);
        let jar_path = dir.join("42/media/java/Mod.jar");
        std::fs::write(&jar_path, &bytes).unwrap();
        let row = Row {
            id: "Mod".into(),
            name: "A mod".into(),
            source: "steam:1".into(),
            info: dir.join("42/mod.info"),
            dir: dir.clone(),
            requires: vec![],
            incompatible: vec![],
            load_after: vec![],
            load_before: vec![],
            supports_41: false,
            supports_42: true,
            ..Default::default()
        };
        let sha = hex(ring::digest::digest(&ring::digest::SHA256, &bytes).as_ref());
        let approvals = Approvals::from([(sha.clone(), false)]);
        let j = inspect(&row, &Authors::new(), &approvals).unwrap();
        assert_eq!((j.signature, j.approved, j.sha256.as_deref()), ("unsigned", Some(false), Some(sha.as_str())));
        assert!(findings(&j, &HashSet::new()).iter().any(|f| f.key == "javaBlocked"));

        // sign it like ZombieBuddy: Ed25519 over "ZBS:<SteamID64>:<sha256 hex>"
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let sid = "76561190000000001";
        let sig = key.sign(format!("ZBS:{sid}:{sha}").as_bytes());
        std::fs::write(dir.join("42/media/java/Mod.jar.zbs"), format!("ZBS\nSteamID64:{sid}\nSignature:{}\n", hex(sig.as_ref()))).unwrap();
        let authors_file = t.path().join("authors.json");
        std::fs::write(&authors_file, format!(r#"{{"authors":[{{"id":{sid},"name":"Someone","keys":["{}"]}}]}}"#, hex(key.public_key().as_ref()))).unwrap();
        let mut authors = Authors::new();
        read_authors(&authors_file, &mut authors);
        let j = inspect(&row, &authors, &Approvals::new()).unwrap();
        assert_eq!((j.signature, j.signer.as_deref(), j.signer_name.as_deref()), ("valid", Some(sid), Some("Someone")));
        assert_eq!(inspect(&row, &Authors::new(), &Approvals::new()).unwrap().signature, "unknown_key");

        // a changed jar no longer matches its signature
        std::fs::write(&jar_path, jar(&[("a/A.class", class_file("java/lang/ProcessBuilder", "start", "z"))])).unwrap();
        let j = inspect(&row, &authors, &Approvals::new()).unwrap();
        assert_eq!(j.signature, "invalid");
        let keys: Vec<&str> = findings(&j, &HashSet::new()).iter().map(|f| f.key).collect();
        assert_eq!(keys, ["javaBadSignature", "javaProcess"]);

        std::fs::remove_file(&jar_path).unwrap();
        assert_eq!(findings(&inspect(&row, &authors, &Approvals::new()).unwrap(), &HashSet::new())[0].key, "javaNoJar");
    }

    #[test]
    fn scans_every_code_file_and_lua_and_honours_dismissals() {
        let (t, paths) = crate::store::tests::temp_paths();
        let dir = t.path().join("Mod");
        std::fs::create_dir_all(dir.join("42/media/java")).unwrap();
        std::fs::create_dir_all(dir.join("42/media/lua/client")).unwrap();
        std::fs::write(dir.join("42/mod.info"), "id=Mod\njavaJarFile=media/java/Mod.jar\n").unwrap();
        std::fs::write(dir.join("42/media/java/Mod.jar"), jar(&[("a/A.class", class_file("zombie/X", "y", "z"))])).unwrap();
        let hidden = jar(&[("a/B.class", class_file("java/lang/Runtime", "exec", "powershell -enc"))]);
        std::fs::write(dir.join("42/media/extra.jar"), &hidden).unwrap();
        std::fs::write(dir.join("42/x.dll"), b"MZ").unwrap();
        // the Build 41 copy's jar, declared by the root mod.info
        std::fs::create_dir_all(dir.join("media/java")).unwrap();
        std::fs::write(dir.join("mod.info"), "id=Mod\njavaJarFile=media\\java\\Old.jar\n").unwrap();
        std::fs::write(dir.join("media/java/Old.jar"), &hidden).unwrap();
        let row = Row { id: "Mod".into(), dir: dir.clone(), info: dir.join("42/mod.info"), ..Default::default() };
        let found = scan_folder(&row, &HashSet::new());
        let mut got: Vec<(&str, &str)> = found.iter().map(|f| (f.key, f.vars["what"].as_str())).collect();
        got.sort(); // folder listing order differs by OS
        assert_eq!(got, [("javaProcess", "java.lang.Runtime.exec"), ("javaShell", "powershell"), ("modCodeFile", "42/media/extra.jar"), ("modCodeFile", "42/x.dll")], "the declared jar is left to inspect()");
        let found: Vec<_> = found.into_iter().filter(|f| f.vars["what"] == "42/media/extra.jar").collect();
        assert_eq!(found[0].action, Some(super::super::doctor::Action::Dismiss(sha256(&hidden))));

        // dismissed by the file's SHA-256: gone until the file changes
        dismiss(&paths, &sha256(&hidden)).unwrap();
        assert!(dismiss(&paths, "nope").is_err());
        let except = exceptions(&paths);
        assert_eq!(scan_folder(&row, &except).len(), 1);
        // a damaged list is kept aside, and dismissing starts a new one
        std::fs::write(exceptions_file(&paths), "[\"damaged").unwrap();
        assert!(exceptions(&paths).is_empty() && exceptions_file(&paths).with_extension("json.bad").exists());
        dismiss(&paths, &sha256(&hidden)).unwrap();
        assert_eq!(exceptions(&paths), except);

        let lua = "local url = \"https://discord.com/api/webhooks/1/x\"\nos.execute('cmd.exe /c x')\nlocal w = getFileWriter(\"../../AppData/x.txt\", true, false)\nprint(\"fine\")";
        let keys: Vec<&str> = scan_lua("Mod", "client/a.lua", lua).iter().map(|f| f.key).collect();
        assert_eq!(keys, ["luaEndpoint", "luaFileEscape", "luaProcess", "luaShell"]);
        assert!(scan_lua("Mod", "client/b.lua", "local w = getFileWriter(\"layout.ini\", true, false)\nopenUrl(\"https://x\")").is_empty());
    }

    #[test]
    fn audits_the_game_folder_and_launcher() {
        let t = tempfile::tempdir().unwrap();
        let game = t.path();
        std::fs::write(game.join("projectzomboid.jar"), b"jar").unwrap();
        std::fs::write(game.join("ProjectZomboid64.json"), r#"{"classpath": [".", "projectzomboid.jar", "inject.jar"], "vmArgs": ["-agentlib:zbNative", "-Xmx3072m"], "windows": {"10": {"vmArgs": ["-javaagent:evil.jar", "-Xbootclasspath/a:x.jar"]}}}"#).unwrap();
        std::fs::write(game.join("inject.jar"), b"x").unwrap();
        std::fs::create_dir_all(game.join("zombie/core")).unwrap();
        std::fs::write(game.join("zombie/core/Core.class"), b"x").unwrap();
        std::fs::create_dir_all(game.join("jre64/lib")).unwrap();
        std::fs::write(game.join("jre64/lib/Fine.class"), b"x").unwrap();
        std::fs::write(game.join("ProjectZomboid64.bat"), "\".\\jre64\\bin\\java.exe\" -agentlib:zbNative \"-agentpath:C:\\x.dll\" -cp projectzomboid.jar zombie.gameStates.MainScreenState\r\n").unwrap();
        let env = [("JAVA_TOOL_OPTIONS", "-javaagent:C:/x.jar".to_string())];
        let found = scan_game(game, None, &env, &HashSet::new());
        let got: Vec<(&str, String)> = found.iter().map(|f| (f.key, f.vars.get("what").cloned().unwrap_or_default())).collect();
        assert_eq!(got, [
            ("javaEnvOptions", "-javaagent:C:/x.jar".to_string()),
            ("launcherAgent", "-javaagent:evil.jar".into()),
            ("launcherBootPath", "-Xbootclasspath/a:x.jar".into()),
            ("launcherClasspath", "inject.jar".into()),
            ("launcherAgent", "-agentpath:C:\\x.dll".into()),
            ("gameJar", "inject.jar".into()),
            ("gameClass", "zombie/core/Core.class".into()),
        ], "ZombieBuddy's agent and the Java runtime's own classes are expected");

        // Build 41 is loose classes and library jars itself: only the launcher and environment count
        std::fs::remove_file(game.join("projectzomboid.jar")).unwrap();
        let keys: Vec<&str> = scan_game(game, None, &[], &HashSet::new()).iter().map(|f| f.key).collect();
        assert_eq!(keys, ["launcherAgent", "launcherBootPath", "launcherAgent"]);
    }

    #[test]
    fn reads_zombiebuddy_approvals() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("mod_approvals.json");
        std::fs::write(&f, r#"{"formatVersion":2,"mods":[{"id":"A","jar_hash":"AB12","decision":true},{"id":"B","jar_hash":"cd34","decision":false}]}"#).unwrap();
        assert_eq!(read_approvals(&f), Approvals::from([("ab12".into(), true), ("cd34".into(), false)]));
    }

    #[test]
    fn approves_enabled_jars_once_and_keeps_a_no() {
        let t = tempfile::tempdir().unwrap();
        let mk = |id: &str, body: &[u8], source: &str| {
            let dir = t.path().join(id);
            std::fs::create_dir_all(dir.join("42/media/java")).unwrap();
            std::fs::write(dir.join("42/mod.info"), format!("id={id}
javaJarFile=media/java/M.jar
")).unwrap();
            std::fs::write(dir.join("42/media/java/M.jar"), body).unwrap();
            Row { id: id.into(), source: source.into(), info: dir.join("42/mod.info"), dir, ..Default::default() }
        };
        let clean = |n: &str| jar(&[(&format!("a/{n}.class"), class_file("zombie/X", "y", "z"))]);
        let (one, two) = (clean("A"), clean("B"));
        let (a, b) = (mk("A", &one, "steam:42"), mk("B", &two, "steam:43"));
        let f = t.path().join("zb/mod_approvals.json");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, format!(r#"{{"formatVersion":2,"mods":[{{"id":"B","jar_hash":"{}","decision":false}}]}}"#, sha256(&two))).unwrap();
        let none = HashSet::new();
        assert_eq!(approve(&f, &[&a, &b], &none).unwrap(), ["A"]);
        assert_eq!(read_approvals(&f), Approvals::from([(sha256(&one), true), (sha256(&two), false)]));
        assert!(approve(&f, &[&a, &b], &none).unwrap().is_empty());
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!((v["formatVersion"].as_u64(), v["mods"][1]["workshop_id"].as_u64()), (Some(2), Some(42)));
        // no Workshop item: left for ZombieBuddy to ask
        assert!(approve(&f, &[&mk("C", &clean("C"), "local:C")], &none).unwrap().is_empty());
        // a jar the scan warns about waits for the player, unless they trusted it
        let bad = b"not a jar".to_vec();
        let d = mk("D", &bad, "steam:44");
        assert!(approve(&f, &[&d], &none).unwrap().is_empty());
        // a jar that loads native code, or a mod folder carrying a program, waits for the player too
        let native = mk("E", &jar(&[("a/E.class", class_file("java/lang/System", "load", "x"))]), "steam:45");
        assert!(approve(&f, &[&native], &none).unwrap().is_empty());
        let dll = mk("F", &clean("F"), "steam:46");
        std::fs::write(dll.dir.join("42/media/java/x.dll"), b"MZ").unwrap();
        assert!(approve(&f, &[&dll], &none).unwrap().is_empty());
        assert_eq!(approve(&f, &[&d], &HashSet::from([sha256(&bad)])).unwrap(), ["D"]);
        // a "no" forgotten: allowed on the next approve
        unblock(&f, &sha256(&two)).unwrap();
        assert_eq!(approve(&f, &[&b], &none).unwrap(), ["B"]);
    }
}
