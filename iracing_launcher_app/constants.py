"""
Application constants and configuration values.
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
STATUS_CARD_HEIGHT = 50

# Application definitions
APPS = {
    "Fanatec": {
        "exe": "Fanatec.exe",
        "shortcut_names": ["Fanatec.lnk", "Fanatec Control Panel.lnk"],
        "paths": [
            r"C:\Program Files\Fanatec\FanatecUI\UI\Fanatec.exe",
            r"C:\Program Files (x86)\Fanatec\FanatecUI\UI\Fanatec.exe"
        ]
    },
    "Crew Chief V4": {
        "exe": "CrewChiefV4.exe",
        "shortcut_names": ["Crew Chief V4.lnk", "CrewChiefV4.lnk"],
        "paths": [
            r"C:\Program Files (x86)\Britton IT Ltd\CrewChiefV4\CrewChiefV4.exe",
            r"C:\Program Files\Britton IT Ltd\CrewChiefV4\CrewChiefV4.exe"
        ]
    },
    "Trading Paints": {
        "exe": "Trading Paints.exe",
        "shortcut_names": ["Trading Paints.lnk"],
        "paths": [
            r"C:\Program Files (x86)\Rhinode LLC\Trading Paints\Trading Paints.exe",
            r"C:\Program Files\Rhinode LLC\Trading Paints\Trading Paints.exe"
        ]
    },
    "SimHub": {
        "exe": "SimHubWPF.exe",
        "shortcut_names": ["SimHub.lnk"],
        "paths": [
            r"C:\Program Files (x86)\SimHub\SimHubWPF.exe",
            r"C:\Program Files\SimHub\SimHubWPF.exe"
        ]
    },
    "Garage61": {
        "exe": "garage61-launcher.exe",
        "shortcut_names": ["Garage 61 Telemetry Agent.lnk", "Garage61.lnk", "garage61.lnk"],
        "paths": []  # Will be populated at runtime
    },
    "Bloops": {
        "exe": "Bloops.exe",
        "shortcut_names": ["Bloops.lnk"],
        "paths": []  # Will be populated at runtime
    }
}
