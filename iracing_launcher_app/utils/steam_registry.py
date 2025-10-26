"""
Steam registry utilities for detecting installed games.
"""

import winreg


def check_steam_game_installed(appid: str) -> bool:
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
