//! The log's pointer and keyboard gestures, against Tk's Text bindings (`tk8.6/text.tcl`) and
//! CustomTkinter's scrollbar (`ctk_scrollbar.py`): word and line selection, the scrollbar's
//! press and drag, the keys of the focused log, scanning while a drag is held past an edge, and
//! what clears a selection and what does not.
//!
//! Every assertion reads what the user gets: the text a copy puts on the clipboard, or where the
//! frame paints the log's lines and the scrollbar thumb. The paint is read from the frame's
//! shapes rather than from a GPU render, which costs about 180 ms a frame;
//! `a_wheel_notch_jumps_40_px_at_once` (`interaction.rs`) holds the pixels to the same offsets.

use crate::support::{
    Probe, button_event, copied, fixture, geometry, harness, live, press, tabs, wheel, with_ctx,
};
use egui::{Key, Pos2, pos2};
use egui_kittest::Harness;
use iracing_companion_launcher::model::{LogLine, UiAction, ViewState};
use iracing_companion_launcher::ui::{self, pack::Bounds, theme};

/// A log as the window shows it.
struct Log {
    state: ViewState,
    inner: Bounds,
    bar: Option<Bounds>,
    /// Every row the log wraps to, the empty row after the last newline included.
    rows: Vec<String>,
    /// The first row of each line.
    starts: Vec<usize>,
}

const LINE: f32 = 17.0;
/// Consolas 14 advances 8 px a character.
const ADVANCE: i32 = 8;

impl Log {
    /// `log-full`: 42 rows in a 429 px view, so it scrolls 285 px.
    fn full() -> Self {
        Self::of(fixture("log-full"))
    }

    /// `log-full` with its log twice over, the second time marked so that a run of lines
    /// places the view: it scrolls more than two pages.
    fn long() -> Self {
        let mut state = fixture("log-full");
        let again = state.log.iter().map(|line| LogLine {
            text: format!("{} again", line.text),
            ..line.clone()
        });
        state.log.extend(again.collect::<Vec<_>>());
        Self::of(state)
    }

    fn of(state: ViewState) -> Self {
        assert_eq!(theme::LOG_FONT.linespace() as f32, LINE);
        let geom = geometry(&state);
        let inner = geom.log.inner;
        let (rows, starts) = with_ctx(|ctx| {
            let rows = ui::log::row_texts(ctx, &state, inner.w as f32);
            let mut starts = Vec::new();
            let mut row = 0;
            for line in &state.log {
                starts.push(row);
                let alone = ViewState {
                    log: vec![line.clone()],
                    ..state.clone()
                };
                row += ui::log::row_texts(ctx, &alone, inner.w as f32).len() - 1;
            }
            (rows, starts)
        });
        Self {
            state,
            inner,
            bar: geom.log.scrollbar,
            rows,
            starts,
        }
    }

    fn bar(&self) -> Bounds {
        self.bar.expect("the log has a scrollbar").painted()
    }

    fn content_h(&self) -> f32 {
        self.rows.len() as f32 * LINE
    }

    fn view_h(&self) -> f32 {
        self.inner.h as f32
    }

    /// How far the view scrolls: the offset of the end, where a new window starts.
    fn max_offset(&self) -> f32 {
        (self.content_h() - self.view_h()).max(0.0)
    }

    /// The middle of row `row`, when the view is scrolled `offset` px from the top.
    fn row_y(&self, row: usize, offset: f32) -> f32 {
        self.inner.y as f32 + row as f32 * LINE - offset + LINE / 2.0
    }

    /// 1 px into the cell of character `ch`: that character's left edge.
    fn ch_x(&self, ch: usize) -> f32 {
        (self.inner.x + ADVANCE * ch as i32 + 1) as f32
    }

    /// Character `ch` of row `row`, with the view at its end.
    fn at_end(&self, row: usize, ch: usize) -> Pos2 {
        pos2(self.ch_x(ch), self.row_y(row, self.max_offset()))
    }

