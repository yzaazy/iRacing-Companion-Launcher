//! The real worker thread with a fake backend: commands queue and run in
//! order, each with the selection captured at its press, and buttons stay
//! disabled while their command is queued or running.

mod app_support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app_support::{FakeBackend, FakeDialog, Recorder};
use iracing_companion_launcher::app::Controller;
use iracing_companion_launcher::contract::UiAction;
use iracing_companion_launcher::core::config::ConfigStore;
use iracing_companion_launcher::core::definitions;
use iracing_companion_launcher::managers::Backend;
use iracing_companion_launcher::worker::{Sequencer, Worker};
use serde_json::{Value, json};

/// Holds the first `launch_app` until the test opens the gate, so later
/// presses land while a sequence is running.
struct GatedBackend {
    inner: FakeBackend,
    gate: Receiver<()>,
    opened: bool,
}

impl Backend for GatedBackend {
    fn app_list(&self) -> Vec<String> {
        self.inner.app_list()
    }
    fn app_exe(&self, app: &str) -> Option<String> {
        self.inner.app_exe(app)
    }
    fn find_app_path(&mut self, app: &str) -> Option<String> {
        self.inner.find_app_path(app)
    }
    fn is_app_running(&mut self, app: &str) -> bool {
        self.inner.is_app_running(app)
    }
    fn get_child_names(&mut self, app: &str) -> Vec<String> {
        self.inner.get_child_names(app)
    }
    fn launch_app(&mut self, app: &str, path: &str) -> bool {
        if !self.opened {
            self.gate.recv().unwrap();
            self.opened = true;
        }
        self.inner.launch_app(app, path)
    }
    fn close_app(&mut self, app: &str) -> bool {
        self.inner.close_app(app)
    }
    fn game_list(&self) -> Vec<String> {
        self.inner.game_list()
    }
    fn game_exe(&self, game: &str) -> Option<String> {
        self.inner.game_exe(game)
    }
    fn find_game_path(&mut self, game: &str) -> Option<String> {
        self.inner.find_game_path(game)
    }
    fn is_game_running(&mut self, game: &str) -> bool {
        self.inner.is_game_running(game)
    }
    fn launch_game(&mut self, game: &str, path: &str) -> bool {
        self.inner.launch_game(game, path)
    }
}

fn every_app(value: Value) -> Value {
    definitions::apps()
        .iter()
        .map(|a| (a.name.to_owned(), json!([value.clone()])))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn wait_until(controller: &mut Controller, done: impl Fn(&Controller) -> bool) {
    let start = Instant::now();
    while !done(controller) {
        controller.pump();
        assert!(start.elapsed() < Duration::from_secs(10), "worker stalled");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn commands_queue_in_order_with_their_own_selection() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Recorder::default();
    let config = Arc::new(Mutex::new(ConfigStore::load(dir.path())));
    let script = json!({
        "find_app_path": every_app(json!("C:/fake/app.exe")),
        "find_game_path": {},
        "launch_app": every_app(json!(true)),
        "close_app": every_app(json!(true)),
    });
    let (open_gate, gate) = channel();
    let mut backend = GatedBackend {
        inner: FakeBackend::new(script, rec.clone()),
        gate,
        opened: false,
    };
    let startup = Controller::startup(&mut backend, Box::new(config), "3.0.0", None);
    let repaints = Arc::new(AtomicUsize::new(0));
    let counter = repaints.clone();
    let sequencer = Sequencer::new(Box::new(backend), startup.game_statuses());
    let worker = Worker::spawn(sequencer, move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let mut controller = startup.attach(Box::new(worker));
    let mut dialog = FakeDialog::new([], rec.clone());
    rec.take();

    assert!(controller.state().launch_enabled && controller.state().close_enabled);

    controller.handle(UiAction::Launch, &mut dialog);
    assert!(!controller.state().launch_enabled, "Launch disables itself");
    assert!(
        controller.state().close_enabled,
        "Close stays usable, as in Python"
    );

    // Pressed mid-launch: Close queues behind it with every app selected.
    controller.handle(UiAction::Close, &mut dialog);
    assert!(!controller.state().close_enabled);

    // A checkbox clicked mid-sequence would re-enable the buttons in Python;
    // here they stay disabled until their own command has finished.
    controller.handle(
        UiAction::ToggleApp {
            name: "Fanatec".into(),
            checked: false,
        },
        &mut dialog,
    );
    assert_eq!(controller.state().launch_label, "Launch apps (6)");
    assert!(!controller.state().launch_enabled && !controller.state().close_enabled);

    // A press on a disabled button is ignored rather than queued twice.
    controller.handle(UiAction::Launch, &mut dialog);

    open_gate.send(()).unwrap();
    wait_until(&mut controller, |c| {
        c.state().launch_enabled && c.state().close_enabled
    });
    // Nothing more arrives: the second Launch press was not queued.
    std::thread::sleep(Duration::from_millis(50));
    assert!(!controller.pump());

    let calls: Vec<Value> = rec
        .snapshot()
        .into_iter()
        .filter(|c| c[1] == "launch_app" || c[1] == "close_app")
        .map(|c| json!([c[1], c[2]]))
        .collect();
    let launched = calls.iter().filter(|c| c[0] == "launch_app").count();
    let closed = calls.iter().filter(|c| c[0] == "close_app").count();
    let apps = definitions::apps().len();
    assert_eq!(launched, apps, "the launch used the selection at its press");
    assert_eq!(closed, apps, "the close used the selection at its press");
    let first_close = calls.iter().position(|c| c[0] == "close_app").unwrap();
    assert_eq!(first_close, apps, "the close ran after the whole launch");

    let log: Vec<&str> = controller
        .state()
        .log
        .iter()
        .map(|l| l.text.get(11..).unwrap_or(&l.text))
        .collect();
    let complete = log.iter().position(|l| *l == "Launch sequence complete!");
    let closing = log.iter().position(|l| *l == "Closing applications...");
    assert!(complete.unwrap() < closing.unwrap(), "{log:#?}");
    assert_eq!(log.last(), Some(&"All apps closed!"));

    assert!(
        repaints.load(Ordering::SeqCst) > 0,
        "events request a repaint"
    );
}
