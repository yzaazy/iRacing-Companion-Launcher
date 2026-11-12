//! Compares a render with the Python reference screenshot of the same state, in the three classes
//! of the parity criteria (`docs/architecture.md`, "Tests"):
//!
//! - **flat**: every pixel outside the two classes below. Passes with zero pixels whose channels
//!   differ by more than [`TOLERANCE`].
//! - **edges**: the rounded-corner zones of cards, buttons, the log box and the scrollbar thumb,
//!   plus every checkbox, radio button and status dot. A zone passes when the bounding box of its
//!   shape (pixels that differ from the zone's background) is the same in both images and every
//!   differing pixel lies on an edge of the reference shape (its 3x3 neighbourhood in the
//!   reference is not uniform): anti-aliasing only.
//! - **text**: every text element and log line in `ui-spec.json`. Its ink box is measured the same
//!   way in both images, inside its text region; it passes when left edge, top and bottom (the
//!   baseline, or the descenders where the text has them) are within 1 px, the right edge within
//!   2 px, and its ink colour within 16 per channel. Glyph pixels themselves are free to differ.
//!
//!   The ink colour is fringe-proof, because the reference is drawn with ClearType: the direction
//!   from the background to the colour is the sum of every ink pixel's difference from the
//!   background (the red and blue fringes either side of a stem cancel out), and the distance is
//!   the farthest any pixel gets along that direction, which some pixel in a stem reaches in both
//!   renderers.
//!
//! Text regions come from `ui-spec.json` (`states.<state>.text_regions`, ink grown by 2 px); edge
//! zones are derived from the element rects. [`diff_image`] draws the reference dimmed, text
//! regions outlined in dark cyan, edge zones in dark magenta, flat differences in red, non-edge
//! differences inside zones in orange, and for each failing text element its reference ink box in
//! green and the render's in yellow.
//!
//! The criteria and thresholds are those of the Python diff tool used while porting the UI,
//! number for number.

use image::{Rgb, RgbImage};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Per-channel tolerance for flat and edges.
pub(crate) const TOLERANCE: i32 = 8;

const FLAT_COLOR: Rgb<u8> = Rgb([255, 40, 60]);
const EDGE_COLOR: Rgb<u8> = Rgb([255, 140, 0]);
const REGION_COLOR: Rgb<u8> = Rgb([0, 90, 110]);
const ZONE_COLOR: Rgb<u8> = Rgb([110, 0, 110]);
const REF_INK_COLOR: Rgb<u8> = Rgb([0, 230, 0]);
const NEW_INK_COLOR: Rgb<u8> = Rgb([255, 230, 0]);
/// Pixels; beyond this, attribution of flat differences samples.
const MAX_ATTRIBUTED: usize = 50_000;

pub(crate) const WINDOW_BG: Px = [0x24, 0x24, 0x24];
pub(crate) const CARD_BG: Px = [0x2d, 0x2d, 0x30];
/// Max channel distance from the background that counts as ink.
const INK_THRESHOLD: i32 = 48;
/// Pixels the text window extends left and right of its region.
const TEXT_SEARCH_X: i32 = 3;
const TEXT_TOL_LEFT: i32 = 1;
const TEXT_TOL_TOP: i32 = 1;
const TEXT_TOL_BOTTOM: i32 = 1;
const TEXT_TOL_RIGHT: i32 = 2;
/// Max channel difference of the ink colour.
const TEXT_TOL_COLOR: i32 = 16;
/// Min channel distance from the background that counts towards the ink colour.
const AA_THRESHOLD: i32 = 16;

pub(crate) type Px = [u8; 3];

pub(crate) fn hex(s: &str) -> Px {
    let h = s.trim_start_matches('#');
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
    [c(0), c(2), c(4)]
}

fn dist(a: Px, b: Px) -> i32 {
    (0..3)
        .map(|i| (i32::from(a[i]) - i32::from(b[i])).abs())
        .max()
        .unwrap()
}

fn px(img: &RgbImage, x: i32, y: i32) -> Px {
    img.get_pixel(x as u32, y as u32).0
}

/// `(x0, y0, x1, y1)`, the right and bottom edges exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Box4 {
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) x1: i32,
    pub(crate) y1: i32,
}

