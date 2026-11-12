//! Ports `ui/sections/` (header, apps, games, log, buttons, footer): paints the geometry
//! [`geometry`](crate::ui::geometry::geometry) resolves and turns clicks into actions.

use crate::model::{AppCard, GameCard, Status, UiAction, ViewState};
use crate::ui::geometry::{AppCardGeom, ButtonGeom, GameCardGeom, NONE_LABEL, TextGeom, geometry};
use crate::ui::input::{Control, Controls, Kind, Ring};
use crate::ui::layout::CardParts;
use crate::ui::log as log_view;
use crate::ui::pack::Bounds;
use crate::ui::text;
use crate::ui::theme;
use crate::ui::widgets::{self, ButtonLook};
use egui::{Color32, Id, Role, Ui, WidgetInfo};

impl TextGeom {
    /// Paint the text, clipped to its label as Tk clips a label's text at the widget's edge.
    fn paint(&self, ctx: &egui::Context, painter: &egui::Painter, color: Color32) {
        if !self.label.is_mapped() {
            return;
        }
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(self.label.rect()));
        text::paint_block(
            ctx, &painter, &self.text, &self.font, color, self.left, self.top,
        );
    }
}

/// A CTkButton that emits `action` when pressed.
struct ButtonSpec<'a> {
    id: Id,
    geom: &'a ButtonGeom,
    radius: f32,
    colors: theme::ButtonColors,
    enabled: bool,
    action: UiAction,
    a11y: &'a str,
}

fn button(ui: &Ui, controls: &mut Controls, b: ButtonSpec<'_>) {
    if !b.geom.rect.is_mapped() {
        return;
    }
    let pointer = controls.control(
        ui,
        Control {
            id: b.id,
            hit: vec![b.geom.rect],
            enabled: b.enabled,
            action: Some(b.action),
            kind: Kind::Button,
            ring: Ring::Rounded {
                rect: b.geom.rect.painted(),
                radius: b.radius,
            },
            info: WidgetInfo::labeled(Role::Button, b.enabled, b.a11y),
        },
    );
    widgets::button(
        ui.ctx(),
        ui.painter(),
        &ButtonLook {
            rect: b.geom.rect,
            label: b.geom.label,
            text: &b.geom.text.text,
            font: &b.geom.text.font,
            corner_radius: b.radius,
            colors: b.colors,
            enabled: b.enabled,
            hovered: pointer.hover_look,
        },
    );
}

fn browse(
    ui: &Ui,
    controls: &mut Controls,
    id: Id,
    geom: &ButtonGeom,
    name: &str,
    action: UiAction,
) {
    button(
        ui,
        controls,
        ButtonSpec {
            id,
            geom,
            radius: theme::SMALL_BUTTON_RADIUS,
            colors: theme::BROWSE_BUTTON,
            enabled: true,
            action,
            a11y: &format!("Browse for {name}"),
        },
    );
}

/// Where a CTkCheckBox or CTkRadioButton reacts to the pointer: its box canvas and its empty
/// text label, the widget's last column. The 6 px spacing column between them is bound to
/// nothing. The label's height is not measured; it is taken as the widget's.
fn toggle_hit(parts: &CardParts) -> Vec<Bounds> {
    let t = parts.toggle;
    vec![parts.toggle_box, Bounds::new(t.right() - 1, t.y, 1, t.h)]
}

fn heading(ui: &Ui, geom: &TextGeom) {
    geom.paint(ui.ctx(), ui.painter(), theme::TEXT_HEADING);
}

fn game_card(
    ui: &Ui,
    controls: &mut Controls,
    id: Id,
    geom: &GameCardGeom,
    game: Option<&GameCard>,
    selected: bool,
) {
    if !geom.card.is_mapped() {
        return;
    }
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
    // CTkRadioButton.invoke only acts when it is not already selected.
    let action = (!selected).then(|| UiAction::SelectGame(game.map(|g| g.name.clone())));
    let radio = controls.control(
        ui,
        Control {
            id: id.with("radio"),
            hit: toggle_hit(&geom.parts),
            enabled: found,
            action,
            kind: Kind::Toggle {
                changes_state: !selected,
            },
            ring: Ring::Circle(geom.parts.toggle_box),
            info: WidgetInfo::selected(Role::RadioButton, found, selected, label),
        },
    );
    widgets::radio(
        painter,
        geom.parts.toggle_box,
        selected,
        radio.hover_look,
        theme::CARD_FILL,
    );

    let name_color = if found {
        theme::TEXT_WHITE
    } else {
        theme::TEXT_DIM
    };
    geom.name.paint(ui.ctx(), painter, name_color);

    if let Some(game) = game {
        if let Some(b) = &geom.browse {
            let action = UiAction::BrowseGame(game.name.clone());
            browse(ui, controls, id.with("browse"), b, &game.name, action);
        } else {
            widgets::dot(
                painter,
                geom.parts.trailing,
                theme::status_color(game.status),
            );
        }
    }
}

