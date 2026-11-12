//! The controller: owns the window's state, handles `UiAction`s and applies worker `Event`s.
//!
//! It is the Python app's main window (up to v2.3.2) minus the widgets: [`ViewState`]
//! stands in for the cards and buttons, and every handler makes the same
//! state changes, config writes, saves and log lines as its Python twin. The
//! launch and close sequences run in [`crate::worker`].
//!
//! ## Buttons across a queue
//!
//! [`ButtonState::normal`] is the Python widget state, changed
//! where Python changes them: recomputing the button labels sets both, and a
//! press disables its own button. The end of a sequence recomputes the labels,
//! where Python enabled its button unconditionally (#11).
//! On top of that a button stays disabled while any command of its kind is
//! queued or running. Python can only re-enable a button mid-sequence through
//! `update()` re-entrancy (a checkbox clicked while the log repaints), which a
//! worker thread does not have; without this rule such a click would re-arm a
//! button whose sequence is still running. With one command at a time, which
//! is all Python defines, the visible states are identical.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::core::config::{SharedConfig, config_key};
use crate::core::log;
use crate::managers::Backend;
use crate::model::{AppCard, GameCard, LogLevel, Status, UiAction, ViewState};
use crate::win::basename;
use crate::worker::{Command, Dispatch, Event};

/// The calls the controller makes on the config store and its `configparser`-format settings.
pub trait Settings {
    /// `config.getboolean('Settings', key, fallback=...)`.
    fn bool_setting(&self, key: &str, fallback: bool) -> bool;
    /// `config.get('Settings', key, fallback=None)`.
    fn setting(&self, key: &str) -> Option<String>;
    /// `config.set('Settings', key, value)`; does not save.
    fn set_setting(&mut self, key: &str, value: &str);
    /// `save_config()`: true when the file was written.
    fn save(&mut self) -> bool;
    /// `set_app_path(key, path)`, which saves; `Err` is Python's `False` return.
    fn set_app_path(&mut self, key: &str, path: &str) -> io::Result<()>;
}

impl Settings for SharedConfig {
    fn bool_setting(&self, key: &str, fallback: bool) -> bool {
        let store = self.lock();
        store.bool_setting(key, fallback)
    }
    fn setting(&self, key: &str) -> Option<String> {
        let store = self.lock();
        store.setting(key)
    }
    fn set_setting(&mut self, key: &str, value: &str) {
        let mut store = self.lock();
        store.set_setting(key, value);
    }
    fn save(&mut self) -> bool {
        let store = self.lock();
        store.save().is_ok()
    }
    fn set_app_path(&mut self, key: &str, path: &str) -> io::Result<()> {
        let mut store = self.lock();
        store.set_app_path(key, path)
    }
}

/// An `askopenfilename` call: title, `(label, pattern)` filters with Tk's
/// `*.exe;*.lnk` patterns, and the initial directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogRequest {
    pub title: String,
    pub filters: Vec<(String, String)>,
    pub initial_dir: String,
}

/// The native open dialog; `None` when cancelled. The real one is
/// [`crate::dialog::NativeDialog`].
pub trait FileDialog {
    fn pick(&mut self, request: &DialogRequest) -> Option<String>;
}

/// `filedialog.askopenfilename`'s `initialdir` in both Browse handlers.
const BROWSE_DIR: &str = "C:\\Program Files";

/// `str(True)` / `str(False)`.
fn py_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn enabled_key(name: &str) -> String {
    format!("{}_enabled", config_key(name))
}

/// The Launch or the Close button: its Python widget state, and how many
/// commands of its kind are queued or running (see the module doc).
#[derive(Debug, Clone, Copy)]
struct ButtonState {
    normal: bool,
    pending: usize,
}

impl ButtonState {
    /// Enabled, nothing queued: the widgets as Python creates them.
    const IDLE: Self = Self {
        normal: true,
        pending: 0,
    };

