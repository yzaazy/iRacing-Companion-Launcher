//! `resolve_config_dir`: where `config.ini` and `process_state.json` live.
//!
//! One test in its own binary: it sets an environment variable, which is only
//! sound while no other thread reads the environment.

use iracing_companion_launcher::core::config::resolve_config_dir;
use std::path::Path;

#[test]
#[cfg(debug_assertions)]
fn override_wins_then_debug_builds_use_target_dev_config() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("nested").join("config");

    // SAFETY: the only test in this binary, so no other thread touches the environment.
    unsafe { std::env::set_var("IRCL_CONFIG_DIR", &dir) };
    assert_eq!(resolve_config_dir(), dir);
    assert!(dir.is_dir(), "created if missing");

    // An empty value counts as unset.
    // SAFETY: still the only thread of the only test in this binary.
    unsafe { std::env::set_var("IRCL_CONFIG_DIR", "") };
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("dev-config");
    assert_eq!(resolve_config_dir(), dev);
    assert!(dev.is_dir());

    // SAFETY: still the only thread of the only test in this binary.
    unsafe { std::env::remove_var("IRCL_CONFIG_DIR") };
    assert_eq!(resolve_config_dir(), dev);
}
