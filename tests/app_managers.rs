//! The real managers: detection order and its config saves, the tracked-then-
//! by-name fallbacks and the game launch routing. The controller transcripts
//! drive a fake backend instead, so this file is the only place the managers'
//! own logic runs.
//!
//! Every definition here is a dummy: exe names come from `dummy_support`
//! (unique names no real program uses), shortcuts live in a fake Start Menu,
//! the Steam check reads a volatile key under `HKCU` that the test creates, and
//! game paths that would go through the shell go to a recording stand-in. The
//! real definitions appear only where nothing is detected, launched or closed.

#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

mod detect_support;
mod dummy_support;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use detect_support::{Tree, s};
use dummy_support::{Dummies, wait_until};
use iracing_companion_launcher::core::config::{ConfigStore, SharedConfig};
use iracing_companion_launcher::core::definitions::{AppDef, GameDef};
use iracing_companion_launcher::managers::{Backend, Managers, ShellOpen, TestHost};
use iracing_companion_launcher::process::is_process_running;
use iracing_companion_launcher::process::tracker::ProcessTracker;
use serde_json::{Value, json};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_VOLATILE, RegCloseKey, RegCreateKeyExW,
    RegDeleteTreeW,
};
use windows::core::HSTRING;

const APP: &str = "Dummy App";
const GAME: &str = "Dummy Game";
const APPID: &str = "990000417";
const STEAM_URL: &str = "steam://rungameid/990000417";
const SHORTCUT: &str = "ircl-test Dummy App.lnk";
/// Long enough for a direct or shell launch of a dummy to be seen alive.
const APP_WAIT: Duration = Duration::from_millis(300);
/// A wait the test can tell apart from no wait.
const MARK_WAIT: Duration = Duration::from_millis(400);

// ---------------------------------------------------------------------------
// Support
// ---------------------------------------------------------------------------

/// `HKCU\Software\ircl-managers-test-<pid>-<tag>`, volatile and deleted on drop.
struct TempKey(String);

impl TempKey {
    fn new(tag: &str) -> Self {
        Self(format!(
            r"Software\ircl-managers-test-{}-{tag}",
            std::process::id()
        ))
    }

    /// Creates `Steam App <appid>` under this key.
    fn install(&self, appid: &str) {
        let path = HSTRING::from(format!(r"{}\Steam App {appid}", self.0));
        let mut key = HKEY::default();
        // SAFETY: `path` is a NUL-terminated HSTRING alive across the call, the
        // optional pointers are null, and `key` is a live HKEY for the result,
        // closed once below.
        let made = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &path,
                None,
                None,
                REG_OPTION_VOLATILE,
                KEY_WRITE,
                None,
                &raw mut key,
                None,
            )
        };
        assert_eq!(made, ERROR_SUCCESS);
        // SAFETY: `key` was opened by the successful call above and is closed once.
        let _ = unsafe { RegCloseKey(key) };
    }

    /// Deletes `Steam App <appid>` under this key.
    fn uninstall(&self, appid: &str) {
        let path = HSTRING::from(format!(r"{}\Steam App {appid}", self.0));
        // SAFETY: the subkey name is a NUL-terminated HSTRING alive across the
        // call; it names only this test's own volatile key.
        let deleted = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &path) };
        assert_eq!(deleted, ERROR_SUCCESS);
    }
}

impl Drop for TempKey {
    fn drop(&mut self) {
        // SAFETY: the subkey name is a NUL-terminated HSTRING alive across the
        // call; it names only this test's own volatile key.
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&self.0)) };
    }
}

/// The shell stand-in: records each path it is asked to open, then starts
/// `starts` (a dummy exe) or fails.
#[derive(Clone, Default)]
struct Shell {
    opened: Arc<Mutex<Vec<String>>>,
    /// Dummies the stand-in started; `Dummies::drop` kills them by name.
    started: Arc<Mutex<Vec<Child>>>,
}

impl Shell {
    fn opened(&self) -> Vec<String> {
        self.opened.lock().unwrap().clone()
    }

