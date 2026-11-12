# Behaviour

What the launcher does, in the detail that the code alone does not make obvious: the file formats it shares with older versions, the process rules, the detection order and the UI rules. Version 3.0.0 reproduces the Python app (v2.3.2) one to one, except for the changes listed first. Where a rule exists only for compatibility with v2.3.2, or with files it wrote, this file says so. The Python source is in git at tag `v2.3.2`.

## Changes from v2.3.2

These are deliberate. Anything else that differs from v2.3.2 is a bug.

1. **The window stays responsive during Launch and Close.** Sequences run on a worker thread. Pressing a button while a sequence runs queues its command, which runs after the current one finishes. Each sequence uses the app selection and the selected game as they were when its button was pressed.
2. **The window opens with the startup log complete.** v2.3.2 showed its window during startup and the log lines arrived one by one; 3.0.0 finishes startup before the window exists.
3. **Keyboard access.** v2.3.2 took no keys. The rules:
   - Nothing is focused at startup, and a mouse click leaves no focus, so a stray key can never launch or close apps.
   - Tab and Shift+Tab move a visible focus ring through the enabled controls in layout order and wrap at both ends: the None radio; each game card's radio, or its Browse button when the game is not found; Select All; each app card's checkbox, or its Browse button when not found; the log, only while it has a scrollbar; Launch; Close. Disabled buttons are skipped.
   - Space and Enter activate only the focused control, once per key press.
   - Escape or any mouse press drops the focus.
   - The focused log scrolls with the arrow keys, Page Up/Down, Home and End (v2.3.2's log took those keys after a click on it).
   - The ring is drawn on top, outside the control's shape: 1 px gap, 1 px black, 2 px white, following the control's corner radius (a circle for radios). An unfocused window is pixel-identical to v2.3.2.
4. **`config.ini` is written as UTF-8.** v2.3.2 wrote the locale encoding (Windows-1252 on a typical install). Both are read; see [Config](#config).
5. **Malformed files no longer stop startup.** A non-boolean `*_enabled` value or a broken `%` reference made v2.3.2 raise while building the window; 3.0.0 uses the fallback (`true`, or no saved path). A leading UTF-8 byte-order mark in `config.ini` is skipped. A `process_state.json` entry without an integer `pid` is dropped. A detected path containing `%`, which v2.3.2 could not save and crashed on, is saved (change 11).
6. **Text is drawn by egui** with the same fonts and the same whole-pixel glyph advances as Tk. Glyph edges can differ by a pixel; positions do not.
7. **Internal only**: the version comes from `Cargo.toml`; debug builds keep their files under `target/dev-config/` (see [architecture](architecture.md#config-and-state-location)); v2.3.2's `print()` diagnostics, invisible in its windowed build, are gone; an unset `APPDATA` or `ProgramData`, which made v2.3.2 raise and cannot occur in a real Windows session, is tolerated; when replacing `process_state.json` fails, the temporary file is removed, which v2.3.2 left behind.
8. **A game is never closed, also when a tracked app started it** (after 3.0.0, [#17](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/17)). Up to 3.0.0, closing a tracked app closed every descendant, so a game started from that app (a Stream Deck button, Crew Chief) was killed with it. Now a process with a game's exe name is no part of an app's tree, and neither is anything below it: it is not opened, not counted or logged as a helper, and not closed. A game is closed by hand only, so a wrong press on Close cannot end a session. The close log then warns `⚠ <game> is still running - please close it manually`, once per game, whether or not that game is selected. For iRacing the simulator (`iRacingSim64DX11.exe`) counts as the game besides the UI.
9. **The end of a sequence applies the button rule** (after 3.0.1, [#11](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/11)). v2.3.2 enabled the Launch button at the end of every launch, and Close at the end of every close, without recomputing the labels. With nothing selected by then, one button was enabled and pressing it ran an empty sequence; since 3.0.0 keeps the checkboxes live during a sequence, unchecking everything during a launch reached it directly. Now the labels are recomputed and both buttons follow the rule in [Buttons](#ui), so the counts are never stale either.
10. **Close on a running app with no detected path reports the close** (after 3.0.1, [#7](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/7)). v2.3.2 used a first close attempt to ask whether there was anything to close, which already killed the app, so the second attempt found nothing and the log said `<app> was not running`. Now the close sequence asks whether the app is running and closes it once: the log says `Closing <app>...` then `<app> closed`. The card still returns to "Not installed", unchecked, since the launcher has no path to start it again. An app with no path that is not running is skipped without a log line, as before.
11. **A path containing `%` is saved** (after 3.0.1, [#13](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/13)). v2.3.2 wrote a path to `config.ini` as it was, so `configparser` refused one with a `%`: a Browse pick in such a folder did nothing, with no log line, and a detected one crashed startup. Now every `%` in a path is written as `%%`, `configparser`'s own escape, which every version reads back as `%`. Such a pick sets the card idle (an app's is checked too) and logs `<name> path configured: <path>` like any other. A path that itself contains `%%` or `%(name)s` is now kept as picked, where v2.3.2 stored it as an escape or a reference.

## Known quirks carried over from v2.3.2

Kept on purpose so 3.0.0 behaves exactly like v2.3.2. Each has a GitHub issue for the decision to fix it; until that issue is worked on, the quirk stays.

- **[#8](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/8)** "All apps launched!" is logged after the app loop regardless of failures or an empty selection.
- **[#9](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/9)** The `stopping` status has no colour of its own, so the dot shows the idle colour.
- **[#10](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/10)** A saved `selected_game` is restored even when that game is not found: the radio is selected and disabled.
- **[#12](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/12)** The Start Menu shortcut names and install paths listed for games are never consulted. Games are found by saved path or Steam registry only.

Other open issues: [#14](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/14) (the installer forgets a custom install folder and the desktop-icon choice on upgrade) and [#5](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/5) (a panic on the worker thread aborts the app instead of being caught).

## Known differences from v2.3.2 in the activity log, undecided

Found by the log gesture tests (`tests/ui/log_gestures.rs`). All are in the activity log and none affects launching or closing. Until the owner decides, they are neither changes nor bugs to fix in passing.

- A drag after a double-click extends the selection by characters. Tk extends by whole words, and by whole lines after a triple-click; Tk also selects the word or line on the press, 3.0.0 on the release. Held by an ignored test.
- A double-click selects a run of letters, digits and underscores. Tk on Windows selects a run of non-space characters, so `[19:45:06]` as a whole. Held by an ignored test.
- A selection dragged past the end stops before the last line's newline. Tk's most likely includes it (read from the Tk source, not observed).
- A drag held outside the text scrolls one line per frame. Tk scrolls (1 + distance past the edge) pixels every 50 ms.
- A press on the scrollbar thumb centres the thumb on the pointer at once. CustomTkinter reacts to a press only on the trough.
- In the focused log a page is the view minus one line, and Home and End go to the top and bottom. In Tk a page is the view minus two lines, Home and End go to the ends of the line, and the arrows scroll only when an invisible cursor leaves the view.

## Config

`config.ini` lives in the config directory ([architecture](architecture.md#config-and-state-location)) and is shared with v2.3.2, so its format follows Python's `configparser`. `src/core/config.rs` is tested byte for byte against files the real `configparser` produced (`tests/fixtures/core/`).

- **Encoding.** Read as UTF-8 (a leading byte-order mark is skipped); when that fails, read with the ANSI code page, which is what v2.3.2 wrote. Written as UTF-8.
- **Written format.** `[Section]`, then `key = value` lines, then one blank line after each section. Section order `AppPaths`, `Settings`. Keys are lowercased. An empty value is written as `key = ` with the trailing space. Writes are atomic: a `.tmp` file, then replace; the `.tmp` is removed when the replace fails.
- **Accepted on read**, as `configparser` accepts it: `=` or `:` as delimiter, surrounding whitespace, `#` and `;` comment lines, keys in any case, indented continuation lines, `[DEFAULT]`, and `%` interpolation (`%%` is a literal `%`, `%(name)s` a reference). A saved path has every `%` written as `%%` (change 11). A failed write of `config.ini` is ignored, as v2.3.2 ignored it.
- **Booleans** are stored as `True` / `False`. On read `1 yes true on` and `0 no false off` are accepted, case-insensitive. A missing `*_enabled` key means true.
- **Keys.** An app's config key is its name lowercased with spaces as underscores (`config_key`). App paths are stored under that key in `[AppPaths]`, game paths under `game_<key>`. Checkbox state is `<key>_enabled` in `[Settings]`; the game choice is `selected_game` (empty string for None; a missing key also means None).
- A saved path is used only when it exists on disk. A saved `steam://rungameid/...` never exists on disk, so every lookup for a Steam game falls through to the registry check and saves the URL again.
- Saving a path writes the file immediately, so detecting an app or game writes `config.ini` as a side effect.

Example, as both versions write it:

```ini
[AppPaths]
fanatec = C:\Program Files\Fanatec\FanatecUI\UI\Fanatec.exe
game_iracing = steam://rungameid/266410

[Settings]
fanatec_enabled = True
selected_game = iRacing

```

## Process tracking

- **State file.** `process_state.json` is `{ "<App Name>": { "pid": int, "create_time": float, "exe_path": str } }`, UTF-8, written atomically (`.tmp` then replace), with the bytes Python's `json.dump(state, f, indent=2)` produced: same key order, same float formatting, same escaping of non-ASCII characters. `create_time` is epoch seconds as a float, converted from the process creation `FILETIME` exactly as psutil does, so entries written by v2.3.2 still validate.
- **Validity.** A tracked entry is valid only while its PID is alive and its creation time matches within 0.5 s. A failed validation drops the entry and saves the file. All entries are revalidated at startup.
- **Process tree.** Descendants are found from one snapshot of (pid, parent pid). A descendant counts only if it was created at or after the tracked root, which guards against a recycled PID. A process that refuses to be opened still has its creation time read, through `NtQuerySystemInformation`. Descendants that re-parent away from the tree (for example to a system process) are not seen, and so never closed. The walk stops at a process whose exe name is a game's (`exe` and `other_exes` in the game definitions, case-insensitive): it and everything below it is skipped before it is opened.
- **Close of a tracked app.** Terminate the root and its descendants, wait up to 3 s for all of them, kill the survivors, drop the entry. Access denied at any step is swallowed; when waiting on any one process is refused, the kill pass is skipped for the whole tree. The close counts as done whenever the root was alive at the start.
- **Direct launch** (every app except Stream Deck): the exe path alone as the command line, `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`, standard handles on `NUL`, inherited environment and working directory. The entry is saved before the 2 s wait; success is "still alive after the wait".
- **Shell launch** (Elgato Stream Deck, whose manifest asks for `uiAccess`, which Windows honours only through the shell; `CreateProcess` fails with error 740): `ShellExecute` with the default verb, wait 2 s, then track a PID with that exe name that was absent before the launch, the lowest if there are several. No new PID means failure.
- **Name matching** compares exe base names case-insensitively across all processes. Killing by name kills every match.
- **Is an app running?** Tracked and alive; otherwise any process with its exe name, so apps started outside the launcher still show as running.
- **Close of an app.** The tracked close when tracked; otherwise (or when the tracked root was already gone) kill by exe name.

## Detection

- **Order.** Apps: saved path, Start Menu shortcut, common install paths. Games: saved path, Steam registry. A path found by detection is saved to `config.ini` before it is used.
- **Start Menu search.** The user's Start Menu (`%APPDATA%\Microsoft\Windows\Start Menu\Programs`) first, then the all-users one under `%ProgramData%`. For each root, for each shortcut name in order, a recursive, case-insensitive match on the file name. For apps the shortcut is resolved through the shell and its target must exist; the target is read into a 260-character buffer, so a longer one fails the existence check.
- **Which shortcut wins** when several share a name follows Python 3.13's `rglob` order, which v2.3.2 used: the root itself first, then a stack of directories where the directory met last is entered first; junctions are followed, directory symlinks are not. A game-mode hit is returned with the casing of the name searched for, not the casing on disk. `src/detect/shortcuts.rs` reproduces this, and `tests/detect_shortcuts.rs` holds v2.3.2's answers for a fake tree.
- **Garage61's** common path is built at runtime: `%APPDATA%\garage61-install\garage61-launcher.exe`.
- **Steam check.** `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App <appid>` can be opened, in the 64-bit registry view. A hit returns and saves `steam://rungameid/<appid>`.
- **Game launch.** `steam://` through the shell, then wait 3 s; `.lnk` through the shell, then wait 2 s; `.exe` started detached and untracked, then wait 2 s. Success is "the game's exe is running afterwards". Games are never closed by the launcher. Close reports whether the selected game is still running, when its card showed it running, and warns about every game that a closed app had started (change 8 above).

## UI

- **Startup.** A not-found app gets `<key>_enabled = False` written to config. A found app's checkbox is restored from config. The saved game choice is restored (see #10 above).
- **Buttons.** The labels count checked cards only. Both buttons are disabled only when no card is checked and no game is selected. While a command of a button's kind is queued or running, that button stays disabled. The labels and the rule are applied again at the end of each sequence.
- **Select All** toggles only found apps; its label is "Deselect All" only when every found app is checked.
- **Log.** Lines are `[HH:MM:SS] message` in local time. The divider is 50 `─` characters with no timestamp. The log is read-only, selectable, wraps by character, scrolls to the end on every new line, scrolls 40 px per wheel notch, and shows a scrollbar only when needed. It is never trimmed.
- **Game names** longer than 18 characters wrap onto a second line at the last space before the limit.
- **Card subtitle.** "Not installed" when not found; "N helper process(es)" only when running with a non-zero count; otherwise "No helpers".
- **Status dot.** v2.3.2 drew the `●` glyph at 40 px; 3.0.0 paints a circle whose size and position were taken from v2.3.2's window.
- **Pointer.** A control acts on the press of the left button, not on release, as CustomTkinter binds it. Hit areas are exact. Controls of not-found entries and disabled buttons do nothing.
- **Browse** opens the Windows open dialog at `C:\Program Files`. Apps require the exact exe name, case-insensitive. Games accept any `.lnk`, otherwise the exact exe name. A valid app pick sets the card idle, checks it and saves. The picked path is logged and saved with forward slashes, as Tk returned it.
- **Picked shortcuts.** Tk 8.6's dialog does not set the option that keeps a picked shortcut unresolved, so picking a `.lnk` returns its target and the `.lnk` branch for games almost never fires. The dialog in 3.0.0 uses the same options and behaves the same.
- **Window.** 1200x700 logical pixels, not resizable, no maximize button. egui's Ctrl+Plus/Minus/0 zoom is turned off.
