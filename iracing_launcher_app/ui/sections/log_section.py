"""Activity log section of the main window."""

import customtkinter as ctk
from ..constants import BG_TERTIARY, FG_SECONDARY, FG_TERTIARY, STATUS_CARD_HEIGHT


class LogSection:
    """Creates and manages the activity log section."""

    def __init__(self, parent: ctk.CTkFrame, num_apps: int):
        """
        Initialize the log section.

        Args:
            parent: Parent frame
            num_apps: Number of apps (for calculating height)
        """
        self.parent = parent
        self.num_apps = num_apps
        self.container = None
        self.log_text = None
        self._create_log_section()

    def _create_log_section(self):
        """Create the activity log section."""
        self.container = ctk.CTkFrame(self.parent, fg_color="transparent")
        self.container.pack(side="right", fill="both", expand=True)

        log_label = ctk.CTkLabel(
            self.container,
            text="Activity Log",
            font=("Segoe UI", 16, "bold"),
            text_color=FG_SECONDARY,
            anchor="w"
        )
        log_label.pack(anchor="w", pady=(0, 10))

        # Calculate height based on number of status cards
        # First card has no top padding, last card has no bottom padding
        # So we have (num_apps - 1) gaps of 10px between cards
        card_padding = 10 * (self.num_apps - 1)
        # Match the exact height of the cards section
        total_height = (STATUS_CARD_HEIGHT * self.num_apps) + card_padding

        self.log_text = ctk.CTkTextbox(
            self.container,
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

    def get_widget(self) -> ctk.CTkTextbox:
        """
        Get the log text widget.

        Returns:
            The CTkTextbox widget for logging
        """
        return self.log_text
