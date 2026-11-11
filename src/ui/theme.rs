//! Ports `ui/constants.py`: every colour, corner radius, border width and font the UI uses.
//!
//! Values come from the Python source and CustomTkinter's dark `blue.json` theme; `ui-spec.json`
//! confirms each one against the reference screenshots (`colors`, `shapes`, `fonts`).

use crate::contract::{LogLevel, Status};
use egui::Color32;

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// Surfaces.
/// CTk window background (`gray14`).
pub const WINDOW_BG: Color32 = hex(0x242424);
/// `BG_SECONDARY`, the header bar.
pub const HEADER_BG: Color32 = hex(0x252526);
/// `BG_TERTIARY`, cards and the log box.
pub const CARD_FILL: Color32 = hex(0x2d2d30);
pub const CARD_BORDER: Color32 = hex(0x3e3e42);
/// Game cards are framed in the Launch blue.
pub const GAME_CARD_BORDER: Color32 = hex(0x0e639c);
pub const LOG_BG: Color32 = CARD_FILL;

// Text.
pub const TEXT_WHITE: Color32 = hex(0xffffff);
/// `FG_SECONDARY`, section headings.
pub const TEXT_HEADING: Color32 = hex(0xcccccc);
/// Subtitles, the footer, and names of apps and games that were not found.
pub const TEXT_DIM: Color32 = hex(0x888888);
/// CTkButton `text_color`.
pub const BUTTON_TEXT: Color32 = hex(0xdce4ee);
/// CTkButton `text_color_disabled` (`gray60`).
pub const BUTTON_TEXT_DISABLED: Color32 = hex(0x999999);

/// Fill and hover fill of a CTkButton.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonColors {
    pub fill: Color32,
    pub hover: Color32,
    pub text: Color32,
}

pub const GREY_BUTTON: ButtonColors = ButtonColors {
    fill: hex(0x555555),
    hover: hex(0x666666),
    text: BUTTON_TEXT,
};
pub const BROWSE_BUTTON: ButtonColors = ButtonColors {
    text: TEXT_WHITE,
    ..GREY_BUTTON
};
pub const LAUNCH_BUTTON: ButtonColors = ButtonColors {
    fill: hex(0x0e639c),
    hover: hex(0x1177bb),
    text: BUTTON_TEXT,
};
pub const CLOSE_BUTTON: ButtonColors = ButtonColors {
    fill: hex(0xc72e2e),
    hover: hex(0xe04343),
    text: BUTTON_TEXT,
};

// CTkCheckBox and CTkRadioButton (blue.json, dark).
pub const CHECK_FILL: Color32 = hex(0x1f6aa5);
/// CTkCheckBox `hover_color`: same blue as the fill; an unchecked box fills with it on hover.
pub const CHECK_HOVER: Color32 = hex(0x1f6aa5);
pub const CHECK_BORDER: Color32 = hex(0x949a9f);
/// `checkmark_color` (`gray90`).
pub const CHECKMARK: Color32 = hex(0xe5e5e5);
pub const RADIO_FILL: Color32 = hex(0x1f6aa5);
/// CTkRadioButton `hover_color`: the ring turns this colour on hover, checked or not.
pub const RADIO_HOVER: Color32 = hex(0x144870);
pub const RADIO_BORDER: Color32 = hex(0x949a9f);

// The log's CTkScrollbar: `gray41`, `gray53` on hover.
pub const SCROLL_THUMB: Color32 = hex(0x696969);
pub const SCROLL_THUMB_HOVER: Color32 = hex(0x878787);
/// Tk's default selection background on Windows (`SystemHighlight`).
pub const LOG_SELECTION: Color32 = hex(0x0078d7);

