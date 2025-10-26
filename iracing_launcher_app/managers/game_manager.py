"""
Racing game management.

Handles path detection, launching, and status checking for racing simulators.
"""

import os
import time
from typing import Dict, Optional, List

from ..core.app_definitions import RACE_GAMES
from ..core.config_manager import ConfigManager
from ..utils.path_finder import find_shortcut_target
from ..utils.steam_registry import check_steam_game_installed
from .process_manager import ProcessManager


class GameManager:
    """Manages racing game operations."""

    def __init__(self, config_manager: ConfigManager):
        """
        Initialize the game manager.

        Args:
            config_manager: ConfigManager instance for path persistence
        """
        self.config_manager = config_manager
        self.games = RACE_GAMES.copy()
        self.process_manager = ProcessManager()

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
            if check_steam_game_installed(steam_appid):
                steam_url = f"steam://rungameid/{steam_appid}"
                print(f"[Game Search] Will use Steam protocol: {steam_url}")
                # Save to config for next time
                self.config_manager.set_app_path(config_key, steam_url)
                return steam_url
        else:
            print(f"[Game Search] No Steam app ID configured for {game_name}")

        print(f"[Game Search] {game_name} not found - use Browse to manually select")
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
        return self.process_manager.is_process_running(exe_name)

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
                if not self.process_manager.launch_process(game_path):
                    return False

            # Check if process is running
            exe_name = self.games[game_name]["exe"]
            is_running = self.process_manager.is_process_running(exe_name)
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
        return self.process_manager.kill_process(exe_name)
