//! A small port of Tk's pack geometry manager (`tkPack.c`: `ArrangePacking`, `XExpansion`,
//! `YExpansion`, `ComputePackSize`), and the whole-pixel rectangle Tk reports widget geometry in.
//!
//! Each container is described as the list of slaves Python packs into it, with the same options;
//! [`pack`] places them in the master's cavity and [`pack_req`] is the size the master requests.

use egui::{Pos2, Rect, pos2, vec2};

/// A rectangle in whole window pixels, the way Tk reports widget geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Bounds {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn rect(&self) -> Rect {
        Rect::from_min_size(
            pos2(self.x as f32, self.y as f32),
            vec2(self.w as f32, self.h as f32),
        )
    }

    pub fn min(&self) -> Pos2 {
        pos2(self.x as f32, self.y as f32)
    }

    /// What CustomTkinter actually paints for a widget of this size: its draw engine floors the
    /// canvas width and height to even numbers, leaving an odd last column or row unpainted.
    pub fn painted(&self) -> Self {
        Self::new(self.x, self.y, self.w & !1, self.h & !1)
    }

    /// Tk unmaps a packed widget whose width or height comes out zero or less.
    pub(super) fn is_mapped(&self) -> bool {
        self.w > 0 && self.h > 0
    }

    /// Shrink by `d` on every side.
    pub(super) fn inset(&self, d: i32) -> Self {
        Self::new(self.x + d, self.y + d, self.w - 2 * d, self.h - 2 * d)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fill {
    None,
    X,
    Y,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Anchor {
    Center,
    W,
}

/// One `widget.pack(...)` call: the widget's requested size and its pack options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Slave {
    side: Side,
    req: (i32, i32),
    /// (left, right)
    padx: (i32, i32),
    /// (top, bottom)
    pady: (i32, i32),
    expand: bool,
    fill: Fill,
    anchor: Anchor,
}

impl Slave {
    pub(super) const fn new(side: Side, w: i32, h: i32) -> Self {
        Self {
            side,
            req: (w, h),
            padx: (0, 0),
            pady: (0, 0),
            expand: false,
            fill: Fill::None,
            anchor: Anchor::Center,
        }
    }

    pub(super) const fn padx(mut self, left: i32, right: i32) -> Self {
        self.padx = (left, right);
        self
    }

    pub(super) const fn pady(mut self, top: i32, bottom: i32) -> Self {
        self.pady = (top, bottom);
        self
    }

    pub(super) const fn expand(mut self) -> Self {
        self.expand = true;
        self
    }

    pub(super) const fn fill(mut self, fill: Fill) -> Self {
        self.fill = fill;
        self
    }

    pub(super) const fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    fn pad_w(&self) -> i32 {
        self.padx.0 + self.padx.1
    }

    fn pad_h(&self) -> i32 {
        self.pady.0 + self.pady.1
    }

    fn horizontal(&self) -> bool {
        matches!(self.side, Side::Left | Side::Right)
    }
}

/// Tk's `XExpansion`: how much extra width an expanding slave may take, given the slaves after it.
fn x_expansion(slaves: &[Slave], mut cavity_w: i32) -> i32 {
    let mut min_expand = cavity_w;
    let mut num_expand = 0;
    for s in slaves {
        let child_w = s.req.0 + s.pad_w();
        if s.horizontal() {
            cavity_w -= child_w;
            if s.expand {
                num_expand += 1;
            }
        } else if num_expand > 0 {
            min_expand = min_expand.min((cavity_w - child_w) / num_expand);
        }
    }
    if num_expand > 0 {
        min_expand = min_expand.min(cavity_w / num_expand);
    }
    min_expand.max(0)
}

/// Tk's `YExpansion`.
fn y_expansion(slaves: &[Slave], mut cavity_h: i32) -> i32 {
    let mut min_expand = cavity_h;
    let mut num_expand = 0;
    for s in slaves {
        let child_h = s.req.1 + s.pad_h();
        if s.horizontal() {
            if num_expand > 0 {
                min_expand = min_expand.min((cavity_h - child_h) / num_expand);
            }
        } else {
            cavity_h -= child_h;
            if s.expand {
                num_expand += 1;
            }
        }
    }
    if num_expand > 0 {
        min_expand = min_expand.min(cavity_h / num_expand);
    }
    min_expand.max(0)
}

/// Tk's `ArrangePacking`: place `slaves`, in packing order, inside `master`.
pub(super) fn pack(master: Bounds, slaves: &[Slave]) -> Vec<Bounds> {
    let (mut cx, mut cy, mut cw, mut ch) = (master.x, master.y, master.w, master.h);
    let mut out = Vec::with_capacity(slaves.len());
    for (i, s) in slaves.iter().enumerate() {
        let (frame_x, frame_y, frame_w, frame_h);
        if s.horizontal() {
            let mut fw = s.req.0 + s.pad_w();
            if s.expand {
                fw += x_expansion(&slaves[i..], cw);
            }
            cw -= fw;
            if cw < 0 {
                fw += cw;
                cw = 0;
            }
            frame_w = fw;
            frame_h = ch;
            frame_y = cy;
            if s.side == Side::Left {
                frame_x = cx;
                cx += fw;
            } else {
                frame_x = cx + cw;
            }
        } else {
            let mut fh = s.req.1 + s.pad_h();
            if s.expand {
                fh += y_expansion(&slaves[i..], ch);
            }
            ch -= fh;
            if ch < 0 {
                fh += ch;
                ch = 0;
            }
            frame_h = fh;
            frame_w = cw;
            frame_x = cx;
            if s.side == Side::Top {
                frame_y = cy;
                cy += fh;
            } else {
                frame_y = cy + ch;
            }
        }

        let mut w = s.req.0;
        if matches!(s.fill, Fill::X | Fill::Both) || w > frame_w - s.pad_w() {
            w = frame_w - s.pad_w();
        }
        let mut h = s.req.1;
        if matches!(s.fill, Fill::Y | Fill::Both) || h > frame_h - s.pad_h() {
            h = frame_h - s.pad_h();
        }
        // C integer division, truncating toward zero like Rust's.
        let centre_y = frame_y + (s.pady.0 + frame_h - h - s.pady.1) / 2;
        let x = match s.anchor {
            Anchor::Center => frame_x + (s.padx.0 + frame_w - w - s.padx.1) / 2,
            Anchor::W => frame_x + s.padx.0,
        };
        out.push(Bounds::new(x, centre_y, w, h));
    }
    out
}

/// Tk's `ComputePackSize`: the size a master with pack propagation on requests for its slaves.
pub(super) fn pack_req(slaves: &[Slave]) -> (i32, i32) {
    let (mut width, mut height, mut max_w, mut max_h) = (0, 0, 0, 0);
    for s in slaves {
        if s.horizontal() {
            max_h = max_h.max(s.req.1 + s.pad_h() + height);
            width += s.req.0 + s.pad_w();
        } else {
            max_w = max_w.max(s.req.0 + s.pad_w() + width);
            height += s.req.1 + s.pad_h();
        }
    }
    (max_w.max(width), max_h.max(height))
}