// Corner radii and border widths (CTk `corner_radius`, `border_width`).
pub const CARD_RADIUS: f32 = 6.0;
pub const CARD_BORDER_WIDTH: f32 = 1.0;
pub const HEADER_RADIUS: f32 = 6.0;
pub const LOG_RADIUS: f32 = 6.0;
pub const SMALL_BUTTON_RADIUS: f32 = 6.0;
pub const ACTION_BUTTON_RADIUS: f32 = 12.0;
pub const CHECKBOX_RADIUS: f32 = 6.0;
pub const CHECKBOX_BORDER_WIDTH: f32 = 3.0;
pub const RADIO_BORDER_UNCHECKED: f32 = 3.0;
pub const RADIO_BORDER_CHECKED: f32 = 6.0;
/// CTkScrollbar width; the thumb is a pill of half this radius.
pub const SCROLLBAR_WIDTH: f32 = 8.0;

/// `STATUS_COLORS`; `stopping` has no entry in Python and falls back to idle grey.
pub fn status_color(status: &Status) -> Color32 {
    match status {
        Status::Starting => hex(0xffa726),
        Status::Running => hex(0x66bb6a),
        Status::Failed => hex(0xef5350),
        Status::Idle | Status::Stopping | Status::Stopped | Status::NotFound => hex(0x666666),
    }
}

/// `LOG_COLORS`.
pub fn log_color(level: &LogLevel) -> Color32 {
    match level {
        LogLevel::Info => hex(0xd4d4d4),
        LogLevel::Success => hex(0x66bb6a),
        LogLevel::Error => hex(0xef5350),
        LogLevel::Warning => hex(0xffa726),
        LogLevel::Launch => hex(0x0e639c),
        LogLevel::Close => hex(0xc72e2e),
        LogLevel::Divider => hex(0x666666),
    }
}

/// A Tk font: the egui family it renders with, its pixel size, and the GDI metrics Tk lays it
/// out with (`ui-spec.json` `fonts`). Tk's line height is `ascent + descent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    pub family: &'static str,
    pub px: f32,
    pub ascent: i32,
    pub descent: i32,
}

impl Font {
    pub fn linespace(&self) -> i32 {
        self.ascent + self.descent
    }

    pub fn id(&self) -> egui::FontId {
        egui::FontId::new(self.px, egui::FontFamily::Name(self.family.into()))
    }
}

/// egui family names registered by `ui::install_fonts`.
pub const SEGOE: &str = "segoe-ui";
pub const SEGOE_BOLD: &str = "segoe-ui-bold";
pub const CONSOLAS: &str = "consolas";

const fn font(family: &'static str, px: f32, ascent: i32, descent: i32) -> Font {
    Font {
        family,
        px,
        ascent,
        descent,
    }
}

/// Header title, Segoe UI 24 bold.
pub const TITLE_FONT: Font = font(SEGOE_BOLD, 24.0, 26, 6);
/// Section headings and the action buttons, Segoe UI 16 bold.
pub const HEADING_FONT: Font = font(SEGOE_BOLD, 16.0, 17, 4);
/// App and game names, Segoe UI 14 bold.
pub const NAME_FONT: Font = font(SEGOE_BOLD, 14.0, 15, 4);
/// Browse buttons, Segoe UI 13.
pub const BROWSE_FONT: Font = font(SEGOE, 13.0, 14, 3);
/// Select All, Segoe UI 12.
pub const SELECT_ALL_FONT: Font = font(SEGOE, 12.0, 12, 3);
/// Footer, Segoe UI 10.
pub const FOOTER_FONT: Font = font(SEGOE, 10.0, 10, 2);
/// Card subtitles, Segoe UI 9.
pub const SUBTITLE_FONT: Font = font(SEGOE, 9.0, 10, 2);
/// The status dot glyph, Segoe UI 40: only its metrics are used, to place the painted disk.
pub const DOT_FONT: Font = font(SEGOE, 40.0, 43, 11);
/// The activity log, Consolas 14.
pub const LOG_FONT: Font = font(CONSOLAS, 14.0, 13, 4);
