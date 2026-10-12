//! A save's sandbox settings: `map_sand.bin`, as the game's `SandboxOptions.save` writes it: "SAND",
//! the world version and the sandbox version (big-endian i32), the option count (i32), each option's
//! name and value as strings (big-endian i16 byte length, UTF-8), then the world's preset name.
//! The form around the values comes from the installed game, like the server form comes from the
//! comments the server writes: each option's kind, limits and default from `SandboxOptions`' constructors
//! in projectzomboid.jar, its label, tooltip and choice names from the game's English Sandbox translation.
//! Options the game doesn't define (mods') get a kind from their value.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::server_config::{Setting, check_value, keep_prev};

#[derive(Clone, Debug, PartialEq)]
pub struct Sand {
    pub world_version: i32,
    pub version: i32,
    /// (name, value as text), in file order.
    pub options: Vec<(String, String)>,
    /// The preset the world was started with (missing in old files).
    pub preset: Option<String>,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.0.len() < n {
            bail!("the file ends early");
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn string(&mut self) -> Result<String> {
        let n = self.u16()? as i16;
        Ok(if n <= 0 { String::new() } else { String::from_utf8(self.take(n as usize)?.to_vec())? })
    }
}

pub fn parse(bytes: &[u8]) -> Result<Sand> {
    let mut r = Reader(bytes);
    if r.take(4)? != b"SAND" {
        bail!("not a sandbox file");
    }
    let world_version = r.u32()? as i32;
    let version = r.u32()? as i32;
    let n = r.u32()?;
    if n > 10_000 {
        bail!("{n} options: the file is damaged");
    }
    let options = (0..n).map(|_| Ok((r.string()?, r.string()?))).collect::<Result<Vec<_>>>()?;
    let preset = if r.0.is_empty() { None } else { Some(r.string()?) };
    if !r.0.is_empty() {
        bail!("{} unknown bytes at the end", r.0.len());
    }
    Ok(Sand { world_version, version, options, preset })
}

pub fn bytes(s: &Sand) -> Result<Vec<u8>> {
    let mut out = b"SAND".to_vec();
    out.extend(s.world_version.to_be_bytes());
    out.extend(s.version.to_be_bytes());
    out.extend((s.options.len() as u32).to_be_bytes());
    let mut string = |t: &str| -> Result<()> {
        let n = i16::try_from(t.len()).context("a value is too long")?;
        out.extend(n.to_be_bytes());
        out.extend(t.as_bytes());
        Ok(())
    };
    for (k, v) in &s.options {
        string(k)?;
        string(v)?;
    }
    if let Some(p) = &s.preset {
        string(p)?;
    }
    Ok(out)
}

// --- the options the game defines, read from its classes ---

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Bool,
    Int,
    Double,
    /// Choices 1..=n; 0: as many as the translation names.
    Enum(u32),
    Text,
}

#[derive(Clone, Debug, PartialEq)]
struct Meta {
    kind: Kind,
    min: Option<f64>,
    max: Option<f64>,
    default: Option<String>,
    /// Label key `Sandbox_<translation>`, else the name after its last dot.
    translation: Option<String>,
    /// Choice labels `Sandbox_<value translation>_option<i>`, else as the label.
    value_translation: Option<String>,
}

enum Cp {
    None,
    Utf8(String),
    Num(f64),
    Class(u16),
    Str(u16),
    Ref(u16, u16),
    Nat(u16, u16),
}

