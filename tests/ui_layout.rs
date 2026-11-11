//! The layout at 1200x700 reproduces every measured element of every fixture state in
//! `docs/rewrite/ui-spec.json`: widget rects, painted boxes, text placement, the log's wrapping,
//! scroll position and scrollbar. It reads `ui::sections::geometry`, the same geometry `draw` paints.

use iracing_companion_launcher::contract::ViewState;
use iracing_companion_launcher::ui::layout::{self, Bounds};
use iracing_companion_launcher::ui::sections::{self, Geometry, TextGeom};
use iracing_companion_launcher::ui::{self as ui, theme};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const WINDOW: egui::Vec2 = egui::vec2(1200.0, 700.0);

fn repo_file(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn fixtures() -> BTreeMap<String, ViewState> {
    serde_json::from_str(&repo_file("docs/rewrite/fixtures.json")).unwrap()
}

fn spec() -> Value {
    serde_json::from_str(&repo_file("docs/rewrite/ui-spec.json")).unwrap()
}

fn bounds(v: &Value) -> Bounds {
    let n = |k: &str| v[k].as_i64().unwrap_or_else(|| panic!("{k} in {v}")) as i32;
    Bounds::new(n("x"), n("y"), n("w"), n("h"))
}

fn slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "_")
}

/// Run `f` inside a frame of a context with the app's fonts installed.
fn with_ctx<R>(f: impl FnOnce(&egui::Context) -> R) -> R {
    let ctx = egui::Context::default();
    ui::install_fonts(&ctx);
    let mut f = Some(f);
    let mut out = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        if let Some(f) = f.take() {
            out = Some(f(ui.ctx()));
        }
    });
    // No renderer here: the font atlas upload is dropped on purpose.
    output.textures_delta.clear();
    out.unwrap()
}

/// Every element the spec names, from the geometry.
fn elements(g: &Geometry, state: &ViewState) -> BTreeMap<String, Bounds> {
    let mut m = BTreeMap::new();
    let l = &g.layout;
    m.insert("header".into(), l.header);
    m.insert("header.title".into(), g.title.label);
    m.insert("footer".into(), l.footer);
    m.insert("footer.label".into(), g.footer.label);
    m.insert("content".into(), l.content);

    m.insert("games.column".into(), l.games.column);
    m.insert("games.heading_row".into(), l.games.heading_row);
    m.insert("games.heading".into(), g.games_heading.label);
    m.insert("games.cards".into(), l.games.cards_frame);
    let none = &g.none_card;
    m.insert("games.none.card".into(), none.card);
    m.insert("games.none.radio".into(), none.parts.toggle);
    m.insert("games.none.radio.box".into(), none.parts.toggle_box);
    m.insert("games.none.label".into(), none.parts.name);
    for (game, geom) in state.games.iter().zip(&g.games) {
        let p = format!("games.{}", slug(&game.name));
        m.insert(format!("{p}.card"), geom.card);
        m.insert(format!("{p}.radio"), geom.parts.toggle);
        m.insert(format!("{p}.radio.box"), geom.parts.toggle_box);
        m.insert(format!("{p}.name"), geom.parts.name);
        match &geom.browse {
            Some(b) => {
                m.insert(format!("{p}.browse"), b.rect);
                m.insert(format!("{p}.browse.text"), b.label);
            }
            None => {
                m.insert(format!("{p}.dot"), geom.parts.trailing);
            }
        }
    }

    m.insert("apps.column".into(), l.apps.column);
    m.insert("apps.heading_row".into(), l.apps.heading_row);
    m.insert("apps.heading".into(), g.apps_heading.label);
    m.insert("apps.select_all".into(), g.select_all.rect);
    m.insert("apps.select_all.text".into(), g.select_all.label);
    m.insert("apps.cards".into(), l.apps.cards_frame);
    for (app, geom) in state.apps.iter().zip(&g.apps) {
        let p = format!("apps.{}", slug(&app.name));
        m.insert(format!("{p}.card"), geom.card);
        m.insert(format!("{p}.checkbox"), geom.parts.toggle);
        m.insert(format!("{p}.checkbox.box"), geom.parts.toggle_box);
        m.insert(format!("{p}.text_frame"), geom.parts.text_frame);
        m.insert(format!("{p}.name"), geom.parts.name);
        m.insert(format!("{p}.subtitle"), geom.parts.subtitle.unwrap());
        match &geom.browse {
            Some(b) => {
                m.insert(format!("{p}.browse"), b.rect);
                m.insert(format!("{p}.browse.text"), b.label);
            }
            None => {
                m.insert(format!("{p}.dot"), geom.parts.trailing);
            }
        }
    }

    m.insert("log.column".into(), l.log.column);
    m.insert("log.heading".into(), g.log_heading.label);
    m.insert("log.box".into(), l.log.log_box);
    m.insert("log.text".into(), g.log.text);
    if let Some(bar) = g.log.scrollbar {
        m.insert("log.scrollbar".into(), bar);
    }

    m.insert("buttons.row".into(), g.buttons.row);
    m.insert("buttons.launch".into(), g.launch.rect);
    m.insert("buttons.launch.text".into(), g.launch.label);
    m.insert("buttons.close".into(), g.close.rect);
    m.insert("buttons.close.text".into(), g.close.label);
    m
}

