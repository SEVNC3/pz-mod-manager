//! Project Zomboid's news: The Indie Stone's own Steam announcements (patches, builds), read
//! from Steam's news API (no key needed) and kept an hour in PZMM's data folder.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::Paths;
use crate::sources::{self, steam_workshop};
use crate::store;

/// The official feed only: press posts and fan sites share the app's news otherwise.
const FEED: &str = "steam_community_announcements";
const URL: &str = "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid=108600&count=10&maxlength=0&format=json&feeds=steam_community_announcements";
const KEEP: u64 = 3600;
/// An excerpt's length at most, in characters.
const EXCERPT: usize = 220;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Post {
    pub title: String,
    /// Unix seconds.
    pub date: u64,
    /// The start of the post as plain text.
    pub excerpt: String,
    /// The build its title names ("42.21"), or "".
    pub build: String,
    /// The full post on Steam.
    pub url: String,
    /// The post's first image, or "".
    #[serde(default)]
    pub image: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct News {
    /// When Steam was last asked (Unix seconds).
    pub fetched: u64,
    /// Newest first.
    pub posts: Vec<Post>,
    /// Why this is an older copy (Steam failed or refused); "" when it's fresh.
    #[serde(default, skip_deserializing)]
    pub error: String,
}

fn file(paths: &Paths) -> PathBuf {
    paths.data.join("news.json")
}

/// The latest official posts: the kept copy when it's under an hour old, else Steam's. When Steam
/// fails, the kept copy (however old) with the reason; with none kept, the error.
pub fn news(paths: &Paths) -> Result<News> {
    let kept: Option<News> = std::fs::read_to_string(file(paths)).ok().and_then(|t| serde_json::from_str(&t).ok());
    let now = store::now();
    if let Some(n) = kept.as_ref().filter(|n| now.saturating_sub(n.fetched) < KEEP) {
        return Ok(News { fetched: n.fetched, posts: n.posts.clone(), error: String::new() });
    }
    let fresh = sources::http_get(URL).and_then(|b| parse(&String::from_utf8_lossy(&b)));
    match (fresh, kept) {
        (Ok(posts), _) => {
            let n = News { fetched: now, posts, error: String::new() };
            std::fs::create_dir_all(&paths.data)?;
            crate::write_atomic(&file(paths), serde_json::to_string(&n)?)?;
            Ok(n)
        }
        (Err(e), Some(old)) if !old.posts.is_empty() => Ok(News { error: format!("{e:#}"), ..old }),
        (Err(e), _) => Err(e.context("reading Project Zomboid's news from Steam")),
    }
}

#[derive(Deserialize)]
struct Reply {
    appnews: AppNews,
}

#[derive(Deserialize)]
struct AppNews {
    #[serde(default)]
    newsitems: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    gid: String,
    title: String,
    #[serde(default)]
    contents: String,
    date: u64,
    #[serde(default)]
    feedname: String,
}

fn parse(json: &str) -> Result<Vec<Post>> {
    let reply: Reply = serde_json::from_str(json)?;
    Ok(reply
        .appnews
        .newsitems
        .into_iter()
        .filter(|i| i.feedname == FEED)
        .map(|i| Post {
            build: build_in(&i.title),
            image: first_image(&i.contents),
            excerpt: excerpt(&i.contents),
            url: format!("https://store.steampowered.com/news/app/{}/view/{}", super::STEAM_APP, i.gid),
            title: i.title.trim().to_string(),
            date: i.date,
        })
        .collect())
}

/// The first build number in a title: "42.21", "42.20.4", "41.78.16" ("" when none).
fn build_in(title: &str) -> String {
    title
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|w| w.trim_matches('.'))
        .find(|w| {
            let parts: Vec<&str> = w.split('.').collect();
            parts.len() >= 2 && matches!(parts[0], "41" | "42" | "43") && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
        .unwrap_or_default()
        .to_string()
}

/// The first image in a post: Steam's clan image placeholder, a BBCode `[img]` or an HTML `<img>`
/// (https only), as a URL ("" when there's none).
fn first_image(contents: &str) -> String {
    let end = |s: &str| s.find(|c: char| c.is_whitespace() || "[]<>\"'".contains(c)).unwrap_or(s.len());
    let clan = contents.find("{STEAM_CLAN_IMAGE}").map(|i| {
        let path = &contents[i + "{STEAM_CLAN_IMAGE}".len()..];
        (i, format!("https://clan.akamai.steamstatic.com/images{}", &path[..end(path)]))
    });
    let tagged = ["[img]", "<img src=\""].iter().filter_map(|tag| {
        let i = contents.find(tag)?;
        let url = &contents[i + tag.len()..];
        url.starts_with("https://").then(|| (i, url[..end(url)].to_string()))
    });
    clan.into_iter().chain(tagged).min_by_key(|(i, _)| *i).map(|(_, u)| u).unwrap_or_default()
}

/// A post's start as plain text: HTML and BBCode tags, image placeholders and bare links go, and it's
/// cut at a word to [`EXCERPT`] characters.
fn excerpt(contents: &str) -> String {
    let text = steam_workshop::html_text(contents);
    let mut out = String::new();
    let mut tag: Option<String> = None;
    for c in text.chars() {
        match (&mut tag, c) {
            (None, '[') => tag = Some(String::new()),
            (Some(t), ']') => {
                // a block ([p], [h2], [*], [list]...) ends a word; [b] and [url] don't
                let name: String = t.trim_start_matches('/').chars().take_while(char::is_ascii_alphanumeric).collect();
                if matches!(name.as_str(), "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "list" | "olist" | "" | "hr" | "br" | "img" | "table" | "tr" | "td" | "th" | "quote" | "code") {
                    out.push(' ');
                }
                tag = None;
            }
            // a tag has no spaces ([url=...] runs long); a "[" followed by a space is text
            (Some(t), c) if !c.is_whitespace() && t.len() < 400 => t.push(c),
            // a lone "[" in the text, not a tag
            (Some(_), c) => {
                out.push('[');
                out.push_str(&tag.take().unwrap_or_default());
                out.push(c);
            }
            (None, c) => out.push(c),
        }
    }
    if let Some(t) = tag {
        out.push('[');
        out.push_str(&t);
    }
    let words: Vec<&str> = out.split_whitespace().filter(|w| !w.starts_with("{STEAM_") && !w.starts_with("http")).collect();
    let mut s = words.join(" ");
    s = s.trim_end_matches("...").trim_end().to_string();
    if s.chars().count() > EXCERPT {
        let cut: String = s.chars().take(EXCERPT).collect();
        s = format!("{}…", cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a).trim_end_matches([',', '.', ';', ':']));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_official_posts_as_plain_text() {
        let json = r#"{"appnews":{"appid":108600,"count":3,"newsitems":[
            {"gid":"11","title":"Build 42.21 Stable Released","url":"x","contents":"{STEAM_CLAN_IMAGE}/4460946/3ab5.png Hello [b]Survivors[/b]![p]See <a href=\"https://x\">notes</a> &amp; more https://example.com/a ...","date":1790597228,"feedname":"steam_community_announcements"},
            {"gid":"12","title":"Project Zomboid mod fills the world","contents":"press","date":1790353587,"feedname":"PCGamesN"},
            {"gid":"13","title":" 42.20.4 STABLE & 42.19.2 UNSTABLE & 41.78.21 LEGACY Hotfixes Released","contents":"","date":1787742746,"feedname":"steam_community_announcements","tags":["patchnotes"]}
        ]}}"#;
        let posts = parse(json).unwrap();
        assert_eq!(posts.len(), 2, "press posts are left out");
        assert_eq!(posts[0].excerpt, "Hello Survivors! See notes & more");
        assert_eq!(posts[0].build, "42.21");
        assert_eq!(posts[0].image, "https://clan.akamai.steamstatic.com/images/4460946/3ab5.png");
        assert_eq!(posts[1].image, "");
        assert_eq!(first_image("text [img]https://cdn.example/a.jpg[/img] then <img src=\"https://x/b.png\">"), "https://cdn.example/a.jpg");
        assert_eq!(first_image("[img]http://insecure/a.jpg[/img]"), "");
        assert_eq!(posts[0].url, "https://store.steampowered.com/news/app/108600/view/11");
        assert_eq!(posts[1].build, "42.20.4");
        assert_eq!(posts[1].title, "42.20.4 STABLE & 42.19.2 UNSTABLE & 41.78.21 LEGACY Hotfixes Released");
        assert_eq!(build_in("Show your Spiffo's pride, and help us fundraise"), "");
        assert_eq!(build_in("Re-population of the Dead: Build 42.21 Unstable Released"), "42.21");
        assert_eq!(build_in("B42 CHECKLIST"), "");

        assert_eq!(excerpt("[list][*]one[*]two[/list] a [ lone bracket that never closes, and on"), "one two a [ lone bracket that never closes, and on");
        assert_eq!(excerpt("We released 42.21 into our [url=https://projectzomboid.com/blog/news/2026/09/re-population-of-the-dead-build-42-21-unstable-released/]Unstable beta[/url] last week"), "We released 42.21 into our Unstable beta last week");
        let long = excerpt(&"word ".repeat(100));
        assert!(long.ends_with("word…") && long.chars().count() <= EXCERPT + 1, "{long}");
    }

    #[test]
    fn serves_the_kept_copy_within_the_hour() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths { zomboid: dir.path().join("Zomboid"), data: dir.path().join("data") };
        let kept = News { fetched: store::now(), posts: vec![Post { title: "Build 42.21".into(), ..Default::default() }], error: String::new() };
        std::fs::create_dir_all(&paths.data).unwrap();
        std::fs::write(file(&paths), serde_json::to_string(&kept).unwrap()).unwrap();
        let n = news(&paths).unwrap();
        assert_eq!(n.posts, kept.posts);
        assert!(n.error.is_empty());
    }
}