impl Box4 {
    pub(crate) fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    fn clip(self, img: &RgbImage) -> Self {
        Self::new(
            self.x0.max(0),
            self.y0.max(0),
            self.x1.min(img.width() as i32),
            self.y1.min(img.height() as i32),
        )
    }

    fn grow(&mut self, x: i32, y: i32) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x + 1);
        self.y1 = self.y1.max(y + 1);
    }

    fn pixels(self) -> impl Iterator<Item = (i32, i32)> {
        (self.y0..self.y1).flat_map(move |y| (self.x0..self.x1).map(move |x| (x, y)))
    }
}

impl std::fmt::Display for Box4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {}, {})", self.x0, self.y0, self.x1, self.y1)
    }
}

/// A box that may be absent, printed as Python printed it.
struct Opt(Option<Box4>);

impl std::fmt::Display for Opt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(b) => b.fmt(f),
            None => f.write_str("None"),
        }
    }
}

/// A spec rect `{x, y, w, h}`, when the value is a non-empty object.
pub(crate) fn rect(v: Option<&Value>) -> Option<(i32, i32, i32, i32)> {
    let v = v?.as_object().filter(|o| !o.is_empty())?;
    let n = |k: &str| v[k].as_i64().unwrap_or_else(|| panic!("{k} in {v:?}")) as i32;
    Some((n("x"), n("y"), n("w"), n("h")))
}

fn rect_box((x, y, w, h): (i32, i32, i32, i32), grow: i32) -> Box4 {
    Box4::new(x - grow, y - grow, x + w + grow, y + h + grow)
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(_) => true,
    }
}

// ------------------------------------------------------------------------------------------ zones

pub(crate) struct Zone {
    pub(crate) name: String,
    pub(crate) bx: Box4,
    pub(crate) bg: Px,
}

/// Four square zones over the corners of a box CustomTkinter paints floored to even width and
/// height, reaching 1 px outside it.
fn corner_zones(name: &str, (x, y, w, h): (i32, i32, i32, i32), radius: f32, bg: Px) -> Vec<Zone> {
    let (w, h) = (w & !1, h & !1);
    let s = radius.ceil() as i32 + 2;
    let (x0, y0) = (x - 1, y - 1);
    let (x1, y1) = (x + w + 1 - s, y + h + 1 - s);
    [
        ("tl", x0, y0),
        ("tr", x1, y0),
        ("bl", x0, y1),
        ("br", x1, y1),
    ]
    .into_iter()
    .map(|(c, x, y)| Zone {
        name: format!("{name}:{c}"),
        bx: Box4::new(x, y, x + s, y + s),
        bg,
    })
    .collect()
}

/// Every zone of the edges class for one state, from the element rects in `ui-spec.json`.
pub(crate) fn edge_zones(st: &Value) -> Vec<Zone> {
    let mut zones = Vec::new();
    for (name, e) in st["elements"].as_object().unwrap() {
        let Some(r) = rect(e.get("rect")) else {
            continue;
        };
        let leaf = name.rsplit('.').next().unwrap();
        if leaf == "card" || name == "log.box" || name == "apps.select_all" {
            zones.extend(corner_zones(name, r, 6.0, WINDOW_BG));
        } else if leaf == "browse" {
            zones.extend(corner_zones(name, r, 6.0, CARD_BG));
        } else if name == "buttons.launch" || name == "buttons.close" {
            zones.extend(corner_zones(name, r, 12.0, WINDOW_BG));
        } else if leaf == "box" && (name.ends_with(".checkbox.box") || name.ends_with(".radio.box"))
        {
            zones.push(Zone {
                name: name.clone(),
                bx: rect_box(r, 1),
                bg: CARD_BG,
            });
        } else if leaf == "dot" && e.get("ink").is_some() {
            zones.push(Zone {
                name: name.clone(),
                bx: rect_box(rect(e.get("ink")).unwrap(), 2),
                bg: CARD_BG,
            });
        } else if name == "log.scrollbar" && e.get("thumb_ink").is_some() {
            let (x, y, w, h) = rect(e.get("thumb_ink")).unwrap();
            let end = w / 2 + 2;
            zones.push(Zone {
                name: format!("{name}:top"),
                bx: Box4::new(x - 1, y - 1, x + w + 1, y + end),
                bg: CARD_BG,
            });
            zones.push(Zone {
                name: format!("{name}:bottom"),
                bx: Box4::new(x - 1, y + h - end, x + w + 1, y + h + 1),
                bg: CARD_BG,
            });
        }
    }
    zones
}

