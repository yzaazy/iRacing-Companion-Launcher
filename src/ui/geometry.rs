//! Where everything in the window goes for a `ViewState` (`ui/sections/`: header, apps, games,
//! log, buttons, footer).
//!
//! [`geometry`] resolves the rule-based `layout` plus the text-dependent parts (label widths,
//! buttons growing with their text, Browse squeezed by a long name, the log scrollbar).
//! `sections::show` paints that geometry, and the layout test reads the same geometry, so what is
//! tested is what is drawn.

use crate::model::{AppCard, Status, ViewState};
use crate::ui::layout::{self, ButtonRow, CardParts, Layout, LogText, Trailing};
use crate::ui::log as log_view;
use crate::ui::pack::{Anchor, Bounds};
use crate::ui::text;
use crate::ui::theme::{self, Font};

const FOOTER_TEXT: &str = "© 2026 Developed & Designed by Tobias Termeczky • Vibed with Claude";
const GAMES_HEADING: &str = "Racing Simulators & Games";
const APPS_HEADING: &str = "Companion Apps";
const LOG_HEADING: &str = "Activity Log";
pub(super) const NONE_LABEL: &str = "None";
const BROWSE_LABEL: &str = "Browse";
/// GameCard wraps names longer than this.
const GAME_NAME_WRAP: usize = 18;

/// A text label: the widget rect and where its text block starts.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGeom {
    pub(super) text: String,
    pub font: Font,
    /// The label widget.
    pub label: Bounds,
    /// Left edge of the pen and top of the text block (first baseline is `top + ascent`).
    pub left: i32,
    pub top: i32,
    /// Advance width of the widest line, and the number of lines.
    pub width: i32,
    pub lines: i32,
}

impl TextGeom {
    fn new(ctx: &egui::Context, text: &str, font: Font, label: Bounds, anchor: Anchor) -> Self {
        let (width, lines) = text::block_size(ctx, text, &font);
        let (left, top) = layout::text_origin(label, width, lines, font.linespace(), anchor);
        Self {
            text: text.to_owned(),
            font,
            label,
            left,
            top,
            width,
            lines,
        }
    }

    /// Each line's baseline in window pixels. Public for tests.
    pub fn baselines(&self) -> Vec<i32> {
        (0..self.lines)
            .map(|i| self.top + self.font.ascent + i * self.font.linespace())
            .collect()
    }
}

/// A CTkButton and its Tk text label.
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonGeom {
    pub rect: Bounds,
    /// The Tk label inside the button (text plus 1 px padding).
    pub label: Bounds,
    pub text: TextGeom,
}

