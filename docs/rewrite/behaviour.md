# Parity traps

The Python source is the spec for the Rust port. This file holds only what reading one Python file at a time will miss: cross-file effects, library behaviour hidden behind a call, and the short list of deliberate differences. Port everything else straight from the source.

## Deliberate differences

These are the only places Rust may differ from Python. Anything else that differs is a bug in the port.

1. **Worker thread.** Launch and close sequences run off the UI thread. Commands queue: a button pressed mid-sequence runs after the current sequence finishes. A sequence uses the app selection and selected game as they were when its button was pressed.
2. **Version source.** `Cargo.toml` replaces `version.py`.
3. **Text rasterisation.** Same fonts, drawn by egui.
4. **Config location in debug builds.** See `contracts.md`.
5. **`print()` diagnostics are dropped.** They are invisible in the windowed Python build. They never become log lines.
6. **`config.ini` is written as UTF-8.** See Config below.
7. **An unset `APPDATA` or `ProgramData` is tolerated.** Python raises there; the case cannot occur on a real Windows session.
8. **A malformed config value falls back instead of crashing startup.** A non-boolean `*_enabled` value or a broken `%` reference makes Python raise while the window is being built. Rust uses the fallback (`true`, or no saved path). A leading UTF-8 byte-order mark is skipped. A `process_state.json` entry without an integer `pid` is dropped. A detected path containing `%`, which Python cannot save and crashes on, is used unsaved.
9. **The window opens with the startup log complete.** Python's logger forces a repaint per line, so its window appears during startup and the lines arrive one by one. Rust finishes startup before the window exists.
10. **Keyboard access, by the owner's decision.** The Python app has none: Tab reaches nothing. The Rust app allows it under these rules: nothing is focused at startup and a mouse click leaves no focus, so a stray key can never launch or close apps; Tab and Shift+Tab move a visible focus indicator through the enabled controls in layout order; Space and Enter activate only the focused control, once per key press. An unfocused window is pixel-identical to the Python reference.
    - Indicator: a ring drawn on top, outside the control's shape: 1 px gap, 1 px black, 2 px white, following the control's corner radius (a circle for radios).
    - Tab order: the None radio; each game card's radio, or its Browse button when the game is not found; Select All; each app card's checkbox, or its Browse button when not found; the log, only while it has a scrollbar; Launch; Close. Disabled buttons are skipped. Tab wraps at both ends.
    - Escape or any mouse press drops the focus.
    - The focused log scrolls with the arrow, page, Home and End keys. Python's log took those keys after a click on it.

## Quirks to port unchanged

- Close on an app with no detected path that is running: the first `close_app` kills it, the second returns false, so the log says "was not running" and the card returns to not-found ([main_window.py:470-494](../../iracing_launcher_app/ui/main_window.py)).
- "All apps launched!" is logged after the app loop regardless of failures or an empty selection.
- `stopping` has no entry in `STATUS_COLORS`, so the dot shows the idle colour.
- A saved `selected_game` is restored even when that game is not found: the radio is selected and disabled.
- At the end of a launch the Launch button is set to enabled unconditionally, and likewise Close after a close, even when `_update_button_text` would have disabled it.
- `shortcut_names` and `paths` in `RACE_GAMES` are never consulted. Games are found by saved path or Steam registry only.
- A path containing `%` cannot be saved: `configparser` rejects the value, so `set_app_path` fails and Browse stops before the card is set idle or checked. `ConfigStore::set_app_path` returns false in that case.

## Config (`core/config_manager.py`)

- Python writes `config.ini` with the locale encoding (Windows-1252 on a typical install), not UTF-8. Read as UTF-8 and fall back to the ANSI code page when that fails. Write UTF-8.
- `configparser` format to reproduce on write: `[Section]`, then `key = value` lines, then one blank line after each section. Section order: `AppPaths`, `Settings`. Keys are lowercased. An empty value is written as `key = ` with the trailing space.
- Accept on read what `configparser` accepts: `=` or `:` as delimiter, surrounding whitespace, `#` and `;` comment lines, keys in any case, indented continuation lines, `[DEFAULT]`, and `%` interpolation (`%%` is a literal `%`). `src/core/config.rs` is tested byte-for-byte against fixtures produced by the real `configparser`.
- Booleans are stored as `True` / `False`. `getboolean` accepts `1 yes true on` and `0 no false off`, case-insensitive. The fallback for a missing `*_enabled` key is true.
- Keys: `ConfigManager.get_config_key(name)` is lowercase with spaces as underscores. App paths live under that key in `[AppPaths]`; game paths under `game_<key>`. Checkbox state is `<key>_enabled` in `[Settings]`; the game choice is `selected_game` (empty string for None).
- `get_app_path` returns a saved value only when it exists on disk. A saved `steam://rungameid/...` never exists on disk, so every lookup for a Steam game falls through to the registry check and re-saves the URL.
- `set_app_path` saves the file immediately. `find_app_path` and `find_game_path` therefore write config as a side effect of a successful detection.