/// A class file's constant pool and the code of its constructors.
fn read_class(b: &[u8]) -> Result<(Vec<Cp>, Vec<Vec<u8>>)> {
    let mut r = Reader(b);
    if r.take(4)? != [0xCA, 0xFE, 0xBA, 0xBE] {
        bail!("not a class file");
    }
    r.take(4)?;
    let n = r.u16()? as usize;
    let mut pool = vec![Cp::None];
    while pool.len() < n {
        let tag = r.u8()?;
        let e = match tag {
            1 => {
                let len = r.u16()? as usize;
                Cp::Utf8(String::from_utf8_lossy(r.take(len)?).into_owned())
            }
            3 => Cp::Num(r.u32()? as i32 as f64),
            // as written in the source (0.65f, not 0.6499999761581421)
            4 => Cp::Num(f32::from_bits(r.u32()?).to_string().parse()?),
            5 | 6 => {
                let v = u64::from_be_bytes(r.take(8)?.try_into()?);
                pool.push(Cp::Num(if tag == 5 { v as i64 as f64 } else { f64::from_bits(v) }));
                Cp::None // 8-byte constants take two slots
            }
            7 => Cp::Class(r.u16()?),
            8 => Cp::Str(r.u16()?),
            9..=11 => Cp::Ref(r.u16()?, r.u16()?),
            12 => Cp::Nat(r.u16()?, r.u16()?),
            15 => {
                r.take(3)?;
                Cp::None
            }
            16 | 19 | 20 => {
                r.take(2)?;
                Cp::None
            }
            17 | 18 => {
                r.take(4)?;
                Cp::None
            }
            _ => bail!("unknown constant tag {tag}"),
        };
        pool.push(e);
    }
    r.take(6)?;
    let interfaces = r.u16()? as usize;
    r.take(interfaces * 2)?;
    let utf8 = |pool: &[Cp], i: u16| match pool.get(i as usize) {
        Some(Cp::Utf8(s)) => s.clone(),
        _ => String::new(),
    };
    let mut inits = vec![];
    for members in 0..2 {
        for _ in 0..r.u16()? {
            r.take(2)?;
            let name = utf8(&pool, r.u16()?);
            r.take(2)?;
            for _ in 0..r.u16()? {
                let attr = utf8(&pool, r.u16()?);
                let len = r.u32()? as usize;
                let data = r.take(len)?;
                if members == 1 && name == "<init>" && attr == "Code" && data.len() >= 8 {
                    let code_len = u32::from_be_bytes(data[4..8].try_into()?) as usize;
                    inits.push(data.get(8..8 + code_len).context("short Code")?.to_vec());
                }
            }
        }
    }
    Ok((pool, inits))
}

/// Length of the instruction at `pc` (with its operands).
fn op_len(code: &[u8], pc: usize) -> usize {
    let int = |at: usize| code.get(at..at + 4).map_or(0, |b| i32::from_be_bytes(b.try_into().unwrap_or_default()));
    match code[pc] {
        0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xa9 | 0xbc => 2,
        0x11 | 0x13 | 0x14 | 0x84 | 0x99..=0xa8 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 | 0xc6 | 0xc7 => 3,
        0xc5 => 4,
        0xb9 | 0xba | 0xc8 | 0xc9 => 5,
        0xc4 => {
            if code.get(pc + 1) == Some(&0x84) {
                6
            } else {
                4
            }
        }
        0xaa => {
            let at = (pc + 4) & !3;
            let (lo, hi) = (int(at + 4), int(at + 8));
            at - pc + 12 + 4 * (hi - lo + 1).max(0) as usize
        }
        0xab => {
            let at = (pc + 4) & !3;
            at - pc + 8 + 8 * int(at + 4).max(0) as usize
        }
        _ => 1,
    }
}

#[derive(Debug)]
enum Val {
    Str(String),
    Num(f64),
    Other,
}

/// How many parameters a method descriptor `(...)R` takes.
fn params(desc: &str) -> usize {
    let mut it = desc.trim_start_matches('(').chars();
    let mut n = 0;
    while let Some(c) = it.next() {
        match c {
            ')' => break,
            '[' => continue,
            'L' => {
                it.by_ref().find(|c| *c == ';');
                n += 1;
            }
            _ => n += 1,
        }
    }
    n
}

fn num(n: f64) -> String {
    format!("{n}")
}

