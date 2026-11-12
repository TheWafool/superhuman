//! Sleep, restart and shut down, per system. Data, not logic.

use std::process::Command;

use crate::sources::on_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Sleep,
    Restart,
    ShutDown,
}

/// The command for each action on this system.
fn command(p: Power) -> (&'static str, &'static [&'static str]) {
    if cfg!(windows) {
        match p {
            Power::Sleep => ("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"]),
            Power::Restart => ("shutdown", &["/r", "/t", "0"]),
            Power::ShutDown => ("shutdown", &["/s", "/t", "0"]),
        }
    } else if cfg!(target_os = "macos") {
        match p {
            Power::Sleep => ("pmset", &["sleepnow"]),
            Power::Restart => ("osascript", &["-e", "tell application \"System Events\" to restart"]),
            Power::ShutDown => ("osascript", &["-e", "tell application \"System Events\" to shut down"]),
        }
    } else {
        // systemd, or elogind's loginctl on distros without it.
        let tool = if on_path("systemctl") { "systemctl" } else { "loginctl" };
        match p {
            Power::Sleep => (tool, &["suspend"]),
            Power::Restart => (tool, &["reboot"]),
            Power::ShutDown => (tool, &["poweroff"]),
        }
    }
}

pub fn run(p: Power) -> Result<(), String> {
    let (prog, args) = command(p);
    Command::new(prog).args(args).spawn().map(|_| ()).map_err(|e| format!("{prog}: {e}"))
}

/// How to leave a console-style session for the desktop, if this system has a way
/// (SteamOS-style `steamos-session-select`). Otherwise quitting ends the session.
pub fn desktop_command() -> Option<Vec<String>> {
    on_path("steamos-session-select").then(|| vec!["steamos-session-select".into(), "plasma".into()])
}
