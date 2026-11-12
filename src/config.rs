//! User settings at `$XDG_CONFIG_HOME/superman/config.toml`.
//!
//! Secrets (API keys) live only here or in the environment, never in the binary.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub steamgriddb_api_key: Option<String>,
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
