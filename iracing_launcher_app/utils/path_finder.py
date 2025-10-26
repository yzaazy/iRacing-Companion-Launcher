"""
Path detection utilities for finding application executables.
"""

import os
from pathlib import Path
from typing import Optional, List
import winshell


def find_shortcut_target(
    shortcut_names: List[str],
    is_game: bool = False
) -> Optional[str]:
    """
    Find application/game path by reading Start Menu shortcuts.

    Args:
        shortcut_names: List of potential shortcut names to search for
        is_game: True if searching for game, False for app

    Returns:
        Path to .lnk file (for games) or target .exe (for apps) if found, None otherwise
    """
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
        for shortcut_name in shortcut_names:
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


def find_path_in_list(paths: List[str]) -> Optional[str]:
    """
    Find the first existing path from a list of potential paths.

    Args:
        paths: List of paths to check

    Returns:
        First existing path, or None if none exist
    """
    for path in paths:
        if os.path.exists(path):
            return path
    return None