/// Bounding box of the pixels in `bx` that differ from `bg` by more than `tol`.
fn shape_bbox(img: &RgbImage, bx: Box4, bg: Px, tol: i32) -> Option<Box4> {
    let mut found: Option<Box4> = None;
    for (x, y) in bx.pixels() {
        if dist(px(img, x, y), bg) > tol {
            match &mut found {
                None => found = Some(Box4::new(x, y, x + 1, y + 1)),
                Some(b) => b.grow(x, y),
            }
        }
    }
    found
}

pub(crate) struct ZoneResult {
    pub(crate) name: String,
    pub(crate) bx: Box4,
    pub(crate) non_edge: Vec<(i32, i32)>,
    pub(crate) ref_bbox: Option<Box4>,
    pub(crate) new_bbox: Option<Box4>,
    pub(crate) ok: bool,
}

fn check_zone(zone: &Zone, reference: &RgbImage, new: &RgbImage, tol: i32) -> ZoneResult {
    let bx = zone.bx.clip(reference);
    let (w, h) = (reference.width() as i32, reference.height() as i32);
    let mut non_edge = Vec::new();
    for (x, y) in bx.pixels() {
        let c = px(reference, x, y);
        if dist(c, px(new, x, y)) <= tol {
            continue;
        }
        let edge = ((y - 1).max(0)..(y + 2).min(h)).any(|yy| {
            ((x - 1).max(0)..(x + 2).min(w)).any(|xx| dist(px(reference, xx, yy), c) > tol)
        });
        if !edge {
            non_edge.push((x, y));
        }
    }
    let ref_bbox = shape_bbox(reference, bx, zone.bg, tol);
    let new_bbox = shape_bbox(new, bx, zone.bg, tol);
    ZoneResult {
        name: zone.name.clone(),
        bx,
        ok: non_edge.is_empty() && ref_bbox == new_bbox,
        non_edge,
        ref_bbox,
        new_bbox,
    }
}

// ------------------------------------------------------------------------------------------- text

pub(crate) struct TextItem {
    pub(crate) name: String,
    pub(crate) region: (i32, i32, i32, i32),
    pub(crate) bg: Px,
}

