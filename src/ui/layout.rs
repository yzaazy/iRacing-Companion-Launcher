//! Rule-based window layout reproducing the measured Python geometry (`docs/rewrite/ui-spec.json`).
//!
//! The Python window is built with Tk's packer, and its quirks are what the window looks like:
//! the button row absorbing a height overflow, columns sharing surplus width by integer division,
//! a long name squeezing its Browse button. So each container is described as the list of slaves
//! Python packs into it, with the same options, and placed by the port of Tk's pack algorithm in
//! [`super::pack`]. The same rules then give a sensible layout at any window size.
//!
//! [`layout`] resolves the frames that depend only on the window size and the number of cards.
//! Where Python's geometry depends on text (a label's width, a button growing with its label),
//! the `Layout` methods take the measured text widths.

use crate::ui::pack::{Anchor, Bounds, Fill, Side, Slave, pack, pack_req};
use egui::Vec2;

// ---------------------------------------------------------------------------------------------
// Widget sizes and pack options, from the Python sections and CustomTkinter's defaults.

/// A CTkFrame's default requested width; frames with propagation off keep it.
const FRAME_DEFAULT_W: i32 = 200;
/// A CTkLabel's default height.
const LABEL_H: i32 = 28;
const HEADER_H: i32 = 70;
const HEADER_GAP: i32 = 10;
const TITLE_PADY: i32 = 20;
const FOOTER_PADX: i32 = 15;
const FOOTER_PAD_BOTTOM: i32 = 5;
const CONTENT_PADX: i32 = 15;
const CONTENT_PADY: i32 = 5;
const COLUMN_GAP: i32 = 10;
const HEADING_ROW_H: i32 = 30;
const HEADING_GAP: i32 = 10;
/// `STATUS_CARD_HEIGHT`.
const CARD_H: i32 = 55;
/// Cards are packed with 5 px above and below, none above the first or below the last.
const CARD_PAD: i32 = 5;
/// The log's CTkTextbox requests 400 px of width.
const LOG_REQ_W: i32 = 400;
/// The button row: `pady=(35, 10)`, each button `padx=10`.
const BUTTONS_PAD_TOP: i32 = 35;
const BUTTONS_PAD_BOTTOM: i32 = 10;
const ACTION_PADX: i32 = 10;
pub(super) const ACTION_BUTTON: (i32, i32) = (200, 60);
pub(super) const SELECT_ALL_BUTTON: (i32, i32) = (100, 30);
const BROWSE_BUTTON: (i32, i32) = (80, 36);
/// A CTkCheckBox with `text=""`: the 24 px box, 6 px of text spacing and a 1 px empty label.
const CHECKBOX_WIDGET: (i32, i32) = (31, 24);
const CHECKBOX_BOX: i32 = 24;
/// A CTkRadioButton with `text=""`: the 22 px circle, spacing and empty label.
const RADIO_WIDGET: (i32, i32) = (29, 22);
const RADIO_BOX: i32 = 22;
/// The status dot label: the advance and line height of `●` in Segoe UI 40.
const DOT_LABEL: (i32, i32) = (24, 54);
/// The subtitle CTkLabel is created with `height=12`.
const SUBTITLE_H: i32 = 12;
/// CTkTextbox pads its Text widget by the corner radius, and the scrollbar by 3 px either side.
const LOG_TEXT_INSET: i32 = 6;
const LOG_SCROLLBAR_GAP: i32 = 3;
const LOG_SCROLLBAR_W: i32 = 8;
/// The Text widget's border (1) plus its padding (1).
const LOG_TEXT_PAD: i32 = 2;

/// Pack options of the toggle (checkbox or radio) at a card's left edge.
fn toggle_slave(widget: (i32, i32)) -> Slave {
    Slave::new(Side::Left, widget.0, widget.1)
        .padx(10, 5)
        .pady(10, 10)
}

fn dot_slave() -> Slave {
    Slave::new(Side::Right, DOT_LABEL.0, DOT_LABEL.1)
        .padx(15, 15)
        .pady(5, 15)
}

