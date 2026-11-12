//! Native and Flatpak games from `.desktop` entries with `Categories=Game`.

use std::collections::HashSet;
use std::path::PathBuf;

use super::{Discovered, LaunchSpec, home};
use crate::library::Source;

/// Launchers and tools that are tagged as games but are covered by other scanners or aren't games.
const SKIP_IDS: &[&str] = &[
    "steam", "com.valvesoftware.Steam", "lutris", "net.lutris.Lutris", "heroic", "com.heroicgameslauncher.hgl",
    "com.usebottles.bottles", "io.itch.itch", "itch", "org.prismlauncher.PrismLauncher", "net.davidotek.pupgui2",
    "org.vinegarhq.Vinegar", "superhuman",
];
/// Entries created by launchers for their own games: their scanners already report them.
const SKIP_EXEC: &[&str] = &["steam://rungameid", "lutris:rungameid", "heroic://launch"];

fn app_dirs() -> Vec<PathBuf> {
    let h = home();
    let mut dirs = vec![
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| h.join(".local/share")),
        h.join(".local/share/flatpak/exports/share"),
        PathBuf::from("/var/lib/flatpak/exports/share"),
    ];
    let data = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(data.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    let mut seen = HashSet::new();
    dirs.into_iter().map(|d| d.join("applications")).filter(|d| seen.insert(d.clone()) && d.is_dir()).collect()
}

pub fn scan() -> Result<Vec<Discovered>, String> {
    if cfg!(target_os = "macos") {
        return Ok(mac_apps());
    }
    let mut out = Vec::new();
    let mut seen_ids = HashSet::new();
    for dir in app_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            let id = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            // Earlier dirs take precedence (XDG rules).
            if !seen_ids.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            if let Some(d) = parse(&id, &text) {
                out.push(d);
            }
        }
    }
    Ok(out)
}

fn parse(id: &str, text: &str) -> Option<Discovered> {
    let mut in_main = false;
    let (mut name, mut exec, mut cats, mut flatpak) = (None, None, String::new(), false);
    let (mut typ_app, mut hidden) = (false, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "Name" => name = Some(v.trim().to_owned()),
            "Exec" => exec = Some(v.trim().to_owned()),
            "Categories" => cats = v.to_owned(),
            "Type" => typ_app = v.trim() == "Application",
            "NoDisplay" | "Hidden" if v.trim() == "true" => hidden = true,
            "X-Flatpak" => flatpak = true,
            _ => {}
        }
    }
    let exec = exec?;
    if !typ_app || hidden || !cats.split(';').any(|c| c == "Game") || SKIP_IDS.contains(&id) || SKIP_EXEC.iter().any(|s| exec.contains(s)) {
        return None;
    }
    let mut argv = split_exec(&exec);
    if argv.is_empty() {
        return None;
    }
    let program = argv.remove(0);
    Some(Discovered {
        source: if flatpak { Source::Flatpak } else { Source::Desktop },
        source_id: id.to_owned(),
        title: name?,
        launch: LaunchSpec::cmd(program, argv),
        minutes_played: 0,
        last_played: None,
        steam_appid: None,
    })
}

/// Split an `Exec=` value into argv, dropping field codes (%f, %U, ...).
fn split_exec(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = exec.chars().peekable();
    let mut has = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                has = true;
            }
            '\\' if quoted => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() && !quoted => {
                if has || !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        args.push(cur);
    }
    args.into_iter()
        .filter(|a| !(a.len() == 2 && a.starts_with('%')))
        // Flatpak file-forwarding markers wrap the (already dropped) field codes.
        .filter(|a| !matches!(a.as_str(), "@@" | "@@u" | "@@f"))
        .map(|a| a.replace("%%", "%"))
        .collect()
}

/// macOS: apps in /Applications whose Info.plist category is a games one.
/// Reads through `defaults`, which copes with binary plists.
fn mac_apps() -> Vec<Discovered> {
    let dirs = [PathBuf::from("/Applications"), super::home().join("Applications")];
    let mut out = Vec::new();
    for e in dirs.iter().filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "app") {
            continue;
        }
        let info = path.join("Contents/Info");
        let cat = std::process::Command::new("defaults").args(["read", &info.to_string_lossy(), "LSApplicationCategoryType"]).output();
        if !cat.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("games")) {
            continue;
        }
        let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(Discovered {
            source: Source::Desktop,
            source_id: title.to_lowercase().replace(' ', "-"),
            launch: LaunchSpec::cmd("open", ["-W".to_owned(), path.to_string_lossy().into_owned()]),
            title,
            minutes_played: 0,
            last_played: None,
            steam_appid: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_split() {
        assert_eq!(split_exec(r#"/usr/bin/flatpak run --branch=stable org.x.Y @@u %U @@"#), vec!["/usr/bin/flatpak", "run", "--branch=stable", "org.x.Y"]);
        assert_eq!(split_exec(r#""/opt/My Game/run.sh" -x %f"#), vec!["/opt/My Game/run.sh", "-x"]);
    }

    #[test]
    fn parses_game_only() {
        let g = parse("wesnoth", "[Desktop Entry]\nType=Application\nName=Battle for Wesnoth\nExec=wesnoth %f\nCategories=Game;StrategyGame;\n").unwrap();
        assert_eq!(g.title, "Battle for Wesnoth");
        assert!(parse("gimp", "[Desktop Entry]\nType=Application\nName=GIMP\nExec=gimp\nCategories=Graphics;\n").is_none());
    }
}
