"""
Compare the Rust UI snapshots with the Python reference screenshots.

    python tools/ui_diff.py                  # every state in docs/rewrite/fixtures.json
    python tools/ui_diff.py --state mixed    # one state
    python tools/ui_diff.py --self-test      # prove the tool passes a copy and finds planted changes

For each state it compares docs/rewrite/reference/<state>.png with
target/ui-snapshots/<state>.png in the three classes of the parity criteria
(docs/rewrite/contracts.md, "Parity criteria"):

  flat   every pixel outside the two classes below. Passes with zero pixels
         whose channels differ by more than --tolerance.
  edges  the rounded-corner zones of cards, buttons, the log box and the
         scrollbar thumb, plus every checkbox, radio button and status dot.
         A zone passes when the bounding box of its shape (pixels that differ
         from the zone's background) is the same in both images and every
         differing pixel lies on an edge of the reference shape (its 3x3
         neighbourhood in the reference is not uniform): anti-aliasing only.
  text   every text element and log line in ui-spec.json. Its ink box is
         measured the same way in both images, inside its text region; it
         passes when left edge and bottom (the baseline, or the descenders
         where the text has them) are within 1 px and the right edge within
         2 px. Glyph pixels themselves are free to differ.

Text regions come from ui-spec.json (states.<state>.text_regions, ink grown by
2 px); edge zones are derived from the element rects. It writes
target/ui-diff/<state>.png: the reference dimmed, text regions outlined in
dark cyan, edge zones in dark magenta, flat differences in red, non-edge
differences inside zones in orange, and for each failing text element its
reference ink box in green and the snapshot's in yellow.

Exit status is non-zero when a file is missing, sizes differ, or any state
fails any class.
"""

from __future__ import annotations

import argparse
import json
import math
import shutil
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

REPO = Path(__file__).resolve().parent.parent
SPEC = REPO / "docs" / "rewrite" / "ui-spec.json"
FIXTURES = REPO / "docs" / "rewrite" / "fixtures.json"
REF_DIR = REPO / "docs" / "rewrite" / "reference"
NEW_DIR = REPO / "target" / "ui-snapshots"
OUT_DIR = REPO / "target" / "ui-diff"

FLAT_COLOR = (255, 40, 60)
EDGE_COLOR = (255, 140, 0)
REGION_COLOR = (0, 90, 110)
ZONE_COLOR = (110, 0, 110)
REF_INK_COLOR = (0, 230, 0)
NEW_INK_COLOR = (255, 230, 0)
MAX_ATTRIBUTED = 50_000  # pixels; beyond this, attribution samples

WINDOW_BG = "#242424"
CARD_BG = "#2d2d30"
INK_THRESHOLD = 48  # max channel distance from the background that counts as ink
TEXT_SEARCH_X = 3  # px the text window extends left and right of its region
TEXT_TOL_LEFT, TEXT_TOL_BOTTOM, TEXT_TOL_RIGHT = 1, 1, 2


def rgb(hex_color: str) -> tuple[int, int, int]:
    h = hex_color.lstrip("#")
    return int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)


def dist(a, b) -> int:
    return max(abs(a[0] - b[0]), abs(a[1] - b[1]), abs(a[2] - b[2]))


def clip(box, size):
    """(x0, y0, x1, y1) exclusive, clipped to the image."""
    x0, y0, x1, y1 = box
    return max(0, x0), max(0, y0), min(size[0], x1), min(size[1], y1)


def rect_box(r: dict, grow: int = 0):
    return r["x"] - grow, r["y"] - grow, r["x"] + r["w"] + grow, r["y"] + r["h"] + grow


def count_on(mask: Image.Image) -> int:
    hist = mask.histogram()
    return sum(hist) - hist[0]


# --------------------------------------------------------------------------------------- zones


def even_floor(r: dict) -> dict:
    """CustomTkinter paints a canvas floored to even width and height."""
    return {"x": r["x"], "y": r["y"], "w": r["w"] & ~1, "h": r["h"] & ~1}


