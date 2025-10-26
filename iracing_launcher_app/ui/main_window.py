"""
Main GUI window for iRacing Companion Launcher.
"""

import os
import sys
import customtkinter as ctk
from tkinter import filedialog
from typing import Dict, Optional

from ..core.config_manager import ConfigManager
from ..core.activity_logger import ActivityLogger
from ..managers.app_manager import AppManager
from ..managers.game_manager import GameManager
from .sections.header import HeaderSection
from .sections.footer import FooterSection
from .sections.apps_section import AppsSection
from .sections.games_section import GamesSection
from .sections.log_section import LogSection
from .sections.buttons_section import ButtonsSection
from .widgets.status_card import StatusCard
from .widgets.game_card import GameCard


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
        self.game_manager = GameManager(self.config_manager)
        self.logger = ActivityLogger()

        # UI components
        self.status_cards: Dict[str, StatusCard] = {}
        self.game_cards: Dict[str, GameCard] = {}
        self.selected_game_var: ctk.StringVar = ctk.StringVar(value="")
        self.select_all_btn: Optional[ctk.CTkButton] = None
        self.launch_btn: Optional[ctk.CTkButton] = None
        self.close_btn: Optional[ctk.CTkButton] = None

        # UI Sections
        self.header_section = None
        self.footer_section = None
        self.apps_section = None
        self.games_section = None
        self.log_section = None
        self.buttons_section = None

        self._setup_icon()
        self._create_widgets()
        self._initialize_app_states()

    def _setup_icon(self):
        """Set the application window icon."""
        try:
            # Determine the base path (works for both script and PyInstaller executable)
            if getattr(sys, 'frozen', False):
                # Running as compiled executable
                base_dir = sys._MEIPASS
            else:
                # Running as script - load icon from file
                base_dir = os.path.dirname(os.path.dirname(os.path.dirname(__file__)))

            ico_path = os.path.join(base_dir, "iRCL.ico")

            if os.path.exists(ico_path):
                try:
                    self.root.iconbitmap(ico_path)
                except Exception as e:
                    print(f"iconbitmap failed: {e}")
            else:
                print(f"Icon not found at: {ico_path}")
        except Exception as e:
            print(f"Could not load icon: {e}")

    def _create_widgets(self):
        """Create and layout all UI widgets."""
        # Create header and footer
        self.header_section = HeaderSection(self.root)
        self.footer_section = FooterSection(self.root)

        # Create main content frame
        content_frame = ctk.CTkFrame(self.root, fg_color="transparent")
        content_frame.pack(pady=5, padx=15, fill="both", expand=True)

        # Create games section
        self.games_section = GamesSection(
            content_frame,
            self.game_manager.get_game_list(),
            self._browse_for_game,
            self._on_game_selected,
            self.selected_game_var
        )
        self.game_cards = self.games_section.get_cards()

        # Create apps section
        self.apps_section = AppsSection(
            content_frame,
            self.app_manager.get_app_list(),
            self._browse_for_app,
            self._on_checkbox_change,
            self._toggle_select_all
        )
        self.status_cards = self.apps_section.get_cards()
        self.select_all_btn = self.apps_section.get_select_all_button()

        # Create log section
        self.log_section = LogSection(
            content_frame,
            len(self.app_manager.get_app_list())
        )
        self.logger.set_widget(self.log_section.get_widget())

        # Create buttons section
        self.buttons_section = ButtonsSection(
            self.root,
            self.launch_apps,
            self.close_apps
        )
        self.launch_btn = self.buttons_section.get_launch_button()
        self.close_btn = self.buttons_section.get_close_button()

    def _initialize_app_states(self):
        """Check all apps on startup and set initial states."""
        self.logger.log_divider()
        self.logger.info("Checking application status...")

        for app_name in self.app_manager.get_app_list():
            app_path = self.app_manager.find_app_path(app_name)
            config_key = f"{app_name.lower().replace(' ', '_')}_enabled"

            if not app_path:
                self.status_cards[app_name].set_status("not_found")
                self.logger.warning(f"{app_name} - not configured")
                # Not-found apps should not be selected - save this to config
                self.config_manager.config.set('Settings', config_key, 'False')
            else:
                if self.app_manager.is_app_running(app_name):
                    self.status_cards[app_name].set_status("running")
                    self.logger.success(f"{app_name} - already running")
                else:
                    self.status_cards[app_name].set_status("idle")
                    self.logger.info(f"{app_name} - configured")

                # Only restore checkbox state for found apps
                enabled = self.config_manager.config.getboolean('Settings', config_key, fallback=True)
                self.status_cards[app_name].set_checked(enabled)

        # Save config after initialization
        self.config_manager.save_config()

        # Initialize game states
        for game_name in self.game_manager.get_game_list():
            game_path = self.game_manager.find_game_path(game_name)
            if not game_path:
                self.game_cards[game_name].set_status("not_found")
                self.logger.warning(f"{game_name} - not configured")
            else:
                if self.game_manager.is_game_running(game_name):
                    self.game_cards[game_name].set_status("running")
                    self.logger.success(f"{game_name} - already running")
                else:
                    self.game_cards[game_name].set_status("idle")
                    self.logger.info(f"{game_name} - configured")

        # Restore selected game from config
        saved_game = self.config_manager.config.get('Settings', 'selected_game', fallback='')
        if saved_game and saved_game in self.game_cards:
            self.selected_game_var.set(saved_game)
        else:
            self.selected_game_var.set('')

        self._update_button_text()
        self._update_select_all_button()

    def _toggle_select_all(self):
        """Toggle all app checkboxes between selected and deselected."""
        enabled_cards = [
            card for card in self.status_cards.values()
            if not card.is_not_found
        ]

        if not enabled_cards:
            return

        checked_count = sum(1 for card in enabled_cards if card.get_checked())
        select_all = checked_count < len(enabled_cards)

        for app_name, card in self.status_cards.items():
            if not card.is_not_found:
                card.set_checked(select_all)
                config_key = f"{app_name.lower().replace(' ', '_')}_enabled"
                self.config_manager.config.set('Settings', config_key, str(select_all))

        self.config_manager.save_config()
        self._update_button_text()
        self._update_select_all_button()

    def _update_select_all_button(self):
        """Update the select all button text based on current selection state."""
        if not self.select_all_btn:
            return

        enabled_cards = [
            card for card in self.status_cards.values()
            if not card.is_not_found
        ]

        if not enabled_cards:
            self.select_all_btn.configure(text="Select All")
            return

        checked_count = sum(1 for card in enabled_cards if card.get_checked())

        if checked_count == len(enabled_cards):
            self.select_all_btn.configure(text="Deselect All")
        else:
            self.select_all_btn.configure(text="Select All")

    def _update_button_text(self):
        """Update button text to show number of selected apps and selected game."""
        if not self.launch_btn or not self.close_btn:
            return

        checked_count = sum(
            1 for card in self.status_cards.values()
            if card.get_checked()
        )

        selected_game = self.selected_game_var.get()

        if selected_game:
            self.launch_btn.configure(text=f"Launch with {selected_game} ({checked_count})")
        else:
            self.launch_btn.configure(text=f"Launch apps ({checked_count})")

        self.close_btn.configure(text=f"Close all ({checked_count})")

        if checked_count == 0 and not selected_game:
            self.launch_btn.configure(state="disabled")
            self.close_btn.configure(state="disabled")
        else:
            self.launch_btn.configure(state="normal")
            self.close_btn.configure(state="normal")

    def _on_checkbox_change(self, app_name: str, checked: bool):
        """Handle checkbox state change and save to config."""
        config_key = f"{app_name.lower().replace(' ', '_')}_enabled"
        self.config_manager.config.set('Settings', config_key, str(checked))
        self.config_manager.save_config()
        self._update_button_text()
        self._update_select_all_button()

    def _on_game_selected(self, game_name: str):
        """Handle game radio button selection."""
        self.config_manager.config.set('Settings', 'selected_game', game_name)
        self.config_manager.save_config()
        self._update_button_text()

    def _browse_for_app(self, app_name: str):
        """Open file dialog to manually select app executable."""
        expected_exe = self.app_manager.get_app_exe(app_name)
        if not expected_exe:
            return

        file_path = filedialog.askopenfilename(
            title=f"Select {app_name} executable",
            filetypes=[("Executable files", "*.exe"), ("All files", "*.*")],
            initialdir="C:\\Program Files"
        )

        if not file_path:
            return

        selected_exe = os.path.basename(file_path)
        if selected_exe.lower() != expected_exe.lower():
            self.logger.error(
                f"Invalid file selected. Expected {expected_exe}, got {selected_exe}"
            )
            return

        if not os.path.exists(file_path):
            self.logger.error(f"Selected file does not exist: {file_path}")
            return

        config_key = ConfigManager.get_config_key(app_name)
        self.config_manager.set_app_path(config_key, file_path)

        self.status_cards[app_name].set_status("idle")
        self.status_cards[app_name].set_checked(True)

        checkbox_config_key = f"{app_name.lower().replace(' ', '_')}_enabled"
        self.config_manager.config.set('Settings', checkbox_config_key, str(True))
        self.config_manager.save_config()

        self.logger.success(f"{app_name} path configured: {file_path}")
        self._update_button_text()
        self._update_select_all_button()

    def _browse_for_game(self, game_name: str):
        """Open file dialog to manually select game executable or shortcut."""
        expected_exe = self.game_manager.get_game_exe(game_name)
        if not expected_exe:
            return

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
            return

        if file_path.lower().endswith('.lnk'):
            if not os.path.exists(file_path):
                self.logger.error(f"Selected file does not exist: {file_path}")
                return
        else:
            selected_exe = os.path.basename(file_path)
            if selected_exe.lower() != expected_exe.lower():
                self.logger.error(
                    f"Invalid file selected. Expected {expected_exe}, got {selected_exe}"
                )
                return

            if not os.path.exists(file_path):
                self.logger.error(f"Selected file does not exist: {file_path}")
                return

        config_key = f"game_{ConfigManager.get_config_key(game_name)}"
        self.config_manager.set_app_path(config_key, file_path)

        self.game_cards[game_name].set_status("idle")
        self.logger.success(f"{game_name} path configured: {file_path}")
        self._update_button_text()

    def launch_apps(self):
        """Launch all configured companion applications."""
        if not self.launch_btn:
            return

        self.launch_btn.configure(state="disabled")

        self.logger.log_divider()
        self.logger.launch("Starting launch sequence...")

        for app_name in self.app_manager.get_app_list():
            if not self.status_cards[app_name].get_checked():
                continue

            app_path = self.app_manager.find_app_path(app_name)

            if not app_path:
                self.logger.warning(f"Skipping {app_name} - not configured")
                continue

            if self.app_manager.is_app_running(app_name):
                self.logger.info(f"Skipping {app_name} - already running")
                self.status_cards[app_name].set_status("running")
                continue

            self.status_cards[app_name].set_status("starting")
            self.logger.info(f"Launching {app_name}...")

            success = self.app_manager.launch_app(app_name, app_path)
            if success:
                self.logger.success(f"{app_name} started successfully")
                self.status_cards[app_name].set_status("running")
            else:
                self.logger.error(f"{app_name} failed to start")
                self.logger.warning(f"Try reinstalling {app_name}")
                self.status_cards[app_name].set_status("failed")

        self.logger.success("All apps launched!")

        # Launch selected game after apps if one is selected
        selected_game = self.selected_game_var.get()
        if selected_game:
            self._launch_selected_game(selected_game)

        self.logger.success("Launch sequence complete!")
        self.launch_btn.configure(state="normal")

    def _launch_selected_game(self, game_name: str):
        """Launch the selected game."""
        game_path = self.game_manager.find_game_path(game_name)

        if not game_path:
            self.logger.warning(f"Skipping {game_name} - not configured")
            return

        if self.game_manager.is_game_running(game_name):
            self.logger.info(f"Skipping {game_name} - already running")
            self.game_cards[game_name].set_status("running")
            return

        self.game_cards[game_name].set_status("starting")
        self.logger.launch(f"Launching {game_name}...")

        success = self.game_manager.launch_game(game_name, game_path)
        if success:
            self.logger.success(f"{game_name} started successfully")
            self.game_cards[game_name].set_status("running")
        else:
            self.logger.error(f"{game_name} failed to start")
            self.logger.warning(f"Try reinstalling {game_name} or selecting a different path")
            self.game_cards[game_name].set_status("failed")

    def close_apps(self):
        """Close all companion applications."""
        if not self.close_btn:
            return

        self.close_btn.configure(state="disabled")

        self.logger.log_divider()
        self.logger.close("Closing applications...")

        for app_name in self.app_manager.get_app_list():
            if not self.status_cards[app_name].get_checked():
                continue

            app_path = self.app_manager.find_app_path(app_name)
            if not app_path:
                killed = self.app_manager.close_app(app_name)
                if not killed:
                    continue

            self.status_cards[app_name].set_status("stopping")
            self.logger.info(f"Closing {app_name}...")

            killed = self.app_manager.close_app(app_name)

            if killed:
                self.logger.success(f"{app_name} closed")
                self.status_cards[app_name].set_status("stopped")
            else:
                self.logger.warning(f"{app_name} was not running")
                if not app_path:
                    self.status_cards[app_name].set_status("not_found")
                else:
                    self.status_cards[app_name].set_status("idle")

        # Close Garage61 Agent (special case)
        if "Garage61" in self.status_cards and self.status_cards["Garage61"].get_checked():
            self.logger.info("Closing Garage61 Agent...")
            if self.app_manager.close_process_by_name("garage61-agent.exe"):
                self.logger.success("Garage61 Agent closed")
            else:
                self.logger.warning("Garage61 Agent was not running")

        # Check game status
        selected_game = self.selected_game_var.get()
        if selected_game:
            if self.game_manager.is_game_running(selected_game):
                self.logger.warning(
                    f"⚠ {selected_game} is still running - please close it manually"
                )
            else:
                self.game_cards[selected_game].set_status("idle")

        self.logger.success("All apps closed!")
        self.close_btn.configure(state="normal")
