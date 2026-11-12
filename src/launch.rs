//! Starting games. M2: fire-and-forget. Process tracking and playtime come in M4.

use std::process::{Command, Stdio};

use crate::sources::LaunchSpec;

pub fn start(spec: &LaunchSpec) -> Result<(), String> {
    match spec {
        LaunchSpec::Command { program, args } => {
            let mut child = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("{program}: {e}"))?;
            // Reap it so it doesn't linger as a zombie.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
    }
}