    fn line(&self, i: usize) -> &str {
        &self.state.log[i].text
    }

    /// A point over the text, for the wheel.
    fn over_text(&self) -> Pos2 {
        self.inner.rect().center()
    }

    /// How far from the top the last frame painted the text: the log's lines as painted inside
    /// the text area, matched to the log, with each line's top where its baseline sits less the
    /// font's ascent, as Tk places a line.
    fn painted_offset(&self, h: &Harness<'static, Probe>) -> f32 {
        let lines: Vec<(String, f32)> = h
            .output()
            .shapes
            .iter()
            .filter(|c| c.clip_rect == self.inner.rect())
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => {
                    let baseline = t.galley.rows[0].glyphs.first()?.pos.y;
                    let top = t.pos.y + baseline - theme::LOG_FONT.ascent as f32;
                    Some((t.galley.text().trim_end_matches('\n').to_owned(), top))
                }
                _ => None,
            })
            .collect();
        assert!(!lines.is_empty(), "no log line painted");
        let log = &self.state.log;
        let runs: Vec<usize> = (0..log.len())
            .filter(|&i| {
                lines
                    .iter()
                    .enumerate()
                    .all(|(k, (text, _))| log.get(i + k).is_some_and(|l| &l.text == text))
            })
            .collect();
        assert_eq!(
            runs.len(),
            1,
            "the painted lines are not one run of the log"
        );
        self.inner.y as f32 + self.starts[runs[0]] as f32 * LINE - lines[0].1
    }

    /// The thumb's top and bottom as the last frame painted it.
    fn painted_thumb(&self, h: &Harness<'static, Probe>) -> (f32, f32) {
        let bar = self.bar().rect();
        let thumbs: Vec<egui::Rect> = h
            .output()
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Rect(r)
                    if r.rect.left() >= bar.left() && r.rect.right() <= bar.right() =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(thumbs.len(), 1, "one thumb");
        (thumbs[0].top(), thumbs[0].bottom())
    }
}

/// Press at `from`, move through `through`, release at the last point, a frame each.
fn drag(h: &mut Harness<'static, Probe>, from: Pos2, through: &[Pos2]) {
    h.hover_at(from);
    h.step();
    button_event(h, from, true);
    h.step();
    let mut last = from;
    for &p in through {
        h.hover_at(p);
        h.step();
        last = p;
    }
    button_event(h, last, false);
    h.step();
}

/// Click `times` times at `p`, 25 ms between presses and releases.
fn click(h: &mut Harness<'static, Probe>, p: Pos2, times: usize) {
    h.hover_at(p);
    h.step();
    for _ in 0..times {
        button_event(h, p, true);
        h.step();
        button_event(h, p, false);
        h.step();
    }
}

/// Scroll to the top with the wheel. A step, not `run`: egui's own smooth scrolling of the same
/// event keeps asking for frames, which `run` counts against its limit.
fn wheel_to_top(log: &Log, h: &mut Harness<'static, Probe>) {
    h.hover_at(log.over_text());
    h.step();
    wheel(h, 1000.0);
    h.step();
    assert_eq!(
        log.painted_offset(h),
        0.0,
        "the wheel did not reach the top"
    );
}

// ---------------------------------------------------------------------------------------------
// Word and line selection.

/// Tk's <Double-1> selects the word under the pointer; "Trading" is a word under either
/// definition (`\S` on Windows, `\w` elsewhere).
#[test]
fn a_double_click_selects_the_word() {
    let log = Log::full();
    // Row 38 is "[19:45:06] Trading Paints closed"; character 13 is the "a" of "Trading".
    assert_eq!(log.rows[38], log.line(37));
    let mut h = live(log.state.clone());
    click(&mut h, log.at_end(38, 13), 2);
    assert_eq!(copied(&mut h).as_deref(), Some("Trading"));
}

