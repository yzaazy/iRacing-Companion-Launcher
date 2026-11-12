//! Ports `ui/sections/` and `ui/widgets/` entry point: draws the whole window from a `ViewState`.

pub mod fonts;
pub mod input;
pub mod layout;
pub mod log;
pub mod pack;
pub mod sections;
pub mod text;
pub mod theme;
pub mod widgets;

use crate::model::{UiAction, ViewState};

/// Pure function of `state`; returns what the user did this frame. Draws over the whole
/// viewport, whatever margin the `ui` it is given has.
pub fn draw(ui: &mut egui::Ui, state: &ViewState) -> Vec<UiAction> {
    // Tk delivers pointer events to the widget under the pointer and nowhere else; egui by
    // default also counts a pointer up to 5 px outside a widget as on it. The hit test reads
    // this at the start of a pass, so from the second frame on.
    if ui.ctx().global_style().interaction.interact_radius != 0.0 {
        ui.ctx()
            .all_styles_mut(|style| style.interaction.interact_radius = 0.0);
    }
    let screen = ui.ctx().content_rect();
    let mut window = ui.new_child(egui::UiBuilder::new().max_rect(screen));
    window.set_clip_rect(screen);
    sections::show(&mut window, state)
}

/// Load the fonts Tk uses and install the fallback for characters they lack (see [`fonts`]).
pub fn install_fonts(ctx: &egui::Context) {
    fonts::install(ctx);
}