    fn opener(&self, starts: Option<String>, fails: bool) -> ShellOpen {
        let opened = Arc::clone(&self.opened);
        let started = Arc::clone(&self.started);
        Box::new(move |path: &str| {
            opened.lock().unwrap().push(path.to_owned());
            if fails {
                return Err(std::io::Error::from_raw_os_error(2));
            }
            if let Some(exe) = &starts {
                started
                    .lock()
                    .unwrap()
                    .push(Command::new(exe).spawn().unwrap());
            }
            Ok(())
        })
    }
}

/// A test's temp directory (config and state), fake Start Menu, Steam key and dummies.
struct Fixture {
    dir: tempfile::TempDir,
    tree: Tree,
    steam: TempKey,
    dummies: Dummies,
    shell: Shell,
    /// The tracker's launch wait.
    tracker_sleep: fn(Duration),
}

impl Fixture {
    fn new(tag: &str) -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            tree: Tree::new(),
            steam: TempKey::new(tag),
            dummies: Dummies::new(),
            shell: Shell::default(),
            tracker_sleep: std::thread::sleep,
        }
    }

    fn config_file(&self) -> PathBuf {
        self.dir.path().join("config.ini")
    }

    fn state_file(&self) -> PathBuf {
        self.dir.path().join("process_state.json")
    }

    fn write_config(&self, text: &str) {
        std::fs::write(self.config_file(), text).unwrap();
    }

    fn config(&self) -> Option<String> {
        std::fs::read_to_string(self.config_file()).ok()
    }

    fn state(&self) -> Option<Value> {
        let text = std::fs::read_to_string(self.state_file()).ok()?;
        Some(serde_json::from_str(&text).unwrap())
    }

    /// Managers over `apps` and `games`, with the given shell stand-in and waits
    /// `(steam, game)`; the app launch wait is [`APP_WAIT`].
    fn managers(
        &self,
        apps: Vec<AppDef>,
        games: Vec<GameDef>,
        shell_open: ShellOpen,
        (steam_launch_wait, game_launch_wait): (Duration, Duration),
    ) -> Managers {
        let host = TestHost {
            start_menu: self.tree.roots(),
            steam_uninstall_key: self.steam.0.clone(),
            shell_open,
            app_launch_wait: APP_WAIT,
            steam_launch_wait,
            game_launch_wait,
        };
        Managers::with_test_host(
            SharedConfig::new(ConfigStore::load(self.dir.path())),
            ProcessTracker::with_sleep(self.state_file(), self.tracker_sleep),
            apps,
            games,
            host,
        )
    }

    /// Managers for detection and app tests: a shell stand-in that fails, no waits.
    fn app_managers(&self, apps: Vec<AppDef>, games: Vec<GameDef>) -> Managers {
        self.managers(
            apps,
            games,
            self.shell.opener(None, true),
            (Duration::ZERO, Duration::ZERO),
        )
    }

    /// One app whose exe name is the dummy role `exe_role`.
    fn app(&self, exe_role: &str, paths: Vec<String>, needs_shell_launch: bool) -> AppDef {
        AppDef {
            name: APP,
            exe: leak(self.dummies.name(exe_role)),
            shortcut_names: &[SHORTCUT],
            paths,
            needs_shell_launch,
        }
    }

    /// One game whose exe name is the dummy role `exe_role`. Its shortcut names
    /// and paths point at things that exist, which detection must ignore.
    fn game(&self, exe_role: &str, steam_appid: &'static str) -> GameDef {
        let lnk_target = self.tree.target("game.exe");
        self.tree
            .lnk(&self.tree.root1(), "ircl-test Dummy Game.lnk", &lnk_target);
        GameDef {
            name: GAME,
            exe: leak(self.dummies.name(exe_role)),
            shortcut_names: &["ircl-test Dummy Game.lnk"],
            steam_appid,
            steam_folder: "ircl-test Dummy Game",
            paths: Vec::leak(vec![leak(s(&lnk_target))]),
            other_exes: &[],
        }
    }
}

