//! Epic Games Store (Windows and macOS): installed games from the launcher's `.item` manifests.
//! Games start through the launcher's own URI. Not available on Linux (use Heroic there).

use std::path::PathBuf;

use serde_json::Value;

use super::{Discovered, LaunchSpec};
use crate::library::Source;

fn manifest_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        let base = std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
        Some(base.join(r"Epic\EpicGamesLauncher\Data\Manifests"))
    } else if cfg!(target_os = "macos") {
        dirs::data_dir().map(|d| d.join("Epic/EpicGamesLauncher/Data/Manifests"))
    } else {
        None
    }
}

pub fn scan() -> Result<Vec<Discovered>, String> {
    let Some(dir) = manifest_dir().filter(|d| d.is_dir()) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
        if e.path().extension().is_some_and(|x| x == "item") {
            if let Some(g) = std::fs::read_to_string(e.path()).ok().and_then(|t| parse(&t)) {
                out.push(g);
            }
        }
    }
    Ok(out)
}

fn parse(text: &str) -> Option<Discovered> {
    let v: Value = serde_json::from_str(text).ok()?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
    // Add-ons and engine components have no executable.
    s("LaunchExecutable")?;
    let (app, ns, item) = (s("AppName")?, s("CatalogNamespace")?, s("CatalogItemId")?);
    Some(Discovered {
        source: Source::Epic,
        source_id: app.to_owned(),
        title: s("DisplayName")?.to_owned(),
        launch: {
            let l = LaunchSpec::uri(format!("com.epicgames.launcher://apps/{ns}%3A{item}%3A{app}?action=launch&silent=true"));
            match s("InstallLocation") {
                Some(d) => l.watching(d),
                None => l,
            }
        },
        minutes_played: 0,
        last_played: None,
        steam_appid: None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_manifest() {
        let g = super::parse(r#"{"DisplayName":"Fortnite","AppName":"Fortnite","CatalogNamespace":"fn","CatalogItemId":"abc","LaunchExecutable":"x.exe"}"#).unwrap();
        assert_eq!(g.title, "Fortnite");
        let crate::sources::LaunchSpec::Command { args, .. } = &g.launch;
        assert!(args.last().unwrap().contains("fn%3Aabc%3AFortnite"));
        assert!(super::parse(r#"{"DisplayName":"DLC","AppName":"a","CatalogNamespace":"n","CatalogItemId":"i"}"#).is_none());
    }
}
