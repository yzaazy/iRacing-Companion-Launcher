"""
Tracks processes launched by the launcher so the entire process tree can
be closed later, even when the launched app spawns helpers with
unstable filenames (e.g. Garage61's
``garage61-agent-<timestamp>-<hash>.exe``).

State is persisted to a small JSON file so a launcher restart does not
orphan running companion apps. Each entry stores the launched ``pid``
together with its ``create_time``; the create-time defends against PID
reuse after a restart.
"""

import json
import os
import subprocess
import time
from typing import Dict, Optional, Tuple

import psutil


# Tolerance (seconds) when comparing persisted vs. live ``create_time``.
# Wall-clock based, so a small NTP correction can shift it slightly.
_CREATE_TIME_EPSILON = 0.5


class ProcessTracker:
    """Owns launch + track + close + persistence for companion apps."""

    def __init__(self, state_file_path: str):
        self.state_file_path = state_file_path
        self._state: Dict[str, Dict] = {}
        self._load()
        self._revalidate()

    def launch_and_track(
        self,
        key: str,
        exe_path: str,
        wait_time: float = 2.0,
    ) -> bool:
        """Launch ``exe_path`` and remember its PID under ``key``.

        Returns True if the launched process is alive after ``wait_time``.
        """
        try:
            proc = subprocess.Popen(
                [exe_path],
                shell=False,
                close_fds=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=(
                    subprocess.DETACHED_PROCESS
                    | subprocess.CREATE_NEW_PROCESS_GROUP
                ),
            )
        except Exception as e:
            print(f"Error launching {exe_path}: {e}")
            return False

        pid = proc.pid
        try:
            create_time = psutil.Process(pid).create_time()
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            return False

        self._state[key] = {
            "pid": pid,
            "create_time": create_time,
            "exe_path": exe_path,
        }
        self._save()

        time.sleep(wait_time)
        return self._proc_for(key) is not None

    def is_tracked(self, key: str) -> bool:
        """Whether we currently hold tracking state for ``key``."""
        return key in self._state

    def is_tracked_running(self, key: str) -> Tuple[bool, int]:
        """Return ``(alive, descendant_count)`` for a tracked entry.

        Drops the entry if the process is gone or PID has been recycled.
        """
        proc = self._proc_for(key)
        if proc is None:
            return False, 0
        try:
            descendants = proc.children(recursive=True)
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            self._drop(key)
            return False, 0
        # Note: descendants that re-parent to wininit/PID 4 (e.g. services)
        # are intentionally invisible here; killing system-managed
        # processes risks Windows instability.
        return True, len(descendants)

    def get_child_count(self, key: str) -> int:
        """Return current descendant count for a tracked entry, or 0."""
        _, count = self.is_tracked_running(key)
        return count

    def get_child_names(self, key: str):
        """Return descendant exe names for a tracked entry, or []."""
        proc = self._proc_for(key)
        if proc is None:
            return []
        try:
            return [c.name() for c in proc.children(recursive=True)]
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            return []

    def close_tracked(self, key: str) -> bool:
        """Terminate the tracked process and all of its descendants.

        Returns True if at least one process was killed.
        """
        proc = self._proc_for(key)
        if proc is None:
            return False

        try:
            descendants = proc.children(recursive=True)
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            self._drop(key)
            return False

        targets = [proc] + descendants
        for p in targets:
            try:
                p.terminate()
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass

        _, alive = psutil.wait_procs(targets, timeout=3)
        for p in alive:
            try:
                p.kill()
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass

        self._drop(key)
        return True

    def forget(self, key: str) -> None:
        """Forget a tracked entry without killing the process."""
        self._drop(key)

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _proc_for(self, key: str) -> Optional[psutil.Process]:
        """Return a live ``psutil.Process`` for ``key`` or None.

        Drops the entry on any mismatch so callers don't act on stale state.
        """
        entry = self._state.get(key)
        if not entry:
            return None
        pid = entry["pid"]
        saved_ct = entry["create_time"]
        try:
            proc = psutil.Process(pid)
            if not proc.is_running() or proc.status() == psutil.STATUS_ZOMBIE:
                self._drop(key)
                return None
            if abs(proc.create_time() - saved_ct) > _CREATE_TIME_EPSILON:
                # PID was reused by an unrelated process.
                self._drop(key)
                return None
            return proc
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            self._drop(key)
            return None

    def _drop(self, key: str) -> None:
        if key in self._state:
            del self._state[key]
            self._save()

    def _revalidate(self) -> None:
        """Drop entries whose process is gone or whose PID was recycled."""
        for key in list(self._state.keys()):
            self._proc_for(key)  # has the side effect of dropping stale entries

    def _load(self) -> None:
        if not os.path.exists(self.state_file_path):
            return
        try:
            with open(self.state_file_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            if isinstance(data, dict):
                self._state = data
        except (OSError, ValueError) as e:
            print(f"Warning: could not read process state: {e}")
            self._state = {}

    def _save(self) -> None:
        tmp_path = self.state_file_path + ".tmp"
        try:
            with open(tmp_path, "w", encoding="utf-8") as f:
                json.dump(self._state, f, indent=2)
            os.replace(tmp_path, self.state_file_path)
        except OSError as e:
            print(f"Warning: could not write process state: {e}")
