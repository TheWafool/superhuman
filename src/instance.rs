//! One copy at a time. A second `superhuman` (or `superhuman --launch <game id>`) hands its request
//! to the running one over a private socket and exits. Unix only for now; Windows runs without.

use std::sync::Mutex;

use iced::{Subscription, stream};

#[derive(Debug, Clone)]
pub enum Remote {
    /// Bring the window forward.
    Focus,
    /// Start this game (`steam:367520`, any copy's id).
    Launch(String),
}

impl Remote {
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        match args.iter().position(|a| a == "--launch").and_then(|i| args.get(i + 1)) {
            Some(id) => Remote::Launch(id.clone()),
            None => Remote::Focus,
        }
    }

    fn encode(&self) -> String {
        match self {
            Remote::Focus => "focus\n".into(),
            Remote::Launch(id) => format!("launch {id}\n"),
        }
    }

    fn decode(line: &str) -> Option<Self> {
        let line = line.trim();
        if line == "focus" {
            return Some(Remote::Focus);
        }
        line.strip_prefix("launch ").filter(|id| !id.is_empty() && id.len() < 512).map(|id| Remote::Launch(id.to_owned()))
    }
}

#[cfg(unix)]
static LISTENER: Mutex<Option<std::os::unix::net::UnixListener>> = Mutex::new(None);
#[cfg(not(unix))]
static LISTENER: Mutex<Option<std::net::TcpListener>> = Mutex::new(None);
/// Windows has no Unix sockets in std; a loopback port (taken exclusively) does the same job.
/// Any local program can reach a port, so requests must carry a random token kept in the
/// user's own (private) app-data folder.
#[cfg(not(unix))]
const ADDR: &str = "127.0.0.1:47619";
#[cfg(not(unix))]
static TOKEN: Mutex<String> = Mutex::new(String::new());

#[cfg(not(unix))]
fn token_file() -> std::path::PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("superhuman").join("instance.token")
}

#[cfg(unix)]
fn socket() -> std::path::PathBuf {
    // XDG_RUNTIME_DIR is private to the user (0700); elsewhere use the user's cache folder.
    dirs::runtime_dir().or_else(dirs::cache_dir).unwrap_or_else(std::env::temp_dir).join("superhuman.sock")
}

/// Hand `req` to a running copy. Returns true if one took it (this copy should exit).
/// Otherwise this copy becomes the one that listens.
#[cfg(unix)]
pub fn claim(req: &Remote) -> bool {
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};
    let path = socket();
    if let Ok(mut s) = UnixStream::connect(&path) {
        if s.write_all(req.encode().as_bytes()).is_ok() {
            return true;
        }
    }
    // Nobody listening: the file is stale.
    let _ = std::fs::remove_file(&path);
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    match UnixListener::bind(&path) {
        Ok(l) => {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            *LISTENER.lock().unwrap() = Some(l);
        }
        Err(e) => eprintln!("single instance: {}: {e}", path.display()),
    }
    false
}

#[cfg(not(unix))]
pub fn claim(req: &Remote) -> bool {
    use std::io::Write;
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::time::Duration;
    let addr: SocketAddr = ADDR.parse().unwrap();
    if let (Ok(mut s), Ok(token)) = (TcpStream::connect_timeout(&addr, Duration::from_millis(500)), std::fs::read_to_string(token_file())) {
        if s.write_all(format!("{} {}", token.trim(), req.encode()).as_bytes()).is_ok() {
            return true;
        }
    }
    match TcpListener::bind(addr) {
        Ok(l) => {
            use std::hash::{BuildHasher, Hasher};
            let token = format!("{:016x}{:016x}", std::hash::RandomState::new().build_hasher().finish(), std::hash::RandomState::new().build_hasher().finish());
            let f = token_file();
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            if std::fs::write(&f, &token).is_ok() {
                *TOKEN.lock().unwrap() = token;
                *LISTENER.lock().unwrap() = Some(l);
            }
        }
        Err(e) => eprintln!("single instance: {ADDR}: {e}"),
    }
    false
}

/// Requests from later copies.
pub fn requests() -> Subscription<Remote> {
    Subscription::run(|| {
        stream::channel(8, async |mut out| {
            use iced::futures::{SinkExt, StreamExt};
            let (tx, mut rx) = iced::futures::channel::mpsc::unbounded();
            #[cfg(unix)]
            if let Some(l) = LISTENER.lock().unwrap().take() {
                std::thread::spawn(move || {
                    use std::io::{BufRead, BufReader, Read};
                    for conn in l.incoming().flatten() {
                        let mut line = String::new();
                        // One short line, at most 1 KiB.
                        if BufReader::new(conn.take(1024)).read_line(&mut line).is_ok_and(|n| n > 0) {
                            if let Some(r) = Remote::decode(&line) {
                                let _ = tx.unbounded_send(r);
                            }
                        }
                    }
                });
            }
            #[cfg(not(unix))]
            if let Some(l) = LISTENER.lock().unwrap().take() {
                std::thread::spawn(move || {
                    use std::io::{BufRead, BufReader, Read};
                    for conn in l.incoming().flatten() {
                        let mut line = String::new();
                        if BufReader::new(conn.take(1024)).read_line(&mut line).is_ok_and(|n| n > 0) {
                            if let Some(r) = line.strip_prefix(&format!("{} ", TOKEN.lock().unwrap())).and_then(Remote::decode) {
                                let _ = tx.unbounded_send(r);
                            }
                        }
                    }
                });
            }
            while let Some(r) = rx.next().await {
                let _ = out.send(r).await;
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::Remote;

    #[test]
    fn round_trips() {
        assert!(matches!(Remote::decode(&Remote::Launch("steam:1".into()).encode()), Some(Remote::Launch(id)) if id == "steam:1"));
        assert!(matches!(Remote::decode("focus\n"), Some(Remote::Focus)));
        assert!(Remote::decode("rm -rf").is_none());
    }
}
