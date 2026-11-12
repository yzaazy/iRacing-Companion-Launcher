//! Ports the activity log's CTkTextbox (`ui/sections/log_section.py`): a read-only Tk Text
//! widget that wraps by character, scrolls to the end on every new line, scrolls 40 px per wheel
//! notch, shows a scrollbar only when needed, and lets the user select and copy text.
//!
//! The log is unbounded, as in Python, so nothing here costs more per frame as it grows. Every
//! line ends in a newline, so lines wrap independently: each is laid out alone, once per text
//! width, and only its number of rows is kept. A frame lays out only the lines in view (egui's
//! galley cache keeps those between frames), and finds the first of them by binary search.
//! The log is append-only (`ActivityLogger` never edits or removes a line), so a frame checks
//! that by its length and its first and last lines and lays out only lines it has not seen.

use crate::model::{LogLine, ViewState};
use crate::ui::input::{Controls, Ring};
use crate::ui::layout::LogText;
use crate::ui::pack::Bounds;
use crate::ui::text::{self, Run};
use crate::ui::theme;
use egui::{
    Color32, CursorIcon, Galley, Id, Key, PointerButton, Rect, Role, Sense, Ui, WidgetInfo, pos2,
    vec2,
};
use std::sync::{Arc, Mutex};

/// Tk's wheel binding on Windows scrolls `%D / 3` pixels, rounded away from zero, where a notch
/// is 120: 40 px per notch.
const WHEEL_NOTCH: f32 = 120.0;
const WHEEL_DIVISOR: f32 = 3.0;

fn line_height() -> f32 {
    theme::LOG_FONT.linespace() as f32
}

/// One line as Tk inserts it: its text and a newline, in its level's colour, wrapped by
/// character at `width`.
fn line_galley(ctx: &egui::Context, line: &LogLine, width: f32) -> Arc<Galley> {
    let text = format!("{}\n", line.text);
    let run = Run {
        text: &text,
        color: theme::log_color(line.level),
    };
    text::layout(ctx, &[run], &theme::LOG_FONT, Some(width))
}

/// The rows each line wraps to at one width, and where each line starts.
#[derive(Default)]
struct Wrap {
    width: f32,
    /// `starts[i]` is the first row of line `i`; one more entry holds the total.
    starts: Vec<u32>,
}

impl Wrap {
    fn total(&self) -> u32 {
        self.starts.last().copied().unwrap_or(0)
    }
}

/// What the log has laid out so far.
#[derive(Default)]
struct Cache {
    pixels_per_point: f32,
    /// The log these rows belong to: its length and its first and last lines.
    len: usize,
    first: Option<LogLine>,
    last: Option<LogLine>,
    wraps: Vec<Wrap>,
}

type SharedCache = Arc<Mutex<Cache>>;

fn cache(ctx: &egui::Context) -> SharedCache {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<SharedCache>(Id::unique("launcher-log-cache"))
            .clone()
    })
}

impl Cache {
    /// Forget everything unless `log` extends the log seen so far.
    fn sync(&mut self, ctx: &egui::Context, log: &[LogLine]) {
        let ppp = ctx.pixels_per_point();
        let extends = self.pixels_per_point == ppp
            && log.len() >= self.len
            && (self.len == 0
                || log.first() == self.first.as_ref()
                    && log.get(self.len - 1) == self.last.as_ref());
        if !extends {
            *self = Self {
                pixels_per_point: ppp,
                ..Default::default()
            };
        }
        self.len = log.len();
        self.first = log.first().cloned();
        self.last = log.last().cloned();
    }

    /// The rows of every line at `width`, laying out only the lines not seen at it yet.
    fn wrap(&mut self, ctx: &egui::Context, log: &[LogLine], width: f32) -> &Wrap {
        let i = match self.wraps.iter().position(|w| w.width == width) {
            Some(i) => i,
            None => {
                self.wraps.push(Wrap {
                    width,
                    starts: vec![0],
                });
                self.wraps.len() - 1
            }
        };
        let wrap = &mut self.wraps[i];
        for line in &log[wrap.starts.len() - 1..] {
            // The galley ends with the empty row after the newline; that row belongs to the next
            // line.
            let rows = line_galley(ctx, line, width).rows.len().saturating_sub(1) as u32;
            let total = wrap.total();
            wrap.starts.push(total + rows);
        }
        wrap
    }
}

