# Contracts

The seams between agents. Each agent builds its own module against these and nobody waits on anybody. After wave 0 the contract is **frozen**: a change is the orchestrator's decision, so stop and report instead of editing a shared type.

## Crate

- One package at the repo root: a library (`src/lib.rs`, which declares every module) and a thin binary (`src/main.rs`, the window shell). Tests under `tests/` use the library. Package and binary name `iracing-companion-launcher`, edition 2024, version `3.0.0`. The installer renames the exe to `iRacing Companion Launcher.exe`.
- Versions are pinned in `Cargo.toml`: eframe, egui and egui_kittest 0.37, `windows` 0.62. Write against those APIs.
- The module named `core` shadows nothing as long as code writes `crate::core::...`, never a bare `core::...`.
- Crates: `eframe`/`egui` for the UI, `windows` for every Win32 call, the Browse dialog opened as `IFileOpenDialog` through `windows` with Tk's exact options (`raw-window-handle` supplies the owner window), `serde` + `serde_json`, `winresource` as a build dependency, `egui_kittest` and `image` as dev dependencies. The INI reader/writer is hand-written because the file format must match Python's `configparser`.
- `#![windows_subsystem = "windows"]` in release builds.

## Layout and ownership

| Path | Owner | Ports |
|---|---|---|
| `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/model.rs` | scaffold (frozen) | — |
| `src/core/definitions.rs`, `config.rs`, `log.rs` | core | `core/*.py` |
| `src/process/mod.rs`, `tracker.rs` | process | `managers/process_manager.py`, `process_tracker.py` |
| `src/detect/shortcuts.rs`, `steam.rs`, `paths.rs` | detect | `utils/*.py` |
| `src/ui/theme.rs`, `layout.rs`, `widgets.rs`, `sections.rs`, `mod.rs`, `tests/ui/` | ui | `ui/sections/`, `ui/widgets/`, `ui/constants.py` |
| `src/managers.rs`, `src/app.rs`, `src/worker.rs`, `src/dialog.rs` | controller | `managers/app_manager.py`, `game_manager.py`, `ui/main_window.py` |
| `build.rs`, `windows installer.iss`, `[package.metadata.winresource]` in `Cargo.toml`, the Rust section of `CLAUDE.md` | packaging (done) | `.spec`, `version.py`, `tools/update_version.py` |
| `tools/capture_reference.py`, `docs/rewrite/fixtures.json`, `ui-spec.json`, `reference/` | capture (done, frozen) | — |
| `tests/snapshots/` (golden renders) | ui | — |

The scaffold created every module file as a compiling stub with the signatures below, so wave 1 agents fill in bodies and add private items. Placeholder fields in `ConfigStore`, `ProcessTracker` and `Layout` are yours to replace if you own the file.

Frozen files change only in two announced cases: the ui agent updates the one `ui::draw` call in `main.rs` when it changes the seam below, and the controller agent replaces the placeholder app in `main.rs` and removes the scaffold-only `#![allow(...)]` in `lib.rs` in wave 2. A missing `windows` feature is added as a single line in `Cargo.toml` and named in your report.

## Shared types (`src/model.rs`, and `Command` and `Event` in `src/worker.rs`)

`src/model.rs` holds what the screen shows and what the user did; `Command` and `Event`, the worker's protocol, live in `src/worker.rs`. All derive `Debug, Clone, PartialEq, Eq` and serde `Serialize, Deserialize` with `snake_case` names, so a fixture file deserialises straight into `ViewState` and transcripts record events as JSON. `Status` and `LogLevel` are also `Copy, Hash` and are passed by value.

```rust
pub enum Status { Idle, Starting, Running, Failed, Stopping, Stopped, NotFound }
pub enum LogLevel { Info, Success, Error, Warning, Launch, Close, Divider }

pub struct LogLine { pub level: LogLevel, pub text: String } // text is final, timestamp included
pub struct AppCard { pub name: String, pub status: Status, pub checked: bool, pub helper_count: usize }
pub struct GameCard { pub name: String, pub status: Status }

pub struct ViewState {
    pub version: String,
    pub apps: Vec<AppCard>,
    pub games: Vec<GameCard>,
    pub selected_game: Option<String>, // None is the "None" radio
    pub log: Vec<LogLine>,
    pub select_all_label: String,
    pub launch_label: String,
    pub close_label: String,
    pub launch_enabled: bool,
    pub close_enabled: bool,
}

pub enum UiAction {
    ToggleApp { name: String, checked: bool },
    ToggleSelectAll,
    SelectGame(Option<String>),
    BrowseApp(String),
    BrowseGame(String),
    Launch,
    Close,
}

// src/worker.rs
pub enum Command {
    Launch { apps: Vec<String>, game: Option<String> },
    Close { apps: Vec<String>, game: Option<String> },
}

pub enum Event {
    Log(LogLine),
    AppStatus { name: String, status: Status, helper_count: usize },
    GameStatus { name: String, status: Status },
    LaunchFinished,
    CloseFinished,
}
```

