"""Action buttons section of the main window."""

import customtkinter as ctk
from typing import Callable


class ButtonsSection:
    """Creates and manages the action buttons section."""

    def __init__(
        self,
        parent: ctk.CTk,
        launch_callback: Callable,
        close_callback: Callable
    ):
        """
        Initialize the buttons section.

        Args:
            parent: Parent window
            launch_callback: Callback for Launch button
            close_callback: Callback for Close button
        """
        self.parent = parent
        self.launch_callback = launch_callback
        self.close_callback = close_callback
        self.frame = None
        self.launch_btn = None
        self.close_btn = None
        self._create_buttons()

    def _create_buttons(self):
        """Create the button section."""
        self.frame = ctk.CTkFrame(self.parent, fg_color="transparent")
        self.frame.pack(pady=(35, 10))

        self.launch_btn = ctk.CTkButton(
            self.frame,
            text="Launch All",
            command=self.launch_callback,
            fg_color="#0e639c",
            hover_color="#1177bb",
            width=200,
            height=60,
            font=("Segoe UI", 16, "bold"),
            corner_radius=12
        )
        self.launch_btn.pack(side="left", padx=10)

        self.close_btn = ctk.CTkButton(
            self.frame,
            text="Close All",
            command=self.close_callback,
            fg_color="#c72e2e",
            hover_color="#e04343",
            width=200,
            height=60,
            font=("Segoe UI", 16, "bold"),
            corner_radius=12
        )
        self.close_btn.pack(side="left", padx=10)

    def get_launch_button(self) -> ctk.CTkButton:
        """
        Get the Launch button.

        Returns:
            The Launch button widget
        """
        return self.launch_btn

    def get_close_button(self) -> ctk.CTkButton:
        """
        Get the Close button.

        Returns:
            The Close button widget
        """
        return self.close_btn