fn leak(s: String) -> &'static str {
    String::leak(s)
}

/// `config.ini` as configparser writes it with one `[AppPaths]` entry.
fn saved(key: &str, path: &str) -> String {
    format!("[AppPaths]\r\n{key} = {path}\r\n\r\n[Settings]\r\n\r\n")
}

fn exited(name: &str) -> bool {
    wait_until(Duration::from_secs(10), || !is_process_running(name))
}

fn started(name: &str) -> bool {
    wait_until(Duration::from_secs(10), || is_process_running(name))
}

// ---------------------------------------------------------------------------
// App detection: saved path, Start Menu shortcut, common paths
// ---------------------------------------------------------------------------

#[test]
fn saved_app_path_wins_and_writes_nothing() {
    let f = Fixture::new("app-saved");
    let saved_exe = f.tree.target(r"saved\app.exe");
    let shortcut_exe = f.tree.target(r"shortcut\app.exe");
    let common = f.tree.target(r"common\app.exe");
    f.tree.lnk(&f.tree.root1(), SHORTCUT, &shortcut_exe);
    let text = format!(
        "[AppPaths]\ndummy_app = {}\n\n[Settings]\n\n",
        s(&saved_exe)
    );
    f.write_config(&text);

    let mut m = f.app_managers(vec![f.app("app", vec![s(&common)], false)], vec![]);
    assert_eq!(m.find_app_path(APP), Some(s(&saved_exe)));
    assert_eq!(f.config().unwrap(), text, "a saved path writes nothing");
}

#[test]
fn shortcut_beats_common_path_and_is_saved() {
    let f = Fixture::new("app-shortcut");
    let shortcut_exe = f.tree.target(r"shortcut\app.exe");
    let common = f.tree.target(r"common\app.exe");
    f.tree.lnk(
        &f.tree.root2(),
        r"Vendor\ircl-test Dummy App.lnk",
        &shortcut_exe,
    );
    // A saved path that no longer exists counts as unsaved.
    let gone = f.dir.path().join("gone.exe");
    f.write_config(&format!("[AppPaths]\ndummy_app = {}\n", s(&gone)));

    let mut m = f.app_managers(vec![f.app("app", vec![s(&common)], false)], vec![]);
    assert_eq!(m.find_app_path(APP), Some(s(&shortcut_exe)));
    assert_eq!(f.config().unwrap(), saved("dummy_app", &s(&shortcut_exe)));
}

#[test]
fn common_path_is_found_in_order_and_saved() {
    let f = Fixture::new("app-common");
    // A shortcut whose target is missing is skipped, as in Python.
    let missing_target = f.dir.path().join("missing").join("app.exe");
    std::fs::create_dir_all(missing_target.parent().unwrap()).unwrap();
    std::fs::write(&missing_target, b"").unwrap();
    f.tree.lnk(&f.tree.root1(), SHORTCUT, &missing_target);
    std::fs::remove_file(&missing_target).unwrap();
    let first = f.tree.target(r"first\app.exe");
    let second = f.tree.target(r"second\app.exe");
    let paths = vec![
        s(&f.dir.path().join("not-there.exe")),
        s(&first),
        s(&second),
    ];

    let mut m = f.app_managers(vec![f.app("app", paths, false)], vec![]);
    assert_eq!(m.find_app_path(APP), Some(s(&first)));
    assert_eq!(f.config().unwrap(), saved("dummy_app", &s(&first)));
}

#[test]
fn app_found_nowhere_writes_nothing() {
    let f = Fixture::new("app-none");
    let paths = vec![s(&f.dir.path().join("not-there.exe"))];
    let mut m = f.app_managers(vec![f.app("app", paths, false)], vec![]);
    assert_eq!(m.find_app_path(APP), None);
    assert_eq!(f.config(), None);
}