/// The text regions of a state by name, without the status dots' glyph regions.
pub(crate) fn text_regions(st: &Value) -> HashMap<String, (i32, i32, i32, i32)> {
    st["text_regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] != "glyph-dot")
        .map(|r| {
            (
                r["name"].as_str().unwrap().to_owned(),
                rect(Some(r)).unwrap(),
            )
        })
        .collect()
}

/// Every text element and visible log line with ink: name, region, background.
pub(crate) fn text_items(st: &Value) -> Vec<TextItem> {
    let regions = text_regions(st);
    let mut items = Vec::new();
    for (name, t) in st["text"].as_object().unwrap() {
        if let Some(&region) = regions.get(name)
            && truthy(t.get("ink"))
        {
            items.push(TextItem {
                name: name.clone(),
                region,
                bg: hex(t["background"].as_str().unwrap()),
            });
        }
    }
    let log_bg = hex(st["log"]["options"]["bg"].as_str().unwrap());
    for line in st["log"]["lines"].as_array().unwrap() {
        let name = format!("log.line[{}]", line["index"].as_str().unwrap());
        if let Some(&region) = regions.get(&name)
            && truthy(line.get("ink"))
        {
            items.push(TextItem {
                name,
                region,
                bg: log_bg,
            });
        }
    }
    items
}

/// Ink bounding box inside `window`: pixels far from `bg`.
fn ink_box(img: &RgbImage, window: Box4, bg: Px) -> Option<Box4> {
    shape_bbox(img, window, bg, INK_THRESHOLD)
}

/// The text colour inside `window`, from its ink pixels (see the module doc).
fn ink_color(img: &RgbImage, window: Box4, bg: Px) -> Option<Px> {
    let diffs: Vec<[i32; 3]> = window
        .pixels()
        .map(|(x, y)| {
            let p = px(img, x, y);
            [0, 1, 2].map(|i| i32::from(p[i]) - i32::from(bg[i]))
        })
        .filter(|d| d.iter().map(|v| v.abs()).max().unwrap() > AA_THRESHOLD)
        .collect();
    let total = [0, 1, 2].map(|i| diffs.iter().map(|d| i64::from(d[i])).sum::<i64>() as f64);
    let norm = total.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm == 0.0 {
        return None;
    }
    let direction = total.map(|v| v / norm);
    let reach = diffs
        .iter()
        .map(|d| (0..3).fold(0.0, |s, i| s + f64::from(d[i]) * direction[i]))
        .fold(f64::NEG_INFINITY, f64::max);
    Some([0, 1, 2].map(|i| {
        (f64::from(bg[i]) + reach * direction[i])
            .round_ties_even()
            .clamp(0.0, 255.0) as u8
    }))
}

pub(crate) struct TextResult {
    pub(crate) name: String,
    pub(crate) reference: Option<Box4>,
    pub(crate) new: Option<Box4>,
    pub(crate) why: String,
    pub(crate) ok: bool,
}

fn hex_color(c: Px) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn check_text(item: &TextItem, reference: &RgbImage, new: &RgbImage) -> TextResult {
    let (x, y, w, h) = item.region;
    let window = Box4::new(x - TEXT_SEARCH_X, y, x + w + TEXT_SEARCH_X, y + h).clip(reference);
    let rb = ink_box(reference, window, item.bg);
    let nb = ink_box(new, window, item.bg);
    let mut res = TextResult {
        name: item.name.clone(),
        reference: rb,
        new: nb,
        why: String::new(),
        ok: true,
    };
    // Nothing measurable in the reference: nothing to hold the render to.
    let Some(rb) = rb else {
        return res;
    };
    let Some(nb) = nb else {
        res.ok = false;
        res.why = "no ink".into();
        return res;
    };
    let (dl, dt, db, dr) = (nb.x0 - rb.x0, nb.y0 - rb.y0, nb.y1 - rb.y1, nb.x1 - rb.x1);
    let rc = ink_color(reference, window, item.bg);
    let nc = ink_color(new, window, item.bg);
    let dc = match (rc, nc) {
        (Some(rc), Some(nc)) => dist(rc, nc),
        _ => 0,
    };
    let box_ok = dl.abs() <= TEXT_TOL_LEFT
        && dt.abs() <= TEXT_TOL_TOP
        && db.abs() <= TEXT_TOL_BOTTOM
        && dr.abs() <= TEXT_TOL_RIGHT;
    res.ok = box_ok && dc <= TEXT_TOL_COLOR;
    let mut why = Vec::new();
    if !box_ok {
        why.push(format!(
            "left {dl:+}, top {dt:+}, baseline {db:+}, right {dr:+}"
        ));
    }
    if dc > TEXT_TOL_COLOR {
        why.push(format!(
            "colour {} != reference {}",
            hex_color(nc.unwrap()),
            hex_color(rc.unwrap())
        ));
    }
    res.why = why.join("; ");
    res
}

// ------------------------------------------------------------------------------------------- flat

/// One element's share of the flat differences: name, pixel count, bounding box `(x, y, w, h)`.
pub(crate) type Owner = (String, usize, (i32, i32, i32, i32));

/// Group the flat difference pixels (raster indices) by the smallest element containing them.
fn attribute(on: &[usize], width: usize, elements: &Value) -> Vec<Owner> {
    let mut rects: Vec<(i64, &str, i32, i32, i32, i32)> = elements
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(name, e)| {
            let r = if truthy(e.get("visible")) {
                e.get("visible")
            } else {
                e.get("rect")
            };
            let (x, y, w, h) = rect(r)?;
            Some((
                i64::from(w) * i64::from(h),
                name.as_str(),
                x,
                y,
                x + w,
                y + h,
            ))
        })
        .collect();
    rects.sort();
    let step = (on.len() / MAX_ATTRIBUTED).max(1);
    // Owner, count, min x, min y, max x, max y; in order of first appearance.
    let mut groups: Vec<(&str, usize, i32, i32, i32, i32)> = Vec::new();
    for &i in on.iter().step_by(step) {
        let (x, y) = ((i % width) as i32, (i / width) as i32);
        let owner = rects
            .iter()
            .find(|&&(_, _, x0, y0, x1, y1)| x0 <= x && x < x1 && y0 <= y && y < y1)
            .map_or("(window)", |r| r.1);
        match groups.iter_mut().find(|g| g.0 == owner) {
            Some(g) => {
                g.1 += step;
                g.2 = g.2.min(x);
                g.3 = g.3.min(y);
                g.4 = g.4.max(x);
                g.5 = g.5.max(y);
            }
            None => groups.push((owner, step, x, y, x, y)),
        }
    }
    let mut out: Vec<Owner> = groups
        .into_iter()
        .map(|(n, c, x0, y0, x1, y1)| (n.to_owned(), c, (x0, y0, x1 - x0 + 1, y1 - y0 + 1)))
        .collect();
    out.sort_by_key(|o| std::cmp::Reverse(o.1));
    out
}

