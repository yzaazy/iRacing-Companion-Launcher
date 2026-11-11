"""Record golden transcripts from the real Python controller (`iRacingLauncherGUI`).

Drives `iracing_launcher_app/ui/main_window.py` through the scenarios below with
fake managers, an in-memory config and a scripted file dialog, and writes
`transcripts.json` beside this file. `tests/app_transcripts.rs` replays every
scenario against the Rust controller and asserts an identical transcript.

Run from anywhere:  python tests/fixtures/app/capture_transcripts.py

Safety: tripwires are installed before anything from the app is imported. No
process can be started or killed: Popen, os.startfile, os.kill and psutil's
kill/terminate/wait_procs raise, time.sleep does nothing, and the real
AppManager, GameManager, ProcessTracker and ConfigManager are replaced where
main_window looks them up, so they are never constructed.
"""

# --- Tripwires: first, before any app import -------------------------------
import os
import subprocess
import time

import psutil


def _tripwire(*_args, **_kwargs):
    raise RuntimeError("tripwire: the transcript script must not touch processes")


subprocess.Popen = _tripwire
os.startfile = _tripwire
os.kill = _tripwire
psutil.Process.kill = _tripwire
psutil.Process.terminate = _tripwire
psutil.wait_procs = _tripwire
time.sleep = lambda *_args, **_kwargs: None

# --- Now the app -----------------------------------------------------------
import configparser  # noqa: E402
import json  # noqa: E402
import sys  # noqa: E402

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
sys.path.insert(0, REPO)
os.chdir(REPO)  # Browse picks are relative to the repo root, as in the Rust tests.

import customtkinter as ctk  # noqa: E402
from tkinter import filedialog  # noqa: E402

from iracing_launcher_app.core.app_definitions import APPS, RACE_GAMES  # noqa: E402
from iracing_launcher_app.ui import main_window  # noqa: E402
from iracing_launcher_app.ui.widgets.game_card import GameCard  # noqa: E402
from iracing_launcher_app.ui.widgets.status_card import StatusCard  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "transcripts.json")
FILES = "tests/fixtures/app/files"

TRACE = []  # the transcript of the scenario being run
SCENARIO = {}  # the scenario being run
COUNTERS = {}  # (method, name) -> calls so far
RECORDING = [False]  # config seeding is not part of the transcript


def respond(method, name, default):
    """The scripted answer for this call: values in order, the last one sticky."""
    seq = SCENARIO["script"].get(method, {}).get(name)
    if seq is None:
        return default
    n = COUNTERS.get((method, name), 0)
    COUNTERS[(method, name)] = n + 1
    return seq[min(n, len(seq) - 1)]


def call(method, *args, default):
    result = respond(method, args[0], default)
    TRACE.append(["call", method, *args, result])
    return result


class FakeAppManager:
    def __init__(self, _config_manager, _process_tracker):
        pass

    def get_app_list(self):
        return list(APPS.keys())

    def get_app_exe(self, name):
        return APPS[name]["exe"] if name in APPS else None

    def find_app_path(self, name):
        return call("find_app_path", name, default=None)

    def is_app_running(self, name):
        return call("is_app_running", name, default=False)

    def get_child_names(self, name):
        return call("get_child_names", name, default=[])

    def launch_app(self, name, path):
        return call("launch_app", name, path, default=False)

    def close_app(self, name):
        return call("close_app", name, default=False)


class FakeGameManager:
    def __init__(self, _config_manager):
        pass

    def get_game_list(self):
        return list(RACE_GAMES.keys())

    def get_game_exe(self, name):
        return RACE_GAMES[name]["exe"] if name in RACE_GAMES else None

    def find_game_path(self, name):
        return call("find_game_path", name, default=None)

    def is_game_running(self, name):
        return call("is_game_running", name, default=False)

    def launch_game(self, name, path):
        return call("launch_game", name, path, default=False)


class FakeProcessTracker:
    def __init__(self, _state_file):
        pass


