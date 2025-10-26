"""Racing games section of the main window."""

import customtkinter as ctk
from typing import Dict, Callable
from ..constants import FG_SECONDARY, STATUS_CARD_HEIGHT
from ..widgets.game_card import GameCard


class GamesSection:
    """Creates and manages the racing games section."""

    def __init__(
        self,
        parent: ctk.CTkFrame,
        game_list: list,
        browse_callback: Callable,
        radio_callback: Callable,
        radio_variable: ctk.StringVar
    ):
        """
        Initialize the games section.

        Args:
            parent: Parent frame
            game_list: List of game names
            browse_callback: Callback for Browse button
            radio_callback: Callback for radio button selection
            radio_variable: Shared StringVar for radio buttons
        """
        self.parent = parent
        self.game_list = game_list
        self.browse_callback = browse_callback
        self.radio_callback = radio_callback
        self.radio_variable = radio_variable
        self.container = None
        self.game_cards: Dict[str, GameCard] = {}
        self._create_games_section()

    def _create_games_section(self):
        """Create the race games section."""
        self.container = ctk.CTkFrame(self.parent, fg_color="transparent")
        self.container.pack(side="left", fill="both", expand=True, padx=(0, 10))

        # Header frame with label
        header_frame = ctk.CTkFrame(self.container, fg_color="transparent", height=30)
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
        cards_frame = ctk.CTkFrame(self.container, fg_color="transparent")
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
            variable=self.radio_variable,
            command=lambda: self.radio_callback("")
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
        for idx, game_name in enumerate(self.game_list):
            card = GameCard(
                cards_frame,
                game_name,
                browse_callback=self.browse_callback,
                radio_callback=self.radio_callback,
                radio_variable=self.radio_variable
            )
            # Last card: no bottom padding
            if idx == len(self.game_list) - 1:
                pady = (5, 0)
            else:
                pady = 5

            card.pack(fill="x", pady=pady)
            self.game_cards[game_name] = card

    def get_cards(self) -> Dict[str, GameCard]:
        """
        Get all game cards.

        Returns:
            Dictionary mapping game names to GameCard widgets
        """
        return self.game_cards
