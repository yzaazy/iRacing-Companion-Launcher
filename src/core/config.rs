//! Ports `core/config_manager.py`: `config.ini` in Python `configparser` format.

use std::path::{Path, PathBuf};

/// Placeholder fields: `contracts.md` leaves the internals to the core agent.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigStore {
    dir: PathBuf,
}

pub fn resolve_config_dir() -> PathBuf {
    todo!()
}

pub fn config_key(name: &str) -> String {
    todo!()
}

impl ConfigStore {
    pub fn load(dir: &Path) -> Self {
        todo!()
    }

    pub fn config_dir(&self) -> &Path {
        &self.dir
    }

    /// Only if it exists on disk.
    pub fn app_path(&self, key: &str) -> Option<String> {
        todo!()
    }

    /// Saves.
    pub fn set_app_path(&mut self, key: &str, path: &str) -> bool {
        todo!()
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        todo!()
    }

    pub fn bool_setting(&self, key: &str, fallback: bool) -> bool {
        todo!()
    }

    /// Does not save.
    pub fn set_setting(&mut self, key: &str, value: &str) {
        todo!()
    }

    pub fn save(&self) -> bool {
        todo!()
    }
}