// ------------------------------------------------------------------------------------------ state

pub(crate) struct Flat {
    pub(crate) px: usize,
    pub(crate) bbox: Option<Box4>,
    pub(crate) elements: Vec<Owner>,
    /// Raster indices of the differing pixels.
    pub(crate) on: Vec<usize>,
}

pub(crate) struct Diff {
    pub(crate) state: String,
    pub(crate) flat: Flat,
    pub(crate) zones: usize,
    pub(crate) edges_failed: Vec<ZoneResult>,
    pub(crate) text_items: usize,
    pub(crate) text_failed: Vec<TextResult>,
}

impl Diff {
    pub(crate) fn flat_ok(&self) -> bool {
        self.flat.px == 0
    }

    pub(crate) fn edges_ok(&self) -> bool {
        self.edges_failed.is_empty()
    }

    pub(crate) fn text_ok(&self) -> bool {
        self.text_failed.is_empty()
    }

    pub(crate) fn ok(&self) -> bool {
        self.flat_ok() && self.edges_ok() && self.text_ok()
    }

    pub(crate) fn edges_failed_names(&self) -> Vec<&str> {
        self.edges_failed.iter().map(|z| z.name.as_str()).collect()
    }

    pub(crate) fn text_failed_names(&self) -> Vec<&str> {
        self.text_failed.iter().map(|t| t.name.as_str()).collect()
    }
}

/// The spec of one state.
pub(crate) fn state_spec<'a>(spec: &'a Value, state: &str) -> &'a Value {
    let st = &spec["states"][state];
    assert!(st.is_object(), "state {state} not in ui-spec.json");
    st
}

/// Compare `new` with `reference` for `state`, at per-channel tolerance `tol`.
pub(crate) fn compare(
    state: &str,
    st: &Value,
    reference: &RgbImage,
    new: &RgbImage,
    tol: i32,
) -> Result<Diff, String> {
    if reference.dimensions() != new.dimensions() {
        let (rw, rh) = reference.dimensions();
        let (nw, nh) = new.dimensions();
        return Err(format!(
            "size differs: reference {rw}x{rh}, render {nw}x{nh}"
        ));
    }
    let (w, h) = (reference.width() as usize, reference.height() as usize);

    // Masks of the text and edges classes; flat is everything else.
    let mut excluded = vec![false; w * h];
    let mut exclude = |b: Box4| {
        for (x, y) in b.clip(reference).pixels() {
            excluded[y as usize * w + x as usize] = true;
        }
    };
    for r in text_regions_all(st) {
        exclude(rect_box(r, 0));
    }
    let zones = edge_zones(st);
    for z in &zones {
        exclude(z.bx);
    }

    let mut on = Vec::new();
    let mut bbox: Option<Box4> = None;
    for (i, (a, b)) in reference.pixels().zip(new.pixels()).enumerate() {
        if !excluded[i] && dist(a.0, b.0) > tol {
            on.push(i);
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            match &mut bbox {
                None => bbox = Some(Box4::new(x, y, x + 1, y + 1)),
                Some(b) => b.grow(x, y),
            }
        }
    }
    let elements = if on.is_empty() {
        Vec::new()
    } else {
        attribute(&on, w, &st["elements"])
    };

    let edges_failed: Vec<ZoneResult> = zones
        .iter()
        .map(|z| check_zone(z, reference, new, tol))
        .filter(|z| !z.ok)
        .collect();
    let items = text_items(st);
    let text_failed: Vec<TextResult> = items
        .iter()
        .map(|t| check_text(t, reference, new))
        .filter(|t| !t.ok)
        .collect();

    Ok(Diff {
        state: state.to_owned(),
        flat: Flat {
            px: on.len(),
            bbox,
            elements,
            on,
        },
        zones: zones.len(),
        edges_failed,
        text_items: items.len(),
        text_failed,
    })
}