/// The height of the whole log at text width `width`: every line's rows and the empty line
/// after the last newline.
pub fn content_height(ctx: &egui::Context, state: &ViewState, width: f32) -> f32 {
    let cache = cache(ctx);
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.sync(ctx, &state.log);
    (cache.wrap(ctx, &state.log, width).total() + 1) as f32 * line_height()
}

/// The text of every row the log shows at text width `width`, as Tk wraps it: each line's rows,
/// then the empty row after the last newline.
pub fn row_texts(ctx: &egui::Context, state: &ViewState, width: f32) -> Vec<String> {
    let mut rows: Vec<String> = state
        .log
        .iter()
        .flat_map(|line| {
            let galley = line_galley(ctx, line, width);
            let n = galley.rows.len().saturating_sub(1);
            galley.rows[..n]
                .iter()
                .map(|r| r.row.text())
                .collect::<Vec<_>>()
        })
        .collect();
    rows.push(String::new());
    rows
}

/// A position in the log: line and character, `ch == text length` being the newline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct At {
    line: usize,
    ch: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Granularity {
    Char,
    Word,
    Line,
}

/// Tk's `sel` tag, and how the pointer is extending it.
#[derive(Debug, Clone, Copy)]
struct Selection {
    anchor: At,
    head: At,
    dragging: bool,
    granularity: Granularity,
}

impl Selection {
    /// The selected span, `end` exclusive.
    fn span(&self) -> (At, At) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }
}

/// Remembered between frames.
#[derive(Clone, Copy, Default)]
struct Memory {
    /// Pixels scrolled from the top.
    offset: f32,
    /// The log length and last line seen, to scroll to the end when they change.
    seen: Option<(usize, u64)>,
    selection: Option<Selection>,
}

fn last_line_hash(state: &ViewState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Some(last) = state.log.last() {
        last.text.hash(&mut h);
    }
    h.finish()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The view's geometry this frame.
struct View<'a> {
    ctx: egui::Context,
    log: &'a [LogLine],
    inner: Bounds,
    offset: f32,
    starts: &'a [u32],
}

impl View<'_> {
    fn line_top(&self, line: usize) -> f32 {
        self.inner.y as f32 + self.starts[line] as f32 * line_height() - self.offset
    }

    fn galley(&self, line: usize) -> Arc<Galley> {
        line_galley(&self.ctx, &self.log[line], self.inner.w as f32)
    }

    /// The log position under `pos`, clamped into the log.
    fn hit(&self, pos: egui::Pos2) -> Option<At> {
        let n = self.log.len();
        if n == 0 {
            return None;
        }
        let row = ((pos.y - self.inner.y as f32 + self.offset) / line_height()).floor();
        if row < 0.0 {
            return Some(At { line: 0, ch: 0 });
        }
        let row = row as u32;
        let line = self.starts.partition_point(|&s| s <= row).saturating_sub(1);
        if line >= n {
            return Some(At {
                line: n - 1,
                ch: self.log[n - 1].text.chars().count(),
            });
        }
        let galley = self.galley(line);
        let mut row_in_line = (row - self.starts[line]) as usize;
        let rows = galley.rows.len() - 1;
        row_in_line = row_in_line.min(rows.saturating_sub(1));
        let before: usize = galley.rows[..row_in_line]
            .iter()
            .map(|r| r.row.glyphs.len())
            .sum();
        let placed = &galley.rows[row_in_line];
        let x = pos.x - self.inner.x as f32 - placed.pos.x;
        let mut ch = before + placed.row.char_at(x).0;
        // Past the end of a wrapped row's last glyph is still on that row, not the next.
        if row_in_line + 1 < rows && ch == before + placed.row.glyphs.len() {
            ch -= 1;
        }
        Some(At { line, ch })
    }

    fn word_at(&self, at: At) -> (At, At) {
        let chars: Vec<char> = self.log[at.line].text.chars().collect();
        let word = |c: Option<&char>| c.is_some_and(|&c| is_word_char(c));
        let (mut a, mut b) = (at.ch, at.ch);
        if word(chars.get(at.ch)) {
            while a > 0 && word(chars.get(a - 1)) {
                a -= 1;
            }
            while word(chars.get(b)) {
                b += 1;
            }
        } else {
            b = (at.ch + 1).min(chars.len() + 1);
        }
        (
            At {
                line: at.line,
                ch: a,
            },
            At {
                line: at.line,
                ch: b,
            },
        )
    }

    fn line_span(&self, line: usize) -> (At, At) {
        // A whole line, its newline included.
        let len = self.log[line].text.chars().count();
        (At { line, ch: 0 }, At { line, ch: len + 1 })
    }
}

