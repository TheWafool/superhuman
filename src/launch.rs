//! Starting games and knowing when they've closed.
//!
//! Every launch runs inside its own `systemd-run --user --scope`, so the whole process tree is
//! tracked even when a launcher script exits early. Launchers that hand the game to an already
//! running client (Steam, Heroic) are tracked by a `watch` pattern in the game's command line
//! instead, e.g. Steam's `reaper SteamLaunch AppId=<id>`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::sources::{LaunchSpec, hidden, on_path};

/// How long a watched game may take to appear (Steam may update or sync first).
const WATCH_GRACE: Duration = Duration::from_secs(180);
/// An unwatched launch that exits sooner than this handed off to another launcher.
const HANDOFF: Duration = Duration::from_secs(8);

/// Command-line text that identifies a running game, per source. Data, not logic.
pub fn watch_pattern(game_id: &str) -> Option<String> {
    let (source, id) = game_id.split_once(':')?;
    match source {
        "steam" if id.chars().all(|c| c.is_ascii_digit()) => Some(format!("AppId={id}")),
        // Heroic ids are `runner:appName`; legendary/gogdl/nile put the appName on their argv.
        "heroic" => id.split_once(':').map(|(_, app)| app.to_owned()),
        _ => None,
    }
}

/// How a game that runs outside our own process tree is recognised.
#[derive(Debug, Clone, PartialEq)]
enum Watch {
    /// A word on its command line (Linux: Steam's reaper, Heroic's runners).
    Cmd(String),
    /// A process started from its install folder (Windows/macOS launchers).
    Dir(PathBuf),
}

/// Pick how to follow this game. Linux has exact command-line patterns; elsewhere the install
/// folder is the dependable sign (Steam's `AppId=` only exists on Linux).
fn watch_for(game_id: &str, watch_dir: Option<&str>) -> Option<Watch> {
    let pattern = watch_pattern(game_id).filter(|_| cfg!(target_os = "linux") || !game_id.starts_with("steam:"));
    let dir = watch_dir.filter(|d| !d.is_empty()).map(|d| Watch::Dir(PathBuf::from(d)));
    if cfg!(target_os = "linux") { pattern.map(Watch::Cmd).or(dir) } else { dir.or(pattern.map(Watch::Cmd)) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Starting,
    Running,
    /// Closed after running; carries the play time.
    Ended(Duration),
    /// Never seen running (failed, or handed off to a launcher we can't follow).
    Lost,
}

pub struct Session {
    pub game_id: String,
    pub title: String,
    pub launched: Instant,
    /// When the game was first seen running.
    pub running_since: Option<Instant>,
    pid: u32,
    unit: Option<String>,
    /// Running in its own process group (no systemd).
    grouped: bool,
    /// Windows Job Object holding the game's whole process tree (0 = none).
    #[cfg(windows)]
    job: isize,
    cgroup: Option<PathBuf>,
    exited: Arc<AtomicBool>,
    watch: Option<Watch>,
    /// The player asked us to close it.
    pub stopped: bool,
}

pub fn start(game_id: &str, title: &str, spec: &LaunchSpec) -> Result<Session, String> {
    let LaunchSpec::Command { program, args, env, cwd, watch_dir } = spec;
    let scoped = on_path("systemd-run");
    let unit = scoped.then(|| unit_name(game_id));
    // Without systemd (Void, Alpine, Gentoo, macOS) the game gets its own process group instead,
    // which `kill -0 -- -pgid` can watch and `kill -- -pgid` can stop.
    let grouped = unit.is_none() && cfg!(unix);
    let mut cmd = match &unit {
        Some(u) => {
            let mut c = Command::new("systemd-run");
            c.args(["--user", "--scope", "--quiet", "--collect", &format!("--unit={u}"), "--"]).arg(program);
            c
        }
        None => Command::new(program),
    };
    cmd.args(args).envs(env.iter().map(|(k, v)| (k, v)));
    #[cfg(unix)]
    if grouped {
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    }
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let pid = child.id();
    #[cfg(windows)]
    let job = job::adopt(&child);
    let exited = Arc::new(AtomicBool::new(false));
    let flag = exited.clone();
    // Reap it so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
        flag.store(true, Ordering::Relaxed);
    });
    Ok(Session {
        game_id: game_id.to_owned(),
        title: title.to_owned(),
        launched: Instant::now(),
        running_since: None,
        pid,
        unit,
        grouped,
        #[cfg(windows)]
        job,
        cgroup: None,
        exited,
        watch: watch_for(game_id, watch_dir.as_deref()),
        stopped: false,
    })
}

