//! Keyboard access, which the Python window does not have: nothing is focused until Tab, Tab
//! follows the reading order and skips what cannot act, Space and Enter activate only the focused
//! control, a visible ring marks it, and any mouse press or Escape lets go of it. Unfocused, the
//! window is exactly what it was before keyboard access existed.

use crate::support::{fixture, geometry, harness, pixel, press, repo_path, tabs};
use egui::{Key, Modifiers, Pos2, pos2};
use iracing_companion_launcher::contract::UiAction;
use iracing_companion_launcher::ui::layout::Bounds;
use iracing_companion_launcher::ui::theme;

/// The unfocused window of each state is held to the reference and to its golden render in
/// `parity.rs`; focus that came and went leaves nothing behind.
#[test]
fn tab_then_escape_leaves_no_mark() {
    for name in crate::support::STATES {
        let mut h = harness(fixture(name));
        let image = h.render().unwrap();
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
    let out = repo_path("target/ui-snapshots/focus");
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
