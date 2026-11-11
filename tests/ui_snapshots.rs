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

fn render_scaled(state: &ViewState, pixels_per_point: f32) -> image::RgbaImage {
    let _gpu = gpu();
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .with_pixels_per_point(pixels_per_point)
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

/// The window at Windows display scaling of 125, 150 and 200 %, for a look:
/// `target/ui-snapshots/scales/`. There is no Python reference at these scales; the window
/// scales as a whole and text keeps GDI's whole-device-pixel spacing (`ui_layout.rs` checks the
/// spacing itself).
#[test]
fn render_display_scales() {
    let out_dir: PathBuf = repo_root().join("target/ui-snapshots/scales");
    std::fs::create_dir_all(&out_dir).unwrap();
    let states = fixture_states();
    for name in ["mixed", "log-full"] {
        let Some(state) = states.get(name) else {
            continue;
        };
        for ppp in [1.0f32, 1.25, 1.5, 2.0] {
            let image = render_scaled(state, ppp);
            let want = (
                (WINDOW.x * ppp).round() as u32,
                (WINDOW.y * ppp).round() as u32,
            );
            assert_eq!(image.dimensions(), want, "{name} at {ppp}");
            image
                .save(out_dir.join(format!("{name}@{}.png", (ppp * 100.0) as u32)))
                .unwrap();
        }
    }
}

/// Names too long for their card are clipped at their label, as Tk clips a label's text,
/// instead of running through the card's border: `target/ui-snapshots/other/long-names.png`.
#[test]
fn long_names_are_clipped_at_their_label() {
    use iracing_companion_launcher::contract::{AppCard, GameCard, Status};
    use iracing_companion_launcher::ui::{sections, theme};

    let long = "Fanatec Wheel Base Control Panel And Firmware Updater Utility";
    let mut state = fixture_states().remove("mixed").unwrap_or_else(placeholder);
    state.apps.truncate(2);
    state.apps[0] = AppCard {
        name: long.into(),
        status: Status::Running,
        checked: true,
        helper_count: 1,
    };
    state.apps[1] = AppCard {
        name: format!("{long} Missing"),
        status: Status::NotFound,
        checked: false,
        helper_count: 0,
    };
    state.games.truncate(1);
    state.games[0] = GameCard {
        name: "Automobilista2RacingSimulationDeluxeEdition".into(),
        status: Status::Idle,
    };
    state.selected_game = None;
    let image = render(&state);
    let out_dir: PathBuf = repo_root().join("target/ui-snapshots/other");
    std::fs::create_dir_all(&out_dir).unwrap();
    image.save(out_dir.join("long-names.png")).unwrap();

    let ctx = egui::Context::default();
    ui::install_fonts(&ctx);
    let mut geom = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        geom = Some(sections::geometry(ui.ctx(), &state, WINDOW));
    });
    output.textures_delta.clear();
    let geom = geom.unwrap();

    let px = |x: i32, y: i32| {
        let p = image.get_pixel(x as u32, y as u32);
        egui::Color32::from_rgb(p[0], p[1], p[2])
    };
    for (card, name) in [
        (geom.apps[0].card, geom.apps[0].name.label),
        (geom.apps[1].card, geom.apps[1].name.label),
        (geom.games[0].card, geom.games[0].name.label),
    ] {
        let painted = card.painted();
        assert!(
            name.right() < painted.right() - 1,
            "the name label {name:?} is not squeezed inside its card {card:?}"
        );
        // Between the name label and the card's right border nothing of the name shows: the
        // column right after the label is card fill or another widget, never white ink.
        for y in name.y..name.bottom() {
            assert_ne!(
                px(name.right(), y),
                theme::TEXT_WHITE,
                "ink past {name:?} at y {y}"
            );
            assert_ne!(
                px(name.right(), y),
                theme::TEXT_DIM,
                "ink past {name:?} at y {y}"
            );
        }
        // The right border is intact along the name's rows.
        let border = painted.right() - 1;
        let border_color = if card == geom.games[0].card {
            theme::GAME_CARD_BORDER
        } else {
            theme::CARD_BORDER
        };
        for y in name.y.max(painted.y + 8)..name.bottom().min(painted.bottom() - 8) {
            assert_eq!(px(border, y), border_color, "border broken at y {y}");
        }
    }
}
