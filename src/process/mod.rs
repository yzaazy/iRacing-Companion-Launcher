//! Ports `managers/process_manager.py`: name-based process checks, kills and launches.

pub mod tracker;

use std::time::Duration;

pub fn is_process_running(exe_name: &str) -> bool {
    todo!()
}

pub fn kill_process(exe_name: &str) -> bool {
    todo!()
}

/// Detached and untracked.
pub fn launch_process(exe_path: &str, wait: Duration) -> bool {
    todo!()
}

pub fn shell_open(target: &str) -> std::io::Result<()> {
    todo!()
}
