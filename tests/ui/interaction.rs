//! Presses become `UiAction`s exactly where the Python widgets are clickable, on the press as
//! CustomTkinter binds `<Button-1>`; disabled controls produce nothing; hover colours, the click
//! flash and the redraw after a toggle match CustomTkinter's; the log wheel jumps as Tk's does.

use crate::support::{
    Probe, button_event, fixture, geometry, harness, live, pixel_now as pixel, wheel,
};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use iracing_companion_launcher::contract::{LogLevel, LogLine, UiAction};
use iracing_companion_launcher::ui::layout::Bounds;
use iracing_companion_launcher::ui::theme;

/// Click the widget labelled `label` and return the actions that click produced.
fn click(state: &str, label: &str) -> Vec<UiAction> {
    let mut h = harness(fixture(state));
    h.get_by_label(label).click();
    h.run();
    h.state().actions.clone()
}

#[test]
fn checkbox_toggles_its_app() {
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
    assert_eq!(click("mixed", "Elgato Stream Deck"), vec![]);
}

#[test]
fn select_all_toggles() {
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
    assert_eq!(
        click("idle", "Automobilista 2"),
        vec![UiAction::SelectGame(Some("Automobilista 2".into()))]
    );
    assert_eq!(click("mixed", "None"), vec![UiAction::SelectGame(None)]);
}

#[test]
fn radio_already_selected_or_missing_does_nothing() {
    assert_eq!(click("mixed", "iRacing"), vec![]);
    assert_eq!(click("idle", "None"), vec![]);
    assert_eq!(click("mixed", "Assetto Corsa Competizione"), vec![]);
}

#[test]
fn browse_buttons_name_their_card() {
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
    assert_eq!(
        click("mixed", "Launch with iRacing (5)"),
        vec![UiAction::Launch]
    );
    assert_eq!(click("mixed", "Close all (5)"), vec![UiAction::Close]);
}

#[test]
fn disabled_action_buttons_do_nothing() {
    assert_eq!(click("none-selected", "Launch apps (0)"), vec![]);
    assert_eq!(click("none-selected", "Close all (0)"), vec![]);
}

#[test]
fn every_found_widget_is_reachable() {
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
    pixel(&mut h, sample)
}

#[test]
fn hover_colours_match_customtkinter() {
    let idle = geometry(&fixture("idle"));
    let corner_free = |b: Bounds| egui::pos2(b.x as f32 + 14.0, b.y as f32 + b.h as f32 / 2.0);

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
    let none = geometry(&fixture("none-selected"));
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
    let mixed = geometry(&fixture("mixed"));
    let browse = mixed.apps[5].browse.as_ref().unwrap().rect;
    let inside = corner_free(browse);
    assert_eq!(
        pixel_after_hover("mixed", inside, inside),
        theme::BROWSE_BUTTON.hover
    );
}

#[test]
fn new_log_line_scrolls_to_the_end() {
    let pointer = egui::pos2(900.0, 300.0);
    let mut state = fixture("log-full");
    let mut h = harness(state.clone());
    h.hover_at(pointer);
    h.run();
    let at_start = h.render().unwrap();
    wheel(&h, 10.0);
    h.run();
    assert_ne!(
        h.render().unwrap(),
        at_start,
        "the wheel did not scroll the log up"
    );

    // A new line arrives while the log is scrolled up.
    state.log.push(LogLine {
        level: LogLevel::Info,
        text: "[19:45:07] One more line".into(),
    });
    h.state_mut().state = state.clone();
    h.run();

    // The end of the same log: a window on it scrolled down as far as the wheel goes.
    let mut end = harness(state);
    end.hover_at(pointer);
    end.run();
    for _ in 0..10 {
        wheel(&end, -10.0);
        end.run();
    }
    assert_eq!(
        h.render().unwrap(),
        end.render().unwrap(),
        "the log did not jump to its end on a new line"
    );
}

