//! itch app: installed games ("caves") from butler's database, opened read-only.
//! Works on Linux, Windows and macOS. Games start directly from their install folder.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::{Discovered, LaunchSpec, Places};
use crate::library::Source;

/// Launch target flavours butler reports, best first, for this system.
const FLAVORS: &[&str] = if cfg!(windows) {
    &["windows", "windows-script"]
} else if cfg!(target_os = "macos") {
    &["app-macos", "macos", "script"]
} else {
    &["linux", "script"]
};

pub fn scan() -> Result<Vec<Discovered>, String> {
    let places = Places { config: &["itch"], data: &[], flatpak: Some("io.itch.itch") };
    let mut out = Vec::new();
    for (root, _) in places.existing() {
        let db = root.join("db/butler.db");
        if db.is_file() {
            out.extend(read_db(&db)?);
        }
    }
    Ok(out)
}

fn read_db(db: &Path) -> Result<Vec<Discovered>, String> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("{}: {e}", db.display()))?;
    let mut stmt = conn
        .prepare("SELECT caves.id, games.title, caves.seconds_run, caves.last_touched_at, caves.verdict FROM caves JOIN games ON games.id = caves.game_id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows
        .flatten()
        .filter_map(|(id, title, secs, touched, verdict)| {
            Some(Discovered {
                source: Source::Itch,
                source_id: id,
                title: title?,
                launch: launch(&serde_json::from_str(&verdict?).ok()?)?,
                minutes_played: (secs.unwrap_or(0) / 60) as u32,
                last_played: touched.as_deref().and_then(parse_time),
                steam_appid: None,
            })
        })
        .collect())
}

/// Pick the best launch target from butler's verdict for this system.
fn launch(verdict: &Value) -> Option<LaunchSpec> {
    let base = PathBuf::from(verdict.get("basePath")?.as_str()?);
    let cands = verdict.get("candidates")?.as_array()?;
    let (flavor, path) = FLAVORS.iter().find_map(|f| {
        cands.iter().find(|c| c.get("flavor").and_then(Value::as_str) == Some(f)).and_then(|c| Some((*f, c.get("path")?.as_str()?)))
    })?;
    let full = base.join(path);
    let dir = full.parent().unwrap_or(&base).to_string_lossy().into_owned();
    let spec = match flavor {
        "app-macos" => LaunchSpec::cmd("open", ["-W".to_owned(), full.to_string_lossy().into_owned()]),
        "script" => LaunchSpec::cmd("sh", [full.to_string_lossy().into_owned()]),
        _ => LaunchSpec::cmd(full.to_string_lossy(), Vec::<String>::new()),
    };
    Some(spec.in_dir(dir))
}

/// butler stores times as `YYYY-MM-DD HH:MM:SS…` in UTC (sometimes RFC 3339).
fn parse_time(s: &str) -> Option<i64> {
    let n: Vec<u32> = s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).take(6).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let [y, mo, d, h, mi, se] = n[..] else { return None };
    let t = chrono::NaiveDate::from_ymd_opt(y as i32, mo, d)?.and_hms_opt(h, mi, se)?;
    Some(t.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_launch_target() {
        let v: Value = serde_json::from_str(
            r#"{"basePath":"/games/celeste","candidates":[{"path":"Celeste.exe","flavor":"windows"},{"path":"bin/Celeste","flavor":"linux"}]}"#,
        )
        .unwrap();
        let LaunchSpec::Command { program, cwd, .. } = launch(&v).unwrap();
        if cfg!(target_os = "linux") {
            assert_eq!(program, "/games/celeste/bin/Celeste");
            assert_eq!(cwd.as_deref(), Some("/games/celeste/bin"));
        }
        assert_eq!(parse_time("2024-03-05 10:20:30.123+00:00"), Some(1709634030));
    }

    #[test]
    fn reads_db() {
        let dir = std::env::temp_dir().join(format!("superhuman-itch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("butler.db");
        let _ = std::fs::remove_file(&db);
        let c = Connection::open(&db).unwrap();
        c.execute_batch(
            r#"CREATE TABLE games (id INTEGER PRIMARY KEY, title TEXT);
               CREATE TABLE caves (id TEXT PRIMARY KEY, game_id INTEGER, seconds_run INTEGER, last_touched_at DATETIME, verdict TEXT);
               INSERT INTO games VALUES (1, 'Celeste');
               INSERT INTO caves VALUES ('abc', 1, 600, '2024-03-05 10:20:30', '{"basePath":"/g","candidates":[{"path":"c","flavor":"linux"},{"path":"c.exe","flavor":"windows"},{"path":"C.app","flavor":"app-macos"}]}');"#,
        )
        .unwrap();
        drop(c);
        let g = read_db(&db).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].title.as_str(), g[0].minutes_played), ("Celeste", 10));
    }
}
