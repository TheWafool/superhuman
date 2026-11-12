//! Prism Launcher: each Minecraft instance is a card. Works on Linux, Windows and macOS.

use std::path::{Path, PathBuf};

use super::{Discovered, LaunchSpec, Places, launcher, which};
use crate::library::Source;

const FLATPAK: &str = "org.prismlauncher.PrismLauncher";

pub fn scan() -> Result<Vec<Discovered>, String> {
    let places = Places { config: &[], data: &["PrismLauncher"], flatpak: Some(FLATPAK) };
    let mut out = Vec::new();
    for (root, flatpak) in places.existing() {
        out.extend(scan_root(&root, flatpak));
    }
    Ok(out)
}

fn scan_root(root: &Path, flatpak: bool) -> Vec<Discovered> {
    // The instance folder can be moved in Prism's settings.
    let dir = std::fs::read_to_string(root.join("prismlauncher.cfg"))
        .ok()
        .and_then(|t| ini(&t, "InstanceDir"))
        .map(|d| if Path::new(&d).is_absolute() { PathBuf::from(d) } else { root.join(d) })
        .unwrap_or_else(|| root.join("instances"));
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let Ok(cfg) = std::fs::read_to_string(e.path().join("instance.cfg")) else { continue };
        let folder = e.file_name().to_string_lossy().into_owned();
        out.push(instance(&folder, &cfg, flatpak));
    }
    out
}

fn instance(folder: &str, cfg: &str, flatpak: bool) -> Discovered {
    Discovered {
        source: Source::Prism,
        source_id: folder.to_owned(),
        title: ini(cfg, "name").unwrap_or_else(|| folder.to_owned()),
        launch: launch(folder, flatpak),
        minutes_played: ini(cfg, "totalTimePlayed").and_then(|s| s.parse::<u64>().ok()).map_or(0, |s| (s / 60) as u32),
        last_played: ini(cfg, "lastLaunchTime").and_then(|s| s.parse::<i64>().ok()).filter(|&t| t > 0).map(|ms| ms / 1000),
        steam_appid: None,
    }
}

fn launch(folder: &str, flatpak: bool) -> LaunchSpec {
    let args = vec!["-l".to_owned(), folder.to_owned()];
    if which("prismlauncher").is_none() && !flatpak {
        // Installed where PATH doesn't reach.
        let known = [
            dirs::data_local_dir().map(|d| d.join("Programs/PrismLauncher/prismlauncher.exe")),
            Some(PathBuf::from("/Applications/Prism Launcher.app/Contents/MacOS/prismlauncher")),
        ];
        if let Some(p) = known.into_iter().flatten().find(|p| p.is_file()) {
            return LaunchSpec::cmd(p.to_string_lossy(), args);
        }
    }
    launcher("prismlauncher", FLATPAK, flatpak, args)
}

/// `key=value` from an INI-style file (sections ignored).
fn ini(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_owned()).filter(|v| !v.is_empty())
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_instance() {
        let cfg = "[General]\nConfigVersion=1.2\nname=Fabulously Optimized\ntotalTimePlayed=7260\nlastLaunchTime=1700000000123\n";
        let d = super::instance("fo-1.20", cfg, false);
        assert_eq!(d.title, "Fabulously Optimized");
        assert_eq!(d.minutes_played, 121);
        assert_eq!(d.last_played, Some(1_700_000_000));
    }
}