/// v2.3.2 crashed saving a detected `%` path; Rust saves it with the `%` escaped
/// (`docs/behaviour.md`, "Changes from v2.3.2", change 9) and finds it there next time.
#[test]
fn detected_path_with_a_percent_is_saved_escaped() {
    let f = Fixture::new("app-percent");
    let common = f.tree.target(r"100%\app.exe");
    let mut m = f.app_managers(vec![f.app("app", vec![s(&common)], false)], vec![]);
    assert_eq!(m.find_app_path(APP), Some(s(&common)));
    let escaped = s(&common).replace('%', "%%");
    assert_eq!(f.config().unwrap(), saved("dummy_app", &escaped));

    let mut again = f.app_managers(vec![f.app("app", vec![], false)], vec![]);
    assert_eq!(
        again.find_app_path(APP),
        Some(s(&common)),
        "from the saved path"
    );
}

// ---------------------------------------------------------------------------
// Game detection: saved path, then the Steam key
// ---------------------------------------------------------------------------

#[test]
fn saved_game_path_wins_before_steam() {
    let f = Fixture::new("game-saved");
    f.steam.install(APPID);
    let lnk = f.tree.target("Dummy Game.lnk");
    let text = format!("[AppPaths]\ngame_dummy_game = {}\n", s(&lnk));
    f.write_config(&text);

    let mut m = f.app_managers(vec![], vec![f.game("game", APPID)]);
    assert_eq!(m.find_game_path(GAME), Some(s(&lnk)));
    assert_eq!(f.config().unwrap(), text, "a saved path writes nothing");
}

/// The saved URL never exists on disk, so every call checks the key and saves again.
#[test]
fn steam_hit_returns_and_saves_the_url_on_every_call() {
    let f = Fixture::new("game-steam");
    f.steam.install(APPID);
    let mut m = f.app_managers(vec![], vec![f.game("game", APPID)]);

    assert_eq!(m.find_game_path(GAME).as_deref(), Some(STEAM_URL));
    assert_eq!(f.config().unwrap(), saved("game_dummy_game", STEAM_URL));

    // Saved again: the file comes back.
    std::fs::remove_file(f.config_file()).unwrap();
    assert_eq!(m.find_game_path(GAME).as_deref(), Some(STEAM_URL));
    assert_eq!(f.config().unwrap(), saved("game_dummy_game", STEAM_URL));

    // Looked up again: without the key the saved URL is not enough.
    f.steam.uninstall(APPID);
    assert_eq!(m.find_game_path(GAME), None);
    assert_eq!(f.config().unwrap(), saved("game_dummy_game", STEAM_URL));
}

/// `shortcut_names` and `paths` of a game are never consulted.
#[test]
fn game_without_a_steam_hit_is_not_found() {
    let f = Fixture::new("game-none");
    f.steam.install("990000418");
    let mut m = f.app_managers(vec![], vec![f.game("game", APPID)]);
    assert_eq!(m.find_game_path(GAME), None);

    let mut m = f.app_managers(vec![], vec![f.game("game", "")]);
    assert_eq!(m.find_game_path(GAME), None);
    assert_eq!(f.config(), None);
}

// ---------------------------------------------------------------------------
// is_app_running: tracked, then by name
// ---------------------------------------------------------------------------

/// The definition's exe never runs, so only the tracked entry can answer true.
#[test]
fn tracked_live_app_is_running() {
    let mut f = Fixture::new("run-tracked");
    let exe = f.dummies.exe("tracked");
    let mut m = f.app_managers(vec![f.app("never-started", vec![], false)], vec![]);

    assert!(m.launch_app(APP, &exe));
    assert!(m.is_app_running(APP));

    f.dummies.stop("tracked");
    assert!(exited(&f.dummies.name("tracked")));
    assert!(!m.is_app_running(APP));
    assert_eq!(f.state(), Some(json!({})), "the dead entry is dropped");
}

