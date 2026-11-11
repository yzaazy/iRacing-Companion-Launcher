//! Entry point (ports `iracing_launcher.py`): opens the window and draws the UI each frame.

// Release builds are windowed; debug builds keep a console for diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iracing_companion_launcher::contract::ViewState;
use iracing_companion_launcher::ui;

const WINDOW_TITLE: &str = "iRacing Companion Launcher";
const WINDOW_SIZE: [f32; 2] = [1200.0, 700.0];

/// Scaffold app: draws a fixed placeholder state. Wave 2 replaces it with the controller in `app.rs`.
struct ScaffoldApp {
    state: ViewState,
}

impl eframe::App for ScaffoldApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let _actions = ui::draw(ui, &self.state);
    }
}

fn placeholder_state() -> ViewState {
    ViewState {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        apps: Vec::new(),
        games: Vec::new(),
        selected_game: None,
        log: Vec::new(),
        select_all_label: "Select All".to_owned(),
        launch_label: "Launch apps (0)".to_owned(),
        close_label: "Close all (0)".to_owned(),
        launch_enabled: false,
        close_enabled: false,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(WINDOW_SIZE)
            .with_resizable(false)
            .with_maximize_button(false),
        ..Default::default()
    };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|cc| {
            ui::install_fonts(&cc.egui_ctx);
            Ok(Box::new(ScaffoldApp {
                state: placeholder_state(),
            }))
        }),
    )
}
