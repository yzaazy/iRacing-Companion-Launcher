//! Ports `ui/sections/` (header, apps, games, log, buttons, footer).
//!
//! [`geometry`] resolves where everything goes for a `ViewState`: the rule-based `layout` plus
//! the text-dependent parts (label widths, buttons growing with their text, Browse squeezed by a
//! long name, the log scrollbar). [`show`] paints that geometry and turns clicks into actions.
//! The layout test reads the same geometry, so what is tested is what is drawn.

use crate::contract::{AppCard, GameCard, LogLevel, Status, UiAction, ViewState};
use crate::ui::layout::{self, Anchor, Bounds, ButtonRow, CardParts, Layout, LogText, Trailing};
use crate::ui::text::{self, Run};
use crate::ui::theme::{self, Font};
use crate::ui::widgets::{self, ButtonLook};
use egui::{Color32, CursorIcon, Id, Sense, Ui, Role, WidgetInfo};
use std::sync::Arc;

pub const FOOTER_TEXT: &str = "© 2026 Developed & Designed by Tobias Termeczky • Vibed with Claude";
pub const GAMES_HEADING: &str = "Racing Simulators & Games";
pub const APPS_HEADING: &str = "Companion Apps";
pub const LOG_HEADING: &str = "Activity Log";
pub const NONE_LABEL: &str = "None";
pub const BROWSE_LABEL: &str = "Browse";
/// GameCard wraps names longer than this.
const GAME_NAME_WRAP: usize = 18;

