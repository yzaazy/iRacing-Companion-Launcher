# CLAUDE.md

Guidance for coding agents working in this repository.

## Project

A Windows desktop app (Rust, egui/eframe 0.37, `windows` crate) that launches and closes sim-racing companion apps and a selected racing sim, tracks the process trees it started, and shows status and an activity log. Version 3.x is a 1:1 port of the Python app v2.3.2 (git tag `v2.3.2`), with its window reproduced pixel for pixel at 1200x700 and its files kept compatible.

## Layout

```
src/main.rs          window shell: startup, eframe loop
src/lib.rs           declares every module
src/model.rs         ViewState, UiAction and the other shared types
src/app.rs           Controller (state, actions, events); Settings and FileDialog seams
src/worker.rs        Command/Event, Sequencer, Worker thread, Dispatch seam
src/managers.rs      Backend seam and Managers: detection, launch, close
src/core/            config.ini store, app/game definitions, log lines
src/detect/          Start Menu, Steam registry, common paths
src/process/         name-based process ops, tracker, process_state.json, Win32 primitives
src/dialog.rs        the Browse dialog
src/ui/              pure draw function: packer port, layout rules, theme, widgets, input, log, text, fonts
src/win.rs           shared Win32 helpers
tests/               one binary per area; tests/ui/ is the UI binary
tests/fixtures/      data captured from v2.3.2 (see its README); tests/fixtures/ui/ holds the UI spec, fixture states and reference screenshots
tests/snapshots/     golden renders of the Rust window
tools/gate.ps1       the gate
.github/workflows/   CI (the gate), Release (draft release on a v* tag), Audit (weekly)
windows installer.iss  Inno Setup script
```

## Where the truth is

- The code and its tests.
- [docs/behaviour.md](docs/behaviour.md): what the app does, the changes from v2.3.2, the quirks kept on purpose (each with its issue), the undecided log differences. Behaviour not listed there as a change must match v2.3.2; a quirk stays until its issue is worked on.
- [docs/architecture.md](docs/architecture.md): modules, seams, threads, tests, parity criteria, packaging. [docs/adr/](docs/adr/README.md): the decisions behind them; do not reverse one in passing.

## Gate

`pwsh tools/gate.ps1` runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`. Work is done when it prints `GATE GREEN` on the final edit. CI runs the same script on every pull request and push to `main`; change the gate there, not in the workflow. Fix lint hits in code; an `#[allow]` needs a reason beside it. Never weaken a test to get green. The `rust-gate` skill has the details.

## Run

`cargo run`. When `cargo` is not found, prepend `$env:USERPROFILE\.cargo\bin` to PATH.

## Guardrails

- The developer's real companion apps and games (Fanatec, Stream Deck, SimHub, iRacing and the rest) run on this machine. Tests launch and kill only **dummy processes** they started themselves, under names no real app uses (`tests/dummy_support`). Pressing Launch or Close in a running build is the developer's step.
- Debug builds and tests keep config and state under `target/dev-config/` or a temp directory. Only an installed release build uses `%APPDATA%\iRacingCompanionLauncher`.
- Edit only the files your brief assigns to you.
- Claim a GitHub issue before working on it, and release the claim when dropping it: the `claim` skill.
- Pushing, tagging, building the installer and releasing each wait for the user's explicit go.
- `main` takes changes only through pull requests, which need the green `Gate` check and the owner's approval; only the owner merges.

## UI changes

`cargo test --test ui parity` compares each fixture state with the v2.3.2 screenshot (`tests/fixtures/ui/reference/`) and then with its golden render (`tests/snapshots/`). Follow the `ui-parity` skill after any change under `src/ui`.

- Parity must pass. The reference screenshots are fixed data from v2.3.2; a change meant to make the window differ from them is the owner's decision and goes into `docs/behaviour.md`.
- Re-approve golden renders (`UPDATE_SNAPSHOTS=1`) only for an intended visible change that still passes parity, after looking at the new images; commit them with the change.

## Version, build and release

The version lives only in `version` in `Cargo.toml`; tags are `v{version}`. Pushing a `v*` tag makes the Release workflow build the exe and the installer and create a **draft** release with the installer attached; the owner tests it, edits the notes and publishes it. For bumping the version, tagging or releasing, use the `release` skill. Each outward step waits for the owner's explicit go.

Release notifications reach Discord through GitTrack (gittrack.me), connected to the repository by a GitHub webhook on "Releases"; publishing the draft is all it needs.
