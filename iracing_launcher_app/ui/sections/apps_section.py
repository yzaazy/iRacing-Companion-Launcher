"""Companion apps section of the main window."""

import customtkinter as ctk
from typing import Dict, Callable
from ..constants import FG_SECONDARY
from ..widgets.status_card import StatusCard


class AppsSection:
    """Creates and manages the companion apps section."""

    def __init__(
        self,
        parent: ctk.CTkFrame,
        app_list: list,
        browse_callback: Callable,
        checkbox_callback: Callable,
        toggle_select_all_callback: Callable
    ):
        """
        Initialize the apps section.

        Args:
            parent: Parent frame
            app_list: List of app names
            browse_callback: Callback for Browse button
            checkbox_callback: Callback for checkbox state changes
            toggle_select_all_callback: Callback for Select All button
        """
        self.parent = parent
        self.app_list = app_list
        self.browse_callback = browse_callback
        self.checkbox_callback = checkbox_callback
        self.toggle_select_all_callback = toggle_select_all_callback
        self.container = None
        self.status_cards: Dict[str, StatusCard] = {}
        self.select_all_btn = None
        self._create_apps_section()

    def _create_apps_section(self):
        """Create the companion apps section."""
        self.container = ctk.CTkFrame(self.parent, fg_color="transparent")
        self.container.pack(side="left", fill="both", expand=True, padx=(0, 10))

        # Header frame with label and select all button
        header_frame = ctk.CTkFrame(self.container, fg_color="transparent", height=30)
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
            command=self.toggle_select_all_callback,
            fg_color="#555555",
            hover_color="#666666",
            width=100,
            height=30,
            font=("Segoe UI", 12),
            corner_radius=6
        )
        self.select_all_btn.pack(side="right")

        cards_frame = ctk.CTkFrame(self.container, fg_color="transparent")
        cards_frame.pack(fill="both", expand=True)

        for idx, app_name in enumerate(self.app_list):
            card = StatusCard(
                cards_frame,
                app_name,
                browse_callback=self.browse_callback,
                checkbox_callback=self.checkbox_callback
            )
            # First card: no top padding; Last card: no bottom padding
            if idx == 0:
                pady = (0, 5)
            elif idx == len(self.app_list) - 1:
                pady = (5, 0)
            else:
                pady = 5

            card.pack(fill="x", pady=pady)
            self.status_cards[app_name] = card

    def get_cards(self) -> Dict[str, StatusCard]:
        """
        Get all status cards.

        Returns:
            Dictionary mapping app names to StatusCard widgets
        """
        return self.status_cards

    def get_select_all_button(self) -> ctk.CTkButton:
        """
        Get the Select All button.

        Returns:
            The Select All button widget
        """
        return self.select_all_btn
