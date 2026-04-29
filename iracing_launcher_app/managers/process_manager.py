"""
Process management utilities.

Provides shared functionality for checking, launching, and killing processes.
"""

import subprocess
import time
import psutil
from typing import List


class ProcessManager:
    """Manages process operations (checking, launching, killing)."""

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

    @staticmethod
    def launch_process(exe_path: str, wait_time: float = 2.0) -> bool:
        """
        Launch a process and verify it started.

        Args:
            exe_path: Full path to the executable
            wait_time: Time to wait before checking if process started (seconds)

        Returns:
            True if launched successfully, False otherwise
        """
        try:
            # Launch the app completely detached from this process
            # DETACHED_PROCESS: Prevents inheriting console
            # CREATE_NEW_PROCESS_GROUP: Creates independent process group
            # This ensures the launched app doesn't hold handles to our temp directory
            subprocess.Popen(
                [exe_path],
                shell=False,
                close_fds=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP
            )
            time.sleep(wait_time)
            return True
        except Exception as e:
            print(f"Error launching {exe_path}: {e}")
            return False

    @staticmethod
    def kill_process(process_name: str) -> bool:
        """
        Kill a process by its executable name.

        Args:
            process_name: Name of the process executable

        Returns:
            True if process was killed, False if not running
        """
        killed = False
        for proc in psutil.process_iter(['name']):
            try:
                if proc.info['name'].lower() == process_name.lower():
                    proc.kill()
                    killed = True
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass
        return killed

    @staticmethod
    def kill_process_by_prefix(prefix: str, suffix: str = ".exe") -> bool:
        """
        Kill any process whose executable name starts with the given prefix
        and ends with the given suffix. Used for processes that include a
        version or build hash in their filename (e.g. Garage61's agent
        ``garage61-agent-<timestamp>-<hash>.exe``).

        Args:
            prefix: Leading portion of the executable name (case-insensitive)
            suffix: Trailing portion of the executable name (default ``.exe``)

        Returns:
            True if at least one matching process was killed, False otherwise
        """
        killed = False
        prefix_lower = prefix.lower()
        suffix_lower = suffix.lower()
        for proc in psutil.process_iter(['name']):
            try:
                name = (proc.info['name'] or "").lower()
                if name.startswith(prefix_lower) and name.endswith(suffix_lower):
                    proc.kill()
                    killed = True
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass
        return killed

    @staticmethod
    def kill_all_processes(process_names: List[str]) -> bool:
        """
        Kill all processes matching the given names.

        Args:
            process_names: List of process executable names

        Returns:
            True if any processes were killed, False otherwise
        """
        any_killed = False
        for process_name in process_names:
            if ProcessManager.kill_process(process_name):
                any_killed = True
        return any_killed
