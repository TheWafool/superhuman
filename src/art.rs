//! Game art: fetch (Steam CDN, then SteamGridDB), resize to display size, cache on disk,
//! and extract an ambient/accent colour pair for the UI.

use std::path::{Path, PathBuf};
use std::time::Duration;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use serde_json::Value;

use crate::library::hsl;

/// Cover size on disk: enough for the focused cover at 4K.
const COVER: (u32, u32) = (460, 690);
const HERO_MAX_W: u32 = 1920;
const LOGO_MAX: (u32, u32) = (1000, 400);
/// The blurred background is tiny on disk and upscaled with linear filtering.
const BACKDROP: (u32, u32) = (320, 180);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Art {
    pub cover: Option<PathBuf>,
    pub hero: Option<PathBuf>,
    pub logo: Option<PathBuf>,
    pub backdrop: Option<PathBuf>,
    /// 0xRRGGBB
    pub ambient: Option<u32>,
    pub accent: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub game_id: String,
    pub title: String,
    pub steam_appid: Option<u32>,
    pub pick: Override,
}

/// What the player chose on the game page: the right SteamGridDB game, and pinned images.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Override {
    /// `Some(0)` = "none of these": skip SteamGridDB for this game.
    pub sgdb_id: Option<u64>,
    /// Image URLs per slot (cover, hero, logo).
    pub pins: [Option<String>; 3],
}

/// A picture to choose from in the art picker.
#[derive(Debug, Clone)]
pub struct Choice {
    pub url: String,
    pub thumb: String,
}

/// A SteamGridDB game to match against.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: u64,
    pub name: String,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Cover,
    Hero,
    Logo,
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| crate::sources::home().join(".cache")).join("superman/art")
}

fn game_dir(game_id: &str) -> PathBuf {
    let safe: String = game_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    cache_dir().join(safe)
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("superman-launcher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(45))
        .build()
        .unwrap_or_default()
}

/// Fetch and process all art for one game. Never fails: missing slots stay `None`.
pub async fn fetch(http: reqwest::Client, sgdb_key: Option<String>, req: Request) -> Art {
    let mut raw: [Option<Vec<u8>>; 3] = [None, None, None];
    let slots = Slot::ALL;

    for (i, pin) in req.pick.pins.iter().enumerate() {
        if let Some(url) = pin {
            raw[i] = get_bytes(&http, url).await;
        }
    }
    if let Some(appid) = req.steam_appid {
        for (i, &slot) in slots.iter().enumerate() {
            if raw[i].is_none() {
                raw[i] = steam_cdn(&http, appid, slot).await;
            }
        }
    }

    if raw.iter().any(Option::is_none) {
        if let Some(key) = &sgdb_key {
            let sgdb = Sgdb { http: &http, key };
            let id = match req.pick.sgdb_id {
                Some(0) => None,
                Some(id) => Some(id),
                None => sgdb.game_id(&req).await,
            };
            if let Some(id) = id {
                for (i, &slot) in slots.iter().enumerate() {
                    if raw[i].is_none() {
                        raw[i] = sgdb.image(id, slot).await;
                    }
                }
            }
        }
    }

    let dir = game_dir(&req.game_id);
    tokio::task::spawn_blocking(move || process(&dir, raw)).await.unwrap_or_default()
}

async fn get_bytes(http: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let resp = http.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    // Guard against HTML error pages served with 200.
    image::guess_format(&bytes).ok()?;
    Some(bytes.to_vec())
}

async fn steam_cdn(http: &reqwest::Client, appid: u32, slot: Slot) -> Option<Vec<u8>> {
    let files: &[&str] = match slot {
        Slot::Cover => &["library_600x900_2x.jpg", "library_600x900.jpg"],
        Slot::Hero => &["library_hero.jpg"],
        Slot::Logo => &["logo.png"],
    };
    let hosts = [
        "https://shared.steamstatic.com/store_item_assets/steam/apps",
        "https://cdn.cloudflare.steamstatic.com/steam/apps",
    ];
    for f in files {
        for h in hosts {
            if let Some(b) = get_bytes(http, &format!("{h}/{appid}/{f}")).await {
                return Some(b);
            }
        }
    }
    None
}

struct Sgdb<'a> {
    http: &'a reqwest::Client,
    key: &'a str,
}

