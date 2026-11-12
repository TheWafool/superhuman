//! Library sources. Each scanner reads a launcher's local files; none touch the network.

pub mod desktop;
pub mod heroic;
pub mod lutris;
pub mod steam;
pub mod vdf;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::Source;

/// How to start a game. Stored as JSON in the library DB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LaunchSpec {
    Command { program: String, args: Vec<String> },
}

impl LaunchSpec {
    pub fn cmd(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        LaunchSpec::Command { program: program.into(), args: args.into_iter().map(Into::into).collect() }
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
    let scanners: [(Source, fn() -> Result<Vec<Discovered>, String>); 4] = [
        (Source::Steam, steam::scan),
        (Source::Lutris, lutris::scan),
        (Source::Heroic, heroic::scan),
        (Source::Desktop, desktop::scan),
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

/// Is `name` an executable on PATH?
pub fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}
