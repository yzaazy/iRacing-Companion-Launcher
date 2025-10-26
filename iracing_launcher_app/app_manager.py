"""
Application path detection and process management.
"""

import os
import subprocess
import time
import winreg
from pathlib import Path
from typing import Dict, Optional, List

import psutil
import winshell

from .config_manager import ConfigManager
from .constants import APPS, RACE_GAMES


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
        self.games = self._initialize_games()

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

    def _initialize_games(self) -> Dict:
        """
        Initialize game definitions.

        Returns:
            Dictionary of game definitions
        """
        # Simply return the games from constants
        # Detection will be done via Steam registry only
        return RACE_GAMES.copy()

    def _check_steam_game_installed(self, appid: str) -> bool:
        """
        Check if a Steam game is installed via Windows Registry.

        Args:
            appid: Steam app ID

        Returns:
            True if game is installed, False otherwise
        """
        try:
            key_path = rf"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App {appid}"
            key = winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, key_path, 0, winreg.KEY_READ)
            winreg.CloseKey(key)
            print(f"[Registry] Steam App {appid} found in registry")
            return True
        except WindowsError:
            print(f"[Registry] Steam App {appid} not found in registry")
            return False

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

    # ==================== Game Management Methods ====================

    def get_game_list(self) -> List[str]:
        """
        Get list of all managed game names.

        Returns:
            List of game names
        """
        return list(self.games.keys())

    def get_game_exe(self, game_name: str) -> Optional[str]:
        """
        Get the executable name for a game.

        Args:
            game_name: Name of the game

        Returns:
            Executable name if found, None otherwise
        """
        if game_name in self.games:
            return self.games[game_name]["exe"]
        return None

    def find_game_path(self, game_name: str) -> Optional[str]:
        """
        Find the installation path for a game.

        Priority order:
        1. Saved path in config.ini (for manually browsed games)
        2. Registry check for Steam games (returns steam://rungameid/{appid})

        Args:
            game_name: Name of the game

        Returns:
            steam:// protocol URL if found via registry, saved path from config, or None
        """
        if game_name not in self.games:
            print(f"[Game Search] {game_name} not in games list")
            return None

        game_info = self.games[game_name]
        config_key = f"game_{ConfigManager.get_config_key(game_name)}"

        print(f"[Game Search] Searching for {game_name}...")

        # First check config.ini for saved path (manual browse)
        saved_path = self.config_manager.get_app_path(config_key)
        if saved_path:
            print(f"[Game Search] Found in config: {saved_path}")
            return saved_path

        # Check Steam registry
        steam_appid = game_info.get("steam_appid")
        if steam_appid:
            print(f"[Game Search] Checking registry for Steam App {steam_appid}...")
            if self._check_steam_game_installed(steam_appid):
                steam_url = f"steam://rungameid/{steam_appid}"
                print(f"[Game Search] Will use Steam protocol: {steam_url}")
                # Save to config for next time
                self.config_manager.set_app_path(config_key, steam_url)
                return steam_url
        else:
            print(f"[Game Search] No Steam app ID configured for {game_name}")

        print(f"[Game Search] {game_name} not found - use Browse to manually select")
        return None

    def _find_shortcut_target(self, app_name: str, is_game: bool = False) -> Optional[str]:
        """
        Find application/game path by reading Start Menu shortcuts.

        Args:
            app_name: Name of the application or game
            is_game: True if searching for game, False for app

        Returns:
            Path to .lnk file if found, None otherwise
        """
        source_dict = self.games if is_game else self.apps
        if app_name not in source_dict:
            return None

        item_info = source_dict[app_name]

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
            for shortcut_name in item_info.get("shortcut_names", []):
                for shortcut_path in start_menu.rglob(shortcut_name):
                    try:
                        # For games, return the .lnk path itself to preserve launch parameters
                        if is_game:
                            if os.path.exists(str(shortcut_path)):
                                return str(shortcut_path)
                        else:
                            # For apps, return the target .exe path
                            shortcut = winshell.shortcut(str(shortcut_path))
                            target_path = shortcut.path
                            if os.path.exists(target_path):
                                return target_path
                    except Exception:
                        continue

        return None

    def is_game_running(self, game_name: str) -> bool:
        """
        Check if a game is currently running.

        Args:
            game_name: Name of the game

        Returns:
            True if game is running, False otherwise
        """
        if game_name not in self.games:
            return False

        exe_name = self.games[game_name]["exe"]
        return self.is_process_running(exe_name)

    def launch_game(self, game_name: str, game_path: str) -> bool:
        """
        Launch a game via Steam protocol, .lnk shortcut, or .exe file.

        Args:
            game_name: Name of the game
            game_path: Full path to .lnk/.exe or steam:// protocol URL

        Returns:
            True if launched successfully, False otherwise
        """
        if game_name not in self.games:
            return False

        try:
            # Check if it's a Steam protocol URL
            if game_path.startswith("steam://"):
                print(f"[Launch] Launching via Steam protocol: {game_path}")
                os.startfile(game_path)
                time.sleep(3)  # Give Steam more time to start the game
            # Check if it's a .lnk file
            elif game_path.lower().endswith('.lnk'):
                print(f"[Launch] Launching via shortcut: {game_path}")
                os.startfile(game_path)
                time.sleep(2)
            else:
                # Launch .exe directly
                print(f"[Launch] Launching executable: {game_path}")
                subprocess.Popen(
                    [game_path],
                    shell=False,
                    close_fds=True,
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP
                )
                time.sleep(2)

            # Check if process is running
            exe_name = self.games[game_name]["exe"]
            is_running = self.is_process_running(exe_name)
            print(f"[Launch] Process check for {exe_name}: {'Running' if is_running else 'Not running'}")
            return is_running
        except Exception as e:
            print(f"[Launch] Error launching {game_name}: {e}")
            import traceback
            traceback.print_exc()
            return False

    def close_game(self, game_name: str) -> bool:
        """
        Close a game by killing its process.

        Args:
            game_name: Name of the game

        Returns:
            True if process was killed, False if not running
        """
        if game_name not in self.games:
            return False

        exe_name = self.games[game_name]["exe"]
        killed = False

        for proc in psutil.process_iter(['name']):
            try:
                if proc.info['name'].lower() == exe_name.lower():
                    proc.kill()
                    killed = True
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass

        return killed
