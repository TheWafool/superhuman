//! User settings at `$XDG_CONFIG_HOME/superman/config.toml`.
//!
//! Secrets (API keys) live only here or in the environment, never in the binary.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steamgriddb_api_key: Option<String>,
    /// Instant movement instead of animation.
    pub reduce_motion: bool,
}

pub fn path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| crate::sources::home().join(".config")).join("superman/config.toml")
}

pub fn load() -> Config {
    let mut cfg: Config = match std::fs::read_to_string(path()) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path().display());
            Config::default()
        }),
        Err(_) => Config::default(),
    };
    if let Ok(k) = std::env::var("SUPERMAN_STEAMGRIDDB_KEY") {
        cfg.steamgriddb_api_key = Some(k);
    }
    cfg.steamgriddb_api_key = cfg.steamgriddb_api_key.filter(|k| !k.trim().is_empty());
    cfg
}

/// Change settings on disk. Reads the file itself (not the environment), so a key passed in
/// through `SUPERMAN_STEAMGRIDDB_KEY` is never written out. The file stays private (0600).
pub fn update(change: impl FnOnce(&mut Config)) -> Result<(), String> {
    let p = path();
    let mut cfg: Config = std::fs::read_to_string(&p).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default();
    change(&mut cfg);
    cfg.steamgriddb_api_key = cfg.steamgriddb_api_key.filter(|k| !k.trim().is_empty());
    let text = toml::to_string(&cfg).map_err(|e| e.to_string())?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = p.with_extension("toml.tmp");
    // Created private from the start, so the key is never readable by others, even briefly.
    let _ = std::fs::remove_file(&tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    std::io::Write::write_all(&mut opts.open(&tmp).map_err(|e| e.to_string())?, text.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}