// ---------------------------------------------------------------------------------------------
// Press, flash and the redraw after a toggle.

#[test]
fn a_press_acts_before_the_release() {
    let geom = geometry(&fixture("mixed"));
    let launch = geom.launch.rect.rect().center();
    let mut h = live(fixture("mixed"));
    h.hover_at(launch);
    button_event(&h, launch, true);
    h.step();
    assert_eq!(
        h.state().actions,
        vec![UiAction::Launch],
        "nothing on press"
    );
    // Released a second later, far away: still the one action, as in Tk.
    h.run_steps(40);
    h.hover_at(egui::pos2(20.0, 600.0));
    button_event(&h, egui::pos2(20.0, 600.0), false);
    h.run();
    assert_eq!(h.state().actions, vec![UiAction::Launch]);
}

#[test]
fn a_pressed_button_flashes_its_normal_colour() {
    let geom = geometry(&fixture("idle"));
    let b = geom.select_all.rect;
    let inside = egui::pos2(b.x as f32 + 14.0, b.y as f32 + b.h as f32 / 2.0);
    let mut h = live(fixture("idle"));
    h.hover_at(inside);
    h.step();
    assert_eq!(pixel(&mut h, inside), theme::GREY_BUTTON.hover);
    button_event(&h, inside, true);
    button_event(&h, inside, false);
    h.step();
    assert_eq!(pixel(&mut h, inside), theme::GREY_BUTTON.fill, "no flash");
    h.step();
    h.step();
    assert_eq!(
        pixel(&mut h, inside),
        theme::GREY_BUTTON.fill,
        "flash under 100 ms"
    );
    h.run_steps(4);
    assert_eq!(
        pixel(&mut h, inside),
        theme::GREY_BUTTON.hover,
        "hover not back"
    );
}

#[test]
fn a_toggle_redraws_without_hover_until_the_pointer_comes_back() {
    let geom = geometry(&fixture("idle"));
    let away = egui::pos2(500.0, 400.0);

    // Unchecking: the box shows the card colour, not the hover blue, under the pointer.
    let b = geom.apps[0].parts.toggle_box;
    let centre = egui::pos2(b.x as f32 + 12.0, b.y as f32 + 12.0);
    let mut h = live(fixture("idle"));
    h.hover_at(centre);
    button_event(&h, centre, true);
    button_event(&h, centre, false);
    h.run();
    assert!(!h.state().state.apps[0].checked);
    assert_eq!(pixel(&mut h, centre), theme::CARD_FILL);
    h.hover_at(away);
    h.run();
    h.hover_at(centre);
    h.run();
    assert_eq!(pixel(&mut h, centre), theme::CHECK_HOVER);

    // Selecting a radio: its ring shows the selected blue, not the hover colour.
    let r = geom.games[3].parts.toggle_box;
    let centre = egui::pos2(r.x as f32 + 11.0, r.y as f32 + 11.0);
    let ring = egui::pos2(r.x as f32 + 1.0, r.y as f32 + 11.0);
    let mut h = live(fixture("idle"));
    h.hover_at(centre);
    button_event(&h, centre, true);
    button_event(&h, centre, false);
    h.run();
    assert_eq!(
        h.state().state.selected_game.as_deref(),
        Some("Automobilista 2")
    );
    assert_eq!(pixel(&mut h, ring), theme::RADIO_FILL);
    h.hover_at(away);
    h.run();
    h.hover_at(centre);
    h.run();
    assert_eq!(pixel(&mut h, ring), theme::RADIO_HOVER);
}

