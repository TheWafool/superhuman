//! Library sources. Each scanner reads a launcher's local files; none touch the network.

pub mod bottles;
pub mod desktop;
pub mod emulation;
pub mod epic;
pub mod gog;
pub mod heroic;
pub mod itch;
pub mod lutris;
pub mod manual;
pub mod prism;
pub mod steam;
pub mod vdf;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::Source;

/// How to start a game. Stored as JSON in the library DB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LaunchSpec {
    Command {
        program: String,
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
}

impl LaunchSpec {
    pub fn cmd(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        LaunchSpec::Command { program: program.into(), args: args.into_iter().map(Into::into).collect(), env: Vec::new(), cwd: None }
    }

    pub fn with_env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        let LaunchSpec::Command { env, .. } = &mut self;
        env.push((k.into(), v.into()));
        self
    }

    pub fn in_dir(mut self, dir: impl Into<String>) -> Self {
        let LaunchSpec::Command { cwd, .. } = &mut self;
        *cwd = Some(dir.into());
        self
    }

    /// Open a URI (`steam://…`, `heroic://…`) with whatever the system has registered for it.
    pub fn uri(uri: impl Into<String>) -> Self {
        let uri = uri.into();
        if cfg!(windows) {
            Self::cmd("cmd", ["/C".to_owned(), "start".into(), String::new(), uri])
        } else if cfg!(target_os = "macos") {
            Self::cmd("open", [uri])
        } else {
            Self::cmd("xdg-open", [uri])
        }
    }
}

/// A game as one scanner reports it.
#[derive(Debug, Clone)]
pub struct Discovered {
    pub source: Source,
    pub source_id: String,
    pub title: String,
    pub launch: LaunchSpec,
    pub minutes_played: u32,
    /// Unix seconds.
    pub last_played: Option<i64>,
    pub steam_appid: Option<u32>,
}

#[derive(Debug)]
/// Result of one scanner. `Err` means "source present but unreadable": keep its old games.
pub struct ScanResult {
    pub source: Source,
    pub games: Result<Vec<Discovered>, String>,
}

/// Run every scanner. Sources that aren't installed return an empty Ok.
pub fn scan_all() -> Vec<ScanResult> {
    let scanners: [(Source, fn() -> Result<Vec<Discovered>, String>); 11] = [
        (Source::Steam, steam::scan),
        (Source::Lutris, lutris::scan),
        (Source::Heroic, heroic::scan),
        (Source::Desktop, desktop::scan),
        (Source::Bottles, bottles::scan),
        (Source::Itch, itch::scan),
        (Source::Epic, epic::scan),
        (Source::Gog, gog::scan),
        (Source::Prism, prism::scan),
        (Source::Emulation, emulation::scan),
        (Source::Manual, manual::scan),
    ];
    std::thread::scope(|s| {
        let handles: Vec<_> = scanners.iter().map(|&(src, f)| (src, s.spawn(f))).collect();
        let mut out: Vec<ScanResult> = Vec::new();
        for (src, h) in handles {
            let games = h.join().unwrap_or_else(|_| Err("scanner panicked".into()));
            if src == Source::Desktop {
                // The desktop scanner reports both Desktop and Flatpak games.
                let (fp, de): (Vec<_>, Vec<_>) = match games {
                    Ok(g) => g.into_iter().partition(|d| d.source == Source::Flatpak),
                    Err(e) => {
                        out.push(ScanResult { source: Source::Flatpak, games: Err(e.clone()) });
                        out.push(ScanResult { source: Source::Desktop, games: Err(e) });
                        continue;
                    }
                };
                out.push(ScanResult { source: Source::Flatpak, games: Ok(fp) });
                out.push(ScanResult { source: Source::Desktop, games: Ok(de) });
            } else {
                out.push(ScanResult { source: src, games });
            }
        }
        out
    })
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Full path of `name` on PATH (`name.exe` on Windows).
pub fn which(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) && !name.ends_with(".exe") { format!("{name}.exe") } else { name.to_owned() };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&file)).find(|p| p.is_file())
}

/// Is `name` an executable on PATH?
pub fn on_path(name: &str) -> bool {
    which(name).is_some()
}

/// Is this Flatpak app installed (per user or system wide)?
pub fn flatpak_installed(app_id: &str) -> bool {
    cfg!(target_os = "linux")
        && [home().join(".local/share/flatpak/app"), PathBuf::from("/var/lib/flatpak/app")].iter().any(|d| d.join(app_id).is_dir())
}

/// Where a launcher keeps its files, per system. Each entry is (path, runs as Flatpak).
/// `config`/`data` are joined to the platform's config and data dirs
/// (`~/.config` · `%APPDATA%` · `~/Library/Application Support`).
pub struct Places<'a> {
    pub config: &'a [&'a str],
    pub data: &'a [&'a str],
    /// Flatpak app id; its sandboxed copy of `config`/`data` is checked too.
    pub flatpak: Option<&'a str>,
}

impl Places<'_> {
    pub fn existing(&self) -> Vec<(PathBuf, bool)> {
        let mut out = Vec::new();
        let mut add = |p: PathBuf, fp: bool| {
            if p.exists() && !out.iter().any(|(q, _): &(PathBuf, bool)| q == &p) {
                out.push((p, fp));
            }
        };
        for c in self.config {
            if let Some(d) = dirs::config_dir() {
                add(d.join(c), false);
            }
        }
        for c in self.data {
            if let Some(d) = dirs::data_dir() {
                add(d.join(c), false);
            }
        }
        if let (Some(id), true) = (self.flatpak, cfg!(target_os = "linux")) {
            let root = home().join(".var/app").join(id);
            for c in self.config {
                add(root.join("config").join(c), true);
            }
            for c in self.data {
                add(root.join("data").join(c), true);
            }
        }
        out
    }
}

/// Run a launcher's own command, through Flatpak when that's how it's installed.
pub fn launcher(program: &str, flatpak_id: &str, flatpak: bool, args: Vec<String>) -> LaunchSpec {
    if flatpak || (!on_path(program) && flatpak_installed(flatpak_id)) {
        let mut a = vec!["run".to_owned(), format!("--command={program}"), flatpak_id.to_owned()];
        a.extend(args);
        LaunchSpec::cmd("flatpak", a)
    } else {
        LaunchSpec::cmd(which(program).map_or(program.to_owned(), |p| p.to_string_lossy().into_owned()), args)
    }
}
