# 0005. `config.ini` and `process_state.json` stay compatible with v2.3.2

**Status:** Accepted

## Context

Installed users have `config.ini` (detected and browsed paths, checkbox state, the selected game) and `process_state.json` (the apps the launcher started, so Close still finds their process trees after a restart) in `%APPDATA%\iRacingCompanionLauncher`. v2.3.2 wrote them with Python's `configparser` and `json`, with psutil's creation times. An upgrade must keep both, and going back to 2.x must not lose them. General-purpose INI and JSON libraries accept and write different things: comments, interpolation, key case, duplicate rules, float formatting, escaping, key order.

## Decision

- `core::config` has a hand-written INI reader and writer that follow `configparser` with its defaults (Python 3.13): delimiters, comments, lowercased keys, continuation lines, `[DEFAULT]`, `%` interpolation, strict duplicates and the exact written layout. It reads the ANSI code page v2.3.2 wrote and writes UTF-8, which `configparser` also reads.
- `process::state_file` keeps the state as an ordered list and writes the bytes `json.dump(state, f, indent=2)` produced, with Python's key order, float formatting and escaping; it reads with Python's duplicate and non-object rules. `create_time` is converted from the process creation `FILETIME` exactly as psutil does.
- Both files stay where v2.3.2 kept them and are written atomically.
- Neither is to be replaced by "idiomatic" library code: their order and duplicate rules are the point.

## Consequences

- 2.x and 3.x share and upgrade the same files; 3.x keeps tracking apps a 2.x launcher started.
- Tests pin the formats with files written by the real Python libraries (`tests/fixtures/core/`, `tests/fixtures/process/`) and with property tests.
- `configparser`'s rules come along, including the `%` quirk (#13), until deliberately changed.
- More code than a dependency, whose rules must be read as compatibility rules, not style.
