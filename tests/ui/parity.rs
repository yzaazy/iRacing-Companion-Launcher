//! Every fixture state renders like the Python reference screenshot (`parity_check` says how that
//! is judged) and like its committed golden render; and the checker passes a copy of the
//! reference and fails each kind of planted change in its own class only.

use crate::parity_check::{self as check, Box4, CARD_BG, Diff, TOLERANCE, hex, rect};
use crate::support::{STATES, fixture, render, repo_path, spec};
use egui_kittest::SnapshotOptions;
use image::{Rgb, RgbImage};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The reference screenshots, read once.
fn reference(state: &str) -> &'static RgbImage {
    static REFS: OnceLock<HashMap<&'static str, RgbImage>> = OnceLock::new();
    let refs = REFS.get_or_init(|| {
        STATES
            .iter()
            .map(|&s| {
                let path = repo_path(&format!("docs/rewrite/reference/{s}.png"));
                let img = image::open(&path)
                    .unwrap_or_else(|e| panic!("missing reference {}: {e}", path.display()));
                (s, img.to_rgb8())
            })
            .collect()
    });
    &refs[state]
}

fn compare(state: &str, new: &RgbImage) -> Result<Diff, String> {
    check::compare(
        state,
        check::state_spec(spec(), state),
        reference(state),
        new,
        TOLERANCE,
    )
}

/// The golden renders in `tests/snapshots/` are renders that pass the parity check. The
/// threshold is egui_kittest's default per-pixel YIQ distance, 0.6: it lets a grey move by one
/// level and nothing more, and no pixel may exceed it. Re-approve with `UPDATE_SNAPSHOTS=1` only
/// after the parity check passes on the new render.
fn golden_options() -> SnapshotOptions {
    SnapshotOptions::new()
        .threshold(0.6)
        .max_failed_pixels(0)
        .output_path(repo_path("tests/snapshots"))
}

/// Render `state`, write it to `target/ui-snapshots/`, and check it with `check_render`.
fn check_state(state: &str) {
    let image = render(&fixture(state));
    let out = repo_path("target/ui-snapshots");
    std::fs::create_dir_all(&out).unwrap();
    image.save(out.join(format!("{state}.png"))).unwrap();
    check_render(state, &image, &repo_path("target/ui-diff"));
}

/// Compare `image` with the reference of `state`, writing `<diff_dir>/<state>.png` when parity
/// fails; only once parity passes, compare it with its golden render. A render that fails parity
/// therefore never reaches the golden step, and `UPDATE_SNAPSHOTS=1` cannot approve it.
fn check_render(state: &str, image: &image::RgbaImage, diff_dir: &std::path::Path) {
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
    let diff_path = diff_dir.join(format!("{state}.png"));
    let _ = std::fs::remove_file(&diff_path);
    let diff = compare(state, &rgb).unwrap_or_else(|e| panic!("{state}: {e}"));
    if !diff.ok() {
        std::fs::create_dir_all(diff_dir).unwrap();
        check::diff_image(&diff, check::state_spec(spec(), state), reference(state))
            .save(&diff_path)
            .unwrap();
        panic!(
            "{state}:\nparity with the Python reference fails, see {}:\n{}\n\
             (the golden render was not compared)",
            diff_path.display(),
            check::report(&diff, 10)
        );
    }
    if let Err(e) = egui_kittest::try_image_snapshot_options(image, state, &golden_options()) {
        panic!("{state}:\ngolden render: {e}");
    }
}

/// A render that fails parity fails with the parity report and stops before the golden step,
/// which would otherwise also fail on it (or, under `UPDATE_SNAPSHOTS=1`, approve it).
#[test]
fn parity_failure_stops_before_the_golden_render() {
    let mut bad = image::DynamicImage::ImageRgb8(reference("idle").clone()).to_rgba8();
    for y in 300..320 {
        for x in 300..340 {
            bad.put_pixel(x, y, image::Rgba([200, 0, 200, 255]));
        }
    }
    let diff_dir = repo_path("target/ui-diff/self-test");
    let failed = std::panic::catch_unwind(|| check_render("idle", &bad, &diff_dir))
        .expect_err("a planted block must fail");
    let message = failed.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(
        message.contains("parity with the Python reference fails")
            && message.contains("the golden render was not compared")
            && !message.contains("golden render:"),
        "{message}"
    );
    assert!(diff_dir.join("idle.png").exists());
}