## Process tracking (`managers/process_tracker.py`, `process_manager.py`)

- `process_state.json` is `{ "<App Name>": { "pid": int, "create_time": float, "exe_path": str } }`, UTF-8, written atomically (tmp file then replace). `create_time` is epoch seconds as a float, derived from the process creation `FILETIME`. Keep sub-second precision: entries written by the Python version must still validate, with the same 0.5 s tolerance.
- A tracked entry is valid only if the PID is alive and its creation time matches. Every failed validation drops the entry and saves. All entries are revalidated at load.
- `psutil` `children(recursive=True)`: build the tree from a snapshot of (pid, parent pid). A descendant counts only if it was created at or after the tracked **root**, which guards against a recycled PID. Creation time falls back to `NtQuerySystemInformation` for processes that refuse to be opened.
- `terminate()` and `kill()` are both `TerminateProcess` on Windows. `close_tracked`: terminate root and descendants, wait up to 3 s for all, kill survivors, drop the entry. Access-denied at any step is swallowed, and when waiting on any one process is refused the kill pass is skipped for the whole tree. It returns true whenever the tracked root was alive at the start.
- Direct launch: argv is the exe path alone, `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`, stdio to null, inherited working directory. State is saved before the 2 s wait; success is "still alive after the wait".
- Shell launch (`needs_shell_launch`, used for Stream Deck's `uiAccess` manifest): `ShellExecute` with no verb (the default verb, as `os.startfile` does), wait 2 s, then take a PID with that exe name that was absent before the launch. Python takes an arbitrary one; Rust takes the lowest. No new PID means failure.
- Name matching (`is_process_running`, `kill_process`) compares exe base names case-insensitively across all processes, and `kill_process` kills every match.
- `is_app_running`: tracked and alive, otherwise fall back to the name match so apps started outside the launcher still show as running.
- `close_app`: tracked close when tracked, otherwise kill by exe name.

## Detection (`utils/`, `managers/app_manager.py`, `game_manager.py`)

- App order: saved path, Start Menu shortcut, common paths. Game order: saved path, Steam registry.
- Start Menu search: user Start Menu first, then `ProgramData`. For each root, for each shortcut name in order, recursive match on file name (case-insensitive on Windows). For apps the shortcut is resolved through the shell and the target must exist.
- When several shortcuts share a name, Python 3.13's `rglob` order decides the winner: the root itself first, then a stack of directories where the directory met last is entered first; junctions are followed, directory symlinks are not. A game-mode hit is returned with the casing of the name searched for, not the casing on disk. `src/detect/shortcuts.rs` reproduces this and is cross-checked against the Python.
- Garage61's common path is built at runtime: `%APPDATA%\garage61-install\garage61-launcher.exe`.
- Steam check: `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App <appid>` exists, 64-bit registry view. A hit returns and saves `steam://rungameid/<appid>`.
- Game launch: `steam://` through the shell then wait 3 s; `.lnk` through the shell then wait 2 s; `.exe` detached then wait 2 s, untracked. Success is "the game's exe name is running afterwards".

## UI behaviour (`ui/`)

- Startup ([main_window.py:140-199](../../iracing_launcher_app/ui/main_window.py)): a not-found app gets `<key>_enabled = False` written to config. A found app's checkbox is restored from config. See deliberate difference 9 for when the window appears.
- Button labels and enabled state: `_update_button_text`. The count is checked cards only. Both buttons are disabled only when the count is zero and no game is selected.
- Select All toggles only found apps; its label is "Deselect All" only when every found app is checked.
- Log lines are `[HH:MM:SS] message` in local time. The divider is 50 `─` characters with no timestamp. The log is read-only, selectable, wraps by character, scrolls to the end on every line, and shows a scrollbar only when needed.
- Game names longer than 18 characters wrap onto a second line at the last space before the limit ([game_card.py:83](../../iracing_launcher_app/ui/widgets/game_card.py)).
- Card subtitle: "Not installed" when not found; "N helper process(es)" only when running with a non-zero count; otherwise "No helpers".
- The status dot is the `●` glyph at 40 px in Python. Draw a circle whose size and position come from the reference capture.
- Browse: native open dialog, initial directory `C:\Program Files`. Apps require the exact exe name, case-insensitive. Games accept any `.lnk`, otherwise the exact exe name. A valid app pick sets the card idle, checks it and saves. The picked path is logged and saved with forward slashes, as Tk returns it.
- Tk 8.6's dialog does not set the option that keeps a picked shortcut unresolved, so picking a `.lnk` returns its target and the `.lnk` branch of `_browse_for_game` almost never fires. The Rust dialog uses the same options and behaves the same.