#[test]
fn dead_tracked_entry_falls_through_to_the_name() {
    let mut f = Fixture::new("run-fallthrough");
    let tracked = f.dummies.exe("tracked");
    let by_name = f.dummies.exe("by-name");
    let mut m = f.app_managers(vec![f.app("by-name", vec![], false)], vec![]);

    assert!(m.launch_app(APP, &tracked));
    f.dummies.spawn(&by_name);
    assert!(started(&f.dummies.name("by-name")));
    f.dummies.stop("tracked");
    assert!(exited(&f.dummies.name("tracked")));

    assert!(m.is_app_running(APP));
    assert_eq!(f.state(), Some(json!({})), "the dead entry is dropped");

    f.dummies.stop("by-name");
    assert!(exited(&f.dummies.name("by-name")));
    assert!(!m.is_app_running(APP));
}

#[test]
fn untracked_app_is_found_by_name() {
    let mut f = Fixture::new("run-untracked");
    let exe = f.dummies.exe("by-name");
    let mut m = f.app_managers(vec![f.app("by-name", vec![], false)], vec![]);

    assert!(!m.is_app_running(APP));
    f.dummies.spawn(&exe);
    assert!(started(&f.dummies.name("by-name")));
    assert!(m.is_app_running(APP));
    assert_eq!(f.state(), None, "nothing was tracked");
}

// ---------------------------------------------------------------------------
// launch_app, child_names, close_app
// ---------------------------------------------------------------------------

thread_local! {
    /// The state file [`note_state_then_sleep`] reads, and what it found.
    static WATCHED: RefCell<(PathBuf, Option<bool>)> = RefCell::default();
}

/// A tracker wait that first notes whether [`APP`] is in the state file yet:
/// a direct launch saves before the wait, a shell launch only after it.
fn note_state_then_sleep(wait: Duration) {
    WATCHED.with_borrow_mut(|(path, seen)| {
        let state: Value = std::fs::read_to_string(path)
            .map(|text| serde_json::from_str(&text).unwrap())
            .unwrap_or_default();
        *seen = Some(state.get(APP).is_some());
    });
    std::thread::sleep(wait);
}

/// Makes `f`'s tracker wait with [`note_state_then_sleep`].
fn watch_the_wait(f: &mut Fixture) {
    WATCHED.set((f.state_file(), None));
    f.tracker_sleep = note_state_then_sleep;
}

thread_local! {
    /// The exe name [`note_state_then_await_start`] waits for, per test thread.
    static AWAITED: RefCell<String> = const { RefCell::new(String::new()) };
}

/// [`note_state_then_sleep`], except that the wait ends when a process named
/// [`AWAITED`] is running instead of after a fixed time: a freshly copied
/// dummy can take longer than any fixed wait to start on a loaded machine.
fn note_state_then_await_start(_: Duration) {
    note_state_then_sleep(Duration::ZERO);
    AWAITED.with_borrow(|name| {
        assert!(
            wait_until(Duration::from_secs(60), || is_process_running(name)),
            "{name} never started"
        );
    });
}

/// Whether [`APP`] was in the state file during the last watched wait.
fn tracked_during_the_wait() -> Option<bool> {
    WATCHED.with_borrow(|(_, seen)| *seen)
}

#[test]
fn direct_launch_is_tracked() {
    let mut f = Fixture::new("launch-direct");
    let exe = f.dummies.exe("app");
    watch_the_wait(&mut f);
    let mut m = f.app_managers(vec![f.app("app", vec![], false)], vec![]);

    let begun = Instant::now();
    assert!(m.launch_app(APP, &exe));
    assert!(begun.elapsed() >= APP_WAIT, "waits the app launch wait");
    assert_eq!(
        tracked_during_the_wait(),
        Some(true),
        "a direct launch is saved before the wait"
    );
    let state = f.state().unwrap();
    assert_eq!(state[APP]["exe_path"], exe.as_str());
    assert!(state[APP]["pid"].is_u64());
    assert!(m.child_names(APP).is_empty());
    assert!(m.is_app_running(APP));
}

