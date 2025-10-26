"""Header section of the main window."""

import customtkinter as ctk
from version import __version__
from ..constants import BG_SECONDARY, FG_PRIMARY


class HeaderSection:
    """Creates and manages the header section."""

    def __init__(self, parent: ctk.CTk):
        """
        Initialize the header section.

        Args:
            parent: Parent window
        """
        self.parent = parent
        self.frame = None
        self._create_header()

    def _create_header(self):
        """Create the header section."""
        self.frame = ctk.CTkFrame(self.parent, height=70, fg_color=BG_SECONDARY)
        self.frame.pack(fill="x", pady=(0, 10))
        self.frame.pack_propagate(False)

        title_label = ctk.CTkLabel(
            self.frame,
            text=f"iRacing Companion Launcher v{__version__}",
            font=("Segoe UI", 24, "bold"),
            text_color=FG_PRIMARY
        )
        title_label.pack(pady=20)