/// Text of the selected span, lines joined by the newlines between them, as Tk copies `sel`.
fn selected_text(log: &[LogLine], (a, b): (At, At)) -> String {
    let mut out = String::new();
    for (line, entry) in log.iter().enumerate().take(b.line + 1).skip(a.line) {
        let chars: Vec<char> = entry.text.chars().collect();
        let start = if line == a.line { a.ch } else { 0 };
        let end = if line == b.line {
            b.ch
        } else {
            chars.len() + 1
        };
        out.extend(chars.iter().take(end.min(chars.len())).skip(start));
        if end > chars.len() && start <= chars.len() {
            out.push('\n');
        }
    }
    out
}

/// What the log draws inside its box.
pub struct LogGeom<'a> {
    pub log_box: Bounds,
    pub text: &'a LogText,
}

/// How far the log scrolls this frame.
#[derive(Clone, Copy)]
struct Extent {
    line_h: f32,
    content_h: f32,
    view_h: f32,
    max_offset: f32,
}

/// Paint the log box, its visible lines, the selection and the scrollbar, and handle the
/// pointer, the wheel, copying and (when the log has focus) the keyboard.
pub fn show(ui: &mut Ui, controls: &mut Controls, geom: LogGeom<'_>, state: &ViewState) {
    let ctx = ui.ctx().clone();
    let id = ui.scope_id().with("activity_log");
    let memory_id = id.with("memory");
    let mut memory: Memory = ctx.data(|d| d.get_temp(memory_id)).unwrap_or_default();

    crate::ui::widgets::rounded(
        ui.painter(),
        geom.log_box.painted(),
        theme::LOG_RADIUS,
        theme::LOG_BG,
    );

    let inner = geom.text.inner;
    let cache = cache(&ctx);
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.sync(&ctx, &state.log);
    let wrap = cache.wrap(&ctx, &state.log, inner.w as f32);
    let lh = line_height();
    let content_h = (wrap.total() + 1) as f32 * lh;
    let view_h = inner.h as f32;
    let extent = Extent {
        line_h: lh,
        content_h,
        view_h,
        max_offset: (content_h - view_h).max(0.0),
    };

    follow(&mut memory, state, extent);
    wheel(ui, geom.log_box, extent, &mut memory);
    keyboard(ui, controls, id, extent, &mut memory);
    if let Some(bar) = geom.text.scrollbar {
        drag_scrollbar(ui, id, bar, extent, &mut memory);
    }
    let (response, pointer) = sense_text(ui, id, geom.text, extent, &mut memory);

    memory.offset = memory.offset.clamp(0.0, extent.max_offset).round();
    let view = View {
        ctx: ctx.clone(),
        log: &state.log,
        inner,
        offset: memory.offset,
        starts: &wrap.starts,
    };

    select(ui, &view, &response, pointer, &mut memory);
    let selection = memory.selection.map(|s| s.span()).filter(|(a, b)| a != b);
    if let Some(span) = selection
        && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)))
    {
        ctx.copy_text(selected_text(&state.log, span));
    }

    paint_lines(ui, &view, selection);
    if let Some(bar) = geom.text.scrollbar {
        paint_scrollbar(ui, controls, id, bar, geom.log_box, extent, memory.offset);
    }

    drop(cache);
    ctx.data_mut(|d| d.insert_temp(memory_id, memory));
}

/// ActivityLogger calls see("end") after every line; a selection past the end of a shorter log
/// is dropped.
fn follow(memory: &mut Memory, state: &ViewState, extent: Extent) {
    let seen = Some((state.log.len(), last_line_hash(state)));
    if memory.seen != seen {
        memory.seen = seen;
        memory.offset = extent.max_offset;
    }
    if memory
        .selection
        .is_some_and(|s| s.span().1.line >= state.log.len().max(1))
    {
        memory.selection = None;
    }
}

