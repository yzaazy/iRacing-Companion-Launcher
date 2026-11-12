//! The real worker thread with a fake backend: commands queue and run in
//! order, each with the selection captured at its press, and buttons stay
//! disabled while their command is queued or running.

#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

mod app_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use app_support::{FakeBackend, FakeDialog, Recorder};
use iracing_companion_launcher::app::Controller;
use iracing_companion_launcher::core::config::{ConfigStore, SharedConfig};
use iracing_companion_launcher::core::definitions;
use iracing_companion_launcher::managers::Backend;
use iracing_companion_launcher::model::{LogLevel, UiAction};
use iracing_companion_launcher::worker::{Inline, Sequencer, Worker};
use serde_json::{Value, json};
use windows::Win32::Foundation::S_FALSE;
use windows::Win32::System::Com::{
    APTTYPE, APTTYPE_MAINSTA, APTTYPE_STA, APTTYPEQUALIFIER, COINIT_APARTMENTTHREADED,
    CoGetApartmentType, CoInitializeEx, CoUninitialize,
};

/// Runs `on_launch` on the worker thread before every `launch_app`, and
/// `on_close` before every `close_app`.
struct HookedBackend {
    inner: FakeBackend,
    on_launch: Box<dyn FnMut() + Send>,
    on_close: Box<dyn FnMut() + Send>,
}

/// Holds the first `launch_app` until the test opens the gate, so later
/// presses land while a sequence is running.
fn gated(inner: FakeBackend, gate: Receiver<()>) -> HookedBackend {
    let mut opened = false;
    HookedBackend {
        inner,
        on_launch: Box::new(move || {
            if !opened {
                gate.recv().unwrap();
                opened = true;
            }
        }),
        on_close: Box::new(|| {}),
    }
}

impl Backend for HookedBackend {
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
    fn child_names(&mut self, app: &str) -> Vec<String> {
        self.inner.child_names(app)
    }
    fn games_under(&mut self, app: &str) -> Vec<String> {
        self.inner.games_under(app)
    }
    fn launch_app(&mut self, app: &str, path: &str) -> bool {
        (self.on_launch)();
        self.inner.launch_app(app, path)
    }
    fn close_app(&mut self, app: &str) -> bool {
        (self.on_close)();
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
    let config = SharedConfig::new(ConfigStore::load(dir.path()));
    let script = json!({
        "find_app_path": every_app(json!("C:/fake/app.exe")),
        "find_game_path": {},
        "launch_app": every_app(json!(true)),
        "close_app": every_app(json!(true)),
    });
    let (open_gate, gate) = channel();
    let mut backend = gated(FakeBackend::new(script, rec.clone()), gate);
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

/// The log lines without their timestamps.
fn log_texts(controller: &Controller) -> Vec<String> {
    controller
        .state()
        .log
        .iter()
        .map(|l| l.text.get(11..).unwrap_or(&l.text).to_owned())
        .collect()
}

/// A panic in a sequence is caught on the worker thread: the log ends with an
/// error line, both buttons work again, and the next command runs (issue #5).
#[test]
fn a_panic_in_a_sequence_is_logged_and_the_worker_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Recorder::default();
    let config = SharedConfig::new(ConfigStore::load(dir.path()));
    let script = json!({
        "find_app_path": every_app(json!("C:/fake/app.exe")),
        "find_game_path": {},
        "launch_app": every_app(json!(true)),
        "close_app": every_app(json!(true)),
    });
    let (mut launches, mut closes) = (0, 0);
    let mut backend = HookedBackend {
        inner: FakeBackend::new(script, rec.clone()),
        on_launch: Box::new(move || {
            launches += 1;
            if launches == 1 {
                panic!("injected launch panic");
            }
        }),
        on_close: Box::new(move || {
            closes += 1;
            if closes == 1 {
                panic!("injected close panic");
            }
        }),
    };
    let startup = Controller::startup(&mut backend, Box::new(config), "3.0.0", None);
    let sequencer = Sequencer::new(Box::new(backend), startup.game_statuses());
    let mut controller = startup.attach(Box::new(Worker::spawn(sequencer, || {})));
    let mut dialog = FakeDialog::new([], rec);
    let idle = |c: &Controller| c.state().launch_enabled && c.state().close_enabled;

    for (action, error, done) in [
        (
            UiAction::Launch,
            "Launch sequence stopped by an internal error: injected launch panic",
            "Launch sequence complete!",
        ),
        (
            UiAction::Close,
            "Close sequence stopped by an internal error: injected close panic",
            "All apps closed!",
        ),
    ] {
        controller.handle(action.clone(), &mut dialog);
        wait_until(&mut controller, idle);
        let last = controller.state().log.last().unwrap();
        assert_eq!(last.level, LogLevel::Error);
        assert_eq!(log_texts(&controller).last().unwrap(), error);

        // The worker is still there: the same command runs to the end.
        controller.handle(action, &mut dialog);
        wait_until(&mut controller, idle);
        assert_eq!(log_texts(&controller).last().unwrap(), done);
    }
}

/// Runs one Close with every app found and checked, inline, and returns the
/// log lines after "Closing applications..." without their timestamps.
/// `config` is the test's `config.ini`.
fn close_log(config: &str, mut script: Value) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.ini"), config).unwrap();
    let rec = Recorder::default();
    let config = SharedConfig::new(ConfigStore::load(dir.path()));
    script["find_app_path"] = every_app(json!("C:/fake/app.exe"));
    script["close_app"] = every_app(json!(true));
    let mut backend = FakeBackend::new(script, rec.clone());
    let startup = Controller::startup(&mut backend, Box::new(config), "3.0.0", None);
    let sequencer = Sequencer::new(Box::new(backend), startup.game_statuses());
    let mut controller = startup.attach(Box::new(Inline::new(sequencer, Box::new(|_| {}))));
    let mut dialog = FakeDialog::new([], rec);

