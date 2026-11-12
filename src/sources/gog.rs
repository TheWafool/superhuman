//! GOG (Windows and macOS): installed games from the `goggame-<id>.info` file each one keeps in
//! its folder. Searched in the usual install roots; start the game's own primary executable.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Discovered, LaunchSpec};
use crate::library::Source;

fn roots() -> Vec<PathBuf> {
    let mut r = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = std::env::var_os(var) {
                r.push(PathBuf::from(&p).join(r"GOG Galaxy\Games"));
                r.push(PathBuf::from(&p).join("GOG Games"));
            }
        }
        r.push(PathBuf::from(r"C:\GOG Games"));
    } else if cfg!(target_os = "macos") {
        r.push(PathBuf::from("/Users/Shared/GOG.com/Galaxy/Applications"));
        r.push(super::home().join("Applications/GOG"));
    }
    r
}

pub fn scan() -> Result<Vec<Discovered>, String> {
    let mut out = Vec::new();
    for root in roots().into_iter().filter(|r| r.is_dir()) {
        for game in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let dir = game.path();
            for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let n = f.file_name().to_string_lossy().into_owned();
                if n.starts_with("goggame-") && n.ends_with(".info") {
                    if let Some(g) = std::fs::read_to_string(f.path()).ok().and_then(|t| parse(&t, &dir)) {
                        out.push(g);
                    }
                }
            }
        }
    }
    // Play time, best effort: Galaxy's database keeps minutes per `gog_<id>` release key.
    let times = galaxy_minutes();
    for g in &mut out {
        g.minutes_played = times.get(&format!("gog_{}", g.source_id)).copied().unwrap_or(0);
    }
    Ok(out)
}

fn galaxy_minutes() -> std::collections::HashMap<String, u32> {
    let mut m = std::collections::HashMap::new();
    let base = if cfg!(windows) {
        std::env::var_os("ProgramData").map(|p| PathBuf::from(p).join(r"GOG.com\Galaxy\storage"))
    } else {
        Some(PathBuf::from("/Users/Shared/GOG.com/Galaxy/Storage"))
    };
    let Some(db) = base.map(|b| b.join("galaxy-2.0.db")).filter(|p| p.is_file()) else { return m };
    let Ok(conn) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { return m };
    let Ok(mut stmt) = conn.prepare("SELECT releaseKey, SUM(gameTime) FROM GameTimes GROUP BY releaseKey") else { return m };
    if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
        for (k, v) in rows.flatten() {
            m.insert(k, (v.max(0) / 60) as u32);
        }
    }
    m
}

fn parse(text: &str, dir: &Path) -> Option<Discovered> {
    let v: Value = serde_json::from_str(text).ok()?;
    let id = v.get("gameId")?.as_str()?;
    let task = v.get("playTasks")?.as_array()?.iter().find(|t| t.get("isPrimary").and_then(Value::as_bool) == Some(true))?;
    let exe = dir.join(task.get("path")?.as_str()?);
    let args = task.get("arguments").and_then(Value::as_str).unwrap_or("").split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    let work = task.get("workingDir").and_then(Value::as_str).map_or_else(|| dir.to_path_buf(), |w| dir.join(w));
    Some(Discovered {
        source: Source::Gog,
        source_id: id.to_owned(),
        title: v.get("name")?.as_str()?.to_owned(),
        launch: LaunchSpec::cmd(exe.to_string_lossy(), args).in_dir(work.to_string_lossy()),
        minutes_played: 0,
        last_played: None,
        steam_appid: None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_info() {
        let g = super::parse(
            r#"{"gameId":"123","name":"Witcher","playTasks":[{"isPrimary":false,"path":"cfg.exe"},{"isPrimary":true,"path":"w.exe","arguments":"-a"}]}"#,
            std::path::Path::new("/g/w"),
        )
        .unwrap();
        assert_eq!((g.title.as_str(), g.source_id.as_str()), ("Witcher", "123"));
        let crate::sources::LaunchSpec::Command { args, .. } = &g.launch;
        assert_eq!(args, &["-a"]);
    }
}