/// A text label: the widget rect and where its text block starts.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGeom {
    pub text: String,
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

    pub fn baselines(&self) -> Vec<i32> {
        (0..self.lines)
            .map(|i| self.top + self.font.ascent + i * self.font.linespace())
            .collect()
    }

    fn paint(&self, ctx: &egui::Context, painter: &egui::Painter, color: Color32) {
        text::paint_block(
            ctx, painter, &self.text, &self.font, color, self.left, self.top,
        );
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
pub fn subtitle(app: &AppCard) -> String {
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

/// `GameCard._wrap_text`: names over 18 characters break at the last space before 18, or at 18.
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
    let galley = log_galley(ctx, state, plain.inner.w as f32);
    galley.size().y > plain.inner.h as f32
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

// ---------------------------------------------------------------------------------------------
// The log text.

/// The whole log as one galley: every line in its level's colour, each followed by a newline as
/// Tk inserts it, wrapped at any character to the Text widget's inner width.
pub fn log_galley(ctx: &egui::Context, state: &ViewState, inner_width: f32) -> Arc<egui::Galley> {
    let mut lines: Vec<(String, Color32)> = Vec::with_capacity(state.log.len());
    for line in &state.log {
        let color = theme::log_color(&line.level);
        let text = if line.level == LogLevel::Divider || line.text.ends_with('\n') {
            format!("{}\n", line.text.trim_end_matches('\n'))
        } else {
            format!("{}\n", line.text)
        };
        lines.push((text, color));
    }
    let runs: Vec<Run<'_>> = lines
        .iter()
        .map(|(text, color)| Run {
            text,
            color: *color,
        })
        .collect();
    text::layout(ctx, &runs, &theme::LOG_FONT, Some(inner_width))
}

// ---------------------------------------------------------------------------------------------
// Painting and interaction.

fn clickable(ui: &Ui, id: Id, b: Bounds, enabled: bool, info: WidgetInfo) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(b.rect(), id, sense);
    response.widget_info(|| info.clone());
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

fn button(
    ui: &Ui,
    id: Id,
    geom: &ButtonGeom,
    radius: f32,
    colors: theme::ButtonColors,
    enabled: bool,
    a11y: &str,
) -> bool {
    let response = clickable(
        ui,
        id,
        geom.rect,
        enabled,
        WidgetInfo::labeled(Role::Button, enabled, a11y),
    );
    widgets::button(
        ui.ctx(),
        ui.painter(),
        &ButtonLook {
            rect: geom.rect,
            label: geom.label,
            text: &geom.text.text,
            font: &geom.text.font,
            corner_radius: radius,
            colors,
            enabled,
            hovered: response.hovered(),
        },
    );
    enabled && response.clicked()
}

fn browse(ui: &Ui, id: Id, geom: &ButtonGeom, name: &str) -> bool {
    button(
        ui,
        id,
        geom,
        theme::SMALL_BUTTON_RADIUS,
        theme::BROWSE_BUTTON,
        true,
        &format!("Browse for {name}"),
    )
}

fn heading(ui: &Ui, geom: &TextGeom) {
    geom.paint(ui.ctx(), ui.painter(), theme::TEXT_HEADING);
}

fn game_card(
    ui: &Ui,
    id: Id,
    geom: &GameCardGeom,
    game: Option<&GameCard>,
    selected: bool,
    actions: &mut Vec<UiAction>,
) {
    let painter = ui.painter();
    let found = game.is_none_or(|g| g.status != Status::NotFound);
    let border = if game.is_some() {
        theme::GAME_CARD_BORDER
    } else {
        theme::CARD_BORDER
    };
    widgets::bordered(
        painter,
        geom.card.painted(),
        theme::CARD_RADIUS,
        theme::CARD_BORDER_WIDTH,
        border,
        theme::CARD_FILL,
    );

    let label = game.map_or(NONE_LABEL, |g| g.name.as_str());
    let radio = clickable(
        ui,
        id.with("radio"),
        geom.parts.toggle,
        found,
        WidgetInfo::selected(Role::RadioButton, found, selected, label),
    );
    widgets::radio(
        painter,
        geom.parts.toggle_box,
        selected,
        found && radio.hovered(),
        theme::CARD_FILL,
    );
    // CTkRadioButton.invoke only acts when it is not already selected.
    if found && radio.clicked() && !selected {
        actions.push(UiAction::SelectGame(game.map(|g| g.name.clone())));
    }

    let name_color = if found {
        theme::TEXT_WHITE
    } else {
        theme::TEXT_DIM
    };
    geom.name.paint(ui.ctx(), painter, name_color);

    if let Some(game) = game {
        if let Some(b) = &geom.browse {
            if browse(ui, id.with("browse"), b, &game.name) {
                actions.push(UiAction::BrowseGame(game.name.clone()));
            }
        } else {
            widgets::dot(
                painter,
                geom.parts.trailing,
                theme::status_color(&game.status),
            );
        }
    }
}

fn app_card(ui: &Ui, id: Id, geom: &AppCardGeom, app: &AppCard, actions: &mut Vec<UiAction>) {
    let painter = ui.painter();
    widgets::bordered(
        painter,
        geom.card.painted(),
        theme::CARD_RADIUS,
        theme::CARD_BORDER_WIDTH,
        theme::CARD_BORDER,
        theme::CARD_FILL,
    );
    let found = app.status != Status::NotFound;
    let check = clickable(
        ui,
        id.with("checkbox"),
        geom.parts.toggle,
        found,
        WidgetInfo::selected(Role::CheckBox, found, app.checked, &app.name),
    );
    widgets::checkbox(
        painter,
        geom.parts.toggle_box,
        app.checked,
        found && check.hovered(),
        theme::CARD_FILL,
    );
    if found && check.clicked() {
        actions.push(UiAction::ToggleApp {
            name: app.name.clone(),
            checked: !app.checked,
        });
    }

    let name_color = if found {
        theme::TEXT_WHITE
    } else {
        theme::TEXT_DIM
    };
    geom.name.paint(ui.ctx(), painter, name_color);
    geom.subtitle.paint(ui.ctx(), painter, theme::TEXT_DIM);

    if let Some(b) = &geom.browse {
        if browse(ui, id.with("browse"), b, &app.name) {
            actions.push(UiAction::BrowseApp(app.name.clone()));
        }
    } else {
        widgets::dot(
            painter,
            geom.parts.trailing,
            theme::status_color(&app.status),
        );
    }
}

/// Remembers how much log the scroll area last showed, so a new line scrolls to the end.
#[derive(Clone, Copy, PartialEq)]
struct LogSeen {
    lines: usize,
    hash: u64,
}

fn log_hash(state: &ViewState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Some(last) = state.log.last() {
        last.text.hash(&mut h);
    }
    h.finish()
}

fn log(ui: &mut Ui, geom: &Geometry, state: &ViewState) {
    let ctx = ui.ctx().clone();
    let id = ui.scope_id().with("activity_log");
    let log_box = geom.layout.log.log_box;
    widgets::rounded(
        ui.painter(),
        log_box.painted(),
        theme::LOG_RADIUS,
        theme::LOG_BG,
    );

    let inner = geom.log.inner;
    let galley = log_galley(&ctx, state, inner.w as f32);
    let content_h = galley.size().y;
    let view_h = inner.h as f32;
    let max_offset = (content_h - view_h).max(0.0);

    let seen = LogSeen {
        lines: state.log.len(),
        hash: log_hash(state),
    };
    let changed = ctx.data(|d| d.get_temp::<LogSeen>(id)) != Some(seen);
    ctx.data_mut(|d| d.insert_temp(id, seen));

    let scroll_id = id.with("scroll");
    let mut offset = if changed {
        // ActivityLogger calls see("end") after every line.
        Some(max_offset)
    } else {
        None
    };

    // The scrollbar: press or drag anywhere on it to centre the thumb on the pointer, as
    // CTkScrollbar does.
    if let Some(bar) = geom.log.scrollbar {
        let response = ui.interact(bar.rect(), id.with("scrollbar"), Sense::click_and_drag());
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.is_pointer_button_down_on())
        {
            let canvas = bar.painted();
            let len = (view_h / content_h).min(1.0);
            let value = (pointer.y - canvas.y as f32) / canvas.h as f32;
            let start = (value - len / 2.0).clamp(0.0, 1.0 - len);
            offset = Some(start * content_h);
        }
    }

    let mut area = egui::ScrollArea::vertical()
        .id_salt(scroll_id)
        .auto_shrink(false)
        .animated(false)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = offset {
        area = area.vertical_scroll_offset(offset.min(max_offset));
    }
    let output = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner.rect()), |ui| {
            ui.visuals_mut().selection.bg_fill = theme::LOG_SELECTION;
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            area.show(ui, |ui| {
                // A selectable label, but with each line's baseline where Tk puts it: `ascent`
                // below the line's top.
                let font = &theme::LOG_FONT;
                let shift = font.ascent as f32 - text::baseline_offset(&galley, font);
                let sense = Sense::click_and_drag() - Sense::FOCUSABLE;
                let (rect, response) = ui.allocate_exact_size(galley.size(), sense);
                let response = response.on_hover_cursor(CursorIcon::Text);
                egui::text_selection::LabelSelectionState::label_text_selection(
                    ui,
                    &response,
                    rect.min + egui::vec2(0.0, shift),
                    galley,
                    theme::log_color(&LogLevel::Info),
                    egui::Stroke::NONE,
                );
            })
        })
        .inner;

    if let Some(bar) = geom.log.scrollbar {
        let top = output.state.clamped_offset().y;
        let start = (top / content_h).clamp(0.0, 1.0);
        let end = ((top + view_h) / content_h).clamp(0.0, 1.0);
        let hovered = ui.rect_contains_pointer(bar.rect());
        let color = if hovered {
            theme::SCROLL_THUMB_HOVER
        } else {
            theme::SCROLL_THUMB
        };
        widgets::scrollbar_thumb(ui.painter(), bar, start, end, color);
    }
}

