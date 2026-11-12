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
use std::ffi::c_void;
use std::path::Path;
use std::sync::PoisonError;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FILEOPENDIALOGOPTIONS, FOS_ALLOWMULTISELECT, FOS_FORCEFILESYSTEM, FOS_NOVALIDATE,
    FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
    IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::PCWSTR;

use crate::core::config::{self, config_key};
use crate::core::log;
use crate::managers::{Backend, SharedConfig};
use crate::model::{AppCard, GameCard, LogLevel, Status, UiAction, ViewState};
use crate::worker::{ComApartment, Command, Dispatch, Event};

/// The calls `main_window.py` makes on `ConfigManager` and its `config` parser.
pub trait Settings {
    /// `config.getboolean('Settings', key, fallback=...)`.
    fn bool_setting(&self, key: &str, fallback: bool) -> bool;
    /// `config.get('Settings', key, fallback=None)`.
    fn setting(&self, key: &str) -> Option<String>;
    /// `config.set('Settings', key, value)`; does not save.
    fn set_setting(&mut self, key: &str, value: &str);
    /// `save_config()`.
    fn save(&mut self) -> bool;
    /// `set_app_path(key, path)`, which saves. False where Python raises
    /// because `configparser` refuses the value; a failed save is not a
    /// refusal, because the window ignores `set_app_path`'s return value.
    fn set_app_path(&mut self, key: &str, path: &str) -> bool;
}

impl Settings for SharedConfig {
    fn bool_setting(&self, key: &str, fallback: bool) -> bool {
        let store = self.lock().unwrap_or_else(PoisonError::into_inner);
        store.bool_setting(key, fallback)
    }
    fn setting(&self, key: &str) -> Option<String> {
        let store = self.lock().unwrap_or_else(PoisonError::into_inner);
        store.setting(key)
    }
    fn set_setting(&mut self, key: &str, value: &str) {
        let mut store = self.lock().unwrap_or_else(PoisonError::into_inner);
        store.set_setting(key, value);
    }
    fn save(&mut self) -> bool {
        let store = self.lock().unwrap_or_else(PoisonError::into_inner);
        store.save()
    }
    fn set_app_path(&mut self, key: &str, path: &str) -> bool {
        let mut store = self.lock().unwrap_or_else(PoisonError::into_inner);
        // ConfigStore::set_app_path returns false for a refusal and for a
        // failed save alike; only the refusal stops Browse.
        store.set_app_path(key, path);
        config::configparser_accepts(path)
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

/// The native open dialog; `None` when cancelled.
pub trait FileDialog {
    fn pick(&mut self, request: &DialogRequest) -> Option<String>;
}

/// The Windows open dialog as Tk 8.6's `tk_getOpenFile` opens it
/// (`GetFileNameVista` in `win/tkWinDialog.c`), modal to the window.
///
/// Tk keeps the dialog's default options and adds its own; it never sets
/// `FOS_NODEREFERENCELINKS`, so a picked shortcut comes back as its target.
/// Opening the dialog here, rather than through a crate with its own option
/// set, keeps that and every other option equal to Tk's.
pub struct NativeDialog<'a> {
    parent: &'a eframe::Frame,
}

impl<'a> NativeDialog<'a> {
    pub fn new(parent: &'a eframe::Frame) -> Self {
        Self { parent }
    }

    fn owner(&self) -> Option<HWND> {
        match self.parent.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut c_void)),
            _ => None,
        }
    }
}

