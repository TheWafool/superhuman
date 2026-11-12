//! Lutris: installed games from its sqlite DB (opened read-only).

use rusqlite::{Connection, OpenFlags};

use super::{Discovered, LaunchSpec, home, on_path};
use crate::library::Source;

pub fn scan() -> Result<Vec<Discovered>, String> {
    let h = home();
    let candidates = [
        (h.join(".local/share/lutris/pga.db"), false),
        (h.join(".var/app/net.lutris.Lutris/data/lutris/pga.db"), true),
    ];
    let mut out = Vec::new();
    for (db, flatpak) in candidates {
        if !db.is_file() {
            continue;
        }
        let conn = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(|e| format!("{}: {e}", db.display()))?;
        let mut stmt = conn
            .prepare("SELECT id, name, playtime, lastplayed, service, service_id FROM games WHERE installed = 1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<f64>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for (id, name, hours, last, service, service_id) in rows.flatten() {
            let Some(title) = name else { continue };
            let uri = format!("lutris:rungameid/{id}");
            let launch = if flatpak || !on_path("lutris") {
                LaunchSpec::cmd("flatpak", ["run", "net.lutris.Lutris", &uri])
            } else {
                LaunchSpec::cmd("lutris", [uri])
            };
            let steam_appid = (service.as_deref() == Some("steam")).then(|| service_id.and_then(|s| s.parse().ok())).flatten();
            out.push(Discovered {
                source: Source::Lutris,
                source_id: id.to_string(),
                title,
                launch,
                minutes_played: (hours.unwrap_or(0.0) * 60.0).round() as u32,
                last_played: last.filter(|&t| t > 0),
                steam_appid,
            });
        }
    }
    Ok(out)
}