/// Tk's <Triple-1> selects the whole logical line, its newline included, from any of its
/// display rows.
#[test]
fn a_triple_click_selects_the_line() {
    let log = Log::full();
    // Rows 28 and 29 are line 28 wrapped; click its second row.
    assert_eq!(format!("{}{}", log.rows[28], log.rows[29]), log.line(28));
    let mut h = live(log.state.clone());
    click(&mut h, log.at_end(29, 5), 3);
    let want = format!("{}\n", log.line(28));
    assert_eq!(copied(&mut h).as_deref(), Some(want.as_str()));
}

/// Tk's <B1-Motion> after <Double-1> or <Triple-1> extends the selection by words or by lines
/// (`TextSelectTo` keeps `selectMode`). The widget selects on the release of the last click, so
/// a drag that follows the last press extends by characters instead.
#[test]
#[ignore = "differs from Tk: a drag after a double or triple click extends by characters"]
fn a_drag_after_a_double_or_triple_click_extends_by_words_or_lines() {
    let log = Log::full();
    // Double-press on "Trading" (row 38), drag to the "l" of "closed" (row 38, ch 27).
    let mut h = live(log.state.clone());
    let word = log.at_end(38, 13);
    click(&mut h, word, 1);
    drag(&mut h, word, &[log.at_end(38, 27)]);
    assert_eq!(copied(&mut h).as_deref(), Some("Trading Paints closed"));

    // Triple-press on row 37, drag down to row 38.
    let mut h = live(log.state.clone());
    let line = log.at_end(37, 13);
    click(&mut h, line, 2);
    drag(&mut h, line, &[log.at_end(38, 3)]);
    let want = format!("{}\n{}\n", log.line(36), log.line(37));
    assert_eq!(copied(&mut h).as_deref(), Some(want.as_str()));
}

/// Tcl's word characters on Windows are `\S` (`tcl8.6/word.tcl`), so Tk's <Double-1> on a
/// timestamp selects all of "[19:45:06]".
#[test]
#[ignore = "differs from Tk on Windows: words are \\w runs, not \\S runs"]
fn a_double_click_on_a_timestamp_selects_it_whole() {
    let log = Log::full();
    let mut h = live(log.state.clone());
    click(&mut h, log.at_end(38, 2), 2);
    assert_eq!(copied(&mut h).as_deref(), Some("[19:45:06]"));
}

// ---------------------------------------------------------------------------------------------
// What clears a selection.

/// Select from character 11 of line 37 to character 5 of line 38 (rows 38 and 39 at the end).
fn select_two_lines(log: &Log, h: &mut Harness<'static, Probe>) -> String {
    drag(h, log.at_end(38, 11), &[log.at_end(39, 5)]);
    let want = format!("{}\n{}", &log.line(37)[11..], &log.line(38)[..5]);
    assert_eq!(copied(h).as_deref(), Some(want.as_str()));
    want
}

/// Tk's <1> removes `sel` before anything else: a click without a drag leaves nothing selected,
/// so nothing is copied.
#[test]
fn a_click_in_the_log_clears_the_selection() {
    let log = Log::full();
    let mut h = live(log.state.clone());
    select_two_lines(&log, &mut h);
    click(&mut h, log.at_end(33, 4), 1);
    assert_eq!(copied(&mut h), None);
}

/// Only the Text widget's own bindings touch `sel`: a click on a button, on the window
/// background or on the scrollbar keeps it, scrolling it out of view keeps it, and Ctrl+C
/// (the Text's <<Copy>>) still copies it.
#[test]
fn a_selection_survives_clicks_elsewhere_and_scrolling() {
    let log = Log::full();
    let geom = geometry(&log.state);
    let mut h = live(log.state.clone());
    let want = select_two_lines(&log, &mut h);

    click(&mut h, geom.select_all.rect.rect().center(), 1);
    assert_eq!(h.state().actions, vec![UiAction::ToggleSelectAll]);
    assert_eq!(copied(&mut h).as_deref(), Some(want.as_str()), "a button");

    click(&mut h, pos2(5.0, 400.0), 1);
    assert_eq!(
        copied(&mut h).as_deref(),
        Some(want.as_str()),
        "the background"
    );

    // A press on the top of the trough scrolls to the top, far from the selection.
    let bar = log.bar();
    click(&mut h, pos2(bar.rect().center().x, bar.y as f32 + 1.0), 1);
    assert_eq!(log.painted_offset(&h), 0.0);
    assert_eq!(
        copied(&mut h).as_deref(),
        Some(want.as_str()),
        "the scrollbar"
    );

    h.hover_at(log.over_text());
    for _ in 0..3 {
        wheel(&h, -1.0);
        h.step();
    }
    assert_eq!(log.painted_offset(&h), 120.0);
    assert_eq!(copied(&mut h).as_deref(), Some(want.as_str()), "the wheel");
}

