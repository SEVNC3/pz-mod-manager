//! Workshop item file lists without downloading the items: one anonymous Steam login, then
//! each item's depot manifest (its file names and sizes) from Steam's CDN. Small files
//! (mod.info, map.info) can be read on their own, chunk by chunk. Every call to Steam gives
//! up after `LIMIT`, and the whole wait as soon as the caller's stop flag is set.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use steamroom::cdn::{CdnClient, CdnServerPool};
use steamroom::client::{LoggedIn, SteamClient};
use steamroom::depot::manifest::ManifestChunk;
use steamroom::depot::{AppId, CellId, DepotId, DepotKey, ManifestId};

use crate::pz::maps::Fail;

/// Project Zomboid: Workshop items live in the app's own depot.
const APP: u32 = 108600;

/// How long one Steam or CDN call may take before Steam counts as not answering.
const LIMIT: Duration = Duration::from_secs(30);

/// One call to Steam, given up after `limit`.
async fn timed<T, E: Into<anyhow::Error>>(limit: Duration, call: impl Future<Output = std::result::Result<T, E>>) -> Result<T> {
    tokio::time::timeout(limit, call).await.map_err(|_| anyhow::Error::new(Fail::Timeout))?.map_err(Into::into)
}

/// Run `work` on `rt` until it ends or `stop` is set.
fn until<T>(rt: &tokio::runtime::Runtime, stop: &AtomicBool, work: impl Future<Output = Result<T>>) -> Result<T> {
    rt.block_on(async {
        tokio::select! {
            r = work => r,
            _ = async {
                while !stop.load(Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } => Err(anyhow::Error::new(Fail::Stopped)),
        }
    })
}

/// One file of an item: its path (with `/`) and size.
pub struct File {
    pub path: String,
    pub size: u64,
    chunks: Vec<ManifestChunk>,
}

/// An anonymous Steam session, reused for many items. Async inside, sync outside.
pub struct Session {
    client: SteamClient<LoggedIn>,
    key: DepotKey,
    cdn: CdnClient,
    pool: CdnServerPool,
    calls: std::sync::Mutex<()>,
    // last: the client's tasks run on it
    rt: tokio::runtime::Runtime,
}

impl Session {
    /// Log in; `stop` ends the wait.
    pub fn open(stop: &AtomicBool) -> Result<Session> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
        let (client, key, servers) = until(&rt, stop, async {
            let client = timed(LIMIT, steamroom_client::login::LoginBuilder::new().anonymous().login()).await?;
            let key = timed(LIMIT, client.get_depot_decryption_key(DepotId(APP), AppId(APP))).await?;
            let servers = timed(LIMIT, client.get_cdn_servers(CellId(0), Some(5))).await?;
            anyhow::Ok((client, key, servers))
        })
        .context(Fail::Login)?;
        if servers.is_empty() {
            bail!("Steam listed no content servers");
        }
        Ok(Session { client, key, cdn: CdnClient::new()?, pool: CdnServerPool::new(servers), calls: std::sync::Mutex::new(()), rt })
    }

    /// The files in a manifest (an item's `hcontent_file`), without folders; `stop` ends the wait.
    pub fn files(&self, manifest: u64, stop: &AtomicBool) -> Result<Vec<File>> {
        let (app, depot, id) = (AppId(APP), DepotId(APP), ManifestId(manifest));
        let code = {
            // one call at a time: each caller reads the connection and drops other callers' answers
            let _one = self.calls.lock().unwrap_or_else(|e| e.into_inner());
            until(&self.rt, stop, timed(LIMIT, self.client.get_manifest_request_code(app, depot, id, None, None)))?.unwrap_or(0)
        };
        // the content servers (plain HTTP) take several readers at once
        let mut m = until(&self.rt, stop, async {
            let data = timed(LIMIT, self.cdn.download_manifest_pooled(&self.pool, depot, id, code, None)).await?;
            anyhow::Ok(steamroom_client::manifest::parse_cdn_manifest(&data)?)
        })?;
        if m.filenames_encrypted {
            m.decrypt_filenames(&self.key)?;
        }
        Ok(m.files
            .into_iter()
            .filter(|f| !steamroom::enums::DepotFileFlags::from_bits_retain(f.flags).is_directory())
            .map(|f| File { path: f.filename.replace('\\', "/"), size: f.size, chunks: f.chunks })
            .collect())
    }

    /// A file's contents (meant for small ones: one request per 1 MB chunk); `stop` ends the wait.
    pub fn read(&self, file: &File, stop: &AtomicBool) -> Result<Vec<u8>> {
        let mut chunks = file.chunks.clone();
        chunks.sort_by_key(|c| c.offset);
        until(&self.rt, stop, async {
            let mut out = vec![];
            for c in &chunks {
                // a busy content server (503) cools down while another one answers
                let mut tries = 0;
                let raw = loop {
                    let (server, _) = self.pool.pick();
                    match timed(LIMIT, self.cdn.download_chunk(server, DepotId(APP), &c.id, None)).await {
                        Ok(raw) => {
                            self.pool.report_success(server);
                            break raw;
                        }
                        Err(e) if tries + 1 >= self.pool.len().min(3) => return Err(e),
                        Err(_) => {
                            self.pool.report_failure(server, None);
                            tries += 1;
                        }
                    }
                };
                out.extend(steamroom::depot::chunk::process_chunk(&raw, &self.key, c.uncompressed_size, c.checksum)?);
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lists a real item (Louisville Riverboat) over the network: `cargo test -p pzmm-core -- --ignored`.
    #[test]
    #[ignore]
    fn lists_a_real_manifest() {
        let item = crate::sources::steam_workshop::details(&["2963883586"]).unwrap().remove(0);
        let never = AtomicBool::new(false);
        let s = Session::open(&never).unwrap();
        let files = s.files(item.hcontent, &never).unwrap();
        assert!(files.iter().any(|f| f.path.ends_with("media/maps/Louisville_Riverboat/world_51_4.lotpack")), "{:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>());
        let info = files.iter().find(|f| f.path.to_ascii_lowercase().ends_with("mod.info")).unwrap();
        assert!(String::from_utf8_lossy(&s.read(info, &never).unwrap()).contains("id="));
    }

    /// A call that never answers ends: after the limit, or at once when stopped (no network).
    #[test]
    fn hung_calls_end() {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let never = AtomicBool::new(false);
        let e = until(&rt, &never, timed(Duration::from_millis(50), std::future::pending::<Result<()>>())).unwrap_err();
        assert_eq!(e.downcast_ref::<Fail>(), Some(&Fail::Timeout));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            flag.store(true, Relaxed);
        });
        let start = std::time::Instant::now();
        let e = until(&rt, &stop, timed(LIMIT, std::future::pending::<Result<()>>())).unwrap_err();
        assert_eq!(e.downcast_ref::<Fail>(), Some(&Fail::Stopped));
        assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
        assert_eq!(until(&rt, &never, async { Ok(7) }).unwrap(), 7);
    }
}
