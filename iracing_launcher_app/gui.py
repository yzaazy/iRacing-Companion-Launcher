"""
Main GUI application class.
"""

import os
import sys
import time
import customtkinter as ctk
from tkinter import filedialog
from typing import Dict, Optional

from version import __version__
from .app_manager import AppManager
from .config_manager import ConfigManager
from .widgets import StatusCard, GameCard
from .constants import (
    BG_PRIMARY, BG_SECONDARY, BG_TERTIARY,
    FG_PRIMARY, FG_SECONDARY, FG_TERTIARY,
    LOG_COLORS, STATUS_CARD_HEIGHT
)


class iRacingLauncherGUI:
    """Main GUI application class for iRacing Companion Launcher."""

    def __init__(self, root: ctk.CTk):
        """
        Initialize the GUI application.

        Args:
            root: Root CustomTkinter window
        """
        self.root = root
        self.root.title("iRacing Companion Launcher")
        self.root.geometry("1200x700")
        self.root.resizable(False, False)

        # Initialize managers
        self.config_manager = ConfigManager()
        self.app_manager = AppManager(self.config_manager)

        # UI components
        self.status_cards: Dict[str, StatusCard] = {}
        self.game_cards: Dict[str, "GameCard"] = {}
        self.selected_game_var: ctk.StringVar = ctk.StringVar(value="")  # Default to None
        self.log_text: Optional[ctk.CTkTextbox] = None
        self.launch_btn: Optional[ctk.CTkButton] = None
        self.close_btn: Optional[ctk.CTkButton] = None

        self._setup_icon()
        self._create_widgets()
        self._initialize_app_states()

    def _setup_icon(self):
        """Set the application window icon."""
        try:
            # Determine the base path (works for both script and PyInstaller executable)
            if getattr(sys, 'frozen', False):
                # Running as compiled executable
                # Try to load from the temporary extraction folder
                base_dir = sys._MEIPASS
            else:
                # Running as script - load icon from file
                base_dir = os.path.dirname(os.path.dirname(__file__))

            ico_path = os.path.join(base_dir, "iRCL.ico")

            if os.path.exists(ico_path):
                try:
                    # Set icon using iconbitmap (works for title bar and taskbar)
                    self.root.iconbitmap(ico_path)
                except Exception as e:
                    print(f"iconbitmap failed: {e}")
            else:
                print(f"Icon not found at: {ico_path}")
        except Exception as e:
            print(f"Could not load icon: {e}")

    def _create_widgets(self):
        """Create and layout all UI widgets."""
        self._create_header()
        self._create_content()
        self._create_buttons()
        self._create_footer()

    def _create_header(self):
        """Create the header section."""
        header_frame = ctk.CTkFrame(self.root, height=70, fg_color=BG_SECONDARY)
        header_frame.pack(fill="x", pady=(0, 10))
        header_frame.pack_propagate(False)

        title_label = ctk.CTkLabel(
            header_frame,
            text=f"iRacing Companion Launcher v{__version__}",
            font=("Segoe UI", 24, "bold"),
            text_color=FG_PRIMARY
        )
        title_label.pack(pady=20)

    def _create_content(self):
        """Create the main content area with games, apps, and log."""
        content_frame = ctk.CTkFrame(self.root, fg_color="transparent")
        content_frame.pack(pady=5, padx=15, fill="both", expand=True)

        # Left side - Race Games
        self._create_games_section(content_frame)

        # Middle - Companion Apps
        self._create_apps_section(content_frame)

        # Right side - Activity Log
        self._create_log_section(content_frame)

    def _create_games_section(self, parent: ctk.CTkFrame):
        """
        Create the race games section.

        Args:
            parent: Parent frame widget
        """
        games_container = ctk.CTkFrame(parent, fg_color="transparent")
        games_container.pack(side="left", fill="both", expand=True, padx=(0, 10))

        # Header frame with label
        header_frame = ctk.CTkFrame(games_container, fg_color="transparent", height=30)
        header_frame.pack(fill="x", pady=(0, 10))
        header_frame.pack_propagate(False)

        games_label = ctk.CTkLabel(
            header_frame,
            text="Racing Simulators & Games",
            font=("Segoe UI", 16, "bold"),
            text_color=FG_SECONDARY,
            anchor="w"
        )
        games_label.pack(side="left")

        # Cards frame
        cards_frame = ctk.CTkFrame(games_container, fg_color="transparent")
        cards_frame.pack(fill="both", expand=True)

        # Add "None" option at the top
        none_frame = ctk.CTkFrame(
            cards_frame,
            fg_color="#2d2d30",
            border_width=1,
            border_color="#3e3e42",
            height=STATUS_CARD_HEIGHT,
            corner_radius=6
        )
        none_frame.pack(fill="x", pady=(0, 5))
        none_frame.pack_propagate(False)

        none_radio = ctk.CTkRadioButton(
            none_frame,
            text="",
            width=20,
            value="",
            variable=self.selected_game_var,
            command=self._on_game_selected_wrapper
        )
        none_radio.pack(side="left", padx=(10, 5), pady=10)

        none_label = ctk.CTkLabel(
            none_frame,
            text="None",
            text_color="#ffffff",
            font=("Segoe UI", 14, "bold"),
            anchor="w"
        )
        none_label.pack(side="left", padx=(5, 15), pady=10)

        # Add actual games
        game_list = self.app_manager.get_game_list()
        for idx, game_name in enumerate(game_list):
            card = GameCard(
                cards_frame,
                game_name,
                browse_callback=self._browse_for_game,
                radio_callback=self._on_game_selected,
                radio_variable=self.selected_game_var
            )
            # Last card: no bottom padding
            if idx == len(game_list) - 1:
                pady = (5, 0)
            else:
                pady = 5

            card.pack(fill="x", pady=pady)
            self.game_cards[game_name] = card

    def _create_apps_section(self, parent: ctk.CTkFrame):
        """
        Create the companion apps section.

        Args:
            parent: Parent frame widget
        """
        apps_container = ctk.CTkFrame(parent, fg_color="transparent")
        apps_container.pack(side="left", fill="both", expand=True, padx=(0, 10))

        # Header frame with label and select all button
        header_frame = ctk.CTkFrame(apps_container, fg_color="transparent", height=30)
        header_frame.pack(fill="x", pady=(0, 10))
        header_frame.pack_propagate(False)

        apps_label = ctk.CTkLabel(
            header_frame,
            text="Companion Apps",
            font=("Segoe UI", 16, "bold"),
            text_color=FG_SECONDARY,
            anchor="w"
        )
        apps_label.pack(side="left")

        self.select_all_btn = ctk.CTkButton(
            header_frame,
            text="Deselect All",
            command=self._toggle_select_all,
            fg_color="#555555",
            hover_color="#666666",
            width=100,
            height=30,
            font=("Segoe UI", 12),
            corner_radius=6
        )
        self.select_all_btn.pack(side="right")

        cards_frame = ctk.CTkFrame(apps_container, fg_color="transparent")
        cards_frame.pack(fill="both", expand=True)

        app_list = self.app_manager.get_app_list()
        for idx, app_name in enumerate(app_list):
            card = StatusCard(
                cards_frame,
                app_name,
                browse_callback=self._browse_for_app,
                checkbox_callback=self._on_checkbox_change
            )
            # First card: no top padding; Last card: no bottom padding
            if idx == 0:
                pady = (0, 5)
            elif idx == len(app_list) - 1:
                pady = (5, 0)
            else:
                pady = 5

            card.pack(fill="x", pady=pady)
            self.status_cards[app_name] = card

            # Restore checkbox state from config
            config_key = f"{app_name.lower().replace(' ', '_')}_enabled"
            enabled = self.config_manager.config.getboolean('Settings', config_key, fallback=True)
            card.set_checked(enabled)

    def _create_log_section(self, parent: ctk.CTkFrame):
        """
        Create the activity log section.

        Args:
            parent: Parent frame widget
        """
        log_container = ctk.CTkFrame(parent, fg_color="transparent")
        log_container.pack(side="right", fill="both", expand=True)

        log_label = ctk.CTkLabel(
            log_container,
            text="Activity Log",
            font=("Segoe UI", 16, "bold"),
            text_color=FG_SECONDARY,
            anchor="w"
        )
        log_label.pack(anchor="w", pady=(0, 10))

        # Calculate height based on number of status cards
        num_apps = len(self.app_manager.get_app_list())
        # First card has no top padding, last card has no bottom padding
        # So we have (num_apps - 1) gaps of 10px between cards
        card_padding = 10 * (num_apps - 1)
        # Match the exact height of the cards section
        total_height = (STATUS_CARD_HEIGHT * num_apps) + card_padding

        self.log_text = ctk.CTkTextbox(
            log_container,
            width=400,
            height=total_height,
            font=("Consolas", 14),
            fg_color=BG_TERTIARY,
            text_color=FG_TERTIARY,
            border_width=0,
            corner_radius=6
        )
        self.log_text.pack(fill="both")
        self.log_text.configure(state="disabled")

        # Configure text tags for colored log messages
        for level, color in LOG_COLORS.items():
            self.log_text.tag_config(level, foreground=color)

    def _create_buttons(self):
        """Create the button section."""
        button_frame = ctk.CTkFrame(self.root, fg_color="transparent")
        button_frame.pack(pady=(35, 10))

        self.launch_btn = ctk.CTkButton(
            button_frame,
            text="Launch All",
            command=self.launch_apps,
            fg_color="#0e639c",
            hover_color="#1177bb",
            width=200,
            height=60,
            font=("Segoe UI", 16, "bold"),
            corner_radius=12
        )
        self.launch_btn.pack(side="left", padx=10)

        self.close_btn = ctk.CTkButton(
            button_frame,
            text="Close All",
            command=self.close_apps,
            fg_color="#c72e2e",
            hover_color="#e04343",
            width=200,
            height=60,
            font=("Segoe UI", 16, "bold"),
            corner_radius=12
        )
        self.close_btn.pack(side="left", padx=10)

    def _create_footer(self):
        """Create the footer section with copyright."""
        footer_frame = ctk.CTkFrame(self.root, fg_color="transparent")
        footer_frame.pack(side="bottom", fill="x", pady=(0, 5), padx=15)

        copyright_label = ctk.CTkLabel(
            footer_frame,
            text="© 2025 Developed & Designed by Tobias Termeczky • Vibed with Claude",
            font=("Segoe UI", 10),
            text_color="#888888",
            anchor="e"
        )
        copyright_label.pack(side="right")

    def _initialize_app_states(self):
        """Check all apps on startup and set initial states."""
        self._log_divider()
        self._log_message("Checking application status...", "info")

        for app_name in self.app_manager.get_app_list():
            app_path = self.app_manager.find_app_path(app_name)
            if not app_path:
                self.status_cards[app_name].set_status("not_found")
                self._log_message(f"{app_name} - not configured", "warning")
            else:
                # Check if the app is already running
                if self.app_manager.is_app_running(app_name):
                    self.status_cards[app_name].set_status("running")
                    self._log_message(f"{app_name} - already running", "success")
                else:
                    self.status_cards[app_name].set_status("idle")
                    self._log_message(f"{app_name} - configured", "info")

        self._update_button_text()
        self._update_select_all_button()

        # Initialize game states
        self._initialize_game_states()

    def _initialize_game_states(self):
        """Check all games on startup and set initial states."""
        for game_name in self.app_manager.get_game_list():
            game_path = self.app_manager.find_game_path(game_name)
            if not game_path:
                self.game_cards[game_name].set_status("not_found")
                self._log_message(f"{game_name} - not configured", "warning")
            else:
                # Check if the game is already running
                if self.app_manager.is_game_running(game_name):
                    self.game_cards[game_name].set_status("running")
                    self._log_message(f"{game_name} - already running", "success")
                else:
                    self.game_cards[game_name].set_status("idle")
                    self._log_message(f"{game_name} - configured", "info")

        # Restore selected game from config
        saved_game = self.config_manager.config.get('Settings', 'selected_game', fallback='')
        if saved_game and saved_game in self.game_cards:
            self.selected_game_var.set(saved_game)
        else:
            # Default to "None" if no valid saved game
            self.selected_game_var.set('')

        self._update_button_text()

    def _toggle_select_all(self):
        """Toggle all app checkboxes between selected and deselected."""
        # Check if all enabled apps are currently checked
        enabled_cards = [
            card for card in self.status_cards.values()
            if not card.is_not_found  # Only consider enabled (non-disabled) apps
        ]

        if not enabled_cards:
            return

        # Count how many are checked
        checked_count = sum(1 for card in enabled_cards if card.get_checked())

        # If all are checked, deselect all; otherwise select all
        select_all = checked_count < len(enabled_cards)

        for app_name, card in self.status_cards.items():
            if not card.is_not_found:  # Only change enabled apps
                card.set_checked(select_all)
                # Save to config
                config_key = f"{app_name.lower().replace(' ', '_')}_enabled"
                self.config_manager.config.set('Settings', config_key, str(select_all))

        # Save config once for all changes
        self.config_manager.save_config()

        # Update button texts
        self._update_button_text()
        self._update_select_all_button()

    def _update_select_all_button(self):
        """Update the select all button text based on current selection state."""
        if not self.select_all_btn:
            return

        # Check if all enabled apps are currently checked
        enabled_cards = [
            card for card in self.status_cards.values()
            if not card.is_not_found
        ]

        if not enabled_cards:
            self.select_all_btn.configure(text="Select All")
            return

        checked_count = sum(1 for card in enabled_cards if card.get_checked())

        # If all are checked, show "Deselect All", otherwise "Select All"
        if checked_count == len(enabled_cards):
            self.select_all_btn.configure(text="Deselect All")
        else:
            self.select_all_btn.configure(text="Select All")

    def _update_button_text(self):
        """Update button text to show number of selected apps and selected game."""
        if not self.launch_btn or not self.close_btn:
            return

        # Count checked apps
        checked_count = sum(
            1 for card in self.status_cards.values()
            if card.get_checked()
        )

        # Get selected game
        selected_game = self.selected_game_var.get()

        # Update button text based on game selection
        if selected_game:
            self.launch_btn.configure(text=f"Launch with {selected_game} ({checked_count})")
        else:
            self.launch_btn.configure(text=f"Launch apps ({checked_count})")

        self.close_btn.configure(text=f"Close all ({checked_count})")

        # Disable buttons only when BOTH no game AND no apps are selected
        if checked_count == 0 and not selected_game:
            self.launch_btn.configure(state="disabled")
            self.close_btn.configure(state="disabled")
        else:
            self.launch_btn.configure(state="normal")
            self.close_btn.configure(state="normal")

    def _log_message(self, message: str, level: str = "info"):
        """
        Add a message to the activity log with timestamp and color.

        Args:
            message: Message to log
            level: Log level ("info", "success", "error", "warning", "launch", "close")
        """
        if not self.log_text:
            return

        self.log_text.configure(state="normal")

        timestamp = time.strftime("%H:%M:%S")
        # Insert the message with colored text using tags
        self.log_text.insert("end", f"[{timestamp}] {message}\n", level)

        self.log_text.see("end")
        self.log_text.configure(state="disabled")
        self.root.update()

    def _log_divider(self):
        """Add a visual divider line to the activity log."""
        if not self.log_text:
            return

        self.log_text.configure(state="normal")
        self.log_text.insert("end", "─" * 50 + "\n", "divider")
        self.log_text.see("end")
        self.log_text.configure(state="disabled")

    def _clear_log(self):
        """Clear all text from the activity log."""
        if not self.log_text:
            return

        self.log_text.configure(state="normal")
        self.log_text.delete("1.0", "end")
        self.log_text.configure(state="disabled")

    def _on_checkbox_change(self, app_name: str, checked: bool):
        """
        Handle checkbox state change and save to config.

        Args:
            app_name: Name of the application
            checked: New checkbox state
        """
        config_key = f"{app_name.lower().replace(' ', '_')}_enabled"
        self.config_manager.config.set('Settings', config_key, str(checked))
        self.config_manager.save_config()
        self._update_button_text()
        self._update_select_all_button()

    def _browse_for_app(self, app_name: str):
        """
        Open file dialog to manually select app executable.

        Args:
            app_name: Name of the application
        """
        expected_exe = self.app_manager.get_app_exe(app_name)
        if not expected_exe:
            return

        # Open file dialog
        file_path = filedialog.askopenfilename(
            title=f"Select {app_name} executable",
            filetypes=[("Executable files", "*.exe"), ("All files", "*.*")],
            initialdir="C:\\Program Files"
        )

        if not file_path:
            return  # User cancelled

        # Validate that the selected file matches the expected exe name
        selected_exe = os.path.basename(file_path)
        if selected_exe.lower() != expected_exe.lower():
            self._log_message(
                f"Invalid file selected. Expected {expected_exe}, got {selected_exe}",
                "error"
            )
            return

        # Verify the file exists
        if not os.path.exists(file_path):
            self._log_message(f"Selected file does not exist: {file_path}", "error")
            return

        # Save to config
        config_key = ConfigManager.get_config_key(app_name)
        self.config_manager.set_app_path(config_key, file_path)

        # Update UI
        self.status_cards[app_name].set_status("idle")
        # Enable checkbox when app is configured
        self.status_cards[app_name].set_checked(True)
        # Save checkbox state to config
        checkbox_config_key = f"{app_name.lower().replace(' ', '_')}_enabled"
        self.config_manager.config.set('Settings', checkbox_config_key, str(True))
        self.config_manager.save_config()

        self._log_message(f"{app_name} path configured: {file_path}", "success")
        self._update_button_text()
        self._update_select_all_button()

    def _browse_for_game(self, game_name: str):
        """
        Open file dialog to manually select game executable or shortcut.

        Args:
            game_name: Name of the game
        """
        expected_exe = self.app_manager.get_game_exe(game_name)
        if not expected_exe:
            return

        # Open file dialog - allow both .exe and .lnk files
        file_path = filedialog.askopenfilename(
            title=f"Select {game_name} executable or shortcut",
            filetypes=[
                ("Executable and Shortcuts", "*.exe;*.lnk"),
                ("Executable files", "*.exe"),
                ("Shortcuts", "*.lnk"),
                ("All files", "*.*")
            ],
            initialdir="C:\\Program Files"
        )

        if not file_path:
            return  # User cancelled

        # Validate file
        if file_path.lower().endswith('.lnk'):
            # Accept .lnk files directly
            if not os.path.exists(file_path):
                self._log_message(f"Selected file does not exist: {file_path}", "error")
                return
        else:
            # For .exe files, validate the name matches
            selected_exe = os.path.basename(file_path)
            if selected_exe.lower() != expected_exe.lower():
                self._log_message(
                    f"Invalid file selected. Expected {expected_exe}, got {selected_exe}",
                    "error"
                )
                return

            if not os.path.exists(file_path):
                self._log_message(f"Selected file does not exist: {file_path}", "error")
                return

        # Save to config
        config_key = f"game_{ConfigManager.get_config_key(game_name)}"
        self.config_manager.set_app_path(config_key, file_path)

        # Update UI
        self.game_cards[game_name].set_status("idle")
        self._log_message(f"{game_name} path configured: {file_path}", "success")
        self._update_button_text()

    def _on_game_selected_wrapper(self):
        """Handle game selection from radio button (wrapper for None option)."""
        game_name = self.selected_game_var.get()
        self._on_game_selected(game_name)

    def _on_game_selected(self, game_name: str):
        """
        Handle game radio button selection.

        Args:
            game_name: Name of the selected game (empty string for None)
        """
        # Save selected game to config
        self.config_manager.config.set('Settings', 'selected_game', game_name)
        self.config_manager.save_config()
        self._update_button_text()

    def launch_apps(self):
        """Launch all configured companion applications."""
        if not self.launch_btn:
            return

        self.launch_btn.configure(state="disabled")

        self._log_divider()
        self._log_message("Starting launch sequence...", "launch")

        for app_name in self.app_manager.get_app_list():
            # Skip unchecked apps
            if not self.status_cards[app_name].get_checked():
                continue

            app_path = self.app_manager.find_app_path(app_name)

            if not app_path:
                self._log_message(f"Skipping {app_name} - not configured", "warning")
                continue

            # Check if app is already running
            if self.app_manager.is_app_running(app_name):
                self._log_message(f"Skipping {app_name} - already running", "info")
                self.status_cards[app_name].set_status("running")
                continue

            self.status_cards[app_name].set_status("starting")
            self._log_message(f"Launching {app_name}...", "info")

            success = self.app_manager.launch_app(app_name, app_path)
            if success:
                self._log_message(f"{app_name} started successfully", "success")
                self.status_cards[app_name].set_status("running")
            else:
                self._log_message(f"{app_name} failed to start", "error")
                self._log_message(f"Try reinstalling {app_name}", "warning")
                self.status_cards[app_name].set_status("failed")

        self._log_message("All apps launched!", "success")

        # Launch selected game after apps if one is selected
        selected_game = self.selected_game_var.get()
        if selected_game:
            self._launch_selected_game(selected_game)

        self._log_message("Launch sequence complete!", "success")
        self.launch_btn.configure(state="normal")

    def _launch_selected_game(self, game_name: str):
        """
        Launch the selected game.

        Args:
            game_name: Name of the game to launch
        """
        game_path = self.app_manager.find_game_path(game_name)

        if not game_path:
            self._log_message(f"Skipping {game_name} - not configured", "warning")
            return

        # Check if game is already running
        if self.app_manager.is_game_running(game_name):
            self._log_message(f"Skipping {game_name} - already running", "info")
            self.game_cards[game_name].set_status("running")
            return

        self.game_cards[game_name].set_status("starting")
        self._log_message(f"Launching {game_name}...", "launch")

        success = self.app_manager.launch_game(game_name, game_path)
        if success:
            self._log_message(f"{game_name} started successfully", "success")
            self.game_cards[game_name].set_status("running")
        else:
            self._log_message(f"{game_name} failed to start", "error")
            self._log_message(f"Try reinstalling {game_name} or selecting a different path", "warning")
            self.game_cards[game_name].set_status("failed")

    def close_apps(self):
        """Close all companion applications."""
        if not self.close_btn:
            return

        self.close_btn.configure(state="disabled")

        self._log_divider()
        self._log_message("Closing applications...", "close")

        # Close main apps
        for app_name in self.app_manager.get_app_list():
            # Skip unchecked apps
            if not self.status_cards[app_name].get_checked():
                continue

            # Skip apps without paths (but still check if process is running)
            app_path = self.app_manager.find_app_path(app_name)
            if not app_path:
                # Still try to close if process is somehow running
                killed = self.app_manager.close_app(app_name)
                if not killed:
                    continue  # Skip if not configured and not running

            self.status_cards[app_name].set_status("stopping")
            self._log_message(f"Closing {app_name}...", "info")

            killed = self.app_manager.close_app(app_name)

            if killed:
                self._log_message(f"{app_name} closed", "success")
                self.status_cards[app_name].set_status("stopped")
            else:
                self._log_message(f"{app_name} was not running", "warning")
                # Restore "not_found" state if app doesn't have a path
                if not app_path:
                    self.status_cards[app_name].set_status("not_found")
                else:
                    self.status_cards[app_name].set_status("idle")

        # Also close Garage61 Agent (special case) - only if Garage61 is checked
        if "Garage61" in self.status_cards and self.status_cards["Garage61"].get_checked():
            self._log_message("Closing Garage61 Agent...", "info")
            if self.app_manager.close_process_by_name("garage61-agent.exe"):
                self._log_message("Garage61 Agent closed", "success")
            else:
                self._log_message("Garage61 Agent was not running", "warning")

        # Check game status and update card
        selected_game = self.selected_game_var.get()
        if selected_game:
            if self.app_manager.is_game_running(selected_game):
                # Game is still running - warn user
                self._log_message(
                    f"⚠ {selected_game} is still running - please close it manually",
                    "warning"
                )
                # Status stays as "running"
            else:
                # Game was closed - update status to idle
                self.game_cards[selected_game].set_status("idle")

        self._log_message("All apps closed!", "success")
        self.close_btn.configure(state="normal")
