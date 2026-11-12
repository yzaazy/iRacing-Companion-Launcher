//! Library half of the launcher (ports the `iracing_launcher_app` package); `main.rs` is the window shell.

pub mod app;
pub mod dialog;
pub mod managers;
pub mod model;
pub mod process;
pub mod ui;
mod win;
pub mod worker;

/// Ports `iracing_launcher_app/core/`.
pub mod core {
    pub mod config;
    pub mod definitions;
    pub mod log;
}

/// Ports `iracing_launcher_app/utils/`.
pub mod detect {
    pub mod paths;
    pub mod shortcuts;
    pub mod steam;
}
