"""
Configuration file management for storing application paths.
"""

import os
import sys
import configparser
from typing import Optional


class ConfigManager:
    """Manages reading and writing application paths to config.ini."""

    CONFIG_FILENAME = "config.ini"
    SECTION_NAME = "AppPaths"
    SETTINGS_SECTION = "Settings"

    def __init__(self):
        """Initialize the config manager and load existing config."""
        self.config = configparser.ConfigParser()
        self.config_path = self._get_config_path()
        self._load_config()

    def _get_config_path(self) -> str:
        """
        Get the path to the config.ini file.

        Returns:
            Full path to config.ini
        """
        if getattr(sys, 'frozen', False):
            # Running as compiled executable - use AppData folder
            # This avoids permission issues when installed to Program Files
            appdata = os.getenv('APPDATA')
            if appdata:
                app_dir = os.path.join(appdata, 'iRacingCompanionLauncher')
                # Create directory if it doesn't exist
                os.makedirs(app_dir, exist_ok=True)
            else:
                # Fallback to executable directory (shouldn't happen on Windows)
                app_dir = os.path.dirname(sys.executable)
        else:
            # Running as script - use script directory
            app_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        return os.path.join(app_dir, self.CONFIG_FILENAME)

    def _load_config(self):
        """Load configuration from config.ini."""
        if os.path.exists(self.config_path):
            try:
                self.config.read(self.config_path)
            except Exception as e:
                print(f"Warning: Error loading config: {e}")

        # Ensure required sections exist
        if self.SECTION_NAME not in self.config:
            self.config[self.SECTION_NAME] = {}
        if self.SETTINGS_SECTION not in self.config:
            self.config[self.SETTINGS_SECTION] = {}

    def save_config(self) -> bool:
        """
        Save configuration to config.ini.

        Returns:
            True if successful, False otherwise
        """
        try:
            with open(self.config_path, 'w') as configfile:
                self.config.write(configfile)
            return True
        except Exception as e:
            print(f"Error saving config: {e}")
            return False

    def get_app_path(self, app_key: str) -> Optional[str]:
        """
        Get the saved path for an application.

        Args:
            app_key: Config key for the application

        Returns:
            Path string if found and exists, None otherwise
        """
        if app_key in self.config[self.SECTION_NAME]:
            saved_path = self.config[self.SECTION_NAME][app_key]
            if os.path.exists(saved_path):
                return saved_path
        return None

    def set_app_path(self, app_key: str, path: str) -> bool:
        """
        Save an application path to config.

        Args:
            app_key: Config key for the application
            path: Full path to the executable

        Returns:
            True if saved successfully, False otherwise
        """
        self.config[self.SECTION_NAME][app_key] = path
        return self.save_config()

    @staticmethod
    def get_config_key(app_name: str) -> str:
        """
        Convert app name to config key format (lowercase with underscores).

        Args:
            app_name: Display name of the application

        Returns:
            Config key string
        """
        return app_name.lower().replace(" ", "_")
