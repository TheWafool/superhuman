//! The library database: one row per (source, game). Dedup happens at load time.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::{Connection, params};

use crate::art::Art;
use crate::library::Source;
use crate::sources::{LaunchSpec, ScanResult};

pub struct Store {
    conn: Connection,
}

pub struct Row {
    pub id: String,
    pub source: Source,
    #[allow(dead_code)]
    pub source_id: String,
    pub title: String,
    pub launch: LaunchSpec,
    pub minutes_played: u32,
    pub last_played: Option<i64>,
    pub steam_appid: Option<u32>,
    pub favorite: bool,
    pub hidden: bool,
}

pub fn default_path() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| crate::sources::home().join(".local/share")).join("superman/library.db")
}

impl Store {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                source_id TEXT NOT NULL,
                title TEXT NOT NULL,
                launch TEXT NOT NULL,
                minutes_played INTEGER NOT NULL DEFAULT 0,
                last_played INTEGER,
                steam_appid INTEGER,
                favorite INTEGER NOT NULL DEFAULT 0,
                hidden INTEGER NOT NULL DEFAULT 0
             );
             DROP TABLE IF EXISTS art;
             CREATE TABLE IF NOT EXISTS art_v2 (
                game_id TEXT PRIMARY KEY,
                cover TEXT, hero TEXT, logo TEXT, backdrop TEXT,
                ambient INTEGER, accent INTEGER,
                fetched_at INTEGER NOT NULL,
                with_key INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS overrides (
                game_id TEXT PRIMARY KEY,
                sgdb_id INTEGER,
                cover_url TEXT, hero_url TEXT, logo_url TEXT
             );
             CREATE TABLE IF NOT EXISTS meta (
                game_id TEXT PRIMARY KEY,
                json TEXT NOT NULL,
                fetched_at INTEGER NOT NULL
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn load(&self) -> rusqlite::Result<Vec<Row>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source, source_id, title, launch, minutes_played, last_played, steam_appid, favorite, hidden FROM games",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, u32>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<u32>>(7)?,
                r.get::<_, bool>(8)?,
                r.get::<_, bool>(9)?,
            ))
        })?;
        Ok(rows
            .flatten()
            .filter_map(|(id, src, source_id, title, launch, mins, last, appid, fav, hidden)| {
                Some(Row {
                    id,
                    source: Source::parse(&src)?,
                    source_id,
                    title,
                    launch: serde_json::from_str(&launch).ok()?,
                    minutes_played: mins,
                    last_played: last,
                    steam_appid: appid,
                    favorite: fav,
                    hidden,
                })
            })
            .collect())
    }

    /// Write scan results. Sources that failed keep their previous rows; user fields are preserved.
    /// Returns true if anything changed.
    pub fn apply(&mut self, results: &[ScanResult]) -> rusqlite::Result<bool> {
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for res in results {
            let Ok(games) = &res.games else { continue };
            let mut keep = HashSet::new();
            for d in games {
                let id = format!("{}:{}", d.source.as_str(), d.source_id);
                changed += tx.execute(
                    "INSERT INTO games (id, source, source_id, title, launch, minutes_played, last_played, steam_appid)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET
                        title = excluded.title, launch = excluded.launch,
                        minutes_played = MAX(minutes_played, excluded.minutes_played),
                        last_played = MAX(COALESCE(last_played, 0), COALESCE(excluded.last_played, 0)),
                        steam_appid = excluded.steam_appid
                     WHERE title IS NOT excluded.title OR launch IS NOT excluded.launch
                        OR minutes_played < excluded.minutes_played
                        OR COALESCE(last_played, 0) < COALESCE(excluded.last_played, 0)
                        OR steam_appid IS NOT excluded.steam_appid",
                    params![
                        id,
                        d.source.as_str(),
                        d.source_id,
                        d.title,
                        serde_json::to_string(&d.launch).unwrap_or_default(),
                        d.minutes_played,
                        d.last_played,
                        d.steam_appid
                    ],
                )?;
                keep.insert(id);
            }
            let existing: Vec<String> = {
                let mut stmt = tx.prepare("SELECT id FROM games WHERE source = ?1")?;
                stmt.query_map([res.source.as_str()], |r| r.get(0))?.flatten().collect()
            };
            for id in existing.into_iter().filter(|id| !keep.contains(id)) {
                changed += tx.execute("DELETE FROM games WHERE id = ?1", [id])?;
            }
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    /// game_id → (art, fetched_at unix, fetched with a SteamGridDB key)
    pub fn load_art(&self) -> rusqlite::Result<HashMap<String, (Art, i64, bool)>> {
        let mut stmt = self.conn.prepare("SELECT game_id, cover, hero, logo, backdrop, ambient, accent, fetched_at, with_key FROM art_v2")?;
        let rows = stmt.query_map([], |r| {
            // A file that was deleted (cache cleared) marks the row stale so it's fetched again.
            let mut missing = false;
            let mut p = |s: Option<String>| {
                let path = s.map(PathBuf::from);
                missing |= path.as_ref().is_some_and(|p| !p.is_file());
                path.filter(|p| p.is_file())
            };
            let art = Art { cover: p(r.get(1)?), hero: p(r.get(2)?), logo: p(r.get(3)?), backdrop: p(r.get(4)?), ambient: r.get(5)?, accent: r.get(6)? };
            let at: i64 = r.get(7)?;
            Ok((r.get::<_, String>(0)?, (art, if missing { 0 } else { at }, r.get(8)?)))
        })?;
        Ok(rows.flatten().collect())
    }

    pub fn save_art(&self, game_id: &str, art: &Art, with_key: bool) -> rusqlite::Result<()> {
        let s = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
        self.conn.execute(
            "INSERT OR REPLACE INTO art_v2 (game_id, cover, hero, logo, backdrop, ambient, accent, fetched_at, with_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![game_id, s(&art.cover), s(&art.hero), s(&art.logo), s(&art.backdrop), art.ambient, art.accent, chrono::Utc::now().timestamp(), with_key],
        )?;
        Ok(())
    }

    pub fn mark_played(&self, id: &str, unix: i64) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE games SET last_played = ?2 WHERE id = ?1", params![id, unix])?;
        Ok(())
    }

    pub fn load_overrides(&self) -> rusqlite::Result<HashMap<String, crate::art::Override>> {
        let mut stmt = self.conn.prepare("SELECT game_id, sgdb_id, cover_url, hero_url, logo_url FROM overrides")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, crate::art::Override { sgdb_id: r.get::<_, Option<i64>>(1)?.map(|v| v as u64), pins: [r.get(2)?, r.get(3)?, r.get(4)?] }))
        })?;
        Ok(rows.flatten().collect())
    }

    pub fn save_override(&self, game_id: &str, o: &crate::art::Override) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO overrides (game_id, sgdb_id, cover_url, hero_url, logo_url) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![game_id, o.sgdb_id.map(|v| v as i64), o.pins[0], o.pins[1], o.pins[2]],
        )?;
        Ok(())
    }

    /// game_id → (details, fetched_at unix)
    pub fn load_meta(&self) -> rusqlite::Result<HashMap<String, (crate::meta::Meta, i64)>> {
        let mut stmt = self.conn.prepare("SELECT game_id, json, fetched_at FROM meta")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        Ok(rows.flatten().filter_map(|(id, j, at)| Some((id, (serde_json::from_str(&j).ok()?, at)))).collect())
    }

    pub fn save_meta(&self, game_id: &str, m: &crate::meta::Meta) -> rusqlite::Result<()> {
        let j = serde_json::to_string(m).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (game_id, json, fetched_at) VALUES (?1, ?2, ?3)",
            params![game_id, j, chrono::Utc::now().timestamp()],
        )?;
        Ok(())
    }

    pub fn set_favorite(&self, ids: &[String], on: bool) -> rusqlite::Result<()> {
        for id in ids {
            self.conn.execute("UPDATE games SET favorite = ?2 WHERE id = ?1", params![id, on])?;
        }
        Ok(())
    }

    pub fn set_hidden(&self, ids: &[String], on: bool) -> rusqlite::Result<()> {
        for id in ids {
            self.conn.execute("UPDATE games SET hidden = ?2 WHERE id = ?1", params![id, on])?;
        }
        Ok(())
    }

    /// Add a tracked session. Launchers that count playtime themselves (Steam) catch up through
    /// the MAX() in `apply`, so this never double counts.
    pub fn add_playtime(&self, id: &str, minutes: u32) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE games SET minutes_played = minutes_played + ?2 WHERE id = ?1", params![id, minutes])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::Discovered;

    fn disc(id: &str, title: &str, mins: u32) -> Discovered {
        Discovered {
            source: Source::Steam,
            source_id: id.into(),
            title: title.into(),
            launch: LaunchSpec::cmd("steam", ["x"]),
            minutes_played: mins,
            last_played: None,
            steam_appid: None,
        }
    }

    #[test]
    fn upsert_remove_and_keep_on_error() {
        let mut s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let ok = |g| vec![ScanResult { source: Source::Steam, games: Ok(g) }];
        assert!(s.apply(&ok(vec![disc("1", "A", 5), disc("2", "B", 0)])).unwrap());
        assert!(!s.apply(&ok(vec![disc("1", "A", 5), disc("2", "B", 0)])).unwrap());
        s.apply(&ok(vec![disc("1", "A", 9)])).unwrap();
        let rows = s.load().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].minutes_played, 9);
        s.apply(&[ScanResult { source: Source::Steam, games: Err("x".into()) }]).unwrap();
        assert_eq!(s.load().unwrap().len(), 1);
    }
}
