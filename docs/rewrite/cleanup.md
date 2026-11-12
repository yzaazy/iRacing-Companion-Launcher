# Post-port cleanup

The port works and the owner has verified it against his real programs. Three audits (architecture, idioms and safety, tests and tooling) produced the work below. Each agent's brief names the items it owns; this file is the shared reference for what each item means and for the rules that apply to all of them.

## Rules

- **Behaviour is frozen.** Nothing here may change what the user sees or what happens to a process: same log lines, same order, same statuses, same files written with the same bytes, same pixels. `behaviour.md` lists the only deliberate differences from the Python and the quirks ported on purpose. A change that would alter behaviour is out of scope: stop and report it.
- **The owner's real-app test was done on commit `4eea24b`.** The code that launches and closes real programs can only be retested by him, by hand. Keep changes to that code to moves and merges of identical logic, and say in your report exactly which launch, close, shell or dialog code you touched.
- **The gate stays green**: `pwsh tools/gate.ps1`. The UI must keep passing the pixel comparison in all five fixture states.
- **Tests are never weakened to get to green.** A test that changes must protect at least as much as before; say what changed and why.
- Line numbers below were taken at `4eea24b` and drift as work lands. Find the code by name.

## Not to do

A multi-crate workspace. tokio or any async runtime. `thiserror` or `anyhow`. Turning the `bool` and `Option` results on the `Backend` trait, `ProcessTracker`, `AppManager` and `GameManager` into `Result`: each result is exactly what the UI reacts to, and the log has no more detail to show. An id type for apps and games. `clippy::pedantic` or `nursery` wholesale, or the arithmetic, indexing and `as` restriction lints. `cargo-deny`. CI. "Idiomatic" rewrites of the INI reader and writer, the `process_state.json` reader and writer (`State(Vec<(String, Slot)>)` and its hand-written `Deserialize`), or the config store's `Vec<(String, String)>`: their order and duplicate rules are Python's, byte for byte.

## A. Safety fixes

- **A1. `AttributeList` dropped uninitialised.** `src/process/mod.rs`, `spawn_detached`: the list is constructed before the second `InitializeProcThreadAttributeList` call, so an early return on its failure runs `Drop`, which calls `DeleteProcThreadAttributeList` on a list that was never initialised. Make construction fallible (`AttributeList::new(count) -> io::Result<Self>`) so a value, and therefore a `Drop`, exists only after successful initialisation, with a buffer of at least one element. The success path is unchanged.
- **A2. Font parsing trusts sizes from the file.** `src/ui/fonts.rs`, `read_faces` and `read_face`: buffers are allocated from lengths read out of a `.ttc` header or table directory before `read_exact` can fail, so a corrupt font file can request gigabytes and a failed allocation aborts the app. Bound every length by the file's size. `start + 1` in the cmap group walk overflows at `u32::MAX`: use saturating arithmetic.
- **A3. Tracker save fallback.** `src/process/tracker.rs`: `to_string_pretty(...).unwrap_or_else(|_| "{}")` would overwrite every tracked entry with an empty object if serialising ever failed. On failure, skip the save.

## B. Tests and gate

- **B1. Freeze the Python cross-checks.** `tests/process_layer.rs` has three tests that run the real Python `ProcessTracker` and are not `#[ignore]`d; when the Python app is deleted they fail. Before that happens, capture what they prove as static fixtures: a `process_state.json` written by the Python tracker (against a dummy process, never a real app), with tests that the Rust tracker reads its format and that Rust's output for the same state is byte-identical to Python's. Drop the live psutil `create_time` comparison; the conversion arithmetic has its own test. `tests/detect_python_crosscheck.rs` (`#[ignore]`d): run it once more, freeze `fake_tree_matches_python`'s Python answers as literal expectations in `tests/detect_shortcuts.rs`, then delete the file.
- **B2. Pixel parity inside the gate.** Today `render_fixture_states` only asserts the image size, and the real comparison is `tools/ui_diff.py`, which nothing runs. Port the comparison (the three classes in `contracts.md`, "Parity criteria", with the text colour and top-edge checks) to a Rust test that renders each fixture state in memory and compares it with `docs/rewrite/reference/<state>.png`, writing a diff image under `target/` on failure. Port its self-test too: an exact copy passes, and a planted block, a shifted text, a recoloured text, an erased dot and a recoloured checkbox each fail only their class. Prove the port equivalent to the Python tool on today's renders and on the self-test cases, then delete `tools/ui_diff.py` and update `.claude/skills/ui-parity/SKILL.md` and the mentions in `contracts.md` and `CLAUDE.md`.
- **B3. Golden snapshots instead of pinned hashes.** `tests/ui_keyboard.rs` pins FNV hashes of five renders. A hash fails on one changed pixel, says nothing about where, and is tied to this machine's CPU rasteriser and font files. Replace with committed golden PNGs of the Rust render compared through egui_kittest's snapshot support (the feature is already enabled) with a small threshold, a diff image on failure, and re-approval through `UPDATE_SNAPSHOTS=1`.
- **B4. Tests that cannot fail or pass on missing input.**
  - `new_log_line_scrolls_to_the_end` (`tests/ui_interaction.rs`): its assertions hold whether or not the log scrolled. Assert through `ui::log::row_texts` that the last visible row is the new line, or compare with a fresh harness on the new state.
  - Silent passes: `tests/ui_snapshots.rs` falls back to a placeholder when `fixtures.json` is missing and `continue`s on a missing state; `tests/ui_fonts.rs` returns early when font files are missing; `tests/process_layer.rs` returns a pass when Python or psutil is missing. Fail loudly, or `#[ignore = "..."]` with the reason.
  - Look-only renders (`render_other_window_sizes`, `render_display_scales`, `rare_characters_render_without_a_missing_glyph`) cost about 17 s and assert almost nothing. Keep one display scale with a real assertion (for example that every dot and the focus ring lie inside their card), keep the missing-glyph check, and move the rest behind `#[ignore = "visual inspection"]`.
  - Tests of the test: `temp_key_is_removed_after_the_test` (`tests/detect_steam.rs`), `every_level_is_kept` (`tests/core_log.rs`). Delete or fold in.
  - The config fixture list is written out twice (`tests/core_config.rs` and the unit tests in `src/core/config.rs`), so a new fixture file can go untested. Enumerate `tests/fixtures/core/*.ini` from the directory.
