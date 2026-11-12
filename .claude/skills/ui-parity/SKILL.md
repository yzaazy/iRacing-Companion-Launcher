---
name: ui-parity
description: Compare the Rust UI with the reference screenshots of the Python app and read the differences. Use after any change under src/ui, and when reviewing the UI.
---

The reference images in `docs/rewrite/reference/<state>.png` were captured from the running Python app. The same states are defined as data in `docs/rewrite/fixtures.json`. The pass criteria are the "Parity criteria" table in `docs/rewrite/contracts.md`; `tests/ui/parity_check.rs` implements them and documents each class in full.

1. **Check.** `cargo test --test ui parity` renders every fixture state in memory and compares it with its reference in the `flat`, `edges` and `text` classes. It writes each render to `target/ui-snapshots/<state>.png`. A failing state lists, per class, the elements that differ and why, and writes `target/ui-diff/<state>.png`: the reference dimmed, text regions outlined in dark cyan, edge zones in dark magenta, flat differences in red, non-edge differences inside zones in orange, and each failing text element's reference ink box in green and the render's in yellow. The same tests run in `cargo test`, so the gate fails when parity breaks.

2. **Look.** Open the diff image and the two source images for every state that fails, and for at least one passing state per session. Numbers say how much differs; only looking says what. A text element shifted by a pixel or two sits inside its text region's margin, so check text position by eye as well as by the ink-box report.

3. **Classify each differing region.**
   - *Geometry*: an edge, gap or size is off. Fix the rule in `layout.rs` (or the packer in `pack.rs` when the rule is right and its evaluation is not).
   - *Colour*: a fill, border or text colour is off. Fix the token in `theme.rs`.
   - *Shape*: a corner radius, ring width, dot size or checkmark is off. Fix the painter call.
   - *Text position*: an ink box is off. Fix the baseline, inset or letter spacing.
   - *Anti-aliasing*: same shape or glyph in the same place, different edge pixels. Expected; leave it.

4. **Repeat** until every state passes all three classes.

5. **Golden renders.** The same tests compare each render with `tests/snapshots/<state>.png`, a committed render of the Rust window, at egui_kittest's per-pixel threshold 0.6 with no failing pixel allowed; a failure writes `<state>.diff.png` and `<state>.new.png` beside it. Any visible change to the window fails it, including one that keeps parity. When the change is intended and every state passes parity, re-approve with `UPDATE_SNAPSHOTS=1 cargo test --test ui parity`, look at the new images, and commit them.

Report the per-state result for each class and each remaining region with its class.