/// The wheel, over the text or the scrollbar.
fn wheel(ui: &Ui, log_box: Bounds, extent: Extent, memory: &mut Memory) {
    let log_rect = log_box.rect();
    if ui.rect_contains_pointer(log_rect) {
        let delta: f32 = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        ..
                    } if !modifiers.shift && !modifiers.ctrl && !modifiers.command => {
                        Some(match unit {
                            egui::MouseWheelUnit::Line => {
                                let d = delta.y * WHEEL_NOTCH;
                                d.signum() * (d.abs() / WHEEL_DIVISOR).ceil()
                            }
                            egui::MouseWheelUnit::Point => delta.y,
                            egui::MouseWheelUnit::Page => delta.y * extent.view_h,
                        })
                    }
                    _ => None,
                })
                .sum()
        });
        memory.offset -= delta;
    }
}

/// The keyboard, while the log is the focused tab stop and can scroll.
fn keyboard(ui: &Ui, controls: &Controls, id: Id, extent: Extent, memory: &mut Memory) {
    let Extent {
        line_h: lh,
        content_h,
        view_h,
        ..
    } = extent;
    if controls.focused() == Some(id) {
        let page = (view_h - lh).max(lh);
        ui.input(|i| {
            for (key, by) in [
                (Key::ArrowUp, -lh),
                (Key::ArrowDown, lh),
                (Key::PageUp, -page),
                (Key::PageDown, page),
                (Key::Home, -content_h),
                (Key::End, content_h),
            ] {
                if i.key_pressed(key) {
                    memory.offset += by;
                }
            }
        });
    }
}

/// The scrollbar: press or drag anywhere on it to centre the thumb on the pointer, as
/// CTkScrollbar does.
fn drag_scrollbar(ui: &Ui, id: Id, bar: Bounds, extent: Extent, memory: &mut Memory) {
    let Extent {
        content_h, view_h, ..
    } = extent;
    let response = ui.interact(bar.rect(), id.with("scrollbar"), Sense::CLICK | Sense::DRAG);
    response.widget_info(|| WidgetInfo::labeled(Role::ScrollBar, true, "Activity Log"));
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.is_pointer_button_down_on())
    {
        let canvas = bar.painted();
        let len = (view_h / content_h).min(1.0);
        let value = (pointer.y - canvas.y as f32) / canvas.h as f32;
        let start = (value - len / 2.0).clamp(0.0, 1.0 - len);
        memory.offset = start * content_h;
    }
}

/// The pointer on the Text widget: selecting happens anywhere on it, and the I-beam shows over
/// all of it, as Tk's `xterm` cursor does. Returns its response and the pointer.
fn sense_text(
    ui: &Ui,
    id: Id,
    text: &LogText,
    extent: Extent,
    memory: &mut Memory,
) -> (egui::Response, Option<egui::Pos2>) {
    let (inner, lh) = (text.inner, extent.line_h);
    let text_rect = text.text.rect();
    let response = ui.interact(text_rect, id.with("text"), Sense::CLICK | Sense::DRAG);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, "Activity Log"));
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let dragging = memory.selection.is_some_and(|s| s.dragging);
    if response.hovered() || dragging {
        ui.ctx().set_cursor_icon(CursorIcon::Text);
    }
    // Tk scans while a drag is held above or below the text.
    if dragging && let Some(p) = pointer {
        if p.y < inner.y as f32 {
            memory.offset -= lh;
        } else if p.y > inner.bottom() as f32 {
            memory.offset += lh;
        }
        ui.ctx().request_repaint_after_secs(0.05);
    }
    (response, pointer)
}