/// The `new*Option(...)` calls in the constructors: the constants loaded since the previous call are its arguments.
fn scan_class(pool: &[Cp], code: &[u8], out: &mut Vec<(String, Meta)>) {
    let utf8 = |i: u16| match pool.get(i as usize) {
        Some(Cp::Utf8(s)) => s.as_str(),
        _ => "",
    };
    let constant = |i: u16| match pool.get(i as usize) {
        Some(Cp::Num(n)) => Val::Num(*n),
        Some(Cp::Str(s)) => Val::Str(utf8(*s).to_string()),
        _ => Val::Other,
    };
    let method = |i: u16| match pool.get(i as usize) {
        Some(Cp::Ref(c, nat)) => {
            let class = match pool.get(*c as usize) {
                Some(Cp::Class(n)) => utf8(*n),
                _ => "",
            };
            match pool.get(*nat as usize) {
                Some(Cp::Nat(n, d)) => (class, utf8(*n), utf8(*d)),
                _ => (class, "", ""),
            }
        }
        _ => ("", "", ""),
    };
    let mut args: Vec<Val> = vec![];
    let mut pc = 0;
    while pc < code.len() {
        let op = code[pc];
        let u16_at = |at: usize| code.get(at..at + 2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]));
        match op {
            0x01 => args.push(Val::Other),
            0x02..=0x08 => args.push(Val::Num(op as f64 - 3.0)),
            0x09 | 0x0a => args.push(Val::Num((op - 0x09) as f64)),
            0x0b..=0x0d => args.push(Val::Num((op - 0x0b) as f64)),
            0x0e | 0x0f => args.push(Val::Num((op - 0x0e) as f64)),
            0x10 => args.push(Val::Num(code.get(pc + 1).map_or(0, |b| *b as i8) as f64)),
            0x11 => args.push(Val::Num(u16_at(pc + 1) as i16 as f64)),
            0x12 => args.push(constant(code.get(pc + 1).copied().unwrap_or(0) as u16)),
            0x13 | 0x14 => args.push(constant(u16_at(pc + 1))),
            0xb6..=0xb9 => {
                let (class, name, desc) = method(u16_at(pc + 1));
                let tail = |n: usize| args.len().checked_sub(n).map(|at| &args[at..]);
                let n = |v: &Val| if let Val::Num(n) = v { Some(*n) } else { None };
                let made = if class == "zombie/SandboxOptions" {
                    match (name, tail(params(desc))) {
                        ("newBooleanOption", Some([Val::Str(k), Val::Num(d)])) => Some((k, Kind::Bool, None, None, Some(if *d != 0.0 { "true" } else { "false" }.to_string()))),
                        ("newDoubleOption", Some([Val::Str(k), lo, hi, d])) => Some((k, Kind::Double, n(lo), n(hi), n(d).map(num))),
                        ("newIntegerOption", Some([Val::Str(k), lo, hi, d])) => Some((k, Kind::Int, n(lo), n(hi), n(d).map(num))),
                        ("newEnumOption", Some([Val::Str(k), Val::Num(count), d])) if desc.starts_with("(Ljava/lang/String;II)") => Some((k, Kind::Enum(*count as u32), Some(1.0), Some(*count), n(d).map(num))),
                        ("newEnumOption", _) => args.iter().rev().nth(1).and_then(|v| if let Val::Str(k) = v { Some((k, Kind::Enum(0), None, None, None)) } else { None }),
                        ("newStringOption", Some([Val::Str(k), Val::Str(d), _])) => Some((k, Kind::Text, None, None, Some(d.clone()))),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some((k, kind, min, max, default)) = made {
                    out.push((k.clone(), Meta { kind, min, max, default, translation: None, value_translation: None }));
                } else if class.starts_with("zombie/SandboxOptions$")
                    && let (Some((_, m)), Some(Val::Str(t))) = (out.last_mut(), args.last())
                {
                    match name {
                        "setTranslation" => m.translation = Some(t.clone()),
                        "setValueTranslation" => m.value_translation = Some(t.clone()),
                        _ => {}
                    }
                }
                args.clear();
            }
            0xba => args.clear(),
            _ => {}
        }
        pc += op_len(code, pc);
    }
}

