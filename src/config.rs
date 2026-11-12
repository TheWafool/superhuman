//! User settings at `$XDG_CONFIG_HOME/superhuman/config.toml`.
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
    /// Button prompts: "auto" (follow the controller), "xbox", "playstation", "nintendo".
    pub prompts: String,
    /// Confirm on the right face button and Back on the bottom one (Nintendo style).
    pub swap_confirm: bool,
    /// Start full screen.
    pub fullscreen: bool,
    pub clock_12h: bool,
    /// Sources whose games are left out of the library (by `Source::as_str`).
    pub disabled_sources: Vec<String>,
}

/// The app was called Superman before; move its folders (settings, library, art) over once.
pub fn migrate_old_name() {
    let pairs = [dirs::config_dir(), dirs::data_dir(), dirs::cache_dir()].map(|b| b.map(|b| (b.join("superman"), b.join("superhuman"))));
    let mut moved_cache = None;
    for (i, (old, new)) in pairs.into_iter().flatten().enumerate() {
        if old.is_dir() && !new.exists() && std::fs::rename(&old, &new).is_ok() && i == 2 {
            moved_cache = Some((old, new));
        }
    }
    // Saved art paths point into the cache folder.
    if let Some((old, new)) = moved_cache {
        let db = crate::store::default_path();
        if let Ok(c) = rusqlite::Connection::open(&db) {
            let (o, n) = (old.to_string_lossy(), new.to_string_lossy());
            for col in ["cover", "hero", "logo", "backdrop"] {
                let _ = c.execute(&format!("UPDATE art_v2 SET {col} = replace({col}, ?1, ?2)"), [&*o, &*n]);
            }
        }
    }
    // A login entry under the old name would start the old binary path; recreate it.
    if let Some(d) = dirs::config_dir() {
        let old = d.join("autostart/superman.desktop");
        if old.is_file() && std::fs::remove_file(&old).is_ok() {
            let _ = crate::autostart::set(true);
        }
    }
}

pub fn path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| crate::sources::home().join(".config")).join("superhuman/config.toml")
}

pub fn load() -> Config {
    let mut cfg: Config = match std::fs::read_to_string(path()) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path().display());
            Config::default()
        }),
        Err(_) => Config::default(),
    };
    if let Ok(k) = std::env::var("SUPERHUMAN_STEAMGRIDDB_KEY") {
        cfg.steamgriddb_api_key = Some(k);
    }
    cfg.steamgriddb_api_key = cfg.steamgriddb_api_key.filter(|k| !k.trim().is_empty());
    cfg
}

/// Change settings on disk. Reads the file itself (not the environment), so a key passed in
/// through `SUPERHUMAN_STEAMGRIDDB_KEY` is never written out. The file stays private (0600).
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
