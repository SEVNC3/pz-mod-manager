//! A rented server's files over FTP, with explicit TLS (`AUTH TLS`) unless the host has none. The
//! suppaftp crate does the protocol (TLS on every data connection, passive addresses behind NAT,
//! LIST formats). PZMM browses to the server's data folder, gets its settings files into a local
//! copy the editors open (`<data>/remote-servers/<name>`, laid out like a data folder), sends them
//! back after backing up the host's copies, and gets the world into a backup. Getting the world
//! again only fetches what changed (size or time), so a stopped or failed pull carries on. The
//! password lives in the OS credential store.

use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use suppaftp::types::FileType;
use suppaftp::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified};
use suppaftp::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use suppaftp::rustls::DigitallySignedStruct;
use suppaftp::{FtpError, RustlsConnector, RustlsFtpStream, rustls};

use super::Paths;
use super::server_backup::{self, Backup};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ftp {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// No TLS: the password and files cross the network readable.
    pub plain: bool,
    /// The server's data folder there (it holds Server/, Saves/ and db/).
    pub folder: String,
    /// The server's name there (`Server/<server>.ini`), often servertest.
    pub server: String,
    /// When the settings files were last got (Unix seconds; 0: never).
    pub pulled: u64,
    /// The host certificate trusted by its SHA-256 (see `fingerprint`), often a self-signed one: then
    /// only that certificate is accepted. Empty: the usual roots decide.
    pub fingerprint: String,
}

/// The settings files, after the server's name.
const SETTINGS: [&str; 4] = [".ini", "_SandboxVars.lua", "_spawnregions.lua", "_spawnpoints.lua"];

const CONNECT: Duration = Duration::from_secs(15);
const IDLE: Duration = Duration::from_secs(60);

/// PZMM's copy of a remote server's settings files (`Server/<name>.ini`...): the editors open it.
pub fn mirror(paths: &Paths, name: &str) -> PathBuf {
    paths.data.join("remote-servers").join(name)
}

fn secret(name: &str) -> String {
    format!("server-ftp:{name}")
}

pub fn password(name: &str) -> Option<String> {
    crate::settings::secret(&secret(name))
}

/// Keep the FTP password (empty: forget it).
pub fn save_password(name: &str, password: &str) -> Result<()> {
    crate::settings::set_secret(&secret(name), password)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    #[serde(skip)]
    pub modified: Option<SystemTime>,
}

type Conn = RustlsFtpStream;

/// Log in, over TLS unless `f.plain`.
pub fn connect(f: &Ftp, password: &str) -> Result<Conn> {
    let host = f.host.trim();
    if host.is_empty() || host.contains(char::is_whitespace) {
        bail!("type the host's FTP address");
    }
    // each address in turn: a host name may have an IPv6 one this network can't reach
    let mut tcp = Err(anyhow!("{host} has no address"));
    for addr in (host, f.port).to_socket_addrs().with_context(|| format!("looking up {host}"))? {
        tcp = TcpStream::connect_timeout(&addr, CONNECT).with_context(|| format!("connecting to {host}:{}", f.port));
        if tcp.is_ok() {
            break;
        }
    }
    let tcp = tcp?;
    tcp.set_read_timeout(Some(IDLE))?;
    tcp.set_write_timeout(Some(IDLE))?;
    let mut c = Conn::connect_with_stream(tcp).with_context(|| format!("{host}:{} didn't answer as an FTP server", f.port))?;
    if !f.plain {
        let (config, pinned) = tls(&f.fingerprint)?;
        c = c.into_secure(RustlsConnector::from(config), host).map_err(|e| match pinned.rejected.lock().unwrap().take() {
            Some(seen) if f.fingerprint.is_empty() => anyhow!(crate::keyed!("ftpUntrusted", "{host}'s certificate isn't one this computer can check (often a self-signed one). Its SHA-256 fingerprint is {fingerprint}: if your host shows the same, trust this certificate.", host = host, fingerprint = seen)),
            Some(seen) => anyhow!(crate::keyed!("ftpCertChanged", "{host}'s certificate changed: PZMM trusts {old} but the host now shows {fingerprint}. Trust the new one only if your host says they replaced it.", host = host, old = f.fingerprint, fingerprint = seen)),
            None => anyhow!("{host} didn't set up FTP over TLS ({e}). Without TLS your password and files cross the network readable: turn it off only if your host offers plain FTP alone"),
        })?;
    }
    c = c.passive_stream_builder(|addr| {
        let s = TcpStream::connect_timeout(&addr, CONNECT).map_err(FtpError::ConnectionError)?;
        s.set_read_timeout(Some(IDLE)).map_err(FtpError::ConnectionError)?;
        Ok(s)
    });
    // hosts behind NAT often send their private address for data connections
    c.set_passive_nat_workaround(true);
    c.login(f.user.trim(), password).map_err(|e| anyhow!("the host refused the login: {e}"))?;
    c.transfer_type(FileType::Binary)?;
    Ok(c)
}

