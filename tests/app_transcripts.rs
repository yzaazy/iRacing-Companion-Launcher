//! Replays the golden transcripts recorded from the Python controller up to
//! v2.3.2 (`tests/fixtures/app/transcripts.json`) against the Rust controller
//! and worker with a fake backend, and asserts identical transcripts and end
//! states, after applying the deliberate changes from v2.3.2 that show in a
//! transcript. Sequences run inline here so the order is deterministic.

#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

mod app_support;

use app_support::{FakeBackend, FakeDialog, Recorder, RecordingSettings};
use iracing_companion_launcher::app::Controller;
use iracing_companion_launcher::core::config::{ConfigStore, SharedConfig};
use iracing_companion_launcher::model::{Status, UiAction};
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
    let config = SharedConfig::new(seeded_config(dir.path(), &scenario["settings"]));
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

/// The Python transcript with change 9 in `docs/behaviour.md` applied (#13): a
/// picked path with a `%`, where v2.3.2's `set_app_path` raised and the Browse
/// handler stopped, is now saved, so the handler goes on as for any other path.
fn with_percent_paths_saved(python: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut step = (String::new(), String::new());
    for (i, item) in python.iter().enumerate() {
        out.push(item.clone());
        if item[0] == "step" {
            step = (
                item[1].as_str().unwrap().to_owned(),
                item[2].as_str().unwrap_or_default().to_owned(),
            );
        }
        let raised =
            item[0] == "set_app_path" && python.get(i + 1).is_none_or(|next| next[0] == "step");
        if !raised {
            continue;
        }
        let (key, path) = (item[1].as_str().unwrap(), item[2].as_str().unwrap());
        assert!(path.contains('%'), "set_app_path raised on {path}");
        let (action, name) = (&step.0, &step.1);
        if action == "browse_app" {
            out.push(json!(["app_status", name, "idle", 0]));
            out.push(json!(["config_set", format!("{key}_enabled"), "True"]));
            out.push(json!(["config_save"]));
        } else {
            out.push(json!(["game_status", name, "idle"]));
        }
        out.push(json!([
            "log",
            "success",
            format!("{name} path configured: {path}")
        ]));
    }
    out
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
        let expected = with_percent_paths_saved(scenario["transcript"].as_array().unwrap());
        let expected = expected.as_slice();
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
    let config = SharedConfig::new(seeded_config(dir.path(), &scenario["settings"]));
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