/// The options the game defines, by name, from `projectzomboid.jar` in `dir`.
fn game_options(dir: &Path) -> Result<HashMap<String, Meta>> {
    let jar = dir.join("projectzomboid.jar");
    let mut z = zip::ZipArchive::new(fs::File::open(&jar).with_context(|| format!("opening {}", jar.display()))?)?;
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok()).filter(|n| n.starts_with("zombie/SandboxOptions") && n.ends_with(".class")).map(|n| n.into_owned()).collect();
    let mut out = vec![];
    for n in names {
        let mut b = vec![];
        z.by_name(&n)?.read_to_end(&mut b)?;
        let (pool, inits) = read_class(&b).with_context(|| format!("reading {n}"))?;
        for code in inits {
            scan_class(&pool, &code, &mut out);
        }
    }
    Ok(out.into_iter().collect())
}

/// The game's English sandbox texts (B42 `Sandbox.json`, B41 `Sandbox_EN.txt`), by key.
fn texts(dir: &Path) -> HashMap<String, String> {
    let t = dir.join("media/lua/shared/Translate/EN");
    if let Ok(json) = fs::read_to_string(t.join("Sandbox.json")) {
        return serde_json::from_str(&json).unwrap_or_default();
    }
    let txt = fs::read(t.join("Sandbox_EN.txt")).map(|b| super::decode_text(&b)).unwrap_or_default();
    txt.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let v = v.trim().trim_end_matches(',').trim().strip_prefix('"')?.strip_suffix('"')?;
            Some((k.trim().to_string(), v.to_string()))
        })
        .collect()
}

/// The form for these values: settings in file order, and whether each must be a whole number.
fn fields(game: Option<&Path>, sand: &Sand) -> Vec<(Setting, bool)> {
    let metas = game.and_then(|g| game_options(g).ok()).unwrap_or_default();
    let texts = game.map(texts).unwrap_or_default();
    sand.options
        .iter()
        .map(|(key, value)| {
            let section = key.rsplit_once('.').map_or("", |(s, _)| s).to_string();
            let short = key.rsplit('.').next().unwrap_or(key);
            let Some(m) = metas.get(key) else {
                let kind = if matches!(value.as_str(), "true" | "false") { "bool" } else if value.parse::<f64>().is_ok() { "number" } else { "text" };
                return (Setting { key: key.clone(), value: value.clone(), section, kind: kind.into(), ..Default::default() }, false);
            };
            let label_key = m.translation.as_deref().unwrap_or(short);
            let text = |k: String| texts.get(&k).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            let label = text(format!("Sandbox_{label_key}"));
            let tooltip = text(format!("Sandbox_{label_key}_tooltip")).map(|t| t.replace("\\n", " ").replace('\n', " "));
            let description = [label, tooltip].into_iter().flatten().collect::<Vec<_>>().join(": ");
            let values = m.value_translation.as_deref().unwrap_or(label_key);
            let mut s = Setting { key: key.clone(), value: value.clone(), section, description, min: m.min, max: m.max, default: m.default.clone(), ..Default::default() };
            let whole = match m.kind {
                Kind::Bool => {
                    s.kind = "bool".into();
                    false
                }
                Kind::Double => {
                    s.kind = "number".into();
                    false
                }
                Kind::Int => {
                    s.kind = "number".into();
                    true
                }
                Kind::Text => {
                    s.kind = "text".into();
                    false
                }
                Kind::Enum(n) => {
                    let label = |i: u32| text(format!("Sandbox_{values}_option{i}"));
                    let n = if n > 0 { n } else { (1..100).take_while(|i| label(*i).is_some()).count() as u32 };
                    s.choices = (1..=n).map(|i| (i.to_string(), label(i).unwrap_or_else(|| i.to_string()))).collect();
                    // the value must be one of them (an option this game reads differently is left as a number)
                    s.kind = if s.choices.iter().any(|(v, _)| v == value) { "choice" } else { "number" }.into();
                    (s.min, s.max) = (Some(1.0), Some(n as f64));
                    s.default = s.default.and_then(|d| s.choices.iter().find(|(v, _)| *v == d).map(|(_, l)| l.clone()).or(Some(d)));
                    true
                }
            };
            (s, whole)
        })
        .collect()
}

