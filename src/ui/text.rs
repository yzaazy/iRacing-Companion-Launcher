//! Text laid out the way Tk on Windows lays it out, painted with egui's rasterizer.
//!
//! Tk measures and draws through GDI, which advances the pen by whole device pixels: each
//! glyph's advance is its design advance at the device pixel size, rounded half up, and there is
//! no kerning. At 125 % display scaling a device pixel is 0.8 points, so the rounding happens in
//! device pixels and the result is converted back to points.
//! egui shapes with kerning and fractional advances, so a string comes out narrower or wider by a
//! few pixels. Here every character is its own layout section, shaped alone (so no kerning), and
//! each section's leading space moves the pen to the next whole-pixel GDI position.

use crate::ui::theme::Font;
use egui::text::{ByteIndex, LayoutJob, LayoutSection, TextFormat, TextWrapping};
use egui::{Color32, Galley, Painter, Pos2, pos2};
use std::sync::Arc;

/// GDI's advance of `c` in device pixels: the linear advance at the device size rounded half up,
/// with egui's linear advance in points. The epsilon keeps an exact half (8.5 px for `Y` in
/// Segoe UI 14 bold) from rounding down through float error.
fn advance(
    fonts: &mut egui::epaint::text::FontsView<'_>,
    font: &Font,
    c: char,
    pixels_per_point: f32,
) -> (f32, f32) {
    let linear = fonts.glyph_width(&font.id(), c);
    (linear, (linear * pixels_per_point + 1e-3).round())
}

/// The width Tk measures for one line of `text`, in points: whole device pixels, rounded up to
/// whole points so a label sized to it never clips its own text. At 1.0 the two are equal.
pub fn width(ctx: &egui::Context, text: &str, font: &Font) -> i32 {
    let ppp = ctx.pixels_per_point();
    let device: f32 =
        ctx.fonts_mut(|fonts| text.chars().map(|c| advance(fonts, font, c, ppp).1).sum());
    (device / ppp - 1e-3).ceil() as i32
}

/// The widest line of `text`, and its number of lines.
pub(super) fn block_size(ctx: &egui::Context, text: &str, font: &Font) -> (i32, i32) {
    let lines: Vec<&str> = text.split('\n').collect();
    let w = lines.iter().map(|l| width(ctx, l, font)).max().unwrap_or(0);
    (w, lines.len() as i32)
}

/// A styled run of text to lay out.
pub struct Run<'a> {
    pub text: &'a str,
    pub color: Color32,
}

/// Lay out `runs` at GDI positions with Tk's line pitch, optionally wrapping at any character
/// once a line would exceed `wrap_width` (Tk's `wrap=char`).
pub fn layout(
    ctx: &egui::Context,
    runs: &[Run<'_>],
    font: &Font,
    wrap_width: Option<f32>,
) -> Arc<Galley> {
    let mut job = LayoutJob {
        wrap: TextWrapping {
            max_width: wrap_width.unwrap_or(f32::INFINITY),
            break_anywhere: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let ppp = ctx.pixels_per_point();
    ctx.fonts_mut(|fonts| {
        // Correction carried to the next character: GDI advance minus egui's advance, in points.
        let mut carry = 0.0;
        for run in runs {
            let format = TextFormat {
                font_id: font.id(),
                color: run.color,
                line_height: Some(font.linespace() as f32),
                ..Default::default()
            };
            for c in run.text.chars() {
                let start = job.text.len();
                job.text.push(c);
                // A combining mark joins its base's section, so the shaper sees the cluster and
                // places the mark on it instead of on a dotted circle.
                if is_combining(c)
                    && let Some(last) = job.sections.last_mut()
                    && last.format == format
                    && !job.text[last.byte_range.start.0..start].ends_with('\n')
                {
                    last.byte_range.end = ByteIndex(job.text.len());
                    let (linear, gdi) = advance(fonts, font, c, ppp);
                    carry += gdi / ppp - linear;
                    continue;
                }
                let leading_space = if c == '\n' { 0.0 } else { carry };
                job.sections.push(LayoutSection {
                    leading_space,
                    byte_range: ByteIndex(start)..ByteIndex(job.text.len()),
                    format: format.clone(),
                });
                carry = if c == '\n' {
                    0.0
                } else {
                    let (linear, gdi) = advance(fonts, font, c, ppp);
                    gdi / ppp - linear
                };
            }
        }
        fonts.layout_job(job)
    })
}

/// Combining marks and joiners that belong to the character before them: the scripts a Windows
/// user name or path is written in (Latin diacritics, Cyrillic, Hebrew, Arabic, the Indic
/// scripts, Sinhala, Thai, Lao, Tibetan, Myanmar, Hangul jamo, kana voicing marks), and
/// variation selectors. epaint's own table is private.
fn is_combining(c: char) -> bool {
    let c = c as u32;
    // The Indic blocks from Devanagari to Malayalam share one layout per 128 code points.
    if (0x0900..0x0D80).contains(&c) {
        let o = c & 0x7F;
        return matches!(o, 0x00..=0x03 | 0x3A..=0x3C | 0x3E..=0x4F | 0x51..=0x57 | 0x62 | 0x63);
    }
    matches!(
        c,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x05BF
            | 0x05C1..=0x05C2
            | 0x05C4..=0x05C5
            | 0x05C7
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06DC
            | 0x06DF..=0x06E4
            | 0x06E7..=0x06E8
            | 0x06EA..=0x06ED
            | 0x0D81..=0x0D83
            | 0x0DCA..=0x0DDF
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x0EB1
            | 0x0EB4..=0x0EBC
            | 0x0EC8..=0x0ECD
            | 0x0F18..=0x0F19
            | 0x0F35
            | 0x0F37
            | 0x0F39
            | 0x0F71..=0x0F84
            | 0x102B..=0x103E
            | 0x1160..=0x11FF
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x200C..=0x200D
            | 0x20D0..=0x20FF
            | 0x302A..=0x302F
            | 0x3099..=0x309A
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0xE0100..=0xE01EF
    )
}

/// The distance from the galley's top to its first baseline.
pub(super) fn baseline_offset(galley: &Galley, font: &Font) -> f32 {
    galley
        .rows
        .iter()
        .flat_map(|row| row.glyphs.first())
        .next()
        .map_or(font.ascent as f32, |g| g.pos.y)
}

/// Paint a laid-out block so its first line's pen starts at `left` and its baseline sits on
/// `baseline`.
fn paint(painter: &Painter, galley: Arc<Galley>, font: &Font, left: i32, baseline: i32) {
    let offset = baseline_offset(&galley, font);
    let pos: Pos2 = pos2(left as f32, baseline as f32 - offset);
    painter.galley(pos, galley, Color32::PLACEHOLDER);
}

/// Lay out and paint one block of text with its first baseline at `top + ascent`.
pub(super) fn paint_block(
    ctx: &egui::Context,
    painter: &Painter,
    text: &str,
    font: &Font,
    color: Color32,
    left: i32,
    top: i32,
) {
    let galley = layout(ctx, &[Run { text, color }], font, None);
    paint(painter, galley, font, left, top + font.ascent);
}
