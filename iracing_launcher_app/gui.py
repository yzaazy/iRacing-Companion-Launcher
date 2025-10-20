"""
Main GUI application class.
"""

import os
import sys
import time
import customtkinter as ctk
from tkinter import filedialog
from typing import Dict, Optional

from PIL import Image, ImageTk

from version import __version__
from .app_manager import AppManager
from .config_manager import ConfigManager
from .widgets import StatusCard
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
        self.root.geometry("800x630")
        self.root.resizable(False, False)

        # Initialize managers
        self.config_manager = ConfigManager()
        self.app_manager = AppManager(self.config_manager)

        # UI components
        self.status_cards: Dict[str, StatusCard] = {}
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
                # Set icon using multiple methods for maximum compatibility
                try:
                    # Method 1: iconbitmap (works for title bar and taskbar)
                    self.root.iconbitmap(ico_path)
                except Exception as e:
                    print(f"iconbitmap failed: {e}")

                try:
                    # Method 2: Also set via iconphoto with PNG for some contexts
                    png_path = ico_path.replace('.ico', '.png')
                    if os.path.exists(png_path):
                        # Open, load, and immediately close the image file
                        with Image.open(png_path) as icon_image:
                            # Load the image data into memory
                            icon_image.load()
                            # Create PhotoImage from the loaded data
                            icon_photo = ImageTk.PhotoImage(icon_image)
                        # Store reference to prevent garbage collection
                        self.root._icon_photo = icon_photo
                        self.root.iconphoto(True, icon_photo)
                except Exception as e:
                    print(f"iconphoto failed: {e}")
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
        """Create the main content area with apps and log."""
        content_frame = ctk.CTkFrame(self.root, fg_color="transparent")
        content_frame.pack(pady=5, padx=15, fill="both")

        # Left side - Companion Apps
        self._create_apps_section(content_frame)

        # Right side - Activity Log
        self._create_log_section(content_frame)

    def _create_apps_section(self, parent: ctk.CTkFrame):
        """
        Create the companion apps section.

        Args:
            parent: Parent frame widget
        """
        apps_container = ctk.CTkFrame(parent, fg_color="transparent")
        apps_container.pack(side="left", fill="both", expand=True, padx=(0, 15))

        # Header frame with label and select all button
        header_frame = ctk.CTkFrame(apps_container, fg_color="transparent")
        header_frame.pack(fill="x", pady=(0, 10))

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
        """Update button text to show number of selected apps."""
        if not self.launch_btn or not self.close_btn:
            return

        # Count checked apps
        checked_count = sum(
            1 for card in self.status_cards.values()
            if card.get_checked()
        )

        # Update button text
        self.launch_btn.configure(text=f"Launch All ({checked_count})")
        self.close_btn.configure(text=f"Close All ({checked_count})")

        # Disable buttons when no apps are selected
        if checked_count == 0:
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

        self._log_message("Launch sequence complete!", "success")
        self.launch_btn.configure(state="normal")

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

        self._log_message("All apps closed!", "success")
        self.close_btn.configure(state="normal")
