//! Countries for the server browser, only when the player turns them on (Settings `countries`).
//! PZMM asks api.country.is (open source, HTTPS, no key, MaxMind GeoLite2 data) and sends only the
//! servers' public IP addresses, 100 per request; answers are kept 30 days in
//! `<data>/countries.json`. An embedded range table could replace [`lookup`] without its callers
//! changing.

use std::collections::BTreeMap;
use std::net::IpAddr;

use anyhow::Result;
use serde_json::Value;

use super::Paths;

pub const API: &str = "https://api.country.is";
const KEEP_SECS: u64 = 30 * 86400;

type Cache = BTreeMap<String, (String, u64)>;

/// ISO 3166-1 alpha-2 codes by IP, for the public addresses among `ips` (none for those without one).
pub fn countries(paths: &Paths, ips: &[String]) -> Result<BTreeMap<String, String>> {
    countries_from(API, paths, ips)
}

/// Only public addresses leave the machine.
fn public(ip: &str) -> bool {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => !(a.is_private() || a.is_loopback() || a.is_link_local() || a.is_unspecified() || a.is_broadcast() || a.is_documentation() || a.octets()[0] == 100 && a.octets()[1] & 0xc0 == 64),
        Ok(IpAddr::V6(a)) => !(a.is_loopback() || a.is_unspecified() || (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80),
        Err(_) => false,
    }
}

fn countries_from(api: &str, paths: &Paths, ips: &[String]) -> Result<BTreeMap<String, String>> {
    let file = paths.data.join("countries.json");
    let mut cache: Cache = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let now = crate::store::now();
    let mut wanted: Vec<&str> = ips.iter().map(|s| s.as_str()).filter(|ip| public(ip)).collect();
    wanted.sort_unstable();
    wanted.dedup();
    let ask: Vec<&str> = wanted.iter().copied().filter(|ip| cache.get(*ip).is_none_or(|(_, t)| now.saturating_sub(*t) > KEEP_SECS)).collect();
    let mut failed = None;
    for chunk in ask.chunks(100) {
        match lookup(api, chunk) {
            // an address without an answer is kept as "" so it isn't asked again
            Ok(got) => cache.extend(chunk.iter().map(|ip| (ip.to_string(), (got.get(*ip).cloned().unwrap_or_default(), now)))),
            Err(e) => {
                failed = Some(e);
                break;
            }
        }
    }
    if !ask.is_empty() {
        cache.retain(|_, (_, t)| now.saturating_sub(*t) <= KEEP_SECS);
        // ponytail: last writer wins if two lookups overlap; it's only a cache
        crate::write_atomic(&file, serde_json::to_string(&cache)?)?;
    }
    if let Some(e) = failed {
        return Err(e.context("asking api.country.is for the servers' countries"));
    }
    Ok(wanted.iter().filter_map(|ip| cache.get(*ip).filter(|(c, _)| !c.is_empty()).map(|(c, _)| (ip.to_string(), c.clone()))).collect())
}

/// One request for up to 100 addresses: country codes by IP (addresses it can't place are left out).
fn lookup(api: &str, ips: &[&str]) -> Result<BTreeMap<String, String>> {
    let mut res = crate::sources::agent().post(&format!("{api}/")).header("Content-Type", "application/json").header("User-Agent", crate::USER_AGENT).send(serde_json::to_string(ips)?)?;
    let v: Vec<Value> = serde_json::from_str(&res.body_mut().read_to_string()?)?;
    Ok(v.iter()
        .filter_map(|r| Some((r["ip"].as_str()?.to_string(), r["country"].as_str().filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase()))?.to_string())))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_once_for_public_addresses_and_caches() {
        let (_t, paths) = crate::store::tests::temp_paths();
        std::fs::create_dir_all(&paths.data).unwrap();
        let (api, seen) = crate::sources::fake_http(vec![("POST /", 200, r#"[{"ip":"8.8.8.8","country":"US"},{"ip":"1.1.1.1","country":"AU"}]"#.into())]);
        let ips: Vec<String> = ["8.8.8.8", "1.1.1.1", "9.9.9.9", "192.168.1.5", "10.0.0.1", "127.0.0.1", "100.64.0.1", "8.8.8.8", "nope"].map(String::from).into();
        let got = countries_from(&api, &paths, &ips).unwrap();
        assert_eq!(got, BTreeMap::from([("1.1.1.1".into(), "AU".into()), ("8.8.8.8".into(), "US".into())]));
        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen.len(), 1);
            let sent: Vec<String> = serde_json::from_str(&seen[0].1).unwrap();
            assert_eq!(sent, ["1.1.1.1", "8.8.8.8", "9.9.9.9"], "only public addresses, once each");
        }
        // cached, the unplaced one too: nothing is asked again
        assert_eq!(countries_from(&api, &paths, &ips).unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
