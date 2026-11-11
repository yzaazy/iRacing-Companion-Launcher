//! Library half of the launcher (ports the `iracing_launcher_app` package); `main.rs` is the window shell.

// Scaffold only: module bodies are stubs until wave 1 fills them in, so their
// parameters and private fields are unused. Removed in wave 2 when the
// controller wires everything together.
#![allow(dead_code, unused_variables)]

pub mod app;
pub mod contract;
pub mod managers;
pub mod process;
pub mod ui;
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
