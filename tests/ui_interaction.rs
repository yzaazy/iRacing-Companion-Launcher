//! Clicks become `UiAction`s exactly where the Python widgets are clickable; disabled controls
//! produce nothing; hover colours match CustomTkinter's.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use iracing_companion_launcher::contract::{UiAction, ViewState};
use iracing_companion_launcher::ui::{self, sections, theme};
use std::collections::BTreeMap;
use std::path::Path;

const WINDOW: egui::Vec2 = egui::vec2(1200.0, 700.0);

/// The headless wgpu adapter crashes when two threads create devices at once; tests in this
/// binary take turns.
static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu() -> std::sync::MutexGuard<'static, ()> {
    GPU.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn fixture(name: &str) -> ViewState {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/rewrite/fixtures.json");
    let all: BTreeMap<String, ViewState> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    all[name].clone()
}

struct Probe {
    state: ViewState,
    actions: Vec<UiAction>,
}

fn harness(state: ViewState) -> Harness<'static, Probe> {
    let mut h = Harness::builder()
        .with_size(WINDOW)
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_context_setup(ui::install_fonts)
        .build_ui_state(
            |ui, probe: &mut Probe| {
                let actions = ui::draw(ui, &probe.state);
                probe.actions.extend(actions);
            },
            Probe {
                state,
                actions: Vec::new(),
            },
        );
    h.run();
    h
}

/// Click the widget labelled `label` and return the actions that click produced.
fn click(state: &str, label: &str) -> Vec<UiAction> {
    let mut h = harness(fixture(state));
    h.get_by_label(label).click();
    h.run();
    h.state().actions.clone()
}

#[test]
fn checkbox_toggles_its_app() {
    let _gpu = gpu();
    assert_eq!(
        click("idle", "Fanatec"),
        vec![UiAction::ToggleApp {
            name: "Fanatec".into(),
            checked: false
        }]
    );
    assert_eq!(
        click("mixed", "Trading Paints"),
        vec![UiAction::ToggleApp {
            name: "Trading Paints".into(),
            checked: true
        }]
    );
}

#[test]
fn checkbox_of_a_missing_app_does_nothing() {
    let _gpu = gpu();
    assert_eq!(click("mixed", "Elgato Stream Deck"), vec![]);
}

#[test]
fn select_all_toggles() {
    let _gpu = gpu();
    assert_eq!(
        click("idle", "Deselect All"),
        vec![UiAction::ToggleSelectAll]
    );
    assert_eq!(
        click("none-selected", "Select All"),
        vec![UiAction::ToggleSelectAll]
    );
}

#[test]
fn radio_selects_a_game_or_none() {
    let _gpu = gpu();
    assert_eq!(
        click("idle", "Automobilista 2"),
        vec![UiAction::SelectGame(Some("Automobilista 2".into()))]
    );
    assert_eq!(click("mixed", "None"), vec![UiAction::SelectGame(None)]);
}

#[test]
fn radio_already_selected_or_missing_does_nothing() {
    let _gpu = gpu();
    assert_eq!(click("mixed", "iRacing"), vec![]);
    assert_eq!(click("idle", "None"), vec![]);
    assert_eq!(click("mixed", "Assetto Corsa Competizione"), vec![]);
}

#[test]
fn browse_buttons_name_their_card() {
    let _gpu = gpu();
    assert_eq!(
        click("mixed", "Browse for Elgato Stream Deck"),
        vec![UiAction::BrowseApp("Elgato Stream Deck".into())]
    );
    assert_eq!(
        click("mixed", "Browse for Assetto Corsa Competizione"),
        vec![UiAction::BrowseGame("Assetto Corsa Competizione".into())]
    );
    // The squeezed Browse button is still clickable.
    assert_eq!(
        click("all-missing", "Browse for MarvinsAIRA Refactored"),
        vec![UiAction::BrowseApp("MarvinsAIRA Refactored".into())]
    );
}

#[test]
fn action_buttons_launch_and_close() {
    let _gpu = gpu();
    assert_eq!(
        click("mixed", "Launch with iRacing (5)"),
        vec![UiAction::Launch]
    );
    assert_eq!(click("mixed", "Close all (5)"), vec![UiAction::Close]);
}

#[test]
fn disabled_action_buttons_do_nothing() {
    let _gpu = gpu();
    assert_eq!(click("none-selected", "Launch apps (0)"), vec![]);
    assert_eq!(click("none-selected", "Close all (0)"), vec![]);
}