def corner_zones(name: str, painted: dict, radius: float, bg: str) -> list[dict]:
    """Four square zones over the corners, reaching 1 px outside the painted box."""
    s = math.ceil(radius) + 2
    x0, y0 = painted["x"] - 1, painted["y"] - 1
    x1, y1 = painted["x"] + painted["w"] + 1 - s, painted["y"] + painted["h"] + 1 - s
    return [
        {"name": f"{name}:{c}", "box": (x, y, x + s, y + s), "bg": bg}
        for c, x, y in (("tl", x0, y0), ("tr", x1, y0), ("bl", x0, y1), ("br", x1, y1))
    ]


def edge_zones(st: dict) -> list[dict]:
    """Every zone of the edges class for one state, from the element rects in ui-spec.json."""
    zones = []
    for name, e in st["elements"].items():
        rect = e.get("rect")
        if not rect:
            continue
        leaf = name.rsplit(".", 1)[-1]
        if leaf == "card" or name == "log.box":
            zones += corner_zones(name, even_floor(rect), 6, WINDOW_BG)
        elif name == "apps.select_all":
            zones += corner_zones(name, even_floor(rect), 6, WINDOW_BG)
        elif leaf == "browse":
            zones += corner_zones(name, even_floor(rect), 6, CARD_BG)
        elif name in ("buttons.launch", "buttons.close"):
            zones += corner_zones(name, even_floor(rect), 12, WINDOW_BG)
        elif leaf == "box" and (name.endswith(".checkbox.box") or name.endswith(".radio.box")):
            zones.append({"name": name, "box": rect_box(rect, 1), "bg": CARD_BG, "spec_ink": e.get("ink")})
        elif leaf == "dot" and "ink" in e:
            zones.append({"name": name, "box": rect_box(e["ink"], 2), "bg": CARD_BG, "spec_ink": e["ink"]})
        elif name == "log.scrollbar" and "thumb_ink" in e:
            t = e["thumb_ink"]
            end = t["w"] // 2 + 2
            zones.append({"name": f"{name}:top", "box": (t["x"] - 1, t["y"] - 1, t["x"] + t["w"] + 1, t["y"] + end), "bg": CARD_BG})
            zones.append({"name": f"{name}:bottom", "box": (t["x"] - 1, t["y"] + t["h"] - end, t["x"] + t["w"] + 1, t["y"] + t["h"] + 1), "bg": CARD_BG})
    return zones


def shape_bbox(img: Image.Image, box, bg, tol: int) -> tuple | None:
    """Bounding box (x0, y0, x1, y1 exclusive) of pixels in box that differ from bg by more than tol."""
    x0, y0, x1, y1 = box
    px = img.load()
    found = None
    for y in range(y0, y1):
        for x in range(x0, x1):
            if dist(px[x, y], bg) > tol:
                if found is None:
                    found = [x, y, x + 1, y + 1]
                else:
                    found[0], found[1] = min(found[0], x), min(found[1], y)
                    found[2], found[3] = max(found[2], x + 1), max(found[3], y + 1)
    return tuple(found) if found else None


def check_zone(zone: dict, ref: Image.Image, new: Image.Image, tol: int) -> dict:
    box = clip(zone["box"], ref.size)
    bg = rgb(zone["bg"])
    rp, np_ = ref.load(), new.load()
    non_edge = []
    for y in range(box[1], box[3]):
        for x in range(box[0], box[2]):
            if dist(rp[x, y], np_[x, y]) <= tol:
                continue
            c = rp[x, y]
            edge = any(
                dist(rp[xx, yy], c) > tol
                for yy in range(max(0, y - 1), min(ref.height, y + 2))
                for xx in range(max(0, x - 1), min(ref.width, x + 2))
            )
            if not edge:
                non_edge.append((x, y))
    rb, nb = shape_bbox(ref, box, bg, tol), shape_bbox(new, box, bg, tol)
    res = {"name": zone["name"], "box": box, "non_edge": non_edge, "ref_bbox": rb, "new_bbox": nb}
    res["ok"] = not non_edge and rb == nb
    return res


