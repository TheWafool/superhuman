//! Game model, cross-source dedup, and home-screen shelves.

use std::collections::HashMap;

use chrono::{DateTime, Local, TimeZone};
use iced::Color;

use crate::sources::LaunchSpec;
use crate::store::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Steam,
    Heroic,
    Lutris,
    Flatpak,
    Desktop,
    Emulation,
    Prism,
    Bottles,
    Itch,
    Epic,
    Gog,
    Xbox,
    Ubisoft,
    Ea,
    BattleNet,
    Amazon,
    /// Added by hand in `games.toml`.
    Manual,
}

impl Source {
    /// Every source, in the order the settings list them.
    pub const ALL: &[Source] = &[
        Source::Steam, Source::Heroic, Source::Lutris, Source::Epic, Source::Gog, Source::Xbox, Source::Ubisoft, Source::Ea,
        Source::BattleNet, Source::Amazon, Source::Itch, Source::Bottles, Source::Prism, Source::Emulation, Source::Manual,
        Source::Flatpak, Source::Desktop,
    ];

    /// Which copy wins when the same game is found in several places (lower = preferred).
    fn priority(self) -> u8 {
        match self {
            Source::Steam => 0,
            Source::Heroic => 1,
            Source::Lutris => 2,
            Source::Manual => 3,
            Source::Itch => 4,
            Source::Epic => 10,
            Source::Gog => 11,
            Source::Xbox => 12,
            Source::Ubisoft => 13,
            Source::Ea => 14,
            Source::BattleNet => 15,
            Source::Amazon => 16,
            Source::Bottles => 5,
            Source::Prism => 6,
            Source::Emulation => 7,
            Source::Flatpak => 8,
            Source::Desktop => 9,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Steam => "Steam",
            Source::Heroic => "Heroic",
            Source::Lutris => "Lutris",
            Source::Flatpak => "Flatpak",
            Source::Desktop => "your system",
            Source::Emulation => "Emulation",
            Source::Prism => "Prism Launcher",
            Source::Bottles => "Bottles",
            Source::Itch => "itch",
            Source::Epic => "Epic Games",
            Source::Gog => "GOG",
            Source::Xbox => "Xbox",
            Source::Ubisoft => "Ubisoft Connect",
            Source::Ea => "EA app",
            Source::BattleNet => "Battle.net",
            Source::Amazon => "Amazon Games",
            Source::Manual => "your games list",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Source::Steam => "steam",
            Source::Heroic => "heroic",
            Source::Lutris => "lutris",
            Source::Flatpak => "flatpak",
            Source::Desktop => "desktop",
            Source::Emulation => "emulation",
            Source::Prism => "prism",
            Source::Bottles => "bottles",
            Source::Itch => "itch",
            Source::Epic => "epic",
            Source::Gog => "gog",
            Source::Xbox => "xbox",
            Source::Ubisoft => "ubisoft",
            Source::Ea => "ea",
            Source::BattleNet => "battlenet",
            Source::Amazon => "amazon",
            Source::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "steam" => Source::Steam,
            "heroic" => Source::Heroic,
            "lutris" => Source::Lutris,
            "flatpak" => Source::Flatpak,
            "desktop" => Source::Desktop,
            "emulation" => Source::Emulation,
            "prism" => Source::Prism,
            "bottles" => Source::Bottles,
            "itch" => Source::Itch,
            "epic" => Source::Epic,
            "gog" => Source::Gog,
            "xbox" => Source::Xbox,
            "ubisoft" => Source::Ubisoft,
            "ea" => Source::Ea,
            "battlenet" => Source::BattleNet,
            "amazon" => Source::Amazon,
            "manual" => Source::Manual,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Game {
    /// Id of the preferred copy (`source:source_id`).
    pub id: String,
    pub title: String,
    pub source: Source,
    pub launch: LaunchSpec,
    /// Other launchers that also have this game.
    pub also_on: Vec<Source>,
    /// Every DB row merged into this card (for favourite/hide on all copies).
    pub copies: Vec<String>,
    pub minutes_played: u32,
    pub last_played: Option<DateTime<Local>>,
    pub favorite: bool,
    pub steam_appid: Option<u32>,
    /// Stand-in for colours extracted from art (M3).
    pub ambient: Color,
    pub accent: Color,
}

impl Game {
    pub fn last_played_label(&self) -> String {
        let Some(t) = self.last_played else { return "Never played".into() };
        let days = (Local::now() - t).num_days();
        match days {
            ..=0 => "Played today".into(),
            1 => "Played yesterday".into(),
            d if d < 14 => format!("Played {d} days ago"),
            d if d < 60 => format!("Played {} weeks ago", d / 7),
            _ => format!("Played {}", t.format("%B %Y")),
        }
    }

    pub fn playtime_label(&self) -> Option<String> {
        match self.minutes_played {
            0 => None,
            m if m < 60 => Some(format!("{m} minutes")),
            m if m < 120 => Some("1 hour".into()),
            m => Some(format!("{} hours", m / 60)),
        }
    }

    pub fn sources_label(&self) -> String {
        let mut s = format!("via {}", self.source.label());
        if !self.also_on.is_empty() {
            let rest: Vec<_> = self.also_on.iter().map(|s| s.label()).collect();
            s.push_str(&format!(", also on {}", rest.join(" and ")));
        }
        s
    }
}

/// Normalised title for matching the same game across launchers.
pub fn match_key(title: &str) -> String {
    title
        .to_lowercase()
        .replace(['™', '®', '©'], "")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Colours derived from the title until real art lands (M3).
fn placeholder_colors(title: &str) -> (Color, Color) {
    let h = title.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let hue = (h % 360) as f32;
    (hsl(hue, 0.45, 0.22), hsl((hue + 25.0) % 360.0, 0.62, 0.55))
}

pub fn hsl(h: f32, s: f32, l: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color::from_rgb(r + m, g + m, b + m)
}

/// Merge DB rows into one game per title, preferring the higher-priority source.
pub fn from_rows(mut rows: Vec<Row>) -> Vec<Game> {
    rows.retain(|r| !r.hidden);
    rows.sort_by_key(|r| r.source.priority());
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut games: Vec<Game> = Vec::new();
    for r in rows {
        let key = match r.steam_appid {
            Some(id) => format!("steam{id}"),
            None => match_key(&r.title),
        };
        let alt_key = match_key(&r.title);
        let last = r.last_played.and_then(|t| Local.timestamp_opt(t, 0).single());
        if let Some(&i) = by_key.get(&key).or_else(|| by_key.get(&alt_key)) {
            let g = &mut games[i];
            if g.source != r.source && !g.also_on.contains(&r.source) {
                g.also_on.push(r.source);
            }
            g.minutes_played = g.minutes_played.max(r.minutes_played);
            g.last_played = g.last_played.max(last);
            g.favorite |= r.favorite;
            g.copies.push(r.id);
            continue;
        }
        let (ambient, accent) = placeholder_colors(&r.title);
        by_key.insert(key, games.len());
        by_key.insert(alt_key, games.len());
        games.push(Game {
            copies: vec![r.id.clone()],
            id: r.id,
            title: r.title,
            source: r.source,
            launch: r.launch,
            also_on: Vec::new(),
            minutes_played: r.minutes_played,
            last_played: last,
            favorite: r.favorite,
            steam_appid: r.steam_appid,
            ambient,
            accent,
        });
    }
    games
}

/// A shelf on the home screen: a named filter + sort over the library.
pub struct ShelfDef {
    pub title: &'static str,
    pub filter: fn(&Game) -> bool,
    pub by_recent: bool,
    pub limit: usize,
}

pub const SHELVES: &[ShelfDef] = &[
    ShelfDef { title: "Continue playing", filter: |g| g.last_played.is_some(), by_recent: true, limit: 12 },
    ShelfDef { title: "Favourites", filter: |g| g.favorite, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Steam", filter: |g| g.source == Source::Steam, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Heroic", filter: |g| g.source == Source::Heroic, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Lutris", filter: |g| g.source == Source::Lutris, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "itch", filter: |g| g.source == Source::Itch, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Epic Games", filter: |g| g.source == Source::Epic, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "GOG", filter: |g| g.source == Source::Gog, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Xbox", filter: |g| g.source == Source::Xbox, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Ubisoft Connect", filter: |g| g.source == Source::Ubisoft, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "EA app", filter: |g| g.source == Source::Ea, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Battle.net", filter: |g| g.source == Source::BattleNet, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Amazon Games", filter: |g| g.source == Source::Amazon, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Bottles", filter: |g| g.source == Source::Bottles, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Added by you", filter: |g| g.source == Source::Manual, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Minecraft instances", filter: |g| g.source == Source::Prism, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Emulation", filter: |g| g.source == Source::Emulation, by_recent: false, limit: usize::MAX },
    ShelfDef { title: "Flatpak and system games", filter: |g| matches!(g.source, Source::Flatpak | Source::Desktop), by_recent: false, limit: usize::MAX },
    ShelfDef { title: "All games", filter: |_| true, by_recent: false, limit: usize::MAX },
];

/// Filter tabs on the search screen (LB/RB). Tabs with no games are skipped.
pub struct FilterDef {
    pub label: &'static str,
    pub filter: fn(&Game) -> bool,
}

pub const FILTERS: &[FilterDef] = &[
    FilterDef { label: "All games", filter: |_| true },
    FilterDef { label: "Favourites", filter: |g| g.favorite },
    FilterDef { label: "Steam", filter: |g| g.source == Source::Steam },
    FilterDef { label: "Heroic", filter: |g| g.source == Source::Heroic },
    FilterDef { label: "Lutris", filter: |g| g.source == Source::Lutris },
    FilterDef { label: "itch", filter: |g| g.source == Source::Itch },
    FilterDef { label: "Epic Games", filter: |g| g.source == Source::Epic },
    FilterDef { label: "GOG", filter: |g| g.source == Source::Gog },
    FilterDef { label: "Xbox", filter: |g| g.source == Source::Xbox },
    FilterDef { label: "Ubisoft Connect", filter: |g| g.source == Source::Ubisoft },
    FilterDef { label: "EA app", filter: |g| g.source == Source::Ea },
    FilterDef { label: "Battle.net", filter: |g| g.source == Source::BattleNet },
    FilterDef { label: "Amazon Games", filter: |g| g.source == Source::Amazon },
    FilterDef { label: "Bottles", filter: |g| g.source == Source::Bottles },
    FilterDef { label: "Added by you", filter: |g| g.source == Source::Manual },
    FilterDef { label: "Minecraft", filter: |g| g.source == Source::Prism },
    FilterDef { label: "Emulation", filter: |g| g.source == Source::Emulation },
    FilterDef { label: "Flatpak and system", filter: |g| matches!(g.source, Source::Flatpak | Source::Desktop) },
];

/// Games matching `query` within a filter, best first. Empty query lists all, by title.
pub fn search(games: &[Game], query: &str, filter: &FilterDef) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    let qk = match_key(&q);
    let mut hits: Vec<(i32, usize)> = (0..games.len())
        .filter(|&i| (filter.filter)(&games[i]))
        .filter_map(|i| {
            if q.is_empty() {
                return Some((0, i));
            }
            let t = games[i].title.to_lowercase();
            if let Some(pos) = t.find(&q) {
                // Whole-title prefix beats word start beats anywhere.
                let word_start = pos == 0 || !t.as_bytes()[pos - 1].is_ascii_alphanumeric();
                return Some((if pos == 0 { 300 } else if word_start { 200 } else { 100 }, i));
            }
            // Letters in order ("hk" finds Hollow Knight).
            let mut it = match_key(&t).chars().collect::<Vec<_>>().into_iter();
            qk.chars().all(|c| it.any(|x| x == c)).then_some((10, i))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| games[a.1].title.to_lowercase().cmp(&games[b.1].title.to_lowercase())));
    hits.into_iter().map(|h| h.1).collect()
}

pub struct Shelf {
    pub title: &'static str,
    /// Indices into the library.
    pub games: Vec<usize>,
}

pub fn build_shelves(games: &[Game]) -> Vec<Shelf> {
    let mut out: Vec<Shelf> = Vec::new();
    for def in SHELVES {
        let mut idx: Vec<usize> = (0..games.len()).filter(|&i| (def.filter)(&games[i])).collect();
        if def.by_recent {
            idx.sort_by_key(|&i| std::cmp::Reverse(games[i].last_played));
        } else {
            idx.sort_by_cached_key(|&i| games[i].title.to_lowercase());
        }
        idx.truncate(def.limit);
        // Skip empty shelves and ones identical to an earlier shelf (e.g. "All games" with one source).
        let mut sorted = idx.clone();
        sorted.sort_unstable();
        let dup = out.iter().any(|s| {
            let mut o = s.games.clone();
            o.sort_unstable();
            o == sorted
        });
        if !idx.is_empty() && !dup {
            out.push(Shelf { title: def.title, games: idx });
        }
    }
    out
}

/// Placeholder library for screenshots and UI work: `SUPERHUMAN_DEMO=1`.
pub fn demo() -> Vec<Game> {
    let now = Local::now().timestamp();
    let day = 86_400;
    #[rustfmt::skip]
    let rows: &[(&str, Source, u32, Option<i64>, bool)] = &[
        ("Hades II", Source::Steam, 2460, Some(0), true),
        ("Elden Ring", Source::Steam, 6120, Some(2), true),
        ("Celeste", Source::Heroic, 840, Some(5), false),
        ("Balatro", Source::Steam, 1980, Some(1), true),
        ("Hollow Knight", Source::Heroic, 2100, Some(21), false),
        ("Baldur's Gate 3", Source::Steam, 9300, Some(40), true),
        ("Stardew Valley", Source::Steam, 5400, Some(12), false),
        ("Outer Wilds", Source::Steam, 960, None, false),
        ("Diablo II", Source::Lutris, 300, Some(30), false),
        ("Battle for Wesnoth", Source::Flatpak, 120, None, false),
        ("Minecraft", Source::Prism, 8800, Some(3), false),
        ("Super Metroid", Source::Emulation, 420, Some(60), true),
        ("Metroid Prime", Source::Emulation, 610, Some(8), false),
    ];
    let rows = rows
        .iter()
        .enumerate()
        .map(|(i, &(t, s, m, d, f))| Row {
            id: format!("demo:{i}"),
            source: s,
            source_id: i.to_string(),
            title: t.into(),
            launch: LaunchSpec::cmd("sleep", ["600"]),
            minutes_played: m,
            last_played: d.map(|d| now - d * day),
            steam_appid: None,
            favorite: f,
            hidden: false,
        })
        .collect();
    from_rows(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(src: Source, title: &str, appid: Option<u32>) -> Row {
        Row {
            id: format!("{}:{title}", src.as_str()),
            source: src,
            source_id: title.into(),
            title: title.into(),
            launch: LaunchSpec::cmd("x", Vec::<String>::new()),
            minutes_played: 0,
            last_played: None,
            steam_appid: appid,
            favorite: false,
            hidden: false,
        }
    }

    #[test]
    fn searches() {
        let games = from_rows(vec![row(Source::Steam, "Hollow Knight", None), row(Source::Steam, "Knights of Pen", None), row(Source::Lutris, "Diablo", None)]);
        let r: Vec<_> = search(&games, "kni", &FILTERS[0]).iter().map(|&i| games[i].title.clone()).collect();
        assert_eq!(r, vec!["Knights of Pen", "Hollow Knight"]);
        assert_eq!(search(&games, "hk", &FILTERS[0]).len(), 1);
        assert_eq!(search(&games, "", &FILTERS[4]).len(), 1);
    }

    #[test]
    fn dedups_across_sources() {
        let games = from_rows(vec![
            row(Source::Lutris, "Hollow Knight™", None),
            row(Source::Steam, "Hollow Knight", Some(367520)),
            row(Source::Desktop, "Wesnoth", None),
        ]);
        assert_eq!(games.len(), 2);
        let hk = games.iter().find(|g| g.title == "Hollow Knight").unwrap();
        assert_eq!(hk.source, Source::Steam);
        assert_eq!(hk.also_on, vec![Source::Lutris]);
    }
}
