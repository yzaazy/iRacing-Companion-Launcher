//! Ports `managers/process_manager.py`: name-based process checks, kills and launches.
//!
//! The Python delegates to `psutil`; [`sys`] reproduces what psutil does on
//! Windows (which Win32 calls, which access rights, which error counts as "no
//! such process" and which as "access denied"), because those details decide
//! what the launcher sees.

mod state_file;
mod sys;
pub mod tracker;

use std::time::Duration;

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use crate::win::wide;
use sys::{identity, kill_if_same, snapshot, spawn_detached};

pub fn is_process_running(exe_name: &str) -> bool {
    let wanted = exe_name.to_lowercase();
    snapshot().iter().any(|p| p.name.to_lowercase() == wanted)
}

/// Kills every process with this exe name. True if at least one kill went through.
///
/// As `psutil.process_iter` + `Process.kill()`: each listed process's identity
/// (its creation time) is taken as it is listed, and checked again just before
/// the kill, so a PID that was reused in between is treated as gone.
pub fn kill_process(exe_name: &str) -> bool {
    let wanted = exe_name.to_lowercase();
    let mut killed = false;
    for p in snapshot() {
        if p.name.to_lowercase() != wanted {
            continue;
        }
        // process_iter skips a process that is gone by the time it is listed.
        let Ok(identity) = identity(p.pid) else {
            continue;
        };
        if kill_if_same(p.pid, identity).is_ok() {
            killed = true;
        }
    }
    killed
}

/// Detached and untracked. True once the process was created and `wait` has passed;
/// Python does not check that it is still alive.
pub fn launch_process(exe_path: &str, wait: Duration) -> bool {
    match spawn_detached(exe_path) {
        Ok(_) => {
            std::thread::sleep(wait);
            true
        }
        Err(_) => false,
    }
}

/// `os.startfile(target)`: `ShellExecuteW` with no verb (the default verb, normally
/// "open"), no parameters, no working directory, `SW_SHOWNORMAL`.
pub fn shell_open(target: &str) -> std::io::Result<()> {
    let file = wide(target);
    // SAFETY: `file` is a NUL-terminated UTF-16 string alive for the whole
    // call; the other string arguments are null, which the call allows.
    let rc = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if rc.0 as usize > 32 {
        return Ok(());
    }
    // Python raises from GetLastError(); fall back to the return code when it is unset.
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(0) | None => Err(std::io::Error::from_raw_os_error(rc.0 as i32)),
        Some(_) => Err(err),
    }
}

/// PIDs of every process named `exe_name`, case-insensitive (`_pids_by_exe`).
pub(crate) fn pids_by_exe(exe_name: &str) -> Vec<u32> {
    let wanted = exe_name.to_lowercase();
    snapshot()
        .into_iter()
        .filter(|p| p.name.to_lowercase() == wanted)
        .map(|p| p.pid)
        .collect()
}