/// Every text label the spec names, except the status dots (painted as disks, checked as
/// elements and by `ui_diff.py`).
fn texts(g: &Geometry, state: &ViewState) -> BTreeMap<String, TextGeom> {
    let mut m = BTreeMap::new();
    m.insert("header.title".into(), g.title.clone());
    m.insert("footer.label".into(), g.footer.clone());
    m.insert("games.heading".into(), g.games_heading.clone());
    m.insert("apps.heading".into(), g.apps_heading.clone());
    m.insert("log.heading".into(), g.log_heading.clone());
    m.insert("games.none.label".into(), g.none_card.name.clone());
    m.insert("apps.select_all.text".into(), g.select_all.text.clone());
    m.insert("buttons.launch.text".into(), g.launch.text.clone());
    m.insert("buttons.close.text".into(), g.close.text.clone());
    for (game, geom) in state.games.iter().zip(&g.games) {
        let p = format!("games.{}", slug(&game.name));
        m.insert(format!("{p}.name"), geom.name.clone());
        if let Some(b) = &geom.browse {
            m.insert(format!("{p}.browse.text"), b.text.clone());
        }
    }
    for (app, geom) in state.apps.iter().zip(&g.apps) {
        let p = format!("apps.{}", slug(&app.name));
        m.insert(format!("{p}.name"), geom.name.clone());
        m.insert(format!("{p}.subtitle"), geom.subtitle.clone());
        if let Some(b) = &geom.browse {
            m.insert(format!("{p}.browse.text"), b.text.clone());
        }
    }
    m
}

/// The rect Tk reports for a label's text: the text block clipped to its label. Labels inside
/// buttons report the label itself, padding included.
fn text_rect(t: &TextGeom, in_button: bool) -> Bounds {
    if in_button {
        return t.label;
    }
    let h = t.lines * t.font.linespace();
    let top = t.top.max(t.label.y);
    let bottom = (t.top + h).min(t.label.bottom());
    Bounds::new(t.left, top, t.width, bottom - top)
}

