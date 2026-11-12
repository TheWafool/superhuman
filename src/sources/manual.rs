//! Games added by hand in `games.toml`, next to `config.toml`. See `games.example.toml`.
//! Windows games on Linux run through umu-launcher (Proton) or Wine.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Discovered, LaunchSpec, home, on_path};
use crate::library::Source;

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    game: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    title: String,
    /// Program, script or `.exe`. `~` means your home folder.
    run: String,
    #[serde(default)]
    args: Vec<String>,
    /// `native`, `proton` (umu-launcher) or `wine`. Default: native, or proton for an `.exe`
    /// outside Windows (wine if umu isn't installed).
    runner: Option<String>,
    /// Wine prefix for `proton`/`wine`.
    prefix: Option<String>,
    /// Proton build for umu (`PROTONPATH`): a path or a name like `GE-Proton`.
    proton: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    /// Steam app id, for art and details.
    steam_appid: Option<u32>,
}

pub fn path() -> PathBuf {
    crate::config::path().with_file_name("games.toml")
}

pub fn scan() -> Result<Vec<Discovered>, String> {
    let p = path();
    let Ok(text) = std::fs::read_to_string(&p) else { return Ok(Vec::new()) };
    parse(&text).map_err(|e| format!("{}: {e}", p.display()))
}

fn parse(text: &str) -> Result<Vec<Discovered>, String> {
    let f: File = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(f.game.into_iter().map(discovered).collect())
}

fn discovered(e: Entry) -> Discovered {
    let run = expand(&e.run);
    let exe = run.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe"));
    let runner = e.runner.as_deref().unwrap_or(match (exe && !cfg!(windows), on_path("umu-run")) {
        (false, _) => "native",
        (true, true) => "proton",
        (true, false) => "wine",
    });
    let run_s = run.to_string_lossy().into_owned();
    let mut spec = match runner {
        "proton" => {
            let mut s = LaunchSpec::cmd("umu-run", std::iter::once(run_s).chain(e.args));
            // umu wants a GAMEID; umu-<steam id> also picks up protonfixes for that game.
            s = s.with_env("GAMEID", e.steam_appid.map_or("umu-default".into(), |id| format!("umu-{id}")));
            s = s.with_env("PROTONPATH", e.proton.as_deref().map_or("GE-Proton".into(), |p| expand(p).to_string_lossy().into_owned()));
            s
        }
        "wine" => LaunchSpec::cmd("wine", std::iter::once(run_s).chain(e.args)),
        _ => LaunchSpec::cmd(run_s, e.args),
    };
    if runner != "native" {
        let prefix = e.prefix.as_deref().map(expand).unwrap_or_else(|| home().join("Games/superhuman-prefixes").join(slug(&e.title)));
        spec = spec.with_env("WINEPREFIX", prefix.to_string_lossy());
    }
    for (k, v) in e.env {
        spec = spec.with_env(k, v);
    }
    if let Some(dir) = run.parent().filter(|d| d.is_dir()) {
        spec = spec.in_dir(dir.to_string_lossy());
    }
    Discovered {
        source: Source::Manual,
        source_id: slug(&e.title),
        title: e.title,
        launch: spec,
        minutes_played: 0,
        last_played: None,
        steam_appid: e.steam_appid,
    }
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => Path::new(p).to_path_buf(),
    }
}

fn slug(s: &str) -> String {
    s.chars().filter_map(|c| if c.is_alphanumeric() { Some(c.to_ascii_lowercase()) } else if c == ' ' { Some('-') } else { None }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_games() {
        let g = parse(
            r#"
            [[game]]
            title = "Doom"
            run = "/usr/bin/gzdoom"
            args = ["-iwad", "doom2.wad"]

            [[game]]
            title = "Old Thing"
            run = "/games/old/old.exe"
            runner = "proton"
            prefix = "/pfx/old"
            env = { DXVK_HUD = "fps" }
            "#,
        )
        .unwrap();
        assert_eq!(g.len(), 2);
        let LaunchSpec::Command { program, args, .. } = &g[0].launch;
        assert_eq!((program.as_str(), args.len()), ("/usr/bin/gzdoom", 2));
        let LaunchSpec::Command { program, args, env, .. } = &g[1].launch;
        assert_eq!(program, "umu-run");
        assert_eq!(args[0], "/games/old/old.exe");
        assert!(env.contains(&("WINEPREFIX".into(), "/pfx/old".into())));
        assert!(env.contains(&("DXVK_HUD".into(), "fps".into())));
        assert_eq!(g[1].source_id, "old-thing");
    }
}