fn app_card(ui: &Ui, controls: &mut Controls, id: Id, geom: &AppCardGeom, app: &AppCard) {
    if !geom.card.is_mapped() {
        return;
    }
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
    let check = controls.control(
        ui,
        Control {
            id: id.with("checkbox"),
            hit: toggle_hit(&geom.parts),
            enabled: found,
            action: Some(UiAction::ToggleApp {
                name: app.name.clone(),
                checked: !app.checked,
            }),
            kind: Kind::Toggle {
                changes_state: true,
            },
            ring: Ring::Rounded {
                rect: geom.parts.toggle_box.painted(),
                radius: theme::CHECKBOX_RADIUS,
            },
            info: WidgetInfo::selected(Role::CheckBox, found, app.checked, &app.name),
        },
    );
    widgets::checkbox(
        painter,
        geom.parts.toggle_box,
        app.checked,
        check.hover_look,
        theme::CARD_FILL,
    );

    let name_color = if found {
        theme::TEXT_WHITE
    } else {
        theme::TEXT_DIM
    };
    geom.name.paint(ui.ctx(), painter, name_color);
    geom.subtitle.paint(ui.ctx(), painter, theme::TEXT_DIM);

    if let Some(b) = &geom.browse {
        let action = UiAction::BrowseApp(app.name.clone());
        browse(ui, controls, id.with("browse"), b, &app.name, action);
    } else {
        widgets::dot(
            painter,
            geom.parts.trailing,
            theme::status_color(app.status),
        );
    }
}

/// Paint the whole window for `state` and collect what the user did.
pub(super) fn show(ui: &mut Ui, state: &ViewState) -> Vec<UiAction> {
    let ctx = ui.ctx().clone();
    let window = ui.max_rect().size();
    let geom = geometry(&ctx, state, window);
    let mut controls = Controls::begin(ui);
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

    // Controls are added in reading order, which is the Tab order: the games column top to
    // bottom, the apps column (Select All, then each card), the log, then Launch and Close.
    heading(ui, &geom.games_heading);
    game_card(
        ui,
        &mut controls,
        root.with("game:none"),
        &geom.none_card,
        None,
        state.selected_game.is_none(),
    );
    for (game, g) in state.games.iter().zip(&geom.games) {
        let selected = state.selected_game.as_deref() == Some(game.name.as_str());
        game_card(
            ui,
            &mut controls,
            root.with(("game", &game.name)),
            g,
            Some(game),
            selected,
        );
    }

    heading(ui, &geom.apps_heading);
    button(
        ui,
        &mut controls,
        ButtonSpec {
            id: root.with("select_all"),
            geom: &geom.select_all,
            radius: theme::SMALL_BUTTON_RADIUS,
            colors: theme::GREY_BUTTON,
            enabled: true,
            action: UiAction::ToggleSelectAll,
            a11y: &state.select_all_label,
        },
    );
    for (app, a) in state.apps.iter().zip(&geom.apps) {
        app_card(ui, &mut controls, root.with(("app", &app.name)), a, app);
    }

    heading(ui, &geom.log_heading);
    log_view::show(
        ui,
        &mut controls,
        log_view::LogGeom {
            log_box: geom.layout.log.log_box,
            text: &geom.log,
        },
        state,
    );

    for (id, b, colors, enabled, action, label) in [
        (
            "launch",
            &geom.launch,
            theme::LAUNCH_BUTTON,
            state.launch_enabled,
            UiAction::Launch,
            &state.launch_label,
        ),
        (
            "close",
            &geom.close,
            theme::CLOSE_BUTTON,
            state.close_enabled,
            UiAction::Close,
            &state.close_label,
        ),
    ] {
        button(
            ui,
            &mut controls,
            ButtonSpec {
                id: root.with(id),
                geom: b,
                radius: theme::ACTION_BUTTON_RADIUS,
                colors,
                enabled,
                action,
                a11y: label,
            },
        );
    }

    geom.footer.paint(&ctx, &painter, theme::TEXT_DIM);
    controls.finish(ui)
}
