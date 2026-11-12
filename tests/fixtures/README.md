# Test fixtures

Files captured from the Python app (v2.3.2), the reference the Rust port copies. The Python app is being removed; these files keep its answers.

## process/

`process_state.json` as the Python `ProcessTracker` (`iracing_launcher_app/managers/process_tracker.py`) wrote it. Captured on 2026-11-12 with Python 3.13.14 and psutil 7.1.0, app v2.3.2, at commit `e45ffe5`, by:

```
rustc --edition 2021 -C debuginfo=0 -o <scratch>\dummy.exe tests/fixtures/process/dummy.rs
python tests/fixtures/process/capture_state_files.py <scratch>\dummy.exe tests/fixtures/process
```

The script drives the real Python tracker against copies of the dummy process only, launched by a relative path from a temp directory named `dümmy 日本 😀`, so the files hold no machine path and pin the escaping of non-ASCII and astral characters.

| File | What Python wrote |
|---|---|
| `python_written.json` | After two launches into a file holding `{"Empty": {}}`: the falsy entry, kept as is; `Dümmy Gone 😀`, a single dummy; `Dummy App`, the root of a three-process dummy tree. |
| `python_closed.json` | After `close_tracked("Dummy App")`. |
| `python_dropped.json` | After the second dummy exited and a new tracker dropped it on load. |
| `python_view.json` | Not a state file: the dummy name prefix, what the tracker reported for the tree, and the PID and `repr(create_time)` it stored per key. |

`create_times.json` pins the conversion from a process's creation `FILETIME` to psutil's `create_time()` float, which upgrading users have in their state file. Captured on 2026-11-12 with Python 3.13.14 and psutil 7.1.0, at commit `d132f0d`, by:

```
python tests/fixtures/process/capture_create_times.py tests/fixtures/process/create_times.json
```

For the script's own Python process and five child Python processes it started and killed itself, each row holds the raw creation `FILETIME` from `GetProcessTimes` (via `ctypes`) as one 64-bit integer and `repr(psutil.Process(pid).create_time())`. `filetime_conversion_matches_psutil` in `src/process/sys.rs` feeds each `FILETIME` through the production conversion and requires the recorded float bit for bit.

The PIDs and creation times in the state files belong to processes that no longer exist. `tests/process_layer.rs` swaps them for live dummies' numbers, which Rust formats exactly as Python did (`python_numbers_are_formatted_like_rust`), and checks that the Rust tracker reads each file and writes the same bytes at the same step. `.gitattributes` keeps the CRLF line endings.

## core/

configparser input and output, written by `core/generate.py`.

## app/

Controller transcripts, written by `app/capture_transcripts.py`.

## Frozen answers in test code

`mixed_tree_matches_python_answers` in `tests/detect_shortcuts.rs` holds the Python `find_shortcut_target` answers for a fake Start Menu tree, from the last run of the former `tests/detect_python_crosscheck.rs` (`cargo test --test detect_python_crosscheck -- --ignored --nocapture`, 2026-11-12, Python 3.13.14, commit `e45ffe5`).