#[test]
fn the_gap_beside_a_checkbox_is_dead() {
    let geom = geometry(&fixture("none-selected"));
    let b = geom.apps[0].parts.toggle_box;
    let centre = egui::pos2(b.x as f32 + 12.0, b.y as f32 + 12.0);
    let gap = egui::pos2(b.right() as f32 + 3.0, b.y as f32 + 12.0);
    let mut h = live(fixture("none-selected"));
    h.hover_at(gap);
    h.run();
    assert_eq!(
        pixel(&mut h, centre),
        theme::CARD_FILL,
        "the gap shows hover"
    );
    button_event(&h, gap, true);
    button_event(&h, gap, false);
    h.run();
    assert_eq!(h.state().actions, vec![]);
}

#[test]
fn the_text_cursor_shows_over_the_whole_log() {
    let geom = geometry(&fixture("idle"));
    let text = geom.log.text;
    // Below the last line of text, and on the Text widget's padding.
    for p in [
        egui::pos2(text.x as f32 + 200.0, text.bottom() as f32 - 20.0),
        egui::pos2(text.x as f32 + 1.0, text.y as f32 + 1.0),
    ] {
        let mut h = live(fixture("idle"));
        h.hover_at(p);
        h.run();
        assert_eq!(
            h.output().platform_output.cursor_icon,
            egui::CursorIcon::Text,
            "at {p:?}"
        );
    }
}

#[test]
fn a_wheel_notch_jumps_40_px_at_once() {
    let geom = geometry(&fixture("log-full"));
    let inner = geom.log.inner;
    let mut h = live(fixture("log-full"));
    h.hover_at(egui::pos2(900.0, 300.0));
    h.run();
    let before = h.render().unwrap();
    wheel(&h, 1.0);
    h.step();
    let after = h.render().unwrap();
    h.run_steps(10);
    assert_eq!(h.render().unwrap(), after, "the wheel kept scrolling");
    // The text moved down by 40 px: every row of the view now shows what was 40 px above.
    let x0 = inner.x as u32;
    let x1 = inner.right() as u32;
    for y in inner.y as u32 + 40..inner.bottom() as u32 {
        for x in x0..x1 {
            assert_eq!(
                after.get_pixel(x, y),
                before.get_pixel(x, y - 40),
                "at {x},{y}"
            );
        }
    }
    assert_ne!(before, after);
}
fn copied(h: &mut Harness<'static, Probe>) -> Option<String> {
    h.event(egui::Event::Copy);
    h.step();
    h.output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

#[test]
fn log_selection_copies_across_lines_and_survives_scrolling() {
    let state = fixture("log-full");
    let geom = geometry(&fixture("log-full"));
    let inner = geom.log.inner;
    let n = state.log.len();
    let (a, b) = (&state.log[n - 3].text, &state.log[n - 2].text);
    // Scrolled to the end: the empty row after the last newline is at the bottom, the last
    // line above it, and these two above that (each fits on one row).
    assert!(a.len() < 60 && b.len() < 60 && state.log[n - 1].text.len() < 60);
    let row_y = |rows_above_bottom: i32| (inner.bottom() - 17 * rows_above_bottom + 8) as f32;
    // Consolas 14 advances 8 px; 1 px into a cell is that cell's left edge.
    let at = |ch: i32| (inner.x + 8 * ch + 1) as f32;
    let start = egui::pos2(at(11), row_y(4));
    let end = egui::pos2(at(5), row_y(3));

    let mut h = live(state.clone());
    h.hover_at(start);
    h.step();
    button_event(&h, start, true);
    h.step();
    h.hover_at(end);
    h.step();
    button_event(&h, end, false);
    h.step();
    let want = format!("{}\n{}", &a[11..], &b[..5]);
    assert_eq!(copied(&mut h).as_deref(), Some(want.as_str()));

    // Scrolled until both lines are out of view, the selection is still there to copy.
    h.hover_at(egui::pos2(900.0, 300.0));
    for _ in 0..4 {
        wheel(&h, 1.0);
        h.step();
    }
    assert_eq!(copied(&mut h).as_deref(), Some(want.as_str()));
    // Pressing a control does not clear it either; Tk keeps the selection.
    assert_eq!(h.state().actions, vec![]);
}