/// A certificate's SHA-256, as hosts' panels and FTP clients show it (`AB:CD:...`).
pub fn fingerprint(der: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, der).as_ref().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// Checks the host's certificate: with a pin, that certificate and nothing else; without one, the
/// usual roots. The handshake's signature is checked either way. Keeps the fingerprint of one it
/// turned down, for the error.
#[derive(Debug)]
struct Pinned {
    roots: Arc<rustls::client::WebPkiServerVerifier>,
    pin: String,
    rejected: std::sync::Mutex<Option<String>>,
}

impl rustls::client::danger::ServerCertVerifier for Pinned {
    fn verify_server_cert(&self, cert: &CertificateDer, inter: &[CertificateDer], name: &ServerName, ocsp: &[u8], now: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        let seen = fingerprint(cert);
        let ok = if self.pin.is_empty() { self.roots.verify_server_cert(cert, inter, name, ocsp, now) } else if seen.eq_ignore_ascii_case(&self.pin) { Ok(ServerCertVerified::assertion()) } else { Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure)) };
        if ok.is_err() {
            *self.rejected.lock().unwrap() = Some(seen);
        }
        ok
    }
    fn verify_tls12_signature(&self, msg: &[u8], cert: &CertificateDer, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.roots.verify_tls12_signature(msg, cert, dss)
    }
    fn verify_tls13_signature(&self, msg: &[u8], cert: &CertificateDer, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.roots.verify_tls13_signature(msg, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.roots.supported_verify_schemes()
    }
}

fn tls(pin: &str) -> Result<(Arc<rustls::ClientConfig>, Arc<Pinned>)> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = Arc::new(rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() });
    let roots = rustls::client::WebPkiServerVerifier::builder_with_provider(roots, provider.clone()).build()?;
    let pinned = Arc::new(Pinned { roots, pin: pin.trim().to_string(), rejected: Default::default() });
    let config = rustls::ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions()?.dangerous().with_custom_certificate_verifier(pinned.clone()).with_no_client_auth();
    Ok((Arc::new(config), pinned))
}

/// `dir/name` on the host.
fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn list(c: &mut Conn, dir: &str) -> Result<Vec<Entry>> {
    let lines = c.list(Some(dir)).with_context(|| format!("listing {dir}"))?;
    Ok(lines
        .iter()
        .filter_map(|l| suppaftp::list::File::try_from(l.as_str()).ok())
        // a name from the host never reaches outside the folder it's saved in
        .filter(|f| crate::safe_file_name(f.name()) && f.name() != "." && f.name() != "..")
        .map(|f| Entry { name: f.name().to_string(), dir: f.is_directory(), size: f.size() as u64, modified: Some(f.modified()) })
        .collect())
}

/// A folder on the host ("": where the login lands): its path and what's in it, folders first.
pub fn browse(f: &Ftp, password: &str, dir: &str) -> Result<(String, Vec<Entry>)> {
    let mut c = connect(f, password)?;
    let dir = if dir.trim().is_empty() { c.pwd()? } else { dir.trim().to_string() };
    let mut entries = list(&mut c, &dir)?;
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let _ = c.quit();
    Ok((dir, entries))
}

fn get(c: &mut Conn, remote: &str) -> Result<Vec<u8>> {
    Ok(c.retr_as_buffer(remote)?.into_inner())
}

/// Get the settings files into PZMM's copy (named after `name`); the ini must be there. Returns the
/// files got.
pub fn pull_settings(paths: &Paths, name: &str, f: &Ftp, password: &str) -> Result<Vec<PathBuf>> {
    let mut c = connect(f, password)?;
    let dir = mirror(paths, name).join("Server");
    fs::create_dir_all(&dir)?;
    let mut got = vec![];
    for sfx in SETTINGS {
        let remote = join(&join(&f.folder, "Server"), &format!("{}{sfx}", f.server));
        match get(&mut c, &remote) {
            Ok(bytes) => {
                let local = dir.join(format!("{name}{sfx}"));
                crate::write_atomic(&local, bytes).with_context(|| format!("saving {}", local.display()))?;
                got.push(local);
            }
            Err(e) if sfx == ".ini" => return Err(e.context(format!("getting {remote}: is the folder the one that holds Server and Saves, and is the name right?"))),
            Err(_) => {} // the others are optional
        }
    }
    let _ = c.quit();
    Ok(got)
}

