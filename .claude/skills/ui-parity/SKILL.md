---
name: ui-parity
description: Compare the Rust UI with the reference screenshots of the Python app and read the differences. Use after any change under src/ui, and when reviewing the UI.
---

The reference images in `docs/rewrite/reference/<state>.png` were captured from the running Python app. The same states are defined as data in `docs/rewrite/fixtures.json`. The pass criteria are the "Parity criteria" table in `docs/rewrite/contracts.md`.

1. **Render.** `cargo test --test ui_snapshots` writes `target/ui-snapshots/<state>.png` for every fixture state.

2. **Diff.** `python tools/ui_diff.py` compares each pair, reports the `flat`, `edges` and `text` classes per state, and writes `target/ui-diff/<state>.png` with differences highlighted.

3. **Look.** Open the diff image and the two source images for every state the tool flags, and for at least one passing state per session. Numbers say how much differs; only looking says what. A text element shifted by a pixel or two sits inside its text region's margin, so check text position by eye as well as by the tool's ink-box report.

4. **Classify each differing region.**
   - *Geometry*: an edge, gap or size is off. Fix the rule in `layout.rs`.
   - *Colour*: a fill, border or text colour is off. Fix the token in `theme.rs`.
   - *Shape*: a corner radius, ring width, dot size or checkmark is off. Fix the painter call.
   - *Text position*: an ink box is off. Fix the baseline, inset or letter spacing.
   - *Anti-aliasing*: same shape or glyph in the same place, different edge pixels. Expected; leave it.

5. **Repeat** until every state passes all three classes.

Report the per-state numbers for each class and each remaining region with its class.
