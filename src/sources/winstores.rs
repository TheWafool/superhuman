//! Windows-only stores: Xbox / Game Pass, Ubisoft Connect, EA app, Battle.net, Amazon Games.
//! Every scanner returns nothing on other systems. The file readers are thin; the parsers are
//! plain functions with tests that run everywhere.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::{Discovered, LaunchSpec};
use crate::library::Source;

type Reg = Vec<(String, BTreeMap<String, String>)>;

/// Parse `reg query <key> /s` output into (key path, values).
fn parse_reg(text: &str) -> Reg {
    let mut out: Reg = Vec::new();
    for line in text.lines() {
        if line.starts_with("HKEY_") {
            out.push((line.trim().to_owned(), BTreeMap::new()));
        } else if let (Some((_, vals)), Some(i)) = (out.last_mut(), line.find("    REG_")) {
            let name = line[..i].trim().to_owned();
            // "    REG_SZ    value"
            let rest = &line[i + 4..];
            let value = rest.split_once("    ").map_or("", |(_, v)| v).trim().to_owned();
            vals.insert(name, value);
        }
    }
    out
}

fn reg_query(key: &str) -> Reg {
    if !cfg!(windows) {
        return Vec::new();
    }
    super::hidden("reg").args(["query", key, "/s"]).output().map(|o| parse_reg(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

fn last_segment(key: &str) -> &str {
    key.rsplit('\\').next().unwrap_or(key)
}

fn game(source: Source, id: String, title: String, launch: LaunchSpec) -> Discovered {
    Discovered { source, source_id: id, title, launch, minutes_played: 0, last_played: None, steam_appid: None }
}

fn local_app_data() -> Option<PathBuf> {
    dirs::data_local_dir()
}

impl Discovered {
    fn watch(mut self, dir: &str) -> Self {
        self.launch = self.launch.watching(dir);
        self
    }
}

// ---------- Ubisoft Connect ----------

pub fn ubisoft() -> Result<Vec<Discovered>, String> {
    Ok(ubisoft_from(&reg_query(r"HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs")))
}

fn ubisoft_from(reg: &Reg) -> Vec<Discovered> {
    reg.iter()
        .filter_map(|(key, v)| {
            let id = last_segment(key);
            let dir = v.get("InstallDir")?.trim_end_matches(['/', '\\']);
            let title = dir.rsplit(['/', '\\']).next()?.to_owned();
            id.chars().all(|c| c.is_ascii_digit()).then(|| game(Source::Ubisoft, id.to_owned(), title, LaunchSpec::uri(format!("uplay://launch/{id}/0"))).watch(dir))
        })
        .collect()
}

// ---------- EA app ----------

pub fn ea() -> Result<Vec<Discovered>, String> {
    let mut out = Vec::new();
    for (key, v) in reg_query(r"HKLM\SOFTWARE\WOW6432Node\EA Games") {
        let (Some(name), Some(dir)) = (v.get("DisplayName"), v.get("Install Dir")) else { continue };
        let dir = Path::new(dir);
        let Ok(xml) = std::fs::read_to_string(dir.join("__Installer/installerdata.xml")) else { continue };
        if let Some(exe) = ea_exe(&xml) {
            out.push(game(Source::Ea, last_segment(&key).to_owned(), name.clone(), LaunchSpec::cmd(dir.join(exe).to_string_lossy(), Vec::<String>::new()).in_dir(dir.to_string_lossy())));
        }
    }
    Ok(out)
}

/// The first `<filePath>` in installerdata.xml: `[HKEY…]\game.exe` → `game.exe`.
fn ea_exe(xml: &str) -> Option<String> {
    let start = xml.find("<filePath>")? + "<filePath>".len();
    let path = &xml[start..start + xml[start..].find("</filePath>")?];
    let path = path.rsplit_once(']').map_or(path, |(_, p)| p).trim_start_matches(['\\', '/']);
    path.to_lowercase().ends_with(".exe").then(|| path.to_owned())
}

// ---------- Battle.net ----------

pub fn battlenet() -> Result<Vec<Discovered>, String> {
    Ok(battlenet_from(&reg_query(r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall")))
}

fn battlenet_from(reg: &Reg) -> Vec<Discovered> {
    reg.iter()
        .filter_map(|(key, v)| {
            if !v.get("Publisher")?.contains("Blizzard Entertainment") {
                return None;
            }
            let title = v.get("DisplayName")?;
            if title.contains("Battle.net") {
                return None;
            }
            let icon = v.get("DisplayIcon")?.trim_matches('"');
            let exe = icon.rsplit_once(',').map_or(icon, |(p, _)| p).trim_matches('"');
            exe.to_lowercase().ends_with(".exe").then(|| {
                let dir = Path::new(exe).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
                game(Source::BattleNet, last_segment(key).to_owned(), title.clone(), LaunchSpec::cmd(exe, Vec::<String>::new()).in_dir(dir))
            })
        })
        .collect()
}

// ---------- Amazon Games ----------

pub fn amazon() -> Result<Vec<Discovered>, String> {
    let Some(db) = local_app_data().map(|d| d.join(r"Amazon Games/Data/Games/Sql/GameInstallInfo.sqlite")).filter(|p| p.is_file()) else {
        return Ok(Vec::new());
    };
    let conn = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| format!("{}: {e}", db.display()))?;
    let mut stmt = conn.prepare("SELECT Id, ProductTitle, InstallDirectory FROM DbSet WHERE Installed = 1").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))
        .map_err(|e| e.to_string())?;
    Ok(rows
        .flatten()
        .map(|(id, title, dir)| {
            let g = game(Source::Amazon, id.clone(), title, LaunchSpec::uri(format!("amazon-games://play/{id}")));
            match dir {
                Some(d) => g.watch(&d),
                None => g,
            }
        })
        .collect())
}

// ---------- Xbox / Game Pass ----------

pub fn xbox() -> Result<Vec<Discovered>, String> {
    if !cfg!(windows) {
        return Ok(Vec::new());
    }
    let script = "Get-AppxPackage | Where-Object { $_.InstallLocation -like '*XboxGames*' } | ForEach-Object { \"$($_.PackageFamilyName)|$($_.InstallLocation)\" }";
    let out = super::hidden("powershell").args(["-NoProfile", "-Command", script]).output().map_err(|e| format!("powershell: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .filter_map(|l| {
            let (pfn, loc) = l.trim().split_once('|')?;
            // MicrosoftGame.config sits in the install folder (or its Content sibling).
            let cfg = [Path::new(loc).join("MicrosoftGame.config"), Path::new(loc).parent()?.join("Content/MicrosoftGame.config")].into_iter().find_map(|p| std::fs::read_to_string(p).ok())?;
            Some(xbox_from(pfn, &cfg)?.watch(loc))
        })
        .collect())
}

fn attr<'a>(xml: &'a str, tag: &str, name: &str) -> Option<&'a str> {
    let t = &xml[xml.find(&format!("<{tag}"))?..];
    let t = &t[..t.find('>')?];
    let a = &t[t.find(&format!("{name}=\""))? + name.len() + 2..];
    Some(&a[..a.find('"')?])
}

