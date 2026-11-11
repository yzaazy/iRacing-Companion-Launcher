//! Text laid out the way Tk on Windows lays it out, painted with egui's rasterizer.
//!
//! Tk measures and draws through GDI, which advances the pen by whole pixels: each glyph's
//! advance is its design advance at the pixel size, rounded half up, and there is no kerning.
//! egui shapes with kerning and fractional advances, so a string comes out narrower or wider by a
//! few pixels. Here every character is its own layout section, shaped alone (so no kerning), and
//! each section's leading space moves the pen to the next whole-pixel GDI position.

use crate::ui::theme::Font;
use egui::text::{ByteIndex, LayoutJob, LayoutSection, TextFormat, TextWrapping};
use egui::{Color32, Galley, Painter, Pos2, pos2};
use std::sync::Arc;

/// GDI's advance of `c`: the linear advance rounded half up. The epsilon keeps an exact half
/// (8.5 px for `Y` in Segoe UI 14 bold) from rounding down through float error.
fn advance(fonts: &mut egui::epaint::text::FontsView<'_>, font: &Font, c: char) -> (f32, i32) {
    let linear = fonts.glyph_width(&font.id(), c);
    (linear, (linear + 1e-3).round() as i32)
}

/// The width Tk measures for one line of `text`.
pub fn width(ctx: &egui::Context, text: &str, font: &Font) -> i32 {
    ctx.fonts_mut(|fonts| text.chars().map(|c| advance(fonts, font, c).1).sum())
}

/// The widest line of `text`, and its number of lines.
pub fn block_size(ctx: &egui::Context, text: &str, font: &Font) -> (i32, i32) {
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
    ctx.fonts_mut(|fonts| {
        // Correction carried to the next character: GDI advance minus egui's advance.
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
                let leading_space = if c == '\n' { 0.0 } else { carry };
                job.sections.push(LayoutSection {
                    leading_space,
                    byte_range: ByteIndex(start)..ByteIndex(job.text.len()),
                    format: format.clone(),
                });
                carry = if c == '\n' {
                    0.0
                } else {
                    let (linear, gdi) = advance(fonts, font, c);
                    gdi as f32 - linear
                };
            }
        }
        fonts.layout_job(job)
    })
}

/// The distance from the galley's top to its first baseline.
pub fn baseline_offset(galley: &Galley, font: &Font) -> f32 {
    galley
        .rows
        .iter()
        .flat_map(|row| row.glyphs.first())
        .next()
        .map_or(font.ascent as f32, |g| g.pos.y)
}

/// Paint a laid-out block so its first line's pen starts at `left` and its baseline sits on
/// `baseline`.
pub fn paint(painter: &Painter, galley: Arc<Galley>, font: &Font, left: i32, baseline: i32) {
    let offset = baseline_offset(&galley, font);
    let pos: Pos2 = pos2(left as f32, baseline as f32 - offset);
    painter.galley(pos, galley, Color32::PLACEHOLDER);
}

/// Lay out and paint one block of text with its first baseline at `top + ascent`.
pub fn paint_block(
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
