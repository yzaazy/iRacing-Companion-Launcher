//! Ports `managers/process_tracker.py`: launches apps and tracks their process trees across restarts.

use std::path::PathBuf;
use std::time::Duration;

/// Placeholder fields: `contracts.md` leaves the internals to the process agent.
#[derive(Debug)]
pub struct ProcessTracker {
    state_file: PathBuf,
}

impl ProcessTracker {
    pub fn new(state_file: PathBuf) -> Self {
        todo!()
    }

    pub fn launch_and_track(
        &mut self,
        key: &str,
        exe_path: &str,
        wait: Duration,
        needs_shell_launch: bool,
    ) -> bool {
        todo!()
    }

    pub fn is_tracked(&self, key: &str) -> bool {
        todo!()
    }

    pub fn is_tracked_running(&mut self, key: &str) -> (bool, usize) {
        todo!()
    }

    pub fn get_child_names(&mut self, key: &str) -> Vec<String> {
        todo!()
    }

    pub fn close_tracked(&mut self, key: &str) -> bool {
        todo!()
    }

    pub fn forget(&mut self, key: &str) {
        todo!()
    }
}
