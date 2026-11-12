# 0002. One package: library plus thin binary

**Status:** Accepted

## Context

The app is about 9,000 lines with one exe and no other consumers. Integration tests need to reach the controller, worker, managers, process layer and UI without a window. A Cargo workspace of several crates would add manifests, version pins and public APIs between crates, and nothing would gain from the separation.

## Decision

One package at the repo root, `iracing-companion-launcher`. `src/lib.rs` declares every module; `src/main.rs` is the window shell: resolve the config directory, run startup, open the window, pump worker events and draw each frame. No workspace.

## Consequences

- Tests under `tests/` link the library and drive the same code the exe runs.
- Module boundaries are held by visibility, not crate boundaries: an item is `pub` only when `main.rs` or a test uses it, and the `unreachable_pub` lint keeps the rest `pub(crate)` or narrower.
- One `Cargo.toml` holds versions, lints, profiles and the exe's version-resource metadata.
- Splitting out a crate later (the UI, say) stays possible along the existing module seams.
