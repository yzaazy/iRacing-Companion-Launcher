//! Ports `core/app_definitions.py`: the companion apps and racing games the launcher knows.

#[derive(Debug, Clone, PartialEq)]
pub struct AppDef {
    pub name: &'static str,
    pub exe: &'static str,
    pub shortcut_names: &'static [&'static str],
    pub paths: Vec<String>,
    pub needs_shell_launch: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameDef {
    pub name: &'static str,
    pub exe: &'static str,
    pub shortcut_names: &'static [&'static str],
    pub steam_appid: &'static str,
    pub steam_folder: &'static str,
    pub paths: &'static [&'static str],
}

/// Companion apps in Python order; Garage61's path is resolved from `%APPDATA%`.
pub fn apps() -> Vec<AppDef> {
    todo!()
}

/// Racing games in Python order.
pub fn games() -> Vec<GameDef> {
    todo!()
}
