//! Entry point (ports `iracing_launcher.py`): runs startup, opens the window and draws the UI each frame.

// Release builds are windowed; debug builds keep a console for diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iracing_companion_launcher::app::Controller;
use iracing_companion_launcher::core::config::{ConfigStore, SharedConfig, resolve_config_dir};
use iracing_companion_launcher::dialog::NativeDialog;
use iracing_companion_launcher::managers::Managers;
use iracing_companion_launcher::process::tracker::ProcessTracker;
use iracing_companion_launcher::ui;
use iracing_companion_launcher::worker::{Sequencer, Worker};

const WINDOW_TITLE: &str = "iRacing Companion Launcher";
const WINDOW_SIZE: [f32; 2] = [1200.0, 700.0];
const STATE_FILE: &str = "process_state.json";

/// `iRCL.ico`, whose 256 px image is a PNG.
const ICON: &[u8] = include_bytes!("../iRCL.ico");

struct LauncherApp {
    controller: Controller,
}

impl eframe::App for LauncherApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.controller.pump();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let actions = ui::draw(ui, self.controller.state());
        if actions.is_empty() {
            return;
        }
        for action in actions {
            self.controller
                .handle(action, &mut NativeDialog::new(frame));
        }
        // The state changed after this frame was drawn.
        ui.ctx().request_repaint();
    }
}

/// The largest PNG image in an `.ico` file, as window icon data.
fn window_icon(ico: &[u8]) -> Option<egui::IconData> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(ico.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(ico.get(at..at + 4)?.try_into().ok()?));
    let count = usize::from(u16_at(4)?);
    let mut best: Option<(u32, &[u8])> = None;
    for i in 0..count {
        let entry = 6 + 16 * i;
        // A width byte of 0 means 256.
        let width = match *ico.get(entry)? {
            0 => 256,
            w => u32::from(w),
        };
        let size = usize::try_from(u32_at(entry + 8)?).ok()?;
        let offset = usize::try_from(u32_at(entry + 12)?).ok()?;
        let image = ico.get(offset..offset.checked_add(size)?)?;
        if image.starts_with(b"\x89PNG") && best.is_none_or(|(w, _)| width > w) {
            best = Some((width, image));
        }
    }
    eframe::icon_data::from_png_bytes(best?.1).ok()
}

/// Fonts and options for the app's egui context.
fn configure_context(ctx: &egui::Context) {
    ui::install_fonts(ctx);
    // egui zooms the window on Ctrl+Plus/Minus/0 by default; the Python app has
    // no such keys, and a zoomed window breaks the fixed 1200x700 layout.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

fn main() -> eframe::Result {
    // iRacingLauncherGUI.__init__: config, tracker, managers, then the startup
    // checks, all before the window first paints.
    let config_dir = resolve_config_dir();
    let config = SharedConfig::new(ConfigStore::load(&config_dir));
    let tracker = ProcessTracker::new(config_dir.join(STATE_FILE));
    let mut managers = Managers::new(config.clone(), tracker);
    let startup = Controller::startup(
        &mut managers,
        Box::new(config),
        env!("CARGO_PKG_VERSION"),
        None,
    );

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size(WINDOW_SIZE)
        .with_resizable(false)
        .with_maximize_button(false);
    if let Some(icon) = window_icon(ICON) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            configure_context(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let sequencer = Sequencer::new(Box::new(managers), startup.game_statuses());
            let worker = Worker::spawn(sequencer, move || ctx.request_repaint());
            Ok(Box::new(LauncherApp {
                controller: startup.attach(Box::new(worker)),
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{ICON, configure_context, window_icon};

    #[test]
    fn embedded_icon_decodes() {
        let icon = window_icon(ICON).expect("iRCL.ico holds a PNG image");
        assert_eq!((icon.width, icon.height), (256, 256));
    }

    /// A frame in which Ctrl and `key` are pressed; returns the zoom factor that follows.
    fn zoom_after(ctx: &egui::Context, key: egui::Key) -> f32 {
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        };
        // A new zoom factor takes effect when the next frame begins.
        for input in [input, egui::RawInput::default()] {
            ctx.run_ui(input, |_| {}).textures_delta.clear();
        }
        ctx.zoom_factor()
    }

    #[test]
    fn keyboard_zoom_is_off() {
        // Control: an unconfigured context does zoom on Ctrl+=.
        let default = egui::Context::default();
        assert_eq!(zoom_after(&default, egui::Key::Equals), 1.1);

        let ctx = egui::Context::default();
        configure_context(&ctx);
        assert!(!ctx.options(|o| o.zoom_with_keyboard));
        for key in [
            egui::Key::Plus,
            egui::Key::Equals,
            egui::Key::Minus,
            egui::Key::Num0,
        ] {
            assert_eq!(zoom_after(&ctx, key), 1.0, "{key:?}");
        }
    }
}