impl Sgdb<'_> {
    const BASE: &'static str = "https://www.steamgriddb.com/api/v2";

    async fn get(&self, path: &str) -> Option<Value> {
        let resp = self.http.get(format!("{}{path}", Self::BASE)).bearer_auth(self.key).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let v: Value = resp.json().await.ok()?;
        v.get("success").and_then(Value::as_bool).filter(|&s| s)?;
        v.get("data").cloned()
    }

    async fn game_id(&self, req: &Request) -> Option<u64> {
        if let Some(appid) = req.steam_appid {
            if let Some(id) = self.get(&format!("/games/steam/{appid}")).await.and_then(|d| d.get("id")?.as_u64()) {
                return Some(id);
            }
        }
        let term = urlencode(&req.title);
        let results = self.get(&format!("/search/autocomplete/{term}")).await?;
        let list = results.as_array()?;
        // Exact title matches only: a wrong guess is worse than a title card ("Fix match" covers the rest).
        let key = crate::library::match_key(&req.title);
        list.iter()
            .find(|g| g.get("name").and_then(Value::as_str).is_some_and(|n| same_title(n, &key)))
            .and_then(|g| g.get("id")?.as_u64())
    }

    async fn list(&self, id: u64, slot: Slot) -> Vec<Choice> {
        let filters = "types=static&nsfw=false&humor=false";
        let path = match slot {
            Slot::Cover => format!("/grids/game/{id}?dimensions=600x900,660x930&{filters}"),
            Slot::Hero => format!("/heroes/game/{id}?{filters}"),
            Slot::Logo => format!("/logos/game/{id}?{filters}"),
        };
        let Some(list) = self.get(&path).await else { return Vec::new() };
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| {
                let url = i.get("url")?.as_str()?.to_owned();
                let thumb = i.get("thumb").and_then(Value::as_str).unwrap_or(&url).to_owned();
                Some(Choice { url, thumb })
            })
            .collect()
    }

    async fn image(&self, id: u64, slot: Slot) -> Option<Vec<u8>> {
        for c in self.list(id, slot).await.iter().take(3) {
            if let Some(b) = get_bytes(self.http, &c.url).await {
                return Some(b);
            }
        }
        None
    }

    async fn search(&self, term: &str) -> Vec<Candidate> {
        let Some(list) = self.get(&format!("/search/autocomplete/{}", urlencode(term))).await else { return Vec::new() };
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| {
                let year = g.get("release_date").and_then(Value::as_i64).and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|d| {
                    use chrono::Datelike;
                    d.year()
                });
                Some(Candidate { id: g.get("id")?.as_u64()?, name: g.get("name")?.as_str()?.to_owned(), year })
            })
            .collect()
    }
}

impl Slot {
    pub const ALL: [Slot; 3] = [Slot::Cover, Slot::Hero, Slot::Logo];

    pub fn label(self) -> &'static str {
        match self {
            Slot::Cover => "Cover",
            Slot::Hero => "Background",
            Slot::Logo => "Logo",
        }
    }
}

/// Games on SteamGridDB that might be this one.
pub async fn search(http: reqwest::Client, key: String, term: String) -> Vec<Candidate> {
    Sgdb { http: &http, key: &key }.search(&term).await
}

/// Pictures to choose from for one slot, using the player's match if they made one.
pub async fn choices(http: reqwest::Client, key: String, req: Request, slot: Slot) -> Vec<Choice> {
    let sgdb = Sgdb { http: &http, key: &key };
    let id = match req.pick.sgdb_id {
        Some(0) => return Vec::new(),
        Some(id) => Some(id),
        None => sgdb.game_id(&req).await,
    };
    match id {
        Some(id) => sgdb.list(id, slot).await.into_iter().take(24).collect(),
        None => Vec::new(),
    }
}

/// Download a thumbnail into the cache (named by URL hash) and return its path.
pub async fn thumb(http: reqwest::Client, url: String) -> Option<PathBuf> {
    let h = url.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let path = cache_dir().join("thumbs").join(format!("{h:016x}.png"));
    if path.is_file() {
        return Some(path);
    }
    let bytes = get_bytes(&http, &url).await?;
    tokio::task::spawn_blocking(move || {
        let img = image::load_from_memory(&bytes).ok()?;
        let img = if img.width() > 640 { img.resize(640, 640, FilterType::Triangle) } else { img };
        std::fs::create_dir_all(path.parent()?).ok()?;
        img.save_with_format(&path, ImageFormat::Png).ok()?;
        Some(path)
    })
    .await
    .ok()
    .flatten()
}

