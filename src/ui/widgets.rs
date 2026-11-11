//! Ports how CustomTkinter paints the widgets of `ui/widgets/status_card.py` and
//! `ui/widgets/game_card.py`: rounded frames, checkbox, radio button, status dot and buttons.
//! Every shape is painted at a rect from `layout`, in colours from `theme`.

use crate::ui::layout::Bounds;
use crate::ui::text;
use crate::ui::theme::{self, ButtonColors, Font};
use egui::{Color32, CornerRadius, Painter, Shape, Stroke, pos2};

fn radius(r: f32) -> CornerRadius {
    CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

/// A CTk rounded rectangle without border.
pub fn rounded(painter: &Painter, painted: Bounds, corner_radius: f32, fill: Color32) {
    painter.rect_filled(painted.rect(), radius(corner_radius), fill);
}

/// A CTk frame with a border: the outer shape in the border colour, the inner one inset by the
/// border width with the corner radius reduced by the same amount.
pub fn bordered(
    painter: &Painter,
    painted: Bounds,
    corner_radius: f32,
    border_width: f32,
    border: Color32,
    fill: Color32,
) {
    rounded(painter, painted, corner_radius, border);
    let bw = border_width as i32;
    rounded(
        painter,
        painted.inset(bw),
        corner_radius - border_width,
        fill,
    );
}

/// The checkmark of a checked CTkCheckBox, in box coordinates of the 24 px box.
const CHECKMARK: [(f32, f32); 3] = [(6.2, 13.5), (10.0, 16.6), (17.7, 7.0)];
const CHECKMARK_WIDTH: f32 = 2.9;

/// A CTkCheckBox square: filled blue with a checkmark when checked, otherwise a grey 3 px ring
/// around the card colour (or the hover blue while hovered).
pub fn checkbox(painter: &Painter, b: Bounds, checked: bool, hovered: bool, bg: Color32) {
    if checked {
        rounded(painter, b, theme::CHECKBOX_RADIUS, theme::CHECK_FILL);
        let points = CHECKMARK
            .iter()
            .map(|&(x, y)| pos2(b.x as f32 + x, b.y as f32 + y))
            .collect();
        painter.add(Shape::line(
            points,
            Stroke::new(CHECKMARK_WIDTH, theme::CHECKMARK),
        ));
    } else {
        let inner = if hovered { theme::CHECK_HOVER } else { bg };
        bordered(
            painter,
            b,
            theme::CHECKBOX_RADIUS,
            theme::CHECKBOX_BORDER_WIDTH,
            theme::CHECK_BORDER,
            inner,
        );
    }
}

/// A CTkRadioButton circle: a ring 3 px wide (grey) or 6 px (blue, checked) around the card colour.
pub fn radio(painter: &Painter, b: Bounds, checked: bool, hovered: bool, bg: Color32) {
    let (ring, width) = if checked {
        (theme::RADIO_FILL, theme::RADIO_BORDER_CHECKED)
    } else {
        (theme::RADIO_BORDER, theme::RADIO_BORDER_UNCHECKED)
    };
    let ring = if hovered { theme::RADIO_HOVER } else { ring };
    let r = b.w as f32 / 2.0;
    let centre = b.rect().center();
    painter.circle_filled(centre, r, ring);
    painter.circle_filled(centre, r - width, bg);
}

/// Radius of the `●` glyph's disk in Segoe UI 40 px, and how far its centre sits above the
/// baseline (`ui-spec.json` `notes.status_dot`).
const DOT_RADIUS: f32 = 8.5;
const DOT_CENTRE_ABOVE_BASELINE: f32 = 11.5;

/// The status dot, painted as the disk the `●` glyph draws in its label.
pub fn dot(painter: &Painter, label: Bounds, color: Color32) {
    if !label.is_mapped() {
        return;
    }
    let font = &theme::DOT_FONT;
    let top = label.y + (label.h - font.linespace()) / 2;
    let baseline = (top + font.ascent) as f32;
    let centre = pos2(
        label.x as f32 + label.w as f32 / 2.0,
        baseline - DOT_CENTRE_ABOVE_BASELINE,
    );
    painter.circle_filled(centre, DOT_RADIUS, color);
}

/// How a button is drawn this frame.
pub struct ButtonLook<'a> {
    pub rect: Bounds,
    /// The Tk label holding the text, from `layout::button_label`.
    pub label: Bounds,
    pub text: &'a str,
    pub font: &'a Font,
    pub corner_radius: f32,
    pub colors: ButtonColors,
    pub enabled: bool,
    pub hovered: bool,
}

/// A CTkButton: the hover colour replaces the fill while hovered; a disabled button keeps its
/// fill and greys its text.
pub fn button(ctx: &egui::Context, painter: &Painter, look: &ButtonLook<'_>) {
    let fill = if look.enabled && look.hovered {
        look.colors.hover
    } else {
        look.colors.fill
    };
    let painted = look.rect.painted();
    rounded(painter, painted, look.corner_radius, fill);
    if look.corner_radius >= theme::ACTION_BUTTON_RADIUS && look.rect.w % 2 == 1 {
        corner_overhang(painter, painted, look.corner_radius, fill);
    }
    let color = if look.enabled {
        look.colors.text
    } else {
        theme::BUTTON_TEXT_DISABLED
    };
    // The Tk label pads its text by 1 px on every side.
    text::paint_block(
        ctx,
        painter,
        look.text,
        look.font,
        color,
        look.label.x + 1,
        look.label.y + 1,
    );
}

/// The right-hand corner glyphs' overhang into the unpainted last column of an odd-width canvas,
/// rasterised by area coverage: a box that thin is below what egui's feathering can draw.
fn corner_overhang(painter: &Painter, painted: Bounds, corner_radius: f32, fill: Color32) {
    let x = painted.right();
    let mut mesh = egui::Mesh::default();
    for cy in [
        painted.y as f32 + corner_radius,
        painted.bottom() as f32 - corner_radius,
    ] {
        let (top, bottom) = (
            cy - theme::ACTION_CORNER_OVERHANG_ABOVE,
            cy + theme::ACTION_CORNER_OVERHANG_BELOW,
        );
        for row in top.floor() as i32..bottom.ceil() as i32 {
            let covered = (bottom.min(row as f32 + 1.0) - top.max(row as f32)).max(0.0);
            mesh.add_colored_rect(
                Bounds::new(x, row, 1, 1).rect(),
                fill.gamma_multiply(covered * theme::ACTION_CORNER_OVERHANG),
            );
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// The log's CTkScrollbar thumb for the visible fraction `start..end` of the content: a pill as
/// wide as the bar, its ends rounded by half the width, on the even-floored canvas.
pub fn scrollbar_thumb(painter: &Painter, bar: Bounds, start: f32, end: f32, color: Color32) {
    let canvas = bar.painted();
    let r = canvas.w as f32 / 2.0;
    let travel = canvas.h as f32 - 2.0 * r;
    let y0 = canvas.y as f32 + travel * start;
    let y1 = canvas.y as f32 + 2.0 * r + travel * end;
    let rect = egui::Rect::from_min_max(
        pos2(canvas.x as f32, y0),
        pos2(canvas.x as f32 + canvas.w as f32, y1),
    );
    painter.rect_filled(rect, radius(r), color);
}