    controller.handle(UiAction::Close, &mut dialog);
    controller.pump();

    let log: Vec<String> = controller
        .state()
        .log
        .iter()
        .map(|l| l.text.get(11..).unwrap_or(&l.text).to_owned())
        .collect();
    let closing = log.iter().position(|l| l == "Closing applications...");
    log[closing.unwrap() + 1..].to_vec()
}

fn still_running(game: &str) -> String {
    format!("⚠ {game} is still running - please close it manually")
}

/// A game that a closed app had started keeps running, and the log says so
/// once per game, whether or not a game is selected.
#[test]
fn close_warns_about_each_game_a_closed_app_had_started() {
    let log = close_log(
        "",
        json!({
            "games_under": {
                "Crew Chief V4": [["rFactor 2"]],
                "Elgato Stream Deck": [["iRacing", "rFactor 2"]],
            },
        }),
    );
    let warnings: Vec<&String> = log.iter().filter(|l| l.starts_with('⚠')).collect();
    assert_eq!(
        warnings,
        [&still_running("rFactor 2"), &still_running("iRacing")]
    );
    assert_eq!(
        &log[log.len() - 3..],
        [
            still_running("rFactor 2"),
            still_running("iRacing"),
            "All apps closed!".to_owned()
        ]
    );
}

/// The selected game already gets that warning when it was running; it is not
/// repeated, and the game is not asked for again.
#[test]
fn close_warns_once_about_the_selected_game() {
    let log = close_log(
        "[Settings]\nselected_game = iRacing\n",
        json!({
            "find_game_path": { "iRacing": ["steam://rungameid/266410"] },
            // Running at startup, and gone by name afterwards: the simulator
            // below the app is what is left running.
            "is_game_running": { "iRacing": [true, false] },
            "games_under": { "Elgato Stream Deck": [["iRacing"]] },
        }),
    );
    let warnings: Vec<&String> = log.iter().filter(|l| l.starts_with('⚠')).collect();
    assert_eq!(warnings, [&still_running("iRacing")]);
    assert!(!log.contains(&"iRacing was closed manually".to_owned()));
}

/// `ShellExecuteW` runs on the worker thread, so the thread must be a
/// single-threaded COM apartment while sequences run, as the Tk thread was.
#[test]
fn worker_thread_is_a_com_apartment() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Recorder::default();
    let config = SharedConfig::new(ConfigStore::load(dir.path()));
    let script = json!({
        "find_app_path": every_app(json!("C:/fake/app.exe")),
        "find_game_path": {},
        "launch_app": every_app(json!(true)),
    });
    let (probe_tx, probe_rx) = channel();
    let mut backend = HookedBackend {
        inner: FakeBackend::new(script, rec.clone()),
        on_launch: Box::new(move || {
            let (mut kind, mut qualifier) = (APTTYPE::default(), APTTYPEQUALIFIER::default());
            // SAFETY: both out-pointers point to live, writable locals.
            let current =
                unsafe { CoGetApartmentType(&raw mut kind, &raw mut qualifier) }.map(|()| kind);
            // What detect::shortcuts' per-call guard does on this thread: it
            // nests (S_FALSE) and its balancing uninit leaves the apartment.
            // SAFETY: CoInitializeEx takes no pointers (the reserved one is null)
            // and affects only this thread; it is balanced just below.
            let nested = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            // SAFETY: balances the nested init above, on the same thread; the
            // worker thread already has an apartment, so that init returned
            // S_FALSE (asserted below) and this leaves the apartment as it was.
            unsafe { CoUninitialize() };
            // SAFETY: both out-pointers point to the same live, writable locals.
            let after =
                unsafe { CoGetApartmentType(&raw mut kind, &raw mut qualifier) }.map(|()| kind);
            let _ = probe_tx.send((current, nested, after));
        }),
        on_close: Box::new(|| {}),
    };
    let startup = Controller::startup(&mut backend, Box::new(config), "3.0.0", None);
    let sequencer = Sequencer::new(Box::new(backend), startup.game_statuses());
    let mut controller = startup.attach(Box::new(Worker::spawn(sequencer, || {})));
    let mut dialog = FakeDialog::new([], rec);

    controller.handle(UiAction::Launch, &mut dialog);
    let (current, nested, after) = probe_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // The first single-threaded apartment in a process is reported as the main one.
    let single_threaded = [Ok(APTTYPE_STA), Ok(APTTYPE_MAINSTA)];
    assert!(single_threaded.contains(&current), "{current:?}");
    assert_eq!(nested, S_FALSE, "the worker's apartment is already there");
    assert_eq!(after, current);
    wait_until(&mut controller, |c| c.state().launch_enabled);
}