#[test]
fn layout_matches_spec_for_every_state() {
    let spec = spec();
    let mut failures = Vec::new();
    let mut checked = 0;
    for (name, state) in fixtures() {
        let st = &spec["states"][&name];
        assert!(st.is_object(), "state {name} missing from ui-spec.json");
        let (geom, rows) = with_ctx(|ctx| {
            let geom = sections::geometry(ctx, &state, WINDOW);
            let rows = ui::log::row_texts(ctx, &state, geom.log.inner.w as f32);
            (geom, rows)
        });
        let ours = elements(&geom, &state);

        // Widget rects and painted boxes.
        for (el, v) in st["elements"].as_object().unwrap() {
            let want = bounds(&v["rect"]);
            let Some(got) = ours.get(el) else {
                failures.push(format!("{name}: {el} not produced"));
                continue;
            };
            checked += 1;
            if *got != want {
                failures.push(format!("{name}: {el} is {got:?}, spec {want:?}"));
            }
            if let Some(p) = v.get("painted") {
                let painted = bounds(p);
                let rule = got.painted();
                // An odd-width radius-12 button shows its corner glyphs' overhang in the last
                // column, so its measured box is one wider than what the floor rule paints.
                let overhang =
                    painted == Bounds::new(rule.x, rule.y, rule.w + 1, rule.h) && got.w % 2 == 1;
                if painted != rule && !overhang {
                    failures.push(format!("{name}: {el} paints {rule:?}, spec {painted:?}"));
                }
            }
        }
        for el in ours.keys() {
            if st["elements"].get(el).is_none() {
                failures.push(format!("{name}: {el} produced but not in the spec"));
            }
        }

        // Text placement: Tk's text rect, top, baselines and advance widths.
        for (el, t) in texts(&geom, &state) {
            let v = &st["text"][&el];
            if !v.is_object() {
                failures.push(format!("{name}: text {el} not in the spec"));
                continue;
            }
            checked += 1;
            let in_button = el.ends_with(".text");
            let want_rect = bounds(&v["rect"]);
            let got_rect = text_rect(&t, in_button);
            if got_rect != want_rect {
                failures.push(format!(
                    "{name}: text {el} rect {got_rect:?}, spec {want_rect:?}"
                ));
            }
            let top = v["text_top_y"].as_i64().unwrap() as i32;
            if t.top != top {
                failures.push(format!("{name}: text {el} top {}, spec {top}", t.top));
            }
            let baselines: Vec<i32> = v["baseline_y"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_i64().unwrap() as i32)
                .collect();
            if t.baselines() != baselines {
                failures.push(format!(
                    "{name}: text {el} baselines {:?}, spec {baselines:?}",
                    t.baselines()
                ));
            }
            let widths: Vec<i64> = v["advance_width"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_i64().unwrap())
                .collect();
            let max = *widths.iter().max().unwrap() as i32;
            if t.width != max {
                failures.push(format!(
                    "{name}: text {el} width {}, spec {widths:?}",
                    t.width
                ));
            }
        }

        // The log: inner area, wrapping and the scroll position after see("end").
        let log = &st["log"];
        let inner = geom.log.inner;
        let n = |k: &str| log[k].as_i64().unwrap() as i32;
        if (inner.x, inner.y, inner.w)
            != (
                n("text_inner_left_x"),
                n("text_inner_top_y"),
                n("text_inner_width"),
            )
        {
            failures.push(format!("{name}: log inner {inner:?}, spec {log}"));
        }
        let shown = log["scrollbar_shown"].as_bool().unwrap_or(false);
        if geom.log.scrollbar.is_some() != shown {
            failures.push(format!(
                "{name}: log scrollbar {:?}, spec shown={shown}",
                geom.log.scrollbar
            ));
        }
        let pitch = theme::LOG_FONT.linespace();
        let offset = (rows.len() as i32 * pitch - inner.h).max(0);
        for line in log["lines"].as_array().unwrap() {
            checked += 1;
            let y = line["rect"]["y"].as_i64().unwrap() as i32;
            let row = (y - inner.y + offset) / pitch;
            let text = line["text"].as_str().unwrap();
            let width = line["rect"]["w"].as_i64().unwrap() as usize;
            // The capture's `text` for a display line that wraps omits its last character;
            // its rect width still counts it (62 characters of 8 px = 496).
            let ours = rows.get(row as usize);
            let same = ours.is_some_and(|r| r.starts_with(text) && r.chars().count() * 8 == width);
            if (y - inner.y + offset) % pitch != 0 || !same {
                failures.push(format!(
                    "{name}: log line {} {text:?} ({width} px) at y {y} is not row {row} ({ours:?})",
                    line["index"]
                ));
            }
            let baseline = line["baseline_y"].as_i64().unwrap() as i32;
            if baseline != y + theme::LOG_FONT.ascent {
                failures.push(format!(
                    "{name}: log line {} baseline {baseline}",
                    line["index"]
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked > 500, "only {checked} checks ran");
}

#[test]
fn packer_reproduces_the_root_overflow() {
    // 713 px requested in 700: the button row, packed last, absorbs the difference.
    let l = layout::layout(WINDOW, 7, 5);
    assert_eq!(l.content, Bounds::new(15, 85, 1170, 485));
    assert_eq!(l.footer, Bounds::new(15, 667, 1170, 28));
    let row = l.buttons((183, 23), (93, 23), 12);
    assert_eq!(row.launch.h, 47);
    assert_eq!(row.launch.y, 610);
    assert_eq!(row.launch.painted().h, 46);
}

#[test]
fn layout_flexes_with_the_window() {
    // Taller and wider than the Python window: the content column takes the extra height,
    // the columns share the extra width, and the button row gets its full 60 px.
    let l = layout::layout(egui::vec2(1400.0, 800.0), 7, 5);
    assert_eq!(l.header, Bounds::new(0, 0, 1400, 70));
    assert_eq!(l.footer.bottom(), 795);
    let row = l.buttons((183, 23), (93, 23), 12);
    assert_eq!(row.launch.h, 60);
    assert_eq!(row.row.bottom() + 10, l.footer.y);
    assert!(l.content.h > 485);
    let widths = [l.games.column.w, l.apps.column.w, l.log.column.w];
    assert_eq!(widths.iter().sum::<i32>() + 20, l.content.w);
    assert!(widths[0] > 316 && widths[2] > 517, "{widths:?}");
    // Cards keep their height and pitch; more apps than fit still get a card each.
    let many = layout::layout(egui::vec2(1400.0, 800.0), 12, 2);
    assert_eq!(many.apps.cards.len(), 12);
    assert_eq!(many.apps.cards[1].y - many.apps.cards[0].y, 65);
    assert_eq!(many.games.cards.len(), 3);
}

#[test]
fn game_names_wrap_like_python() {
    assert_eq!(
        sections::wrap_game_name("Assetto Corsa Competizione"),
        "Assetto Corsa\nCompetizione"
    );
    assert_eq!(
        sections::wrap_game_name("Automobilista 2"),
        "Automobilista 2"
    );
    assert_eq!(
        sections::wrap_game_name("ABCDEFGHIJKLMNOPQRSTU"),
        "ABCDEFGHIJKLMNOPQR\nSTU"
    );
    assert_eq!(
        sections::wrap_game_name("Exactly eighteen!!"),
        "Exactly eighteen!!"
    );
}

/// GDI advances the pen by whole device pixels, so at any display scale every glyph starts on a
/// device pixel and each step is the glyph's own advance rounded at that scale: no gap opens or
/// closes between letters ("Not installed", not "Notinstalled").
#[test]
fn text_advances_are_whole_device_pixels_at_every_scale() {
    for ppp in [1.0f32, 1.25, 1.5, 2.0] {
        let ctx = egui::Context::default();
        ui::install_fonts(&ctx);
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        let mut output = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx();
            assert_eq!(ctx.pixels_per_point(), ppp);
            for (text, font) in [
                ("Not installed", theme::SUBTITLE_FONT),
                ("Tobias Termeczky", theme::FOOTER_FONT),
                ("Launch with iRacing (5)", theme::HEADING_FONT),
                ("MarvinsAIRA Refactored", theme::NAME_FONT),
                ("[19:45:07] Closing Fanatec...", theme::LOG_FONT),
            ] {
                let run = ui::text::Run {
                    text,
                    color: egui::Color32::WHITE,
                };
                let galley = ui::text::layout(ctx, &[run], &font, None);
                let glyphs = &galley.rows[0].row.glyphs;
                let mut pen = 0.0f32;
                for (i, g) in glyphs.iter().enumerate() {
                    let device = g.pos.x * ppp;
                    assert!(
                        (device - pen).abs() < 0.01,
                        "{text:?} at {ppp}: glyph {i} {:?} at {device} device px, expected {pen}",
                        g.chr
                    );
                    let linear = ctx.fonts_mut(|f| f.glyph_width(&font.id(), g.chr));
                    pen += (linear * ppp + 1e-3).round();
                }
                // The width Tk measures is the same pen, in whole points.
                let width = ui::text::width(ctx, text, &font);
                assert_eq!(width, (pen / ppp - 1e-3).ceil() as i32, "{text:?} at {ppp}");
            }
        });
        output.textures_delta.clear();
    }
}
