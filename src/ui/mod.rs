//! Ports `ui/sections/` and `ui/widgets/` entry point: draws the whole window from a `ViewState`.

pub mod layout;
pub mod sections;
pub mod text;
pub mod theme;
pub mod widgets;

use crate::contract::{UiAction, ViewState};
use egui::epaint::text::{FontData, FontDefinitions, FontFamily, FontTweak};
use std::sync::Arc;

/// Pure function of `state`; returns what the user did this frame. Draws over the whole
/// viewport, whatever margin the `ui` it is given has.
pub fn draw(ui: &mut egui::Ui, state: &ViewState) -> Vec<UiAction> {
    let screen = ui.ctx().content_rect();
    let mut window = ui.new_child(egui::UiBuilder::new().max_rect(screen));
    window.set_clip_rect(screen);
    sections::show(&mut window, state)
}

/// The Windows fonts Tk uses, keyed by the egui family each one heads.
const SYSTEM_FONTS: [(&str, &str); 3] = [
    (theme::SEGOE, "segoeui.ttf"),
    (theme::SEGOE_BOLD, "segoeuib.ttf"),
    (theme::CONSOLAS, "consola.ttf"),
];

fn fonts_dir() -> std::path::PathBuf {
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    std::path::Path::new(&windir).join("Fonts")
}

/// Load Segoe UI (regular and bold) and Consolas from the Windows font directory and register
/// the families the theme refers to. Each family falls back to the other Windows fonts and then
/// to egui's built-in fonts, so a missing file or glyph never fails.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let builtin_proportional = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let builtin_monospace = defs
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let dir = fonts_dir();
    let mut loaded = Vec::new();
    for (name, file) in SYSTEM_FONTS {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            // GDI places glyphs on whole pixels; so do we, so sub-pixel offsets stay off.
            let data = FontData::from_owned(bytes).tweak(FontTweak {
                subpixel_binning: Some(false),
                ..Default::default()
            });
            defs.font_data.insert(name.to_owned(), Arc::new(data));
            loaded.push(name);
        }
    }

    let chain = |primary: &[&str], builtin: &[String]| -> Vec<String> {
        let mut names: Vec<String> = primary
            .iter()
            .filter(|n| loaded.contains(n))
            .map(|n| (*n).to_owned())
            .collect();
        names.extend(builtin.iter().cloned());
        names
    };
    let segoe = chain(&[theme::SEGOE, theme::CONSOLAS], &builtin_proportional);
    let segoe_bold = chain(
        &[theme::SEGOE_BOLD, theme::SEGOE, theme::CONSOLAS],
        &builtin_proportional,
    );
    let consolas = chain(&[theme::CONSOLAS, theme::SEGOE], &builtin_monospace);

    defs.families
        .insert(FontFamily::Name(theme::SEGOE.into()), segoe.clone());
    defs.families
        .insert(FontFamily::Name(theme::SEGOE_BOLD.into()), segoe_bold);
    defs.families
        .insert(FontFamily::Name(theme::CONSOLAS.into()), consolas.clone());
    defs.families.insert(FontFamily::Proportional, segoe);
    defs.families.insert(FontFamily::Monospace, consolas);
    ctx.set_fonts(defs);
}
