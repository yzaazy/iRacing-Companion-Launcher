"""
iRacing Companion Launcher - Main Entry Point

A modern GUI application to launch and manage iRacing companion apps.
"""

import customtkinter as ctk
from iracing_launcher_app.ui.main_window import iRacingLauncherGUI


def main():
    """Application entry point."""
    # Set appearance mode and color theme
    ctk.set_appearance_mode("dark")
    ctk.set_default_color_theme("blue")

    root = ctk.CTk()
    app = iRacingLauncherGUI(root)
    root.mainloop()


if __name__ == "__main__":
    main()