class RecordingParser(configparser.ConfigParser):
    """A real in-memory ConfigParser, so getboolean and the `%` rules are real."""

    def set(self, section, option, value=None):
        if RECORDING[0] and section == "Settings":
            TRACE.append(["config_set", option, value])
        super().set(section, option, value)


class FakeConfigManager:
    def __init__(self):
        self.config = RecordingParser()
        self.config["AppPaths"] = {}
        self.config["Settings"] = dict(SCENARIO["settings"])
        RECORDING[0] = True

    def get_config_dir(self):
        return "unused"

    def save_config(self):
        TRACE.append(["config_save"])
        return True

    def get_app_path(self, _key):
        raise AssertionError("the window never reads app paths itself")

    def set_app_path(self, key, path):
        # Recorded first: ConfigManager.set_app_path raises before saving when
        # configparser refuses the value. A recorded set_app_path implies its save.
        TRACE.append(["set_app_path", key, path])
        self.config["AppPaths"][key] = path
        return True

    @staticmethod
    def get_config_key(name):
        return name.lower().replace(" ", "_")


class FakeLogger:
    def set_widget(self, _widget):
        pass

    def log_divider(self):
        TRACE.append(["log", "divider", "─" * 50])

    def _log(level):
        def log(self, message):
            TRACE.append(["log", level, message])

        return log

    info = _log("info")
    success = _log("success")
    error = _log("error")
    warning = _log("warning")
    launch = _log("launch")
    close = _log("close")


def fake_askopenfilename(**kwargs):
    filetypes = [[label, pattern] for label, pattern in kwargs["filetypes"]]
    TRACE.append(["dialog", kwargs["title"], filetypes, kwargs["initialdir"]])
    picks = SCENARIO["picks"]
    return picks.pop(0)


LAST_APP_STATUS = {}
_original_card_status = StatusCard.set_status
_original_game_status = GameCard.set_status


def recording_card_status(self, status, child_count=None):
    TRACE.append(["app_status", self.app_name, status, child_count or 0])
    LAST_APP_STATUS[self.app_name] = (status, child_count or 0)
    _original_card_status(self, status, child_count=child_count)


def recording_game_status(self, status):
    TRACE.append(["game_status", self.game_name, status])
    _original_game_status(self, status)


main_window.AppManager = FakeAppManager
main_window.GameManager = FakeGameManager
main_window.ProcessTracker = FakeProcessTracker
main_window.ConfigManager = FakeConfigManager
main_window.ActivityLogger = FakeLogger
filedialog.askopenfilename = fake_askopenfilename
StatusCard.set_status = recording_card_status
GameCard.set_status = recording_game_status


# --- Scenarios -------------------------------------------------------------
APP_NAMES = list(APPS.keys())
GAME_NAMES = list(RACE_GAMES.keys())
FANATEC, CREW, PAINTS, SIMHUB, G61, DECK, MARVIN = APP_NAMES
IRACING, ACC, ACE, AMS2, RF2 = GAME_NAMES


def every(value):
    return {name: [value] for name in APP_NAMES}


def app_paths(missing=()):
    return {
        name: [None if name in missing else f"C:/fake/{APPS[name]['exe']}"]
        for name in APP_NAMES
    }


def game_paths(missing=()):
    return {
        name: [None if name in missing else f"steam://rungameid/{RACE_GAMES[name]['steam_appid']}"]
        for name in GAME_NAMES
    }


def scenario(name, steps=(), settings=None, picks=(), **script):
    merged = {"find_app_path": app_paths(), "find_game_path": game_paths()}
    merged.update(script)
    return {
        "name": name,
        "settings": settings or {},
        "script": merged,
        "picks": list(picks),
        "steps": list(steps),
    }


LAUNCH = {"do": "launch"}
CLOSE = {"do": "close"}
SELECT_ALL = {"do": "select_all"}


def toggle(app):
    return {"do": "toggle_app", "name": app}


def select(game):
    return {"do": "select_game", "name": game}


