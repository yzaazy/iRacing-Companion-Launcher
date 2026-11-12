---
name: rust-gate
description: Definition of done for Rust changes in this repo. Run before reporting any Rust work as finished, and after every fix.
---

Run the gate from the repo root:

```
pwsh tools/gate.ps1
```

It runs `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --no-fail-fast` (every failing test binary reported in one run) on the toolchain pinned in `rust-toolchain.toml`, and ends with `GATE GREEN` or `GATE RED: <step>`. `pwsh tools/gate.ps1 -Release` adds `cargo build --release` and compiles the installer script into a temp directory; when Inno Setup is not installed it skips that step and says so on the last line (`GATE GREEN (installer check skipped: ...)`).

The gate holds the machine-wide build lock (`tools/build-lock.ps1`) from start to end. When another worktree holds it, the gate prints `== waiting for the build lock` and starts once that build ends; run it in the background and wait for it rather than killing it.

Work is done when the last line is `GATE GREEN` on your final edit. On red, fix the cause and rerun the whole gate.

A fix makes the code correct. Keep every test and assertion, and satisfy a lint by changing the code. An `#[allow(...)]` needs a one-line reason beside it and a mention in your report. A change under `src/ui` also follows the `ui-parity` skill.

Quote the gate's last line in your report.
