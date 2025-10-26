"""
UI constants and theme configuration.
"""

# UI Colors (CustomTkinter compatible)
# CustomTkinter handles most theming automatically, but we keep custom colors for specific elements
BG_PRIMARY = "transparent"  # Use CTk's default background
BG_SECONDARY = "#252526"
BG_TERTIARY = "#2d2d30"
FG_PRIMARY = "#ffffff"
FG_SECONDARY = "#cccccc"
FG_TERTIARY = "#d4d4d4"

# Log level colors
LOG_COLORS = {
    "info": "#d4d4d4",
    "success": "#66BB6A",
    "error": "#EF5350",
    "warning": "#FFA726",
    "launch": "#0e639c",  # Blue - matches Launch button
    "close": "#c72e2e",   # Red - matches Close button
    "divider": "#666666"  # Gray for divider lines
}

# Status color constants
STATUS_COLORS = {
    "idle": "#666666",
    "starting": "#FFA726",
    "running": "#66BB6A",
    "failed": "#EF5350",
    "stopped": "#666666",
    "not_found": "#666666"
}

# Widget dimensions
STATUS_CARD_HEIGHT = 55
