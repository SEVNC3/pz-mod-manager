//! Steam server queries (A2S_INFO, A2S_RULES, A2S_PLAYER) over UDP: PZ servers answer them on
//! the game port. Handles the challenge step (public servers always send one; localhost skips it)
//! and Source split packets (PZ sends even 7.7 KB of rules as one datagram, but the protocol allows splits).

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Info {
    pub name: String,
    /// The ini's `Map=`, cut to 31 characters by the server.
    pub map: String,
    pub folder: String,
    pub players: u32,
    pub max_players: u32,
    /// A join password is set.
    pub password: bool,
    /// Always "1.0.0.0" on PZ: useless (the build is in `keywords` or the `version` rule).
    pub version: String,
    pub game_port: Option<u16>,
    /// e.g. `;modded;pvp;VERSION:42.21` (`hidden` in place of the leading `;` when not public).
    pub keywords: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Player {
    pub name: String,
    /// Seconds connected.
    pub duration: f32,
}

/// Reads a reply payload (after the 4-byte header).
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.0.len() < n {
            bail!("the server's reply is cut short");
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn str(&mut self) -> Result<String> {
        let Some(end) = self.0.iter().position(|&b| b == 0) else { bail!("the server's reply is cut short") };
        let s = String::from_utf8_lossy(&self.0[..end]).into_owned();
        self.0 = &self.0[end + 1..];
        Ok(s)
    }
}

/// An A2S_INFO reply (payload starting at its 0x49 type byte).
pub fn parse_info(p: &[u8]) -> Result<Info> {
    let mut r = Reader(p);
    if r.u8()? != 0x49 {
        bail!("not an A2S_INFO reply");
    }
    r.u8()?; // protocol
    let (name, map, folder) = (r.str()?, r.str()?, r.str()?);
    r.str()?; // game
    r.u16()?; // app id (0 on PZ)
    let (players, max_players) = (r.u8()? as u32, r.u8()? as u32);
    r.take(3)?; // bots, server type, environment
    let password = r.u8()? == 1;
    r.u8()?; // VAC
    let version = r.str()?;
    let mut info = Info { name, map, folder, players, max_players, password, version, ..Default::default() };
    if let Ok(edf) = r.u8() {
        if edf & 0x80 != 0 {
            info.game_port = Some(r.u16()?);
        }
        if edf & 0x10 != 0 {
            r.take(8)?; // Steam id
        }
        if edf & 0x40 != 0 {
            r.u16()?;
            r.str()?; // SourceTV
        }
        if edf & 0x20 != 0 {
            info.keywords = r.str()?;
        }
    }
    Ok(info)
}

/// An A2S_RULES reply: (name, value) in the server's order.
pub fn parse_rules(p: &[u8]) -> Result<Vec<(String, String)>> {
    let mut r = Reader(p);
    if r.u8()? != 0x45 {
        bail!("not an A2S_RULES reply");
    }
    let n = r.u16()?;
    let mut out = vec![];
    for _ in 0..n {
        if r.0.is_empty() {
            break; // some servers cap the packet and send fewer than they count
        }
        out.push((r.str()?, r.str()?));
    }
    Ok(out)
}

/// An A2S_PLAYER reply.
pub fn parse_players(p: &[u8]) -> Result<Vec<Player>> {
    let mut r = Reader(p);
    if r.u8()? != 0x44 {
        bail!("not an A2S_PLAYER reply");
    }
    let n = r.u8()?;
    let mut out = vec![];
    for _ in 0..n {
        if r.0.len() < 10 {
            break;
        }
        r.u8()?; // index
        let name = r.str()?;
        r.i32()?; // the score: always 0 in Project Zomboid
        out.push(Player { name, duration: r.f32()? });
    }
    Ok(out)
}

/// One server's query socket.
pub struct Client {
    sock: UdpSocket,
    timeout: Duration,
}

const SINGLE: [u8; 4] = [0xff; 4];
const SPLIT: [u8; 4] = [0xfe, 0xff, 0xff, 0xff];

impl Client {
    /// `timeout` bounds each wait for a packet, and four of them a whole request (a server that
    /// keeps sending other packets can't hold PZMM).
    pub fn new(addr: SocketAddr, timeout: Duration) -> Result<Client> {
        // loopback stays on loopback (tests): Windows asks about programs listening on every interface
        let local = match (addr.is_ipv4(), addr.ip().is_loopback()) {
            (true, true) => "127.0.0.1:0",
            (true, false) => "0.0.0.0:0",
            (false, true) => "[::1]:0",
            (false, false) => "[::]:0",
        };
        let sock = UdpSocket::bind(local)?;
        sock.set_read_timeout(Some(timeout))?;
        sock.connect(addr)?;
        Ok(Client { sock, timeout })
    }

    /// One reply: a single packet, or Source split packets joined. Payload from its type byte.
    fn receive(&self, until: Instant) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 65535];
        let mut parts: Vec<Option<Vec<u8>>> = vec![];
        let mut id = None;
        loop {
            if Instant::now() > until {
                bail!("no answer");
            }
            let n = self.sock.recv(&mut buf).map_err(|e| match e.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => anyhow::anyhow!("no answer"),
                _ => anyhow::anyhow!("no answer ({e})"),
            })?;
            let pkt = &buf[..n];
            if pkt.len() > 4 && pkt[..4] == SINGLE {
                return Ok(pkt[4..].to_vec());
            }
            if pkt.len() < 12 || pkt[..4] != SPLIT {
                continue; // not A2S
            }
            let pid = i32::from_le_bytes(pkt[4..8].try_into()?);
            if pid < 0 {
                bail!("the server sent a compressed reply, which PZMM doesn't read");
            }
            let (total, num) = (pkt[8] as usize, pkt[9] as usize);
            if id != Some(pid) {
                // a new reply (a late part of an earlier one is dropped with it)
                id = Some(pid);
                parts = vec![None; total];
            }
            if num < parts.len() {
                parts[num] = Some(pkt[12..].to_vec());
            }
            if !parts.is_empty() && parts.iter().all(Option::is_some) {
                let whole: Vec<u8> = parts.into_iter().flatten().flatten().collect();
                if whole.len() < 5 || whole[..4] != SINGLE {
                    bail!("the server's split reply doesn't join up");
                }
                return Ok(whole[4..].to_vec());
            }
        }
    }

    /// Send request `kind` (with `body`), answering a challenge if one comes; returns the reply
    /// of type `reply` and the time to the first answer.
    fn request(&self, kind: u8, body: &[u8], reply: u8) -> Result<(Vec<u8>, Duration)> {
        let mut challenge: Vec<u8> = if kind == b'T' { vec![] } else { SINGLE.to_vec() };
        let start = Instant::now();
        let until = start + self.timeout * 4;
        let mut first = None;
        for _ in 0..3 {
            self.sock.send(&[&SINGLE[..], &[kind], body, &challenge].concat())?;
            loop {
                let p = self.receive(until)?;
                first.get_or_insert_with(|| start.elapsed());
                match p.first() {
                    Some(b'A') if p.len() >= 5 => {
                        challenge = p[1..5].to_vec();
                        break;
                    }
                    Some(&t) if t == reply => return Ok((p, first.unwrap_or_default())),
                    _ => continue, // a stale reply to an earlier request
                }
            }
        }
        bail!("the server kept asking for a challenge")
    }

    /// The server's info and the round trip (the ping).
    pub fn info(&self) -> Result<(Info, Duration)> {
        let (p, rtt) = self.request(b'T', b"Source Engine Query\0", 0x49)?;
        Ok((parse_info(&p)?, rtt))
    }

    pub fn rules(&self) -> Result<Vec<(String, String)>> {
        parse_rules(&self.request(b'V', &[], 0x45)?.0)
    }

    pub fn players(&self) -> Result<Vec<Player>> {
        parse_players(&self.request(b'U', &[], 0x44)?.0)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Our own 42.21 server's INFO and RULES as captured (two description sets: one stale), and a
    /// public server's A2S_PLAYER with every name replaced by "Player N".
    pub(crate) fn fixture(key: &str) -> Vec<u8> {
        let all: serde_json::Value = serde_json::from_str(include_str!("a2s-fixtures.json")).unwrap();
        let s = all[key].as_str().unwrap();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn parses_captured_replies() {
        let i = parse_info(&fixture("info")[4..]).unwrap();
        assert_eq!((i.name.as_str(), i.map.as_str()), ("pzmmtest", "Muldraugh_FireDept;Muldraugh, K"));
        assert_eq!((i.players, i.max_players, i.password), (0, 32, false));
        assert_eq!((i.version.as_str(), i.game_port, i.keywords.as_str()), ("1.0.0.0", Some(16261), "hidden;modded;pvp;VERSION:42.21"));

        let r = parse_rules(&fixture("rules")[4..]).unwrap();
        let get = |k: &str| r.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(r.len(), 29);
        assert_eq!((get("modCount"), get("version"), get("lastWipe")), (Some("48"), Some("42.21"), Some("1791695548")));
        assert!(get("mods").unwrap().starts_with(r"\SpawnSelector;"));

        let p = parse_players(&fixture("players")[4..]).unwrap();
        assert_eq!(p.len(), 98);
        assert_eq!(p[0].name, "Player 1");
        assert_eq!(p.iter().map(|p| p.duration).fold(0.0, f32::max), 8010.0);
        assert!(parse_rules(&fixture("info")[4..]).is_err(), "the type byte is checked");
        assert!(parse_info(&fixture("info")[4..60]).is_err(), "a cut reply is an error, not a panic");
    }

    /// A fake server on localhost: challenges every request, then sends `reply` (split in two).
    fn fake_server(reply: Vec<u8>) -> SocketAddr {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = s.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1500];
            for _ in 0..2 {
                let Ok((n, from)) = s.recv_from(&mut buf) else { return };
                if buf[..n].ends_with(&[0x12, 0x34, 0x56, 0x78]) {
                    let whole = [&SINGLE[..], &reply].concat();
                    let half = whole.len() / 2;
                    for (num, part) in [&whole[..half], &whole[half..]].into_iter().enumerate().rev() {
                        let head = [&SPLIT[..], &7i32.to_le_bytes(), &[2, num as u8], &1248u16.to_le_bytes()].concat();
                        s.send_to(&[&head[..], part].concat(), from).unwrap();
                    }
                } else {
                    s.send_to(&[&SINGLE[..], b"A", &[0x12, 0x34, 0x56, 0x78]].concat(), from).unwrap();
                }
            }
        });
        addr
    }

    #[test]
    fn answers_challenges_and_joins_split_replies() {
        let rules = fixture("rules")[4..].to_vec();
        let c = Client::new(fake_server(rules.clone()), Duration::from_secs(2)).unwrap();
        assert_eq!(c.rules().unwrap(), parse_rules(&rules).unwrap());
        let info = fixture("info")[4..].to_vec();
        let (i, rtt) = Client::new(fake_server(info), Duration::from_secs(2)).unwrap().info().unwrap();
        assert_eq!(i.name, "pzmmtest");
        assert!(rtt < Duration::from_secs(2));
    }

    #[test]
    fn a_silent_server_times_out() {
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let start = Instant::now();
        let e = Client::new(silent.local_addr().unwrap(), Duration::from_millis(200)).unwrap().info().unwrap_err();
        assert_eq!(e.to_string(), "no answer");
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_server_sending_junk_times_out() {
        let junk = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = junk.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1500];
            let Ok((_, from)) = junk.recv_from(&mut buf) else { return };
            // replies of the wrong type and non-A2S packets, faster than the wait for each
            for i in 0..200 {
                let pkt: &[u8] = if i % 2 == 0 { &[0xff, 0xff, 0xff, 0xff, b'D', 0] } else { b"noise" };
                let _ = junk.send_to(pkt, from);
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let start = Instant::now();
        let e = Client::new(addr, Duration::from_millis(200)).unwrap().info().unwrap_err();
        assert_eq!(e.to_string(), "no answer");
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