impl ButtonGeom {
    fn new(
        ctx: &egui::Context,
        rect: Bounds,
        req: (i32, i32),
        text: &str,
        font: Font,
        corner_radius: f32,
    ) -> Self {
        let width = text::width(ctx, text, &font);
        let label_req = layout::button_text_req(width, font.linespace());
        let label = layout::button_label(rect, req, label_req, corner_radius as i32);
        Self {
            rect,
            label,
            text: TextGeom {
                text: text.to_owned(),
                font,
                label,
                left: label.x + 1,
                top: label.y + 1,
                width,
                lines: 1,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppCardGeom {
    pub card: Bounds,
    pub parts: CardParts,
    pub name: TextGeom,
    pub subtitle: TextGeom,
    pub browse: Option<ButtonGeom>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameCardGeom {
    pub card: Bounds,
    pub parts: CardParts,
    pub name: TextGeom,
    pub browse: Option<ButtonGeom>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub layout: Layout,
    pub title: TextGeom,
    pub footer: TextGeom,
    pub games_heading: TextGeom,
    pub apps_heading: TextGeom,
    pub log_heading: TextGeom,
    pub select_all: ButtonGeom,
    pub none_card: GameCardGeom,
    pub games: Vec<GameCardGeom>,
    pub apps: Vec<AppCardGeom>,
    pub log: LogText,
    pub buttons: ButtonRow,
    pub launch: ButtonGeom,
    pub close: ButtonGeom,
}

/// The app card subtitle (`StatusCard.set_status`).
fn subtitle(app: &AppCard) -> String {
    match app.status {
        Status::NotFound => "Not installed".to_owned(),
        Status::Running if app.helper_count > 0 => {
            let noun = if app.helper_count == 1 {
                "process"
            } else {
                "processes"
            };
            format!("{} helper {noun}", app.helper_count)
        }
        _ => "No helpers".to_owned(),
    }
}

/// A game card's name: names over 18 characters break at the last space before 18, or at 18.
pub fn wrap_game_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= GAME_NAME_WRAP {
        return name.to_owned();
    }
    let head: String = chars[..GAME_NAME_WRAP].iter().collect();
    match head.rfind(' ') {
        Some(byte) => {
            let split = head[..byte].chars().count();
            let first: String = chars[..split].iter().collect();
            let rest: String = chars[split + 1..].iter().collect();
            format!("{first}\n{rest}")
        }
        None => {
            let rest: String = chars[GAME_NAME_WRAP..].iter().collect();
            format!("{head}\n{rest}")
        }
    }
}

fn browse_trailing(ctx: &egui::Context) -> Trailing {
    let w = text::width(ctx, BROWSE_LABEL, &theme::BROWSE_FONT);
    Trailing::Browse {
        label: layout::button_text_req(w, theme::BROWSE_FONT.linespace()),
    }
}

fn browse_geom(ctx: &egui::Context, parts: &CardParts) -> Option<ButtonGeom> {
    parts.browse_req.map(|req| {
        ButtonGeom::new(
            ctx,
            parts.trailing,
            req,
            BROWSE_LABEL,
            theme::BROWSE_FONT,
            theme::SMALL_BUTTON_RADIUS,
        )
    })
}

fn game_card_geom(
    ctx: &egui::Context,
    card: Bounds,
    name: &str,
    trailing: Option<Trailing>,
) -> GameCardGeom {
    let font = theme::NAME_FONT;
    let (w, lines) = text::block_size(ctx, name, &font);
    let parts = Layout::game_card(card, w, lines, font.linespace(), trailing);
    GameCardGeom {
        card,
        parts,
        name: TextGeom::new(ctx, name, font, parts.name, Anchor::W),
        browse: browse_geom(ctx, &parts),
    }
}

fn app_card_geom(ctx: &egui::Context, card: Bounds, app: &AppCard) -> AppCardGeom {
    let sub = subtitle(app);
    let name_w = text::width(ctx, &app.name, &theme::NAME_FONT);
    let sub_w = text::width(ctx, &sub, &theme::SUBTITLE_FONT);
    let trailing = if app.status == Status::NotFound {
        browse_trailing(ctx)
    } else {
        Trailing::Dot
    };
    let parts = Layout::app_card(card, name_w, sub_w, trailing);
    AppCardGeom {
        card,
        parts,
        name: TextGeom::new(ctx, &app.name, theme::NAME_FONT, parts.name, Anchor::W),
        subtitle: TextGeom::new(
            ctx,
            &sub,
            theme::SUBTITLE_FONT,
            parts.subtitle.unwrap_or_default(),
            Anchor::W,
        ),
        browse: browse_geom(ctx, &parts),
    }
}

/// Lay the log out at the width of the Text widget without a scrollbar; if it then overflows,
/// the scrollbar shows and takes width from the text (CTkTextbox's automatic scrollbar).
fn log_needs_scrollbar(ctx: &egui::Context, layout: &Layout, state: &ViewState) -> bool {
    let plain = layout.log_text(false);
    log_view::content_height(ctx, state, plain.inner.w as f32) > plain.inner.h as f32
}

pub fn geometry(ctx: &egui::Context, state: &ViewState, window: egui::Vec2) -> Geometry {
    let layout = layout::layout(window, state.apps.len(), state.games.len());

    let title_text = format!("iRacing Companion Launcher v{}", state.version);
    let title_w = text::width(ctx, &title_text, &theme::TITLE_FONT);
    let title = TextGeom::new(
        ctx,
        &title_text,
        theme::TITLE_FONT,
        layout.title(title_w, theme::TITLE_FONT.linespace()),
        Anchor::Center,
    );
    let footer_w = text::width(ctx, FOOTER_TEXT, &theme::FOOTER_FONT);
    let footer = TextGeom::new(
        ctx,
        FOOTER_TEXT,
        theme::FOOTER_FONT,
        layout.footer_label(footer_w),
        Anchor::Center,
    );

    let heading = theme::HEADING_FONT;
    let games_w = text::width(ctx, GAMES_HEADING, &heading);
    let (games_label, _) = layout.heading_row(&layout.games, games_w, None);
    let games_heading = TextGeom::new(ctx, GAMES_HEADING, heading, games_label, Anchor::W);

    let apps_w = text::width(ctx, APPS_HEADING, &heading);
    let sa_font = theme::SELECT_ALL_FONT;
    let sa_text_w = text::width(ctx, &state.select_all_label, &sa_font);
    let sa_req = layout::button_req(
        layout::SELECT_ALL_BUTTON,
        layout::button_text_req(sa_text_w, sa_font.linespace()),
        theme::SMALL_BUTTON_RADIUS as i32,
    );
    let (apps_label, sa_rect) = layout.heading_row(&layout.apps, apps_w, Some(sa_req));
    let apps_heading = TextGeom::new(ctx, APPS_HEADING, heading, apps_label, Anchor::W);
    let select_all = ButtonGeom::new(
        ctx,
        sa_rect.unwrap_or_default(),
        sa_req,
        &state.select_all_label,
        sa_font,
        theme::SMALL_BUTTON_RADIUS,
    );

    let log_w = text::width(ctx, LOG_HEADING, &heading);
    let log_heading = TextGeom::new(
        ctx,
        LOG_HEADING,
        heading,
        layout.log_heading(log_w),
        Anchor::W,
    );

    let none_card = game_card_geom(ctx, layout.games.cards[0], NONE_LABEL, None);
    let games = state
        .games
        .iter()
        .zip(&layout.games.cards[1..])
        .map(|(game, &card)| {
            let trailing = if game.status == Status::NotFound {
                browse_trailing(ctx)
            } else {
                Trailing::Dot
            };
            game_card_geom(ctx, card, &wrap_game_name(&game.name), Some(trailing))
        })
        .collect();
    let apps = state
        .apps
        .iter()
        .zip(&layout.apps.cards)
        .map(|(app, &card)| app_card_geom(ctx, card, app))
        .collect();

    let log = layout.log_text(log_needs_scrollbar(ctx, &layout, state));

    let action = theme::HEADING_FONT;
    let radius = theme::ACTION_BUTTON_RADIUS;
    let launch_w = text::width(ctx, &state.launch_label, &action);
    let close_w = text::width(ctx, &state.close_label, &action);
    let launch_label = layout::button_text_req(launch_w, action.linespace());
    let close_label = layout::button_text_req(close_w, action.linespace());
    let buttons = layout.buttons(launch_label, close_label, radius as i32);
    let launch = ButtonGeom::new(
        ctx,
        buttons.launch,
        layout::button_req(layout::ACTION_BUTTON, launch_label, radius as i32),
        &state.launch_label,
        action,
        radius,
    );
    let close = ButtonGeom::new(
        ctx,
        buttons.close,
        layout::button_req(layout::ACTION_BUTTON, close_label, radius as i32),
        &state.close_label,
        action,
        radius,
    );

    Geometry {
        layout,
        title,
        footer,
        games_heading,
        apps_heading,
        log_heading,
        select_all,
        none_card,
        games,
        apps,
        log,
        buttons,
        launch,
        close,
    }
}
