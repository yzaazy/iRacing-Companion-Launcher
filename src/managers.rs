//! The app and game managers: detection, launch and close per app and game.
//!
//! The controller and the worker reach both managers through the [`Backend`]
//! trait, so tests can swap in a scripted fake that never touches a process.
//! The managers' own tests use [`Managers::with_test_host`] instead: the real
//! logic over their own definitions, Start Menu, Steam key and shell.

use std::path::PathBuf;
use std::time::Duration;

use windows::Win32::System::Registry::HKEY_CURRENT_USER;

use crate::core::config::{SharedConfig, config_key};
use crate::core::definitions::{self, AppDef, GameDef};
use crate::detect::paths::find_path_in_list;
use crate::detect::shortcuts::{ShortcutLookup, find_shortcut_target, find_shortcut_target_in};
use crate::detect::steam::{check_steam_game_installed, check_steam_game_installed_in};
use crate::process::tracker::{LaunchMethod, ProcessTracker};
use crate::process::{is_process_running, kill_process, launch_process, shell_open};

/// Wait after a tracked app launch (`launch_and_track`'s `wait_time=2.0`).
const APP_LAUNCH_WAIT: Duration = Duration::from_secs(2);
/// `time.sleep(3)` after a `steam://` launch: Steam needs longer to start the game.
const STEAM_LAUNCH_WAIT: Duration = Duration::from_secs(3);
/// `time.sleep(2)` after a `.lnk` launch, and `launch_process`'s default wait for an `.exe`.
const GAME_LAUNCH_WAIT: Duration = Duration::from_secs(2);

/// The stand-in for `os.startfile` in a [`TestHost`].
pub type ShellOpen = Box<dyn FnMut(&str) -> std::io::Result<()> + Send>;

/// What [`Managers`] consults and calls on the real machine, replaced for
/// tests: the Start Menus, the Steam uninstall key, `os.startfile` for game
/// paths and the launch waits.
pub struct TestHost {
    /// Searched in place of the user's and the all-users Start Menu.
    pub start_menu: Vec<PathBuf>,
    /// A key under `HKEY_CURRENT_USER`, searched in place of
    /// `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`.
    pub steam_uninstall_key: String,
    /// Called in place of `ShellExecute` for `steam://` and `.lnk` game paths.
    pub shell_open: ShellOpen,
    /// In place of the 2 s wait after a tracked app launch.
    pub app_launch_wait: Duration,
    /// In place of the 3 s wait after a `steam://` game launch.
    pub steam_launch_wait: Duration,
    /// In place of the 2 s wait after a `.lnk` or `.exe` game launch.
    pub game_launch_wait: Duration,
}

/// Everything the window and the launch/close sequences ask of `AppManager`
/// and `GameManager`. Blocking methods (`launch_app`, `close_app`,
/// `launch_game`) do their waits inside the real implementation.
pub trait Backend: Send {
    fn app_list(&self) -> Vec<String>;
    fn app_exe(&self, app: &str) -> Option<String>;
    fn find_app_path(&mut self, app: &str) -> Option<String>;
    fn is_app_running(&mut self, app: &str) -> bool;
    fn child_names(&mut self, app: &str) -> Vec<String>;
    /// Names of the games running below the tracked app (the app started
    /// them), which `close_app` leaves running.
    fn games_under(&mut self, app: &str) -> Vec<String>;
    fn launch_app(&mut self, app: &str, path: &str) -> bool;
    fn close_app(&mut self, app: &str) -> bool;

    fn game_list(&self) -> Vec<String>;
    fn game_exe(&self, game: &str) -> Option<String>;
    fn find_game_path(&mut self, game: &str) -> Option<String>;
    fn is_game_running(&mut self, game: &str) -> bool;
    fn launch_game(&mut self, game: &str, path: &str) -> bool;
}

/// The real managers behind the [`Backend`] seam: Python's `AppManager`
/// (companion app detection, launch and close) and `GameManager` (racing game
/// detection and launch) in one value.
pub struct Managers {
    config: SharedConfig,
    apps: Vec<AppDef>,
    games: Vec<GameDef>,
    tracker: ProcessTracker,
    /// `None` on the real machine.
    test_host: Option<TestHost>,
}

