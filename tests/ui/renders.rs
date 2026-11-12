//! Renders that hold rules the reference screenshots do not cover: long names, and display
//! scaling. The fixture states themselves are checked in `parity.rs`.

use crate::support::{WINDOW, builder, fixture, geometry, render, render_at, repo_path};
use egui::{Color32, Key};
use iracing_companion_launcher::contract::{AppCard, GameCard, Status};
use iracing_companion_launcher::ui::layout::Bounds;
use iracing_companion_launcher::ui::{self, theme};

/// The same rules at other window sizes and display scales, for a look:
/// `target/ui-snapshots/other-sizes/` and `target/ui-snapshots/scales/`. Nothing to compare them
/// with; `display_scale_keeps_dots_and_focus_inside_their_cards` holds one scale to rules.
#[test]
#[ignore = "visual inspection"]
fn render_other_window_sizes_and_scales() {
    let out_dir = repo_path("target/ui-snapshots/other-sizes");
    std::fs::create_dir_all(&out_dir).unwrap();
    for (name, size) in [
        ("mixed", egui::vec2(1400.0, 800.0)),
        ("log-full", egui::vec2(1000.0, 640.0)),
    ] {
        let image = render_at(&fixture(name), size, 1.0);
        assert_eq!(image.dimensions(), (size.x as u32, size.y as u32));
        image
            .save(out_dir.join(format!("{name}-{}x{}.png", size.x, size.y)))
            .unwrap();
    }
    let out_dir = repo_path("target/ui-snapshots/scales");
    std::fs::create_dir_all(&out_dir).unwrap();
    for name in ["mixed", "log-full"] {
        for ppp in [1.0f32, 1.25, 1.5, 2.0] {
            let image = render_at(&fixture(name), WINDOW, ppp);
            image
                .save(out_dir.join(format!("{name}@{}.png", (ppp * 100.0) as u32)))
                .unwrap();
        }
    }
}

/// Whether device pixel `x, y` lies inside `b`, given in points, at `scale`.
fn inside_scaled(b: Bounds, scale: f32, x: u32, y: u32) -> bool {
    let r = b.rect();
    let (x, y) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
    r.left() <= x && x < r.right() && r.top() <= y && y < r.bottom()
}

/// At 150 % Windows display scaling the window scales as a whole: every status dot is drawn
/// inside its card and nowhere near it outside, and the focus ring around a checkbox stays
/// inside its card. Text spacing at this scale is held by `layout.rs`.
#[test]
fn display_scale_keeps_dots_and_focus_inside_their_cards() {
    const SCALE: f32 = 1.5;
    for name in ["mixed", "idle"] {
        let state = fixture(name);
        let geom = geometry(&state);
        let image = render_at(&state, WINDOW, SCALE);
        assert_eq!(image.dimensions(), (1800, 1050), "{name}");
        let at = |x: u32, y: u32| {
            let p = image.get_pixel(x, y);
            Color32::from_rgb(p[0], p[1], p[2])
        };
        let dots = state
            .apps
            .iter()
            .zip(&geom.apps)
            .filter(|(_, g)| g.browse.is_none())
            .map(|(a, g)| (&a.name, a.status.clone(), g.card.painted()))
            .chain(
                state
                    .games
                    .iter()
                    .zip(&geom.games)
                    .filter(|(_, g)| g.browse.is_none())
                    .map(|(a, g)| (&a.name, a.status.clone(), g.card.painted())),
            );
        let mut checked = 0;
        for (card_name, status, card) in dots {
            let color = theme::status_color(&status);
            // The card and a 12 pt band around it.
            let band = Bounds::new(card.x - 12, card.y - 12, card.w + 24, card.h + 24);
            let (x0, y0) = (
                (band.x as f32 * SCALE) as u32,
                (band.y as f32 * SCALE) as u32,
            );
            let (x1, y1) = (
                (band.right() as f32 * SCALE) as u32,
                (band.bottom() as f32 * SCALE) as u32,
            );
            let mut inside = 0;
            for y in y0..y1.min(1050) {
                for x in x0..x1.min(1800) {
                    if at(x, y) != color {
                        continue;
                    }
                    assert!(
                        inside_scaled(card, SCALE, x, y),
                        "{name}: {card_name}'s dot colour at {x},{y}, outside its card {card:?}"
                    );
                    inside += 1;
                }
            }
            // A dot of radius 8.5 pt has well over 300 solid device pixels at 150 %.
            assert!(inside > 300, "{name}: {card_name}'s dot has {inside} px");
            checked += 1;
        }
        assert!(checked >= 7, "{name}: only {checked} dots");
    }

    // Tab to the Fanatec checkbox: everything the ring changes lies inside the Fanatec card.
    let state = fixture("mixed");
    let card = geometry(&state).apps[0].card.painted();
    let mut h = builder().with_pixels_per_point(SCALE).build_ui(move |ui| {
        ui::draw(ui, &state);
    });
    h.run();
    let plain = h.render().unwrap();
    for _ in 0..8 {
        h.key_press(Key::Tab);
        h.run();
    }
    let focused = h.render().unwrap();
    let mut ring = 0;
    for (x, y, p) in focused.enumerate_pixels() {
        if p != plain.get_pixel(x, y) {
            assert!(
                inside_scaled(card, SCALE, x, y),
                "the focus ring changed {x},{y}, outside the Fanatec card {card:?}"
            );
            ring += 1;
        }
    }
    assert!(ring > 100, "no focus ring: {ring} px changed");
}

/// Names too long for their card are clipped at their label, as Tk clips a label's text,
/// instead of running through the card's border: `target/ui-snapshots/other/long-names.png`.
#[test]
fn long_names_are_clipped_at_their_label() {
    let long = "Fanatec Wheel Base Control Panel And Firmware Updater Utility";
    let mut state = fixture("mixed");
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
    let out_dir = repo_path("target/ui-snapshots/other");
    std::fs::create_dir_all(&out_dir).unwrap();
    image.save(out_dir.join("long-names.png")).unwrap();

    let geom = geometry(&state);

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