def browse_app(app):
    return {"do": "browse_app", "name": app}


def browse_game(game):
    return {"do": "browse_game", "name": game}


SCENARIOS = [
    # Startup
    scenario(
        "startup_mixed_no_settings",
        find_app_path=app_paths(missing=(PAINTS, G61, MARVIN)),
        is_app_running={CREW: [True], SIMHUB: [True]},
        get_child_names={CREW: [["crewchief-helper.exe", "tts.exe"]], SIMHUB: [[]]},
        find_game_path=game_paths(missing=(ACE, RF2)),
        is_game_running={IRACING: [True]},
    ),
    scenario(
        "startup_saved_settings",
        settings={
            "fanatec_enabled": "False",
            "simhub_enabled": "no",
            "trading_paints_enabled": "True",
            "crew_chief_v4_enabled": "on",
            "selected_game": ACE,
        },
        find_app_path=app_paths(missing=(PAINTS,)),
        find_game_path=game_paths(missing=(ACE,)),
    ),
    scenario(
        "startup_saved_game_found",
        settings={"selected_game": AMS2},
    ),
    scenario(
        "startup_unknown_game_everything_off",
        settings={
            "selected_game": "Unknown Sim",
            **{f"{FakeConfigManager.get_config_key(a)}_enabled": "False" for a in APP_NAMES},
        },
        steps=[LAUNCH, CLOSE],
    ),
    scenario(
        "startup_nothing_found",
        find_app_path=app_paths(missing=APP_NAMES),
        find_game_path=game_paths(missing=GAME_NAMES),
        steps=[SELECT_ALL, LAUNCH],
    ),
    # Launch
    scenario(
        "launch_all_succeed",
        launch_app=every(True),
        get_child_names={G61: [[], ["garage61-agent.exe"]], DECK: [[], ["a.exe", "b.exe", "c.exe"]]},
        steps=[LAUNCH],
    ),
    scenario(
        "launch_one_fails",
        launch_app={**every(True), SIMHUB: [False]},
        steps=[LAUNCH],
    ),
    scenario(
        "launch_already_running",
        is_app_running={FANATEC: [False, True], CREW: [False, True], PAINTS: [False]},
        get_child_names={FANATEC: [["fanatec-svc.exe"]], CREW: [[]]},
        launch_app=every(True),
        steps=[LAUNCH],
    ),
    scenario(
        "launch_path_gone",
        find_app_path={**app_paths(), PAINTS: ["C:/fake/Trading Paints.exe", None]},
        launch_app=every(True),
        steps=[LAUNCH],
    ),
    scenario(
        "launch_nothing_selected_with_game",
        launch_game={IRACING: [True]},
        steps=[SELECT_ALL, select(IRACING), LAUNCH],
    ),
    scenario(
        "launch_nothing_selected_then_none",
        steps=[SELECT_ALL, select(IRACING), LAUNCH, select(None), LAUNCH, CLOSE],
        launch_game={IRACING: [True]},
    ),
    scenario(
        "launch_game_steam_succeeds",
        settings={"selected_game": IRACING},
        launch_app=every(True),
        launch_game={IRACING: [True]},
        steps=[LAUNCH],
    ),
    scenario(
        "launch_game_fails",
        settings={"selected_game": ACC},
        launch_app=every(True),
        launch_game={ACC: [False]},
        steps=[LAUNCH],
    ),
    scenario(
        "launch_game_already_running",
        settings={"selected_game": IRACING},
        is_game_running={IRACING: [False, True]},
        launch_app=every(True),
        steps=[LAUNCH],
    ),
    scenario(
        "launch_game_not_configured",
        settings={"selected_game": RF2},
        find_game_path=game_paths(missing=(RF2,)),
        launch_app=every(True),
        steps=[LAUNCH],
    ),
    scenario(
        "launch_twice_then_close",
        launch_app=every(True),
        is_app_running={FANATEC: [False, False, True]},
        close_app=every(True),
        steps=[LAUNCH, LAUNCH, CLOSE, CLOSE],
    ),
    # Close
    scenario(
        "close_tracked_with_helpers",
        is_app_running=every(True),
        get_child_names={
            CREW: [["tts.exe"]],
            G61: [["garage61-agent.exe"], ["garage61-agent.exe", "x.exe"], []],
            DECK: [["a.exe", "b.exe", "c.exe"]],
        },
        close_app=every(True),
        steps=[CLOSE],
    ),
    scenario(
        "close_not_running",
        close_app={**every(True), FANATEC: [False], SIMHUB: [False]},
        steps=[CLOSE],
    ),
    scenario(
        "close_not_found_running_double_close",
        find_app_path={
            **app_paths(),
            SIMHUB: ["C:/fake/SimHubWPF.exe", None],
            MARVIN: ["C:/fake/MarvinsAIRARefactored.exe", None],
        },
        close_app={**every(True), SIMHUB: [True, False], MARVIN: [False]},
        steps=[CLOSE, LAUNCH],
        launch_app=every(True),
    ),
    scenario(
        "close_game_still_running",
        settings={"selected_game": IRACING},
        is_game_running={IRACING: [True]},
        close_app=every(True),
        steps=[CLOSE],
    ),
    scenario(
        "close_game_closed_manually",
        settings={"selected_game": IRACING},
        is_game_running={IRACING: [True, False]},
        close_app=every(True),
        steps=[CLOSE],
    ),
    scenario(
        "close_game_after_launch",
        settings={"selected_game": ACC},
        launch_app=every(True),
        launch_game={ACC: [True]},
        is_game_running={ACC: [False, False, True]},
        close_app=every(True),
        steps=[LAUNCH, CLOSE],
    ),
    scenario(
        "close_game_not_running_card",
        settings={"selected_game": ACC},
        close_app=every(True),
        steps=[CLOSE],
    ),
    # Selection
    scenario(
        "checkbox_toggles",
        find_app_path=app_paths(missing=(MARVIN,)),
        steps=[toggle(FANATEC), toggle(SIMHUB), toggle(FANATEC), toggle(MARVIN)],
    ),
    scenario(
        "toggle_all_off_disables_buttons",
        steps=[toggle(a) for a in APP_NAMES] + [LAUNCH, toggle(DECK)],
    ),
    scenario(
        "select_all_directions",
        find_app_path=app_paths(missing=(PAINTS, G61)),
        steps=[SELECT_ALL, SELECT_ALL, toggle(CREW), SELECT_ALL, toggle(CREW), SELECT_ALL],
    ),
    scenario(
        "game_selection",
        find_game_path=game_paths(missing=(RF2,)),
        steps=[select(ACC), select(IRACING), select(RF2), select(None), SELECT_ALL, select(AMS2), select(None)],
    ),
    # Browse
    scenario(
        "browse_app",
        find_app_path=app_paths(missing=(FANATEC, SIMHUB, CREW, G61)),
        picks=[
            f"{FILES}/Fanatec.exe",
            f"{FILES}/SimHubWPF.exe",
            "",
            f"{FILES}/missing/SimHubWPF.exe",
            f"{FILES}/100%/CrewChiefV4.exe",
            f"{FILES}/FANATEC.EXE",
            f"{FILES}/100%%/CrewChiefV4.exe",
        ],
        steps=[
            browse_app(FANATEC),
            browse_app(SIMHUB),
            browse_app(SIMHUB),
            browse_app(SIMHUB),
            browse_app(CREW),
            browse_app(G61),
            browse_app(CREW),
        ],
    ),
    scenario(
        "browse_game",
        find_game_path=game_paths(missing=(IRACING, ACC, ACE, AMS2)),
        settings={"selected_game": ACE},
        picks=[
            f"{FILES}/iRacingUI.exe",
            f"{FILES}/Some Game.lnk",
            f"{FILES}/missing.lnk",
            f"{FILES}/iRacingUI.exe",
            "",
            f"{FILES}/100%/Shortcut.LNK",
            f"{FILES}/missing/AMS2AVX.exe",
            f"{FILES}/Some Game.lnk",
        ],
        steps=[
            browse_game(IRACING),
            browse_game(ACC),
            browse_game(ACE),
            browse_game(ACE),
            browse_game(ACE),
            browse_game(ACE),
            browse_game(AMS2),
            browse_game(ACE),
            LAUNCH,
        ],
        launch_app=every(True),
    ),
]