/// Send PZMM's copy of the settings files back; the host's copies are backed up first (an
/// "upload" backup). Returns that backup, when the host had any of them.
pub fn push_settings(paths: &Paths, name: &str, f: &Ftp, password: &str) -> Result<Option<PathBuf>> {
    let dir = mirror(paths, name).join("Server");
    let local: Vec<(&str, PathBuf)> = SETTINGS.iter().map(|s| (*s, dir.join(format!("{name}{s}")))).filter(|(_, p)| p.is_file()).collect();
    if local.is_empty() {
        bail!("get the settings files first");
    }
    let mut c = connect(f, password)?;
    let remote = |sfx: &str| join(&join(&f.folder, "Server"), &format!("{}{sfx}", f.server));
    let tmp = tempfile::tempdir_in(&paths.data)?;
    let mut items = vec![];
    for (sfx, _) in &local {
        if let Ok(bytes) = get(&mut c, &remote(sfx)) {
            let p = tmp.path().join(format!("{name}{sfx}"));
            fs::write(&p, bytes)?;
            items.push((p, format!("Server/{name}{sfx}")));
        }
    }
    let backup = server_backup::new_file(&server_backup::dir(paths, name), "upload");
    let backed = server_backup::zip_items(&items, &backup)?;
    for (sfx, p) in &local {
        c.put_file(remote(sfx), &mut fs::File::open(p)?).with_context(|| format!("sending {}", remote(sfx)))?;
    }
    let _ = c.quit();
    crate::say!("ftpSent", "Sent {name}'s settings files to its host", name = name);
    Ok(backed.then_some(backup))
}

/// (on the host, here) pairs.
type Pairs = Vec<(String, String)>;

/// What a world pull fetches: the save's folders (recursively) and the other files.
fn world_parts(server: &str, name: &str) -> (Pairs, Pairs) {
    let dirs = ["", "_player"].map(|s| (format!("Saves/Multiplayer/{server}{s}"), format!("Saves/Multiplayer/{name}{s}"))).to_vec();
    let files = SETTINGS.iter().map(|s| (format!("Server/{server}{s}"), format!("Server/{name}{s}"))).chain([(format!("db/{server}.db"), format!("db/{name}.db"))]).collect();
    (dirs, files)
}

fn walk(c: &mut Conn, remote: &str, local: &str, out: &mut Vec<(String, String, Entry)>) -> Result<()> {
    for e in list(c, remote)? {
        let (r, l) = (join(remote, &e.name), format!("{local}/{}", e.name));
        if e.dir {
            walk(c, &r, &l, out)?;
        } else {
            out.push((r, l, e));
        }
    }
    Ok(())
}

