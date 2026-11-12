//! Steam: installed apps from library manifests, playtime from localconfig, non-Steam shortcuts.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::vdf;
use super::{Discovered, LaunchSpec, home, read};
use crate::library::Source;

/// Steam tools that are installed like games but aren't games.
const NOT_GAMES: &[u32] = &[228980, 1070560, 1391110, 1628350, 1493710, 2180100, 2348590, 4183110, 250820, 1826330];
const NOT_GAME_PREFIXES: &[&str] = &["Proton", "Steam Linux Runtime", "Steamworks Common", "SteamVR"];

struct Root {
    path: PathBuf,
    flatpak: bool,
}

fn roots() -> Vec<Root> {
    let h = home();
    let mut candidates = vec![
        (h.join(".local/share/Steam"), false),
        (h.join(".steam/steam"), false),
        (h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"), true),
    ];
    // macOS: ~/Library/Application Support/Steam
    if let Some(d) = dirs::data_dir() {
        candidates.push((d.join("Steam"), false));
    }
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = std::env::var_os(var) {
                candidates.push((PathBuf::from(p).join("Steam"), false));
            }
        }
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|(p, _)| p.join("steamapps").is_dir())
        .filter(|(p, _)| seen.insert(p.canonicalize().unwrap_or_else(|_| p.clone())))
        .map(|(path, flatpak)| Root { path, flatpak })
        .collect()
}

fn launch(root: &Root, rungameid: u64) -> LaunchSpec {
    let uri = format!("steam://rungameid/{rungameid}");
    if !cfg!(target_os = "linux") {
        LaunchSpec::uri(uri)
    } else if root.flatpak {
        LaunchSpec::cmd("flatpak", ["run", "com.valvesoftware.Steam", &uri])
    } else {
        LaunchSpec::cmd("steam", [uri])
    }
}

pub fn scan() -> Result<Vec<Discovered>, String> {
    let mut out = Vec::new();
    for root in roots() {
        let stats = playtime(&root.path);
        for lib in library_paths(&root.path) {
            let Ok(entries) = std::fs::read_dir(lib.join("steamapps")) else { continue };
            for e in entries.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                    continue;
                }
                let Ok(text) = read(&e.path()) else { continue };
                if let Some(d) = manifest(&root, &text, &stats) {
                    out.push(d);
                }
            }
        }
        out.extend(shortcuts(&root));
    }
    out.sort_by_key(|d| d.source_id.clone());
    out.dedup_by_key(|d| d.source_id.clone());
    Ok(out)
}

fn library_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.to_path_buf()];
    if let Ok(text) = read(&root.join("steamapps/libraryfolders.vdf")) {
        let n = vdf::parse_text(&text);
        if let Some(lf) = n.get("libraryfolders") {
            for (_, f) in lf.entries() {
                if let Some(p) = f.str("path") {
                    let p = PathBuf::from(p);
                    if !paths.contains(&p) {
                        paths.push(p);
                    }
                }
            }
        }
    }
    paths
}

/// appid → (minutes, last played unix).
fn playtime(root: &Path) -> HashMap<u32, (u32, i64)> {
    let mut out = HashMap::new();
    let Ok(users) = std::fs::read_dir(root.join("userdata")) else { return out };
    for u in users.flatten() {
        let Ok(text) = read(&u.path().join("config/localconfig.vdf")) else { continue };
        let n = vdf::parse_text(&text);
        let Some(apps) = n.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]) else { continue };
        for (id, app) in apps.entries() {
            let Ok(id) = id.parse::<u32>() else { continue };
            let mins = app.str("Playtime").and_then(|s| s.parse().ok()).unwrap_or(0);
            let last = app.str("LastPlayed").and_then(|s| s.parse().ok()).unwrap_or(0);
            let e = out.entry(id).or_insert((0, 0));
            e.0 = e.0.max(mins);
            e.1 = e.1.max(last);
        }
    }
    out
}

fn manifest(root: &Root, text: &str, stats: &HashMap<u32, (u32, i64)>) -> Option<Discovered> {
    let n = vdf::parse_text(text);
    let app = n.get("AppState")?;
    let appid: u32 = app.str("appid")?.parse().ok()?;
    let title = app.str("name")?.to_owned();
    let flags: u32 = app.str("StateFlags").and_then(|s| s.parse().ok()).unwrap_or(0);
    if flags & 4 == 0 || NOT_GAMES.contains(&appid) || NOT_GAME_PREFIXES.iter().any(|p| title.starts_with(p)) {
        return None;
    }
    let (mins, last_cfg) = stats.get(&appid).copied().unwrap_or((0, 0));
    let last_manifest: i64 = app.str("LastPlayed").and_then(|s| s.parse().ok()).unwrap_or(0);
    let last = last_cfg.max(last_manifest);
    Some(Discovered {
        source: Source::Steam,
        source_id: appid.to_string(),
        title,
        launch: launch(root, appid as u64),
        minutes_played: mins,
        last_played: (last > 0).then_some(last),
        steam_appid: Some(appid),
    })
}

/// Non-Steam games added to Steam.
fn shortcuts(root: &Root) -> Vec<Discovered> {
    let mut out = Vec::new();
    let Ok(users) = std::fs::read_dir(root.path.join("userdata")) else { return out };
    for u in users.flatten() {
        let Ok(data) = std::fs::read(u.path().join("config/shortcuts.vdf")) else { continue };
        let Some(n) = vdf::parse_binary(&data) else { continue };
        let Some(list) = n.get("shortcuts") else { continue };
        for (_, s) in list.entries() {
            let (Some(id), Some(name)) = (s.str("appid").and_then(|a| a.parse::<u32>().ok()), s.str("AppName")) else { continue };
            if s.str("IsHidden") == Some("1") {
                continue;
            }
            let last: i64 = s.str("LastPlayTime").and_then(|t| t.parse().ok()).unwrap_or(0);
            out.push(Discovered {
                source: Source::Steam,
                source_id: format!("shortcut-{id}"),
                title: name.to_owned(),
                launch: launch(root, ((id as u64) << 32) | 0x0200_0000),
                minutes_played: 0,
                last_played: (last > 0).then_some(last),
                steam_appid: None,
            });
        }
    }
    out
}
