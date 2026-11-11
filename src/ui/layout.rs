//! Rule-based window layout reproducing the measured Python geometry (`docs/rewrite/ui-spec.json`).

/// Placeholder fields: `contracts.md` leaves the shape of `Layout` to the ui agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub window: egui::Vec2,
}

pub fn layout(window: egui::Vec2, n_apps: usize, n_games: usize) -> Layout {
    Layout { window }
}