fn browse_slave(req: (i32, i32)) -> Slave {
    Slave::new(Side::Right, req.0, req.1)
        .padx(15, 15)
        .pady(10, 10)
}

/// A card packed `fill="x"` with the given vertical padding.
fn card_slave(pady: (i32, i32)) -> Slave {
    Slave::new(Side::Top, FRAME_DEFAULT_W, CARD_H)
        .pady(pady.0, pady.1)
        .fill(Fill::X)
}

/// The requested size of a CTkLabel showing `lines` lines of text `text_w` wide.
fn label_req(text_w: i32, lines: i32, linespace: i32) -> (i32, i32) {
    (text_w, LABEL_H.max(lines * linespace))
}

/// The requested size of a CTkButton: its declared size, grown to fit the label plus the
/// grid's outer columns (`max(corner_radius, border_width + 1, border_spacing)` each).
pub(super) fn button_req(
    declared: (i32, i32),
    label: (i32, i32),
    corner_radius: i32,
) -> (i32, i32) {
    (
        declared.0.max(label.0 + 2 * button_col_min(corner_radius)),
        declared.1.max(label.1 + 2 * BUTTON_ROW_MIN),
    )
}

fn button_col_min(corner_radius: i32) -> i32 {
    corner_radius.max(BUTTON_ROW_MIN)
}

/// `max(border_width + 1, border_spacing)` with CTk's defaults (0 and 2).
const BUTTON_ROW_MIN: i32 = 2;

/// The text label of a CTkButton: Tk's grid gives the outer rows and columns weight 1000 and the
/// middle ones weight 1 or 0, so the spare space at the requested size is split almost evenly,
/// and when pack squeezes the button the outer cells give it back the same way.
pub(super) fn button_label(
    button: Bounds,
    req: (i32, i32),
    label: (i32, i32),
    corner_radius: i32,
) -> Bounds {
    // Columns: 1000, 1, 0, 1, 1000. Rows: 1000, 0, 1, 0, 1000.
    fn offset(req: i32, actual: i32, min: i32, label: i32, total_weight: f32) -> i32 {
        let share = |px: i32| (px as f32 * 1000.0 / total_weight).round() as i32;
        min + share(req - 2 * min - label) - share(req - actual)
    }
    let x = offset(
        req.0,
        button.w,
        button_col_min(corner_radius),
        label.0,
        2002.0,
    );
    let y = offset(req.1, button.h, BUTTON_ROW_MIN, label.1, 2001.0);
    Bounds::new(button.x + x, button.y + y, label.0, label.1)
}

/// The width and height of a plain Tk label inside a CTkButton (1 px `padx`/`pady`).
pub(super) fn button_text_req(text_w: i32, linespace: i32) -> (i32, i32) {
    (text_w + 2, linespace + 2)
}

/// Where Tk draws a block of `lines` lines `text_w` wide inside a label: centred with C integer
/// division (`TkComputeAnchor`), or flush left for `anchor="w"`. Returns (left x, top y).
pub(super) fn text_origin(
    label: Bounds,
    text_w: i32,
    lines: i32,
    linespace: i32,
    anchor: Anchor,
) -> (i32, i32) {
    let top = label.y + (label.h - lines * linespace) / 2;
    let left = match anchor {
        Anchor::Center => label.x + (label.w - text_w) / 2,
        Anchor::W => label.x,
    };
    (left, top)
}

// ---------------------------------------------------------------------------------------------
// The window.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub column: Bounds,
    pub heading_row: Bounds,
    pub cards_frame: Bounds,
    /// Card widget rects; for games the first is the None card.
    pub cards: Vec<Bounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogColumn {
    pub column: Bounds,
    /// The parcel the heading label is packed into (side top, anchor w).
    heading_frame: Bounds,
    pub log_box: Bounds,
}

/// Geometry of the log box's contents, with or without the scrollbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogText {
    /// The Tk Text widget.
    pub text: Bounds,
    /// Where lines are drawn and clipped: the Text widget inside its border and padding.
    pub inner: Bounds,
    pub scrollbar: Option<Bounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    window: Vec2,
    pub header: Bounds,
    pub footer: Bounds,
    pub content: Bounds,
    pub games: Column,
    pub apps: Column,
    pub log: LogColumn,
    /// The parcel the button row is centred in, and how much of its height the row may use.
    buttons_frame: Bounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonRow {
    pub row: Bounds,
    pub launch: Bounds,
    pub(super) close: Bounds,
}

