"""Captures process_state.json as the Python ProcessTracker (v2.3.2) writes it.

Run once, before the Python app was removed, from the repository root:

    python tests/fixtures/process/capture_state_files.py <dummy.exe> tests/fixtures/process

<dummy.exe> is tests/fixtures/process/dummy.rs compiled with
`rustc --edition 2021 -C debuginfo=0 -o <dummy.exe> tests/fixtures/process/dummy.rs`.
Every process this script starts is a copy of that dummy under an
`ircl-fixture-<random>-<role>.exe` name in a fresh temp directory; nothing else
is launched, matched or killed. The dummies exit through their `.stop` files.

The tracker launches them through a relative path from inside the temp
directory, so the files hold no machine-specific path, and the directory name
holds non-ASCII and an astral character so the escaping is pinned.

Writes, into the output directory:
  python_written.json  three entries after two launches: a falsy entry the
                       tracker keeps as is, a key and a path with non-ASCII and
                       an astral character, and the root of a dummy tree
  python_closed.json   after close_tracked of the tree root
  python_dropped.json  after the second dummy exited by itself and a new
                       tracker dropped it on load (Python's re-save on load)
  python_view.json     what the tracker reported for the root, and the PIDs and
                       creation times it stored, for the Rust tests to map
"""

import importlib.util
import json
import os
import secrets
import shutil
import sys
import tempfile
import time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
spec = importlib.util.spec_from_file_location(
    "process_tracker", os.path.join(REPO, "iracing_launcher_app", "managers", "process_tracker.py"))
pt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pt)
import psutil  # noqa: E402

dummy, out_dir = sys.argv[1], os.path.abspath(sys.argv[2])
prefix = f"ircl-fixture-{secrets.token_hex(4)}"
work = tempfile.mkdtemp(prefix="ircl fixture ")
sub = "dümmy 日本 \U0001F600"
os.makedirs(os.path.join(work, sub))
roles = ["root", "child", "grandchild", "gone"]
for role in roles:
    shutil.copy(dummy, os.path.join(work, sub, f"{prefix}-{role}.exe"))
os.chdir(work)


def rel(role):
    return f"{sub}\\{prefix}-{role}.exe"


def stop(role):
    open(os.path.join(work, rel(role) + ".stop"), "w").close()


def running(role):
    name = f"{prefix}-{role}.exe".lower()
    return any((p.info["name"] or "").lower() == name for p in psutil.process_iter(["name"]))


def wait_for(cond, timeout=15.0):
    end = time.time() + timeout
    while time.time() < end:
        if cond():
            return True
        time.sleep(0.05)
    return False


state = os.path.join(work, "process_state.json")
APP, GONE = "Dummy App", "Dümmy Gone \U0001F600"
try:
    with open(state, "w", encoding="utf-8") as f:
        f.write('{"Empty": {}}')
    t = pt.ProcessTracker(state)
    assert t.launch_and_track(GONE, rel("gone"), wait_time=1.0)
    assert t.launch_and_track(APP, rel("root"), wait_time=1.0)
    assert wait_for(lambda: t.is_tracked_running(APP) == (True, 2))
    shutil.copy(state, os.path.join(out_dir, "python_written.json"))
    with open(state, encoding="utf-8") as f:
        saved = json.load(f)
    view = {
        "prefix": prefix,
        "names": t.get_child_names(APP),
        "running": list(t.is_tracked_running(APP)),
        "entries": {k: [v["pid"], repr(v["create_time"])] for k, v in saved.items() if v},
    }

    assert t.close_tracked(APP)
    assert t.is_tracked(GONE) and t.is_tracked("Empty")
    shutil.copy(state, os.path.join(out_dir, "python_closed.json"))

    stop("gone")
    assert wait_for(lambda: not running("gone"))
    t = pt.ProcessTracker(state)
    assert not t.is_tracked(GONE) and t.is_tracked("Empty")
    shutil.copy(state, os.path.join(out_dir, "python_dropped.json"))
    with open(os.path.join(out_dir, "python_view.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(view, f, indent=2, ensure_ascii=False)
        f.write("\n")
    print(json.dumps(view, ensure_ascii=False))
finally:
    for role in roles:
        stop(role)
    os.chdir(REPO)
    wait_for(lambda: not any(running(r) for r in roles), timeout=10.0)
    shutil.rmtree(work, ignore_errors=True)
