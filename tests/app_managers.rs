//! The real managers, limited to what cannot reach a real process: saved-path
//! detection, unknown names, and launches of paths that do not exist.
//! Name-based running checks and kills would match the developer's real
//! apps, so they are covered by the transcript tests through the fake backend.

use iracing_companion_launcher::core::config::{ConfigStore, SharedConfig};
use iracing_companion_launcher::managers::{AppManager, Backend, GameManager, Managers};
use iracing_companion_launcher::process::tracker::ProcessTracker;

fn config_in(dir: &std::path::Path, text: &str) -> SharedConfig {
    std::fs::write(dir.join("config.ini"), text).unwrap();
    SharedConfig::new(ConfigStore::load(dir))
}

fn tracker_in(dir: &std::path::Path) -> ProcessTracker {
    ProcessTracker::new(dir.join("process_state.json"))
}

#[test]
fn saved_app_path_wins_without_a_save() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("Fanatec.exe");
    std::fs::write(&exe, b"").unwrap();
    let text = format!("[AppPaths]\nfanatec = {}\n\n[Settings]\n\n", exe.display());
    let config = config_in(dir.path(), &text);
    let mut apps = AppManager::new(config, tracker_in(dir.path()));

    assert_eq!(
        apps.find_app_path("Fanatec"),
        Some(exe.display().to_string())
    );
    let after = std::fs::read_to_string(dir.path().join("config.ini")).unwrap();
    assert_eq!(
        after, text,
        "a saved path is returned without writing config"
    );
}

#[test]
fn saved_game_path_wins_before_steam() {
    let dir = tempfile::tempdir().unwrap();
    let lnk = dir.path().join("iRacing.lnk");
    std::fs::write(&lnk, b"").unwrap();
    let text = format!(
        "[AppPaths]\ngame_iracing = {}\n\n[Settings]\n\n",
        lnk.display()
    );
    let mut games = GameManager::new(config_in(dir.path(), &text));
    assert_eq!(
        games.find_game_path("iRacing"),
        Some(lnk.display().to_string())
    );
}

#[test]
fn unknown_names_answer_like_python() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_in(dir.path(), "");
    let mut managers = Managers::new(config, tracker_in(dir.path()));
    let name = "Not An App";
    assert_eq!(managers.app_exe(name), None);
    assert_eq!(managers.find_app_path(name), None);
    assert!(!managers.is_app_running(name));
    assert!(managers.get_child_names(name).is_empty());
    assert!(!managers.launch_app(name, "C:/x.exe"));
    assert!(!managers.close_app(name));
    assert_eq!(managers.game_exe(name), None);
    assert_eq!(managers.find_game_path(name), None);
    assert!(!managers.is_game_running(name));
    assert!(!managers.launch_game(name, "C:/x.exe"));
}

#[test]
fn lists_follow_the_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let managers = Managers::new(config_in(dir.path(), ""), tracker_in(dir.path()));
    assert_eq!(managers.app_list()[0], "Fanatec");
    assert_eq!(managers.app_list().len(), 7);
    assert_eq!(managers.game_list()[4], "rFactor 2");
    assert_eq!(managers.app_exe("SimHub").as_deref(), Some("SimHubWPF.exe"));
    assert_eq!(
        managers.game_exe("iRacing").as_deref(),
        Some("iRacingUI.exe")
    );
}

/// A missing file fails to start, so nothing runs and nothing is tracked.
#[test]
fn launching_a_missing_exe_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope").join("missing.exe");
    let missing = missing.to_str().unwrap();
    let mut managers = Managers::new(config_in(dir.path(), ""), tracker_in(dir.path()));
    assert!(!managers.launch_app("Fanatec", missing));
    assert!(!dir.path().join("process_state.json").exists());
    assert!(!managers.launch_game("iRacing", missing));
}
