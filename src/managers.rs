//! Ports `managers/app_manager.py` and `managers/game_manager.py`: detection, launch and close per app and game.
//!
//! The controller and the worker reach both managers through the [`Backend`]
//! trait, so tests can swap in a scripted fake that never touches a process.

use std::time::Duration;

use crate::core::config::{SharedConfig, config_key};
use crate::core::definitions::{self, AppDef, GameDef};
use crate::detect::paths::find_path_in_list;
use crate::detect::shortcuts::find_shortcut_target;
use crate::detect::steam::check_steam_game_installed;
use crate::process::tracker::ProcessTracker;
use crate::process::{is_process_running, kill_process, launch_process, shell_open};

/// Wait after a tracked app launch (`launch_and_track`'s `wait_time=2.0`).
const APP_LAUNCH_WAIT: Duration = Duration::from_secs(2);
/// `time.sleep(3)` after a `steam://` launch: Steam needs longer to start the game.
const STEAM_LAUNCH_WAIT: Duration = Duration::from_secs(3);
/// `time.sleep(2)` after a `.lnk` launch, and `launch_process`'s default wait for an `.exe`.
const GAME_LAUNCH_WAIT: Duration = Duration::from_secs(2);

/// Everything the window and the launch/close sequences ask of `AppManager`
/// and `GameManager`. Blocking methods (`launch_app`, `close_app`,
/// `launch_game`) do their waits inside the real implementation.
pub trait Backend: Send {
    fn app_list(&self) -> Vec<String>;
    fn app_exe(&self, app: &str) -> Option<String>;
    fn find_app_path(&mut self, app: &str) -> Option<String>;
    fn is_app_running(&mut self, app: &str) -> bool;
    fn get_child_names(&mut self, app: &str) -> Vec<String>;
    fn launch_app(&mut self, app: &str, path: &str) -> bool;
    fn close_app(&mut self, app: &str) -> bool;

    fn game_list(&self) -> Vec<String>;
    fn game_exe(&self, game: &str) -> Option<String>;
    fn find_game_path(&mut self, game: &str) -> Option<String>;
    fn is_game_running(&mut self, game: &str) -> bool;
    fn launch_game(&mut self, game: &str, path: &str) -> bool;
}

/// `AppManager`: companion app detection, launch and close.
pub struct AppManager {
    config: SharedConfig,
    apps: Vec<AppDef>,
    tracker: ProcessTracker,
}

impl AppManager {
    pub fn new(config: SharedConfig, tracker: ProcessTracker) -> Self {
        Self {
            config,
            apps: definitions::apps(),
            tracker,
        }
    }

    fn def(&self, app: &str) -> Option<&AppDef> {
        self.apps.iter().find(|a| a.name == app)
    }

    pub fn get_app_list(&self) -> Vec<String> {
        self.apps.iter().map(|a| a.name.to_owned()).collect()
    }

    pub fn get_app_exe(&self, app: &str) -> Option<String> {
        self.def(app).map(|a| a.exe.to_owned())
    }

    /// Saved path, then Start Menu shortcut, then common paths. A detected
    /// path is saved to config before it is returned.
    pub fn find_app_path(&mut self, app: &str) -> Option<String> {
        let info = self.def(app)?.clone();
        let key = config_key(app);

        if let Some(saved) = self.config.lock().app_path(&key) {
            return Some(saved);
        }
        if let Some(target) = find_shortcut_target(info.shortcut_names, false) {
            // Python raises here if configparser refuses the value (a `%` in
            // the path) and startup crashes; the port keeps the path unsaved.
            self.config.lock().set_app_path(&key, &target);
            return Some(target);
        }
        if let Some(found) = find_path_in_list(&info.paths) {
            self.config.lock().set_app_path(&key, &found);
            return Some(found);
        }
        None
    }

    /// Tracked and alive, otherwise any process with the exe name, so an app
    /// started outside the launcher still shows as running.
    pub fn is_app_running(&mut self, app: &str) -> bool {
        let Some(exe) = self.get_app_exe(app) else {
            return false;
        };
        // A dead tracked entry is dropped by the check and falls through.
        if self.tracker.is_tracked(app) && self.tracker.is_tracked_running(app).0 {
            return true;
        }
        is_process_running(&exe)
    }

    pub fn get_child_count(&mut self, app: &str) -> usize {
        if self.def(app).is_none() {
            return 0;
        }
        self.tracker.is_tracked_running(app).1
    }

    pub fn get_child_names(&mut self, app: &str) -> Vec<String> {
        if self.def(app).is_none() {
            return Vec::new();
        }
        self.tracker.get_child_names(app)
    }

    pub fn launch_app(&mut self, app: &str, path: &str) -> bool {
        let Some(needs_shell_launch) = self.def(app).map(|a| a.needs_shell_launch) else {
            return false;
        };
        self.tracker
            .launch_and_track(app, path, APP_LAUNCH_WAIT, needs_shell_launch)
    }

