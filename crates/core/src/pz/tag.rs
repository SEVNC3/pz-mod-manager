//! The PZMM list in a server's public description (`PublicDescription=`, which A2S_RULES
//! carries): its Workshop items with their Workshop update times, its mod IDs in load order and
//! its map order, so a PZMM player gets exactly its mods. It goes at the end of the owner's
//! text as `[PZMM:1:<length>:<crc32>:<data>]`, the data deflated and in URL-safe base64; the
//! length and checksum tell a cut or damaged list from a good one.

use std::io::{Read, Write};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// The longest description A2S carries whole (3,937 characters measured; a margin kept).
pub const LIMIT: usize = 3900;
const START: &str = "[PZMM:";
/// Between the owner's text and the list: a line break in PZ's descriptions.
const GAP: &str = "\\n";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tag {
    /// Each Workshop item with its Workshop update time when the list was made (0: unknown).
    pub items: Vec<(String, u64)>,
    /// The mod IDs in load order, and the map order: both empty in the short form (the full
    /// list didn't fit).
    pub mods: Vec<String>,
    pub maps: Vec<String>,
    /// Mods only the server needs, and mods players may leave out (see `host::Server::sides`):
    /// a PZMM player's join leaves both off unless ticked. Lines older PZMMs don't read.
    pub server_only: Vec<String>,
    pub optional: Vec<String>,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64(b: &[u8]) -> String {
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = c.iter().enumerate().fold(0u32, |n, (i, &x)| n | (x as u32) << (16 - 8 * i));
        for i in 0..=c.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn unbase64(s: &str) -> Result<Vec<u8>> {
    let mut out = vec![];
    for c in s.as_bytes().chunks(4) {
        let n = c.iter().enumerate().try_fold(0u32, |n, (i, x)| B64.iter().position(|b| b == x).map(|v| n | (v as u32) << (18 - 6 * i)));
        let Some(n) = n.filter(|_| c.len() > 1) else { bail!("not base64") };
        out.extend((0..c.len() - 1).map(|i| (n >> (16 - 8 * i)) as u8));
    }
    Ok(out)
}

fn crc(s: &str) -> String {
    let mut c = flate2::Crc::new();
    c.update(s.as_bytes());
    format!("{:08x}", c.sum())
}

/// The tag for `t`.
pub fn encode(t: &Tag) -> String {
    let items: Vec<String> = t.items.iter().map(|(i, at)| format!("{i}:{at}")).collect();
    let mut text = format!("{}\n{}\n{}", items.join(","), t.mods.join(";"), t.maps.join(";"));
    if !t.server_only.is_empty() || !t.optional.is_empty() {
        text += &format!("\n{}\n{}", t.server_only.join(";"), t.optional.join(";"));
    }
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    // writing to memory doesn't fail
    let _ = z.write_all(text.as_bytes());
    let data = base64(&z.finish().unwrap_or_default());
    format!("{START}1:{}:{}:{data}]", data.len(), crc(&data))
}

/// The list in a description: None when there's none, else the list or why it can't be used:
/// "cut" (cut short), "damaged" or "newer" (a newer PZMM's format).
pub fn decode(desc: &str) -> Option<Result<Tag, &'static str>> {
    let at = desc.rfind(START)?;
    Some((|| {
        let rest = &desc[at + START.len()..];
        let mut parts = rest.splitn(4, ':');
        let (v, len, sum, data) = (parts.next(), parts.next(), parts.next(), parts.next().unwrap_or(""));
        if v != Some("1") {
            return Err(if v.is_some_and(|v| v.parse::<u32>().is_ok()) { "newer" } else { "damaged" });
        }
        let len: usize = len.and_then(|l| l.parse().ok()).ok_or("damaged")?;
        // a 42.21 server drops a description's last character from A2S: the closing `]` may be gone
        let data = data.get(..len).filter(|_| data[len..].is_empty() || data[len..].starts_with(']')).ok_or("cut")?;
        if sum != Some(crc(data).as_str()) {
            return Err("damaged");
        }
        let mut text = String::new();
        let bytes = unbase64(data).map_err(|_| "damaged")?;
        flate2::read::DeflateDecoder::new(&bytes[..]).take(1 << 20).read_to_string(&mut text).map_err(|_| "damaged")?;
        let mut lines = text.split('\n');
        let mut list = |sep| lines.next().unwrap_or("").split(sep).filter(|s: &&str| !s.is_empty()).map(String::from).collect::<Vec<_>>();
        let items = list(',').iter().map(|i| i.split_once(':').map_or((i.clone(), 0), |(id, t)| (id.to_string(), t.parse().unwrap_or(0)))).collect();
        Ok(Tag { items, mods: list(';'), maps: list(';'), server_only: list(';'), optional: list(';') })
    })())
}

/// The list itself ("" when there's none).
pub fn part(desc: &str) -> &str {
    desc.rfind(START).map_or("", |at| &desc[at..])
}

/// The owner's text: the description without the list (and the line break before it).
pub fn strip(desc: &str) -> &str {
    let mut s = desc.rfind(START).map_or(desc, |at| &desc[..at]).trim_end();
    while let Some(t) = s.strip_suffix(GAP) {
        s = t.trim_end();
    }
    s
}

/// The description with the list at the end (an old one replaced), and which form fit:
/// "full", "items" (Workshop items and times only) or "none" (the owner's text alone).
pub fn with_tag(desc: &str, t: &Tag) -> (String, &'static str) {
    let own = strip(desc);
    let join = |tag: String| if own.is_empty() { tag } else { format!("{own}{GAP}{tag}") };
    let full = join(encode(t));
    if full.chars().count() <= LIMIT {
        return (full, "full");
    }
    let short = join(encode(&Tag { items: t.items.clone(), ..Default::default() }));
    if short.chars().count() <= LIMIT {
        return (short, "items");
    }
    (own.to_string(), "none")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A list like a big public server's: `n` items, `m` mods, a few maps.
    fn big(n: usize, m: usize) -> Tag {
        let mut seed = 7u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            seed >> 33
        };
        let items = (0..n).map(|_| ((2_000_000_000 + next() % 1_800_000_000).to_string(), 1_700_000_000 + next() % 90_000_000)).collect();
        let words = ["Better", "Sorting", "Cars", "Hydrocraft", "Lib", "Tweaks", "B42", "Fixes", "Zombies", "Loot"];
        let mods = (0..m).map(|i| format!("{}{}_{i}", words[next() as usize % 10], words[next() as usize % 10])).collect();
        Tag { items, mods, maps: vec!["Louisville_Riverboat".into(), "RavenCreek".into(), "Muldraugh, KY".into()], ..Default::default() }
    }

    #[test]
    fn round_trip_and_damage() {
        for s in ["", "a", "ab", "abc", "abcd", "\u{0}\u{ff}x"] {
            assert_eq!(unbase64(&base64(s.as_bytes())).unwrap(), s.as_bytes(), "{s:?}");
        }
        let t = Tag { items: vec![("2392709985".into(), 1790000000), ("111".into(), 0)], mods: vec!["tsarslib".into(), "Common Sense".into()], maps: vec!["Muldraugh, KY".into()], ..Default::default() };
        let tag = encode(&t);
        assert!(tag.starts_with("[PZMM:1:") && tag.ends_with(']') && !tag.contains(['<', '>', ';', ' ', '\\']), "{tag}");
        let (desc, form) = with_tag("Sunday Drivers <LINE> Discord: x", &t);
        assert_eq!(form, "full");
        assert_eq!(desc, format!("Sunday Drivers <LINE> Discord: x\\n{tag}"));
        assert_eq!(decode(&desc).unwrap().unwrap(), t);
        assert_eq!(decode(&desc[..desc.len() - 1]).unwrap().unwrap(), t, "as a real server sends it: without the last character");
        assert_eq!(strip(&desc), "Sunday Drivers <LINE> Discord: x");
        // replaced in place: the owner's text stays as it was
        let t2 = Tag { mods: vec!["tsarslib".into()], ..t.clone() };
        let (again, _) = with_tag(&desc, &t2);
        assert_eq!(strip(&again), strip(&desc));
        assert_eq!(decode(&again).unwrap().unwrap(), t2);
        assert_eq!(with_tag("", &t).0, tag, "no text: the list alone");
        assert!(decode("no list here").is_none());
        // cut short (A2S's limit) or changed: an error, never a wrong list
        assert_eq!(decode(&desc[..desc.len() - 5]).unwrap(), Err("cut"));
        let i = desc.len() - 3;
        let flipped = format!("{}{}{}", &desc[..i], if &desc[i..i + 1] == "A" { "B" } else { "A" }, &desc[i + 1..]);
        assert_eq!(decode(&flipped).unwrap(), Err("damaged"));
        assert_eq!(decode("[PZMM:2:4:00000000:abcd]").unwrap(), Err("newer"));
    }

    #[test]
    fn sides_ride_along_without_changing_old_lists() {
        let t = Tag { items: vec![("111".into(), 5)], mods: vec!["A".into(), "B".into(), "C".into()], ..Default::default() };
        let plain = encode(&t);
        assert_eq!(decode(&plain).unwrap().unwrap(), t, "a list without sides reads as before");
        let sided = Tag { server_only: vec!["B".into()], optional: vec!["C".into()], ..t.clone() };
        assert_ne!(encode(&sided), plain);
        assert_eq!(decode(&encode(&sided)).unwrap().unwrap(), sided);
        let only_optional = Tag { optional: vec!["C".into()], ..t.clone() };
        assert_eq!(decode(&encode(&only_optional)).unwrap().unwrap(), only_optional);
    }

    #[test]
    fn falls_back_when_it_wont_fit() {
        let own = "x".repeat(1500);
        // a typical modded server: 48 mods in 30 items
        let (d, form) = with_tag(&own, &big(30, 48));
        assert_eq!(form, "full");
        println!("30 items / 48 mods: {} characters", d.len() - own.len());
        assert!(d.len() - own.len() < 1000);
        // measured: full 1,626 characters for 60 items / 100 mods, 2,519 for 100 / 150; items only about 14.5 per item
        for (n, m) in [(60, 100), (100, 150), (150, 300), (220, 359)] {
            let t = big(n, m);
            println!("{n} items / {m} mods: full {}, items only {}", encode(&t).len(), encode(&Tag { items: t.items.clone(), ..Default::default() }).len());
        }
        // a big public server: 359 mods in 220 items: the Workshop items and times only, if the owner's text leaves room
        let huge = big(220, 359);
        let (d, form) = with_tag("Big server", &huge);
        assert_eq!(form, "items");
        assert_eq!(decode(&d).unwrap().unwrap(), Tag { items: huge.items.clone(), ..Default::default() });
        assert_eq!(with_tag(&own, &huge), (own.clone(), "none"));
        let (d, form) = with_tag(&"y".repeat(3800), &big(30, 48));
        assert_eq!((d.len(), form), (3800, "none"), "nothing fits: the owner's text alone");
    }
}