fn unit_name(game_id: &str) -> String {
    let safe: String = game_id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(48).collect();
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("superhuman-{safe}-{ts}")
}

impl Session {
    /// Check on the game. Cheap enough to call once a second.
    pub fn poll(&mut self) -> Status {
        let now = Instant::now();
        let scope_alive = self.scope_alive();
        let watched = self.watch.as_ref().map(|w| !find(w).is_empty());

        let alive = match watched {
            // A watched game is judged only by the pattern: the scope may hold a whole Steam client.
            Some(seen) => seen,
            None => scope_alive,
        };
        if alive {
            let quick = self.watch.is_none() && now - self.launched < Duration::from_millis(1500);
            if self.running_since.is_none() && !quick {
                self.running_since = Some(now);
            }
            return if self.running_since.is_some() { Status::Running } else { Status::Starting };
        }
        match self.running_since {
            Some(t) => Status::Ended(now - t),
            None if self.watch.is_some() && now - self.launched < WATCH_GRACE => Status::Starting,
            None if self.watch.is_none() && now - self.launched < HANDOFF && scope_alive => Status::Starting,
            None => Status::Lost,
        }
    }

    fn scope_alive(&mut self) -> bool {
        if self.cgroup.is_none() {
            self.cgroup = self.resolve_cgroup();
        }
        match &self.cgroup {
            Some(cg) => std::fs::read_to_string(cg.join("cgroup.procs")).is_ok_and(|s| !s.trim().is_empty()),
            #[cfg(windows)]
            None if self.job != 0 => job::active(self.job),
            None if self.grouped => Command::new("kill").args(["-0", "--", &format!("-{}", self.pid)]).stderr(Stdio::null()).status().is_ok_and(|s| s.success()),
            None => !self.exited.load(Ordering::Relaxed),
        }
    }

    fn resolve_cgroup(&self) -> Option<PathBuf> {
        let unit = self.unit.as_ref()?;
        let from_proc = std::fs::read_to_string(format!("/proc/{}/cgroup", self.pid))
            .ok()
            .and_then(|s| s.lines().find_map(|l| l.strip_prefix("0::").map(str::to_owned)))
            .filter(|p| p.contains(unit.as_str()));
        let path = from_proc.or_else(|| {
            let out = Command::new("systemctl")
                .args(["--user", "show", "-p", "ControlGroup", "--value", &format!("{unit}.scope")])
                .output()
                .ok()?;
            let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            (!s.is_empty()).then_some(s)
        })?;
        Some(PathBuf::from("/sys/fs/cgroup").join(path.trim_start_matches('/')))
    }

    /// Ask the game to close. Watched games get SIGTERM on the matched processes (never the
    /// launcher client); scoped games have their whole scope stopped.
    pub fn stop(&mut self) {
        self.stopped = true;
        if let Some(p) = &self.watch {
            for pid in find(p) {
                    terminate(pid);
            }
        } else if let Some(u) = &self.unit {
            let _ = Command::new("systemctl").args(["--user", "stop", &format!("{u}.scope")]).status();
        } else if self.job_stop() {
        } else if self.grouped {
            let _ = Command::new("kill").args(["-TERM", "--", &format!("-{}", self.pid)]).status();
        } else {
            terminate(self.pid);
        }
    }
}

