//! The library database: one row per (source, game). Dedup happens at load time.

use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::{Connection, params};

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

    pub fn mark_played(&self, id: &str, unix: i64) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE games SET last_played = ?2 WHERE id = ?1", params![id, unix])?;
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
