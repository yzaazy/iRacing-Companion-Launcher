# Architecture decision records

Short records of the decisions that shape the code, each with the context that led to it and what follows from it. A new decision gets the next number; a reversed one is marked Superseded and links its successor.

| # | Decision | Status |
|---|---|---|
| [0001](0001-rust-egui-pixel-parity.md) | Rust and egui; the v2.3.2 window pixel for pixel; a pure draw function with its own layout, focus and log | Accepted |
| [0002](0002-one-package-library-and-binary.md) | One package: library plus thin binary, no workspace | Accepted |
| [0003](0003-one-worker-thread-no-async.md) | One worker thread fed by a command queue, no async runtime | Accepted |
| [0004](0004-win32-through-windows-crate.md) | Win32 directly through the `windows` crate | Accepted |
| [0005](0005-files-compatible-with-v2.md) | `config.ini` and `process_state.json` byte-compatible with v2.3.2 | Accepted |
| [0006](0006-errors-lints-and-the-gate.md) | Plain error types, a fixed lint set, the gate as the definition of done | Accepted |
