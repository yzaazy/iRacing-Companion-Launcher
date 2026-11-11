//! Ports `ui/sections/` and `ui/widgets/` entry point: draws the whole window from a `ViewState`.

pub mod layout;
pub mod sections;
pub mod theme;
pub mod widgets;

use crate::contract::{UiAction, ViewState};

/// Pure function of `state`; returns what the user did this frame.
pub fn draw(ctx: &egui::Context, state: &ViewState) -> Vec<UiAction> {
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.rect_filled(ctx.content_rect(), 0.0, theme::BG_WINDOW);
    Vec::new()
}