## The UI seam

```rust
// src/ui/mod.rs
pub fn draw(ui: &mut egui::Ui, state: &ViewState) -> Vec<UiAction>;
pub fn install_fonts(ctx: &egui::Context);
// src/ui/layout.rs
pub fn layout(window: egui::Vec2, n_apps: usize, n_games: usize) -> Layout;
```

- `draw` takes the root `Ui` that eframe 0.37 hands to `App::ui`. The scaffold stub still takes a `Context`; the ui agent changes it to this signature.
- `install_fonts` is called once at startup by `main.rs` and by the snapshot harness (`with_context_setup`), so both render with the same fonts. egui's built-in fonts lack `●` and `─`, and egui_kittest fails a test on a missing glyph.
- `draw` is a pure function of `ViewState`. The UI holds scroll position and hover, nothing else. Labels and enabled flags arrive computed.
- Every position comes from `layout`, every colour and font size from `theme.rs`. `layout` is rule-based (margins, card height, gaps, column shares, minimum sizes) so the window can become resizable later, and a test asserts it reproduces `ui-spec.json` at 1200x700.
- Card lists are drawn from `state.apps` and `state.games`, whatever their length.
- Fonts: Segoe UI regular and bold and Consolas, loaded from `C:\Windows\Fonts` at startup, with egui's defaults as fallback.
- The window is 1200x700 logical pixels, not resizable, title `iRacing Companion Launcher`.

## Module APIs

Names mirror the Python methods so the mapping stays obvious. Signatures may gain parameters that make them testable (a root directory, a state file path); they keep their meaning.

```rust
// core/definitions.rs
pub struct AppDef { pub name: &'static str, pub exe: &'static str, pub shortcut_names: &'static [&'static str], pub paths: Vec<String>, pub needs_shell_launch: bool }
pub struct GameDef { pub name: &'static str, pub exe: &'static str, pub shortcut_names: &'static [&'static str], pub steam_appid: &'static str, pub steam_folder: &'static str, pub paths: &'static [&'static str] }
pub fn apps() -> Vec<AppDef>;   // Python order; Garage61's path resolved from %APPDATA%
pub fn games() -> Vec<GameDef>; // Python order

// core/config.rs
pub fn resolve_config_dir() -> PathBuf;
pub fn config_key(name: &str) -> String;
impl ConfigStore {
    pub fn load(dir: &Path) -> Self;
    pub fn config_dir(&self) -> &Path;
    pub fn app_path(&self, key: &str) -> Option<String>;      // only if it exists on disk
    pub fn set_app_path(&mut self, key: &str, path: &str) -> bool; // saves
    pub fn setting(&self, key: &str) -> Option<String>;
    pub fn bool_setting(&self, key: &str, fallback: bool) -> bool;
    pub fn set_setting(&mut self, key: &str, value: &str);     // does not save
    pub fn save(&self) -> bool;
}

// core/log.rs
pub fn line(level: LogLevel, message: &str) -> LogLine; // prefixes [HH:MM:SS]
pub fn divider() -> LogLine;

// process/mod.rs
pub fn is_process_running(exe_name: &str) -> bool;
pub fn kill_process(exe_name: &str) -> bool;
pub fn launch_process(exe_path: &str, wait: Duration) -> bool; // detached, untracked
pub fn shell_open(target: &str) -> std::io::Result<()>;

// process/tracker.rs
impl ProcessTracker {
    pub fn new(state_file: PathBuf) -> Self;
    pub fn launch_and_track(&mut self, key: &str, exe_path: &str, wait: Duration, needs_shell_launch: bool) -> bool;
    pub fn is_tracked(&self, key: &str) -> bool;
    pub fn is_tracked_running(&mut self, key: &str) -> (bool, usize);
    pub fn get_child_names(&mut self, key: &str) -> Vec<String>;
    pub fn close_tracked(&mut self, key: &str) -> bool;
    pub fn forget(&mut self, key: &str);
}

// detect/
pub fn find_shortcut_target(shortcut_names: &[&str], is_game: bool) -> Option<String>;
pub fn find_path_in_list(paths: &[String]) -> Option<String>;
pub fn check_steam_game_installed(appid: &str) -> bool;
```

### What wave 1 established

