"""
Activity logging functionality for the application.

This module provides a logging abstraction that manages
the activity log text widget and formatting.
"""

import time
import customtkinter as ctk
from typing import Optional

from ..ui.constants import LOG_COLORS


class ActivityLogger:
    """Manages activity logging to a CTkTextbox widget."""

    def __init__(self, log_widget: Optional[ctk.CTkTextbox] = None):
        """
        Initialize the activity logger.

        Args:
            log_widget: Optional CTkTextbox widget for displaying logs
        """
        self.log_widget = log_widget
        self._configure_tags()

    def _configure_tags(self):
        """Configure text tags for colored log messages."""
        if self.log_widget:
            for level, color in LOG_COLORS.items():
                self.log_widget.tag_config(level, foreground=color)

    def set_widget(self, log_widget: ctk.CTkTextbox):
        """
        Set or update the log widget.

        Args:
            log_widget: CTkTextbox widget for displaying logs
        """
        self.log_widget = log_widget
        self._configure_tags()

    def log_message(self, message: str, level: str = "info"):
        """
        Add a message to the activity log with timestamp and color.

        Args:
            message: Message to log
            level: Log level ("info", "success", "error", "warning", "launch", "close")
        """
        if not self.log_widget:
            return

        self.log_widget.configure(state="normal")

        timestamp = time.strftime("%H:%M:%S")
        # Insert the message with colored text using tags
        self.log_widget.insert("end", f"[{timestamp}] {message}\n", level)

        self.log_widget.see("end")
        self.log_widget.configure(state="disabled")

        # Force update to show message immediately
        if hasattr(self.log_widget, 'master'):
            self.log_widget.master.update()

    def log_divider(self):
        """Add a visual divider line to the activity log."""
        if not self.log_widget:
            return

        self.log_widget.configure(state="normal")
        self.log_widget.insert("end", "─" * 50 + "\n", "divider")
        self.log_widget.see("end")
        self.log_widget.configure(state="disabled")

    def clear_log(self):
        """Clear all text from the activity log."""
        if not self.log_widget:
            return

        self.log_widget.configure(state="normal")
        self.log_widget.delete("1.0", "end")
        self.log_widget.configure(state="disabled")

    def info(self, message: str):
        """Log an info message."""
        self.log_message(message, "info")

    def success(self, message: str):
        """Log a success message."""
        self.log_message(message, "success")

    def error(self, message: str):
        """Log an error message."""
        self.log_message(message, "error")

    def warning(self, message: str):
        """Log a warning message."""
        self.log_message(message, "warning")

    def launch(self, message: str):
        """Log a launch message."""
        self.log_message(message, "launch")

    def close(self, message: str):
        """Log a close message."""
        self.log_message(message, "close")