/// Paint the whole window for `state` and collect what the user did.
pub fn show(ui: &mut Ui, state: &ViewState) -> Vec<UiAction> {
    let ctx = ui.ctx().clone();
    let window = ui.max_rect().size();
    let geom = geometry(&ctx, state, window);
    let mut actions = Vec::new();
    let painter = ui.painter().clone();
    let root = ui.scope_id().with("launcher");

    painter.rect_filled(ui.max_rect(), 0.0, theme::WINDOW_BG);

    // Header.
    widgets::rounded(
        &painter,
        geom.layout.header.painted(),
        theme::HEADER_RADIUS,
        theme::HEADER_BG,
    );
    geom.title.paint(&ctx, &painter, theme::TEXT_WHITE);

    // Games column.
    heading(ui, &geom.games_heading);
    game_card(
        ui,
        root.with("game:none"),
        &geom.none_card,
        None,
        state.selected_game.is_none(),
        &mut actions,
    );
    for (game, g) in state.games.iter().zip(&geom.games) {
        let selected = state.selected_game.as_deref() == Some(game.name.as_str());
        game_card(
            ui,
            root.with(("game", &game.name)),
            g,
            Some(game),
            selected,
            &mut actions,
        );
    }

    // Apps column.
    heading(ui, &geom.apps_heading);
    if button(
        ui,
        root.with("select_all"),
        &geom.select_all,
        theme::SMALL_BUTTON_RADIUS,
        theme::GREY_BUTTON,
        true,
        &state.select_all_label,
    ) {
        actions.push(UiAction::ToggleSelectAll);
    }
    for (app, a) in state.apps.iter().zip(&geom.apps) {
        app_card(ui, root.with(("app", &app.name)), a, app, &mut actions);
    }

    // Log column.
    heading(ui, &geom.log_heading);
    log(ui, &geom, state);

    // Action buttons.
    if button(
        ui,
        root.with("launch"),
        &geom.launch,
        theme::ACTION_BUTTON_RADIUS,
        theme::LAUNCH_BUTTON,
        state.launch_enabled,
        &state.launch_label,
    ) {
        actions.push(UiAction::Launch);
    }
    if button(
        ui,
        root.with("close"),
        &geom.close,
        theme::ACTION_BUTTON_RADIUS,
        theme::CLOSE_BUTTON,
        state.close_enabled,
        &state.close_label,
    ) {
        actions.push(UiAction::Close);
    }

    // Footer.
    geom.footer.paint(&ctx, &painter, theme::TEXT_DIM);
    actions
}
