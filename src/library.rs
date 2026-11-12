//! Placeholder library for M1. Replaced by real source scanners in M2.

use chrono::{DateTime, Duration, Local};
use iced::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Steam,
    Heroic,
    Lutris,
    Flatpak,
    Emulation,
    Prism,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Steam => "Steam",
            Source::Heroic => "Heroic",
            Source::Lutris => "Lutris",
            Source::Flatpak => "Flatpak",
            Source::Emulation => "Emulation",
            Source::Prism => "Prism Launcher",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Game {
    pub title: String,
    pub source: Source,
    pub minutes_played: u32,
    pub last_played: Option<DateTime<Local>>,
    pub favorite: bool,
    /// Stand-in for colours extracted from art (M3).
    pub ambient: Color,
    pub accent: Color,
}

impl Game {
    pub fn last_played_label(&self) -> String {
        let Some(t) = self.last_played else { return "Never played".into() };
        let days = (Local::now() - t).num_days();
        match days {
            0 => "Played today".into(),
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
}

/// A shelf on the home screen: a named filter + sort over the library.
pub struct ShelfDef {
    pub title: &'static str,
    pub filter: fn(&Game) -> bool,
    pub by_recent: bool,
}

pub const SHELVES: &[ShelfDef] = &[
    ShelfDef { title: "Continue playing", filter: |g| g.last_played.is_some(), by_recent: true },
    ShelfDef { title: "Favourites", filter: |g| g.favorite, by_recent: false },
    ShelfDef { title: "Steam", filter: |g| g.source == Source::Steam, by_recent: false },
    ShelfDef { title: "Heroic", filter: |g| g.source == Source::Heroic, by_recent: false },
    ShelfDef { title: "Lutris and Flatpak", filter: |g| matches!(g.source, Source::Lutris | Source::Flatpak | Source::Prism), by_recent: false },
    ShelfDef { title: "Emulation", filter: |g| g.source == Source::Emulation, by_recent: false },
];

pub struct Shelf {
    pub title: &'static str,
    /// Indices into the library.
    pub games: Vec<usize>,
}

pub fn build_shelves(games: &[Game]) -> Vec<Shelf> {
    SHELVES
        .iter()
        .map(|def| {
            let mut idx: Vec<usize> = (0..games.len()).filter(|&i| (def.filter)(&games[i])).collect();
            if def.by_recent {
                idx.sort_by_key(|&i| std::cmp::Reverse(games[i].last_played));
            } else {
                idx.sort_by(|&a, &b| games[a].title.cmp(&games[b].title));
            }
            Shelf { title: def.title, games: idx }
        })
        .filter(|s| !s.games.is_empty())
        .collect()
}

pub fn fake() -> Vec<Game> {
    let now = Local::now();
    let ago = |d: i64| Some(now - Duration::days(d));
    let c = |hex: u32| Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    #[rustfmt::skip]
    let rows: &[(&str, Source, u32, Option<DateTime<Local>>, bool, u32, u32)] = &[
        ("Hades II",           Source::Steam,     2460, ago(0),   true,  0x5A1E3A, 0xE0654A),
        ("Elden Ring",         Source::Steam,     6120, ago(2),   true,  0x3B3420, 0xD8B65C),
        ("Celeste",            Source::Heroic,     840, ago(5),   false, 0x3A2156, 0xE46FA8),
        ("Balatro",            Source::Steam,     1980, ago(1),   true,  0x1E3A4A, 0xE4483C),
        ("Hollow Knight",      Source::Heroic,    2100, ago(21),  false, 0x1B2638, 0xB7C8E4),
        ("Baldur's Gate 3",    Source::Steam,     9300, ago(40),  true,  0x2E2014, 0xC9943E),
        ("Cyberpunk 2077",     Source::Heroic,    3480, ago(90),  false, 0x3A3A10, 0xF2E640),
        ("Stardew Valley",     Source::Steam,     5400, ago(12),  false, 0x23401F, 0x8CCB5E),
        ("Outer Wilds",        Source::Steam,      960, None,     false, 0x1C2C4A, 0xF09A3E),
        ("Disco Elysium",      Source::Heroic,    1500, ago(200), false, 0x2C3A3A, 0xD9C7A0),
        ("Diablo II",          Source::Lutris,     300, ago(30),  false, 0x3A1010, 0xC8442C),
        ("Battle for Wesnoth", Source::Flatpak,    120, None,     false, 0x2A2E1A, 0xC0A86A),
        ("Minecraft",          Source::Prism,     8800, ago(3),   false, 0x24381C, 0x7BBE4E),
        ("Super Metroid",      Source::Emulation,  420, ago(60),  true,  0x2A1438, 0xE24A6A),
        ("Chrono Trigger",     Source::Emulation,  900, None,     false, 0x1A2A44, 0xE8C25A),
        ("Metroid Prime",      Source::Emulation,  610, ago(8),   false, 0x2A1A10, 0xF07A2A),
    ];
    rows.iter()
        .map(|&(t, s, m, lp, f, a, b)| Game {
            title: t.into(),
            source: s,
            minutes_played: m,
            last_played: lp,
            favorite: f,
            ambient: c(a),
            accent: c(b),
        })
        .collect()
}
