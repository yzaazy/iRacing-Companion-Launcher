//! Runs launch and close sequences off the UI thread (the deliberate difference from `ui/main_window.py`).
//!
//! [`Sequencer`] holds `launch_apps`, `_launch_selected_game` and `close_apps`
//! line for line; [`Worker`] runs it on a background thread, one command at a
//! time in the order sent, and [`Inline`] runs it on the caller's thread for tests.
//!
//! Python reads two things from the window mid-sequence: each card's checkbox,
//! and the selected game card's status in `close_apps`. Checkboxes arrive
//! captured in the [`Command`] (deliberate difference 1). The game status is
//! mirrored here: it changes only at startup (the seed), through this
//! sequencer's own events, and through Browse, which only turns `not_found`
//! into `idle` and so never changes the one question asked of it, "is it
//! `running`?".

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::contract::{Command, Event, LogLevel, Status};
use crate::core::log;
use crate::managers::Backend;

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
    pub fn run(&mut self, command: Command, emit: &mut dyn FnMut(Event)) {
        let mut out = Out {
            emit,
            game_status: &mut self.game_status,
        };
        match command {
            Command::Launch { apps, game } => {
                launch_apps(self.backend.as_mut(), &mut out, &apps, game.as_deref())
            }
            Command::Close { apps, game } => {
                close_apps(self.backend.as_mut(), &mut out, &apps, game.as_deref())
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
        self.game_status.insert(name.to_owned(), status.clone());
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

/// `launch_apps` ([main_window.py:368-429]). `apps` are the checked cards at the press.
fn launch_apps(backend: &mut dyn Backend, out: &mut Out, apps: &[String], game: Option<&str>) {
    out.divider();
    out.log(LogLevel::Launch, "Starting launch sequence...");

    for app in backend.app_list() {
        if !apps.contains(&app) {
            continue;
        }

        let Some(path) = backend.find_app_path(&app) else {
            out.log(
                LogLevel::Warning,
                &format!("Skipping {app} - not configured"),
            );
            continue;
        };

        if backend.is_app_running(&app) {
            out.log(LogLevel::Info, &format!("Skipping {app} - already running"));
            let children = backend.get_child_names(&app);
            out.helpers("helpers", &children);
            out.app(&app, Status::Running, children.len());
            continue;
        }

        out.app(&app, Status::Starting, 0);
        out.log(LogLevel::Info, &format!("Launching {app}..."));

        if backend.launch_app(&app, &path) {
            out.log(LogLevel::Success, &format!("{app} started successfully"));
            let children = backend.get_child_names(&app);
            out.helpers("helpers", &children);
            out.app(&app, Status::Running, children.len());
        } else {
            out.log(LogLevel::Error, &format!("{app} failed to start"));
            out.log(LogLevel::Warning, &format!("Try reinstalling {app}"));
            out.app(&app, Status::Failed, 0);
        }
    }

    // Logged whatever happened above, even with nothing selected.
    out.log(LogLevel::Success, "All apps launched!");

    if let Some(game) = game {
        launch_selected_game(backend, out, game);
    }

    out.log(LogLevel::Success, "Launch sequence complete!");
    (out.emit)(Event::LaunchFinished);
}

/// `_launch_selected_game` ([main_window.py:431-454]).
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

/// `close_apps` ([main_window.py:456-517]).
fn close_apps(backend: &mut dyn Backend, out: &mut Out, apps: &[String], game: Option<&str>) {
    out.divider();
    out.log(LogLevel::Close, "Closing applications...");

    for app in backend.app_list() {
        if !apps.contains(&app) {
            continue;
        }

        let path = backend.find_app_path(&app);
        // No detected path: the first close_app already kills a running
        // instance, so the second one below reports "was not running" and the
        // card goes back to not-found. Ported unchanged.
        if path.is_none() && !backend.close_app(&app) {
            continue;
        }

        out.app(&app, Status::Stopping, 0);
        out.log(LogLevel::Info, &format!("Closing {app}..."));
        let children = backend.get_child_names(&app);
        out.helpers("helpers to close", &children);

        if backend.close_app(&app) {
            out.log(LogLevel::Success, &format!("{app} closed"));
            out.app(&app, Status::Stopped, 0);
        } else {
            out.log(LogLevel::Warning, &format!("{app} was not running"));
            let status = if path.is_none() {
                Status::NotFound
            } else {
                Status::Idle
            };
            out.app(&app, status, 0);
        }
    }

    if let Some(game) = game
        && out.game_status.get(game) == Some(&Status::Running)
    {
        if backend.is_game_running(game) {
            out.log(
                LogLevel::Warning,
                &format!("⚠ {game} is still running - please close it manually"),
            );
        } else {
            out.log(LogLevel::Success, &format!("{game} was closed manually"));
            out.game(game, Status::Idle);
        }
    }

    out.log(LogLevel::Success, "All apps closed!");
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
