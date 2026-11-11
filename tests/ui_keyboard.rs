//! Keyboard access, which the Python window does not have: nothing is focused until Tab, Tab
//! follows the reading order and skips what cannot act, Space and Enter activate only the focused
//! control, a visible ring marks it, and any mouse press or Escape lets go of it. Unfocused, the
//! window is exactly what it was before keyboard access existed.

use egui::{Key, Modifiers, Pos2, pos2};
use egui_kittest::Harness;
use iracing_companion_launcher::contract::{UiAction, ViewState};
use iracing_companion_launcher::ui::layout::Bounds;
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

fn fixtures() -> BTreeMap<String, ViewState> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/rewrite/fixtures.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn fixture(name: &str) -> ViewState {
    fixtures()[name].clone()
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

fn press(h: &mut Harness<'static, Probe>, key: Key) {
    h.key_press(key);
    h.run();
}

fn tabs(h: &mut Harness<'static, Probe>, n: usize) {
    for _ in 0..n {
        press(h, Key::Tab);
    }
}

fn geometry(state: &ViewState) -> sections::Geometry {
    let ctx = egui::Context::default();
    ui::install_fonts(&ctx);
    let mut geom = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        geom = Some(sections::geometry(ui.ctx(), state, WINDOW));
    });
    output.textures_delta.clear();
    geom.unwrap()
}

fn pixel(image: &image::RgbaImage, p: Pos2) -> egui::Color32 {
    let q = image.get_pixel(p.x as u32, p.y as u32);
    egui::Color32::from_rgb(q[0], q[1], q[2])
}

/// FNV-1a over the RGBA bytes of a render.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The unfocused renders at 1200x700 and 1.0 pixels per point on the headless wgpu adapter,
/// which pass `ui_diff.py`. Keyboard access changed none of their pixels. They differ from the
/// renders before it in 18 pixels only: the bottom row of the title's `p` descender, which
/// overhangs the 30 px title label; Tk clips it there (the reference shows background), and so
/// does the label clipping that came in with it. If the adapter or driver changes, regenerate
/// these from renders that still pass `ui_diff.py`.
const UNFOCUSED_RENDERS: [(&str, u64); 5] = [
    ("all-missing", 0xcd55_e977_b6b6_c894),
    ("idle", 0x2a29_1b38_0e52_b06f),
    ("log-full", 0xf197_3aee_bd80_7c24),
    ("mixed", 0x62c1_41b2_1559_0863),
    ("none-selected", 0x59d3_2c28_c367_9682),
];

#[test]
fn unfocused_renders_are_unchanged() {
    let _gpu = gpu();
    for (name, want) in UNFOCUSED_RENDERS {
        let mut h = harness(fixture(name));
        let image = h.render().unwrap();
        assert_eq!(
            fnv(image.as_raw()),
            want,
            "{name}: unfocused render changed"
        );
        // Focus that came and went leaves nothing behind.
        tabs(&mut h, 3);
        press(&mut h, Key::Escape);
        assert_eq!(
            h.render().unwrap(),
            image,
            "{name}: Tab then Escape left a mark"
        );
    }
}

#[test]
fn nothing_is_focused_at_startup() {
    let _gpu = gpu();
    for name in ["idle", "mixed", "log-full"] {
        let mut h = harness(fixture(name));
        let before = h.render().unwrap();
        for key in [Key::Space, Key::Enter, Key::Space, Key::Enter] {
            press(&mut h, key);
        }
        assert_eq!(
            h.state().actions,
            vec![],
            "{name}: a key acted without focus"
        );
        assert_eq!(
            h.render().unwrap(),
            before,
            "{name}: a key changed the window"
        );
    }
}

/// What Enter does after `n` presses of Tab: the n-th stop's action.
fn enter_after_tabs(state: &str, n: usize) -> Vec<UiAction> {
    let mut h = harness(fixture(state));
    tabs(&mut h, n);
    press(&mut h, Key::Enter);
    h.state().actions.clone()
}