/// The shell launch goes through the real `ShellExecute`, on a dummy exe.
#[test]
fn shell_launch_is_tracked_by_the_new_pid() {
    let mut f = Fixture::new("launch-shell");
    let exe = f.dummies.exe("shell");
    watch_the_wait(&mut f);
    AWAITED.set(f.dummies.name("shell"));
    f.tracker_sleep = note_state_then_await_start;
    let mut m = f.app_managers(vec![f.app("shell", vec![], true)], vec![]);

    assert!(m.launch_app(APP, &exe));
    assert_eq!(
        tracked_during_the_wait(),
        Some(false),
        "a shell launch is saved after the wait"
    );
    let state = f.state().unwrap();
    assert_eq!(state[APP]["exe_path"], exe.as_str());
    assert!(m.is_app_running(APP));
    assert!(
        f.shell.opened().is_empty(),
        "apps never use the game opener"
    );

    assert!(m.close_app(APP));
    assert!(exited(&f.dummies.name("shell")));
}

#[test]
fn launch_of_a_missing_exe_fails_and_tracks_nothing() {
    let f = Fixture::new("launch-missing");
    let missing = s(&f.dir.path().join("nope").join("missing.exe"));
    let mut m = f.app_managers(vec![f.app("app", vec![], false)], vec![]);
    assert!(!m.launch_app(APP, &missing));
    assert_eq!(f.state(), None);
}

/// The definition names the root, but a kill by name would leave the
/// descendants running: only the tracked close takes the whole tree.
#[test]
fn tracked_tree_is_named_and_closed_as_a_tree() {
    let mut f = Fixture::new("close-tree");
    let root = f.dummies.tree();
    let mut m = f.app_managers(vec![f.app("root", vec![], false)], vec![]);

    assert!(m.launch_app(APP, &root));
    let names = [f.dummies.name("child"), f.dummies.name("grandchild")];
    assert!(
        wait_until(Duration::from_secs(15), || m.child_names(APP) == names),
        "expected child and grandchild"
    );

    assert!(m.close_app(APP));
    for role in ["root", "child", "grandchild"] {
        assert!(exited(&f.dummies.name(role)), "{role} survived");
    }
    assert_eq!(f.state(), Some(json!({})));
    assert!(m.child_names(APP).is_empty());
}

/// A tracked app can start a game (a Stream Deck button does). The game and
/// everything below it is no helper of the app and outlives the app's close.
#[test]
fn game_started_by_a_tracked_app_survives_the_close() {
    let mut f = Fixture::new("close-spares-game");
    // The dummy root starts "child", which starts "grandchild".
    let root = f.dummies.tree();
    let mut m = f.app_managers(
        vec![f.app("root", vec![], false)],
        vec![f.game("child", "")],
    );

    assert!(m.launch_app(APP, &root));
    assert!(started(&f.dummies.name("grandchild")));
    assert!(m.is_game_running(GAME));
    assert!(m.is_app_running(APP));
    assert!(m.child_names(APP).is_empty(), "a game is not a helper");
    assert_eq!(m.games_under(APP), [GAME]);

    assert!(m.close_app(APP));
    assert!(exited(&f.dummies.name("root")));
    assert_eq!(f.state(), Some(json!({})));
    // Long enough for a terminated process to be gone.
    std::thread::sleep(Duration::from_millis(500));
    for role in ["child", "grandchild"] {
        assert!(
            is_process_running(&f.dummies.name(role)),
            "{role} was closed"
        );
    }
    assert!(m.games_under(APP).is_empty(), "the app is not tracked now");
}

/// A game's other processes (the simulator behind iRacing's UI) are spared
/// like its `exe`, wherever they sit in the app's tree.
#[test]
fn game_process_under_another_exe_name_survives_the_close() {
    let mut f = Fixture::new("close-spares-sim");
    let root = f.dummies.tree();
    let game = GameDef {
        other_exes: Vec::leak(vec![leak(f.dummies.name("grandchild"))]),
        ..f.game("never-started", "")
    };
    let mut m = f.app_managers(vec![f.app("root", vec![], false)], vec![game]);

    assert!(m.launch_app(APP, &root));
    assert!(started(&f.dummies.name("grandchild")));
    assert_eq!(m.child_names(APP), [f.dummies.name("child")]);
    assert_eq!(m.games_under(APP), [GAME]);

    assert!(m.close_app(APP));
    for role in ["root", "child"] {
        assert!(exited(&f.dummies.name(role)), "{role} survived");
    }
    // Long enough for a terminated process to be gone.
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        is_process_running(&f.dummies.name("grandchild")),
        "the game was closed"
    );
}

