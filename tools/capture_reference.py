"""
Capture the reference screenshots and measurements of the Python launcher UI.

    python tools/capture_reference.py                 # every state in fixtures.json
    python tools/capture_reference.py --state mixed   # one state (ui-spec.json keeps the others)
    python tools/capture_reference.py --verify-screen # also cross-check PrintWindow against a screen grab

For each state in docs/rewrite/fixtures.json it builds the window the way
iracing_launcher.py and iRacingLauncherGUI._create_widgets do (same CTk theme,
geometry, icon, section classes and pack order), drives the widgets into the
fixture state through their own methods, captures the client area with
PrintWindow(PW_CLIENTONLY | PW_RENDERFULLCONTENT), measures it, and closes the
window. Only UI modules are imported: no ConfigManager, ProcessTracker,
AppManager, GameManager or iRacingLauncherGUI, so nothing is detected, launched,
killed or written outside docs/rewrite/.

Outputs
    docs/rewrite/reference/<state>.png   client area, 1200x700, RGB
    docs/rewrite/ui-spec.json            everything below

ui-spec.json structure (all coordinates are window client pixels, origin top
left, x right, y down; a rect is {"x", "y", "w", "h"}; colours are "#rrggbb")
    _readme       this summary, in short
    environment   versions, DPI, scaling factors, capture method
    window        client size, scaling, title
    fonts         one entry per Tk font in use: family, px size, weight, the
                  family Tk actually resolved, ascent, descent, linespace
    colors        per colour role: the value the source or CTk's blue.json says,
                  the value sampled from the screenshots, where, and a match flag
    shapes        ink measurements from pixels: corner radii (area fit), border
                  thickness, checkbox / radio / checkmark / status dot / scrollbar
                  thumb geometry, and small raw pixel grids of the tiny shapes
    layout        how Tk resolved the pack layout for idle: requested vs actual
                  sizes of the root's and the content frame's slaves (the 713 px
                  overflow, the column widths); per state in states.<state>.pack
    notes         findings the numbers alone do not explain, one key per topic
    states.<state>
      elements    name -> {"rect", "req", "visible"?, "painted"?, "ink"?, ...}.
                  rect is the Tk widget rectangle, req its requested size;
                  visible is rect clipped by its ancestors, present only when
                  it differs; painted (CTk-drawn shapes) and ink (glyphs,
                  checkbox/radio canvases, scrollbar thumb) are bounding boxes
                  of what is actually painted, measured from pixels.
      text        name -> {"string", "font", "rect", "ink", "baseline_y",
                  "color_expected", "color_measured", ...} for every label
      log         Text widget options, yview, and every visible display line
                  with its rect, baseline, level, text and ink box
      pack        the same pack results as 'layout', for this state
      text_regions  rects (ink box grown by 2 px) that tools/ui_diff.py treats
                  as text; status dots are included because Python paints them
                  as a glyph (kind "glyph-dot")

Element names are stable: header, header.title, games.heading, games.none.card,
games.<slug>.card / .radio / .radio.box / .name / .dot / .browse,
apps.heading, apps.select_all, apps.<slug>.card / .checkbox / .checkbox.box /
.name / .subtitle / .dot / .browse, log.heading, log.box, log.text,
log.scrollbar, buttons.launch, buttons.close, footer.label, where <slug> is the
name lowercased with spaces as underscores (ConfigManager.get_config_key).
A ".box" is the canvas the CTk widget draws its square / circle on.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import math
import platform
import sys
import time
from collections import Counter
from ctypes import wintypes
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO))

FIXTURES = REPO / "docs" / "rewrite" / "fixtures.json"
REF_DIR = REPO / "docs" / "rewrite" / "reference"
SPEC = REPO / "docs" / "rewrite" / "ui-spec.json"
ICON = REPO / "iRCL.ico"
CLIENT_W, CLIENT_H = 1200, 700

# Modules whose import (let alone use) would touch config, processes or detection.
FORBIDDEN_MODULES = (
    "iracing_launcher_app.core.config_manager",
    "iracing_launcher_app.managers",
    "iracing_launcher_app.managers.app_manager",
    "iracing_launcher_app.managers.game_manager",
    "iracing_launcher_app.managers.process_manager",
    "iracing_launcher_app.managers.process_tracker",
    "iracing_launcher_app.ui.main_window",
    "iracing_launcher_app.utils",
    "psutil",
)

import tkinter  # noqa: E402

import customtkinter as ctk  # noqa: E402
from customtkinter.windows.widgets.scaling import ScalingTracker  # noqa: E402
from PIL import Image, ImageGrab  # noqa: E402

from iracing_launcher_app.core.activity_logger import ActivityLogger  # noqa: E402
from iracing_launcher_app.core.app_definitions import APPS, RACE_GAMES  # noqa: E402
from iracing_launcher_app.ui import constants as C  # noqa: E402
from iracing_launcher_app.ui.sections.apps_section import AppsSection  # noqa: E402
from iracing_launcher_app.ui.sections.buttons_section import ButtonsSection  # noqa: E402
from iracing_launcher_app.ui.sections.footer import FooterSection  # noqa: E402
from iracing_launcher_app.ui.sections.games_section import GamesSection  # noqa: E402
from iracing_launcher_app.ui.sections.header import HeaderSection  # noqa: E402
from iracing_launcher_app.ui.sections.log_section import LogSection  # noqa: E402

_loaded = [m for m in FORBIDDEN_MODULES if m in sys.modules]
if _loaded:
    sys.exit(f"abort: importing the UI pulled in {_loaded}; refusing to run")

STATUSES = {"idle", "starting", "running", "failed", "stopping", "stopped", "not_found"}
LEVELS = {"info", "success", "error", "warning", "launch", "close", "divider"}

# --------------------------------------------------------------------------- Win32

user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
HGDIOBJ = ctypes.c_void_p
user32.GetDC.argtypes = [wintypes.HWND]
user32.GetDC.restype = wintypes.HDC
user32.ReleaseDC.argtypes = [wintypes.HWND, wintypes.HDC]
user32.PrintWindow.argtypes = [wintypes.HWND, wintypes.HDC, wintypes.UINT]
user32.PrintWindow.restype = wintypes.BOOL
user32.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
user32.GetCursorPos.argtypes = [ctypes.POINTER(wintypes.POINT)]
user32.GetDpiForWindow.argtypes = [wintypes.HWND]
user32.GetDpiForWindow.restype = wintypes.UINT
gdi32.CreateCompatibleDC.argtypes = [wintypes.HDC]
gdi32.CreateCompatibleDC.restype = wintypes.HDC
gdi32.CreateCompatibleBitmap.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int]
gdi32.CreateCompatibleBitmap.restype = wintypes.HBITMAP
gdi32.SelectObject.argtypes = [wintypes.HDC, HGDIOBJ]
gdi32.SelectObject.restype = HGDIOBJ
gdi32.DeleteObject.argtypes = [HGDIOBJ]
gdi32.DeleteDC.argtypes = [wintypes.HDC]
gdi32.GetDIBits.argtypes = [wintypes.HDC, wintypes.HBITMAP, wintypes.UINT, wintypes.UINT,
                            ctypes.c_void_p, ctypes.c_void_p, wintypes.UINT]

PW_CLIENTONLY = 0x1
PW_RENDERFULLCONTENT = 0x2


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
                ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD),
                ("biCompression", wintypes.DWORD), ("biSizeImage", wintypes.DWORD),
                ("biXPelsPerMeter", wintypes.LONG), ("biYPelsPerMeter", wintypes.LONG),
                ("biClrUsed", wintypes.DWORD), ("biClrImportant", wintypes.DWORD)]


def frame_hwnd(root) -> int:
    return int(root.wm_frame(), 16)


def client_size(hwnd) -> tuple[int, int]:
    r = wintypes.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    return r.right - r.left, r.bottom - r.top


def window_rect(hwnd) -> tuple[int, int, int, int]:
    r = wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(r))
    return r.left, r.top, r.right, r.bottom


def cursor_pos() -> tuple[int, int]:
    p = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(p))
    return p.x, p.y


def print_window_client(hwnd) -> Image.Image:
    """Client area through DWM: correct even when the window is covered."""
    w, h = client_size(hwnd)
    screen = user32.GetDC(None)
    mem = gdi32.CreateCompatibleDC(screen)
    bmp = gdi32.CreateCompatibleBitmap(screen, w, h)
    old = gdi32.SelectObject(mem, bmp)
    try:
        if not user32.PrintWindow(hwnd, mem, PW_CLIENTONLY | PW_RENDERFULLCONTENT):
            raise RuntimeError(f"PrintWindow failed: {ctypes.get_last_error()}")
        bmi = BITMAPINFOHEADER()
        bmi.biSize = ctypes.sizeof(BITMAPINFOHEADER)
        bmi.biWidth, bmi.biHeight = w, -h  # top-down
        bmi.biPlanes, bmi.biBitCount = 1, 32
        buf = ctypes.create_string_buffer(w * h * 4)
        if gdi32.GetDIBits(mem, bmp, 0, h, buf, ctypes.byref(bmi), 0) != h:
            raise RuntimeError("GetDIBits failed")
    finally:
        gdi32.SelectObject(mem, old)
        gdi32.DeleteObject(bmp)
        gdi32.DeleteDC(mem)
        user32.ReleaseDC(None, screen)
    return Image.frombuffer("RGB", (w, h), buf.raw, "raw", "BGRX", 0, 1).copy()


def screen_grab_client(hwnd) -> Image.Image:
    w, h = client_size(hwnd)
    p = wintypes.POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(p))
    return ImageGrab.grab(bbox=(p.x, p.y, p.x + w, p.y + h), all_screens=True).convert("RGB")


# --------------------------------------------------------------------------- fixtures


def slug(name: str) -> str:
    return name.lower().replace(" ", "_")


def load_fixtures() -> dict:
    data = json.loads(FIXTURES.read_text(encoding="utf-8"))
    for state, v in data.items():
        where = f"fixtures.json[{state}]"
        if [a["name"] for a in v["apps"]] != list(APPS):
            sys.exit(f"{where}: app names/order differ from app_definitions.APPS")
        if [g["name"] for g in v["games"]] != list(RACE_GAMES):
            sys.exit(f"{where}: game names/order differ from app_definitions.RACE_GAMES")
        for a in v["apps"]:
            if a["status"] not in STATUSES:
                sys.exit(f"{where}: bad app status {a['status']}")
            if a["status"] == "not_found" and a["checked"]:
                sys.exit(f"{where}: {a['name']} is not_found but checked; set_status would uncheck it")
        for g in v["games"]:
            if g["status"] not in STATUSES:
                sys.exit(f"{where}: bad game status {g['status']}")
        if v["selected_game"] is not None and v["selected_game"] not in RACE_GAMES:
            sys.exit(f"{where}: unknown selected_game")
        for line in v["log"]:
            if line["level"] not in LEVELS:
                sys.exit(f"{where}: bad log level {line['level']}")
    return data


# --------------------------------------------------------------------------- window


class Window:
    """The launcher window, built from the real section classes."""

    def __init__(self, state: dict):
        ctk.set_appearance_mode("dark")
        ctk.set_default_color_theme("blue")
        self.state = state
        self.root = root = ctk.CTk()

        sx, sy = cursor_pos()
        # Keep the window away from the mouse so no hover colour is painted.
        x = 40 if sx > root.winfo_screenwidth() // 2 else max(40, root.winfo_screenwidth() - CLIENT_W - 60)
        root.title("iRacing Companion Launcher")
        root.geometry(f"{CLIENT_W}x{CLIENT_H}+{x}+40")
        root.resizable(False, False)
        root.iconbitmap(str(ICON))

        noop = lambda *a, **k: None  # noqa: E731
        # Same order as iRacingLauncherGUI._create_widgets.
        self.header = HeaderSection(root)
        self.footer = FooterSection(root)
        self.content = ctk.CTkFrame(root, fg_color="transparent")
        self.content.pack(pady=5, padx=15, fill="both", expand=True)
        self.selected_game_var = ctk.StringVar(value="")
        self.games = GamesSection(self.content, [g["name"] for g in state["games"]], noop, noop,
                                  self.selected_game_var)
        self.game_cards = self.games.get_cards()
        self.apps = AppsSection(self.content, [a["name"] for a in state["apps"]], noop, noop, noop)
        self.status_cards = self.apps.get_cards()
        self.select_all_btn = self.apps.get_select_all_button()
        self.log = LogSection(self.content, len(state["apps"]))
        self.logger = ActivityLogger()
        self.logger.set_widget(self.log.get_widget())
        self.buttons = ButtonsSection(root, noop, noop)
        self.launch_btn = self.buttons.get_launch_button()
        self.close_btn = self.buttons.get_close_button()

    # -- state ------------------------------------------------------------

    def apply_state(self):
        s = self.state
        for a in s["apps"]:
            card = self.status_cards[a["name"]]
            if a["status"] == "not_found":
                card.set_status("not_found")
            else:
                card.set_status(a["status"], child_count=a["helper_count"])
                card.set_checked(a["checked"])
        for g in s["games"]:
            self.game_cards[g["name"]].set_status(g["status"])
        self.selected_game_var.set(s["selected_game"] or "")

        self.select_all_btn.configure(text=s["select_all_label"])
        self.launch_btn.configure(text=s["launch_label"], state="normal" if s["launch_enabled"] else "disabled")
        self.close_btn.configure(text=s["close_label"], state="normal" if s["close_enabled"] else "disabled")
        self._check_labels_against_rules()

    def _check_labels_against_rules(self):
        """Recompute labels from the live widgets with main_window.py's rules."""
        cards = self.status_cards.values()
        count = sum(1 for c in cards if c.get_checked())
        selected = self.selected_game_var.get()
        found = [c for c in cards if not c.is_not_found]
        if not found:
            select_all = "Select All"
        else:
            select_all = "Deselect All" if sum(1 for c in found if c.get_checked()) == len(found) else "Select All"
        launch = f"Launch with {selected} ({count})" if selected else f"Launch apps ({count})"
        enabled = not (count == 0 and not selected)
        expect = {"select_all_label": select_all, "launch_label": launch, "close_label": f"Close all ({count})",
                  "launch_enabled": enabled, "close_enabled": enabled}
        bad = {k: (self.state[k], v) for k, v in expect.items() if self.state[k] != v}
        if bad:
            raise SystemExit(f"fixture labels disagree with main_window.py rules: {bad}")

    def write_log(self):
        """ActivityLogger.log_message / log_divider, with the fixture's fixed text.

        In the running app the 200 ms scrollbar poll fires between lines, so the
        poll is run after every line here too; the final see('end') then sees
        the scrollbar already in place, as it would in a real session."""
        w = self.log.get_widget()
        for line in self.state["log"]:
            w.configure(state="normal")
            w.insert("end", line["text"] + "\n", line["level"])
            w.see("end")
            w.configure(state="disabled")
            self.root.update_idletasks()
            w._check_if_scrollbars_needed()
            self.root.update_idletasks()
        w.see("end")

    # -- rendering --------------------------------------------------------

    def pump(self, ms: int):
        end = time.monotonic() + ms / 1000
        while time.monotonic() < end:
            self.root.update()
            time.sleep(0.01)

    def cursor_inside(self) -> bool:
        x, y = cursor_pos()
        l, t, r, b = window_rect(frame_hwnd(self.root))
        return l <= x < r and t <= y < b

    def destroy(self):
        """Cancel CTk's pending after() callbacks first, so none fires on a dead widget."""
        try:
            for after_id in self.root.tk.splitlist(self.root.tk.call("after", "info")):
                self.root.after_cancel(after_id)
            self.root.destroy()
        except tkinter.TclError:
            pass


