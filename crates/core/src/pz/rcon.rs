//! Source RCON (the dedicated server's remote console): `RCONPort`, on only while `RCONPassword` is set.
//! Long replies come in several packets with the command's id; an empty RESPONSE_VALUE sent after
//! the command comes back once they're all in (the server echoes it twice, so stale ids are skipped).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

const AUTH: i32 = 3;
const AUTH_RESPONSE: i32 = 2;
const EXEC: i32 = 2;
const RESPONSE: i32 = 0;

/// Longer commands make the server drop the connection (it was tested with 8 KB).
const MAX_COMMAND: usize = 4000;

#[derive(Debug, PartialEq)]
pub struct Packet {
    pub id: i32,
    pub kind: i32,
    pub body: String,
}

pub fn encode(id: i32, kind: i32, body: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 14);
    out.extend(((body.len() + 10) as i32).to_le_bytes());
    out.extend(id.to_le_bytes());
    out.extend(kind.to_le_bytes());
    out.extend(body.as_bytes());
    out.extend([0, 0]);
    out
}

/// The first whole packet in `buf` and its length in bytes; None until it has all arrived.
pub fn decode(buf: &[u8]) -> Result<Option<(Packet, usize)>> {
    let Some(size) = buf.get(..4).map(|b| i32::from_le_bytes(b.try_into().unwrap())) else { return Ok(None) };
    if !(10..=1 << 20).contains(&size) {
        bail!("not an RCON reply (size {size})");
    }
    let end = 4 + size as usize;
    if buf.len() < end {
        return Ok(None);
    }
    let int = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
    let body = String::from_utf8_lossy(&buf[12..end - 2]).into_owned();
    Ok(Some((Packet { id: int(4), kind: int(8), body }, end)))
}

/// A command's reply from the packets that follow it: the bodies with its `id`, up to the `marker`
/// echo. Returns the reply and whether the marker came (the whole reply is in).
pub fn reply(packets: impl IntoIterator<Item = Packet>, id: i32, marker: i32) -> (String, bool) {
    let mut out = String::new();
    for p in packets {
        if p.id == marker {
            return (out, true);
        }
        if p.id == id {
            out.push_str(&p.body);
        }
    }
    (out, false)
}

/// Commands go to the server as they are: one line, within the size the server takes.
pub fn check_command(cmd: &str) -> Result<()> {
    if cmd.trim().is_empty() || cmd.contains(['\r', '\n']) {
        bail!("a command is one line of text");
    }
    if cmd.len() > MAX_COMMAND {
        bail!("that command is {} bytes; the server drops commands over {MAX_COMMAND} bytes (about 4 KB), so split it up", cmd.len());
    }
    Ok(())
}

pub struct Rcon {
    stream: TcpStream,
    buf: Vec<u8>,
    next: i32,
}

