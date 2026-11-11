---
name: ui-parity
description: Compare the Rust UI with the reference screenshots of the Python app and read the differences. Use after any change under src/ui, and when reviewing the UI.
---

The reference images in `docs/rewrite/reference/<state>.png` were captured from the running Python app. The same states are defined as data in `docs/rewrite/fixtures.json`.

1. **Render.** `cargo test --test ui_snapshots` writes `target/ui-snapshots/<state>.png` for every fixture state.

2. **Diff.** `python tools/ui_diff.py` compares each pair and prints, per state, the share of differing pixels outside text and inside text, and writes `target/ui-diff/<state>.png` with differences highlighted.

3. **Look.** Open the diff image and the two source images for every state the tool flags. Numbers say how much differs; only looking says what.

4. **Classify each differing region.**
   - *Geometry*: an edge, gap or size is off. Fix the rule in `layout.rs`.
   - *Colour*: a fill, border or text colour is off. Fix the token in `theme.rs`.
   - *Shape*: a corner radius, ring width or checkmark is off. Fix the painter call.
   - *Glyph edges*: same text, same position, different anti-aliasing. Expected; leave it.

5. **Repeat** until every state reports zero differing pixels outside text, and text regions differ only at glyph edges with text boxes at the same position and size.

Report the per-state numbers and each remaining region with its class.