    fn enabled(self) -> bool {
        self.normal && self.pending == 0
    }

    /// A press: disables the button and queues its command.
    fn press(&mut self) {
        self.normal = false;
        self.pending += 1;
    }

    /// The end of its sequence: its command is no longer queued.
    fn finish(&mut self) {
        self.pending = self.pending.saturating_sub(1);
    }
}

/// Sees every log line and card status the controller produces itself.
type Trace = Box<dyn FnMut(&Event)>;

/// The controller. Built in two steps because startup needs the managers on
/// this thread before they move to the worker: [`Controller::startup`], then
/// [`Startup::attach`].
pub struct Controller {
    state: ViewState,
    settings: Box<dyn Settings>,
    app_exes: HashMap<String, String>,
    game_exes: HashMap<String, String>,
    launch: ButtonState,
    close: ButtonState,
    trace: Option<Trace>,
    dispatch: Option<Box<dyn Dispatch>>,
}

/// A controller that has run its startup sequence but has no worker yet.
pub struct Startup(Controller);

impl Startup {
    /// Every game card's status, to seed the worker's mirror.
    pub fn game_statuses(&self) -> HashMap<String, Status> {
        self.0
            .state
            .games
            .iter()
            .map(|g| (g.name.clone(), g.status))
            .collect()
    }

    pub fn attach(mut self, dispatch: Box<dyn Dispatch>) -> Controller {
        self.0.dispatch = Some(dispatch);
        self.0
    }
}

impl Controller {
    /// What the Python window did on creation, up to the first paint: builds the cards
    /// and runs the initial status check. `trace` sees every log line and card
    /// status this controller produces itself (tests record transcripts with it).
    pub fn startup(
        backend: &mut dyn Backend,
        settings: Box<dyn Settings>,
        version: &str,
        trace: Option<Trace>,
    ) -> Startup {
        let apps = backend.app_list();
        let games = backend.game_list();
        let app_exes = apps
            .iter()
            .filter_map(|a| Some((a.clone(), backend.app_exe(a)?)))
            .collect();
        let game_exes = games
            .iter()
            .filter_map(|g| Some((g.clone(), backend.game_exe(g)?)))
            .collect();
        let state = ViewState {
            version: version.to_owned(),
            // A new StatusCard is checked and idle, a GameCard idle.
            apps: apps
                .iter()
                .map(|name| AppCard {
                    name: name.clone(),
                    status: Status::Idle,
                    checked: true,
                    helper_count: 0,
                })
                .collect(),
            games: games
                .iter()
                .map(|name| GameCard {
                    name: name.clone(),
                    status: Status::Idle,
                })
                .collect(),
            selected_game: None,
            log: Vec::new(),
            // The texts the Python widgets are created with.
            select_all_label: "Deselect All".to_owned(),
            launch_label: "Launch All".to_owned(),
            close_label: "Close All".to_owned(),
            launch_enabled: true,
            close_enabled: true,
        };
        let mut controller = Controller {
            state,
            settings,
            app_exes,
            game_exes,
            launch: ButtonState::IDLE,
            close: ButtonState::IDLE,
            trace,
            dispatch: None,
        };
        controller.initialize_app_states(backend);
        Startup(controller)
    }

    pub fn state(&self) -> &ViewState {
        &self.state
    }

