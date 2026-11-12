//! Emulation: ROM folders → systems → the first installed emulator for each.
//! All of it is data in `data/emulators.toml` (plus the user's own `emulators.toml`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Discovered, LaunchSpec, flatpak_installed, home, which};
use crate::library::Source;

const BUILT_IN: &str = include_str!("../../data/emulators.toml");
/// Deep enough for `psx/Game (Disc 1)/game.cue`, shallow enough to stay quick.
const MAX_DEPTH: usize = 3;
const MAX_FILES: usize = 50_000;

#[derive(Debug, Default, Deserialize)]
struct Data {
    #[serde(default)]
    rom_dirs: Vec<String>,
    retroarch: Option<RetroArch>,
    #[serde(default)]
    system: Vec<System>,
}

#[derive(Debug, Clone, Deserialize)]
struct RetroArch {
    programs: Vec<String>,
    flatpak: Option<String>,
    core_dirs: Vec<String>,
    args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct System {
    id: String,
    #[allow(dead_code)]
    name: String,
    folders: Vec<String>,
    extensions: Vec<String>,
    emulators: Vec<Emu>,
}

#[derive(Debug, Clone, Deserialize)]
struct Emu {
    core: Option<String>,
    program: Option<String>,
    flatpak: Option<String>,
    mac_app: Option<String>,
    args: Option<Vec<String>>,
}

/// A resolved launch template; `{rom}` in `args` is replaced per game.
#[derive(Debug, Clone)]
struct Template {
    program: String,
    args: Vec<String>,
}

pub fn scan() -> Result<Vec<Discovered>, String> {
    let mut data: Data = toml::from_str(BUILT_IN).map_err(|e| format!("built-in emulators.toml: {e}"))?;
    let user = crate::config::path().with_file_name("emulators.toml");
    if let Ok(text) = std::fs::read_to_string(&user) {
        let u: Data = toml::from_str(&text).map_err(|e| format!("{}: {e}", user.display()))?;
        merge(&mut data, u);
    }
    let ra = data.retroarch.as_ref().and_then(find_retroarch);
    let mut seen = HashSet::new();
    let dirs: Vec<PathBuf> = data
        .rom_dirs
        .iter()
        .map(|d| expand(d))
        .filter(|d| d.is_dir() && seen.insert(d.canonicalize().unwrap_or_else(|_| d.clone())))
        .collect();
    let mut cache: Vec<(String, Option<Template>)> = Vec::new();
    let mut out = Vec::new();
    for dir in dirs {
        for (sys, files) in roms(&dir, &data.system) {
            let t = match cache.iter().find(|(id, _)| *id == sys.id) {
                Some((_, t)) => t.clone(),
                None => {
                    let t = resolve(sys, data.retroarch.as_ref(), ra.as_ref());
                    cache.push((sys.id.clone(), t.clone()));
                    t
                }
            };
            // No emulator installed for this system: nothing we could start.
            let Some(t) = t else { continue };
            for f in files {
                let rom = f.to_string_lossy().into_owned();
                out.push(Discovered {
                    source: Source::Emulation,
                    source_id: format!("{}:{rom}", sys.id),
                    title: title(&f),
                    launch: LaunchSpec::cmd(&t.program, t.args.iter().map(|a| a.replace("{rom}", &rom))),
                    minutes_played: 0,
                    last_played: None,
                    steam_appid: None,
                });
            }
        }
    }
    Ok(out)
}

fn merge(base: &mut Data, user: Data) {
    base.rom_dirs.extend(user.rom_dirs);
    if user.retroarch.is_some() {
        base.retroarch = user.retroarch;
    }
    for s in user.system {
        match base.system.iter_mut().find(|b| b.id == s.id) {
            Some(b) => *b = s,
            None => base.system.push(s),
        }
    }
}

fn expand(p: &str) -> PathBuf {
    let s = |d: Option<PathBuf>| d.unwrap_or_default().to_string_lossy().into_owned();
    PathBuf::from(p.replace("{home}", &s(Some(home()))).replace("{config}", &s(dirs::config_dir())).replace("{data}", &s(dirs::data_dir())))
}

/// Program on PATH, or a full path that exists.
fn find_program(p: &str) -> Option<String> {
    if p.contains('/') || p.contains('\\') {
        let e = expand(p);
        e.is_file().then(|| e.to_string_lossy().into_owned())
    } else {
        which(p).map(|w| w.to_string_lossy().into_owned())
    }
}

/// (program and leading args, core dirs to search)
struct FoundRetroArch {
    run: Template,
    core_dirs: Vec<PathBuf>,
}

fn find_retroarch(ra: &RetroArch) -> Option<FoundRetroArch> {
    let mut core_dirs: Vec<PathBuf> = ra.core_dirs.iter().map(|d| expand(d)).filter(|d| d.is_dir()).collect();
    if let Some(p) = ra.programs.iter().find_map(|p| find_program(p)) {
        // Windows installs keep cores beside the exe.
        if let Some(d) = Path::new(&p).parent().map(|d| d.join("cores")).filter(|d| d.is_dir()) {
            core_dirs.push(d);
        }
        return Some(FoundRetroArch { run: Template { program: p, args: Vec::new() }, core_dirs });
    }
    let id = ra.flatpak.as_deref().filter(|id| flatpak_installed(id))?;
    Some(FoundRetroArch { run: Template { program: "flatpak".into(), args: vec!["run".into(), id.into()] }, core_dirs })
}

fn resolve(sys: &System, ra_def: Option<&RetroArch>, ra: Option<&FoundRetroArch>) -> Option<Template> {
    let rom_args = |e: &Emu| e.args.clone().unwrap_or_else(|| vec!["{rom}".into()]);
    sys.emulators.iter().find_map(|e| {
        if let Some(core) = &e.core {
            let (ra, def) = (ra?, ra_def?);
            let file = format!("{core}_libretro.{}", std::env::consts::DLL_EXTENSION);
            let core = ra.core_dirs.iter().map(|d| d.join(&file)).find(|p| p.is_file())?;
            let core = core.to_string_lossy();
            let mut args = ra.run.args.clone();
            args.extend(def.args.iter().map(|a| a.replace("{core}", &core)));
            Some(Template { program: ra.run.program.clone(), args })
        } else if let Some(p) = &e.program {
            Some(Template { program: find_program(p)?, args: rom_args(e) })
        } else if let Some(id) = &e.flatpak {
            flatpak_installed(id).then(|| Template { program: "flatpak".into(), args: [vec!["run".into(), id.clone()], rom_args(e)].concat() })
        } else if let Some(app) = &e.mac_app {
            let ok = cfg!(target_os = "macos") && Path::new(&format!("/Applications/{app}.app")).is_dir();
            ok.then(|| Template { program: "open".into(), args: [vec!["-W".into(), "-a".into(), app.clone(), "--args".into()], rom_args(e)].concat() })
        } else {
            None
        }
    })
}

/// ROM files under `dir`, grouped by system. Sub-folders are matched to systems by name; loose
/// files only when their extension belongs to a single system.
fn roms<'a>(dir: &Path, systems: &'a [System]) -> Vec<(&'a System, Vec<PathBuf>)> {
    let mut out: Vec<(&System, Vec<PathBuf>)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let mut loose = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if let Some(sys) = systems.iter().find(|s| s.id == name || s.folders.iter().any(|f| *f == name)) {
                let mut files = Vec::new();
                walk(&p, sys, 0, &mut files);
                files.sort();
                out.push((sys, files));
            }
        } else {
            loose.push(p);
        }
    }
    for p in loose {
        let Some(x) = ext(&p) else { continue };
        let mut owners = systems.iter().filter(|s| s.extensions.contains(&x));
        if let (Some(sys), None) = (owners.next(), owners.next()) {
            match out.iter_mut().find(|(s, _)| s.id == sys.id) {
                Some((_, f)) => f.push(p),
                None => out.push((sys, vec![p])),
            }
        }
    }
    out
}