impl FileDialog for NativeDialog<'_> {
    fn pick(&mut self, request: &DialogRequest) -> Option<String> {
        // Tk calls CoInitialize on its thread before its first dialog.
        let _com = ComApartment::enter();
        let dialog = open_dialog(request)?;
        // SAFETY: `dialog` is a live IFileOpenDialog; the owner is our window or none.
        unsafe { dialog.Show(self.owner()) }.ok()?;
        // SAFETY: Show succeeded, so the dialog holds a result.
        let item = unsafe { dialog.GetResult() }.ok()?;
        // SAFETY: `item` is a live IShellItem.
        let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
        // SAFETY: GetDisplayName returned a NUL-terminated string we now own.
        let path = unsafe { name.to_string() };
        // SAFETY: the string came from the COM task allocator and is freed once.
        unsafe { CoTaskMemFree(Some(name.0.cast_const().cast())) };
        // Tk's ConvertExternalFilename turns backslashes into forward slashes,
        // and Python logs and saves the path as returned.
        path.ok().map(|p| p.replace('\\', "/"))
    }
}

/// `GetFileNameVista` up to `Show`: filters, options, title and folder, in Tk's order.
fn open_dialog(request: &DialogRequest) -> Option<IFileOpenDialog> {
    // SAFETY: plain COM creation on a thread with an apartment.
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    // SAFETY: `dialog` is live for every call below.
    let defaults = unsafe { dialog.GetOptions() }.ok()?;

    // Tk passes each pattern list joined with ';', as the request holds it.
    let texts: Vec<(Vec<u16>, Vec<u16>)> = request
        .filters
        .iter()
        .map(|(label, pattern)| (wide(label), wide(pattern)))
        .collect();
    let specs: Vec<COMDLG_FILTERSPEC> = texts
        .iter()
        .map(|(label, pattern)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(pattern.as_ptr()),
        })
        .collect();
    if !specs.is_empty() {
        // SAFETY: `specs` points into `texts`, both alive across the call.
        unsafe { dialog.SetFileTypes(&specs) }.ok()?;
        // No -typevariable: Tk shows the first filter (indices are 1-based).
        unsafe { dialog.SetFileTypeIndex(1) }.ok()?;
    }

    unsafe { dialog.SetOptions(tk_open_options(defaults)) }.ok()?;

    let title = wide(&request.title);
    // SAFETY: `title` is NUL-terminated and alive across the call.
    unsafe { dialog.SetTitle(PCWSTR(title.as_ptr())) }.ok()?;

    // Tk ignores every failure here and opens wherever the dialog would.
    let dir = wide(&request.initial_dir);
    // SAFETY: `dir` is NUL-terminated and alive across the call.
    let folder: windows::core::Result<IShellItem> =
        unsafe { SHCreateItemFromParsingName(PCWSTR(dir.as_ptr()), None) };
    if let Ok(folder) = folder {
        // SAFETY: both interfaces are live.
        let _ = unsafe { dialog.SetFolder(&folder) };
    }
    Some(dialog)
}