/// Get the server's world (its save, database and settings files) into `<mirror>/world`, then zip
/// it into a backup ("ftp"). Only what changed since the last pull is fetched.
pub fn pull_world(paths: &Paths, name: &str, f: &Ftp, password: &str, stop: &AtomicBool) -> Result<Backup> {
    let mut c = connect(f, password)?;
    let root = mirror(paths, name).join("world");
    let (dirs, files) = world_parts(&f.server, name);
    let mut todo = vec![];
    for (i, (r, l)) in dirs.iter().enumerate() {
        let there = join(&f.folder, r);
        // some hosts list a missing folder as empty
        match walk(&mut c, &there, l, &mut todo) {
            Ok(()) if i > 0 || !todo.is_empty() => {}
            Ok(()) => bail!("no world at {there}: is the folder the one that holds Server and Saves, and is the name right?"),
            Err(e) if i == 0 => return Err(e.context(format!("no world at {there}"))),
            Err(_) => {} // no _player folder
        }
    }
    for parent in ["Server", "db"] {
        let there = list(&mut c, &join(&f.folder, parent)).unwrap_or_default();
        for (r, l) in files.iter().filter(|(r, _)| r.starts_with(parent)) {
            if let Some(e) = there.iter().find(|e| !e.dir && Some(e.name.as_str()) == r.rsplit('/').next()) {
                todo.push((join(&f.folder, r), l.clone(), e.clone()));
            }
        }
    }
    crate::say!("ftpWorld", "Getting {name}'s world from its host: {count} files", name = name, count = todo.len());
    let total = todo.len();
    for (i, (r, l, e)) in todo.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            bail!("stopped; get the world again to carry on where it stopped");
        }
        crate::progress(i, total);
        let local = root.join(l);
        let same = fs::metadata(&local).is_ok_and(|m| m.len() == e.size && m.modified().ok() == e.modified);
        if same {
            continue;
        }
        fs::create_dir_all(local.parent().context("a file's folder")?)?;
        let part = local.with_extension("pzmm-part");
        let got = c.retr(r, |rd| {
            let mut out = fs::File::create(&part).map_err(FtpError::ConnectionError)?;
            std::io::copy(rd, &mut out).map_err(FtpError::ConnectionError)?;
            Ok(())
        });
        if let Err(e) = got {
            let _ = fs::remove_file(&part);
            return Err(e).with_context(|| format!("getting {r}"));
        }
        // ponytail: LIST times are to the minute, so a same-size change within that minute is missed until the next one
        if let Some(t) = e.modified {
            fs::File::options().write(true).open(&part)?.set_modified(t)?;
        }
        fs::rename(&part, &local)?;
    }
    crate::progress(total, total);
    let _ = c.quit();
    // what the host no longer has goes from the copy too
    let keep: std::collections::BTreeSet<&str> = todo.iter().map(|(_, l, _)| l.as_str()).collect();
    for (_, l) in &dirs {
        let mut here = vec![];
        server_backup::walk(&root, l, &mut here);
        for (p, _) in here.iter().filter(|(_, rel)| !keep.contains(rel.as_str())) {
            let _ = fs::remove_file(p);
        }
    }
    let s = super::host::Server { name: name.into(), cachedir: Some(root), ..Default::default() };
    server_backup::create(paths, &s, "ftp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use suppaftp::rustls::pki_types::pem::PemObject;

    /// A small FTP server on 127.0.0.1 serving `root` (plain FTP, passive mode, password "secret").
    fn fake_ftp(root: PathBuf) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let root = root.clone();
                std::thread::spawn(move || session(s, &root));
            }
        });
        port
    }

    fn session(s: TcpStream, root: &Path) {
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut w = s;
        let mut data: Option<TcpListener> = None;
        let say = |w: &mut TcpStream, t: &str| w.write_all(format!("{t}\r\n").as_bytes()).unwrap();
        say(&mut w, "220 fake");
        let path = |arg: &str| root.join(arg.trim_start_matches('/'));
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim_end();
            let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
            match cmd {
                "USER" => say(&mut w, "331 password"),
                "PASS" if arg == "secret" => say(&mut w, "230 in"),
                "PASS" => say(&mut w, "530 no"),
                "TYPE" => say(&mut w, "200 ok"),
                "PWD" => say(&mut w, "257 \"/\" here"),
                "PASV" => {
                    let l = TcpListener::bind("127.0.0.1:0").unwrap();
                    let p = l.local_addr().unwrap().port();
                    data = Some(l);
                    say(&mut w, &format!("227 Entering Passive Mode (127,0,0,1,{},{})", p >> 8, p & 255));
                }
                "LIST" => {
                    let mut out = String::new();
                    for e in fs::read_dir(path(arg)).into_iter().flatten().flatten() {
                        let m = e.metadata().unwrap();
                        let kind = if m.is_dir() { 'd' } else { '-' };
                        out += &format!("{kind}rw-r--r-- 1 pz pz {} Jan 02 2024 {}\r\n", if m.is_dir() { 0 } else { m.len() }, e.file_name().to_string_lossy());
                    }
                    say(&mut w, "150 here");
                    let (mut d, _) = data.take().unwrap().accept().unwrap();
                    d.write_all(out.as_bytes()).unwrap();
                    drop(d);
                    say(&mut w, "226 done");
                }
                "RETR" => match fs::read(path(arg)) {
                    Ok(bytes) => {
                        say(&mut w, "150 here");
                        let (mut d, _) = data.take().unwrap().accept().unwrap();
                        d.write_all(&bytes).unwrap();
                        drop(d);
                        // the connection dropped part way
                        say(&mut w, if arg.ends_with("broken.bin") { "426 aborted" } else { "226 done" });
                    }
                    Err(_) => say(&mut w, "550 no such file"),
                },
                "STOR" => {
                    say(&mut w, "150 send");
                    let (mut d, _) = data.take().unwrap().accept().unwrap();
                    let mut bytes = vec![];
                    d.read_to_end(&mut bytes).unwrap();
                    fs::write(path(arg), bytes).unwrap();
                    say(&mut w, "226 done");
                }
                "QUIT" => return say(&mut w, "221 bye"),
                _ => say(&mut w, "502 not here"),
            }
        }
    }

    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn browses_pulls_pushes_and_backs_up_a_world_on_a_local_server() {
        let host = tempfile::tempdir().unwrap();
        let h = host.path();
        put(h, "Zomboid/Server/servertest.ini", "PVP=true\nMods=a\n");
        put(h, "Zomboid/Server/servertest_SandboxVars.lua", "SandboxVars = { Zombies = 3 }\n");
        put(h, "Zomboid/Saves/Multiplayer/servertest/map_t.bin", "time");
        put(h, "Zomboid/Saves/Multiplayer/servertest/map/1/2.bin", "chunk");
        put(h, "Zomboid/db/servertest.db", "players");
        let port = fake_ftp(h.to_path_buf());
        let (_t, paths) = crate::store::tests::temp_paths();
        fs::create_dir_all(&paths.data).unwrap();
        let f = Ftp { host: "127.0.0.1".into(), port, user: "pz".into(), plain: true, folder: "/Zomboid".into(), server: "servertest".into(), ..Default::default() };

        assert!(connect(&f, "wrong").err().unwrap().to_string().contains("refused the login"));
        let (dir, entries) = browse(&f, "secret", "").unwrap();
        assert_eq!(dir, "/");
        assert_eq!(entries.iter().map(|e| (e.name.as_str(), e.dir)).collect::<Vec<_>>(), [("Zomboid", true)]);
        let (_, inside) = browse(&f, "secret", "/Zomboid").unwrap();
        assert_eq!(inside.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["db", "Saves", "Server"], "folders, by name");

        // settings: got under PZMM's name for the server, edited, sent back with a backup
        let got = pull_settings(&paths, "Rented", &f, "secret").unwrap();
        assert_eq!(got.len(), 2, "ini and sandbox; the rest are optional");
        let ini = mirror(&paths, "Rented").join("Server/Rented.ini");
        assert_eq!(fs::read_to_string(&ini).unwrap(), "PVP=true\nMods=a\n");
        fs::write(&ini, "PVP=false\nMods=a;b\n").unwrap();
        let backup = push_settings(&paths, "Rented", &f, "secret").unwrap().unwrap();
        assert_eq!(fs::read_to_string(h.join("Zomboid/Server/servertest.ini")).unwrap(), "PVP=false\nMods=a;b\n");
        let mut z = zip::ZipArchive::new(fs::File::open(&backup).unwrap()).unwrap();
        let mut old = String::new();
        z.by_name("Server/Rented.ini").unwrap().read_to_string(&mut old).unwrap();
        assert_eq!(old, "PVP=true\nMods=a\n", "the host's copy from before");

        // the world: everything, then only what changed; what's gone on the host goes here too
        let b = pull_world(&paths, "Rented", &f, "secret", &AtomicBool::new(false)).unwrap();
        let names: Vec<String> = zip::ZipArchive::new(fs::File::open(&b.file).unwrap()).unwrap().file_names().map(|n| n.unwrap().to_string()).collect();
        for want in ["Saves/Multiplayer/Rented/map_t.bin", "Saves/Multiplayer/Rented/map/1/2.bin", "db/Rented.db", "Server/Rented.ini"] {
            assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
        }
        let world = mirror(&paths, "Rented").join("world/Saves/Multiplayer/Rented");
        let before = fs::metadata(world.join("map_t.bin")).unwrap().modified().unwrap();
        fs::remove_file(h.join("Zomboid/Saves/Multiplayer/servertest/map/1/2.bin")).unwrap();
        pull_world(&paths, "Rented", &f, "secret", &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::metadata(world.join("map_t.bin")).unwrap().modified().unwrap(), before, "unchanged: not fetched again");
        assert!(!world.join("map/1/2.bin").exists());
        assert!(pull_world(&paths, "Rented", &f, "secret", &AtomicBool::new(true)).unwrap_err().to_string().contains("stopped"));
        assert!(pull_world(&paths, "Rented", &Ftp { server: "other".into(), ..f.clone() }, "secret", &AtomicBool::new(false)).is_err());
        put(h, "Zomboid/Saves/Multiplayer/servertest/broken.bin", "half");
        assert!(pull_world(&paths, "Rented", &f, "secret", &AtomicBool::new(false)).is_err());
        assert!(!world.join("broken.pzmm-part").exists() && !world.join("broken.bin").exists(), "no half file left");
    }

    /// Self-signed test certificates (P-256, CN ftp.test); A's key serves A.
    const CERT_A: &str = "-----BEGIN CERTIFICATE-----
