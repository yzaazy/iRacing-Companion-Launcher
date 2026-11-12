//! Runs launch and close sequences off the UI thread; the Python app (up to v2.3.2) ran them on
//! the Tk thread (`docs/behaviour.md`, "Changes from v2.3.2": "The window stays responsive during
//! Launch and Close").
//!
//! [`Sequencer`] holds the Python app's launch, game launch and close sequences
//! line for line; [`Worker`] runs it on a background thread, one command at a
//! time in the order sent, and [`Inline`] runs it on the caller's thread for tests.
//!
//! Python reads two things from the window mid-sequence: each card's checkbox,
//! and the selected game card's status in the close sequence. Checkboxes arrive
//! captured in the [`Command`] (same change from v2.3.2). The game status is
//! mirrored here: it changes only at startup (the seed), through this
//! sequencer's own events, and through Browse, which only turns `not_found`
//! into `idle` and so never changes the one question asked of it, "is it
//! `running`?".

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use serde::{Deserialize, Serialize};

use crate::core::log;
use crate::managers::Backend;
use crate::model::{LogLevel, LogLine, Status};
use crate::win::ComApartment;

/// What the controller asks the worker to run. The serde derives on this and
/// [`Event`] keep both readable in tests and transcripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Launch {
        apps: Vec<String>,
        game: Option<String>,
    },
    Close {
        apps: Vec<String>,
        game: Option<String>,
    },
}

/// What a sequence reports back, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Log(LogLine),
    AppStatus {
        name: String,
        status: Status,
        helper_count: usize,
    },
    GameStatus {
        name: String,
        status: Status,
    },
    LaunchFinished,
    CloseFinished,
}

/// Executes commands against a [`Backend`], reporting through `emit`.
pub struct Sequencer {
    backend: Box<dyn Backend>,
    game_status: HashMap<String, Status>,
}

impl Sequencer {
    /// `game_status` is every game card's status when the window finished starting up.
    pub fn new(backend: Box<dyn Backend>, game_status: HashMap<String, Status>) -> Self {
        Self {
            backend,
            game_status,
        }
    }

    /// Runs one command to completion; the last event is `LaunchFinished` or `CloseFinished`.
    fn run(&mut self, command: Command, emit: &mut dyn FnMut(Event)) {
        let mut out = Out {
            emit,
            game_status: &mut self.game_status,
        };
        match command {
            Command::Launch { apps, game } => {
                launch_apps(self.backend.as_mut(), &mut out, &apps, game.as_deref());
            }
            Command::Close { apps, game } => {
                close_apps(self.backend.as_mut(), &mut out, &apps, game.as_deref());
            }
        }
    }
}

/// Emits events and keeps the game status mirror in step with them.
struct Out<'a> {
    emit: &'a mut dyn FnMut(Event),
    game_status: &'a mut HashMap<String, Status>,
}

impl Out<'_> {
    fn log(&mut self, level: LogLevel, message: &str) {
        (self.emit)(Event::Log(log::line(level, message)));
    }

    fn divider(&mut self) {
        (self.emit)(Event::Log(log::divider()));
    }

    /// `StatusCard.set_status(status, child_count)`.
    fn app(&mut self, name: &str, status: Status, helper_count: usize) {
        (self.emit)(Event::AppStatus {
            name: name.to_owned(),
            status,
            helper_count,
        });
    }

    fn game(&mut self, name: &str, status: Status) {
        self.game_status.insert(name.to_owned(), status);
        (self.emit)(Event::GameStatus {
            name: name.to_owned(),
            status,
        });
    }

    fn helpers(&mut self, prefix: &str, names: &[String]) {
        if !names.is_empty() {
            self.log(LogLevel::Info, &format!("  {prefix}: {}", names.join(", ")));
        }
    }
}