# --- Runner ----------------------------------------------------------------
def run_step(gui, step):
    do = step["do"]
    if do in ("launch", "close"):
        button = gui.launch_btn if do == "launch" else gui.close_btn
        if button.cget("state") == "disabled":
            TRACE.append(["ignored", do])
        elif do == "launch":
            gui.launch_apps()
        else:
            gui.close_apps()
    elif do == "toggle_app":
        # A click flips the box and runs its command; a disabled box ignores it.
        checkbox = gui.status_cards[step["name"]].checkbox
        if checkbox.cget("state") == "disabled":
            TRACE.append(["ignored", do])
        else:
            checkbox.toggle()
    elif do == "select_all":
        gui._toggle_select_all()
    elif do == "select_game":
        # The UI fires only for an enabled radio that is not selected yet.
        name = step["name"] or ""
        if gui.selected_game_var.get() == name:
            TRACE.append(["ignored", do])
        elif name and gui.game_cards[name].is_not_found:
            TRACE.append(["ignored", do])
        else:
            gui.selected_game_var.set(name)
            gui._on_game_selected(name)
    elif do == "browse_app":
        gui._browse_for_app(step["name"])
    elif do == "browse_game":
        gui._browse_for_game(step["name"])
    else:
        raise ValueError(do)


def final_state(gui):
    apps = []
    for name in APP_NAMES:
        status, count = LAST_APP_STATUS.get(name, ("idle", 0))
        apps.append(
            {
                "name": name,
                "status": status,
                "checked": gui.status_cards[name].get_checked(),
                "helper_count": count if status == "running" else 0,
            }
        )
    games = [{"name": n, "status": gui.game_cards[n].get_status()} for n in GAME_NAMES]
    return {
        "apps": apps,
        "games": games,
        "selected_game": gui.selected_game_var.get() or None,
        "select_all_label": gui.select_all_btn.cget("text"),
        "launch_label": gui.launch_btn.cget("text"),
        "close_label": gui.close_btn.cget("text"),
        "launch_enabled": gui.launch_btn.cget("state") == "normal",
        "close_enabled": gui.close_btn.cget("state") == "normal",
    }


