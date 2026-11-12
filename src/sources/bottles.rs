//! Bottles (Linux): programs added to each bottle, from its `bottle.yml`.

use super::{Discovered, Places, launcher};
use crate::library::Source;

const FLATPAK: &str = "com.usebottles.bottles";

pub fn scan() -> Result<Vec<Discovered>, String> {
    if !cfg!(target_os = "linux") {
        return Ok(Vec::new());
    }
    let places = Places { config: &[], data: &["bottles/bottles"], flatpak: Some(FLATPAK) };
    let mut out = Vec::new();
    for (root, flatpak) in places.existing() {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for e in entries.flatten() {
            let Ok(text) = std::fs::read_to_string(e.path().join("bottle.yml")) else { continue };
            let folder = e.file_name().to_string_lossy().into_owned();
            out.extend(programs(&folder, &text, flatpak));
        }
    }
    Ok(out)
}

/// The bottle's programs. Hand-rolled reading of the two levels we need, to avoid a YAML crate.
fn programs(folder: &str, text: &str, flatpak: bool) -> Vec<Discovered> {
    let bottle = text.lines().find_map(|l| l.strip_prefix("Name:")).map(unquote).unwrap_or_else(|| folder.to_owned());
    let mut out = Vec::new();
    let mut in_section = false;
    let mut cur: Option<(String, bool)> = None; // (name, removed)
    let mut finish = |cur: &mut Option<(String, bool)>| {
        if let Some((name, false)) = cur.take().filter(|(n, _)| !n.is_empty()) {
            out.push(Discovered {
                source: Source::Bottles,
                source_id: format!("{folder}/{name}"),
                launch: launcher("bottles-cli", FLATPAK, flatpak, vec!["run".into(), "-b".into(), bottle.clone(), "-p".into(), name.clone()]),
                title: name,
                minutes_played: 0,
                last_played: None,
                steam_appid: None,
            });
        }
    };
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        if indent == 0 && !t.is_empty() {
            finish(&mut cur);
            in_section = t.starts_with("External_Programs:") && !t.ends_with("{}");
            continue;
        }
        if !in_section || t.is_empty() {
            continue;
        }
        if indent == 2 {
            finish(&mut cur);
            cur = Some((String::new(), false));
        } else if let Some(c) = cur.as_mut() {
            if let Some(v) = t.strip_prefix("name:") {
                c.0 = unquote(v);
            } else if let Some(v) = t.strip_prefix("removed:") {
                c.1 = v.trim() == "true";
            }
        }
    }
    finish(&mut cur);
    out
}

fn unquote(v: &str) -> String {
    v.trim().trim_matches(|c| c == '\'' || c == '"').to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_programs() {
        let yml = "Arch: win64\nExternal_Programs:\n  a1b2:\n    executable: Game.exe\n    id: a1b2\n    name: 'Cool Game'\n    path: /x/Game.exe\n  c3d4:\n    name: Old Thing\n    removed: true\nName: Gaming\nRunner: soda\n";
        let p = super::programs("Gaming", yml, false);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].title, "Cool Game");
        assert_eq!(p[0].source_id, "Gaming/Cool Game");
        let crate::sources::LaunchSpec::Command { args, .. } = &p[0].launch;
        assert_eq!(args[args.len() - 4..], ["-b", "Gaming", "-p", "Cool Game"]);
    }
}