/// The launch sequence: the checked apps, then the selected game. `apps` are the checked cards
/// at the press.
fn launch_apps(backend: &mut dyn Backend, out: &mut Out, apps: &[String], game: Option<&str>) {
    out.divider();
    out.log(LogLevel::Launch, "Starting launch sequence...");

    let mut tally = LaunchTally::default();
    for app in backend.app_list() {
        if !apps.contains(&app) {
            continue;
        }
        tally.selected += 1;

        let Some(path) = backend.find_app_path(&app) else {
            out.log(
                LogLevel::Warning,
                &format!("Skipping {app} - not configured"),
            );
            tally.not_configured += 1;
            continue;
        };

        if backend.is_app_running(&app) {
            out.log(LogLevel::Info, &format!("Skipping {app} - already running"));
            let children = backend.child_names(&app);
            out.helpers("helpers", &children);
            out.app(&app, Status::Running, children.len());
            tally.running += 1;
            continue;
        }

        out.app(&app, Status::Starting, 0);
        out.log(LogLevel::Info, &format!("Launching {app}..."));

        if backend.launch_app(&app, &path) {
            out.log(LogLevel::Success, &format!("{app} started successfully"));
            let children = backend.child_names(&app);
            out.helpers("helpers", &children);
            out.app(&app, Status::Running, children.len());
            tally.running += 1;
        } else {
            out.log(LogLevel::Error, &format!("{app} failed to start"));
            out.log(LogLevel::Warning, &format!("Try reinstalling {app}"));
            out.app(&app, Status::Failed, 0);
            tally.failed += 1;
        }
    }

    // v2.3.2 logged "All apps launched!" whatever happened above (#8).
    if let Some((level, summary)) = tally.summary() {
        out.log(level, &summary);
    }

    if let Some(game) = game {
        launch_selected_game(backend, out, game);
    }

    out.log(LogLevel::Success, "Launch sequence complete!");
    (out.emit)(Event::LaunchFinished);
}

/// What became of the checked apps in a launch sequence.
#[derive(Default)]
struct LaunchTally {
    selected: usize,
    /// Started, or already running.
    running: usize,
    failed: usize,
    not_configured: usize,
}

impl LaunchTally {
    /// The line after the app loop: none when no app was checked.
    fn summary(&self) -> Option<(LogLevel, String)> {
        if self.selected == 0 {
            return None;
        }
        if self.running == self.selected {
            return Some((LogLevel::Success, "All apps launched!".to_owned()));
        }
        let noun = if self.selected == 1 { "app" } else { "apps" };
        let mut summary = format!("Launched {} of {} {noun}", self.running, self.selected);
        if self.failed > 0 {
            summary.push_str(&format!(", {} failed", self.failed));
        }
        if self.not_configured > 0 {
            summary.push_str(&format!(", {} not configured", self.not_configured));
        }
        Some((LogLevel::Warning, summary))
    }
}

/// Launches the selected game at the end of the launch sequence, unless it is not found or
/// already running.
fn launch_selected_game(backend: &mut dyn Backend, out: &mut Out, game: &str) {
    let Some(path) = backend.find_game_path(game) else {
        out.log(
            LogLevel::Warning,
            &format!("Skipping {game} - not configured"),
        );
        return;
    };

    if backend.is_game_running(game) {
        out.log(
            LogLevel::Info,
            &format!("Skipping {game} - already running"),
        );
        out.game(game, Status::Running);
        return;
    }

    out.game(game, Status::Starting);
    out.log(LogLevel::Launch, &format!("Launching {game}..."));

    if backend.launch_game(game, &path) {
        out.log(LogLevel::Success, &format!("{game} started successfully"));
        out.game(game, Status::Running);
    } else {
        out.log(LogLevel::Error, &format!("{game} failed to start"));
        out.log(
            LogLevel::Warning,
            &format!("Try reinstalling {game} or selecting a different path"),
        );
        out.game(game, Status::Failed);
    }
}

