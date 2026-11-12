"""Captures psutil's create_time next to the raw creation FILETIME it comes from.

Run once, from the repository root:

    python tests/fixtures/process/capture_create_times.py tests/fixtures/process/create_times.json

For this Python process and five child Python processes it starts itself
(`python -c "import time; time.sleep(60)"`, started at uneven intervals so the
sub-second parts differ, and killed through their own handles), records:

  filetime     the creation FILETIME from GetProcessTimes, as one 64-bit integer
               (100 ns ticks since 1601-01-01)
  create_time  repr(psutil.Process(pid).create_time())

No other process is opened. The PIDs are not recorded.
"""

import ctypes
import ctypes.wintypes as wt
import json
import random
import subprocess
import sys
import time

import psutil

PROCESS_QUERY_LIMITED_INFORMATION = 0x1000

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.OpenProcess.restype = wt.HANDLE
kernel32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
kernel32.GetProcessTimes.argtypes = [wt.HANDLE] + [ctypes.POINTER(wt.FILETIME)] * 4
kernel32.CloseHandle.argtypes = [wt.HANDLE]


def creation_filetime(pid):
    h = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not h:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        times = [wt.FILETIME() for _ in range(4)]
        if not kernel32.GetProcessTimes(h, *(ctypes.byref(t) for t in times)):
            raise ctypes.WinError(ctypes.get_last_error())
        created = times[0]
        return (created.dwHighDateTime << 32) | created.dwLowDateTime
    finally:
        kernel32.CloseHandle(h)


def record(pid):
    return {
        "filetime": creation_filetime(pid),
        "create_time": repr(psutil.Process(pid).create_time()),
    }


def main(out_path):
    rows = [record(psutil.Process().pid)]
    children = []
    try:
        for _ in range(5):
            time.sleep(random.uniform(0.05, 0.4))
            children.append(subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"]))
        rows += [record(child.pid) for child in children]
    finally:
        for child in children:
            child.kill()
            child.wait()
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(rows, f, indent=2)
        f.write("\n")


if __name__ == "__main__":
    main(sys.argv[1])