// ---------------------------------------------------------------------------------------------
// Scanning while a drag is held past an edge.

/// The number of lines a copy takes in, and the painted offset, every 3 frames, 3 times.
fn scan_samples(log: &Log, h: &mut Harness<'static, Probe>) -> Vec<(usize, f32)> {
    (0..3)
        .map(|_| {
            h.run_steps(3);
            let offset = log.painted_offset(h);
            (copied(h).map_or(0, |t| t.lines().count()), offset)
        })
        .collect()
}

/// Tk's <B1-Leave> starts `TextAutoScan`: while the button is held above or below the text, it
/// keeps scrolling and extends the selection to the pointer, until the end of the text.
#[test]
fn a_drag_held_above_the_text_scrolls_up_and_extends_to_the_top() {
    let log = Log::full();
    let mut h = live(log.state.clone());
    let anchor = log.at_end(38, 11);
    h.hover_at(anchor);
    h.step();
    button_event(&h, anchor, true);
    h.step();
    let above = pos2(log.ch_x(20), log.inner.y as f32 - 6.0);
    h.hover_at(above);
    h.step();

    // Each sample shows more of the top and takes in more lines than the one before.
    let samples = scan_samples(&log, &mut h);
    assert!(
        samples
            .windows(2)
            .all(|w| w[1].0 > w[0].0 && w[1].1 < w[0].1),
        "the scan stopped: (lines, offset) {samples:?}"
    );
    assert!(samples[2].1 > 0.0, "reached the top too soon to sample");

    // Held long enough, it reaches the top and the first character of the log.
    h.run_steps(30);
    button_event(&h, above, false);
    h.step();
    assert_eq!(log.painted_offset(&h), 0.0);
    let mut want: Vec<&str> = (0..37).map(|i| log.line(i)).collect();
    want.push(&log.line(37)[..11]);
    assert_eq!(copied(&mut h).as_deref(), Some(want.join("\n").as_str()));
}

