//! Heroic: Epic (legendary), GOG, Amazon (nile) and sideloaded games.
//!
//! Untested against a real install (Heroic isn't on the dev machine); file layouts
//! follow Heroic 2.x. Every file is optional and parsed loosely.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::{Discovered, LaunchSpec, Places, on_path};

const FLATPAK: &str = "com.heroicgameslauncher.hgl";
use crate::library::Source;

pub fn scan() -> Result<Vec<Discovered>, String> {
    let places = Places { config: &["heroic"], data: &[], flatpak: Some(FLATPAK) };
    Ok(places.existing().into_iter().flat_map(|(dir, flatpak)| scan_dir(&dir, flatpak)).collect())
}

fn json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn scan_dir(dir: &Path, flatpak: bool) -> Vec<Discovered> {
    // (appName, runner, title)
    let mut found: Vec<(String, &str, String)> = Vec::new();

    if let Some(Value::Object(m)) = json(&dir.join("legendaryConfig/legendary/installed.json")) {
        for (app, v) in m {
            let title = v.get("title").and_then(Value::as_str).unwrap_or(&app).to_owned();
            found.push((app, "legendary", title));
        }
    }

    let gog_titles = titles(json(&dir.join("gog_store/library.json")).as_ref(), "games", "app_name", "title");
    if let Some(v) = json(&dir.join("gog_store/installed.json")) {
        for g in v.get("installed").and_then(Value::as_array).into_iter().flatten() {
            if let Some(app) = g.get("appName").and_then(Value::as_str) {
                let title = gog_titles.get(app).cloned().unwrap_or_else(|| app.to_owned());
                found.push((app.to_owned(), "gog", title));
            }
        }
    }

    if let Some(Value::Array(list)) = json(&dir.join("nile_config/nile/installed.json")) {
        let lib = json(&dir.join("nile_config/nile/library.json"));
        let mut amazon: HashMap<String, String> = HashMap::new();
        for g in lib.as_ref().and_then(Value::as_array).into_iter().flatten() {
            if let (Some(id), Some(t)) = (g.get("id").and_then(Value::as_str), g.pointer("/product/title").and_then(Value::as_str)) {
                amazon.insert(id.into(), t.into());
            }
        }
        for g in list {
            if let Some(id) = g.get("id").and_then(Value::as_str) {
                let title = amazon.get(id).cloned().unwrap_or_else(|| id.to_owned());
                found.push((id.to_owned(), "nile", title));
            }
        }
    }

    if let Some(v) = json(&dir.join("sideload_apps/library.json")) {
        for g in v.get("games").and_then(Value::as_array).into_iter().flatten() {
            let installed = g.get("is_installed").and_then(Value::as_bool).unwrap_or(true);
            if let (true, Some(app), Some(t)) = (installed, g.get("app_name").and_then(Value::as_str), g.get("title").and_then(Value::as_str)) {
                found.push((app.to_owned(), "sideload", t.to_owned()));
            }
        }
    }

    let stats = json(&dir.join("store/timestamp.json"));
    found
        .into_iter()
        .map(|(app, runner, title)| {
            let s = stats.as_ref().and_then(|s| s.get(&app));
            let minutes = s.and_then(|s| s.get("totalPlayed")).and_then(Value::as_f64).unwrap_or(0.0) as u32;
            let last = s
                .and_then(|s| s.get("lastPlayed"))
                .and_then(Value::as_str)
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.timestamp());
            Discovered {
                source: Source::Heroic,
                source_id: format!("{runner}:{app}"),
                title,
                launch: launch(&app, runner, flatpak),
                minutes_played: minutes,
                last_played: last,
                steam_appid: None,
            }
        })
        .collect()
}

fn titles(v: Option<&Value>, list: &str, id: &str, title: &str) -> HashMap<String, String> {
    v.and_then(|v| v.get(list))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| Some((g.get(id)?.as_str()?.to_owned(), g.get(title)?.as_str()?.to_owned())))
        .collect()
}

fn launch(app: &str, runner: &str, flatpak: bool) -> LaunchSpec {
    let uri = format!("heroic://launch?appName={app}&runner={runner}");
    if !cfg!(target_os = "linux") {
        LaunchSpec::uri(uri)
    } else if flatpak || !on_path("heroic") {
        LaunchSpec::cmd("flatpak", ["run", FLATPAK, "--no-gui", &uri])
    } else {
        LaunchSpec::cmd("heroic", ["--no-gui".to_owned(), uri])
    }
}
