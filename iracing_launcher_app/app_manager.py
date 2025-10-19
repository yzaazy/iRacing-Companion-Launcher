"""
Application path detection and process management.
"""

import os
import subprocess
import time
from pathlib import Path
from typing import Dict, Optional, List

import psutil
import winshell

from .config_manager import ConfigManager
from .constants import APPS


class AppManager:
    """Manages application path detection and process operations."""

    def __init__(self, config_manager: ConfigManager):
        """
        Initialize the app manager.

        Args:
            config_manager: ConfigManager instance for path persistence
        """
        self.config_manager = config_manager
        self.apps = self._initialize_apps()

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
        return apps

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
        shortcut_path = self._find_shortcut_target(app_name)
        if shortcut_path:
            # Save to config for next time
            self.config_manager.set_app_path(config_key, shortcut_path)
            return shortcut_path

        # Fall back to hardcoded paths
        for path in app_info.get("paths", []):
            if os.path.exists(path):
                # Save to config for next time
                self.config_manager.set_app_path(config_key, path)
                return path

        return None

    def _find_shortcut_target(self, app_name: str) -> Optional[str]:
        """
        Find application path by reading Start Menu shortcuts.

        Args:
            app_name: Name of the application

        Returns:
            Path to executable if found, None otherwise
        """
        if app_name not in self.apps:
            return None

        app_info = self.apps[app_name]

        # Start Menu locations to search
        start_menu_paths = [
            Path(os.getenv('APPDATA')) / 'Microsoft' / 'Windows' / 'Start Menu' / 'Programs',
            Path(os.getenv('ProgramData')) / 'Microsoft' / 'Windows' / 'Start Menu' / 'Programs'
        ]

        # Search for shortcuts
        for start_menu in start_menu_paths:
            if not start_menu.exists():
                continue

            # Search recursively for matching shortcut names
            for shortcut_name in app_info.get("shortcut_names", []):
                for shortcut_path in start_menu.rglob(shortcut_name):
                    try:
                        shortcut = winshell.shortcut(str(shortcut_path))
                        target_path = shortcut.path
                        if os.path.exists(target_path):
                            return target_path
                    except Exception:
                        continue

        return None

    @staticmethod
    def is_process_running(process_name: str) -> bool:
        """
        Check if a process is currently running.

        Args:
            process_name: Name of the process executable

        Returns:
            True if process is running, False otherwise
        """
        for proc in psutil.process_iter(['name']):
            try:
                if proc.info['name'].lower() == process_name.lower():
                    return True
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass
        return False

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
        return self.is_process_running(exe_name)

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

        try:
            # Launch the app completely detached from this process
            # DETACHED_PROCESS: Prevents inheriting console
            # CREATE_NEW_PROCESS_GROUP: Creates independent process group
            # This ensures the launched app doesn't hold handles to our temp directory
            subprocess.Popen(
                [app_path],
                shell=False,
                close_fds=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP
            )
            time.sleep(2)  # Wait for process to start

            exe_name = self.apps[app_name]["exe"]
            return self.is_process_running(exe_name)
        except Exception as e:
            print(f"Error launching {app_name}: {e}")
            return False

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
        killed = False

        for proc in psutil.process_iter(['name']):
            try:
                if proc.info['name'].lower() == exe_name.lower():
                    proc.kill()
                    killed = True
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass

        return killed

    def close_process_by_name(self, exe_name: str) -> bool:
        """
        Close a process by its executable name (for special cases like Garage61 Agent).

        Args:
            exe_name: Name of the executable

        Returns:
            True if process was killed, False if not running
        """
        killed = False
        for proc in psutil.process_iter(['name']):
            try:
                if proc.info['name'].lower() == exe_name.lower():
                    proc.kill()
                    killed = True
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass
        return killed

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