# ---------------------------------------------------------------------------------------- text


def text_items(st: dict) -> list[dict]:
    """Every text element and visible log line with ink: name, region, background."""
    regions = {r["name"]: r for r in st["text_regions"] if r["kind"] != "glyph-dot"}
    items = []
    for name, t in st["text"].items():
        if name in regions and t.get("ink"):
            items.append({"name": name, "region": regions[name], "bg": t["background"]})
    for line in st["log"]["lines"]:
        name = f"log.line[{line['index']}]"
        if name in regions and line.get("ink"):
            items.append({"name": name, "region": regions[name], "bg": st["log"]["options"]["bg"]})
    return items


def ink_box(img: Image.Image, window, bg) -> tuple | None:
    """Ink bounding box (x0, y0, x1, y1 exclusive) inside window: pixels far from bg."""
    x0, y0, x1, y1 = window
    px = img.load()
    found = None
    for y in range(y0, y1):
        for x in range(x0, x1):
            if dist(px[x, y], bg) > INK_THRESHOLD:
                if found is None:
                    found = [x, y, x + 1, y + 1]
                else:
                    found[0], found[1] = min(found[0], x), min(found[1], y)
                    found[2], found[3] = max(found[2], x + 1), max(found[3], y + 1)
    return tuple(found) if found else None


def check_text(item: dict, ref: Image.Image, new: Image.Image) -> dict:
    r = item["region"]
    window = clip((r["x"] - TEXT_SEARCH_X, r["y"], r["x"] + r["w"] + TEXT_SEARCH_X, r["y"] + r["h"]), ref.size)
    bg = rgb(item["bg"])
    rb, nb = ink_box(ref, window, bg), ink_box(new, window, bg)
    res = {"name": item["name"], "ref": rb, "new": nb}
    if rb is None:  # nothing measurable in the reference: nothing to hold the render to
        res["ok"] = True
        return res
    if nb is None:
        res["ok"], res["why"] = False, "no ink"
        return res
    dl, db, dr = nb[0] - rb[0], nb[3] - rb[3], nb[2] - rb[2]
    res.update(dl=dl, db=db, dr=dr)
    res["ok"] = abs(dl) <= TEXT_TOL_LEFT and abs(db) <= TEXT_TOL_BOTTOM and abs(dr) <= TEXT_TOL_RIGHT
    if not res["ok"]:
        res["why"] = f"left {dl:+d}, baseline {db:+d}, right {dr:+d}"
    return res


# ----------------------------------------------------------------------------------------- flat


