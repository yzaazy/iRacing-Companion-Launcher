//! Ports `ui/main_window.py`: the controller that owns state, handles `UiAction`s and applies worker `Event`s.
//!
//! The controller is `iRacingLauncherGUI` minus the widgets: [`ViewState`]
//! stands in for the cards and buttons, and every handler makes the same
//! state changes, config writes, saves and log lines as its Python twin. The
//! launch and close sequences run in [`crate::worker`].
//!
//! ## Buttons across a queue
//!
//! `launch_normal` and `close_normal` are the Python widget states, changed
//! exactly where Python changes them: `_update_button_text` sets both, a
//! press disables its own button, and the end of its sequence enables it
//! unconditionally (even where `_update_button_text` would not have).
//! On top of that a button stays disabled while any command of its kind is
//! queued or running. Python can only re-enable a button mid-sequence through
//! `update()` re-entrancy (a checkbox clicked while the log repaints), which a
//! worker thread does not have; without this rule such a click would re-arm a
//! button whose sequence is still running. With one command at a time, which
//! is all Python defines, the visible states are identical.

use std::collections::HashMap;
use std::path::Path;

use crate::core::config::{SetPathError, SharedConfig, config_key};
use crate::core::log;
use crate::managers::Backend;
use crate::model::{AppCard, GameCard, LogLevel, Status, UiAction, ViewState};
use crate::win::basename;
use crate::worker::{Command, Dispatch, Event};

/// The calls `main_window.py` makes on `ConfigManager` and its `config` parser.
pub trait Settings {
    /// `config.getboolean('Settings', key, fallback=...)`.
    fn bool_setting(&self, key: &str, fallback: bool) -> bool;
    /// `config.get('Settings', key, fallback=None)`.
    fn setting(&self, key: &str) -> Option<String>;
    /// `config.set('Settings', key, value)`; does not save.
    fn set_setting(&mut self, key: &str, value: &str);
    /// `save_config()`: true when the file was written.
    fn save(&mut self) -> bool;
    /// `set_app_path(key, path)`, which saves. [`SetPathError::Refused`] is
    /// where Python raises; [`SetPathError::Save`] is its `False` return.
    fn set_app_path(&mut self, key: &str, path: &str) -> Result<(), SetPathError>;
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
    fn set_app_path(&mut self, key: &str, path: &str) -> Result<(), SetPathError> {
        let mut store = self.lock();
        store.set_app_path(key, path)
    }
}

/// An `askopenfilename` call: title, `(label, pattern)` filters with Tk's
/// `*.exe;*.lnk` patterns, and the initial directory.
#[derive(Debug, Clone, PartialEq)]
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

/// Sees every log line and card status the controller produces itself.
pub type Trace = Box<dyn FnMut(&Event)>;

/// The controller. Built in two steps because startup needs the managers on
/// this thread before they move to the worker: [`Controller::startup`], then
/// [`Startup::attach`].
pub struct Controller {
    state: ViewState,
    settings: Box<dyn Settings>,
    app_exes: HashMap<String, String>,
    game_exes: HashMap<String, String>,
    launch_normal: bool,
    close_normal: bool,
    launches_pending: usize,
    closes_pending: usize,
    trace: Option<Trace>,
    dispatch: Option<Box<dyn Dispatch>>,
}

/// A controller that has run its startup sequence but has no worker yet.
pub struct Startup(Controller);

impl Startup {
    pub fn state(&self) -> &ViewState {
        &self.0.state
    }

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
    /// `iRacingLauncherGUI.__init__` up to the first paint: builds the cards
    /// and runs `_initialize_app_states`. `trace` sees every log line and card
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
            launch_normal: true,
            close_normal: true,
            launches_pending: 0,
            closes_pending: 0,
            trace,
            dispatch: None,
        };
        controller.initialize_app_states(backend);
        Startup(controller)
    }

    pub fn state(&self) -> &ViewState {
        &self.state
    }

    /// `_initialize_app_states` ([main_window.py:140-199]).
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
                    let children = backend.get_child_names(&app);
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

        // Restored even when that game is not found: its radio shows selected
        // and disabled.
        let saved = self.settings.setting("selected_game").unwrap_or_default();
        self.state.selected_game = self
            .state
            .games
            .iter()
            .any(|g| g.name == saved)
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

    /// `_toggle_select_all` ([main_window.py:201-222]): found apps only.
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

    /// `_update_select_all_button`: "Deselect All" only when every found app is checked.
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

    /// `_update_button_text` ([main_window.py:245-269]).
    fn update_button_text(&mut self) {
        let count = self.state.apps.iter().filter(|c| c.checked).count();
        self.state.launch_label = match &self.state.selected_game {
            Some(game) => format!("Launch with {game} ({count})"),
            None => format!("Launch apps ({count})"),
        };
        self.state.close_label = format!("Close all ({count})");

        let enabled = count != 0 || self.state.selected_game.is_some();
        self.launch_normal = enabled;
        self.close_normal = enabled;
        self.refresh_buttons();
    }

    fn refresh_buttons(&mut self) {
        self.state.launch_enabled = self.launch_normal && self.launches_pending == 0;
        self.state.close_enabled = self.close_normal && self.closes_pending == 0;
    }

    /// `_on_checkbox_change`: the UI has already flipped the box.
    fn on_checkbox_change(&mut self, name: &str, checked: bool) {
        self.set_checked(name, checked);
        self.settings
            .set_setting(&enabled_key(name), py_bool(checked));
        // Python ignores save_config's result too.
        let _ = self.settings.save();
        self.update_button_text();
        self.update_select_all_button();
    }

    /// `_on_game_selected`; the None radio passes `""`.
    fn on_game_selected(&mut self, game: Option<String>) {
        self.settings
            .set_setting("selected_game", game.as_deref().unwrap_or(""));
        self.state.selected_game = game;
        // Python ignores save_config's result too.
        let _ = self.settings.save();
        self.update_button_text();
    }

    /// `_browse_for_app` ([main_window.py:285-323]): the exact exe name, case-insensitive.
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

        if !self.store_browsed_path(&config_key(name), &path) {
            return;
        }

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

    /// `_browse_for_game` ([main_window.py:325-366]): any `.lnk`, otherwise the exact exe name.
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

        if !self.store_browsed_path(&format!("game_{}", config_key(name)), &path) {
            return;
        }

        self.game_status(name, Status::Idle);
        self.log(
            LogLevel::Success,
            &format!("{name} path configured: {path}"),
        );
        self.update_button_text();
    }

    /// `set_app_path` in both Browse handlers. False where Python raises out of
    /// the handler (configparser refuses the path), so nothing after it happens.
    /// Python ignores a failed save here.
    fn store_browsed_path(&mut self, key: &str, path: &str) -> bool {
        match self.settings.set_app_path(key, path) {
            Err(SetPathError::Refused) => false,
            Ok(()) | Err(SetPathError::Save(_)) => true,
        }
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
        self.launch_normal = false;
        self.launches_pending += 1;
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
        self.close_normal = false;
        self.closes_pending += 1;
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
            // The end of a sequence enables its button unconditionally; the
            // labels are not recomputed, so a count can go stale, as in Python.
            Event::LaunchFinished => {
                self.launches_pending = self.launches_pending.saturating_sub(1);
                self.launch_normal = true;
                self.refresh_buttons();
            }
            Event::CloseFinished => {
                self.closes_pending = self.closes_pending.saturating_sub(1);
                self.close_normal = true;
                self.refresh_buttons();
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