    /// The startup check: each app's and game's status, its log line, the checkboxes and the
    /// game selection from config, then the button labels.
    fn initialize_app_states(&mut self, backend: &mut dyn Backend) {
        self.emit(Event::Log(log::divider()));
        self.log(LogLevel::Info, "Checking application status...");

        for app in backend.app_list() {
            let path = backend.find_app_path(&app);
            let key = enabled_key(&app);

            if path.is_none() {
                self.app_status(&app, Status::NotFound, 0);
                self.log(LogLevel::Warning, &format!("{app} - not configured"));
                // Not-found apps start unselected, and config remembers that.
                self.settings.set_setting(&key, "False");
            } else {
                if backend.is_app_running(&app) {
                    let children = backend.child_names(&app);
                    self.app_status(&app, Status::Running, children.len());
                    self.log(LogLevel::Success, &format!("{app} - already running"));
                    if !children.is_empty() {
                        self.log(
                            LogLevel::Info,
                            &format!("  helpers: {}", children.join(", ")),
                        );
                    }
                } else {
                    self.app_status(&app, Status::Idle, 0);
                    self.log(LogLevel::Info, &format!("{app} - configured"));
                }

                let enabled = self.settings.bool_setting(&key, true);
                self.set_checked(&app, enabled);
            }
        }

        // Python ignores save_config's result too.
        let _ = self.settings.save();

        for game in backend.game_list() {
            if backend.find_game_path(&game).is_none() {
                self.game_status(&game, Status::NotFound);
                self.log(LogLevel::Warning, &format!("{game} - not configured"));
            } else if backend.is_game_running(&game) {
                self.game_status(&game, Status::Running);
                self.log(LogLevel::Success, &format!("{game} - already running"));
            } else {
                self.game_status(&game, Status::Idle);
                self.log(LogLevel::Info, &format!("{game} - configured"));
            }
        }

        // A saved game that is not found starts as None (#10). The setting is
        // kept, so the choice comes back once the game is found again.
        let saved = self.settings.setting("selected_game").unwrap_or_default();
        self.state.selected_game = self
            .state
            .games
            .iter()
            .any(|g| g.name == saved && g.status != Status::NotFound)
            .then_some(saved);

        self.update_button_text();
        self.update_select_all_button();
    }

    /// One user action from the UI. `dialog` serves Browse.
    pub fn handle(&mut self, action: UiAction, dialog: &mut dyn FileDialog) {
        match action {
            UiAction::ToggleApp { name, checked } => self.on_checkbox_change(&name, checked),
            UiAction::ToggleSelectAll => self.toggle_select_all(),
            UiAction::SelectGame(game) => self.on_game_selected(game),
            UiAction::BrowseApp(name) => self.browse_for_app(&name, dialog),
            UiAction::BrowseGame(name) => self.browse_for_game(&name, dialog),
            UiAction::Launch => self.launch_apps(),
            UiAction::Close => self.close_apps(),
        }
    }

    /// Applies the worker's events. True if there were any.
    pub fn pump(&mut self) -> bool {
        let Some(dispatch) = self.dispatch.as_mut() else {
            return false;
        };
        let events = dispatch.poll();
        let any = !events.is_empty();
        for event in events {
            self.apply(event);
        }
        any
    }

    /// The Select All / Deselect All button: found apps only.
    fn toggle_select_all(&mut self) {
        let found: Vec<(String, bool)> = self
            .state
            .apps
            .iter()
            .filter(|c| c.status != Status::NotFound)
            .map(|c| (c.name.clone(), c.checked))
            .collect();
        if found.is_empty() {
            return;
        }
        let checked = found.iter().filter(|(_, c)| *c).count();
        let select_all = checked < found.len();

        for (name, _) in &found {
            self.set_checked(name, select_all);
            self.settings
                .set_setting(&enabled_key(name), py_bool(select_all));
        }

        // Python ignores save_config's result too.
        let _ = self.settings.save();
        self.update_button_text();
        self.update_select_all_button();
    }

    /// The Select All button's label: "Deselect All" only when every found app is checked.
    fn update_select_all_button(&mut self) {
        let found: Vec<bool> = self
            .state
            .apps
            .iter()
            .filter(|c| c.status != Status::NotFound)
            .map(|c| c.checked)
            .collect();
        let all_checked = !found.is_empty() && found.iter().all(|c| *c);
        self.state.select_all_label = if all_checked {
            "Deselect All"
        } else {
            "Select All"
        }
        .to_owned();
    }