/// The options Tk sets on an open dialog, over the dialog's defaults: it adds
/// `FOS_FORCEFILESYSTEM | FOS_NOVALIDATE | FOS_PATHMUSTEXIST`, clears folder
/// picking and multi-select, and adds `FOS_OVERWRITEPROMPT` because
/// `-confirmoverwrite` defaults to on for every file dialog.
fn tk_open_options(defaults: FILEOPENDIALOGOPTIONS) -> FILEOPENDIALOGOPTIONS {
    let mut flags = defaults | FOS_FORCEFILESYSTEM | FOS_NOVALIDATE | FOS_PATHMUSTEXIST;
    flags &= !FOS_PICKFOLDERS;
    flags &= !FOS_ALLOWMULTISELECT;
    flags | FOS_OVERWRITEPROMPT
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
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

/// `ntpath.basename`: after the drive, after the last `\` or `/`.
fn basename(path: &str) -> &str {
    let bytes = path.as_bytes();
    let tail = if bytes.len() >= 2 && bytes[1] == b':' {
        &path[2..]
    } else {
        path
    };
    tail.rsplit(['\\', '/']).next().unwrap_or(tail)
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

        self.settings.save();

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

        self.settings.save();
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
        self.settings.save();
        self.update_button_text();
        self.update_select_all_button();
    }

    /// `_on_game_selected`; the None radio passes `""`.
    fn on_game_selected(&mut self, game: Option<String>) {
        self.settings
            .set_setting("selected_game", game.as_deref().unwrap_or(""));
        self.state.selected_game = game;
        self.settings.save();
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

        // Python raises out of the handler here; nothing after it happens.
        if !self.settings.set_app_path(&config_key(name), &path) {
            return;
        }

        self.app_status(name, Status::Idle, 0);
        self.set_checked(name, true);
        self.settings.set_setting(&enabled_key(name), py_bool(true));
        self.settings.save();

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

        if !self
            .settings
            .set_app_path(&format!("game_{}", config_key(name)), &path)
        {
            return;
        }

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

#[cfg(test)]
mod tests {
    use super::{ComApartment, DialogRequest, basename, open_dialog, tk_open_options};
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_NODEREFERENCELINKS,
        FOS_NOVALIDATE, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
    };

    fn game_request() -> DialogRequest {
        DialogRequest {
            title: "Select iRacing executable or shortcut".to_owned(),
            filters: vec![
                (
                    "Executable and Shortcuts".to_owned(),
                    "*.exe;*.lnk".to_owned(),
                ),
                ("Executable files".to_owned(), "*.exe".to_owned()),
                ("Shortcuts".to_owned(), "*.lnk".to_owned()),
                ("All files".to_owned(), "*.*".to_owned()),
            ],
            initial_dir: "C:\\Program Files".to_owned(),
        }
    }

    #[test]
    fn options_are_tks_and_shortcuts_are_followed_as_in_tk() {
        let flags = tk_open_options(FOS_PICKFOLDERS | FOS_NOCHANGEDIR);
        assert_eq!(
            flags,
            FOS_NOCHANGEDIR
                | FOS_FORCEFILESYSTEM
                | FOS_NOVALIDATE
                | FOS_PATHMUSTEXIST
                | FOS_OVERWRITEPROMPT
        );
        assert_eq!(flags & FOS_NODEREFERENCELINKS, Default::default());
    }

    /// Builds the real dialog up to `Show` (which would wait for a person) and
    /// reads back what it was given.
    #[test]
    fn dialog_is_configured_like_tks() {
        let _com = ComApartment::enter();
        let dialog = open_dialog(&game_request()).expect("dialog configured");
        // SAFETY: `dialog` is live.
        let flags = unsafe { dialog.GetOptions() }.unwrap();
        for wanted in [
            FOS_FORCEFILESYSTEM,
            FOS_NOVALIDATE,
            FOS_PATHMUSTEXIST,
            FOS_FILEMUSTEXIST,
            FOS_OVERWRITEPROMPT,
        ] {
            assert_eq!(flags & wanted, wanted, "{wanted:?} set");
        }
        assert_eq!(flags & FOS_NODEREFERENCELINKS, Default::default());
        // SAFETY: as above.
        assert_eq!(unsafe { dialog.GetFileTypeIndex() }.unwrap(), 1);
        // SAFETY: as above.
        let folder = unsafe { dialog.GetFolder() }.unwrap();
        // SAFETY: `folder` is live; the returned string is freed by the allocator below.
        let name = unsafe { folder.GetDisplayName(super::SIGDN_FILESYSPATH) }.unwrap();
        // SAFETY: NUL-terminated, owned here.
        let path = unsafe { name.to_string() }.unwrap();
        // SAFETY: allocated by the COM task allocator, freed once.
        unsafe { super::CoTaskMemFree(Some(name.0.cast_const().cast())) };
        assert_eq!(path.to_lowercase(), "c:\\program files");
    }

    #[test]
    fn basename_splits_like_ntpath() {
        assert_eq!(basename("C:/Program Files/A/Fanatec.exe"), "Fanatec.exe");
        assert_eq!(basename(r"C:\A\b.exe"), "b.exe");
        assert_eq!(basename("C:b.exe"), "b.exe");
        assert_eq!(basename("b.exe"), "b.exe");
        assert_eq!(basename("C:/A/"), "");
    }
}
