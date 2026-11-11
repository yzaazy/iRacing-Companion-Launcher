---
name: rust-gate
description: Definition of done for Rust changes in this repo. Run before reporting any Rust work as finished, and after every fix.
---

Run the gate from the repo root:

```
pwsh tools/gate.ps1
```

It runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`, and ends with `GATE GREEN` or `GATE RED: <step>`.

Work is done when the last line is `GATE GREEN` on your final edit. On red, fix the cause and rerun the whole gate.

A fix makes the code correct. Keep every test and assertion, and satisfy a lint by changing the code. An `#[allow(...)]` needs a one-line reason beside it and a mention in your report.

Quote the gate's last line in your report.