#[test]
fn all_missing_matches_the_reference() {
    check_state("all-missing");
}

#[test]
fn idle_matches_the_reference() {
    check_state("idle");
}

#[test]
fn log_full_matches_the_reference() {
    check_state("log-full");
}

#[test]
fn mixed_matches_the_reference() {
    check_state("mixed");
}

#[test]
fn none_selected_matches_the_reference() {
    check_state("none-selected");
}

// ---------------------------------------------------------------------------------------------
// The checker itself, against the reference set: a copy passes, each planted change fails its
// own class only.

fn idle_spec() -> &'static serde_json::Value {
    check::state_spec(spec(), "idle")
}

/// A spec rect of the idle state: `elements.<name>.<key>` or `text.<name>.<key>`.
fn spec_rect(section: &str, name: &str, key: &str) -> (i32, i32, i32, i32) {
    rect(idle_spec()[section][name].get(key)).unwrap_or_else(|| panic!("{section}.{name}.{key}"))
}

/// The idle reference with `edit` applied, compared with the reference.
fn altered(edit: impl FnOnce(&mut RgbImage)) -> Diff {
    let mut img = reference("idle").clone();
    edit(&mut img);
    let diff = compare("idle", &img).unwrap();
    println!("{}", check::report(&diff, 10));
    diff
}

fn fill(img: &mut RgbImage, b: Box4, color: [u8; 3]) {
    for y in b.y0.max(0)..b.y1.min(img.height() as i32) {
        for x in b.x0.max(0)..b.x1.min(img.width() as i32) {
            img.put_pixel(x as u32, y as u32, Rgb(color));
        }
    }
}

#[test]
fn checker_passes_a_copy_of_every_reference() {
    for state in STATES {
        let diff = compare(state, reference(state)).unwrap();
        assert!(diff.ok(), "{}", check::report(&diff, 10));
        assert!(
            diff.zones > 90 && diff.text_items > 40,
            "{state}: too few checks"
        );
    }
}

#[test]
fn checker_fails_a_size_mismatch() {
    let small = RgbImage::new(100, 100);
    let err = compare("mixed", &small).err().unwrap();
    assert!(err.contains("size differs"), "{err}");
}

#[test]
fn checker_finds_a_planted_block_in_flat_only() {
    // A 20x10 block inside the Fanatec card, right of its text.
    let card = spec_rect("elements", "apps.fanatec.card", "rect");
    let name = spec_rect("text", "apps.fanatec.name", "ink");
    let (bx, by) = (name.0 + name.2 + 40, card.1 + 20);
    let block = Box4::new(bx, by, bx + 20, by + 10);
    let diff = altered(|img| fill(img, block, [200, 0, 200]));
    assert_eq!(diff.flat.px, 200);
    assert_eq!(diff.flat.bbox, Some(block));
    assert!(
        !diff.flat.elements.is_empty()
            && diff
                .flat
                .elements
                .iter()
                .all(|o| o.0.starts_with("apps.fanatec")),
        "attributed to {:?}",
        diff.flat.elements
    );
    assert!(diff.edges_ok() && diff.text_ok(), "only flat may fail");
}

#[test]
fn checker_finds_a_shifted_text_in_text_only() {
    // The Fanatec name moved 2 px right, which stays inside its region on the left.
    let (x, y, w, h) = spec_rect("text", "apps.fanatec.name", "ink");
    let bg = hex(idle_spec()["text"]["apps.fanatec.name"]["background"]
        .as_str()
        .unwrap());
    let diff = altered(|img| {
        let (x0, y0) = (x - 1, y - 1);
        let patch =
            image::imageops::crop_imm(img, x0 as u32, y0 as u32, (w + 2) as u32, (h + 2) as u32)
                .to_image();
        fill(img, Box4::new(x0, y0, x + w + 1, y + h + 1), bg);
        image::imageops::replace(img, &patch, i64::from(x0 + 2), i64::from(y0));
    });
    assert_eq!(diff.text_failed_names(), ["apps.fanatec.name"]);
    assert!(diff.flat_ok() && diff.edges_ok(), "only text may fail");
}