#[test]
fn untracked_app_is_killed_by_name() {
    let mut f = Fixture::new("close-by-name");
    let exe = f.dummies.exe("by-name");
    let mut m = f.app_managers(vec![f.app("by-name", vec![], false)], vec![]);
    f.dummies.spawn(&exe);
    f.dummies.spawn(&exe);
    assert!(started(&f.dummies.name("by-name")));

    assert!(m.close_app(APP));
    for child in &mut f.dummies.children {
        let gone = wait_until(Duration::from_secs(5), || {
            matches!(child.try_wait(), Ok(Some(_)))
        });
        assert!(gone, "an instance survived close_app");
    }
    assert!(!m.close_app(APP), "nothing left to close");
}

#[test]
fn dead_tracked_entry_falls_back_to_a_kill_by_name() {
    let mut f = Fixture::new("close-fallback");
    let tracked = f.dummies.exe("tracked");
    let by_name = f.dummies.exe("by-name");
    let mut m = f.app_managers(vec![f.app("by-name", vec![], false)], vec![]);

    assert!(m.launch_app(APP, &tracked));
    f.dummies.spawn(&by_name);
    assert!(started(&f.dummies.name("by-name")));
    f.dummies.stop("tracked");
    assert!(exited(&f.dummies.name("tracked")));

    assert!(m.close_app(APP));
    assert!(exited(&f.dummies.name("by-name")));
    assert_eq!(f.state(), Some(json!({})));
}

#[test]
fn nothing_running_is_not_running_and_not_closed() {
    let f = Fixture::new("close-nothing");
    let mut m = f.app_managers(vec![f.app("never-started", vec![], false)], vec![]);
    assert!(!m.is_app_running(APP));
    assert!(!m.close_app(APP));
    assert!(m.child_names(APP).is_empty());
    assert_eq!(f.state(), None);
}

// ---------------------------------------------------------------------------
// launch_game routing
// ---------------------------------------------------------------------------

/// Opens `path` with a stand-in that starts the game's dummy (or fails), with
/// the given `(steam, game)` waits. Returns the result and the time it took.
fn launch_game(
    f: &mut Fixture,
    path: &str,
    starts_game: bool,
    fails: bool,
    waits: (Duration, Duration),
) -> (bool, Duration) {
    let game_exe = f.dummies.exe("game");
    let starts = starts_game.then_some(game_exe);
    let opener = f.shell.opener(starts, fails);
    let mut m = f.managers(vec![], vec![f.game("game", APPID)], opener, waits);
    let begun = Instant::now();
    let launched = m.launch_game(GAME, path);
    (launched, begun.elapsed())
}

#[test]
fn steam_url_goes_to_the_shell_then_waits_the_steam_wait() {
    let mut f = Fixture::new("game-steam-launch");
    let (launched, took) = launch_game(&mut f, STEAM_URL, true, false, (MARK_WAIT, Duration::ZERO));
    assert!(launched);
    assert_eq!(f.shell.opened(), [STEAM_URL]);
    assert!(took >= MARK_WAIT, "waited {took:?}");
    assert_eq!(f.state(), None, "a game is never tracked");
}

#[test]
fn lnk_goes_to_the_shell_then_waits_the_game_wait() {
    let mut f = Fixture::new("game-lnk-launch");
    let lnk = s(&f.tree.root1().join("Dummy Game.LNK"));
    let (launched, took) = launch_game(&mut f, &lnk, true, false, (Duration::ZERO, MARK_WAIT));
    assert!(launched);
    assert_eq!(f.shell.opened(), [lnk]);
    assert!(took >= MARK_WAIT, "waited {took:?}");
}

