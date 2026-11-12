//! Library half of the launcher; `main.rs` is the window shell.

pub mod app;
pub mod dialog;
pub mod managers;
pub mod model;
pub mod process;
pub mod ui;
mod win;
pub mod worker;

/// Config store, app and game definitions, log lines.
pub mod core {
    pub mod config;
    pub mod definitions;
    pub mod log;
}

/// Finding an app's or game's exe: common paths, Start Menu shortcuts, Steam.
pub mod detect {
    pub mod paths;
    pub mod shortcuts;
    pub mod steam;
}