#[test]
fn checker_finds_an_erased_dot_in_edges_only() {
    let dot = spec_rect("elements", "apps.fanatec.dot", "ink");
    let diff = altered(|img| fill(img, check_box(dot), CARD_BG));
    assert_eq!(diff.edges_failed_names(), ["apps.fanatec.dot"]);
    assert!(diff.flat_ok() && diff.text_ok(), "only edges may fail");
}

#[test]
fn checker_finds_a_recoloured_checkbox_in_edges_only() {
    let (x, y, _, _) = spec_rect("elements", "apps.crew_chief_v4.checkbox.box", "rect");
    let diff = altered(|img| fill(img, Box4::new(x + 9, y + 2, x + 15, y + 5), [255, 0, 0]));
    assert_eq!(
        diff.edges_failed_names(),
        ["apps.crew_chief_v4.checkbox.box"]
    );
    assert!(diff.flat_ok() && diff.text_ok(), "only edges may fail");
}

fn check_box((x, y, w, h): (i32, i32, i32, i32)) -> Box4 {
    Box4::new(x, y, x + w, y + h)
}

/// Repaint the text in `region` in `color`, keeping each pixel's coverage over `bg`.
fn recolor(img: &mut RgbImage, region: (i32, i32, i32, i32), bg: [u8; 3], color: [u8; 3]) {
    let full = (0..3)
        .map(|i| (i32::from(color[i]) - i32::from(bg[i])).abs())
        .max()
        .unwrap();
    let b = check_box(region);
    for y in b.y0.max(0)..b.y1.min(img.height() as i32) {
        for x in b.x0.max(0)..b.x1.min(img.width() as i32) {
            let p = img.get_pixel(x as u32, y as u32).0;
            let d = (0..3)
                .map(|i| (i32::from(p[i]) - i32::from(bg[i])).abs())
                .max()
                .unwrap();
            let a = (f64::from(d) / f64::from(full)).min(1.0);
            let q = [0, 1, 2].map(|i| {
                (f64::from(bg[i]) + a * (f64::from(color[i]) - f64::from(bg[i]))).round_ties_even()
                    as u8
            });
            img.put_pixel(x as u32, y as u32, Rgb(q));
        }
    }
}

fn assert_recolour_fails_text_only(target: &str, bg: [u8; 3]) {
    let region = check::text_regions(idle_spec())[target];
    let diff = altered(|img| recolor(img, region, bg, [239, 83, 80]));
    assert_eq!(diff.text_failed_names(), [target]);
    assert!(diff.flat_ok() && diff.edges_ok(), "only text may fail");
}

#[test]
fn checker_finds_a_recoloured_name_in_text_only() {
    let bg = hex(idle_spec()["text"]["apps.crew_chief_v4.name"]["background"]
        .as_str()
        .unwrap());
    assert_recolour_fails_text_only("apps.crew_chief_v4.name", bg);
}

#[test]
fn checker_finds_a_recoloured_log_line_in_text_only() {
    // The first log line with text on it (not the divider, a 1 px rule).
    let line = check::text_items(idle_spec())
        .into_iter()
        .find(|t| t.name.starts_with("log.line") && t.region.3 > 6)
        .unwrap();
    assert_recolour_fails_text_only(&line.name, line.bg);
}

#[test]
fn checker_finds_a_raised_top_edge_in_text_only() {
    // One ink pixel 2 px above the Trading Paints name, as a taller glyph would add.
    let (x, y, _, _) = spec_rect("text", "apps.trading_paints.name", "ink");
    let diff = altered(|img| img.put_pixel((x + 3) as u32, (y - 2) as u32, Rgb([255, 255, 255])));
    assert_eq!(diff.text_failed_names(), ["apps.trading_paints.name"]);
    assert!(diff.flat_ok() && diff.edges_ok(), "only text may fail");
}