impl Session {
    #[cfg(windows)]
    fn job_stop(&self) -> bool {
        self.job != 0 && job::terminate(self.job)
    }
    #[cfg(not(windows))]
    fn job_stop(&self) -> bool {
        false
    }
}

/// Windows: the child and everything it starts live in one Job Object, so a launcher that exits
/// early doesn't lose the game, and closing it ends the whole tree.
#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
        QueryInformationJobObject, TerminateJobObject,
    };

    pub fn adopt(child: &std::process::Child) -> isize {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() || AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0 {
                return 0;
            }
            job as isize
        }
    }

    pub fn active(job: isize) -> bool {
        unsafe {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                job as _,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut _,
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            );
            ok != 0 && info.ActiveProcesses > 0
        }
    }

    pub fn terminate(job: isize) -> bool {
        unsafe { TerminateJobObject(job as _, 1) != 0 }
    }
}

fn terminate(pid: u32) {
    let pid = pid.to_string();
    let _ = if cfg!(windows) {
        hidden("taskkill").args(["/PID", &pid, "/T"]).status()
    } else {
        Command::new("kill").args(["-TERM", &pid]).status()
    };
}

/// Pids of the watched game (never the launcher client or us).
fn find(w: &Watch) -> Vec<u32> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let me = std::process::id();
    let Ok(mut sys) = System::new() else { return Vec::new() };
    let what = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_exe(UpdateKind::Always);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, what);
    sys.processes()
        .iter()
        .filter(|(pid, _)| pid.as_u32() != me)
        .filter(|(_, p)| match w {
            Watch::Cmd(pattern) => {
                let cmd = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
                matches_cmd(&format!("{cmd} "), pattern)
            }
            Watch::Dir(dir) => p.exe().is_some_and(|exe| in_dir(exe, dir)),
        })
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

/// Is `exe` inside `dir`? Ignores case and slash style on Windows.
fn in_dir(exe: &std::path::Path, dir: &std::path::Path) -> bool {
    if cfg!(windows) {
        let norm = |p: &std::path::Path| p.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_owned();
        let (e, d) = (norm(exe), norm(dir));
        e.starts_with(&d) && e[d.len()..].starts_with('\\')
    } else {
        exe.starts_with(dir)
    }
}

fn matches_cmd(cmd: &str, pattern: &str) -> bool {
    if cmd.contains("steam://") || cmd.contains("heroic://") {
        return false;
    }
    cmd.split(' ').any(|w| w == pattern || w.ends_with(&format!("/{pattern}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert_eq!(watch_pattern("steam:367520").as_deref(), Some("AppId=367520"));
        assert_eq!(watch_pattern("heroic:legendary:Fortnite").as_deref(), Some("Fortnite"));
        assert_eq!(watch_pattern("desktop:wesnoth"), None);
        let reaper = "/home/u/.steam/ubuntu12_32/reaper SteamLaunch AppId=367520 -- /x/hollow_knight.x86_64 ";
        assert!(matches_cmd(reaper, "AppId=367520"));
        assert!(!matches_cmd(reaper, "AppId=36752"));
        assert!(!matches_cmd("steam steam://rungameid/367520 ", "AppId=367520"));
        assert!(matches_cmd("legendary launch Fortnite --wine x ", "Fortnite"));
        assert!(!matches_cmd("heroic --no-gui heroic://launch/legendary/Fortnite ", "Fortnite"));
        assert!(in_dir(std::path::Path::new("/g/steamapps/common/HK/hk"), std::path::Path::new("/g/steamapps/common/HK")));
        assert!(!in_dir(std::path::Path::new("/g/steamapps/common/HK2/hk"), std::path::Path::new("/g/steamapps/common/HK")));
        let w = watch_for("steam:1", Some("/x"));
        assert_eq!(w, if cfg!(target_os = "linux") { Some(Watch::Cmd("AppId=1".into())) } else { Some(Watch::Dir("/x".into())) });
    }
}
