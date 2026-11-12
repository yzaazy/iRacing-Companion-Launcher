//! The definitions equal the Python app's up to v2.3.2, dumped into `tests/fixtures/core/definitions.json`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

use iracing_companion_launcher::core::definitions::{apps, games};
use serde_json::Value;
use std::path::Path;

fn python_data() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core/definitions.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn apps_match_python_in_order() {
    let data = python_data();
    let want = data["apps"].as_array().unwrap();
    let got = apps();
    assert_eq!(got.len(), want.len());
    for (app, py) in got.iter().zip(want) {
        assert_eq!(app.name, py["name"], "name");
        assert_eq!(app.exe, py["exe"], "{}: exe", app.name);
        assert_eq!(
            app.shortcut_names.to_vec(),
            strings(&py["shortcut_names"]),
            "{}: shortcut_names",
            app.name
        );
        assert_eq!(
            app.needs_shell_launch, py["needs_shell_launch"],
            "{}: needs_shell_launch",
            app.name
        );
        if app.name == "Garage61" {
            // Empty in app_definitions.py; AppManager fills it from %APPDATA%.
            assert!(strings(&py["paths"]).is_empty());
        } else {
            assert_eq!(app.paths, strings(&py["paths"]), "{}: paths", app.name);
        }
    }
}

#[test]
fn garage61_path_comes_from_appdata() {
    let appdata = std::env::var("APPDATA").expect("APPDATA is set on Windows");
    let garage61 = apps().into_iter().find(|a| a.name == "Garage61").unwrap();
    assert_eq!(
        garage61.paths,
        vec![format!(r"{appdata}\garage61-install\garage61-launcher.exe")]
    );
}

#[test]
fn games_match_python_in_order() {
    let data = python_data();
    let want = data["games"].as_array().unwrap();
    let got = games();
    assert_eq!(got.len(), want.len());
    for (game, py) in got.iter().zip(want) {
        assert_eq!(game.name, py["name"], "name");
        assert_eq!(game.exe, py["exe"], "{}: exe", game.name);
        assert_eq!(game.shortcut_names.to_vec(), strings(&py["shortcut_names"]));
        assert_eq!(game.steam_appid, py["steam_appid"], "{}: appid", game.name);
        assert_eq!(
            game.steam_folder, py["steam_folder"],
            "{}: folder",
            game.name
        );
        assert_eq!(
            game.paths.to_vec(),
            strings(&py["paths"]),
            "{}: paths",
            game.name
        );
    }
}