fn xbox_from(pfn: &str, cfg: &str) -> Option<Discovered> {
    let title = attr(cfg, "ShellVisuals", "DefaultDisplayName").or_else(|| attr(cfg, "Identity", "Name"))?;
    // ms-resource names mean the real title lives in a resource file we don't read.
    if title.starts_with("ms-resource") {
        return None;
    }
    let app = attr(cfg, "Executable", "Id").unwrap_or("Game");
    Some(game(
        Source::Xbox,
        pfn.to_owned(),
        title.to_owned(),
        LaunchSpec::cmd("explorer.exe", [format!(r"shell:AppsFolder\{pfn}!{app}")]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_parsing() {
        let r = parse_reg("\r\nHKEY_LOCAL_MACHINE\\S\\Installs\\720\r\n    InstallDir    REG_SZ    C:/Games/Far Cry 5/\r\n    Language    REG_SZ    en\r\n");
        assert_eq!(r.len(), 1);
        let g = ubisoft_from(&r);
        assert_eq!((g[0].title.as_str(), g[0].source_id.as_str()), ("Far Cry 5", "720"));
    }

    #[test]
    fn ea_and_battlenet_and_xbox() {
        assert_eq!(ea_exe(r"<runtime><filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\X]\bin\game.exe</filePath></runtime>").as_deref(), Some(r"bin\game.exe"));
        let r = parse_reg("HKEY_LOCAL_MACHINE\\U\\Diablo IV\r\n    Publisher    REG_SZ    Blizzard Entertainment\r\n    DisplayName    REG_SZ    Diablo IV\r\n    DisplayIcon    REG_SZ    \"C:\\Games\\D4\\Diablo IV.exe\",0\r\n");
        assert_eq!(battlenet_from(&r)[0].title, "Diablo IV");
        let x = xbox_from("Fam_abc", r#"<Game><Identity Name="N"/><ShellVisuals DefaultDisplayName="Forza" /><ExecutableList><Executable Name="f.exe" Id="Game"/></ExecutableList></Game>"#).unwrap();
        assert_eq!(x.title, "Forza");
        assert!(xbox_from("p", r#"<Game><ShellVisuals DefaultDisplayName="ms-resource:Name"/></Game>"#).is_none());
    }
}