    /// The Launch and Close labels and their enabled state, from the checked apps and the game.
    fn update_button_text(&mut self) {
        let count = self.state.apps.iter().filter(|c| c.checked).count();
        self.state.launch_label = match &self.state.selected_game {
            Some(game) => format!("Launch with {game} ({count})"),
            None => format!("Launch apps ({count})"),
        };
        self.state.close_label = format!("Close all ({count})");

        let enabled = count != 0 || self.state.selected_game.is_some();
        self.launch.normal = enabled;
        self.close.normal = enabled;
        self.refresh_buttons();
    }

    fn refresh_buttons(&mut self) {
        self.state.launch_enabled = self.launch.enabled();
        self.state.close_enabled = self.close.enabled();
    }

    /// An app's checkbox changed: the UI has already flipped the box.
    fn on_checkbox_change(&mut self, name: &str, checked: bool) {
        self.set_checked(name, checked);
        self.settings
            .set_setting(&enabled_key(name), py_bool(checked));
        // Python ignores save_config's result too.
        let _ = self.settings.save();
        self.update_button_text();
        self.update_select_all_button();
    }

    /// A game radio was selected; the None radio passes `""`.
    fn on_game_selected(&mut self, game: Option<String>) {
        self.settings
            .set_setting("selected_game", game.as_deref().unwrap_or(""));
        self.state.selected_game = game;
        // Python ignores save_config's result too.
        let _ = self.settings.save();
        self.update_button_text();
    }

    /// Browse on an app card: accepts the exact exe name, case-insensitive.
    fn browse_for_app(&mut self, name: &str, dialog: &mut dyn FileDialog) {
        let Some(expected) = self.app_exes.get(name).cloned() else {
            return;
        };
        let request = DialogRequest {
            title: format!("Select {name} executable"),
            filters: vec![
                ("Executable files".to_owned(), "*.exe".to_owned()),
                ("All files".to_owned(), "*.*".to_owned()),
            ],
            initial_dir: BROWSE_DIR.to_owned(),
        };
        let Some(path) = dialog.pick(&request).filter(|p| !p.is_empty()) else {
            return;
        };

        if !self.check_exe(&path, &expected) {
            return;
        }

        // Python ignores set_app_path's result here.
        let _ = self.settings.set_app_path(&config_key(name), &path);

        self.app_status(name, Status::Idle, 0);
        self.set_checked(name, true);
        self.settings.set_setting(&enabled_key(name), py_bool(true));
        // Python ignores save_config's result too.
        let _ = self.settings.save();

        self.log(
            LogLevel::Success,
            &format!("{name} path configured: {path}"),
        );
        self.update_button_text();
        self.update_select_all_button();
    }

    /// Browse on a game card: accepts any `.lnk`, otherwise the exact exe name.
    fn browse_for_game(&mut self, name: &str, dialog: &mut dyn FileDialog) {
        let Some(expected) = self.game_exes.get(name).cloned() else {
            return;
        };
        let request = DialogRequest {
            title: format!("Select {name} executable or shortcut"),
            filters: vec![
                (
                    "Executable and Shortcuts".to_owned(),
                    "*.exe;*.lnk".to_owned(),
                ),
                ("Executable files".to_owned(), "*.exe".to_owned()),
                ("Shortcuts".to_owned(), "*.lnk".to_owned()),
                ("All files".to_owned(), "*.*".to_owned()),
            ],
            initial_dir: BROWSE_DIR.to_owned(),
        };
        let Some(path) = dialog.pick(&request).filter(|p| !p.is_empty()) else {
            return;
        };

        if path.to_lowercase().ends_with(".lnk") {
            if !Path::new(&path).exists() {
                self.log(
                    LogLevel::Error,
                    &format!("Selected file does not exist: {path}"),
                );
                return;
            }
        } else if !self.check_exe(&path, &expected) {
            return;
        }

        let key = format!("game_{}", config_key(name));
        // Python ignores set_app_path's result here.
        let _ = self.settings.set_app_path(&key, &path);

        self.game_status(name, Status::Idle);
        self.log(
            LogLevel::Success,
            &format!("{name} path configured: {path}"),
        );
        self.update_button_text();
    }