fn toggle(name: &str, checked: bool) -> Option<UiAction> {
    Some(UiAction::ToggleApp {
        name: name.into(),
        checked,
    })
}

fn select(name: Option<&str>) -> Option<UiAction> {
    Some(UiAction::SelectGame(name.map(Into::into)))
}

#[test]
fn tab_follows_the_layout_and_skips_inert_controls() {
    let _gpu = gpu();
    // mixed: iRacing is selected (its radio is a stop that does nothing), Assetto Corsa
    // Competizione and Elgato Stream Deck are not found (their toggles are skipped; their Browse
    // buttons are stops), and the log does not overflow (no stop).
    let mixed = [
        select(None),
        None,
        Some(UiAction::BrowseGame("Assetto Corsa Competizione".into())),
        select(Some("Assetto Corsa Evo")),
        select(Some("Automobilista 2")),
        select(Some("rFactor 2")),
        Some(UiAction::ToggleSelectAll),
        toggle("Fanatec", false),
        toggle("Crew Chief V4", false),
        toggle("Trading Paints", true),
        toggle("SimHub", false),
        toggle("Garage61", false),
        Some(UiAction::BrowseApp("Elgato Stream Deck".into())),
        toggle("MarvinsAIRA Refactored", false),
        Some(UiAction::Launch),
        Some(UiAction::Close),
    ];
    for (i, want) in mixed.iter().enumerate() {
        assert_eq!(
            enter_after_tabs("mixed", i + 1),
            want.iter().cloned().collect::<Vec<_>>(),
            "mixed, stop {i}"
        );
    }
    // Past the last stop, Tab wraps to the first.
    assert_eq!(
        enter_after_tabs("mixed", mixed.len() + 1),
        vec![UiAction::SelectGame(None)]
    );

    // none-selected: Launch and Close are disabled and skipped; after the last checkbox Tab
    // wraps to the None radio (selected, so Enter does nothing) and then iRacing.
    assert_eq!(
        enter_after_tabs("none-selected", 14),
        vec![UiAction::ToggleApp {
            name: "MarvinsAIRA Refactored".into(),
            checked: true
        }]
    );
    assert_eq!(enter_after_tabs("none-selected", 15), vec![]);
    assert_eq!(
        enter_after_tabs("none-selected", 16),
        vec![UiAction::SelectGame(Some("iRacing".into()))]
    );

    // log-full: the log overflows, so it is a stop between the apps and Launch.
    assert_eq!(enter_after_tabs("log-full", 15), vec![]);
    assert_eq!(enter_after_tabs("log-full", 16), vec![UiAction::Launch]);
}

#[test]
fn shift_tab_goes_backwards_and_space_activates_too() {
    let _gpu = gpu();
    let mut h = harness(fixture("mixed"));
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run();
    press(&mut h, Key::Space);
    assert_eq!(h.state().actions, vec![UiAction::Close]);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run();
    press(&mut h, Key::Enter);
    assert_eq!(h.state().actions, vec![UiAction::Close, UiAction::Launch]);
}