- **B5. Speed.** Warm test run is about 88 s, 80 s of it in four render binaries.
  - `[profile.dev.package."*"] opt-level = 2` in `Cargo.toml`: measured 88 s to 55 s with every test passing. Retune the log performance guard for it (use far more lines, or a tighter slack plus a ratio), since shorter frames let the fixed 2 ms slack hide a moderate regression.
  - One UI test binary (`tests/ui/main.rs` with modules and one `support` module) instead of six files: one link instead of four large ones, no duplicated `gpu()`, fixture loading, `harness`, `Probe`, `pixel`, `geometry`.
  - One wgpu device per process, shared by every harness, and the render lock taken only by tests that render.
- **B6. Gate and toolchain.** `rust-toolchain.toml` pinning the channel with `rustfmt` and `clippy`, so a `rustup update` cannot turn the gate red. `tools/gate.ps1 -Release` also compiles the installer script with ISCC into a temp directory (never `installer_output/`, never run the result), skipping with a clear message when ISCC is not installed.

## C. Structure

- **C1. Shared types.** `src/contract.rs` becomes `src/model.rs` with what the screen shows and what the user did (`Status`, `LogLevel`, `LogLine`, `AppCard`, `GameCard`, `ViewState`, `UiAction`). `Command` and `Event`, the worker's protocol, move into `src/worker.rs`. Keep the serde derives, with one line in the module doc saying the fixtures and transcripts need them. `Status` and `LogLevel` gain `Copy, Eq, Hash`; every model type without floats gains `Eq`. Remove the `.clone()`s this makes needless and pass the two enums by value.
- **C2. Platform code out of the controller.** The `IFileOpenDialog` implementation in `src/app.rs` (`NativeDialog`, `open_dialog`, `tk_open_options`, with its tests) moves to `src/dialog.rs`; the `FileDialog` trait and `DialogRequest` stay with the controller. `app.rs` ends with no `windows` imports.
- **C3. One copy of each Windows helper**, in a crate-private `src/win.rs`:
  - `wide()` (three copies: `app.rs`, `process/mod.rs`, `detect/shortcuts.rs`).
  - `ntpath.basename` (two copies that disagree on `a:b:c`: keep the `app.rs` version, which matches Python).
  - The COM apartment guard (`ComApartment` in `worker.rs`, `ComGuard` in `detect/shortcuts.rs`): one type, `enter() -> Option<Self>`, preserving each caller's current behaviour on every return code.
  - RAII wrappers: `Handle` (`process/mod.rs`) and `FindHandle` (`detect/shortcuts.rs`) differ only in the close call, and `detect/steam.rs` closes its registry key by hand.
  - The poison-tolerant lock written six times (`managers.rs`, `app.rs`): `SharedConfig` becomes a newtype in `core/config.rs` with a `lock()` method, which also removes the controller's dependency on `managers` for a config type.
- **C4. Files that do two jobs.** Pure moves, no line of logic changed:
  - `ui/layout.rs` into `ui/pack.rs` (the port of Tk's packer) and `ui/layout.rs` (the launcher's rules).
  - `ui/sections.rs` into `ui/geometry.rs` (resolution, which tests read) and `ui/sections.rs` (painting).
  - `ui/fonts.rs` into `ui/fonts/mod.rs` and `ui/fonts/cmap.rs`.
  - `process/mod.rs` into `process/mod.rs` (the public name-based functions) and `process/sys.rs` (the Win32 helpers that copy psutil).
  - `process/tracker.rs` into `tracker.rs` and `process/state_file.rs` (the Python-compatible `process_state.json` reading and writing).
  - `ui::log::show` (240 lines) splits into wheel, keyboard, scrollbar, selection and paint steps.
