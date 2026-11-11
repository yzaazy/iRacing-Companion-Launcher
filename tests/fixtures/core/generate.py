"""Regenerates the core_* test fixtures from the Python reference.

Run from the repo root: python tests/fixtures/core/generate.py

Every .ini here is either written by configparser itself or hand-written
input; the .expected.json and .saved.ini next to it are what configparser
and ConfigManager make of it, so the Rust tests compare against the real
library rather than a recollection of it.
"""

import configparser
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.abspath(os.path.join(HERE, "..", "..", "..")))

from iracing_launcher_app.core.app_definitions import APPS, RACE_GAMES  # noqa: E402


def dump_definitions():
    apps = [
        {
            "name": name,
            "exe": d["exe"],
            "shortcut_names": d["shortcut_names"],
            "paths": d["paths"],
            # AppManager reads it with .get(..., False)
            "needs_shell_launch": d.get("needs_shell_launch", False),
        }
        for name, d in APPS.items()
    ]
    games = [
        {
            "name": name,
            "exe": d["exe"],
            "shortcut_names": d["shortcut_names"],
            "steam_appid": d["steam_appid"],
            "steam_folder": d["steam_folder"],
            "paths": d["paths"],
        }
        for name, d in RACE_GAMES.items()
    ]
    with open(os.path.join(HERE, "definitions.json"), "w", encoding="utf-8") as f:
        json.dump({"apps": apps, "games": games}, f, indent=2, ensure_ascii=False)
        f.write("\n")


def write_like_the_app(name, app_paths, settings):
    """Writes config.ini exactly as ConfigManager.save_config does: locale encoding."""
    cp = configparser.ConfigParser()
    cp["AppPaths"] = {}
    cp["Settings"] = {}
    for k, v in app_paths:
        cp["AppPaths"][k] = v
    for k, v in settings:
        cp.set("Settings", k, v)
    with open(os.path.join(HERE, name), "w") as f:  # no encoding: as the app does
        cp.write(f)


def load_like_the_app(path):
    """ConfigManager._load_config: read with the locale encoding, keep going on error."""
    cp = configparser.ConfigParser()
    if os.path.exists(path):
        try:
            cp.read(path)
        except Exception:
            pass
    if "AppPaths" not in cp:
        cp["AppPaths"] = {}
    if "Settings" not in cp:
        cp["Settings"] = {}
    return cp


def expected_for(name):
    path = os.path.join(HERE, name)
    cp = load_like_the_app(path)
    out = {
        "defaults": [[k, v] for k, v in cp._defaults.items()],
        "sections": [[s, [[k, v] for k, v in d.items()]] for s, d in cp._sections.items()],
        "get": {},
        "getboolean": {},
    }
    for section in cp.sections():
        keys = list(cp._sections[section]) + list(cp._defaults)
        for key in keys + ["missing_key"]:
            ident = f"{section}/{key}"
            try:
                out["get"][ident] = cp.get(section, key, fallback=None)
            except Exception:
                out["get"][ident] = {"error": True}
            try:
                out["getboolean"][ident] = cp.getboolean(section, key, fallback=None)
            except Exception:
                out["getboolean"][ident] = {"error": True}
    stem = os.path.splitext(name)[0]
    with open(os.path.join(HERE, stem + ".expected.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, indent=2, ensure_ascii=False)
        f.write("\n")
    # What ConfigManager.save_config writes after that load; UTF-8 because
    # the Rust port writes UTF-8 (behaviour.md, deliberate difference 6).
    with open(os.path.join(HERE, stem + ".saved.ini"), "w", encoding="utf-8") as f:
        cp.write(f)


def win(text):
    """Forward slashes to backslashes, so this file needs no escaped backslashes."""
    return text.replace("/", chr(92))


def write_raw(name, text, encoding="utf-8"):
    with open(os.path.join(HERE, name), "w", encoding=encoding, newline="") as f:
        f.write(text)


def main():
    dump_definitions()

    write_like_the_app(
        "app_written.ini",
        [
            ("fanatec", r"C:\Program Files\Fanatec\FanatecUI\UI\Fanatec.exe"),
            ("simhub", r"C:\Program Files (x86)\SimHub\SimHubWPF.exe"),
            ("game_iracing", "steam://rungameid/266410"),
        ],
        [
            ("fanatec_enabled", str(True)),
            ("simhub_enabled", str(False)),
            ("garage61_enabled", "False"),
            ("selected_game", ""),
        ],
    )

    write_like_the_app(
        "cp1252.ini",
        [
            ("crew_chief_v4", win("C:/Programme/Spiele/Équipe Müller/CrewChiefV4.exe")),
            ("game_rfactor_2", win("D:/Jeux/rFactor 2 – Édition/rFactor2.exe")),
        ],
        [("crew_chief_v4_enabled", "True"), ("selected_game", "rFactor 2")],
    )

    write_raw(
        "comments.ini",
        "# Hand-edited config\r\n"
        "; another comment style\r\n"
        "[DEFAULT]\r\n"
        "Shared_Key = from default\r\n"
        "\r\n"
        "[Settings]\r\n"
        "Fanatec_Enabled: yes\r\n"
        "   SimHub_Enabled   =   OFF   \r\n"
        "selected_game:iRacing\r\n"
        "garage61_enabled = 1\r\n"
        "    # indented comment\r\n"
        "crew_chief_v4_enabled = maybe\r\n"
        "empty_value =\r\n"
        "colon_and_equals : a = b\r\n"
        "multi = first line\r\n"
        "    second line\r\n"
        "\r\n"
        "    after blank\r\n"
        "\r\n"
        "[My Custom Section]\r\n"
        "Some Key = Some Value\r\n"
        "url = http://example.com:8080/x\r\n"
        "\r\n"
        "[AppPaths]\r\n"
        + win("FANATEC = C:/Program Files/Fanatec/FanatecUI/UI/Fanatec.exe\r\n")
        + "trailing = value with trailing spaces   \r\n"
        "[Trailing Junk] ; ignored after bracket\r\n"
        "k=v\r\n",
    )

    write_raw(
        "interpolation.ini",
        win(
            "[AppPaths]\n"
            "base = C:/Games\n"
            "percent = C:/100%% Racing/app.exe\n"
            "ref = %(base)s/iRacing/x.exe\n"
            "chain = %(ref)s.bak\n"
            "bad = C:/50% off/x.exe\n"
            "missing_ref = %(nope)s/x\n"
            "loop = %(loop)s\n"
            "[Settings]\n"
            "flag = %(on)s\n"
            "on = yes\n"
        ),
    )

    write_raw(
        "parse_error.ini",
        "[AppPaths]\n"
        "fanatec = " + win("C:/a.exe") + "\n"
        "this line has no delimiter\n"
        "simhub = " + win("C:/b.exe") + "\n"
        "[Settings]\n"
        "selected_game = iRacing\n",
    )

    write_raw("lf_only.ini", "[Settings]\nselected_game = iRacing\n[AppPaths]\nx = y\n")
    write_raw("empty.ini", "")

    for name in [
        "app_written.ini",
        "cp1252.ini",
        "comments.ini",
        "interpolation.ini",
        "parse_error.ini",
        "lf_only.ini",
        "empty.ini",
        "missing.ini",
    ]:
        expected_for(name)


if __name__ == "__main__":
    main()