/// Selecting: press, drag, double-click a word, triple-click a line.
fn select(
    ui: &Ui,
    view: &View<'_>,
    response: &egui::Response,
    pointer: Option<egui::Pos2>,
    memory: &mut Memory,
) {
    let (pressed, down, clicks) = ui.input(|i| {
        let clicks = if i.pointer.button_triple_clicked(PointerButton::Primary) {
            3
        } else if i.pointer.button_double_clicked(PointerButton::Primary) {
            2
        } else {
            1
        };
        (
            i.pointer.button_pressed(PointerButton::Primary),
            i.pointer.primary_down(),
            clicks,
        )
    });
    if let Some(p) = pointer {
        if pressed && response.hovered() {
            memory.selection = view.hit(p).map(|at| Selection {
                anchor: at,
                head: at,
                dragging: true,
                granularity: Granularity::Char,
            });
        }
        if clicks > 1
            && response.hovered()
            && let Some(at) = view.hit(p)
        {
            let ((a, b), granularity) = if clicks == 3 {
                (view.line_span(at.line), Granularity::Line)
            } else {
                (view.word_at(at), Granularity::Word)
            };
            memory.selection = Some(Selection {
                anchor: a,
                head: b,
                dragging: false,
                granularity,
            });
        }
        if let Some(sel) = &mut memory.selection
            && sel.dragging
            && sel.granularity == Granularity::Char
            && down
            && let Some(at) = view.hit(p)
        {
            sel.head = at;
        }
    }
    if !down && let Some(sel) = &mut memory.selection {
        sel.dragging = false;
    }
}

/// The visible lines, with the selection behind them.
fn paint_lines(ui: &Ui, view: &View<'_>, selection: Option<(At, At)>) {
    let (inner, lh) = (view.inner, line_height());
    let painter = ui
        .painter()
        .with_clip_rect(inner.rect().intersect(ui.clip_rect()));
    let first_row = (view.offset / lh).floor() as u32;
    let first = view
        .starts
        .partition_point(|&s| s <= first_row)
        .saturating_sub(1);
    let font = &theme::LOG_FONT;
    for line in first..view.log.len() {
        let top = view.line_top(line);
        if top >= inner.bottom() as f32 {
            break;
        }
        let galley = view.galley(line);
        if let Some((a, b)) = selection
            && a.line <= line
            && line <= b.line
        {
            paint_selection(&painter, view, &galley, line, (a, b), top);
        }
        let shift = font.ascent as f32 - text::baseline_offset(&galley, font);
        painter.galley(
            pos2(inner.x as f32, top + shift),
            galley,
            Color32::PLACEHOLDER,
        );
    }
}

/// The scrollbar thumb; with something to scroll, the log is a tab stop for the keyboard.
fn paint_scrollbar(
    ui: &Ui,
    controls: &mut Controls,
    id: Id,
    bar: Bounds,
    log_box: Bounds,
    extent: Extent,
    offset: f32,
) {
    let Extent {
        content_h, view_h, ..
    } = extent;
    let start = (offset / content_h).clamp(0.0, 1.0);
    let end = ((offset + view_h) / content_h).clamp(0.0, 1.0);
    let color = if ui.rect_contains_pointer(bar.rect()) {
        theme::SCROLL_THUMB_HOVER
    } else {
        theme::SCROLL_THUMB
    };
    crate::ui::widgets::scrollbar_thumb(ui.painter(), bar, start, end, color);
    controls.stop(
        id,
        Ring::Rounded {
            rect: log_box.painted(),
            radius: theme::LOG_RADIUS,
        },
    );
}

/// Tk's selection highlight on one line: behind the selected glyphs of each row, and on to the
/// right edge where the selection takes in the line's newline.
fn paint_selection(
    painter: &egui::Painter,
    view: &View<'_>,
    galley: &Galley,
    line: usize,
    (a, b): (At, At),
    top: f32,
) {
    let len = view.log[line].text.chars().count();
    let start = if line == a.line { a.ch } else { 0 };
    let end = if line == b.line { b.ch } else { len + 1 };
    let rows = galley.rows.len() - 1;
    let mut row_start = 0;
    for (r, placed) in galley.rows[..rows].iter().enumerate() {
        let n = placed.row.glyphs.len();
        let (s, e) = (start.max(row_start), end.min(row_start + n));
        let newline = r + 1 == rows && end > len;
        if s < e || newline && start <= row_start + n {
            let x0 = placed
                .row
                .x_offset(egui::text::CharIndex(s.saturating_sub(row_start).min(n)));
            let x1 = if newline {
                view.inner.w as f32
            } else {
                placed.row.x_offset(egui::text::CharIndex(e - row_start))
            };
            let y = top + r as f32 * line_height();
            let left = view.inner.x as f32 + placed.pos.x;
            painter.rect_filled(
                Rect::from_min_size(pos2(left + x0, y), vec2(x1 - x0, line_height())),
                0.0,
                theme::LOG_SELECTION,
            );
        }
        row_start += n;
    }
}