/// What sits at a card's right edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Trailing {
    /// The status dot.
    Dot,
    /// The Browse button, whose label is this wide (text plus label padding).
    Browse { label: (i32, i32) },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardParts {
    /// The checkbox or radio widget, and the box or circle it paints.
    pub toggle: Bounds,
    pub toggle_box: Bounds,
    /// App cards: the frame holding name and subtitle. Game cards: the name label.
    pub text_frame: Bounds,
    pub name: Bounds,
    pub subtitle: Option<Bounds>,
    /// The dot label or the Browse button.
    pub trailing: Bounds,
    /// The Browse button's requested size, when it is a Browse button.
    pub(super) browse_req: Option<(i32, i32)>,
}

/// A heading row: the label packed left, then the optional button packed right, which gets
/// what the label leaves.
fn heading_row_slaves(heading_w: i32, button: Option<(i32, i32)>) -> Vec<Slave> {
    let mut slaves = vec![Slave::new(Side::Left, heading_w, LABEL_H)];
    if let Some(req) = button {
        slaves.push(Slave::new(Side::Right, req.0, req.1));
    }
    slaves
}

fn log_box_req(n_apps: usize) -> i32 {
    // LogSection: STATUS_CARD_HEIGHT * n + 10 * (n - 1), so the box matches the app cards.
    let n = n_apps as i32;
    (CARD_H * n + 2 * CARD_PAD * (n - 1)).max(0)
}

/// The cards of a column with Python's padding: AppsSection gives the first card `pady=(0, 5)`,
/// the last `(5, 0)` and the rest 5; GamesSection packs the None card with `(0, 5)` and every game
/// with 5 except the last, `(5, 0)`.
fn cards_slaves(n_cards: usize, leading_none: bool) -> Vec<Slave> {
    let mut slaves = Vec::new();
    if leading_none {
        slaves.push(card_slave((0, CARD_PAD)));
    }
    slaves.extend((0..n_cards).map(|i| {
        card_slave(if i == 0 && !leading_none {
            (0, CARD_PAD)
        } else if i + 1 == n_cards {
            (CARD_PAD, 0)
        } else {
            (CARD_PAD, CARD_PAD)
        })
    }));
    slaves
}

fn column_slaves(cards_req: (i32, i32)) -> [Slave; 2] {
    [
        Slave::new(Side::Top, FRAME_DEFAULT_W, HEADING_ROW_H)
            .pady(0, HEADING_GAP)
            .fill(Fill::X),
        Slave::new(Side::Top, cards_req.0, cards_req.1)
            .expand()
            .fill(Fill::Both),
    ]
}

fn resolve_column(column: Bounds, cards: &[Slave]) -> Column {
    let frames = pack(column, &column_slaves(pack_req(cards)));
    Column {
        column,
        heading_row: frames[0],
        cards_frame: frames[1],
        cards: pack(frames[1], cards),
    }
}