def attribute(mask: Image.Image, elements: dict) -> list[tuple[str, int, tuple]]:
    """Group the 'on' pixels of mask by the smallest ui-spec element containing them."""
    rects = []
    for name, e in elements.items():
        r = e.get("visible") or e["rect"]
        if r:
            rects.append((r["w"] * r["h"], name, r["x"], r["y"], r["x"] + r["w"], r["y"] + r["h"]))
    rects.sort()
    w = mask.width
    on = [i for i, v in enumerate(mask.getdata()) if v]
    step = max(1, len(on) // MAX_ATTRIBUTED)
    groups: dict[str, list] = {}
    for i in on[::step]:
        x, y = i % w, i // w
        owner = next((n for _, n, x0, y0, x1, y1 in rects if x0 <= x < x1 and y0 <= y < y1), "(window)")
        g = groups.setdefault(owner, [0, x, y, x, y])
        g[0] += step
        g[1], g[2], g[3], g[4] = min(g[1], x), min(g[2], y), max(g[3], x), max(g[4], y)
    out = [(n, g[0], (g[1], g[2], g[3] - g[1] + 1, g[4] - g[2] + 1)) for n, g in groups.items()]
    return sorted(out, key=lambda t: -t[1])


# ---------------------------------------------------------------------------------------- state


def diff_state(state: str, spec: dict, ref_dir: Path, new_dir: Path, out_dir: Path, tol: int) -> dict:
    """Compare one state. Returns a result dict; result['ok'] is False on any failure."""
    ref_path, new_path = ref_dir / f"{state}.png", new_dir / f"{state}.png"
    res = {"state": state, "ok": False}
    for p in (ref_path, new_path):
        if not p.exists():
            res["error"] = f"missing {p}"
            return res
    ref = Image.open(ref_path).convert("RGB")
    new = Image.open(new_path).convert("RGB")
    if ref.size != new.size:
        res["error"] = f"size differs: reference {ref.size[0]}x{ref.size[1]}, snapshot {new.size[0]}x{new.size[1]}"
        return res
    if state not in spec["states"]:
        res["error"] = f"state {state} not in {SPEC.name}"
        return res
    st = spec["states"][state]

    # Masks of the text and edges classes; flat is everything else.
    excluded = Image.new("L", ref.size, 0)
    draw = ImageDraw.Draw(excluded)
    for r in st["text_regions"]:
        if r["kind"] != "glyph-dot":
            draw.rectangle((r["x"], r["y"], r["x"] + r["w"] - 1, r["y"] + r["h"] - 1), fill=255)
    zones = edge_zones(st)
    for z in zones:
        x0, y0, x1, y1 = z["box"]
        draw.rectangle((x0, y0, x1 - 1, y1 - 1), fill=255)

    r, g, b = ImageChops.difference(ref, new).split()
    delta = ImageChops.lighter(ImageChops.lighter(r, g), b)
    flat = ImageChops.multiply(delta.point(lambda v: 255 if v > tol else 0), ImageChops.invert(excluded))
    n_flat = count_on(flat)
    res["flat"] = {"px": n_flat, "bbox": flat.getbbox(), "elements": attribute(flat, st["elements"]) if n_flat else [],
                   "ok": n_flat == 0}

    zone_results = [check_zone(z, ref, new, tol) for z in zones]
    res["edges"] = {"zones": len(zone_results), "failed": [z for z in zone_results if not z["ok"]]}
    res["edges"]["ok"] = not res["edges"]["failed"]

    text_results = [check_text(t, ref, new) for t in text_items(st)]
    res["text"] = {"items": len(text_results), "failed": [t for t in text_results if not t["ok"]]}
    res["text"]["ok"] = not res["text"]["failed"]

    out_dir.mkdir(parents=True, exist_ok=True)
    img = ref.point(lambda v: v * 30 // 100)
    d = ImageDraw.Draw(img)
    for reg in st["text_regions"]:
        if reg["kind"] != "glyph-dot":
            d.rectangle((reg["x"], reg["y"], reg["x"] + reg["w"] - 1, reg["y"] + reg["h"] - 1), outline=REGION_COLOR)
    for z in zones:
        x0, y0, x1, y1 = z["box"]
        d.rectangle((x0, y0, x1 - 1, y1 - 1), outline=ZONE_COLOR)
    img.paste(FLAT_COLOR, mask=flat)
    for z in res["edges"]["failed"]:
        for x, y in z["non_edge"]:
            img.putpixel((x, y), EDGE_COLOR)
        if z["ref_bbox"] != z["new_bbox"]:
            x0, y0, x1, y1 = z["box"]
            d.rectangle((x0, y0, x1 - 1, y1 - 1), outline=EDGE_COLOR)
    for t in res["text"]["failed"]:
        for box, color in ((t["ref"], REF_INK_COLOR), (t.get("new"), NEW_INK_COLOR)):
            if box:
                d.rectangle((box[0], box[1], box[2] - 1, box[3] - 1), outline=color)
    res["diff_image"] = out_dir / f"{state}.png"
    img.save(res["diff_image"])
    res["ok"] = res["flat"]["ok"] and res["edges"]["ok"] and res["text"]["ok"]
    return res


def report(res: dict, detail: int = 10) -> None:
    if "error" in res:
        print(f"{res['state']:<14} ERROR  {res['error']}")
        return
    flag = lambda ok: "ok  " if ok else "FAIL"  # noqa: E731
    f, e, t = res["flat"], res["edges"], res["text"]
    print(f"{res['state']:<14} flat {flag(f['ok'])} {f['px']:>6} px   "
          f"edges {flag(e['ok'])} {len(e['failed']):>3}/{e['zones']:<3} zones   "
          f"text {flag(t['ok'])} {len(t['failed']):>3}/{t['items']:<3} elements   -> {res['diff_image']}")
    for name, n, (x, y, w, h) in f["elements"][:detail]:
        print(f"{'':<6}flat  {n:>6} px in {name}  bbox x={x} y={y} w={w} h={h}")
    for z in e["failed"][:detail]:
        why = []
        if z["non_edge"]:
            why.append(f"{len(z['non_edge'])} px off the edge")
        if z["ref_bbox"] != z["new_bbox"]:
            why.append(f"shape bbox {z['new_bbox']} != reference {z['ref_bbox']}")
        print(f"{'':<6}edges {z['name']}: {'; '.join(why)}")
    for x in t["failed"][:detail]:
        print(f"{'':<6}text  {x['name']}: {x['why']}  (reference ink {x['ref']}, snapshot {x.get('new')})")
    extra = max(0, len(f["elements"]) - detail) + max(0, len(e["failed"]) - detail) + max(0, len(t["failed"]) - detail)
    if extra:
        print(f"{'':<6}... {extra} more")


def states_from_fixtures() -> list[str]:
    return list(json.loads(FIXTURES.read_text(encoding="utf-8")))


def self_test(spec: dict, tol: int) -> int:
    """Identical copy passes every class; each planted change fails the class it belongs to."""
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        same, out = tmp / "same", tmp / "out"
        shutil.copytree(REF_DIR, same)
        for state in states_from_fixtures():
            res = diff_state(state, spec, REF_DIR, same, out, tol)
            if not res["ok"] or res["flat"]["px"] or res["edges"]["failed"] or res["text"]["failed"]:
                failures.append(f"{state}: identical copy did not pass: {res.get('error', '')}")

        state = "idle"
        st = spec["states"][state]
        el, tx = st["elements"], st["text"]

        def altered(edit) -> dict:
            d = tmp / "altered"
            if d.exists():
                shutil.rmtree(d)
            shutil.copytree(REF_DIR, d)
            img = Image.open(d / f"{state}.png").convert("RGB")
            edit(img)
            img.save(d / f"{state}.png")
            return diff_state(state, spec, REF_DIR, d, out, tol)

        # flat: a 20x10 block inside the Fanatec card, right of its text.
        card, name = el["apps.fanatec.card"]["rect"], tx["apps.fanatec.name"]["ink"]
        bx, by = name["x"] + name["w"] + 40, card["y"] + 20
        res = altered(lambda img: ImageDraw.Draw(img).rectangle((bx, by, bx + 19, by + 9), fill=(200, 0, 200)))
        print("self-test, planted block in idle:")
        report(res)
        owners = {n for n, _, _ in res["flat"]["elements"]}
        if res["flat"]["px"] != 200:
            failures.append(f"planted block: expected 200 px flat, got {res['flat']['px']}")
        if res["flat"]["bbox"] != (bx, by, bx + 20, by + 10):
            failures.append(f"planted block: bbox {res['flat']['bbox']} != {(bx, by, bx + 20, by + 10)}")
        if not owners or not all(o.startswith("apps.fanatec") for o in owners):
            failures.append(f"planted block attributed to {owners}, expected apps.fanatec.*")
        if res["ok"] or res["edges"]["failed"] or res["text"]["failed"]:
            failures.append("planted block: expected only flat to fail")

        # text: the Fanatec name moved 2 px right, which stays inside its region on the left.
        def shift_name(img):
            box = (name["x"] - 1, name["y"] - 1, name["x"] + name["w"] + 1, name["y"] + name["h"] + 1)
            patch = img.crop(box)
            img.paste(rgb(tx["apps.fanatec.name"]["background"]), box)
            img.paste(patch, (box[0] + 2, box[1]))
        res = altered(shift_name)
        print("self-test, Fanatec name moved 2 px right:")
        report(res)
        failed = {t["name"] for t in res["text"]["failed"]}
        if failed != {"apps.fanatec.name"}:
            failures.append(f"moved text: expected text to fail for apps.fanatec.name only, got {failed}")

        # edges: the Fanatec dot painted over with the card colour, and a checkbox's core recoloured.
        dot, check = el["apps.fanatec.dot"]["ink"], el["apps.crew_chief_v4.checkbox.box"]["rect"]
        def edit_shapes(img):
            ImageDraw.Draw(img).rectangle((dot["x"], dot["y"], dot["x"] + dot["w"] - 1, dot["y"] + dot["h"] - 1), fill=rgb(CARD_BG))
            ImageDraw.Draw(img).rectangle((check["x"] + 9, check["y"] + 2, check["x"] + 14, check["y"] + 4), fill=(255, 0, 0))
        res = altered(edit_shapes)
        print("self-test, Fanatec dot erased and a checkbox recoloured:")
        report(res)
        failed = {z["name"] for z in res["edges"]["failed"]}
        if failed != {"apps.fanatec.dot", "apps.crew_chief_v4.checkbox.box"}:
            failures.append(f"edited shapes: expected those two zones to fail, got {failed}")
        if res["flat"]["px"]:
            failures.append(f"edited shapes: {res['flat']['px']} px leaked into flat")

        # A size mismatch and a missing file must fail.
        d = tmp / "broken"
        shutil.copytree(REF_DIR, d)
        Image.new("RGB", (100, 100)).save(d / "mixed.png")
        (d / "log-full.png").unlink()
        for st_name, want in (("mixed", "size differs"), ("log-full", "missing")):
            r = diff_state(st_name, spec, REF_DIR, d, out, tol)
            if r["ok"] or want not in r.get("error", ""):
                failures.append(f"{st_name}: expected '{want}' error, got {r}")
    for f in failures:
        print("SELF-TEST FAIL:", f)
    print("self-test passed" if not failures else f"self-test failed ({len(failures)})")
    return 1 if failures else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0].strip())
    ap.add_argument("--state", help="compare one state only")
    ap.add_argument("--tolerance", type=int, default=8, help="per-channel tolerance for flat and edges (default 8)")
    ap.add_argument("--detail", type=int, default=10, help="failures listed per class (default 10)")
    ap.add_argument("--ref-dir", type=Path, default=REF_DIR)
    ap.add_argument("--new-dir", type=Path, default=NEW_DIR)
    ap.add_argument("--out-dir", type=Path, default=OUT_DIR)
    ap.add_argument("--self-test", action="store_true", help="check the tool against the reference set itself")
    args = ap.parse_args()

    spec = json.loads(SPEC.read_text(encoding="utf-8"))
    if args.self_test:
        return self_test(spec, args.tolerance)

    states = [args.state] if args.state else states_from_fixtures()
    print(f"reference {args.ref_dir}  vs  snapshots {args.new_dir}  (tolerance {args.tolerance})")
    failed = 0
    for state in states:
        res = diff_state(state, spec, args.ref_dir, args.new_dir, args.out_dir, args.tolerance)
        report(res, args.detail)
        failed += not res["ok"]
    print(f"{len(states) - failed}/{len(states)} states pass flat, edges and text")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