    /// Tracked close when tracked; otherwise (or when the tracked process was
    /// already gone) kill by exe name so Close works on apps started by hand.
    pub fn close_app(&mut self, app: &str) -> bool {
        let Some(exe) = self.get_app_exe(app) else {
            return false;
        };
        if self.tracker.is_tracked(app) && self.tracker.close_tracked(app) {
            return true;
        }
        kill_process(&exe)
    }
}

/// `GameManager`: racing game detection and launch.
pub struct GameManager {
    config: SharedConfig,
    games: Vec<GameDef>,
}

impl GameManager {
    pub fn new(config: SharedConfig) -> Self {
        Self {
            config,
            games: definitions::games(),
        }
    }

    fn def(&self, game: &str) -> Option<&GameDef> {
        self.games.iter().find(|g| g.name == game)
    }

    pub fn get_game_list(&self) -> Vec<String> {
        self.games.iter().map(|g| g.name.to_owned()).collect()
    }

    pub fn get_game_exe(&self, game: &str) -> Option<String> {
        self.def(game).map(|g| g.exe.to_owned())
    }

    /// Saved path, then the Steam registry. `shortcut_names` and `paths` are
    /// never consulted, as in Python. A saved `steam://` URL never exists on
    /// disk, so a Steam game is re-detected and re-saved on every lookup.
    pub fn find_game_path(&mut self, game: &str) -> Option<String> {
        let appid = self.def(game)?.steam_appid;
        let key = format!("game_{}", config_key(game));

        if let Some(saved) = self.config.lock().app_path(&key) {
            return Some(saved);
        }
        if !appid.is_empty() && check_steam_game_installed(appid) {
            let url = format!("steam://rungameid/{appid}");
            self.config.lock().set_app_path(&key, &url);
            return Some(url);
        }
        None
    }

    pub fn is_game_running(&self, game: &str) -> bool {
        self.def(game).is_some_and(|g| is_process_running(g.exe))
    }

    /// `steam://` and `.lnk` go through the shell, an `.exe` is started
    /// detached and untracked. Success is "the game's exe is running afterwards".
    pub fn launch_game(&mut self, game: &str, path: &str) -> bool {
        let Some(exe) = self.get_game_exe(game) else {
            return false;
        };
        if path.starts_with("steam://") {
            // os.startfile raising is caught by the method's `except Exception`.
            if shell_open(path).is_err() {
                return false;
            }
            std::thread::sleep(STEAM_LAUNCH_WAIT);
        } else if path.to_lowercase().ends_with(".lnk") {
            if shell_open(path).is_err() {
                return false;
            }
            std::thread::sleep(GAME_LAUNCH_WAIT);
        } else if !launch_process(path, GAME_LAUNCH_WAIT) {
            return false;
        }
        is_process_running(&exe)
    }

    /// Unused by the window, as in Python.
    pub fn close_game(&self, game: &str) -> bool {
        self.def(game).is_some_and(|g| kill_process(g.exe))
    }
}

/// The real managers behind the [`Backend`] seam.
pub struct Managers {
    pub apps: AppManager,
    pub games: GameManager,
}

impl Managers {
    /// Both managers share `config`, as both Python managers share one `ConfigManager`.
    pub fn new(config: SharedConfig, tracker: ProcessTracker) -> Self {
        Self {
            apps: AppManager::new(config.clone(), tracker),
            games: GameManager::new(config),
        }
    }
}

impl Backend for Managers {
    fn app_list(&self) -> Vec<String> {
        self.apps.get_app_list()
    }
    fn app_exe(&self, app: &str) -> Option<String> {
        self.apps.get_app_exe(app)
    }
    fn find_app_path(&mut self, app: &str) -> Option<String> {
        self.apps.find_app_path(app)
    }
    fn is_app_running(&mut self, app: &str) -> bool {
        self.apps.is_app_running(app)
    }
    fn get_child_names(&mut self, app: &str) -> Vec<String> {
        self.apps.get_child_names(app)
    }
    fn launch_app(&mut self, app: &str, path: &str) -> bool {
        self.apps.launch_app(app, path)
    }
    fn close_app(&mut self, app: &str) -> bool {
        self.apps.close_app(app)
    }
    fn game_list(&self) -> Vec<String> {
        self.games.get_game_list()
    }
    fn game_exe(&self, game: &str) -> Option<String> {
        self.games.get_game_exe(game)
    }
    fn find_game_path(&mut self, game: &str) -> Option<String> {
        self.games.find_game_path(game)
    }
    fn is_game_running(&mut self, game: &str) -> bool {
        self.games.is_game_running(game)
    }
    fn launch_game(&mut self, game: &str, path: &str) -> bool {
        self.games.launch_game(game, path)
    }
}