#[test]
fn every_found_widget_is_reachable() {
    let _gpu = gpu();
    let h = harness(fixture("mixed"));
    for label in [
        "Fanatec",
        "Crew Chief V4",
        "Trading Paints",
        "SimHub",
        "Garage61",
        "MarvinsAIRA Refactored",
        "None",
        "iRacing",
        "Assetto Corsa Evo",
        "Automobilista 2",
        "rFactor 2",
        "Select All",
    ] {
        let _ = h.get_by_label(label);
    }
}

/// The colour at a point after hovering another point, rendered headlessly.
fn pixel_after_hover(state: &str, hover: egui::Pos2, sample: egui::Pos2) -> egui::Color32 {
    let mut h = harness(fixture(state));
    h.hover_at(hover);
    h.run();
    let image = h.render().expect("headless render");
    let p = image.get_pixel(sample.x as u32, sample.y as u32);
    egui::Color32::from_rgb(p[0], p[1], p[2])
}

fn geometry(state: &str) -> sections::Geometry {
    let state = fixture(state);
    let ctx = egui::Context::default();
    ui::install_fonts(&ctx);
    let mut geom = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        geom = Some(sections::geometry(ui.ctx(), &state, WINDOW));
    });
    output.textures_delta.clear();
    geom.unwrap()
}

#[test]
fn hover_colours_match_customtkinter() {
    let _gpu = gpu();
    let idle = geometry("idle");
    let corner_free = |b: iracing_companion_launcher::ui::layout::Bounds| {
        egui::pos2(b.x as f32 + 14.0, b.y as f32 + b.h as f32 / 2.0)
    };

    // Buttons fill with their hover colour.
    let launch = corner_free(idle.launch.rect);
    assert_eq!(
        pixel_after_hover("idle", launch, launch),
        theme::LAUNCH_BUTTON.hover
    );
    let close = corner_free(idle.close.rect);
    assert_eq!(
        pixel_after_hover("idle", close, close),
        theme::CLOSE_BUTTON.hover
    );
    let select_all = corner_free(idle.select_all.rect);
    assert_eq!(
        pixel_after_hover("idle", select_all, select_all),
        theme::GREY_BUTTON.hover
    );

    // A disabled button keeps its fill.
    let none = geometry("none-selected");
    let disabled = corner_free(none.launch.rect);
    assert_eq!(
        pixel_after_hover("none-selected", disabled, disabled),
        theme::LAUNCH_BUTTON.fill
    );

    // An unchecked checkbox fills with the hover blue inside its grey ring.
    let b = none.apps[0].parts.toggle_box;
    let centre = egui::pos2(b.x as f32 + 12.0, b.y as f32 + 12.0);
    assert_eq!(
        pixel_after_hover("none-selected", centre, centre),
        theme::CHECK_HOVER
    );
    assert_eq!(
        pixel_after_hover(
            "none-selected",
            centre,
            egui::pos2(b.x as f32 + 1.0, b.y as f32 + 12.0)
        ),
        theme::CHECK_BORDER
    );

    // A radio button's ring turns the hover colour.
    let r = idle.games[0].parts.toggle_box;
    let ring = egui::pos2(r.x as f32 + 1.0, r.y as f32 + 11.0);
    let centre = egui::pos2(r.x as f32 + 11.0, r.y as f32 + 11.0);
    assert_eq!(pixel_after_hover("idle", centre, ring), theme::RADIO_HOVER);

    // A Browse button.
    let mixed = geometry("mixed");
    let browse = mixed.apps[5].browse.as_ref().unwrap().rect;
    let inside = corner_free(browse);
    assert_eq!(
        pixel_after_hover("mixed", inside, inside),
        theme::BROWSE_BUTTON.hover
    );
}

#[test]
fn new_log_line_scrolls_to_the_end() {
    let _gpu = gpu();
    let mut state = fixture("log-full");
    let mut h = harness(state.clone());
    // Scroll up with the wheel, then add a line: the log must jump back to the end.
    h.hover_at(egui::pos2(900.0, 300.0));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 10.0),
        modifiers: egui::Modifiers::default(),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
    // The wheel scrolls smoothly over several frames.
    h.run_steps(40);
    let scrolled = h.render().unwrap();
    state
        .log
        .push(iracing_companion_launcher::contract::LogLine {
            level: iracing_companion_launcher::contract::LogLevel::Info,
            text: "[19:45:07] One more line".into(),
        });
    h.state_mut().state = state;
    h.run();
    let after = h.render().unwrap();
    assert_ne!(scrolled, after, "scrolling had no effect");
    // The new line is the last one shown: 17 px above the final empty line at the bottom.
    let log_bottom = 131 + 429;
    let y = (log_bottom - 2 * 17 + 10) as u32;
    let row_has_ink = (680..900).any(|x| {
        let p = after.get_pixel(x, y);
        p[0] > 150
    });
    assert!(row_has_ink, "the new line is not at the end of the log");
}
