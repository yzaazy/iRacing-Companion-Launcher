# Test fixtures

Files captured from the Python app v2.3.2, the app the Rust port replaces. They keep its answers, so the tests can hold the Rust app to them without Python. The Python app as released is at tag `v2.3.2`. Commit `d0a9ffb` is the last one that has the Python app together with the fixture generators and `tools/capture_reference.py`; `git show d0a9ffb:<path>` shows any of them.

Nothing here runs during `cargo test` or `tools/gate.ps1`: the tests read the committed files and never invoke Python.

## process/

`process_state.json` as the Python `ProcessTracker` wrote it. Captured on 2026-11-12 with Python 3.13.14 and psutil 7.1.0, app v2.3.2, at commit `e45ffe5`, by `capture_state_files.py`:

```
rustc --edition 2021 -C debuginfo=0 -o <scratch>\dummy.exe tests/fixtures/process/dummy.rs
python tests/fixtures/process/capture_state_files.py <scratch>\dummy.exe tests/fixtures/process
```

The script loaded the app's own `process_tracker.py` and drove it against copies of the dummy process only, launched by a relative path from a temp directory named `dümmy 日本 😀`, so the files hold no machine path and pin the escaping of non-ASCII and astral characters. It needed the Python app, so it was deleted with it; its last version is in commit `d0a9ffb` (`git show d0a9ffb:tests/fixtures/process/capture_state_files.py`).

| File | What Python wrote |
|---|---|
| `python_written.json` | After two launches into a file holding `{"Empty": {}}`: the falsy entry, kept as is; `Dümmy Gone 😀`, a single dummy; `Dummy App`, the root of a three-process dummy tree. |
| `python_closed.json` | After `close_tracked("Dummy App")`. |
| `python_dropped.json` | After the second dummy exited and a new tracker dropped it on load. |
| `python_view.json` | Not a state file: the dummy name prefix, what the tracker reported for the tree, and the PID and `repr(create_time)` it stored per key. |

`create_times.json` pins the conversion from a process's creation `FILETIME` to psutil's `create_time()` float, which upgrading users have in their state file. Captured on 2026-11-12 with Python 3.13.14 and psutil 7.1.0, at commit `d132f0d`, by `capture_create_times.py`, which needs only Python's standard library and psutil and still runs (Windows, `pip install psutil`):

```
python tests/fixtures/process/capture_create_times.py tests/fixtures/process/create_times.json
```

For the script's own Python process and five child Python processes it started and killed itself, each row holds the raw creation `FILETIME` from `GetProcessTimes` (via `ctypes`) as one 64-bit integer and `repr(psutil.Process(pid).create_time())`. `filetime_conversion_matches_psutil` in `src/process/sys.rs` feeds each `FILETIME` through the production conversion and requires the recorded float bit for bit. A new capture holds other processes' times, so rerunning it replaces the committed rows rather than reproducing them; the gate does not need it.

The PIDs and creation times in the state files belong to processes that no longer exist. `tests/process_layer.rs` swaps them for live dummies' numbers, which Rust formats exactly as Python did (`python_numbers_are_formatted_like_rust`), and checks that the Rust tracker reads each file and writes the same bytes at the same step. `.gitattributes` keeps the CRLF line endings.

## core/

configparser input and output, and the app definitions, written on 2026-11-11 (commit `feadc36`) by `generate.py` with Python 3.13's `configparser` against the app v2.3.2; the commit does not record the patch version (the other captures on that machine used 3.13.14). It needed no library beyond the standard library, but imported the app's `app_definitions.py`, so it was deleted with the app; its last version is in commit `d0a9ffb` (`git show d0a9ffb:tests/fixtures/core/generate.py`). It:

- dumped the app's `APPS` and `RACE_GAMES` into `definitions.json` (`needs_shell_launch` defaulting to `false`, as the app read it), which `tests/core_definitions.rs` compares with the Rust definitions;
- wrote `app_written.ini` and `cp1252.ini` with `configparser` exactly as the app's `save_config` did, in the locale encoding (Windows-1252), and the hand-written inputs `comments.ini`, `interpolation.ini`, `parse_error.ini`, `lf_only.ini` and `empty.ini` byte for byte (`missing.ini` is absent on purpose);
- for each of those eight, loaded it as the app did (locale encoding, a parse error caught and ignored, the `AppPaths` and `Settings` sections added when missing) and wrote `<name>.expected.json`, the defaults, the sections and what `get` and `getboolean` return for every key and for `missing_key` (or `{"error": true}`), and `<name>.saved.ini`, what `save_config` writes after that load, in UTF-8 because the Rust app writes UTF-8.

