//! "Start with your computer": an autostart entry per system.
//! Linux: XDG autostart file. macOS: a LaunchAgent. Windows: the Run registry key.

use std::path::PathBuf;

const NAME: &str = "Superhuman";
#[cfg(target_os = "macos")]
const AGENT: &str = "app.superhuman.launcher";

fn exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "superhuman".into())
}

fn file() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        dirs::home_dir().map(|h| h.join("Library/LaunchAgents/app.superhuman.launcher.plist"))
    } else if cfg!(windows) {
        None
    } else {
        dirs::config_dir().map(|d| d.join("autostart/superhuman.desktop"))
    }
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

pub fn enabled() -> bool {
    if cfg!(windows) {
        return crate::sources::hidden("reg").args(["query", RUN_KEY, "/v", NAME]).output().is_ok_and(|o| o.status.success());
    }
    file().is_some_and(|f| f.is_file())
}

pub fn set(on: bool) -> Result<(), String> {
    let exe = exe();
    if cfg!(windows) {
        let cmd = format!("\"{exe}\" --fullscreen");
        let args: Vec<&str> =
            if on { vec!["add", RUN_KEY, "/v", NAME, "/d", &cmd, "/f"] } else { vec!["delete", RUN_KEY, "/v", NAME, "/f"] };
        let ok = crate::sources::hidden("reg").args(&args).output().map_err(|e| e.to_string())?.status.success();
        return if ok || !on { Ok(()) } else { Err("couldn't write the registry".into()) };
    }
    let f = file().ok_or("no config folder")?;
    if !on {
        return match std::fs::remove_file(&f) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    #[cfg(target_os = "macos")]
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{AGENT}</string>\n<key>ProgramArguments</key><array><string>{exe}</string><string>--fullscreen</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n"
    );
    #[cfg(not(target_os = "macos"))]
    let text = format!("[Desktop Entry]\nType=Application\nName={NAME}\nComment=Game launcher\nExec=\"{exe}\" --fullscreen\nX-GNOME-Autostart-enabled=true\n");
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f, text).map_err(|e| e.to_string())
}