MIIBfDCCASOgAwIBAgIUcEUl3TolfXZkqgaAFUAYH8tmq3EwCgYIKoZIzj0EAwIw
EzERMA8GA1UEAwwIZnRwLnRlc3QwIBcNMjYxMDEyMDQyMTMwWhgPMjEyNjA5MTgw
NDIxMzBaMBMxETAPBgNVBAMMCGZ0cC50ZXN0MFkwEwYHKoZIzj0CAQYIKoZIzj0D
AQcDQgAEEs9i+9y82agofwT4IdrSFxbBggTfpxc5Ape4eAyeDv2t92zGA9eHHcnD
jCO6GFD0gIT0RPyf3ZN/cu2LbPG59aNTMFEwHQYDVR0OBBYEFExKcgN9NI4iym+n
VHnqUDky2fbEMB8GA1UdIwQYMBaAFExKcgN9NI4iym+nVHnqUDky2fbEMA8GA1Ud
EwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDRwAwRAIgN/mxzlMb59YbNdRRdbUJe0vQ
FnB55WFzPN+W1rOxbeICIEOndzWALWC79zkOOAwwTRJP69gwQ+En2oMLrJwrtZFk
-----END CERTIFICATE-----";
    const KEY_A: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgXJm9yakww3kbOfj1
