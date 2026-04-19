//! Local `pulsex-server` child process (loopback DEX UI). Separate from the dApp WebView process.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

static PULSEX_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Child>> {
    PULSEX_CHILD.get_or_init(|| Mutex::new(None))
}

/// Best-effort: `true` if our spawned server is still running.
pub fn is_pulsex_running() -> bool {
    let Ok(mut g) = slot().lock() else {
        return false;
    };
    let Some(ref mut c) = *g else {
        return false;
    };
    match c.try_wait() {
        Ok(Some(_)) => {
            *g = None;
            false
        }
        Ok(None) => true,
        Err(_) => false,
    }
}

pub fn stop_pulsex_local() {
    if let Ok(mut g) = slot().lock() {
        if let Some(mut c) = g.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub fn start_pulsex_local(binary: &Path, server_bind: &str) -> Result<(), String> {
    if !binary.as_os_str().is_empty() && !binary.exists() {
        return Err(format!(
            "pulsex-server binary not found at {}. Re-run Install.",
            binary.display()
        ));
    }

    stop_pulsex_local();

    let workdir = binary.parent().filter(|p| p.as_os_str().len() > 0);

    let mut cmd = Command::new(binary);
    cmd.arg("-s")
        .arg(server_bind)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start pulsex-server: {}", e))?;

    // If the binary exits immediately (port taken, bad bind, etc.), don't claim success.
    std::thread::sleep(Duration::from_millis(120));
    match child.try_wait() {
        Ok(Some(status)) => {
            return Err(format!(
                "pulsex-server exited immediately (status: {status}). \
                 Often the port is already in use — try Stop server, or run: fuser -k 3691/tcp"
            ));
        }
        Ok(None) => { /* still running */ }
        Err(e) => {
            return Err(format!("Could not query pulsex-server status: {}", e));
        }
    }

    if let Ok(mut g) = slot().lock() {
        *g = Some(child);
    }
    tracing::info!(
        target: "vaughan_app",
        "pulsex-server started (bind {})",
        server_bind
    );
    Ok(())
}

/// Kills the local PulseX server on wallet exit (same pattern as [`crate::browser::BrowserProcessGuard`]).
pub struct PulsexServerGuard;

impl Drop for PulsexServerGuard {
    fn drop(&mut self) {
        stop_pulsex_local();
    }
}