#[test]
fn exe_is_launched_detached_and_untracked() {
    let mut f = Fixture::new("game-exe-launch");
    let exe = f.dummies.exe("game");
    let (launched, took) = launch_game(&mut f, &exe, false, false, (Duration::ZERO, MARK_WAIT));
    assert!(launched);
    assert!(
        f.shell.opened().is_empty(),
        "an exe does not go through the shell"
    );
    assert!(took >= MARK_WAIT, "waited {took:?}");
    assert!(is_process_running(&f.dummies.name("game")));
    assert_eq!(f.state(), None, "a game is never tracked");
}

/// Success is whether the game's exe runs afterwards, whatever the shell said.
#[test]
fn game_that_does_not_start_is_a_failed_launch() {
    let mut f = Fixture::new("game-not-started");
    let (launched, _) = launch_game(
        &mut f,
        STEAM_URL,
        false,
        false,
        (Duration::ZERO, Duration::ZERO),
    );
    assert!(!launched);
    assert_eq!(f.shell.opened(), [STEAM_URL]);

    let lnk = s(&f.tree.root1().join("Dummy Game.lnk"));
    let (launched, _) = launch_game(&mut f, &lnk, false, false, (Duration::ZERO, Duration::ZERO));
    assert!(!launched);

    let missing = s(&f.dir.path().join("missing.exe"));
    let (launched, _) = launch_game(
        &mut f,
        &missing,
        false,
        false,
        (Duration::ZERO, Duration::ZERO),
    );
    assert!(!launched);
}

/// `os.startfile` raising is caught by `launch_game`'s `except Exception`.
#[test]
fn shell_error_is_a_failed_launch() {
    let mut f = Fixture::new("game-shell-error");
    let (launched, _) = launch_game(
        &mut f,
        STEAM_URL,
        false,
        true,
        (Duration::ZERO, Duration::ZERO),
    );
    assert!(!launched);
    assert_eq!(f.shell.opened(), [STEAM_URL]);
}

#[test]
fn game_running_follows_the_exe_name() {
    let mut f = Fixture::new("game-running");
    let exe = f.dummies.exe("game");
    let mut m = f.app_managers(vec![], vec![f.game("game", APPID)]);
    assert!(!m.is_game_running(GAME));
    f.dummies.spawn(&exe);
    assert!(started(&f.dummies.name("game")));
    assert!(m.is_game_running(GAME));
}

// ---------------------------------------------------------------------------
// The production constructor, where nothing is detected, launched or closed
// ---------------------------------------------------------------------------

fn production(dir: &Path) -> Managers {
    Managers::new(
        SharedConfig::new(ConfigStore::load(dir)),
        ProcessTracker::new(dir.join("process_state.json")),
    )
}

/// Unknown names return before any lookup, launch or kill.
#[test]
fn unknown_names_answer_like_python() {
    let dir = tempfile::tempdir().unwrap();
    let mut managers = production(dir.path());
    let name = "Not An App";
    assert_eq!(managers.app_exe(name), None);
    assert_eq!(managers.find_app_path(name), None);
    assert!(!managers.is_app_running(name));
    assert!(managers.child_names(name).is_empty());
    assert!(!managers.launch_app(name, "C:/x.exe"));
    assert!(!managers.close_app(name));
    assert_eq!(managers.game_exe(name), None);
    assert_eq!(managers.find_game_path(name), None);
    assert!(!managers.is_game_running(name));
    assert!(!managers.launch_game(name, "C:/x.exe"));
    assert!(!dir.path().join("config.ini").exists());
}

#[test]
fn lists_follow_the_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let managers = production(dir.path());
    assert_eq!(managers.app_list()[0], "Fanatec");
    assert_eq!(managers.app_list().len(), 7);
    assert_eq!(managers.game_list()[4], "rFactor 2");
    assert_eq!(managers.app_exe("SimHub").as_deref(), Some("SimHubWPF.exe"));
    assert_eq!(
        managers.game_exe("iRacing").as_deref(),
        Some("iRacingUI.exe")
    );
}