`.gitattributes` turns off line-ending conversion for every file here.

## app/

`transcripts.json` holds 29 controller scenarios recorded on 2026-11-11 (commit `051a556`) by `capture_transcripts.py` from the app's real controller (`iRacingLauncherGUI`) with Python 3.13, CustomTkinter 5.2.2 and psutil 7.1.0 (the versions on the capture machine; the commit does not record them). The script replaced the app and game managers, the process tracker, the config manager, the logger and the file dialog with fakes that answer from each scenario's script and record every call, put tripwires on every way to start or kill a process, drove the window through each scenario's steps, and wrote per scenario the transcript (calls, status changes, log lines, dialogs) and the end state. It needed the Python app, so it was deleted with it; its last version is in commit `d0a9ffb` (`git show d0a9ffb:tests/fixtures/app/capture_transcripts.py`).

The transcripts name the backend calls by the Python method names (`get_child_names`, ...); `tests/app_support/mod.rs` maps them to the Rust names. `files/` holds the files the browse scenarios pick in the file dialog; the paths in `transcripts.json` point at them.

`tests/app_transcripts.rs` replays every scenario against the Rust controller and requires the same transcript and end state. One deliberate change is applied by the test to every recorded transcript rather than to the file: a picked path containing `%` is saved (change 11 in `docs/behaviour.md`, [#13](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/13)), where v2.3.2's handler raised.

One scenario was edited by hand after the capture, for two deliberate changes from v2.3.2 (`docs/behaviour.md`, "Changes from v2.3.2"). `close_not_found_running`, recorded as `close_not_found_running_double_close`:

- ends with the labels at `(6)` where Python left them stale at `(7)`, because the end of a sequence now recomputes them (change 9, [#11](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/11));
- asks `is_app_running` instead of a first `close_app`, and logs `SimHub closed` instead of `SimHub was not running` (change 10, [#7](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/7)).

Three more scenarios were edited by hand for a later change from v2.3.2 ([docs/behaviour.md](../../docs/behaviour.md#changes-from-v232)):

- Change 12 ([#10](https://github.com/yzaazy/iRacing-Companion-Launcher/issues/10)), a saved game that is not found starts as None: in `startup_saved_settings`, `launch_game_not_configured` and `browse_game` the end state has `selected_game` null and the Launch label `Launch apps (N)`, and the last two no longer look the game up at the end of Launch or log `Skipping <game> - not configured`. `selected_game_gone_at_launch_is_skipped` keeps that line, as v2.3.2 recorded it, under test.

## ui/

The window as the Python app v2.3.2 drew it, at 1200x700 and 96 DPI. The layout tests (`tests/ui/layout.rs`) and the screenshot parity tests (`tests/ui/parity.rs`) compare the Rust window against these files.

| File | What it is |
|---|---|
| `fixtures.json` | The five fixture states (`idle`, `mixed`, `none-selected`, `all-missing`, `log-full`): what every card, checkbox, radio button and log line shows. Written by hand; the capture script and the Rust tests both drive the window into these states. |
| `ui-spec.json` | Everything measured from the running Python window per state: widget rectangles, painted boxes, text placement and ink, fonts, colours, shape geometry, and notes on what the numbers alone do not explain. `_readme` inside describes the structure, `environment` the machine (Windows 11, Python 3.13.14, CustomTkinter 5.2.2, Tk 8.6.15). |
| `reference/<state>.png` | The window's client area in each state, captured with `PrintWindow`. |

`ui-spec.json` and the screenshots were measured and captured on 2026-11-11 from the running Python app v2.3.2 by `tools/capture_reference.py`, which built the window from the app's own UI modules, drove it into each state of `fixtures.json` and closed it again. That script needed the Python app, so it was deleted with it; its last version is in commit `d0a9ffb` (`git show d0a9ffb:tools/capture_reference.py`).

## Frozen answers in test code

`mixed_tree_matches_python_answers` in `tests/detect_shortcuts.rs` holds the Python `find_shortcut_target` answers for a fake Start Menu tree, from the last run of the former `tests/detect_python_crosscheck.rs` (`cargo test --test detect_python_crosscheck -- --ignored --nocapture`, 2026-11-12, Python 3.13.14, commit `e45ffe5`).