/// The close sequence: the checked apps, then a check on the selected game if it was running,
/// then a warning for every other game that a closed app had started.
fn close_apps(backend: &mut dyn Backend, out: &mut Out, apps: &[String], game: Option<&str>) {
    out.divider();
    out.log(LogLevel::Close, "Closing applications...");

    // Games running below a tracked app, which its close leaves running
    // (`docs/behaviour.md`, "Changes from v2.3.2"). A game is found there only
    // while the app is tracked and alive, and then the app is closed below.
    let mut left_running: Vec<String> = Vec::new();
    let mut any_selected = false;

    for app in backend.app_list() {
        if !apps.contains(&app) {
            continue;
        }
        any_selected = true;

        for name in backend.games_under(&app) {
            if !left_running.contains(&name) {
                left_running.push(name);
            }
        }

        let path = backend.find_app_path(&app);
        // No detected path: closed only when running, and the card goes back to
        // not-found either way (`docs/behaviour.md`, "Changes from v2.3.2").
        if path.is_none() && !backend.is_app_running(&app) {
            continue;
        }

        out.app(&app, Status::Stopping, 0);
        out.log(LogLevel::Info, &format!("Closing {app}..."));
        let children = backend.child_names(&app);
        out.helpers("helpers to close", &children);

        let closed = backend.close_app(&app);
        if closed {
            out.log(LogLevel::Success, &format!("{app} closed"));
        } else {
            out.log(LogLevel::Warning, &format!("{app} was not running"));
        }
        let status = match (path.is_some(), closed) {
            (false, _) => Status::NotFound,
            (true, true) => Status::Stopped,
            (true, false) => Status::Idle,
        };
        out.app(&app, status, 0);
    }

    let still_running =
        |game: &str| format!("⚠ {game} is still running - please close it manually");
    let mut told = None;
    if let Some(game) = game
        && out.game_status.get(game) == Some(&Status::Running)
    {
        if left_running.iter().any(|g| g.as_str() == game) || backend.is_game_running(game) {
            out.log(LogLevel::Warning, &still_running(game));
            told = Some(game);
        } else {
            out.log(LogLevel::Success, &format!("{game} was closed manually"));
            out.game(game, Status::Idle);
        }
    }
    for game in &left_running {
        if told != Some(game.as_str()) {
            out.log(LogLevel::Warning, &still_running(game));
        }
    }

    // Every checked app is closed or was not running by now. v2.3.2 logged this
    // line also when no app was checked (#8).
    if any_selected {
        out.log(LogLevel::Success, "All apps closed!");
    }
    (out.emit)(Event::CloseFinished);
}

/// Where the controller sends commands and collects events.
pub trait Dispatch {
    fn send(&mut self, command: Command);
    /// Events produced since the last call, in order.
    fn poll(&mut self) -> Vec<Event>;
}

/// A background thread that runs commands one at a time, in the order sent.
///
/// Dropping it stops the thread after the current sequence; the process
/// exiting first ends it mid-sequence, as closing the Python window did.
pub struct Worker {
    commands: Sender<Command>,
    events: Receiver<Event>,
}

impl Worker {
    /// `notify` runs after every event, from the worker thread (the app requests a repaint).
    pub fn spawn(mut sequencer: Sequencer, notify: impl Fn() + Send + 'static) -> Self {
        let (command_tx, command_rx) = mpsc::channel::<Command>();
        let (event_tx, event_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _com = ComApartment::enter();
            for command in command_rx {
                sequencer.run(command, &mut |event| {
                    // The window is gone when sending fails; finish the sequence anyway.
                    let _ = event_tx.send(event);
                    notify();
                });
            }
        });
        Self {
            commands: command_tx,
            events: event_rx,
        }
    }
}

impl Dispatch for Worker {
    fn send(&mut self, command: Command) {
        // Fails only if the thread panicked; there is nothing left to run it.
        let _ = self.commands.send(command);
    }

    fn poll(&mut self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

/// Runs each command to completion inside `send`, for tests. `observe` sees
/// every event the moment it is emitted, before the controller applies it.
pub struct Inline {
    sequencer: Sequencer,
    observe: Box<dyn FnMut(&Event)>,
    pending: Vec<Event>,
}

impl Inline {
    /// Public for tests, like the type.
    pub fn new(sequencer: Sequencer, observe: Box<dyn FnMut(&Event)>) -> Self {
        Self {
            sequencer,
            observe,
            pending: Vec::new(),
        }
    }
}

impl Dispatch for Inline {
    fn send(&mut self, command: Command) {
        let observe = &mut self.observe;
        let pending = &mut self.pending;
        self.sequencer.run(command, &mut |event| {
            observe(&event);
            pending.push(event);
        });
    }

    fn poll(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.pending)
    }
}