# --------------------------------------------------------------------------- pixels


def hexc(c) -> str:
    return "#%02x%02x%02x" % tuple(c[:3])


def tk_rgb(root, colour: str) -> tuple[int, int, int]:
    r, g, b = root.winfo_rgb(colour)
    return r >> 8, g >> 8, b >> 8


def diffmax(a, b) -> int:
    return max(abs(a[0] - b[0]), abs(a[1] - b[1]), abs(a[2] - b[2]))


def clip(rect, bounds=(0, 0, CLIENT_W, CLIENT_H)):
    x0, y0 = max(rect["x"], bounds[0]), max(rect["y"], bounds[1])
    x1 = min(rect["x"] + rect["w"], bounds[2])
    y1 = min(rect["y"] + rect["h"], bounds[3])
    if x1 <= x0 or y1 <= y0:
        return None
    return {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0}


def bbox_rect(x0, y0, x1, y1):
    return {"x": x0, "y": y0, "w": x1 - x0 + 1, "h": y1 - y0 + 1}


def ink_bbox(pix, rect, bg, thr=2):
    rect = clip(rect)
    if rect is None:
        return None
    xs, ys = [], []
    for y in range(rect["y"], rect["y"] + rect["h"]):
        for x in range(rect["x"], rect["x"] + rect["w"]):
            if diffmax(pix[x, y], bg) > thr:
                xs.append(x)
                ys.append(y)
    if not xs:
        return None
    return bbox_rect(min(xs), min(ys), max(xs), max(ys))


