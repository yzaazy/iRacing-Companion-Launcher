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


class AppManager:
    """Manages companion application operations."""

    def __init__(self, config_manager: ConfigManager):
        """
        Initialize the app manager.

        Args:
            config_manager: ConfigManager instance for path persistence
        """
        self.config_manager = config_manager
        self.apps = self._initialize_apps()
        self.process_manager = ProcessManager()

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

        # Add dynamic path for Bloops
        if "Bloops" in apps:
            localappdata_path = os.path.join(
                os.getenv('LOCALAPPDATA'),
                r"Bloops\current\Bloops.exe"
            )
            apps["Bloops"]["paths"] = [localappdata_path]

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
        """
        Check if an application is currently running.

        Args:
            app_name: Name of the application

        Returns:
            True if app is running, False otherwise
        """
        if app_name not in self.apps:
            return False

        exe_name = self.apps[app_name]["exe"]
        return self.process_manager.is_process_running(exe_name)

    def launch_app(self, app_name: str, app_path: str) -> bool:
        """
        Launch an application and verify it started.

        Args:
            app_name: Name of the application
            app_path: Full path to the executable

        Returns:
            True if launched successfully, False otherwise
        """
        if app_name not in self.apps:
            return False

        if not self.process_manager.launch_process(app_path):
            return False

        exe_name = self.apps[app_name]["exe"]
        return self.process_manager.is_process_running(exe_name)

    def close_app(self, app_name: str) -> bool:
        """
        Close an application by killing its process.

        Args:
            app_name: Name of the application

        Returns:
            True if process was killed, False if not running
        """
        if app_name not in self.apps:
            return False

        exe_name = self.apps[app_name]["exe"]
        return self.process_manager.kill_process(exe_name)

    def close_process_by_name(self, exe_name: str) -> bool:
        """
        Close a process by its executable name (for special cases like Garage61 Agent).

        Args:
            exe_name: Name of the executable

        Returns:
            True if process was killed, False if not running
        """
        return self.process_manager.kill_process(exe_name)
