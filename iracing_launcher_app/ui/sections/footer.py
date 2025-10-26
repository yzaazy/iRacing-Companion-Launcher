"""Footer section of the main window."""

import customtkinter as ctk


class FooterSection:
    """Creates and manages the footer section."""

    def __init__(self, parent: ctk.CTk):
        """
        Initialize the footer section.

        Args:
            parent: Parent window
        """
        self.parent = parent
        self.frame = None
        self._create_footer()

    def _create_footer(self):
        """Create the footer section with copyright."""
        self.frame = ctk.CTkFrame(self.parent, fg_color="transparent")
        self.frame.pack(side="bottom", fill="x", pady=(0, 5), padx=15)

        copyright_label = ctk.CTkLabel(
            self.frame,
            text="© 2025 Developed & Designed by Tobias Termeczky • Vibed with Claude",
            font=("Segoe UI", 10),
            text_color="#888888",
            anchor="e"
        )
        copyright_label.pack(side="right")