/// Exact match, ignoring a trailing note like "(Program)" or "(2018)".
fn same_title(name: &str, key: &str) -> bool {
    let base = match name.rfind(" (") {
        Some(i) if name.ends_with(')') => &name[..i],
        _ => name,
    };
    crate::library::match_key(name) == key || crate::library::match_key(base) == key
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn process(dir: &Path, raw: [Option<Vec<u8>>; 3]) -> Art {
    // Fresh names each time: textures are cached by path, so reusing one would show the old image.
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::create_dir_all(dir);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let name = |n: &str, ext: &str| dir.join(format!("{n}-{stamp}.{ext}"));
    let decode = |b: &Option<Vec<u8>>| b.as_ref().and_then(|b| image::load_from_memory(b).ok());
    let (cover, hero, logo) = (decode(&raw[0]), decode(&raw[1]), decode(&raw[2]));
    let mut art = Art::default();

    if let Some(img) = &cover {
        let out = img.resize_to_fill(COVER.0, COVER.1, FilterType::Lanczos3);
        art.cover = save_jpeg(&out, &name("cover", "jpg"));
    }
    if let Some(img) = &hero {
        let out = if img.width() > HERO_MAX_W { img.resize(HERO_MAX_W, u32::MAX, FilterType::Lanczos3) } else { img.clone() };
        let path = name("hero", "png");
        art.hero = fade_bottom(&out).save_with_format(&path, ImageFormat::Png).ok().map(|_| path);
    }
    if let Some(img) = &logo {
        let img = trim_transparent(img);
        let out = if img.width() > LOGO_MAX.0 || img.height() > LOGO_MAX.1 { img.resize(LOGO_MAX.0, LOGO_MAX.1, FilterType::Lanczos3) } else { img };
        let path = name("logo", "png");
        art.logo = out.save_with_format(&path, ImageFormat::Png).ok().map(|_| path);
    }
    if let Some(src) = hero.as_ref().or(cover.as_ref()) {
        let small = src.resize_to_fill(BACKDROP.0, BACKDROP.1, FilterType::Triangle);
        let blurred = DynamicImage::ImageRgba8(image::imageops::blur(&small, 7.0));
        art.backdrop = save_jpeg(&blurred, &name("backdrop", "jpg"));
        let (amb, acc) = palette(src);
        art.ambient = Some(amb);
        art.accent = Some(acc);
    }
    art
}

fn save_jpeg(img: &DynamicImage, path: &Path) -> Option<PathBuf> {
    let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).ok()?);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 90).encode_image(&rgb).ok()?;
    Some(path.to_path_buf())
}

/// Bake a soft fade into the bottom of the hero so it melts into the blurred backdrop
/// (the renderer has no masks).
fn fade_bottom(img: &DynamicImage) -> DynamicImage {
    let mut rgba = img.to_rgba8();
    let h = rgba.height() as f32;
    let start = h * 0.5;
    for (_, y, p) in rgba.enumerate_pixels_mut() {
        let y = y as f32;
        if y > start {
            let t = ((y - start) / (h - start)).clamp(0.0, 1.0);
            let keep = 1.0 - t * t * (3.0 - 2.0 * t); // smoothstep
            p[3] = (p[3] as f32 * keep) as u8;
        }
    }
    DynamicImage::ImageRgba8(rgba)
}

/// Logos often ship with wide transparent margins, which break left alignment.
fn trim_transparent(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in rgba.enumerate_pixels() {
        if p[3] > 8 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x1 < x0 || y1 < y0 {
        return img.clone();
    }
    img.crop_imm(x0, y0, x1 - x0 + 1, y1 - y0 + 1)
}

/// Dominant hue (for the dark ambient wash) and the most vivid hue (accent), as 0xRRGGBB.
pub fn palette(img: &DynamicImage) -> (u32, u32) {
    let small = img.resize_exact(48, 48, FilterType::Triangle);
    // 24 hue buckets: (weight, sum sin, sum cos) for dominance; best vividness for accent.
    let mut weight = [0f32; 24];
    let mut vivid = [0f32; 24];
    let mut grey = 0f32;
    for (_, _, p) in small.pixels() {
        let (h, s, l) = rgb_to_hsl(p[0], p[1], p[2]);
        if s < 0.18 || l < 0.08 || l > 0.95 {
            grey += 1.0;
            continue;
        }
        let b = ((h / 15.0) as usize).min(23);
        weight[b] += s;
        vivid[b] += s * s * (1.0 - (l - 0.55).abs());
    }
    let argmax = |a: &[f32; 24]| a.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1)).map(|(i, &v)| (i, v));
    let to_u32 = |c: iced::Color| ((c.r * 255.0) as u32) << 16 | ((c.g * 255.0) as u32) << 8 | (c.b * 255.0) as u32;
    match (argmax(&weight), argmax(&vivid)) {
        (Some((dom, w)), Some((viv, _))) if w > 0.5 => {
            let dh = dom as f32 * 15.0 + 7.5;
            let vh = viv as f32 * 15.0 + 7.5;
            // Mostly grey art gets a desaturated wash.
            let sat = if grey > 48.0 * 48.0 * 0.8 { 0.18 } else { 0.42 };
            (to_u32(hsl(dh, sat, 0.2)), to_u32(hsl(vh, 0.65, 0.58)))
        }
        _ => (to_u32(hsl(250.0, 0.2, 0.18)), to_u32(hsl(38.0, 0.0, 0.7))),
    }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-4 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    ((h + 360.0) % 360.0, s, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_finds_red() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(10, 10, image::Rgb([200, 30, 30])));
        let (amb, acc) = palette(&img);
        assert!(amb >> 16 > (amb >> 8) & 0xff, "ambient should be reddish: {amb:06x}");
        assert!(acc >> 16 > (acc >> 8) & 0xff, "accent should be reddish: {acc:06x}");
    }

    #[test]
    fn title_notes() {
        assert!(same_title("Sober (Program)", "sober"));
        assert!(!same_title("SoberUp", "sober"));
    }

    #[test]
    fn urlencodes() {
        assert_eq!(urlencode("Baldur's Gate 3"), "Baldur%27s%20Gate%203");
    }
}