pub fn layout(window: Vec2, n_apps: usize, n_games: usize) -> Layout {
    let win = Bounds::new(0, 0, window.x.round() as i32, window.y.round() as i32);

    let game_cards = cards_slaves(n_games, true);
    let app_cards = cards_slaves(n_apps, false);
    let log_slaves = [
        Slave::new(Side::Top, 0, LABEL_H)
            .pady(0, HEADING_GAP)
            .anchor(Anchor::W),
        Slave::new(Side::Top, LOG_REQ_W, log_box_req(n_apps)).fill(Fill::Both),
    ];
    let column_req = |cards: &[Slave]| pack_req(&column_slaves(pack_req(cards)));
    let content_slaves = [
        {
            let (w, h) = column_req(&game_cards);
            Slave::new(Side::Left, w, h)
                .padx(0, COLUMN_GAP)
                .expand()
                .fill(Fill::Both)
        },
        {
            let (w, h) = column_req(&app_cards);
            Slave::new(Side::Left, w, h)
                .padx(0, COLUMN_GAP)
                .expand()
                .fill(Fill::Both)
        },
        {
            let (w, h) = pack_req(&log_slaves);
            Slave::new(Side::Right, w, h).expand().fill(Fill::Both)
        },
    ];
    let content_req = pack_req(&content_slaves);

    // The root: header, footer, content, buttons, in Python's packing order. The footer label's
    // and the button row's widths do not change any height, so placeholders do here.
    let root = pack(
        win,
        &[
            Slave::new(Side::Top, FRAME_DEFAULT_W, HEADER_H)
                .pady(0, HEADER_GAP)
                .fill(Fill::X),
            Slave::new(Side::Bottom, 0, LABEL_H)
                .padx(FOOTER_PADX, FOOTER_PADX)
                .pady(0, FOOTER_PAD_BOTTOM)
                .fill(Fill::X),
            Slave::new(Side::Top, content_req.0, content_req.1)
                .padx(CONTENT_PADX, CONTENT_PADX)
                .pady(CONTENT_PADY, CONTENT_PADY)
                .expand()
                .fill(Fill::Both),
            Slave::new(Side::Top, 0, ACTION_BUTTON.1).pady(BUTTONS_PAD_TOP, BUTTONS_PAD_BOTTOM),
        ],
    );
    let (header, footer, content, buttons) = (root[0], root[1], root[2], root[3]);
    // The parcel pack gave the button row: everything between content and footer, full width.
    let buttons_frame = Bounds::new(
        win.x,
        buttons.y - BUTTONS_PAD_TOP,
        win.w,
        buttons.h + BUTTONS_PAD_TOP + BUTTONS_PAD_BOTTOM,
    );

    let columns = pack(content, &content_slaves);
    let log_frames = pack(columns[2], &log_slaves);
    Layout {
        window,
        header,
        footer,
        content,
        games: resolve_column(columns[0], &game_cards),
        apps: resolve_column(columns[1], &app_cards),
        log: LogColumn {
            column: columns[2],
            heading_frame: Bounds::new(
                columns[2].x,
                columns[2].y,
                columns[2].w,
                log_frames[0].h + HEADING_GAP,
            ),
            log_box: log_frames[1],
        },
        buttons_frame,
    }
}

impl Layout {
    /// The header title label, `pady=20`, centred.
    pub(super) fn title(&self, text_w: i32, linespace: i32) -> Bounds {
        let req = label_req(text_w, 1, linespace);
        pack(
            self.header,
            &[Slave::new(Side::Top, req.0, req.1).pady(TITLE_PADY, TITLE_PADY)],
        )[0]
    }

    /// The footer label, packed to the right.
    pub(super) fn footer_label(&self, text_w: i32) -> Bounds {
        pack(self.footer, &[Slave::new(Side::Right, text_w, LABEL_H)])[0]
    }

    /// The heading label of a card column, and the button packed to its right (Select All).
    pub(super) fn heading_row(
        &self,
        column: &Column,
        heading_w: i32,
        button: Option<(i32, i32)>,
    ) -> (Bounds, Option<Bounds>) {
        let placed = pack(column.heading_row, &heading_row_slaves(heading_w, button));
        (placed[0], placed.get(1).copied())
    }

    /// The "Activity Log" label.
    pub(super) fn log_heading(&self, text_w: i32) -> Bounds {
        pack(
            self.log.heading_frame,
            &[Slave::new(Side::Top, text_w, LABEL_H)
                .pady(0, HEADING_GAP)
                .anchor(Anchor::W)],
        )[0]
    }

    /// The Text widget inside the log box, without or with the scrollbar beside it.
    pub(super) fn log_text(&self, scrollbar: bool) -> LogText {
        let b = self.log.log_box;
        let mut text = b.inset(LOG_TEXT_INSET);
        let scrollbar = scrollbar.then(|| {
            text.w = b.w - LOG_TEXT_INSET - 2 * LOG_SCROLLBAR_GAP - LOG_SCROLLBAR_W;
            Bounds::new(
                text.right() + LOG_SCROLLBAR_GAP,
                text.y,
                LOG_SCROLLBAR_W,
                text.h,
            )
        });
        LogText {
            text,
            inner: text.inset(LOG_TEXT_PAD),
            scrollbar,
        }
    }