impl Managers {
    /// Apps and games share `config`, as both Python managers share one `ConfigManager`.
    pub fn new(config: SharedConfig, mut tracker: ProcessTracker) -> Self {
        let games = definitions::games();
        spare_games(&mut tracker, &games);
        Self {
            config,
            apps: definitions::apps(),
            games,
            tracker,
            test_host: None,
        }
    }

    /// Managers over the test's own definitions and [`TestHost`], for tests.
    pub fn with_test_host(
        config: SharedConfig,
        mut tracker: ProcessTracker,
        apps: Vec<AppDef>,
        games: Vec<GameDef>,
        host: TestHost,
    ) -> Self {
        spare_games(&mut tracker, &games);
        Self {
            config,
            apps,
            games,
            tracker,
            test_host: Some(host),
        }
    }

    fn shortcut_target(&self, shortcut_names: &[&str]) -> Option<String> {
        match &self.test_host {
            None => find_shortcut_target(shortcut_names, ShortcutLookup::Target),
            Some(host) => {
                find_shortcut_target_in(&host.start_menu, shortcut_names, ShortcutLookup::Target)
            }
        }
    }

    fn steam_game_installed(&self, appid: &str) -> bool {
        match &self.test_host {
            None => check_steam_game_installed(appid),
            Some(host) => {
                check_steam_game_installed_in(HKEY_CURRENT_USER, &host.steam_uninstall_key, appid)
            }
        }
    }

    fn shell_open(&mut self, path: &str) -> std::io::Result<()> {
        match &mut self.test_host {
            None => shell_open(path),
            Some(host) => (host.shell_open)(path),
        }
    }

    fn app_launch_wait(&self) -> Duration {
        self.test_host
            .as_ref()
            .map_or(APP_LAUNCH_WAIT, |host| host.app_launch_wait)
    }

    fn steam_launch_wait(&self) -> Duration {
        self.test_host
            .as_ref()
            .map_or(STEAM_LAUNCH_WAIT, |host| host.steam_launch_wait)
    }

    fn game_launch_wait(&self) -> Duration {
        self.test_host
            .as_ref()
            .map_or(GAME_LAUNCH_WAIT, |host| host.game_launch_wait)
    }

    fn app_def(&self, app: &str) -> Option<&AppDef> {
        self.apps.iter().find(|a| a.name == app)
    }

    fn game_def(&self, game: &str) -> Option<&GameDef> {
        self.games.iter().find(|g| g.name == game)
    }
}

/// A game is closed by hand only, so a wrong press on Close never ends a
/// session: a game that a tracked app started is no part of that app's tree.
fn spare_games(tracker: &mut ProcessTracker, games: &[GameDef]) {
    tracker.spare(games.iter().flat_map(GameDef::exes));
}

impl Backend for Managers {
    fn app_list(&self) -> Vec<String> {
        self.apps.iter().map(|a| a.name.to_owned()).collect()
    }

    fn app_exe(&self, app: &str) -> Option<String> {
        self.app_def(app).map(|a| a.exe.to_owned())
    }

    /// Saved path, then Start Menu shortcut, then common paths. A detected
    /// path is saved to config before it is returned.
    fn find_app_path(&mut self, app: &str) -> Option<String> {
        let info = self.app_def(app)?;
        let key = config_key(app);

        if let Some(saved) = self.config.lock().app_path(&key) {
            return Some(saved);
        }
        if let Some(target) = self.shortcut_target(info.shortcut_names) {
            // Python raises here if configparser refuses the value (a `%` in
            // the path) and startup crashes; the port keeps the path unsaved.
            // A failed save is ignored, as in Python.
            let _ = self.config.lock().set_app_path(&key, &target);
            return Some(target);
        }
        if let Some(found) = find_path_in_list(&info.paths) {
            // Refused or unsaved, the path is still used, as above.
            let _ = self.config.lock().set_app_path(&key, &found);
            return Some(found);
        }
        None
    }

