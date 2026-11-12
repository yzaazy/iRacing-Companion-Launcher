# 0006. Plain error types, a fixed lint set, the gate as the definition of done

**Status:** Accepted

## Context

Most failures in the launcher have exactly one consequence the user sees: a card turns red, a log line says "failed to start", a path is not found. The log has no further detail to show, and v2.3.2 reported these as `True`/`False` or `None`. Error frameworks and wide lint groups would add code and churn without changing what the user sees. Several people and coding agents change this code, so "done" needs one mechanical definition.

## Decision

- **Errors.** `bool` and `Option` at the seams the UI reacts to: the `Backend` trait, `ProcessTracker`, the managers. Each result is exactly what the UI acts on. Small hand-written enums where a caller tells cases apart (the config reader's private `ReadError`). `io::Result` inside file and Win32 code. No `anyhow`, no `thiserror`. Nothing panics on a missing file, a dead PID or access denied.
- **Lints.** The `[lints]` table in `Cargo.toml`: `unreachable_pub`, `undocumented_unsafe_blocks`, `borrow_as_ptr`, `derive_partial_eq_without_eq`, `trivially_copy_pass_by_ref`, `manual_let_else`, `semicolon_if_nothing_returned`, `unwrap_used`, `expect_used`, `dbg_macro`, `todo`. Test code may `unwrap` and `expect` (`clippy.toml`). Not `clippy::pedantic` or `nursery` wholesale, and not the arithmetic, indexing or `as` restriction lints.
- **The gate.** `pwsh tools/gate.ps1` runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` on the toolchain pinned in `rust-toolchain.toml`, and prints `GATE GREEN` or `GATE RED: <step>`. `-Release` adds the release build and a compile of the installer script into a temp directory. Work is done when it prints `GATE GREEN`. A lint hit is fixed in code; an `#[allow]` needs a reason beside it.
- No multi-crate workspace, no `cargo-deny`. CI runs the same gate on every pull request and push to `main` (issue #6).

## Consequences

- Results map one to one onto what the UI shows, and tests assert them directly.
- A new error case that callers must tell apart is a new variant, not a string.
- A `rustup update` cannot turn the gate red; moving the toolchain pin is a deliberate change followed by a gate run.
- A pull request into `main` cannot merge without the green `Gate` check.