#[test]
fn a_mouse_press_or_escape_drops_the_focus() {
    let _gpu = gpu();
    let state = fixture("mixed");
    let geom = geometry(&state);

    // A click on a control acts once and leaves no focus behind.
    let mut h = harness(state.clone());
    tabs(&mut h, 8);
    let sa = geom.select_all.rect;
    h.event(egui::Event::PointerMoved(sa.rect().center()));
    h.event(egui::Event::PointerButton {
        pos: sa.rect().center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos: sa.rect().center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    press(&mut h, Key::Enter);
    press(&mut h, Key::Space);
    assert_eq!(h.state().actions, vec![UiAction::ToggleSelectAll]);
    let image = h.render().unwrap();
    let fanatec = geom.apps[0].parts.toggle_box;
    assert_ne!(
        pixel(&image, above(fanatec)),
        theme::FOCUS_RING,
        "the ring stayed on Fanatec"
    );

    // A click on the window background.
    let mut h = harness(state.clone());
    tabs(&mut h, 15);
    h.event(egui::Event::PointerButton {
        pos: pos2(5.0, 400.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    press(&mut h, Key::Enter);
    assert_eq!(h.state().actions, vec![]);

    // Escape.
    let mut h = harness(state);
    tabs(&mut h, 15);
    press(&mut h, Key::Escape);
    press(&mut h, Key::Enter);
    assert_eq!(h.state().actions, vec![]);
}

#[test]
fn a_held_enter_acts_once() {
    let _gpu = gpu();
    let mut h = harness(fixture("mixed"));
    tabs(&mut h, 15);
    h.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    for _ in 0..3 {
        h.event(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    assert_eq!(h.state().actions, vec![UiAction::Launch]);
}

/// Where the ring's white band is, 3 px above the middle of a control's top edge.
fn above(b: Bounds) -> Pos2 {
    pos2(b.x as f32 + (b.w / 2) as f32, b.y as f32 - 3.0)
}

/// Whether pixel `x, y` lies within the rounded rectangle `b` with corner radius `radius`.
fn inside_rounded(b: Bounds, radius: f32, x: i32, y: i32) -> bool {
    let p = pos2(x as f32 + 0.5, y as f32 + 0.5);
    let r = b.rect();
    let cx = p.x.clamp(r.left() + radius, r.right() - radius);
    let cy = p.y.clamp(r.top() + radius, r.bottom() - radius);
    p.distance(pos2(cx, cy)) <= radius
}

#[test]
fn the_focus_ring_shows_on_every_kind_of_control() {
    let _gpu = gpu();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/ui-snapshots/focus");
    std::fs::create_dir_all(&out).unwrap();
    let mixed = geometry(&fixture("mixed"));
    let log_full = geometry(&fixture("log-full"));
    let browse = mixed.games[1].browse.as_ref().unwrap().rect.painted();
    let small = theme::SMALL_BUTTON_RADIUS;
    let action = theme::ACTION_BUTTON_RADIUS;
    // Kind, state, Tab presses to reach it, painted shape and its corner radius.
    let cases = [
        ("radio", "mixed", 1, mixed.none_card.parts.toggle_box, 11.0),
        ("browse", "mixed", 3, browse, small),
        (
            "select-all",
            "mixed",
            7,
            mixed.select_all.rect.painted(),
            small,
        ),
        (
            "checkbox",
            "mixed",
            8,
            mixed.apps[0].parts.toggle_box,
            theme::CHECKBOX_RADIUS,
        ),
        ("launch", "mixed", 15, mixed.launch.rect.painted(), action),
        ("close", "mixed", 16, mixed.close.rect.painted(), action),
        (
            "log",
            "log-full",
            15,
            log_full.layout.log.log_box.painted(),
            theme::LOG_RADIUS,
        ),
    ];
    for (kind, state, n, b, radius) in cases {
        let mut h = harness(fixture(state));
        let plain = h.render().unwrap();
        tabs(&mut h, n);
        let image = h.render().unwrap();
        image.save(out.join(format!("{kind}.png"))).unwrap();
        assert_eq!(
            pixel(&image, above(b)),
            theme::FOCUS_RING,
            "{kind}: no ring above it"
        );
        // Drawn around the control, it changes nothing inside its shape.
        for y in b.y..b.bottom() {
            for x in b.x..b.right() {
                if !inside_rounded(b, radius, x, y) {
                    continue;
                }
                assert_eq!(
                    image.get_pixel(x as u32, y as u32),
                    plain.get_pixel(x as u32, y as u32),
                    "{kind}: the ring touched the control at {x},{y}"
                );
            }
        }
    }
}