/// Every text region rect, duplicates included, without the status dots' glyph regions: the
/// area the text class covers.
fn text_regions_all(st: &Value) -> Vec<(i32, i32, i32, i32)> {
    st["text_regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] != "glyph-dot")
        .map(|r| rect(Some(r)).unwrap())
        .collect()
}

fn outline(img: &mut RgbImage, b: Box4, color: Rgb<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut put = |x: i32, y: i32| {
        if (0..w).contains(&x) && (0..h).contains(&y) {
            img.put_pixel(x as u32, y as u32, color);
        }
    };
    for x in b.x0..b.x1 {
        put(x, b.y0);
        put(x, b.y1 - 1);
    }
    for y in b.y0..b.y1 {
        put(b.x0, y);
        put(b.x1 - 1, y);
    }
}

/// The reference dimmed, with the classes' areas outlined and every failure highlighted.
pub(crate) fn diff_image(diff: &Diff, st: &Value, reference: &RgbImage) -> RgbImage {
    let mut img = reference.clone();
    for p in img.pixels_mut() {
        for c in &mut p.0 {
            *c = (u16::from(*c) * 30 / 100) as u8;
        }
    }
    for r in text_regions_all(st) {
        outline(&mut img, rect_box(r, 0), REGION_COLOR);
    }
    for z in edge_zones(st) {
        outline(&mut img, z.bx, ZONE_COLOR);
    }
    let w = img.width() as usize;
    for &i in &diff.flat.on {
        img.put_pixel((i % w) as u32, (i / w) as u32, FLAT_COLOR);
    }
    for z in &diff.edges_failed {
        for &(x, y) in &z.non_edge {
            img.put_pixel(x as u32, y as u32, EDGE_COLOR);
        }
        if z.ref_bbox != z.new_bbox {
            outline(&mut img, z.bx, EDGE_COLOR);
        }
    }
    for t in &diff.text_failed {
        for (b, color) in [(t.reference, REF_INK_COLOR), (t.new, NEW_INK_COLOR)] {
            if let Some(b) = b {
                outline(&mut img, b, color);
            }
        }
    }
    img
}

/// The per-class summary line and up to `detail` failures per class, one per line.
pub(crate) fn report(diff: &Diff, detail: usize) -> String {
    let flag = |ok: bool| if ok { "ok  " } else { "FAIL" };
    let mut out = format!(
        "{:<14} flat {} {:>6} px   edges {} {:>3}/{:<3} zones   text {} {:>3}/{:<3} elements\n",
        diff.state,
        flag(diff.flat_ok()),
        diff.flat.px,
        flag(diff.edges_ok()),
        diff.edges_failed.len(),
        diff.zones,
        flag(diff.text_ok()),
        diff.text_failed.len(),
        diff.text_items,
    );
    for (name, n, (x, y, w, h)) in diff.flat.elements.iter().take(detail) {
        let _ = writeln!(
            out,
            "      flat  {n:>6} px in {name}  bbox x={x} y={y} w={w} h={h}"
        );
    }
    for z in diff.edges_failed.iter().take(detail) {
        let mut why = Vec::new();
        if !z.non_edge.is_empty() {
            why.push(format!("{} px off the edge", z.non_edge.len()));
        }
        if z.ref_bbox != z.new_bbox {
            why.push(format!(
                "shape bbox {} != reference {}",
                Opt(z.new_bbox),
                Opt(z.ref_bbox)
            ));
        }
        let _ = writeln!(out, "      edges {}: {}", z.name, why.join("; "));
    }
    for t in diff.text_failed.iter().take(detail) {
        let _ = writeln!(
            out,
            "      text  {}: {}  (reference ink {}, snapshot {})",
            t.name,
            t.why,
            Opt(t.reference),
            Opt(t.new)
        );
    }
    let extra = diff.flat.elements.len().saturating_sub(detail)
        + diff.edges_failed.len().saturating_sub(detail)
        + diff.text_failed.len().saturating_sub(detail);
    if extra > 0 {
        let _ = writeln!(out, "      ... {extra} more");
    }
    out
}
