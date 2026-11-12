//! The UI tests, in one binary: the window against the Python reference and the measured spec,
//! its input handling, fonts and the log's cost.
//!
//! Tests that render share one wgpu device (`support`), so they run in parallel.

#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

mod fonts;
mod interaction;
mod keyboard;
mod layout;
mod log_gestures;
mod log_perf;
mod parity;
mod parity_check;
mod renders;
mod support;
