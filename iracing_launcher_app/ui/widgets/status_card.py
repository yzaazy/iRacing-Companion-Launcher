"""
Status card widget for companion applications.
"""

import customtkinter as ctk
from ..constants import STATUS_COLORS, STATUS_CARD_HEIGHT


class StatusCard(ctk.CTkFrame):
    """Widget to display app status with colored indicator or Browse button."""

    def __init__(self, parent, app_name, browse_callback=None, checkbox_callback=None):
        """
        Initialize a status card.

        Args:
            parent: Parent widget
            app_name: Name of the application
            browse_callback: Optional callback for Browse button
            checkbox_callback: Optional callback when checkbox state changes
        """
        super().__init__(
            parent,
            fg_color="#2d2d30",
            border_width=1,
            border_color="#3e3e42",
            height=STATUS_CARD_HEIGHT,
            corner_radius=6
        )
        self.pack_propagate(False)  # Prevent frame from resizing to contents
        self.app_name = app_name
        self.browse_callback = browse_callback
        self.checkbox_callback = checkbox_callback
        self.is_not_found = False

        # Checkbox for enabling/disabling this app
        self.checkbox = ctk.CTkCheckBox(
            self,
            text="",
            width=20,
            command=self._on_checkbox_change
        )
        self.checkbox.select()  # Default to checked
        self.checkbox.pack(side="left", padx=(10, 5), pady=10)

        # Wrapper holds the app name and an optional subtitle below it.
        self.text_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.text_frame.pack(side="left", padx=(5, 15), pady=4, fill="y")

        self.name_label = ctk.CTkLabel(
            self.text_frame,
            text=app_name,
            text_color="#ffffff",
            font=("Segoe UI", 14, "bold"),
            anchor="w"
        )
        self.name_label.pack(side="top", anchor="w", pady=0)

        # Subtitle (always present so the name label sits in a stable
        # position; text toggles between placeholder and helper count).
        self.subtitle_label = ctk.CTkLabel(
            self.text_frame,
            text="No helpers",
            text_color="#888888",
            font=("Segoe UI", 9),
            anchor="w",
            height=12
        )
        self.subtitle_label.pack(side="top", anchor="w")

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

    def _on_browse_click(self):
        """Handle browse button click."""
        if self.browse_callback:
            self.browse_callback(self.app_name)

    def _on_checkbox_change(self):
        """Handle checkbox state change."""
        if self.checkbox_callback:
            self.checkbox_callback(self.app_name, self.get_checked())

    def get_checked(self):
        """
        Get the checkbox state.

        Returns:
            bool: True if checked, False otherwise
        """
        return self.checkbox.get() == 1

    def set_checked(self, checked):
        """
        Set the checkbox state.

        Args:
            checked: True to check, False to uncheck
        """
        if checked:
            self.checkbox.select()
        else:
            self.checkbox.deselect()

    def set_status(self, status, child_count=None):
        """
        Update the status indicator color or show Browse button.

        Args:
            status: Status string ("idle", "starting", "running", "failed",
                   "stopped", "not_found")
            child_count: Number of helper processes spawned by the app, if
                known. Surfaces as a subtitle when status is "running".
        """
        if status == "not_found":
            # Hide status indicator, show Browse button
            self.is_not_found = True
            self.status_label.pack_forget()
            self.browse_btn.pack(side="right", padx=15, pady=10)
            self.name_label.configure(text_color="#888888")  # Dim the app name
            # Uncheck and disable checkbox for not found apps
            self.checkbox.deselect()
            self.checkbox.configure(state="disabled")
            self._set_subtitle("Not installed")
        else:
            # Show status indicator, hide Browse button
            if self.is_not_found:
                self.browse_btn.pack_forget()
                self.status_label.pack(side="right", padx=15, pady=(5, 15))
                self.name_label.configure(text_color="#ffffff")  # Restore app name color
                # Re-enable checkbox when app is found
                self.checkbox.configure(state="normal")
                self.is_not_found = False

            color = STATUS_COLORS.get(status, STATUS_COLORS["idle"])
            self.status_label.configure(text_color=color)

            if status == "running" and child_count:
                noun = "process" if child_count == 1 else "processes"
                self._set_subtitle(f"{child_count} helper {noun}")
            else:
                self._set_subtitle("No helpers")

    def _set_subtitle(self, text):
        """Update the subtitle line under the app name. The label slot is
        always reserved so the app name doesn't visually shift when
        helper info appears or disappears."""
        self.subtitle_label.configure(text=text)
