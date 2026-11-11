//! Replays the golden transcripts recorded from the Python controller
//! (`tests/fixtures/app/capture_transcripts.py`) against the Rust controller
//! and worker with a fake backend, and asserts identical transcripts and end
//! states. Sequences run inline here so the order is deterministic.

mod app_support;

use std::sync::{Arc, Mutex};

use app_support::{FakeBackend, FakeDialog, Recorder, RecordingSettings};
use iracing_companion_launcher::app::Controller;
use iracing_companion_launcher::contract::{Status, UiAction};
use iracing_companion_launcher::core::config::ConfigStore;
use iracing_companion_launcher::worker::{Inline, Sequencer};
use serde_json::{Value, json};

const TRANSCRIPTS: &str = include_str!("fixtures/app/transcripts.json");

fn scenarios() -> Vec<Value> {
    serde_json::from_str::<Value>(TRANSCRIPTS)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

/// Seeds `config.ini` with the scenario's `[Settings]`.
fn seeded_config(dir: &std::path::Path, settings: &Value) -> ConfigStore {
    let mut text = String::from("[AppPaths]\n\n[Settings]\n");
    for (key, value) in settings.as_object().unwrap() {
        text.push_str(&format!("{key} = {}\n", value.as_str().unwrap()));
    }
    std::fs::write(dir.join("config.ini"), text).unwrap();
    ConfigStore::load(dir)
}

struct Run {
    transcript: Vec<Value>,
    final_state: Value,
}

fn replay(scenario: &Value) -> Run {
    let dir = tempfile::tempdir().unwrap();
    let rec = Recorder::default();
    let config = Arc::new(Mutex::new(seeded_config(dir.path(), &scenario["settings"])));
    let settings = RecordingSettings::new(config, rec.clone());
    let mut backend = FakeBackend::new(scenario["script"].clone(), rec.clone());
    let picks = scenario["picks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned());
    let mut dialog = FakeDialog::new(picks, rec.clone());

    let trace_rec = rec.clone();
    let startup = Controller::startup(
        &mut backend,
        Box::new(settings),
        "3.0.0",
        Some(Box::new(move |e| trace_rec.event(e))),
    );
    let sequencer = Sequencer::new(Box::new(backend), startup.game_statuses());
    let worker_rec = rec.clone();
    let mut controller = startup.attach(Box::new(Inline::new(
        sequencer,
        Box::new(move |e| worker_rec.event(e)),
    )));

    for step in scenario["steps"].as_array().unwrap() {
        let action = step["do"].as_str().unwrap();
        let name = step["name"].as_str().map(str::to_owned);
        rec.push(json!(["step", action, name]));
        let state = controller.state();
        let ui_action = match action {
            "launch" if !state.launch_enabled => None,
            "close" if !state.close_enabled => None,
            "launch" => Some(UiAction::Launch),
            "close" => Some(UiAction::Close),
            "toggle_app" => {
                let name = name.unwrap();
                let card = state.apps.iter().find(|c| c.name == name).unwrap();
                // A disabled (not-found) checkbox does nothing.
                (card.status != Status::NotFound).then_some(UiAction::ToggleApp {
                    checked: !card.checked,
                    name,
                })
            }
            "select_all" => Some(UiAction::ToggleSelectAll),
            "select_game" => {
                let not_found = name.as_ref().is_some_and(|n| {
                    state
                        .games
                        .iter()
                        .any(|g| &g.name == n && g.status == Status::NotFound)
                });
                (state.selected_game != name && !not_found).then_some(UiAction::SelectGame(name))
            }
            "browse_app" => Some(UiAction::BrowseApp(name.unwrap())),
            "browse_game" => Some(UiAction::BrowseGame(name.unwrap())),
            other => panic!("unknown step {other}"),
        };
        match ui_action {
            Some(a) => controller.handle(a, &mut dialog),
            None => rec.push(json!(["ignored", action])),
        }
        controller.pump();
    }
    assert!(dialog.picks.is_empty(), "unused dialog picks");

    let mut final_state = serde_json::to_value(controller.state()).unwrap();
    let object = final_state.as_object_mut().unwrap();
    object.remove("version");
    object.remove("log");
    Run {
        transcript: rec.take(),
        final_state,
    }
}

fn first_difference(expected: &[Value], actual: &[Value]) -> Option<usize> {
    (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
}

#[test]
fn every_scenario_replays_the_python_transcript() {
    let mut failures = Vec::new();
    let all = scenarios();
    assert!(all.len() >= 25, "transcripts.json lost scenarios");
    for scenario in &all {
        let name = scenario["name"].as_str().unwrap();
        let run = replay(scenario);
        let expected = scenario["transcript"].as_array().unwrap();
        if let Some(i) = first_difference(expected, &run.transcript) {
            let from = i.saturating_sub(3);
            failures.push(format!(
                "{name}: transcript differs at item {i}\n  python: {:?}\n  rust:   {:?}",
                &expected[from..(i + 3).min(expected.len())],
                &run.transcript[from.min(run.transcript.len())..(i + 3).min(run.transcript.len())],
            ));
        } else if run.final_state != scenario["final"] {
            failures.push(format!(
                "{name}: end state differs\n  python: {}\n  rust:   {}",
                scenario["final"], run.final_state
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Startup writes not-found apps as unchecked and saves, through the real config store.
#[test]
fn startup_saves_config_to_disk() {
    let scenario = scenarios()
        .into_iter()
        .find(|s| s["name"] == "startup_mixed_no_settings")
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let rec = Recorder::default();
    let config = Arc::new(Mutex::new(seeded_config(dir.path(), &scenario["settings"])));
    let mut backend = FakeBackend::new(scenario["script"].clone(), rec.clone());
    let _ = Controller::startup(&mut backend, Box::new(config), "3.0.0", None);
    let saved = std::fs::read_to_string(dir.path().join("config.ini")).unwrap();
    assert!(saved.contains("trading_paints_enabled = False"), "{saved}");
    assert!(
        saved.contains("marvinsaira_refactored_enabled = False"),
        "{saved}"
    );
    assert!(!saved.contains("fanatec_enabled"), "{saved}");
}