jtd35J8We9Ag93YkUNksZr5d+qOhRANCAAQSz2L73LzZqCh/BPgh2tIXFsGCBN+n
FzkCl7h4DJ4O/a33bMYD14cdycOMI7oYUPSAhPRE/J/dk39y7Yts8bn1
-----END PRIVATE KEY-----";
    const FP_A: &str = "E9:92:67:12:B7:19:A6:16:C3:39:7B:0B:D0:E2:78:63:BC:09:55:8C:3E:3C:B1:31:19:E3:80:2B:71:D8:C2:EB";
    const FP_B: &str = "34:18:02:3A:F6:EF:61:4B:42:4F:42:50:7B:50:60:BC:9C:74:89:92:6E:6E:6B:A5:8A:11:34:85:0A:8E:83:67";

    /// A TLS handshake in memory with a server showing certificate A; what the client turned down.
    fn handshake(pin: &str) -> (bool, Option<String>) {
        use rustls::pki_types::PrivateKeyDer;
        let (config, pinned) = tls(pin).unwrap();
        let cert = CertificateDer::from_pem_slice(CERT_A.as_bytes()).unwrap();
        let key = PrivateKeyDer::from_pem_slice(KEY_A.as_bytes()).unwrap();
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider())).with_safe_default_protocol_versions().unwrap().with_no_client_auth().with_single_cert(vec![cert], key).unwrap();
        let mut s = rustls::ServerConnection::new(Arc::new(server)).unwrap();
        let mut c = rustls::ClientConnection::new(config, ServerName::try_from("ftp.test").unwrap()).unwrap();
        let ok = (|| -> Result<(), rustls::Error> {
            while c.is_handshaking() || s.is_handshaking() {
                let mut buf = vec![];
                c.write_tls(&mut buf).unwrap();
                s.read_tls(&mut buf.as_slice()).unwrap();
                s.process_new_packets()?;
                buf.clear();
                s.write_tls(&mut buf).unwrap();
                c.read_tls(&mut buf.as_slice()).unwrap();
                c.process_new_packets()?;
            }
            Ok(())
        })();
        (ok.is_ok(), pinned.rejected.lock().unwrap().take())
    }

    #[test]
    fn a_pinned_certificate_is_trusted_and_no_other() {
        assert_eq!(fingerprint(&CertificateDer::from_pem_slice(CERT_A.as_bytes()).unwrap()), FP_A, "as openssl shows it");
        assert_eq!(handshake(FP_A), (true, None));
        assert_eq!(handshake(&FP_A.to_lowercase()), (true, None));
        assert_eq!(handshake(FP_B), (false, Some(FP_A.into())), "a changed certificate");
        assert_eq!(handshake(""), (false, Some(FP_A.into())), "self-signed, nothing pinned");
    }
}