- **C5. Smaller idiom items.**
  - `set_app_path` merges "configparser refused the value" and "the save failed" into one `bool`, so `app.rs` re-derives the cause: return `Result<(), SetPathError>` with `Refused` and `Save(io::Error)`, and make `configparser_accepts` private again.
  - `Result<_, ()>` in the config reader: a named error type.
  - `save()` and `replace_atomically` return `io::Result<()>` internally; the `bool` stays only where Python ignored the result. Ignored saves in `app.rs` become explicit, with a one-line comment that Python ignores them too.
  - Booleans to enums: `needs_shell_launch: bool` on `launch_and_track`, `is_game: bool` on `find_shortcut_target`. The two parallel pairs `launch_normal`/`launches_pending` and `close_normal`/`closes_pending` in `app.rs` become one small struct used twice.
  - `Controls::stops` in `ui/input.rs` is a four-tuple with a nested `Option` of a tuple: a struct.
  - `AppManager::find_app_path` clones a whole `AppDef` where a borrow compiles. `configparser_accepts` reallocates per `%`: walk with a `&str` cursor.
  - Magic numbers: `0xC000_0004_u32 as i32` is `STATUS_INFO_LENGTH_MISMATCH`; `116_444_736_000_000_000` deserves a named constant. In `nt_create_time`, bound the walk by the bytes the kernel wrote.
  - `SAFETY:` comments on the twelve `unsafe` blocks that lack one (`detect/shortcuts.rs`, `detect/steam.rs`, `app.rs`), and real invariants in the ones that say "plain call" or "as above".
  - `&mut x` coerced to a raw pointer at FFI calls: `&raw mut x`.
- **C6. Public surface, dead code, lints.** Done last, so the moved code is linted.
  - `pub(crate)` unless `main.rs` or `tests/` need the item. Real test seams stay public (`find_shortcut_target_in`, `check_steam_game_installed_in`, `Inline`, `Controller::startup` with a trace); test-only helpers get a one-line "for tests" doc.
  - Delete what only a test keeps alive: `GameManager::close_game`, `AppManager::get_child_count`, `ProcessTracker::forget`, with their assertions. Implement `Backend` directly on the managers and drop the duplicate `get_*` names.
  - `Cargo.toml`:
    ```toml
    [lints.rust]
    unreachable_pub = "warn"

    [lints.clippy]
    undocumented_unsafe_blocks = "warn"
    borrow_as_ptr = "warn"
    derive_partial_eq_without_eq = "warn"
    trivially_copy_pass_by_ref = "warn"
    manual_let_else = "warn"
    semicolon_if_nothing_returned = "warn"
    unwrap_used = "warn"
    expect_used = "warn"
    dbg_macro = "warn"
    todo = "warn"
    ```
    with whatever is needed so test code may still `unwrap` and `expect`. Every hit is fixed in code, not allowed away.
  - `Cargo.toml` hygiene: drop the `[[bin]]` section that repeats the defaults; add `license` and `repository`; remove each `windows` feature that nothing needs (`Win32_System_Com_StructuredStorage` and `Win32_System_Time` are candidates; removing one and compiling is the test).

## D. Coverage gaps

- **D1. The managers' own logic is about 30% covered.** The transcripts drive a fake that replaces the managers entirely, and the comment at the top of `tests/app_managers.rs` claims otherwise. Add constructors that take the definitions and the Start Menu roots (and an injectable shell-open for `launch_game`), leaving the default constructors and their behaviour untouched, and test with dummy exe names and a fake Start Menu tree: shortcut found and saved to config, common path found and saved, Steam hit saving the `steam://` URL, `is_app_running` and `close_app` falling back from tracked to name match, `launch_game` routing for `steam://`, `.lnk` and `.exe`. Correct the comment.
- **D2. Log gestures with no test** (`src/ui/log.rs`): double-click selects a word, triple-click selects a line, a press or drag on the scrollbar centres the thumb on the pointer, arrow keys scroll one line when the log is focused, dragging past the top or bottom keeps scrolling.
- **D3. Property tests** (`proptest` as a dev-dependency): `ConfigStore::load` never panics on arbitrary bytes; saving twice gives the same bytes; a value `configparser` accepts survives set, save, load, get; the tracker's state-file load never panics on arbitrary JSON; the game-name wrap keeps short names, otherwise breaks exactly once with a first line of at most 18 characters and loses nothing; `list2cmdline` round-trips through `CommandLineToArgvW`.

## Later, with the removal of the Python app

Repository layout, `docs/architecture.md` and the ADRs, `README.md` and `CLAUDE.md`, renaming the Python-style getters, and the comments that link to Python files by line number. Not part of this round.