def mode_color(pix, rect):
    rect = clip(rect)
    cnt = Counter(pix[x, y] for y in range(rect["y"], rect["y"] + rect["h"])
                  for x in range(rect["x"], rect["x"] + rect["w"]))
    return cnt.most_common(1)[0][0]


def core_color(pix, rect, bg):
    """The colour text is drawn in: the commonest of the pixels furthest from bg."""
    rect = clip(rect)
    if rect is None:
        return None
    px = [pix[x, y] for y in range(rect["y"], rect["y"] + rect["h"]) for x in range(rect["x"], rect["x"] + rect["w"])]
    d = [math.dist(p, bg) for p in px]
    top = max(d)
    if top == 0:
        return None
    cand = Counter(p for p, dd in zip(px, d) if dd >= 0.9 * top)
    return cand.most_common(1)[0][0]


def coverage(p, bg, ref) -> float:
    v = [ref[i] - bg[i] for i in range(3)]
    n = sum(c * c for c in v)
    if n == 0:
        return 0.0
    t = sum((p[i] - bg[i]) * v[i] for i in range(3)) / n
    return min(1.0, max(0.0, t))


def _circle_row_missing(r: float, k: int) -> float:
    """Area outside a radius-r corner arc in row k (rows of 1 px from the edge)."""
    if k >= r:
        return 0.0
    total, steps = 0.0, 32
    for i in range(steps):
        y = k + (i + 0.5) / steps
        if y < r:
            total += r - math.sqrt(max(0.0, r * r - (r - y) ** 2))
    return total / steps