    /// The Launch and Close buttons, each at least 200 px and grown to fit its label; the row is
    /// centred in the window and gets whatever height is left above the footer, at most 60 px.
    pub fn buttons(
        &self,
        launch_label: (i32, i32),
        close_label: (i32, i32),
        corner_radius: i32,
    ) -> ButtonRow {
        let launch_req = button_req(ACTION_BUTTON, launch_label, corner_radius);
        let close_req = button_req(ACTION_BUTTON, close_label, corner_radius);
        let buttons = [
            Slave::new(Side::Left, launch_req.0, launch_req.1).padx(ACTION_PADX, ACTION_PADX),
            Slave::new(Side::Left, close_req.0, close_req.1).padx(ACTION_PADX, ACTION_PADX),
        ];
        let req = pack_req(&buttons);
        let row = pack(
            self.buttons_frame,
            &[Slave::new(Side::Top, req.0, req.1).pady(BUTTONS_PAD_TOP, BUTTONS_PAD_BOTTOM)],
        )[0];
        let placed = pack(row, &buttons);
        ButtonRow {
            row,
            launch: placed[0],
            close: placed[1],
        }
    }

    /// The parts of an app card: checkbox, name over subtitle, then the dot or Browse button.
    pub(super) fn app_card(
        card: Bounds,
        name_w: i32,
        subtitle_w: i32,
        trailing: Trailing,
    ) -> CardParts {
        let text_slaves = [
            Slave::new(Side::Top, name_w, LABEL_H).anchor(Anchor::W),
            Slave::new(Side::Top, subtitle_w, SUBTITLE_H).anchor(Anchor::W),
        ];
        let text_req = pack_req(&text_slaves);
        let (trail, browse_req) = trailing_slave(trailing);
        let placed = pack(
            card,
            &[
                toggle_slave(CHECKBOX_WIDGET),
                Slave::new(Side::Left, text_req.0, text_req.1)
                    .padx(5, 15)
                    .pady(4, 4)
                    .fill(Fill::Y),
                trail,
            ],
        );
        let text = pack(placed[1], &text_slaves);
        CardParts {
            toggle: placed[0],
            toggle_box: Bounds::new(placed[0].x, placed[0].y, CHECKBOX_BOX, CHECKBOX_BOX),
            text_frame: placed[1],
            name: text[0],
            subtitle: Some(text[1]),
            trailing: placed[2],
            browse_req,
        }
    }

    /// The parts of a game card (and the None card, which has no trailing widget).
    pub(super) fn game_card(
        card: Bounds,
        name_w: i32,
        name_lines: i32,
        linespace: i32,
        trailing: Option<Trailing>,
    ) -> CardParts {
        let name_req = label_req(name_w, name_lines, linespace);
        let mut slaves = vec![
            toggle_slave(RADIO_WIDGET),
            Slave::new(Side::Left, name_req.0, name_req.1)
                .padx(5, 15)
                .pady(10, 10),
        ];
        let mut browse_req = None;
        if let Some(t) = trailing {
            let (s, req) = trailing_slave(t);
            slaves.push(s);
            browse_req = req;
        }
        let placed = pack(card, &slaves);
        CardParts {
            toggle: placed[0],
            toggle_box: Bounds::new(placed[0].x, placed[0].y, RADIO_BOX, RADIO_BOX),
            text_frame: placed[1],
            name: placed[1],
            subtitle: None,
            trailing: placed.get(2).copied().unwrap_or_default(),
            browse_req,
        }
    }
}

fn trailing_slave(trailing: Trailing) -> (Slave, Option<(i32, i32)>) {
    match trailing {
        Trailing::Dot => (dot_slave(), None),
        Trailing::Browse { label } => {
            let req = button_req(
                BROWSE_BUTTON,
                label,
                crate::ui::theme::SMALL_BUTTON_RADIUS as i32,
            );
            (browse_slave(req), Some(req))
        }
    }
}