/// A save's sandbox settings as a form, read from its `map_sand.bin`. `game`: the folder with projectzomboid.jar.
pub fn form(game: Option<&Path>, file: &Path) -> Result<Vec<Setting>> {
    let sand = parse(&fs::read(file).with_context(|| format!("reading {}", file.display()))?).with_context(|| format!("PZMM can't read {}", file.display()))?;
    Ok(fields(game, &sand).into_iter().map(|(s, _)| s).collect())
}

/// Change values in a save's `map_sand.bin` (checked against the form, and read back before it's written).
/// The file before stays as `.prev` (before PZMM's first change, `.bak`).
pub fn write(game: Option<&Path>, file: &Path, changes: &[(String, String)]) -> Result<()> {
    let old = fs::read(file).with_context(|| format!("reading {} (nothing was changed)", file.display()))?;
    let mut sand = parse(&old).with_context(|| format!("PZMM can't read {}, so it leaves it as it is", file.display()))?;
    let form = fields(game, &sand);
    for (key, value) in changes {
        let (s, whole) = form.iter().find(|(s, _)| &s.key == key).with_context(|| format!("the save has no setting {key}"))?;
        check_value(s, value)?;
        if *whole && value.trim().parse::<i64>().is_err() {
            bail!("{key} must be a whole number");
        }
        if let Some(v) = sand.options.iter_mut().find(|(k, _)| k == key) {
            v.1 = if s.kind == "text" { value.clone() } else { value.trim().to_string() };
        }
    }
    let out = bytes(&sand)?;
    if parse(&out)? != sand {
        bail!("the changed file didn't read back (nothing was written)");
    }
    keep_prev(file)?;
    crate::write_atomic(file, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sand() -> Sand {
        let o = |k: &str, v: &str| (k.to_string(), v.to_string());
        Sand {
            world_version: 249,
            version: 6,
            options: vec![o("Zombies", "4"), o("ZombieLore.Speed", "2"), o("Doubles", "1.0"), o("Flag", "true"), o("Count", "10"), o("LootItemRemovalList", "Base.Hat, Base.Glasses"), o("MyMod.Speed", "3")],
            preset: Some("Apocalypse".into()),
        }
    }

    #[test]
    fn map_sand_round_trips() {
        let s = sand();
        let b = bytes(&s).unwrap();
        assert_eq!(&b[..16], b"SAND\0\0\0\xf9\0\0\0\x06\0\0\0\x07", "magic, world version 249, sandbox version 6, 7 options");
        assert_eq!(&b[16..25], b"\0\x07Zombies");
        assert_eq!(parse(&b).unwrap(), s);
        let old = Sand { preset: None, ..s.clone() };
        assert_eq!(parse(&bytes(&old).unwrap()).unwrap(), old, "files from before the preset name");
        assert!(parse(&b[..b.len() - 1]).is_err() && parse(b"SANDx").is_err() && parse(&[b.as_slice(), &[0]].concat()).is_err());
    }

    /// A class file with these constants and a constructor running `code` (constants by index).
    struct Class {
        pool: Vec<u8>,
        n: u16,
    }

    impl Class {
        fn add(&mut self, b: &[u8], slots: u16) -> u16 {
            self.pool.extend(b);
            self.n += slots;
            self.n - slots + 1
        }
        fn utf8(&mut self, s: &str) -> u16 {
            self.add(&[&[1], &(s.len() as u16).to_be_bytes()[..], s.as_bytes()].concat(), 1)
        }
        fn class(&mut self, s: &str) -> u16 {
            let u = self.utf8(s).to_be_bytes();
            self.add(&[7, u[0], u[1]], 1)
        }
        fn string(&mut self, s: &str) -> [u8; 2] {
            let u = self.utf8(s).to_be_bytes();
            self.add(&[8, u[0], u[1]], 1).to_be_bytes()
        }
        fn double(&mut self, d: f64) -> [u8; 2] {
            self.add(&[&[6], &d.to_be_bytes()[..]].concat(), 2).to_be_bytes()
        }
        fn method(&mut self, class: &str, name: &str, desc: &str) -> [u8; 2] {
            let (c, n, d) = (self.class(class).to_be_bytes(), self.utf8(name).to_be_bytes(), self.utf8(desc).to_be_bytes());
            let nat = self.add(&[12, n[0], n[1], d[0], d[1]], 1).to_be_bytes();
            self.add(&[10, c[0], c[1], nat[0], nat[1]], 1).to_be_bytes()
        }
        fn file(mut self, code: &[u8]) -> Vec<u8> {
            let (this, init, v, code_attr) = (self.class("zombie/SandboxOptions"), self.utf8("<init>"), self.utf8("()V"), self.utf8("Code"));
            let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 61];
            out.extend((self.n + 1).to_be_bytes());
            out.extend(&self.pool);
            out.extend([0, 0x21]);
            out.extend(this.to_be_bytes());
            out.extend(this.to_be_bytes());
            out.extend([0, 0, 0, 0, 0, 1, 0, 1]);
            out.extend(init.to_be_bytes());
            out.extend(v.to_be_bytes());
            out.extend([0, 1]);
            out.extend(code_attr.to_be_bytes());
            out.extend((12 + code.len() as u32).to_be_bytes());
            out.extend([0, 8, 0, 1]);
            out.extend((code.len() as u32).to_be_bytes());
            out.extend(code);
            out.extend([0, 0, 0, 0, 0, 0]);
            out
        }
    }

    #[test]
    fn options_come_from_the_games_constructor() {
        let mut c = Class { pool: vec![], n: 0 };
        let so = "zombie/SandboxOptions";
        let (speed, zspeed, doubles, flag, count, list, empty) = (c.string("ZombieLore.Speed"), c.string("ZSpeed"), c.string("Doubles"), c.string("Flag"), c.string("Count"), c.string("LootItemRemovalList"), c.string(""));
        let (d0, d4, d1) = (c.double(0.0), c.double(4.0), c.double(1.0));
        let new_enum = c.method(so, "newEnumOption", "(Ljava/lang/String;II)Lzombie/SandboxOptions$EnumSandboxOption;");
        let set_tr = c.method("zombie/SandboxOptions$EnumSandboxOption", "setTranslation", "(Ljava/lang/String;)Lzombie/SandboxOptions$EnumSandboxOption;");
        let new_double = c.method(so, "newDoubleOption", "(Ljava/lang/String;DDD)Lzombie/SandboxOptions$DoubleSandboxOption;");
        let new_bool = c.method(so, "newBooleanOption", "(Ljava/lang/String;Z)Lzombie/SandboxOptions$BooleanSandboxOption;");
        let new_int = c.method(so, "newIntegerOption", "(Ljava/lang/String;III)Lzombie/SandboxOptions$IntegerSandboxOption;");
        let new_string = c.method(so, "newStringOption", "(Ljava/lang/String;Ljava/lang/String;I)Lzombie/SandboxOptions$StringSandboxOption;");
        #[rustfmt::skip]
        let code = [
            &[0x2a, 0x2a, 0x13, speed[0], speed[1], 0x07, 0x05, 0xb6, new_enum[0], new_enum[1], 0x13, zspeed[0], zspeed[1], 0xb6, set_tr[0], set_tr[1], 0x57][..],
            &[0x2a, 0x13, doubles[0], doubles[1], 0x14, d0[0], d0[1], 0x14, d4[0], d4[1], 0x14, d1[0], d1[1], 0xb6, new_double[0], new_double[1], 0x57],
            &[0x2a, 0x13, flag[0], flag[1], 0x04, 0xb6, new_bool[0], new_bool[1], 0x57],
            &[0x2a, 0x13, count[0], count[1], 0x02, 0x11, 0x03, 0xe8, 0x10, 10, 0xb6, new_int[0], new_int[1], 0x57],
            &[0x2a, 0x13, list[0], list[1], 0x13, empty[0], empty[1], 0x10, 1, 0xb6, new_string[0], new_string[1], 0x57, 0xb1],
        ]
        .concat();
        let class = c.file(&code);
        let (pool, inits) = read_class(&class).unwrap();
        let mut out = vec![];
        scan_class(&pool, &inits[0], &mut out);
        let m: HashMap<String, Meta> = out.into_iter().collect();
        let meta = |kind, min, max, default: &str| Meta { kind, min, max, default: Some(default.into()), translation: None, value_translation: None };
        assert_eq!(m["ZombieLore.Speed"], Meta { translation: Some("ZSpeed".into()), ..meta(Kind::Enum(4), Some(1.0), Some(4.0), "2") });
        assert_eq!(m["Doubles"], meta(Kind::Double, Some(0.0), Some(4.0), "1"));
        assert_eq!(m["Flag"], meta(Kind::Bool, None, None, "true"));
        assert_eq!(m["Count"], meta(Kind::Int, Some(-1.0), Some(1000.0), "10"));
        assert_eq!(m["LootItemRemovalList"], meta(Kind::Text, None, None, ""));

        // the form and a write, with the game's texts
        let t = tempfile::tempdir().unwrap();
        let game = t.path();
        let mut z = zip::ZipWriter::new(fs::File::create(game.join("projectzomboid.jar")).unwrap());
        z.start_file("zombie/SandboxOptions.class", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut z, &class).unwrap();
        z.finish().unwrap();
        let tr = game.join("media/lua/shared/Translate/EN");
        fs::create_dir_all(&tr).unwrap();
        fs::write(tr.join("Sandbox.json"), r#"{ "Sandbox_ZSpeed": "Speed", "Sandbox_ZSpeed_tooltip": "How fast zombies move.", "Sandbox_ZSpeed_option1": "Sprinters", "Sandbox_ZSpeed_option2": "Fast Shamblers", "Sandbox_ZSpeed_option3": "Shamblers", "Sandbox_ZSpeed_option4": "Random" }"#).unwrap();
        let file = game.join("map_sand.bin");
        fs::write(&file, bytes(&sand()).unwrap()).unwrap();
        let f = form(Some(game), &file).unwrap();
        let speed = f.iter().find(|s| s.key == "ZombieLore.Speed").unwrap();
        assert_eq!((speed.kind.as_str(), speed.section.as_str(), speed.description.as_str(), speed.default.as_deref()), ("choice", "ZombieLore", "Speed: How fast zombies move.", Some("Fast Shamblers")));
        assert_eq!(speed.choices[0], ("1".to_string(), "Sprinters".to_string()));
        let kinds: Vec<&str> = f.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["number", "choice", "number", "bool", "number", "text", "number"], "Zombies and MyMod.Speed aren't in this game: kinds from their values");
        for bad in [("Count", "1.5"), ("Count", "5000"), ("ZombieLore.Speed", "5"), ("Flag", "1"), ("Nope", "1")] {
            assert!(write(Some(game), &file, &[(bad.0.into(), bad.1.into())]).is_err(), "{bad:?}");
        }
        write(Some(game), &file, &[("ZombieLore.Speed".into(), "1".into()), ("Count".into(), "-1".into()), ("LootItemRemovalList".into(), "Base.Axe".into())]).unwrap();
        let back = parse(&fs::read(&file).unwrap()).unwrap();
        let v = |k: &str| back.options.iter().find(|(n, _)| n == k).unwrap().1.clone();
        assert_eq!((v("ZombieLore.Speed"), v("Count"), v("LootItemRemovalList"), v("Doubles")), ("1".into(), "-1".into(), "Base.Axe".into(), "1.0".into()));
        assert_eq!(back.preset.as_deref(), Some("Apocalypse"));
        assert_eq!(parse(&fs::read(game.join("map_sand.bin.prev")).unwrap()).unwrap(), sand(), "the file before");
    }
}