def corner_fit(pix, rect, bg, ref):
    """Effective corner radius of a rounded rect filling `rect`, by area fit per corner."""
    n = max(2, min(16, rect["w"] // 2, rect["h"] // 2))
    out = {}
    for corner, (sx, sy) in {"tl": (1, 1), "tr": (-1, 1), "bl": (1, -1), "br": (-1, -1)}.items():
        x0 = rect["x"] if sx > 0 else rect["x"] + rect["w"] - 1
        y0 = rect["y"] if sy > 0 else rect["y"] + rect["h"] - 1
        obs = []
        for k in range(n):
            ts = [coverage(pix[x0 + sx * i, y0 + sy * k], bg, ref) for i in range(n)]
            peak = max(ts)
            if peak <= 0:
                obs.append(float(n))
                continue
            xp = next(i for i, t in enumerate(ts) if t >= 0.9 * peak)
            obs.append(sum(1 - t / peak for t in ts[:xp]))
        best = min((sum((_circle_row_missing(r / 20, k) - o) ** 2 for k, o in enumerate(obs)), r / 20)
                   for r in range(0, 20 * n + 1))
        out[corner] = {"radius": round(best[1], 2), "row_missing_px": [round(o, 2) for o in obs]}
    radii = [v["radius"] for v in out.values()]
    return {"effective_radius": round(sum(radii) / 4, 2), "corners": out}


def disk_measure(pix, rect, bg, ref):
    """Centre, outer radius and hole radius of a disk / ring painted in `ref` on `bg`.

    Areas come from summed coverage, so anti-aliasing counts fractionally:
    outer = sum of coverage + enclosed hole, hole = sum of (1 - coverage) over
    the pixels between the first and last run of solid (>= 0.5) pixels in a row."""
    rect = clip(rect)
    outer = hole = 0.0
    wx = wy = 0.0
    for y in range(rect["y"], rect["y"] + rect["h"]):
        ts = [coverage(pix[x, y], bg, ref) for x in range(rect["x"], rect["x"] + rect["w"])]
        runs, start = [], None
        for i, t in enumerate(ts + [0.0]):
            if t >= 0.5 and start is None:
                start = i
            elif t < 0.5 and start is not None:
                runs.append((start, i - 1))
                start = None
        inside = range(runs[0][1] + 1, runs[-1][0]) if len(runs) >= 2 else range(0)
        for i, t in enumerate(ts):
            wt = 1.0 if i in inside else t
            if i in inside:
                hole += 1 - t
            outer += wt
            wx += wt * (rect["x"] + i + 0.5)
            wy += wt * (y + 0.5)
    if outer == 0:
        return None
    return {
        "center": [round(wx / outer, 2), round(wy / outer, 2)],
        "outer_radius": round(math.sqrt(outer / math.pi), 2),
        "hole_radius": round(math.sqrt(hole / math.pi), 2),
        "ring_width": round(math.sqrt(outer / math.pi) - math.sqrt(hole / math.pi), 2),
    }


def corner_grids(pix, rect, n):
    """Raw n x n pixel grids at the four corners of rect (rows top to bottom)."""
    x1, y1 = rect["x"] + rect["w"] - n, rect["y"] + rect["h"] - n
    return {c: grid(pix, {"x": x, "y": y, "w": n, "h": n})
            for c, (x, y) in {"tl": (rect["x"], rect["y"]), "tr": (x1, rect["y"]),
                              "bl": (rect["x"], y1), "br": (x1, y1)}.items()}


def even_floor(v: int) -> int:
    return v // 2 * 2


def shape_facts(pix, rect, bg, ref, declared_radius, thr=2):
    """Painted box and corner geometry of a CTk rounded rect drawn on a canvas at `rect`.

    CTk's DrawEngine (font_shapes on Windows) floors the canvas width and height
    to even numbers and draws corners as anti-aliased circle glyphs of radius
    round(corner_radius) centred (r, r) in from each corner."""
    ink = ink_bbox(pix, rect, bg, thr)
    rule = {"x": rect["x"], "y": rect["y"], "w": even_floor(rect["w"]), "h": even_floor(rect["h"])}
    out = {"painted": ink, "painted_by_even_floor_rule": rule, "rule_matches": ink == rule,
           "declared_radius": declared_radius}
    if ink:
        out.update(corner_fit(pix, ink, bg, ref))
        out["corner_grids"] = corner_grids(pix, ink, min(max(4, round(declared_radius) + 2), ink["w"] // 2, ink["h"] // 2))
    return out


def strip(pix, x, y, dx, dy, n):
    return [hexc(pix[x + dx * i, y + dy * i]) for i in range(n)]


def grid(pix, rect):
    return [" ".join(hexc(pix[x, y])[1:] for x in range(rect["x"], rect["x"] + rect["w"]))
            for y in range(rect["y"], rect["y"] + rect["h"])]


def border_measure(pix, rect, bg, border, fill):
    """Border thickness at the middle of each edge: pixels closer to the border colour than the fill."""
    def run(x, y, dx, dy):
        n = 0
        for i in range(6):
            p = pix[x + dx * i, y + dy * i]
            if math.dist(p, border) < math.dist(p, fill) and math.dist(p, border) < math.dist(p, bg):
                n += 1
            elif n:
                break
        return n
    mx, my = rect["x"] + rect["w"] // 2, rect["y"] + rect["h"] // 2
    r, b = rect["x"] + rect["w"] - 1, rect["y"] + rect["h"] - 1
    return {
        "left": run(rect["x"], my, 1, 0), "right": run(r, my, -1, 0),
        "top": run(mx, rect["y"], 0, 1), "bottom": run(mx, b, 0, -1),
        "left_strip": strip(pix, rect["x"], my, 1, 0, 4),
        "top_strip": strip(pix, mx, rect["y"], 0, 1, 4),
    }


# --------------------------------------------------------------------------- measurement


NO_DESCENDERS = set("ABCDEFHIKLMNOPRSTUVWXZabcdefhiklmnorstuvwxz0123456789.:-!# ")


class Measure:
    def __init__(self, win: Window, img: Image.Image):
        self.win = win
        self.root = win.root
        self.img = img
        self.pix = img.load()
        self.ox, self.oy = self.root.winfo_rootx(), self.root.winfo_rooty()
        self.elements: dict = {}
        self.text: dict = {}
        self.fonts: dict = {}
        self.colors: list = []
        self.shapes: dict = {}
        self.text_regions: list = []

    # -- geometry ---------------------------------------------------------

    def rect(self, w):
        return {"x": w.winfo_rootx() - self.ox, "y": w.winfo_rooty() - self.oy,
                "w": w.winfo_width(), "h": w.winfo_height()}

    def visible(self, w):
        r = self.rect(w)
        x0, y0, x1, y1 = r["x"], r["y"], r["x"] + r["w"], r["y"] + r["h"]
        p = w.master
        while p is not None and p is not self.root:
            pr = self.rect(p)
            x0, y0 = max(x0, pr["x"]), max(y0, pr["y"])
            x1, y1 = min(x1, pr["x"] + pr["w"]), min(y1, pr["y"] + pr["h"])
            p = p.master
        x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, CLIENT_W), min(y1, CLIENT_H)
        if x1 <= x0 or y1 <= y0:
            return None
        return {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0}

    def bg_of(self, w):
        """The Tk background colour of a widget (bypasses CTk's cget, which hides 'bg')."""
        return tk_rgb(self.root, tkinter.Misc.cget(w, "bg"))

    def add(self, name, w, **extra):
        e = {"rect": self.rect(w)}
        vis = self.visible(w)
        if vis != e["rect"]:
            e["visible"] = vis
        e["req"] = {"w": w.winfo_reqwidth(), "h": w.winfo_reqheight()}
        e.update(extra)
        self.elements[name] = e
        return e

    # -- fonts and text ---------------------------------------------------

    def font_key(self, tkw):
        """Font facts straight from Tk for the widget's own font descriptor.

        (tkinter.font.Font(font=...) would round-trip through 'font actual',
        which turns -14 px into 10 pt = 13.3 px and gives wrong metrics.)"""
        desc = tkw.cget("font")
        parts = self.root.tk.splitlist(desc)
        family, size = parts[0], int(parts[1])
        weight = "bold" if "bold" in parts[2:] else "normal"
        key = f"{slug(family)}_{abs(size)}{'_bold' if weight == 'bold' else ''}"
        if key not in self.fonts:
            tk = self.root.tk
            m = dict(zip(*[iter(tk.splitlist(tk.call("font", "metrics", desc)))] * 2))
            a = dict(zip(*[iter(tk.splitlist(tk.call("font", "actual", desc)))] * 2))
            self.fonts[key] = {
                "family": family, "px": abs(size), "weight": weight,
                "resolved_family": str(a["-family"]),
                "ascent": int(m["-ascent"]), "descent": int(m["-descent"]), "linespace": int(m["-linespace"]),
                "fixed": bool(int(m["-fixed"])),
            }
            if self.fonts[key]["fixed"]:
                self.fonts[key]["advance_width"] = int(tk.call("font", "measure", desc, "0"))
        return key, desc

    def measure_text(self, desc, s):
        return int(self.root.tk.call("font", "measure", desc, s))

    def add_text(self, name, tkw, expected_color, kind="text", color_role=None):
        """tkw is the plain tkinter.Label holding the text."""
        string = tkw.cget("text")
        if not string:
            return
        key, desc = self.font_key(tkw)
        m = self.fonts[key]
        r = self.rect(tkw)
        vis = self.visible(tkw)
        bg = self.bg_of(tkw)
        lines = string.split("\n")
        block_h = m["linespace"] * len(lines)
        # TkComputeAnchor: text block centred in the label, C integer division.
        top = r["y"] + int((r["h"] - block_h) / 2)
        ink = ink_bbox(self.pix, vis, bg) if vis else None
        measured = core_color(self.pix, ink, bg) if ink else None
        exp = tk_rgb(self.root, expected_color)
        t = {
            "string": string,
            "font": key,
            "rect": r,
            "text_top_y": top,
            "baseline_y": [top + m["ascent"] + i * m["linespace"] for i in range(len(lines))],
            "advance_width": [self.measure_text(desc, s) for s in lines],
            "ink": ink,
            "color_expected": hexc(exp),
            "color_measured": hexc(measured) if measured else None,
            "background": hexc(bg),
        }
        if vis != r:
            t["visible"] = vis
        if ink and len(lines) == 1 and set(string) <= NO_DESCENDERS:
            # Last ink row of a string without descenders sits right above the baseline.
            t["baseline_check"] = {"ink_bottom_plus_1": ink["y"] + ink["h"], "ok": ink["y"] + ink["h"] == t["baseline_y"][0]}
        self.text[name] = t
        self.color(color_role or f"{name}.text", exp, measured, "text ink core", "source text_color")
        if ink:
            g = clip({"x": ink["x"] - 2, "y": ink["y"] - 2, "w": ink["w"] + 4, "h": ink["h"] + 4})
            self.text_regions.append({"name": name, "kind": kind, **g})
        return t

    def color(self, role, expected, measured, where, source):
        exp = hexc(tk_rgb(self.root, expected)) if isinstance(expected, str) else hexc(expected)
        self.colors.append({"role": role, "expected": exp, "source": source,
                            "measured": hexc(measured) if measured else None, "where": where})

    # -- the window -------------------------------------------------------

    def run(self):
        w = self.win
        root = self.root
        win_bg = tk_rgb(root, root.cget("bg"))
        self.color("window.bg", win_bg, mode_color(self.pix, {"x": 0, "y": 75, "w": 15, "h": 520}),
                   "left margin x0-14", "CTk fg_color dark gray14")

        # header / footer / content / sections
        hdr = w.header.frame
        self.add("header", hdr)
        title = hdr.winfo_children()
        title = [c for c in title if isinstance(c, ctk.CTkLabel)][0]
        self.add("header.title", title)
        self.add_text("header.title", title._label, C.FG_PRIMARY)
        hr = self.rect(hdr)
        self.color("header.bg", C.BG_SECONDARY, mode_color(self.pix, hr), "mode of header rect", "constants.BG_SECONDARY")
        self.shapes["header"] = shape_facts(self.pix, hr, win_bg, tk_rgb(root, C.BG_SECONDARY), 6, thr=0)
        self.shapes["header"]["note"] = ("#252526 on #242424 differs by 1-2 per channel: the corners are invisible "
                                         "and the radius fit is unreliable")
        self.elements["header"]["painted"] = self.shapes["header"]["painted"]

        self.add("footer", w.footer.frame)
        fl = w.footer.frame.winfo_children()[0]
        self.add("footer.label", fl)
        self.add_text("footer.label", fl._label, "#888888")

        self.add("content", w.content)

        # games column
        gc = w.games.container
        self.add("games.column", gc)
        g_hdr, g_cards = gc.winfo_children()[:2]
        self.add("games.heading_row", g_hdr)
        gl = g_hdr.winfo_children()[0]
        self.add("games.heading", gl)
        self.add_text("games.heading", gl._label, C.FG_SECONDARY)
        self.add("games.cards", g_cards)
        none_card = g_cards.winfo_children()[0]
        none_radio, none_label = none_card.winfo_children()[:2]
        self.card("games.none", none_card, "#3e3e42")
        self.radio("games.none", none_radio)
        self.add("games.none.label", none_label)
        self.add_text("games.none.label", none_label._label, "#ffffff")
        for g in w.state["games"]:
            card = w.game_cards[g["name"]]
            base = f"games.{slug(g['name'])}"
            self.card(base, card, "#0e639c")
            self.radio(base, card.radio_btn)
            self.add(base + ".name", card.name_label)
            self.add_text(base + ".name", card.name_label._label, "#888888" if card.is_not_found else "#ffffff")
            self.status_or_browse(base, card, g["status"])

        # apps column
        ac = w.apps.container
        self.add("apps.column", ac)
        a_hdr, a_cards = ac.winfo_children()[:2]
        self.add("apps.heading_row", a_hdr)
        al = [c for c in a_hdr.winfo_children() if isinstance(c, ctk.CTkLabel)][0]
        self.add("apps.heading", al)
        self.add_text("apps.heading", al._label, C.FG_SECONDARY)
        self.button("apps.select_all", w.select_all_btn, "#555555")
        self.add("apps.cards", a_cards)
        for a in w.state["apps"]:
            card = w.status_cards[a["name"]]
            base = f"apps.{slug(a['name'])}"
            self.card(base, card, "#3e3e42")
            self.checkbox(base, card.checkbox, card.get_checked())
            self.add(base + ".text_frame", card.text_frame)
            self.add(base + ".name", card.name_label)
            self.add_text(base + ".name", card.name_label._label, "#888888" if card.is_not_found else "#ffffff")
            self.add(base + ".subtitle", card.subtitle_label)
            self.add_text(base + ".subtitle", card.subtitle_label._label, "#888888")
            self.status_or_browse(base, card, a["status"])

        self.log_measure()

        # buttons
        self.add("buttons.row", w.buttons.frame)
        self.button("buttons.launch", w.launch_btn, "#0e639c")
        self.button("buttons.close", w.close_btn, "#c72e2e")

    def card(self, base, card, border_color):
        e = self.add(base + ".card", card)
        r = e["rect"]
        bg = self.bg_of(card.master)
        fill = tk_rgb(self.root, "#2d2d30")
        border = tk_rgb(self.root, border_color)
        self.color(f"{base}.card.fill", "#2d2d30", mode_color(self.pix, r), "mode of card rect", "card fg_color")
        self.color(f"{base}.card.border", border_color, self.pix[r["x"], r["y"] + r["h"] // 2],
                   "left edge, mid height", "card border_color")
        s = shape_facts(self.pix, r, bg, border, 6)
        e["painted"] = s["painted"]
        key = "game_card" if border_color == "#0e639c" else ("none_card" if base == "games.none" else "app_card")
        if key not in self.shapes:
            s["declared_border_width"] = 1
            s["border"] = border_measure(self.pix, s["painted"], bg, border, fill)
            s["measured_on"] = base + ".card"
            s["note"] = ("radius fit on a 1 px border over a near-background fill is approximate; "
                         "use corner_grids as ground truth")
            self.shapes[key] = s

    def radio(self, base, rb):
        self.add(base + ".radio", rb)
        e = self.add(base + ".radio.box", rb._canvas)
        r = e["rect"]
        bg = self.bg_of(rb._canvas)
        checked = rb._check_state
        ring_col = tk_rgb(self.root, ctk.ThemeManager.theme["CTkRadioButton"]["fg_color" if checked else "border_color"][1])
        e["checked"] = bool(checked)
        e["disabled"] = rb.cget("state") == "disabled"
        e["ink"] = ink_bbox(self.pix, r, bg)
        e["disk"] = disk_measure(self.pix, r, bg, ring_col)
        self.color(f"{base}.radio.ring", ring_col, core_color(self.pix, r, bg), "radio canvas",
                   "blue.json CTkRadioButton " + ("fg_color" if checked else "border_color"))
        key = "radio_checked" if checked else "radio_unchecked"
        if key not in self.shapes:
            self.shapes[key] = {
                "measured_on": base + ".radio.box", "canvas": r, "ink": e["ink"], "disk": e["disk"],
                "declared": {"size": 22, "border_width": 6 if checked else 3, "corner_radius": 1000,
                             "ring_color": hexc(ring_col)},
                "pixels": grid(self.pix, r),
            }

    def checkbox(self, base, cb, checked):
        self.add(base + ".checkbox", cb)
        e = self.add(base + ".checkbox.box", cb._canvas)
        r = e["rect"]
        bg = self.bg_of(cb._canvas)
        theme = ctk.ThemeManager.theme["CTkCheckBox"]
        ref = tk_rgb(self.root, theme["fg_color" if checked else "border_color"][1])
        e["checked"] = checked
        e["disabled"] = cb.cget("state") == "disabled"
        e["ink"] = ink_bbox(self.pix, r, bg)
        self.color(f"{base}.checkbox." + ("fill" if checked else "border"), ref,
                   mode_color(self.pix, r) if checked else core_color(self.pix, r, bg),
                   "checkbox canvas", "blue.json CTkCheckBox " + ("fg_color" if checked else "border_color"))
        key = "checkbox_checked" if checked else "checkbox_unchecked"
        if key in self.shapes:
            return
        s = {"measured_on": base + ".checkbox.box", "canvas": r, "ink": e["ink"],
             "declared": {"size": 24, "corner_radius": 6, "border_width": 3}}
        box = e["ink"]
        s.update(corner_fit(self.pix, box, bg, ref))
        if checked:
            mark = tk_rgb(self.root, theme["checkmark_color"][1])
            xs, ys = [], []
            for y in range(box["y"], box["y"] + box["h"]):
                for x in range(box["x"], box["x"] + box["w"]):
                    p = self.pix[x, y]
                    if math.dist(p, mark) < math.dist(p, ref):
                        xs.append(x)
                        ys.append(y)
            s["checkmark_ink"] = bbox_rect(min(xs), min(ys), max(xs), max(ys))
            s["checkmark_color_measured"] = hexc(core_color(self.pix, s["checkmark_ink"], ref))
            s["declared"]["checkmark_font_size"] = round(24 * 0.58, 2)
            self.color("checkbox.checkmark", mark, core_color(self.pix, s["checkmark_ink"], ref),
                       base + ".checkbox.box", "blue.json CTkCheckBox checkmark_color")
        else:
            fill = bg
            s["border"] = border_measure(self.pix, box, self.bg_of(cb._canvas), ref, fill)
        s["pixels"] = grid(self.pix, r)
        self.shapes[key] = s

    def status_or_browse(self, base, card, status):
        if card.is_not_found:
            self.button(base + ".browse", card.browse_btn, "#555555")
            return
        lbl = card.status_label
        e = self.add(base + ".dot", lbl)
        vis = e.get("visible", e["rect"])
        bg = self.bg_of(lbl._label)
        expected = C.STATUS_COLORS.get(status, C.STATUS_COLORS["idle"])
        col = tk_rgb(self.root, expected)
        e["status"] = status
        e["ink"] = ink_bbox(self.pix, vis, bg)
        e["disk"] = disk_measure(self.pix, e["ink"], bg, col) if e["ink"] else None
        self.add_text(base + ".dot", lbl._label, expected, kind="glyph-dot", color_role=f"{base}.dot[{status}]")
        if "status_dot" not in self.shapes and e["ink"]:
            self.shapes["status_dot"] = {"measured_on": base + ".dot", "label_rect": e["rect"], "ink": e["ink"],
                                         "disk": e["disk"], "pixels": grid(self.pix, e["ink"])}

    def button(self, name, btn, fill_color):
        e = self.add(name, btn)
        r = e.get("visible", e["rect"])
        bg = tk_rgb(self.root, btn._apply_appearance_mode(btn._bg_color))
        fill = tk_rgb(self.root, fill_color)
        e["state"] = btn.cget("state")
        self.color(name + ".fill" + ("[disabled]" if e["state"] == "disabled" else ""), fill_color,
                   mode_color(self.pix, r), "mode of button rect", "button fg_color")
        tl = btn._text_label
        if tl is not None:
            self.add(name + ".text", tl)
            text_col = btn._text_color_disabled if e["state"] == "disabled" else btn._text_color
            self.add_text(name + ".text", tl, btn._apply_appearance_mode(text_col))
        s = shape_facts(self.pix, e["rect"], bg, fill, btn.cget("corner_radius"))
        e["painted"] = s["painted"]
        key = {"apps.select_all": "select_all_button", "buttons.launch": "action_button",
               "buttons.close": "action_button"}.get(name, "browse_button")
        if key not in self.shapes:
            s["measured_on"] = name
            self.shapes[key] = s

    def log_measure(self):
        w = self.win
        lc = w.log.container
        self.add("log.column", lc)
        heading = [c for c in lc.winfo_children() if isinstance(c, ctk.CTkLabel)][0]
        self.add("log.heading", heading)
        self.add_text("log.heading", heading._label, C.FG_SECONDARY)
        box = w.log.get_widget()
        e = self.add("log.box", box)
        bg = self.bg_of(box.master._canvas) if hasattr(box.master, "_canvas") else tk_rgb(self.root, self.root.cget("bg"))
        fill = tk_rgb(self.root, C.BG_TERTIARY)
        self.color("log.bg", C.BG_TERTIARY, mode_color(self.pix, e["rect"]), "mode of log box", "constants.BG_TERTIARY")
        s = shape_facts(self.pix, e["rect"], bg, fill, 6)
        e["painted"] = s["painted"]
        if "log_box" not in self.shapes:
            s["measured_on"] = "log.box"
            self.shapes["log_box"] = s

        tw = box._textbox
        te = self.add("log.text", tw)
        opts = {k: str(tw.cget(k)) for k in ("wrap", "borderwidth", "padx", "pady", "spacing1", "spacing2",
                                              "spacing3", "relief", "highlightthickness", "bg", "fg")}
        key, desc = self.font_key(tw)
        bd, padx, pady = int(tw.cget("borderwidth")), int(tw.cget("padx")), int(tw.cget("pady"))
        tr = te["rect"]
        inner_w = tr["w"] - 2 * (bd + padx)
        char_w = self.measure_text(desc, "0")
        log = {
            "font": key, "options": opts, "yview": [round(v, 6) for v in tw.yview()],
            "text_inner_left_x": tr["x"] + bd + padx, "text_inner_top_y": tr["y"] + bd + pady,
            "text_inner_width": inner_w, "char_width": char_w,
            "chars_per_display_line": inner_w // char_w,
            "line_pitch": self.fonts[key]["linespace"],
            "lines": [],
        }
        sb = box._y_scrollbar
        if sb.winfo_ismapped():
            se = self.add("log.scrollbar", sb)
            track = tk_rgb(self.root, sb._apply_appearance_mode(sb._fg_color))
            thumb_col = tk_rgb(self.root, sb._apply_appearance_mode(sb._button_color))
            ink = ink_bbox(self.pix, se["rect"], track)
            se["thumb_ink"] = ink
            se["track_color"] = hexc(track)
            log["scrollbar_shown"] = True
            self.color("log.scrollbar.thumb", thumb_col, core_color(self.pix, ink, track) if ink else None,
                       "scrollbar thumb ink", "blue.json CTkTextbox scrollbar_button_color")
            if ink and "scrollbar_thumb" not in self.shapes:
                s = corner_fit(self.pix, ink, track, thumb_col)
                s.update({"measured_on": "log.scrollbar", "ink": ink, "scrollbar_rect": se["rect"],
                          "declared": {"width": 8, "corner_radius": 1000, "border_spacing": 0},
                          "yview": log["yview"],
                          "top_grid": grid(self.pix, {"x": ink["x"], "y": ink["y"], "w": ink["w"], "h": 6})})
                self.shapes["scrollbar_thumb"] = s
        else:
            log["scrollbar_shown"] = False

        idx = tw.index("@0,0")
        seen = set()
        while idx not in seen:
            seen.add(idx)
            info = tw.dlineinfo(idx)
            if info is None:
                break
            x, y, lw, lh, base = info
            start = tw.index(f"{idx} display linestart")
            end = tw.index(f"{idx} display lineend")
            text = tw.get(start, end)
            tags = [t for t in tw.tag_names(start) if t in LEVELS]
            line_rect = {"x": tr["x"] + x, "y": tr["y"] + y, "w": lw, "h": lh}
            vis = clip(line_rect, (tr["x"], tr["y"], tr["x"] + tr["w"], tr["y"] + tr["h"]))
            ink = ink_bbox(self.pix, vis, fill) if vis and text.strip() else None
            ln = {"index": start, "text": text, "level": tags[0] if tags else None,
                  "rect": line_rect, "baseline_y": tr["y"] + y + base,
                  "wrapped_continuation": not start.endswith(".0"), "ink": ink}
            if vis != line_rect:
                ln["visible"] = vis
            if ink:
                col = core_color(self.pix, ink, fill)
                ln["color_measured"] = hexc(col)
                ln["color_expected"] = C.LOG_COLORS[ln["level"]].lower()
                g = clip({"x": ink["x"] - 2, "y": ink["y"] - 2, "w": ink["w"] + 4, "h": ink["h"] + 4},
                         (tr["x"], tr["y"], tr["x"] + tr["w"], tr["y"] + tr["h"]))
                self.text_regions.append({"name": f"log.line[{start}]", "kind": "log", **g})
            log["lines"].append(ln)
            idx = tw.index(f"{idx} +1 display lines")
        lv = Counter()
        for ln in log["lines"]:
            if ln.get("color_measured") and ln["level"] not in lv:
                lv[ln["level"]] += 1
                self.color(f"log.{ln['level']}", C.LOG_COLORS[ln["level"]], ln["color_measured"] and
                           tuple(int(ln["color_measured"][i:i + 2], 16) for i in (1, 3, 5)),
                           f"log line {ln['index']}", "constants.LOG_COLORS")
        self.log = log

    # -- layout facts -----------------------------------------------------

    def layout(self):
        def slave(name, w):
            info = w.pack_info()
            return {"name": name, "y": self.rect(w)["y"], "x": self.rect(w)["x"], "w": w.winfo_width(),
                    "h": w.winfo_height(), "req_w": w.winfo_reqwidth(), "req_h": w.winfo_reqheight(),
                    "side": info["side"], "padx": str(info["padx"]), "pady": str(info["pady"]),
                    "expand": bool(int(info["expand"])), "fill": info["fill"]}

        w = self.win
        top = [slave("header", w.header.frame), slave("footer", w.footer.frame),
               slave("content", w.content), slave("buttons", w.buttons.frame)]
        cols = [slave("games.column", w.games.container), slave("apps.column", w.apps.container),
                slave("log.column", w.log.container)]
        return {"root_pack_order": top, "content_pack_order": cols,
                "root_reqheight": self.root.winfo_reqheight()}


# --------------------------------------------------------------------------- driver


def capture_state(name: str, state: dict, verify_screen: bool):
    for attempt in range(3):
        win = Window(state)
        try:
            win.pump(150)
            hwnd = frame_hwnd(win.root)
            scaling = ScalingTracker.get_window_scaling(win.root)
            wscaling = ScalingTracker.get_widget_scaling(win.root)
            if scaling != 1.0 or wscaling != 1.0:
                raise SystemExit(f"abort: CTk window scaling {scaling}, widget scaling {wscaling}; need exactly 1.0 "
                                 "(set the display to 100%)")
            win.apply_state()
            win.pump(100)
            win.write_log()
            hovered = False
            end = time.monotonic() + 0.7  # > 3 scrollbar polls (200 ms) and any CTk after() redraws
            while time.monotonic() < end:
                win.root.update()
                hovered |= win.cursor_inside()
                time.sleep(0.01)
            if client_size(hwnd) != (CLIENT_W, CLIENT_H):
                raise SystemExit(f"abort: client area is {client_size(hwnd)}, expected {CLIENT_W}x{CLIENT_H}")
            img = print_window_client(hwnd)
            hovered |= win.cursor_inside()
            if hovered:
                print(f"  {name}: mouse entered the window, retrying")
                continue
            check = None
            if verify_screen:
                win.root.attributes("-topmost", True)
                win.pump(300)
                shot = screen_grab_client(hwnd)
                win.root.attributes("-topmost", False)
                pts = [(i % CLIENT_W, i // CLIENT_W) for i, (a, b) in enumerate(zip(img.getdata(), shot.getdata()))
                       if a != b]
                check = {"differing_pixels_vs_screen_grab": len(pts)}
                if pts:
                    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
                    check["differing_bbox"] = bbox_rect(min(xs), min(ys), max(xs), max(ys))
                    shot.save(REPO / "target" / f"screen-grab-{name}.png") if (REPO / "target").is_dir() else None
                print(f"  {name}: PrintWindow vs screen grab: {check}")
            env = {
                "window_scaling": scaling, "widget_scaling": wscaling,
                "dpi": int(user32.GetDpiForWindow(hwnd)),
                "tk_scaling_px_per_pt": round(float(win.root.tk.call("tk", "scaling")), 4),
                "tk_patchlevel": win.root.tk.call("info", "patchlevel"),
            }
            m = Measure(win, img)
            m.run()
            layout = m.layout()
            return img, m, layout, env, check
        finally:
            win.destroy()
    raise SystemExit(f"abort: the mouse kept entering the window while capturing {name}")


CARD_SLUGS = {slug(n) for n in list(APPS) + list(RACE_GAMES)}


def aggregate_colors(per_state: dict) -> list:
    rows = {}
    for state, colors in per_state.items():
        for c in colors:
            role = c["role"]
            # collapse per-card roles into one role per kind
            parts = role.split(".")
            if parts[0] in ("apps", "games") and parts[1] in CARD_SLUGS:
                role = parts[0] + ".<card>." + ".".join(parts[2:])
            key = (role, c["expected"])
            r = rows.setdefault(key, {"role": role, "expected": c["expected"], "source": c["source"],
                                      "measured": Counter(), "where": c["where"], "states": set()})
            r["measured"][c["measured"]] += 1
            r["states"].add(state)
    out = []
    for r in rows.values():
        measured = [m for m, _ in r["measured"].most_common()]
        out.append({"role": r["role"], "expected": r["expected"], "source": r["source"],
                    "measured": measured, "match": measured == [r["expected"]],
                    "where": r["where"], "states": sorted(r["states"])})
    return sorted(out, key=lambda r: r["role"])


README = [
    "Measured from the running Python app by tools/capture_reference.py; see its docstring for the full description.",
    "Coordinates: window client pixels, origin top-left. Rect = {x, y, w, h}. Colours #rrggbb.",
    "states.<state>.elements: Tk widget rect ('rect'), clipped rect when ancestors clip it ('visible'), "
    "pixel-measured painted box ('ink'), requested size ('req').",
    "states.<state>.text: per label: string, font key (see 'fonts'), Tk text_top_y and baseline_y "
    "(TkComputeAnchor centring), ink box from pixels, expected vs measured colour.",
    "states.<state>.log: Text widget options, yview, every visible display line with rect, baseline, level, ink.",
    "states.<state>.text_regions: what tools/ui_diff.py counts as text.",
    "shapes: corner radii fitted from pixel coverage, border widths, checkbox/radio/dot/scrollbar geometry, raw pixel grids "
    "(rows of 'rrggbb' hex, space-separated).",
    "colors: expected (source/theme) vs measured; match=false is a finding, see 'notes'.",
    "layout: Tk pack results (idle; per state in states.<state>.pack): requested vs actual sizes of the root's "
    "and the content frame's slaves, including what happens to the 713 px of requested height in 700 px.",
    "notes: findings that the numbers alone do not explain.",
]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--state", help="capture one state only")
    ap.add_argument("--verify-screen", action="store_true",
                    help="briefly raise the window and compare PrintWindow with a screen grab")
    args = ap.parse_args()

    fixtures = load_fixtures()
    names = [args.state] if args.state else list(fixtures)
    for n in names:
        if n not in fixtures:
            sys.exit(f"unknown state {n}; have {list(fixtures)}")

    spec = json.loads(SPEC.read_text(encoding="utf-8")) if SPEC.exists() and args.state else {}
    spec_states = spec.get("states", {})
    color_rows = {s: v.pop("_colors", []) for s, v in spec_states.items()}
    shapes = spec.get("shapes", {})
    fonts = spec.get("fonts", {})
    REF_DIR.mkdir(parents=True, exist_ok=True)
    env = None
    checks = spec.get("environment", {}).get("capture_check", {})

    run_shapes: dict = {}
    for n in names:
        print(f"capturing {n}")
        img, m, layout, env, check = capture_state(n, fixtures[n], args.verify_screen)
        if img.size != (CLIENT_W, CLIENT_H):
            sys.exit(f"abort: {n} capture is {img.size}")
        img.save(REF_DIR / f"{n}.png")
        if check:
            checks[n] = check
        fonts.update(m.fonts)
        for k, v in m.shapes.items():
            run_shapes.setdefault(k, v)  # first state in this run that shows the shape wins
        color_rows[n] = m.colors
        spec_states[n] = {"elements": m.elements, "text": m.text, "log": m.log, "pack": layout,
                          "text_regions": m.text_regions}

    out = {
        "_readme": README,
        "generated_by": "tools/capture_reference.py",
        "environment": {
            "os": platform.platform(), "python": platform.python_version(),
            "customtkinter": ctk.__version__, **env,
            "capture_method": "PrintWindow(frame hwnd, PW_CLIENTONLY | PW_RENDERFULLCONTENT) into a 32-bit DIB; "
                              "DWM composition, so correct when the window is covered",
            "capture_check": checks,
        },
        "window": {"client_w": CLIENT_W, "client_h": CLIENT_H, "scaling": env["window_scaling"],
                   "title": "iRacing Companion Launcher", "resizable": False},
        "fonts": dict(sorted(fonts.items())),
        "colors": aggregate_colors(color_rows),
        "shapes": dict(sorted({**shapes, **run_shapes}.items())),
        "layout": {"note": "Tk pack results for the idle state; states.<state>.pack has them per state "
                           "(the buttons row widens when the Launch label is long)",
                   **spec_states.get("idle", spec_states[names[0]])["pack"]},
        "notes": NOTES,
        "states": {s: {**spec_states[s], "_colors": color_rows[s]} for s in fixtures if s in spec_states},
    }
    SPEC.write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {SPEC.relative_to(REPO)} and {len(names)} image(s) in {REF_DIR.relative_to(REPO)}")


NOTES = {
    "height_overflow": (
        "The root's pack slaves request 713 px in a 700 px client: header 70 + pady 10, footer 28 + pady 5, "
        "content 485 + pady 10, buttons row 60 + pady 45. Tk pack hands out height in packing order (header, "
        "footer, content, buttons), each slave getting its full request while the cavity lasts, so header, "
        "footer and content are untouched and the buttons row, packed last, absorbs the whole 13 px: it gets "
        "700 - 80 - 33 - 495 = 92 px, minus its pady 35 + 10 = 47 px of height (requested 60). Pack does not "
        "clip it; it resizes it, and the two CTkButtons inside are resized to 47 px and redrawn at that height "
        "(painted 46 px, see even_floor_painting). The row starts at y 610 (content bottom 575 + 35), buttons "
        "occupy y 610-656, the 10 px pady follows, and the footer sits at 667-694 with 5 px below it. content's "
        "expand=True gets no extra height. The Rust layout must produce buttons 47 px tall (46 painted), not 60."),
    "header_title_squeeze": (
        "The header frame is fixed at 70 px (pack_propagate False) and the title label has pady 20, so the "
        "label gets 30 px though its text needs 32 (Segoe UI 24 bold linespace). Tk centres the text block with "
        "C integer division: top = 20 + int((30 - 32) / 2) = 19, baseline 45."),
    "label_text_placement": (
        "Every text sits in a plain tkinter.Label whose height is the font linespace times the line count, "
        "centred inside its CTk widget. When the widget is shorter than the text (title, wrapped game names, "
        "status dots) the block is centred by TkComputeAnchor with C integer division (truncates toward 0). "
        "text_top_y and baseline_y in states.*.text follow that rule and were checked against the pixels: for "
        "every single-line text without descenders the bottom ink row + 1 equals baseline_y (baseline_check)."),
    "columns": (
        "content is 1170 px wide (window 1200 - padx 15 * 2). Requested widths: games 200 + padx 10, apps "
        "200 + padx 10, log 400 (CTkTextbox width). The 350 px surplus goes to the three expanding columns, "
        "each taking int(remaining / columns left): 116, 117, 117. Result: games x 15 w 316, apps x 341 w 317, "
        "log x 668 w 517 (log is packed side=right but last, so it fills the remainder)."),
    "even_floor_painting": (
        "CTk's DrawEngine on Windows (font_shapes) floors a canvas's width and height to even numbers before "
        "drawing. Odd-sized widgets leave their last column / row unpainted (parent colour shows): app cards "
        "317x55 paint 316x54, game cards 316x55 paint 316x54, the log box 517x445 paints 516x444, the action "
        "buttons 200x47 paint 200x46, Browse buttons squeezed to 80x35 paint 80x34. Card pitch is 65 px (55 + "
        "10 pady), so the visible gap between cards is 11 px. elements.*.painted holds the measured box; "
        "shapes.*.rule_matches confirms the rule for each shape kind. One visible exception: when a canvas is "
        "odd-sized, the anti-aliasing of the right-hand corner glyphs spills faintly into the unpainted last "
        "column; at high contrast that shows (the 205 px Launch button in mixed / log-full paints x 387-590 "
        "and has 8 faint pixels at x 591, so its measured ink box is 205 wide)."),
    "corners": (
        "Corners are anti-aliased circle glyphs (CustomTkinter_shapes_font) of radius round(corner_radius) "
        "centred (r, r) in from each painted corner. Measured by area fit, the effective radius is about "
        "r - 0.5 and differs per corner: tl/br about 5.8 and tr/bl about 5.2 for r 6; 10.6 / 11.35 for r 12. "
        "shapes.*.corner_grids hold the raw corner pixels as ground truth. Card borders are 1 px "
        "(shapes.app_card.border). The header's corners are invisible (#252526 on #242424)."),
    "status_dot": (
        "The dot is the U+25CF glyph in Segoe UI 40 px, in a label that requests 24x54 but is squeezed to "
        "24x35 by the 55 px card (pady 5 / 15). In every card and state its ink is the 18x17 box at "
        "(card.x + card.w - 36, card.y + 19), where card is the widget rect; as a disk its radius is 8.6 and "
        "its centre (card.x + card.w - 26.6..26.9, card.y + 27.5); the 0.3 px spread is glyph anti-aliasing "
        "varying with x position (elements.*.dot.disk per card, shapes.status_dot for the pixels). 'stopping' "
        "has no STATUS_COLORS entry and paints #666666."),
    "checkbox_radio": (
        "Checkbox: 24x24 canvas at (card.x + 10, card.y + 15), radius 6 (effective ~5.5), unchecked border "
        "3 px #949a9f with the card colour inside, checked fill #1f6aa5 with a #e5e5e5 checkmark glyph (ink box "
        "14x12 at +5,+6). Radio: 22x22 canvas at (card.x + 10, card.y + 16), circle of radius 11; unchecked "
        "ring 3 px #949a9f, checked ring 6 px #1f6aa5, inside the card colour. Disabled checkboxes and radios "
        "paint exactly like enabled ones: CTk only recolours their (empty) text label. shapes.checkbox_* and "
        "shapes.radio_* include the full pixel grids."),
    "buttons": (
        "Disabled CTkButtons keep their fill; only the text turns #999999 (gray60). A CTkButton's width is a "
        "minimum: with 'Launch with iRacing (5)' (181 px of text) the Launch button grows to 181 + 2 * 12 "
        "(corner radius) = 205 px, the row becomes 445 px and is re-centred (x 377). Text in a button is "
        "centred in the full widget height (47 px for the action buttons)."),
    "browse_squeeze": (
        "Inside a card, pack serves the left-packed checkbox and name block first; the right-packed Browse "
        "button (packed when the status becomes not_found) gets what is left, at most 80 px. With 'MarvinsAIRA "
        "Refactored' (164 px) only 57 px remain (all-missing: x 586 w 57, painted 56x34) and the text is "
        "centred in that. Browse height is 35 (36 requested, card 55 minus pady 10 * 2)."),
    "game_name_wrap": (
        "GameCard wraps names longer than 18 characters at the last space before 18: 'Assetto Corsa "
        "Competizione' becomes two lines. The label (38 px of text) is squeezed to 35 by the card, so its "
        "block starts 1 px above the label (baselines 279 and 296 for a card at y 255)."),
    "log": (
        "Consolas 14 px: ascent 13, descent 4, line pitch 17, advance 8 px. The Text widget sits 6 px inside "
        "the log box on the left and top (corner radius 6) and 6 px from the right / bottom edge (or 3 px + "
        "8 px scrollbar + 3 px on the right when the scrollbar shows); it has borderwidth 1 and padx/pady 1, "
        "so the first line's top is log.text.y + 2 and its left edge log.text.x + 2. wrap=char: 62 characters "
        "per display line without the scrollbar (501 px inner width), 61 with it (493 px). Every message ends "
        "with a newline, so an empty display line follows the last message; see('end') scrolls until that "
        "empty line is fully visible, which leaves the top line cut (log-full shows 6 px of it). The divider "
        "is 50 U+2500 characters = 400 px, painted as a 1 px line 8 px below the line top. states.*.log.lines "
        "lists every visible display line with its rect, baseline and ink."),
    "scrollbar": (
        "The log's CTkScrollbar appears only when the content overflows (CTk polls every 200 ms). It is 8 px "
        "wide at x = log.box.x + 506 (1174), from y 129 to 562; its track is the log colour (invisible), the "
        "thumb #696969 is a pill (radius ~4 / effective ~2.6) whose length and offset follow yview "
        "(shapes.scrollbar_thumb)."),
    "fonts": (
        "Font sizes in the source are pixels (CTk passes them to Tk negated). Segoe UI resolves to Segoe UI, "
        "Consolas to Consolas. Metrics in 'fonts' are Tk's for the exact descriptor; Tk line height = ascent + "
        "descent. text_regions in ui_diff absorb glyph-edge differences."),
    "colors": (
        "Every surface, border, text and state colour sampled from the screenshots equals the value in the "
        "source or in CTk's blue.json (dark variant). See 'colors' for the table."),
    "window_corners": (
        "Windows 11 rounds the window's outer corners. PrintWindow returns the full client area, so the "
        "reference images show client content in the bottom corners; a screen grab would show 35 desktop "
        "pixels there instead (environment.capture_check)."),
}

if __name__ == "__main__":
    main()
