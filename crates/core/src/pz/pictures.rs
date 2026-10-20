//! Pictures for the PZMM - Client mod: the game's getTexture reads PNG only and most Workshop and news pictures
//! are JPEG, so PZMM keeps PNG copies in `Zomboid/Lua/pzmm/img`:
//! - `<item>.png`: a Workshop item's preview (installed mods and What's new entries);
//! - `<item>-2.png`, `<item>-3.png`...: its other Workshop pictures (asked for with the `pics` request);
//! - `news_<n>.png`: the picture of news.txt's post `n` (1 is the newest).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::Paths;

/// Wider pictures are scaled down to this: the game shows them small, and textures cost it memory.
const WIDE: u32 = 512;
/// Downloads per call at most: the rest come on the next (launch, the feed, the news).
const MAX: usize = 40;

/// The game's picture folder, when the bridge folder is there (the mod has run).
pub fn dir(paths: &Paths) -> Option<PathBuf> {
    let d = super::bridge::dir(paths);
    d.is_dir().then(|| d.join("img"))
}

/// `bytes` (JPEG, PNG or GIF) as a PNG at most `WIDE` pixels wide.
pub fn to_png(bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(bytes)?;
    let img = if img.width() > WIDE { img.resize(WIDE, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
    let mut out = std::io::Cursor::new(vec![]);
    img.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// Save each `(name, url)` as `<dir>/<name>.png`, skipping names already saved (`fresh`: saved again) and empty urls;
/// at most `MAX` downloads. A picture that can't be read is logged and left out.
pub fn save(dir: &Path, pics: &[(String, String)], fresh: bool) {
    let mut got = 0;
    for (name, url) in pics {
        let file = dir.join(format!("{name}.png"));
        if url.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') || (!fresh && file.exists()) {
            continue;
        }
        if got >= MAX {
            break;
        }
        got += 1;
        match crate::sources::http_get(url).and_then(|b| to_png(&b)) {
            Ok(png) => {
                let _ = std::fs::create_dir_all(dir);
                super::bridge::write_for_game(&file, png);
            }
            Err(e) => crate::log!("error: picture {url} for the game: {e:#}"),
        }
    }
}

/// [`save`] on its own thread, when the game's picture folder is there (it never holds up what asked).
pub fn save_later(paths: &Paths, pics: Vec<(String, String)>, fresh: bool) {
    if let Some(dir) = dir(paths).filter(|_| !pics.is_empty()) {
        std::thread::spawn(move || save(&dir, &pics, fresh));
    }
}

/// Workshop item `id`'s pictures beyond its preview, as `<id>-2.png`... (up to 8), for the game's details carousel.
/// Only an installed item's (What's new shows installed ones too): any mod can ask. The first call of a launch also
/// clears out the pictures of items no longer installed.
pub fn screenshots(paths: &Paths, id: &str) -> Result<()> {
    let Some(dir) = dir(paths) else { return Ok(()) };
    let installed: HashSet<String> = crate::store::rows(paths)?.values().filter_map(|r| crate::sources::workshop_item(&r.source).map(String::from)).collect();
    static PRUNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !PRUNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        prune(&dir, &installed);
    }
    anyhow::ensure!(installed.contains(id), "{id} isn't installed");
    let page = crate::sources::steam_workshop::item_page(id)?;
    let pics: Vec<(String, String)> = page.screenshots.into_iter().take(7).enumerate().map(|(i, url)| (format!("{id}-{}", i + 2), url)).collect();
    std::thread::spawn(move || save(&dir, &pics, false));
    Ok(())
}

/// Delete the `<item>-<n>.png` pictures of items not in `installed` (previews, `<item>.png`, stay).
fn prune(dir: &Path, installed: &HashSet<String>) {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.strip_suffix(".png").and_then(|s| s.split_once('-')).is_some_and(|(item, n)| digits(item) && digits(n) && !installed.contains(item)) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_becomes_a_png_no_wider_than_the_game_needs() {
        let mut jpg = std::io::Cursor::new(vec![]);
        image::RgbImage::from_pixel(1024, 300, image::Rgb([200, 30, 30])).write_to(&mut jpg, image::ImageFormat::Jpeg).unwrap();
        let png = to_png(&jpg.into_inner()).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let back = image::load_from_memory(&png).unwrap();
        assert_eq!((back.width(), back.height()), (512, 150));
        assert!(to_png(b"not a picture").is_err());
    }

    #[test]
    fn saves_once_and_only_safe_names() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("123.png"), "kept").unwrap();
        let (base, seen) = crate::sources::fake_http(vec![("GET /p", 200, "not a picture".into())]);
        let pics: Vec<(String, String)> = [("123", "/p"), ("../evil", "/p"), ("456", ""), ("789", "/p")].iter().map(|(n, u)| (n.to_string(), if u.is_empty() { String::new() } else { format!("{base}{u}") })).collect();
        save(t.path(), &pics, false);
        assert_eq!(seen.lock().unwrap().len(), 1, "only 789: 123 is saved, the unsafe name and the empty url never asked for");
        assert!(!t.path().join("789.png").exists(), "what isn't a picture isn't saved");
        save(t.path(), &pics[..1], true);
        assert_eq!(seen.lock().unwrap().len(), 2, "fresh: asked again");
    }

    #[test]
    fn prunes_only_the_pictures_of_items_gone() {
        let t = tempfile::tempdir().unwrap();
        for f in ["1.png", "1-2.png", "2.png", "2-3.png", "news_1.png", "x-2.png"] {
            std::fs::write(t.path().join(f), "").unwrap();
        }
        prune(t.path(), &HashSet::from(["1".to_string()]));
        let mut left: Vec<String> = std::fs::read_dir(t.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["1-2.png", "1.png", "2.png", "news_1.png", "x-2.png"]);
    }
}
