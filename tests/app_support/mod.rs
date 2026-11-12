//! Fakes for the controller tests: a scripted backend that records every call
//! and never touches a process, recording config and dialog wrappers, and the
//! transcript format `tests/fixtures/app/capture_transcripts.py` writes.

#![allow(dead_code)] // each test binary uses a different part of this module

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use iracing_companion_launcher::app::{DialogRequest, FileDialog, Settings};
use iracing_companion_launcher::core::config::SharedConfig;
use iracing_companion_launcher::core::definitions;
use iracing_companion_launcher::managers::Backend;
use iracing_companion_launcher::model::LogLevel;
use iracing_companion_launcher::worker::Event;
use serde_json::{Value, json};

/// The ordered transcript shared by every fake.
#[derive(Clone, Default)]
pub struct Recorder(Arc<Mutex<Vec<Value>>>);

impl Recorder {
    pub fn push(&self, item: Value) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(item);
    }

    pub fn take(&self) -> Vec<Value> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Records what the Python `StatusCard`/`GameCard`/`ActivityLogger` wrappers record.
    pub fn event(&self, event: &Event) {
        match event {
            Event::Log(line) => {
                let level = serde_json::to_value(line.level).unwrap();
                let text = if line.level == LogLevel::Divider {
                    line.text.as_str()
                } else {
                    strip_timestamp(&line.text)
                };
                self.push(json!(["log", level, text]));
            }
            Event::AppStatus {
                name,
                status,
                helper_count,
            } => self.push(json!(["app_status", name, status, helper_count])),
            Event::GameStatus { name, status } => self.push(json!(["game_status", name, status])),
            Event::LaunchFinished | Event::CloseFinished => {}
        }
    }
}

/// `[HH:MM:SS] message` -> `message`, checking the prefix's shape.
fn strip_timestamp(text: &str) -> &str {
    let b = text.as_bytes();
    let digits = [1, 2, 4, 5, 7, 8].iter().all(|&i| b[i].is_ascii_digit());
    assert!(
        b.len() >= 11
            && b[0] == b'['
            && b[3] == b':'
            && b[6] == b':'
            && b[9] == b']'
            && b[10] == b' '
            && digits,
        "log line without a [HH:MM:SS] prefix: {text:?}"
    );
    &text[11..]
}

/// Scripted answers: `script[method][name]` is a list consumed in order, the
/// last value sticky; a missing entry gives the method's default. Defaults
/// match the Python fakes.
pub struct FakeBackend {
    script: Value,
    counters: HashMap<(String, String), usize>,
    rec: Recorder,
}

impl FakeBackend {
    pub fn new(script: Value, rec: Recorder) -> Self {
        Self {
            script,
            counters: HashMap::new(),
            rec,
        }
    }

    fn respond(&mut self, method: &str, name: &str, default: Value) -> Value {
        let Some(seq) = self.script[method][name].as_array() else {
            return default;
        };
        let n = self
            .counters
            .entry((method.to_owned(), name.to_owned()))
            .or_insert(0);
        let value = seq[(*n).min(seq.len() - 1)].clone();
        *n += 1;
        value
    }

    fn call(&mut self, method: &str, args: &[&str], default: Value) -> Value {
        let result = self.respond(method, args[0], default);
        let mut item = vec![json!("call"), json!(method)];
        item.extend(args.iter().map(|a| json!(a)));
        item.push(result.clone());
        self.rec.push(Value::Array(item));
        result
    }
}

fn path(v: Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn flag(v: Value) -> bool {
    v.as_bool().expect("scripted bool")
}

impl Backend for FakeBackend {
    fn app_list(&self) -> Vec<String> {
        definitions::apps()
            .iter()
            .map(|a| a.name.to_owned())
            .collect()
    }
    fn app_exe(&self, app: &str) -> Option<String> {
        definitions::apps()
            .into_iter()
            .find(|a| a.name == app)
            .map(|a| a.exe.to_owned())
    }
    fn find_app_path(&mut self, app: &str) -> Option<String> {
        path(self.call("find_app_path", &[app], Value::Null))
    }
    fn is_app_running(&mut self, app: &str) -> bool {
        flag(self.call("is_app_running", &[app], json!(false)))
    }
    fn get_child_names(&mut self, app: &str) -> Vec<String> {
        serde_json::from_value(self.call("get_child_names", &[app], json!([]))).unwrap()
    }
    fn launch_app(&mut self, app: &str, path: &str) -> bool {
        flag(self.call("launch_app", &[app, path], json!(false)))
    }
    fn close_app(&mut self, app: &str) -> bool {
        flag(self.call("close_app", &[app], json!(false)))
    }
    fn game_list(&self) -> Vec<String> {
        definitions::games()
            .iter()
            .map(|g| g.name.to_owned())
            .collect()
    }
    fn game_exe(&self, game: &str) -> Option<String> {
        definitions::games()
            .into_iter()
            .find(|g| g.name == game)
            .map(|g| g.exe.to_owned())
    }
    fn find_game_path(&mut self, game: &str) -> Option<String> {
        path(self.call("find_game_path", &[game], Value::Null))
    }
    fn is_game_running(&mut self, game: &str) -> bool {
        flag(self.call("is_game_running", &[game], json!(false)))
    }
    fn launch_game(&mut self, game: &str, path: &str) -> bool {
        flag(self.call("launch_game", &[game, path], json!(false)))
    }
}

/// The real `ConfigStore` (in a temp dir) behind the real `Settings` impl, recording writes.
pub struct RecordingSettings {
    pub inner: SharedConfig,
    rec: Recorder,
}

impl RecordingSettings {
    pub fn new(inner: SharedConfig, rec: Recorder) -> Self {
        Self { inner, rec }
    }
}

impl Settings for RecordingSettings {
    fn bool_setting(&self, key: &str, fallback: bool) -> bool {
        self.inner.bool_setting(key, fallback)
    }
    fn setting(&self, key: &str) -> Option<String> {
        self.inner.setting(key)
    }
    fn set_setting(&mut self, key: &str, value: &str) {
        self.rec.push(json!(["config_set", key, value]));
        self.inner.set_setting(key, value);
    }
    fn save(&mut self) -> bool {
        self.rec.push(json!(["config_save"]));
        Settings::save(&mut self.inner)
    }
    fn set_app_path(&mut self, key: &str, path: &str) -> bool {
        self.rec.push(json!(["set_app_path", key, path]));
        self.inner.set_app_path(key, path)
    }
}

/// Returns scripted picks (`""` is a cancelled dialog) and records each request.
pub struct FakeDialog {
    pub picks: VecDeque<String>,
    rec: Recorder,
}

impl FakeDialog {
    pub fn new(picks: impl IntoIterator<Item = String>, rec: Recorder) -> Self {
        Self {
            picks: picks.into_iter().collect(),
            rec,
        }
    }
}

impl FileDialog for FakeDialog {
    fn pick(&mut self, request: &DialogRequest) -> Option<String> {
        let filters: Vec<Value> = request
            .filters
            .iter()
            .map(|(label, pattern)| json!([label, pattern]))
            .collect();
        self.rec.push(json!([
            "dialog",
            request.title,
            filters,
            request.initial_dir
        ]));
        let pick = self.picks.pop_front().expect("a scripted dialog pick");
        // askopenfilename returns "" when cancelled.
        (!pick.is_empty()).then_some(pick)
    }
}
