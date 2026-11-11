//! Renders every fixture state headlessly to `target/ui-snapshots/<state>.png` for `tools/ui_diff.py`.

use egui_kittest::Harness;
use iracing_companion_launcher::contract::ViewState;
use iracing_companion_launcher::ui;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const WINDOW: egui::Vec2 = egui::vec2(1200.0, 700.0);

/// The headless wgpu adapter crashes when two threads create devices at once; tests in this
/// binary take turns.
static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu() -> std::sync::MutexGuard<'static, ()> {
    GPU.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The fixture states, or one built-in `placeholder` state while `fixtures.json` does not exist yet.
fn fixture_states() -> BTreeMap<String, ViewState> {
    let path = repo_root().join("docs/rewrite/fixtures.json");
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} does not match ViewState: {e}", path.display())),
        Err(_) => BTreeMap::from([("placeholder".to_owned(), placeholder())]),
    }
}

fn placeholder() -> ViewState {
    ViewState {
        version: "2.3.2".into(),
        apps: Vec::new(),
        games: Vec::new(),
        selected_game: None,
        log: Vec::new(),
        select_all_label: "Select All".into(),
        launch_label: "Launch apps (0)".into(),
        close_label: "Close all (0)".into(),
        launch_enabled: false,
        close_enabled: false,
    }
}

fn render(state: &ViewState) -> image::RgbaImage {
    render_at(state, WINDOW)
}

fn render_at(state: &ViewState, window: egui::Vec2) -> image::RgbaImage {
    let _gpu = gpu();
    let mut harness = Harness::builder()
        .with_size(window)
        .with_pixels_per_point(1.0)
        .wgpu()
        .with_context_setup(ui::install_fonts)
        .build_ui(|ui| {
            ui::draw(ui, state);
        });
    harness.run();
    harness
        .render()
        .unwrap_or_else(|e| panic!("headless wgpu render failed: {e}"))
}

#[test]
fn render_fixture_states() {
    let out_dir: PathBuf = repo_root().join("target/ui-snapshots");
    std::fs::create_dir_all(&out_dir).unwrap();
    for (name, state) in fixture_states() {
        let image = render(&state);
        assert_eq!(image.dimensions(), (1200, 700), "state {name}");
        let path = out_dir.join(format!("{name}.png"));
        image.save(&path).unwrap();
    }
}

/// The same rules at other window sizes, for a look: `target/ui-snapshots/other-sizes/`.
/// ui_diff.py does not compare these; they have no reference.
#[test]
fn render_other_window_sizes() {
    let out_dir: PathBuf = repo_root().join("target/ui-snapshots/other-sizes");
    std::fs::create_dir_all(&out_dir).unwrap();
    let states = fixture_states();
    for (name, size) in [
        ("mixed", egui::vec2(1400.0, 800.0)),
        ("log-full", egui::vec2(1000.0, 640.0)),
    ] {
        let Some(state) = states.get(name) else {
            continue;
        };
        let image = render_at(state, size);
        assert_eq!(image.dimensions(), (size.x as u32, size.y as u32));
        image
            .save(out_dir.join(format!("{name}-{}x{}.png", size.x, size.y)))
            .unwrap();
    }
}
