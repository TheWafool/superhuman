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
    Ok(out)
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
