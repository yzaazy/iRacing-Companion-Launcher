//! Shared types between the UI, the controller and the worker (no single Python file; see docs/rewrite/contracts.md).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Idle,
    Starting,
    Running,
    Failed,
    Stopping,
    Stopped,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Success,
    Error,
    Warning,
    Launch,
    Close,
    Divider,
}

/// One activity log line. `text` is final, timestamp included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppCard {
    pub name: String,
    pub status: Status,
    pub checked: bool,
    pub helper_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GameCard {
    pub name: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ViewState {
    pub version: String,
    pub apps: Vec<AppCard>,
    pub games: Vec<GameCard>,
    /// `None` is the "None" radio; `null` in JSON.
    pub selected_game: Option<String>,
    pub log: Vec<LogLine>,
    pub select_all_label: String,
    pub launch_label: String,
    pub close_label: String,
    pub launch_enabled: bool,
    pub close_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAction {
    ToggleApp { name: String, checked: bool },
    ToggleSelectAll,
    SelectGame(Option<String>),
    BrowseApp(String),
    BrowseGame(String),
    Launch,
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Launch {
        apps: Vec<String>,
        game: Option<String>,
    },
    Close {
        apps: Vec<String>,
        game: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Log(LogLine),
    AppStatus {
        name: String,
        status: Status,
        helper_count: usize,
    },
    GameStatus {
        name: String,
        status: Status,
    },
    LaunchFinished,
    CloseFinished,
}
