//! The shared types in `model.rs` read the fixture shape from `docs/rewrite/contracts.md` and round-trip.

use iracing_companion_launcher::model::{
    AppCard, GameCard, LogLevel, LogLine, Status, UiAction, ViewState,
};
use iracing_companion_launcher::worker::Event;
use std::collections::BTreeMap;

const FIXTURE: &str = r#"{
  "mixed": {
    "version": "2.3.2",
    "apps": [
      { "name": "Fanatec", "status": "running", "checked": true, "helper_count": 3 },
      { "name": "SimHub", "status": "not_found", "checked": false, "helper_count": 0 },
      { "name": "Garage61", "status": "stopping", "checked": true, "helper_count": 0 }
    ],
    "games": [
      { "name": "iRacing", "status": "idle" },
      { "name": "Le Mans Ultimate", "status": "not_found" }
    ],
    "selected_game": "iRacing",
    "log": [
      { "level": "info", "text": "[12:00:00] Started" },
      { "level": "divider", "text": "──────────" },
      { "level": "warning", "text": "[12:00:01] Missing" }
    ],
    "select_all_label": "Deselect All",
    "launch_label": "Launch with iRacing (2)",
    "close_label": "Close all (2)",
    "launch_enabled": true,
    "close_enabled": true
  },
  "none-selected": {
    "version": "2.3.2",
    "apps": [],
    "games": [],
    "selected_game": null,
    "log": [],
    "select_all_label": "Select All",
    "launch_label": "Launch apps (0)",
    "close_label": "Close all (0)",
    "launch_enabled": false,
    "close_enabled": false
  }
}"#;

#[test]
fn fixture_shape_deserialises_into_view_state() {
    let states: BTreeMap<String, ViewState> = serde_json::from_str(FIXTURE).unwrap();
    let mixed = &states["mixed"];
    assert_eq!(
        mixed.apps[0],
        AppCard {
            name: "Fanatec".into(),
            status: Status::Running,
            checked: true,
            helper_count: 3,
        }
    );
    assert_eq!(mixed.apps[1].status, Status::NotFound);
    assert_eq!(mixed.apps[2].status, Status::Stopping);
    assert_eq!(
        mixed.games[1],
        GameCard {
            name: "Le Mans Ultimate".into(),
            status: Status::NotFound,
        }
    );
    assert_eq!(mixed.selected_game.as_deref(), Some("iRacing"));
    assert_eq!(
        mixed.log[1],
        LogLine {
            level: LogLevel::Divider,
            text: "──────────".into(),
        }
    );
    assert_eq!(states["none-selected"].selected_game, None);
}

#[test]
fn view_state_round_trips() {
    let states: BTreeMap<String, ViewState> = serde_json::from_str(FIXTURE).unwrap();
    let json = serde_json::to_string(&states).unwrap();
    let again: BTreeMap<String, ViewState> = serde_json::from_str(&json).unwrap();
    assert_eq!(states, again);
}

#[test]
fn enums_use_python_snake_case_names() {
    assert_eq!(
        serde_json::to_string(&Status::NotFound).unwrap(),
        "\"not_found\""
    );
    assert_eq!(
        serde_json::to_string(&LogLevel::Divider).unwrap(),
        "\"divider\""
    );
    let none = serde_json::to_value(UiAction::SelectGame(None)).unwrap();
    assert_eq!(none, serde_json::json!({ "select_game": null }));
    let event = Event::AppStatus {
        name: "SimHub".into(),
        status: Status::Idle,
        helper_count: 0,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), event);
}

#[test]
fn none_selected_game_serialises_as_null() {
    let states: BTreeMap<String, ViewState> = serde_json::from_str(FIXTURE).unwrap();
    let value = serde_json::to_value(&states["none-selected"]).unwrap();
    assert!(value["selected_game"].is_null());
}