    /// Tracked and alive, otherwise any process with the exe name, so an app
    /// started outside the launcher still shows as running.
    fn is_app_running(&mut self, app: &str) -> bool {
        let Some(exe) = self.app_exe(app) else {
            return false;
        };
        // A dead tracked entry is dropped by the check and falls through.
        if self.tracker.is_tracked(app) && self.tracker.is_tracked_running(app).0 {
            return true;
        }
        is_process_running(&exe)
    }

    fn child_names(&mut self, app: &str) -> Vec<String> {
        if self.app_def(app).is_none() {
            return Vec::new();
        }
        self.tracker.child_names(app)
    }

    fn games_under(&mut self, app: &str) -> Vec<String> {
        if self.app_def(app).is_none() {
            return Vec::new();
        }
        let running: Vec<String> = self
            .tracker
            .spared_names(app)
            .iter()
            .map(|exe| exe.to_lowercase())
            .collect();
        self.games
            .iter()
            .filter(|g| g.exes().any(|exe| running.contains(&exe.to_lowercase())))
            .map(|g| g.name.to_owned())
            .collect()
    }

    fn launch_app(&mut self, app: &str, path: &str) -> bool {
        let Some(needs_shell_launch) = self.app_def(app).map(|a| a.needs_shell_launch) else {
            return false;
        };
        let method = if needs_shell_launch {
            LaunchMethod::Shell
        } else {
            LaunchMethod::Direct
        };
        let wait = self.app_launch_wait();
        self.tracker.launch_and_track(app, path, wait, method)
    }

    /// Tracked close when tracked; otherwise (or when the tracked process was
    /// already gone) kill by exe name so Close works on apps started by hand.
    fn close_app(&mut self, app: &str) -> bool {
        let Some(exe) = self.app_exe(app) else {
            return false;
        };
        if self.tracker.is_tracked(app) && self.tracker.close_tracked(app) {
            return true;
        }
        kill_process(&exe)
    }

    fn game_list(&self) -> Vec<String> {
        self.games.iter().map(|g| g.name.to_owned()).collect()
    }

    fn game_exe(&self, game: &str) -> Option<String> {
        self.game_def(game).map(|g| g.exe.to_owned())
    }

    /// Saved path, then the Steam registry. `shortcut_names` and `paths` are
    /// never consulted, as in Python. A saved `steam://` URL never exists on
    /// disk, so a Steam game is re-detected and re-saved on every lookup.
    fn find_game_path(&mut self, game: &str) -> Option<String> {
        let appid = self.game_def(game)?.steam_appid;
        let key = format!("game_{}", config_key(game));

        if let Some(saved) = self.config.lock().app_path(&key) {
            return Some(saved);
        }
        if !appid.is_empty() && self.steam_game_installed(appid) {
            let url = format!("steam://rungameid/{appid}");
            // A steam:// URL is always accepted; a failed save is ignored, as in Python.
            let _ = self.config.lock().set_app_path(&key, &url);
            return Some(url);
        }
        None
    }

    fn is_game_running(&mut self, game: &str) -> bool {
        self.game_def(game)
            .is_some_and(|g| is_process_running(g.exe))
    }

    /// `steam://` and `.lnk` go through the shell, an `.exe` is started
    /// detached and untracked. Success is "the game's exe is running afterwards".
    fn launch_game(&mut self, game: &str, path: &str) -> bool {
        let Some(exe) = self.game_exe(game) else {
            return false;
        };
        if path.starts_with("steam://") {
            // os.startfile raising is caught by the method's `except Exception`.
            if self.shell_open(path).is_err() {
                return false;
            }
            std::thread::sleep(self.steam_launch_wait());
        } else if path.to_lowercase().ends_with(".lnk") {
            if self.shell_open(path).is_err() {
                return false;
            }
            std::thread::sleep(self.game_launch_wait());
        } else if !launch_process(path, self.game_launch_wait()) {
            return false;
        }
        is_process_running(&exe)
    }
}