- **UI**: call `ui::install_fonts` once at startup and `ui::draw(ui, &state)` once per frame. `draw` paints the whole viewport and requests its own repaints for the button flash, focus changes and drag-scrolling; the controller still requests one when an event changes the state. Actions are emitted on the frame of the press, as CustomTkinter does, or of Space or Enter on the focused control; `ToggleApp` carries the new value; `SelectGame` fires only for a radio that is not already selected; controls of not-found entries and disabled buttons emit nothing. The log jumps to the end when its length or last line changes; log text carries no trailing newline. `ui::sections::geometry(ctx, state, window)` resolves the full layout, because sizes depend on measured text.
- **Fonts**: startup reads only Segoe UI regular and bold and Consolas. A character they lack is resolved on first use through Windows' font-link list for Segoe UI, loading a fallback file only when it holds the character. The five fixture states load no fallback.
- **Keyboard and hit areas**: the UI keeps its own focus (see deliberate difference 10 in `behaviour.md`) and does not use egui's. Hit areas are exact, without egui's 5 px slack. `main.rs` turns off egui's Ctrl+Plus/Minus/0 zoom.
- **Config**: checkbox keys are `config_key(name) + "_enabled"`, game path keys `"game_" + config_key(name)`. Booleans are written as `True` / `False`. `set_setting` does not save. `setting("selected_game")` is `Some("")` for None and `None` when missing; both mean no game. `ConfigStore` has no interior locking.
- **Process**: `launch_and_track`, `close_tracked` and `launch_process` block for their waits, so they run on the worker thread. `is_tracked_running` and `get_child_names` each take a full process snapshot. The exe-name fallbacks in `is_app_running` and `close_app` belong to the manager, as in Python.
- **Detection**: synchronous, safe from any thread, writes nothing. Saving a detected path is the manager's job.
- **Tests that render** build their harness with `tests/ui/support.rs`, which gives every harness a renderer on one wgpu device for the whole process: two headless GPU devices created at once crash intermittently, one device shared by several threads does not.

`managers.rs`, `app.rs` and `worker.rs` belong to one agent and have no cross-agent seam beyond the types above. In eframe 0.37 the app implements `App::ui(&mut self, ui, frame)`, with `logic(ctx, frame)` available for work that is not drawing.

## Config and state location

- `IRCL_CONFIG_DIR`, when set, wins.
- Release build: `%APPDATA%\iRacingCompanionLauncher`, created if missing.
- Debug build: `target/dev-config/` under the repo.
- Tests always pass an explicit temp directory.

## UI fixtures

`docs/rewrite/fixtures.json` is `{ "<state>": ViewState }`. The Python capture script and the Rust snapshot tests both read it, so both render the same content. Timestamps in fixture log text are fixed strings, and `version` is `2.3.2` to match the reference screenshots.

Required states:

| State | Shows |
|---|---|
| `idle` | Every app found, checked and idle; every game found; None selected; startup log |
| `mixed` | Running apps with 0, 1 and 3 helpers, one idle, one failed, one not found; a game selected; one game not found |
| `none-selected` | Every app unchecked, no game: both buttons disabled, label "Select All" |
| `all-missing` | Every app and game not found: Browse buttons everywhere |
| `log-full` | More log lines than fit, every level present, one line long enough to wrap |

The reference image for each state is `docs/rewrite/reference/<state>.png`. The parity tests (`tests/ui/parity.rs`, part of `cargo test`) render each state in memory, write it to `target/ui-snapshots/<state>.png`, compare it with the reference and fail per class and element, writing `target/ui-diff/<state>.png` when a state fails. They also compare it with its golden render in `tests/snapshots/<state>.png`.

Fixtures hold game names unwrapped; the UI applies the 18-character wrap. `mixed` adds a seventh status, `starting`, so every dot colour appears.

### What the measurements showed

`ui-spec.json` is 500 KB: read its `_readme`, `notes`, `layout`, `shapes`, `fonts` and `colors` keys, and query `states` with a short script instead of loading the file whole. The measured window differs from the declared sizes in ways the layout rules must reproduce:

- The sections request 713 px of height in a 700 px window. Tk allocates in packing order, so the button row, packed last, absorbs the 13 px: the action buttons are 47 px tall (46 painted) at y 610–656, and nothing else moves.
- CustomTkinter rounds painted sizes down to even numbers: cards paint 316x54, the log box 516x444, so the visible gap between cards is 11 px.
- Buttons grow with their label: Launch is 205 px wide for "Launch with iRacing (5)", and the row re-centres.
- A long app name squeezes its Browse button (57 px for MarvinsAIRA Refactored).
- Consolas 14 px lays out at 8 px per character and a 17 px line pitch: 62 characters per log line, 61 with the scrollbar. GDI rounds glyph advances to whole pixels, which egui does not do by default.

### Parity criteria

The parity check (`tests/ui/parity_check.rs`, which documents each class in full) reports three classes per state, and a state passes when all three hold:

| Class | Covers | Passes when |
|---|---|---|
| flat | Everything outside the two classes below | Zero differing pixels |
| edges | Rounded-corner zones of cards, buttons, the log box, checkboxes and radios, and the status dots | The shape's bounding box equals the spec, and only anti-aliased edge pixels differ |
| text | Every text element | Left edge and baseline within 1 px of the reference ink box, right edge within 2 px; glyph anti-aliasing is free to differ |

## Testing rules

- Process tests spawn their own dummy child (for example `ping -n 60 127.0.0.1` or `cmd /c timeout`) under a unique copied exe name where name matching is under test, and clean it up.
- Detection tests build a fake Start Menu tree in a temp directory.
- Config tests round-trip a fixture written in Python's exact format, including a Windows-1252 file with a non-ASCII path.
