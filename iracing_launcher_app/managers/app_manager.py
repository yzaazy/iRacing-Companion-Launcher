"""
Companion application management.

Handles path detection, launching, and closing of companion applications.
"""

import os
from typing import Dict, Optional, List

from ..core.app_definitions import APPS
from ..core.config_manager import ConfigManager
from ..utils.path_finder import find_shortcut_target, find_path_in_list
from .process_manager import ProcessManager
from .process_tracker import ProcessTracker


class AppManager:
    """Manages companion application operations."""

    def __init__(
        self,
        config_manager: ConfigManager,
        process_tracker: ProcessTracker,
    ):
        """
        Initialize the app manager.

        Args:
            config_manager: ConfigManager instance for path persistence
            process_tracker: Tracker for PID-based launch/close
        """
        self.config_manager = config_manager
        self.apps = self._initialize_apps()
        self.process_manager = ProcessManager()
        self.process_tracker = process_tracker

    def _initialize_apps(self) -> Dict:
        """
        Initialize app definitions with dynamic paths.

        Returns:
            Dictionary of app definitions
        """
        apps = APPS.copy()

        # Add dynamic path for Garage61
        if "Garage61" in apps:
            appdata_path = os.path.join(
                os.getenv('APPDATA'),
                r"garage61-install\garage61-launcher.exe"
            )
            apps["Garage61"]["paths"] = [appdata_path]

        # Elgato Stream Deck uses static paths from app_definitions.py
        # No dynamic path needed

        # Add dynamic path for TrackTitan
        if "TrackTitan" in apps:
            localappdata_path = os.path.join(
                os.getenv('LOCALAPPDATA'),
                r"Programs\track-titan-ghost-application\TrackTitanDesktopApplication.exe"
            )
            apps["TrackTitan"]["paths"] = [localappdata_path]

        return apps

    def get_app_list(self) -> List[str]:
        """
        Get list of all managed application names.

        Returns:
            List of application names
        """
        return list(self.apps.keys())

    def get_app_exe(self, app_name: str) -> Optional[str]:
        """
        Get the executable name for an application.

        Args:
            app_name: Name of the application

        Returns:
            Executable name if found, None otherwise
        """
        if app_name in self.apps:
            return self.apps[app_name]["exe"]
        return None

    def find_app_path(self, app_name: str) -> Optional[str]:
        """
        Find the installation path for an application.

        Checks in order:
        1. Saved path in config.ini
        2. Start Menu shortcuts
        3. Hardcoded common paths

        Args:
            app_name: Name of the application

        Returns:
            Path to executable if found, None otherwise
        """
        if app_name not in self.apps:
            return None

        app_info = self.apps[app_name]
        config_key = ConfigManager.get_config_key(app_name)

        # First check config.ini for saved path
        saved_path = self.config_manager.get_app_path(config_key)
        if saved_path:
            return saved_path

        # Try to find via Start Menu shortcut
        shortcut_path = find_shortcut_target(
            app_info.get("shortcut_names", []),
            is_game=False
        )
        if shortcut_path:
            # Save to config for next time
            self.config_manager.set_app_path(config_key, shortcut_path)
            return shortcut_path

        # Fall back to hardcoded paths
        found_path = find_path_in_list(app_info.get("paths", []))
        if found_path:
            # Save to config for next time
            self.config_manager.set_app_path(config_key, found_path)
            return found_path

        return None

    def is_app_running(self, app_name: str) -> bool:
        """Whether the app is currently running.

        Prefers tracked PID state; falls back to exe-name match so apps
        the user started outside the launcher still register.
        """
        if app_name not in self.apps:
            return False

        if self.process_tracker.is_tracked(app_name):
            alive, _ = self.process_tracker.is_tracked_running(app_name)
            if alive:
                return True
            # Fall through: tracked entry was dropped, but the user may
            # have a separate instance running.

        exe_name = self.apps[app_name]["exe"]
        return self.process_manager.is_process_running(exe_name)

    def get_child_count(self, app_name: str) -> int:
        """Descendant-process count for a tracked app, else 0."""
        if app_name not in self.apps:
            return 0
        return self.process_tracker.get_child_count(app_name)

    def get_child_names(self, app_name: str):
        """Descendant exe names for a tracked app, else []."""
        if app_name not in self.apps:
            return []
        return self.process_tracker.get_child_names(app_name)

    def launch_app(self, app_name: str, app_path: str) -> bool:
        """Launch an app, track its PID, and report whether it's alive."""
        if app_name not in self.apps:
            return False
        return self.process_tracker.launch_and_track(app_name, app_path)

    def close_app(self, app_name: str) -> bool:
        """Close a tracked app and its process tree.

        If we don't have a tracked PID (e.g. user launched it manually),
        fall back to killing by exe name so the close button still works.
        """
        if app_name not in self.apps:
            return False

        if self.process_tracker.is_tracked(app_name):
            if self.process_tracker.close_tracked(app_name):
                return True

        exe_name = self.apps[app_name]["exe"]
        return self.process_manager.kill_process(exe_name)