impl Rcon {
    /// Log in to RCON at `host` (a name or an IP) and `port`.
    pub fn connect(host: &str, port: u16, password: &str) -> Result<Rcon> {
        use std::net::ToSocketAddrs;
        let addrs: Vec<_> = (host, port).to_socket_addrs().with_context(|| format!("looking up {host}"))?.collect();
        let stream = addrs
            .iter()
            .find_map(|a| TcpStream::connect_timeout(a, Duration::from_secs(5)).ok())
            .with_context(|| format!("couldn't connect to RCON at {host}:{port} (is the server running, and its RCON port open?)"))?;
        let mut r = Rcon { stream, buf: vec![], next: 10 };
        r.stream.write_all(&encode(1, AUTH, password))?;
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let p = r.read(until)?.context("the server didn't answer the RCON login")?;
            if p.kind == AUTH_RESPONSE {
                if p.id == -1 {
                    bail!("the server refused the RCON password (RCONPassword in its ini)");
                }
                return Ok(r);
            }
        }
    }

    /// The next packet, or None once `until` passes (silence, or a server trickling junk).
    fn read(&mut self, until: Instant) -> Result<Option<Packet>> {
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            if let Some((p, n)) = decode(&self.buf)? {
                self.buf.drain(..n);
                return Ok(Some(p));
            }
            self.stream.set_read_timeout(Some(left))?;
            let mut chunk = [0u8; 8192];
            match self.stream.read(&mut chunk) {
                Ok(0) => bail!("the server closed the RCON connection"),
                Ok(n) => self.buf.extend(&chunk[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Run `cmd` and return its whole reply.
    pub fn exec(&mut self, cmd: &str) -> Result<String> {
        check_command(cmd)?;
        let (id, marker) = (self.next, self.next + 1);
        self.next += 2;
        self.stream.write_all(&encode(id, EXEC, cmd))?;
        self.stream.write_all(&encode(marker, RESPONSE, ""))?;
        let mut got = vec![];
        // ponytail: 3 s without a packet (10 s in all) ends a reply whose marker never came
        let until = Instant::now() + Duration::from_secs(10);
        while let Some(p) = self.read(until.min(Instant::now() + Duration::from_secs(3)))? {
            let done = p.id == marker;
            got.push(p);
            if done {
                break;
            }
        }
        Ok(reply(got, id, marker).0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn packets_round_trip() {
        // the server's auth success, as captured: an empty RESPONSE_VALUE then AUTH_RESPONSE id 1
        let wire = hex("0a00000001000000000000000000\
                        0a00000001000000020000000000");
        let (a, n) = decode(&wire).unwrap().unwrap();
        assert_eq!((a, n), (Packet { id: 1, kind: 0, body: String::new() }, 14));
        assert_eq!(decode(&wire[n..]).unwrap().unwrap().0, Packet { id: 1, kind: AUTH_RESPONSE, body: String::new() });
        assert_eq!(encode(1, AUTH_RESPONSE, ""), wire[n..]);
        let p = encode(10, EXEC, "players");
        assert_eq!(decode(&p).unwrap().unwrap().0, Packet { id: 10, kind: EXEC, body: "players".into() });
        assert!(decode(&p[..p.len() - 1]).unwrap().is_none(), "half a packet waits for the rest");
        assert!(decode(&hex("ffffff7f")).is_err());
    }

    #[test]
    fn multi_packet_replies_reassemble() {
        // `help` came as 4086 + 1780 bytes with the command's id, then the marker (twice); a
        // stale marker from the command before arrives first
        let (a, b) = ("x".repeat(4086), "y".repeat(1780));
        let mut wire = encode(1011, RESPONSE, "");
        wire.extend(encode(12, RESPONSE, &a));
        wire.extend(encode(12, RESPONSE, &b));
        wire.extend(encode(13, RESPONSE, ""));
        wire.extend(encode(13, RESPONSE, ""));
        let mut packets = vec![];
        let mut at = 0;
        // fed a few bytes at a time, as TCP may deliver them
        let mut buf = vec![];
        for chunk in wire.chunks(1000) {
            buf.extend(chunk);
            while let Some((p, n)) = decode(&buf[at..]).unwrap() {
                packets.push(p);
                at += n;
            }
        }
        let (text, done) = reply(packets, 12, 13);
        assert!(done);
        assert_eq!(text, a + &b);
        let (text, done) = reply(vec![Packet { id: 14, kind: 0, body: "Players connected (0): ".into() }], 14, 15);
        assert_eq!((text.as_str(), done), ("Players connected (0): ", false));
    }

    /// A fake RCON server on 127.0.0.1 (password "pw"): answers `players` with one packet, then
    /// the marker's echo.
    fn fake_server() -> u16 {
        fake(|cmd| if cmd == "players" { "Players connected (1): \n-Kate\n".into() } else { "Unknown command".into() })
    }

    /// A fake RCON server on 127.0.0.1 (password "pw") answering each command with `answer`.
    fn fake(answer: fn(&str) -> String) -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let mut buf = vec![];
                let mut chunk = [0u8; 4096];
                'conn: while let Ok(n @ 1..) = s.read(&mut chunk) {
                    buf.extend(&chunk[..n]);
                    while let Some((p, used)) = decode(&buf).unwrap() {
                        buf.drain(..used);
                        let out = match (p.kind, p.body.as_str()) {
                            (AUTH, "pw") => [encode(p.id, RESPONSE, ""), encode(p.id, AUTH_RESPONSE, "")].concat(),
                            (AUTH, _) => encode(-1, AUTH_RESPONSE, ""),
                            (EXEC, cmd) => encode(p.id, RESPONSE, &answer(cmd)),
                            _ => encode(p.id, RESPONSE, ""),
                        };
                        if s.write_all(&out).is_err() {
                            break 'conn;
                        }
                    }
                }
            }
        });
        port
    }

    #[test]
    fn logs_in_to_a_server_by_host_and_port() {
        let port = fake_server();
        let mut r = Rcon::connect("localhost", port, "pw").unwrap();
        assert_eq!(r.exec("players").unwrap(), "Players connected (1): \n-Kate\n");
        assert_eq!(r.exec("help").unwrap(), "Unknown command");
        drop(r); // the fake serves one connection at a time
        let refused = Rcon::connect("127.0.0.1", port, "wrong").err().unwrap().to_string();
        assert!(refused.contains("refused the RCON password"), "{refused}");
        assert!(Rcon::connect("127.0.0.1", 1, "pw").is_err(), "nothing listens on port 1");
    }

    #[test]
    fn a_server_flooding_packets_cant_hold_a_read() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            while s.write_all(&encode(7, RESPONSE, "junk")).is_ok() {}
        });
        let mut r = Rcon { stream: TcpStream::connect(addr).unwrap(), buf: vec![], next: 10 };
        let start = Instant::now();
        let until = start + Duration::from_millis(300);
        while r.read(until).unwrap().is_some() {}
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// The game's way of reading a command: words, or "quoted words" as one.
    fn args(cmd: &str) -> Vec<String> {
        let (mut out, mut word, mut quoted) = (vec![], String::new(), false);
        for c in cmd.chars().chain([' ']) {
            match c {
                '"' => quoted = !quoted,
                ' ' if !quoted => {
                    if !word.is_empty() {
                        out.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        out
    }

    /// What the 42.x server answers the admin commands PZMM's forms send (their argument rules as
    /// the game's command classes declare them); "bad" for anything else.
    fn game(cmd: &str) -> String {
        let a = args(cmd);
        let a: Vec<&str> = a.iter().map(String::as_str).collect();
        let num = |s: &str| s.parse::<f32>().is_ok();
        let ip = |s: &str| s.split('.').count() == 4 && s.split('.').all(|n| n.parse::<u8>().is_ok());
        match a.as_slice() {
            ["banip" | "unbanip", x] if ip(x) => String::new(),
            ["banuser", u] => format!("User {u} is now banned"),
            ["banuser", u, "-ip"] => format!("User {u} is now banned (and their IP)"),
            ["voiceban", u, "-true"] => format!("User {u} voice is banned."),
            ["voiceban", u, "-false"] => format!("User {u} voice is unbanned."),
            ["startrain"] => "Rain started".into(),
            ["startrain", n] if n.parse::<u32>().is_ok_and(|n| (1..=100).contains(&n)) => "Rain started".into(),
            ["startstorm", n] if num(n) => "Thunderstorm started".into(),
            ["stopweather"] => "Weather stopped".into(),
            ["thunder" | "lightning", _] => "Thunder triggered".into(),
            ["setTimeSpeed", n] if num(n) => format!("Multiplier was set on the following value: {n}"),
            ["teleportto", u, xyz] if xyz.split(',').count() == 3 && xyz.split(',').all(|n| n.parse::<i32>().is_ok()) => format!("{u} teleported to {xyz}"),
            _ => "bad".into(),
        }
    }

    #[test]
    fn admin_commands_reach_a_server_as_the_game_reads_them() {
        let port = fake(game);
        let mut r = Rcon::connect("127.0.0.1", port, "pw").unwrap();
        // as the Players tab and the console's forms write them
        for (cmd, answer) in [
            ("banip 203.0.113.9", ""),
            ("unbanip 203.0.113.9", ""),
            (r#"banuser "Kate Smith" -ip"#, "User Kate Smith is now banned (and their IP)"),
            (r#"voiceban "Kate Smith" -true"#, "User Kate Smith voice is banned."),
            (r#"voiceban "Kate" -false"#, "User Kate voice is unbanned."),
            ("startrain 75", "Rain started"),
            ("startstorm 6", "Thunderstorm started"),
            ("stopweather", "Weather stopped"),
            (r#"lightning "Kate""#, "Thunder triggered"),
            ("setTimeSpeed 2.5", "Multiplier was set on the following value: 2.5"),
            (r#"teleportto "Kate" 10593,9734,0"#, "Kate teleported to 10593,9734,0"),
        ] {
            assert_eq!(r.exec(cmd).unwrap(), answer, "{cmd}");
        }
        assert_eq!(r.exec("startrain 0").unwrap(), "bad", "the fake checks, too");
    }

    #[test]
    fn commands_are_one_line_under_4_kb() {
        assert!(check_command("servermsg \"hi\"").is_ok());
        assert!(check_command(&"x".repeat(MAX_COMMAND)).is_ok());
        let long = check_command(&"x".repeat(MAX_COMMAND + 1)).unwrap_err().to_string();
        assert!(long.contains("4 KB"), "{long}");
        for bad in ["", "  ", "save\nquit", "a\rb"] {
            assert!(check_command(bad).is_err(), "{bad:?}");
        }
    }
}