    /// The exe-name and existence checks both Browse handlers share, with their error lines.
    fn check_exe(&mut self, path: &str, expected: &str) -> bool {
        let selected = basename(path);
        if selected.to_lowercase() != expected.to_lowercase() {
            let message = format!("Invalid file selected. Expected {expected}, got {selected}");
            self.log(LogLevel::Error, &message);
            return false;
        }
        if !Path::new(path).exists() {
            self.log(
                LogLevel::Error,
                &format!("Selected file does not exist: {path}"),
            );
            return false;
        }
        true
    }

    /// `launch_apps` up to the sequence: disable Launch and queue it with the
    /// selection as it is now.
    fn launch_apps(&mut self) {
        if !self.state.launch_enabled {
            return;
        }
        self.launch.press();
        self.refresh_buttons();
        let command = Command::Launch {
            apps: self.checked_apps(),
            game: self.state.selected_game.clone(),
        };
        self.send(command);
    }

    /// `close_apps` up to the sequence, as [`Self::launch_apps`].
    fn close_apps(&mut self) {
        if !self.state.close_enabled {
            return;
        }
        self.close.press();
        self.refresh_buttons();
        let command = Command::Close {
            apps: self.checked_apps(),
            game: self.state.selected_game.clone(),
        };
        self.send(command);
    }

    fn send(&mut self, command: Command) {
        if let Some(dispatch) = self.dispatch.as_mut() {
            dispatch.send(command);
        }
    }

    fn checked_apps(&self) -> Vec<String> {
        self.state
            .apps
            .iter()
            .filter(|c| c.checked)
            .map(|c| c.name.clone())
            .collect()
    }

    /// Applies an event from the worker or from this controller.
    fn apply(&mut self, event: Event) {
        match event {
            Event::Log(line) => self.state.log.push(line),
            Event::AppStatus {
                name,
                status,
                helper_count,
            } => {
                if let Some(card) = self.state.apps.iter_mut().find(|c| c.name == name) {
                    // StatusCard.set_status: not-found unchecks (and disables) the box;
                    // leaving not-found re-enables it without checking it.
                    if status == Status::NotFound {
                        card.checked = false;
                    }
                    card.helper_count = if status == Status::Running {
                        helper_count
                    } else {
                        0
                    };
                    card.status = status;
                }
            }
            Event::GameStatus { name, status } => {
                if let Some(card) = self.state.games.iter_mut().find(|c| c.name == name) {
                    card.status = status;
                }
            }
            // The end of a sequence applies the button rule, so a button is not
            // re-enabled when nothing is selected any more (#11).
            Event::LaunchFinished => {
                self.launch.finish();
                self.update_button_text();
            }
            Event::CloseFinished => {
                self.close.finish();
                self.update_button_text();
            }
        }
    }

    fn emit(&mut self, event: Event) {
        if let Some(trace) = self.trace.as_mut() {
            trace(&event);
        }
        self.apply(event);
    }

    fn log(&mut self, level: LogLevel, message: &str) {
        self.emit(Event::Log(log::line(level, message)));
    }

    fn app_status(&mut self, name: &str, status: Status, helper_count: usize) {
        self.emit(Event::AppStatus {
            name: name.to_owned(),
            status,
            helper_count,
        });
    }

    fn game_status(&mut self, name: &str, status: Status) {
        self.emit(Event::GameStatus {
            name: name.to_owned(),
            status,
        });
    }

    fn set_checked(&mut self, name: &str, checked: bool) {
        if let Some(card) = self.state.apps.iter_mut().find(|c| c.name == name) {
            card.checked = checked;
        }
    }
}
