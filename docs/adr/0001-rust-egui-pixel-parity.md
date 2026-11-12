# 0001. Rust and egui, keeping the v2.3.2 window pixel for pixel

**Status:** Accepted

## Context

Up to v2.3.2 the launcher was a Python/CustomTkinter app that PyInstaller packed into a folder of files. Users knew its window and its behaviour; the goal was a single native exe that changes neither. A rewrite that "looks about the same" would be judged by eye and drift with every change. CustomTkinter's look comes from Tk: the packer's allocation order, painted sizes rounded down to even pixels, GDI's whole-pixel glyph advances. egui's widgets look and behave differently (focus, a 5 px hit slack, kerned text, their own scroll areas).

## Decision

- Rewrite in Rust with egui/eframe 0.37 and keep v2.3.2's behaviour one to one, quirks included. The only differences are the ones listed in [behaviour.md](../behaviour.md#changes-from-v232).
- Keep the window identical to v2.3.2 at 1200x700, enforced by tests that compare renders with screenshots of v2.3.2 ([parity](../architecture.md#pixel-parity-and-golden-renders)).
- The UI is one pure function, `ui::draw(ui, &ViewState) -> Vec<UiAction>`. The controller computes every label and enabled flag; the UI keeps only hover, focus, scroll position, the log selection and the button flash.
- No egui widgets. The UI paints with egui's painter and has its own layout (`ui::pack`, a port of Tk's packer, and `ui::layout`, the launcher's rules on top of it), its own focus and hit-testing (`ui::input`), its own log text view (`ui::log`) and GDI-style text layout (`ui::text`).
- Layout is rules evaluated for a window size, not measured coordinates, so the window can become resizable later.

## Consequences

- The UI is tested without a controller (feed a `ViewState`, read actions and pixels), and the controller without a UI.
- Every visible change fails the golden renders and must be re-approved. A change away from v2.3.2's look also changes what parity expects, which is a deliberate decision.
- More UI code than egui's widgets would need (packer, text layout, log view, font fallback), each part with its own tests.
- Keyboard access had to be designed explicitly, since egui's focus is not used.