/// As above, below the text: it scrolls to the end and the selection takes in the last line.
/// Tk's would also take in that line's newline, which the widget leaves out (see the report);
/// the comparison ignores it.
#[test]
fn a_drag_held_below_the_text_scrolls_down_and_extends_to_the_end() {
    let log = Log::full();
    let mut h = live(log.state.clone());
    wheel_to_top(&log, &mut h);
    // At the top, row 2 is line 2.
    let anchor = pos2(log.ch_x(11), log.row_y(2, 0.0));
    h.hover_at(anchor);
    h.step();
    button_event(&h, anchor, true);
    h.step();
    let below = pos2(log.ch_x(20), log.inner.bottom() as f32 + 6.0);
    h.hover_at(below);
    h.step();

    let samples = scan_samples(&log, &mut h);
    assert!(
        samples
            .windows(2)
            .all(|w| w[1].0 > w[0].0 && w[1].1 > w[0].1),
        "the scan stopped: (lines, offset) {samples:?}"
    );
    assert!(
        samples[2].1 < log.max_offset(),
        "reached the end too soon to sample"
    );

    h.run_steps(30);
    button_event(&h, below, false);
    h.step();
    assert_eq!(log.painted_offset(&h), log.max_offset());
    let n = log.state.log.len();
    let mut want = vec![&log.line(2)[11..]];
    want.extend((3..n).map(|i| log.line(i)));
    let got = copied(&mut h).unwrap();
    assert_eq!(got.trim_end_matches('\n'), want.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// The scrollbar.

/// Where CTkScrollbar draws the thumb (`draw_rounded_scrollbar`) after `_clicked` at `y`: the
/// pointer's fraction of the canvas, less half the thumb's length, clamped, mapped into the
/// travel left by the round ends. Returns its top and bottom.
fn thumb_for_pointer(log: &Log, y: f32) -> (f32, f32) {
    let canvas = log.bar();
    let len = log.view_h() / log.content_h();
    let value = (y - canvas.y as f32) / canvas.h as f32;
    let start = (value - len / 2.0).clamp(0.0, 1.0 - len);
    let r = canvas.w as f32 / 2.0;
    let travel = canvas.h as f32 - 2.0 * r;
    (
        canvas.y as f32 + travel * start,
        canvas.y as f32 + 2.0 * r + travel * (start + len),
    )
}

/// The pointer's `y` on the scrollbar that scrolls the text `offset` px from the top.
fn pointer_for_offset(log: &Log, offset: f32) -> f32 {
    let canvas = log.bar();
    let len = log.view_h() / log.content_h();
    canvas.y as f32 + (offset / log.content_h() + len / 2.0) * canvas.h as f32
}

/// The thumb sits where CTkScrollbar puts it for a pointer at `y`, and the text is scrolled by
/// `offset`.
fn assert_scrolled(log: &Log, h: &Harness<'static, Probe>, y: f32, offset: f32, what: &str) {
    assert_eq!(log.painted_offset(h), offset, "{what}: text offset");
    let (top, bottom) = log.painted_thumb(h);
    let (want_top, want_bottom) = thumb_for_pointer(log, y);
    assert!(
        (top - want_top).abs() < 0.5 && (bottom - want_bottom).abs() < 0.5,
        "{what}: thumb {top}..{bottom}, CTkScrollbar draws {want_top}..{want_bottom}"
    );
}

/// CTkScrollbar's `_clicked` on a press in the trough: centre the thumb on the pointer, clamped
/// at both ends, and `moveto` its start.
#[test]
fn a_press_on_the_trough_centres_the_thumb_on_the_pointer() {
    let log = Log::full();
    let canvas = log.bar();
    let x = canvas.rect().center().x;
    // Near the top (clamped), the middle at 120 px, and near the bottom (clamped).
    for (y, offset) in [
        (canvas.y as f32 + 1.0, 0.0),
        (pointer_for_offset(&log, 120.0), 120.0),
        (canvas.bottom() as f32 - 1.0, log.max_offset()),
    ] {
        let mut h = live(log.state.clone());
        if offset == log.max_offset() {
            wheel_to_top(&log, &mut h);
        }
        let p = pos2(x, y);
        h.hover_at(p);
        h.step();
        button_event(&h, p, true);
        h.step();
        assert_scrolled(&log, &h, y, offset, &format!("pressed at {y}"));
        button_event(&h, p, false);
        h.step();
        assert_scrolled(&log, &h, y, offset, &format!("released at {y}"));
    }
}

/// CTkScrollbar binds <B1-Motion> to `_clicked` too: the thumb follows the pointer's centre
/// while dragged, also beside or beyond the bar, and stops at both ends.
#[test]
fn dragging_the_thumb_keeps_it_centred_on_the_pointer() {
    let log = Log::full();
    let canvas = log.bar();
    let x = canvas.rect().center().x;
    let mut h = live(log.state.clone());
    // The thumb sits at the bottom; grab it in its middle.
    let (top, bottom) = log.painted_thumb(&h);
    let grab = pos2(x, (top + bottom) / 2.0);
    h.hover_at(grab);
    h.step();
    button_event(&h, grab, true);
    h.step();

    // Up to 80 px from the top, 30 px to the left of the bar.
    let y = pointer_for_offset(&log, 80.0);
    h.hover_at(pos2(x - 30.0, y));
    h.step();
    assert_scrolled(&log, &h, y, 80.0, "dragged to 80 px");

    // Above the log: the top.
    let y = canvas.y as f32 - 50.0;
    h.hover_at(pos2(x, y));
    h.step();
    assert_scrolled(&log, &h, y, 0.0, "dragged above");

    // Below it: the end.
    let y = canvas.bottom() as f32 + 50.0;
    h.hover_at(pos2(x, y));
    h.step();
    assert_scrolled(&log, &h, y, log.max_offset(), "dragged below");

    // Released, the pointer moves the thumb no more.
    button_event(&h, pos2(x, y), false);
    h.step();
    h.hover_at(pos2(x, pointer_for_offset(&log, 80.0)));
    h.step();
    assert_scrolled(&log, &h, y, log.max_offset(), "after the release");
}

// ---------------------------------------------------------------------------------------------
// The keyboard (`docs/behaviour.md`, "Changes from v2.3.2": "Keyboard access").

/// Tab stop 15 of `log-full` and of a longer log: the log, which overflows.
const LOG_STOP: usize = 15;

/// With the log focused through Tab, the arrows scroll one line, PageUp and PageDown the view
/// less one line, Home and End to the ends; past an end a key does nothing.
#[test]
fn the_focused_log_scrolls_with_the_keys() {
    let log = Log::long();
    let end = log.max_offset();
    let page = log.view_h() - LINE;
    assert!(2.0 * page <= end, "the log scrolls less than two pages");
    let mut h = harness(log.state.clone());
    tabs(&mut h, LOG_STOP);
    assert_eq!(log.painted_offset(&h), end);

    for (key, want) in [
        (Key::ArrowDown, end),
        (Key::ArrowUp, end - LINE),
        (Key::ArrowUp, end - 2.0 * LINE),
        (Key::ArrowDown, end - LINE),
        (Key::PageUp, end - LINE - page),
        (Key::PageUp, end - LINE - 2.0 * page),
        (Key::PageDown, end - LINE - page),
        (Key::Home, 0.0),
        (Key::ArrowUp, 0.0),
        (Key::PageUp, 0.0),
        (Key::ArrowDown, LINE),
        (Key::PageDown, LINE + page),
        (Key::End, end),
        (Key::PageDown, end),
    ] {
        press(&mut h, key);
        assert_eq!(log.painted_offset(&h), want, "after {key:?}");
    }
}

const SCROLL_KEYS: [Key; 6] = [
    Key::ArrowUp,
    Key::ArrowDown,
    Key::PageUp,
    Key::PageDown,
    Key::Home,
    Key::End,
];

/// The keys reach the log only while it is the focused tab stop, which it is only while it
/// overflows: unfocused, with another control focused, or not overflowing, they move nothing.
#[test]
fn the_keys_move_nothing_unless_the_log_is_focused() {
    // Nothing focused, at the end, where all but ArrowDown, PageDown and End would show.
    let log = Log::full();
    let mut h = harness(log.state.clone());
    for key in SCROLL_KEYS {
        press(&mut h, key);
        assert_eq!(
            log.painted_offset(&h),
            log.max_offset(),
            "{key:?}, nothing focused"
        );
    }

    // Launch focused, one stop after the log.
    tabs(&mut h, LOG_STOP + 1);
    for key in SCROLL_KEYS {
        press(&mut h, key);
        assert_eq!(
            log.painted_offset(&h),
            log.max_offset(),
            "{key:?}, Launch focused"
        );
    }

    // idle's log fits: at every stop, the keys leave it at the top.
    let log = Log::of(fixture("idle"));
    assert_eq!(log.max_offset(), 0.0);
    let mut h = harness(log.state.clone());
    for stop in 1..=20 {
        press(&mut h, Key::Tab);
        for key in SCROLL_KEYS {
            press(&mut h, key);
            assert_eq!(log.painted_offset(&h), 0.0, "idle, {key:?} at stop {stop}");
        }
    }
}