def run(spec):
    global SCENARIO
    SCENARIO = json.loads(json.dumps(spec))  # picks are consumed; keep spec intact
    TRACE.clear()
    COUNTERS.clear()
    LAST_APP_STATUS.clear()
    RECORDING[0] = False

    root = ctk.CTk()
    root.withdraw()
    root.geometry("+-10000+-10000")
    try:
        gui = main_window.iRacingLauncherGUI(root)
        root.withdraw()
        for step in spec["steps"]:
            TRACE.append(["step", step["do"], step.get("name")])
            try:
                run_step(gui, step)
            except ValueError as e:
                # configparser refusing a value raises out of the Tk callback;
                # Tk prints it and carries on. Nothing after the raise happens.
                print(f"  {spec['name']}: ValueError {e}")
        assert not SCENARIO["picks"], f"{spec['name']}: unused dialog picks"
        return {**spec, "transcript": list(TRACE), "final": final_state(gui)}
    finally:
        root.destroy()


def main():
    ctk.set_appearance_mode("dark")
    results = []
    for spec in SCENARIOS:
        print(spec["name"])
        results.append(run(spec))
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        json.dump(results, f, ensure_ascii=False, indent=1)
        f.write("\n")
    print(f"wrote {len(results)} scenarios to {OUT}")


if __name__ == "__main__":
    main()
