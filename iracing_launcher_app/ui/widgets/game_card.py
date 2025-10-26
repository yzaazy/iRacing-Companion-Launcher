"""
Game card widget for racing simulators.
"""

import customtkinter as ctk
from ..constants import STATUS_COLORS, STATUS_CARD_HEIGHT


class GameCard(ctk.CTkFrame):
    """Widget to display race game with radio button selection."""

    def __init__(self, parent, game_name, browse_callback=None, radio_callback=None, radio_variable=None):
        """
        Initialize a game card.

        Args:
            parent: Parent widget
            game_name: Name of the game
            browse_callback: Optional callback for Browse button
            radio_callback: Optional callback when radio button is selected
            radio_variable: Shared StringVar for radio button group
        """
        super().__init__(
            parent,
            fg_color="#2d2d30",
            border_width=1,
            border_color="#0e639c",  # Blue border to distinguish from apps
            height=STATUS_CARD_HEIGHT,
            corner_radius=6
        )
        self.pack_propagate(False)
        self.game_name = game_name
        self.browse_callback = browse_callback
        self.radio_callback = radio_callback
        self.is_not_found = False
        self.current_status = "idle"  # Track current status

        # Radio button for selecting this game
        self.radio_btn = ctk.CTkRadioButton(
            self,
            text="",
            width=20,
            value=game_name,
            variable=radio_variable,
            command=self._on_radio_change
        )
        self.radio_btn.pack(side="left", padx=(10, 5), pady=10)

        # Game name label
        self.name_label = ctk.CTkLabel(
            self,
            text=self._wrap_text(game_name),
            text_color="#ffffff",
            font=("Segoe UI", 14, "bold"),
            anchor="w",
            justify="left"
        )
        self.name_label.pack(side="left", padx=(5, 15), pady=10)

        # Status indicator (will be replaced with Browse button if not found)
        self.status_label = ctk.CTkLabel(
            self,
            text="●",
            text_color=STATUS_COLORS["idle"],
            font=("Segoe UI", 40)
        )
        self.status_label.pack(side="right", padx=15, pady=(5, 15))

        # Browse button (hidden by default)
        self.browse_btn = ctk.CTkButton(
            self,
            text="Browse",
            fg_color="#555555",
            hover_color="#666666",
            text_color="#ffffff",
            font=("Segoe UI", 13),
            width=80,
            height=36,
            command=self._on_browse_click,
            corner_radius=6
        )

    def _wrap_text(self, text: str, max_length: int = 18) -> str:
        """
        Wrap text to multiple lines if longer than max_length.
        Breaks on spaces if possible.

        Args:
            text: Text to wrap
            max_length: Maximum characters per line

        Returns:
            Wrapped text with newlines
        """
        if len(text) <= max_length:
            return text

        # Find the last space before max_length
        break_point = text.rfind(' ', 0, max_length)

        if break_point == -1:
            # No space found, just break at max_length
            return text[:max_length] + '\n' + text[max_length:]
        else:
            # Break at the space
            return text[:break_point] + '\n' + text[break_point + 1:]

    def _on_browse_click(self):
        """Handle browse button click."""
        if self.browse_callback:
            self.browse_callback(self.game_name)

    def _on_radio_change(self):
        """Handle radio button selection."""
        if self.radio_callback:
            self.radio_callback(self.game_name)

    def get_status(self):
        """
        Get the current status of the game.

        Returns:
            Current status string
        """
        return self.current_status

    def set_status(self, status):
        """
        Update the status indicator color or show Browse button.

        Args:
            status: Status string ("idle", "starting", "running", "failed",
                   "stopped", "not_found")
        """
        self.current_status = status  # Store the status

        if status == "not_found":
            # Hide status indicator, show Browse button
            self.is_not_found = True
            self.status_label.pack_forget()
            self.browse_btn.pack(side="right", padx=15, pady=10)
            self.name_label.configure(text_color="#888888")
            # Disable radio button for not found games
            self.radio_btn.configure(state="disabled")
        else:
            # Show status indicator, hide Browse button
            if self.is_not_found:
                self.browse_btn.pack_forget()
                self.status_label.pack(side="right", padx=15, pady=(5, 15))
                self.name_label.configure(text_color="#ffffff")
                # Re-enable radio button when game is found
                self.radio_btn.configure(state="normal")
                self.is_not_found = False

            color = STATUS_COLORS.get(status, STATUS_COLORS["idle"])
            self.status_label.configure(text_color=color)