fn walk(dir: &Path, sys: &System, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < MAX_DEPTH {
                walk(&p, sys, depth + 1, out);
            }
        } else if ext(&p).is_some_and(|x| sys.extensions.contains(&x)) {
            files.push(p);
        }
    }
    // A playlist stands for its discs.
    if files.iter().any(|p| ext(p).as_deref() == Some("m3u")) {
        files.retain(|p| ext(p).as_deref() == Some("m3u"));
    }
    if out.len() + files.len() <= MAX_FILES {
        out.extend(files);
    }
}

fn ext(p: &Path) -> Option<String> {
    p.extension().map(|x| x.to_string_lossy().to_lowercase())
}

/// "Legend of Zelda, The - A Link to the Past (USA) [!].sfc" → "The Legend of Zelda - A Link to the Past"
fn title(p: &Path) -> String {
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut out = String::new();
    let mut depth = 0i32;
    for c in stem.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let t = out.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    for article in ["The", "A", "An"] {
        let tag = format!(", {article}");
        if let Some(i) = t.find(&tag) {
            let after = &t[i + tag.len()..];
            if after.is_empty() || after.starts_with(" -") || after.starts_with(':') {
                return format!("{article} {}{after}", &t[..i]);
            }
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_data_parses() {
        let d: Data = toml::from_str(BUILT_IN).unwrap();
        assert!(d.system.len() > 15);
        assert!(d.system.iter().all(|s| !s.emulators.is_empty() && !s.extensions.is_empty()));
    }

    #[test]
    fn titles() {
        let t = |s: &str| title(Path::new(s));
        assert_eq!(t("Legend of Zelda, The - A Link to the Past (USA) [!].sfc"), "The Legend of Zelda - A Link to the Past");
        assert_eq!(t("Super_Mario_World (U).smc"), "Super Mario World");
        assert_eq!(t("Final Fantasy VII (Disc 1).cue"), "Final Fantasy VII");
    }

    #[test]
    fn finds_roms() {
        let d: Data = toml::from_str(BUILT_IN).unwrap();
        let dir = std::env::temp_dir().join(format!("superhuman-roms-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for f in ["SNES/a.sfc", "snes/notes.txt", "psx/FF7/ff7.m3u", "psx/FF7/ff7 (Disc 1).chd", "psx/Crash.chd", "loose.gba", "loose.zip"] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "").unwrap();
        }
        let mut got: Vec<(String, usize)> = roms(&dir, &d.system).into_iter().map(|(s, f)| (s.id.clone(), f.len())).collect();
        got.sort();
        let _ = std::fs::remove_dir_all(&dir);
        // Case-insensitive folders mean SNES and snes may both exist; count snes files overall.
        let n = |id: &str| got.iter().filter(|(s, _)| s == id).map(|(_, n)| n).sum::<usize>();
        assert_eq!(n("snes"), 1);
        assert_eq!(n("psx"), 2, "playlist replaces its discs");
        assert_eq!(n("gba"), 1);
        assert_eq!(n("arcade"), 0, "zip is ambiguous when loose");
    }
}
