"""
Compare the Rust UI snapshots with the Python reference screenshots.

    python tools/ui_diff.py                  # every state in docs/rewrite/fixtures.json
    python tools/ui_diff.py --state mixed    # one state
    python tools/ui_diff.py --self-test      # prove the tool reports zero / a planted change

For each state it compares docs/rewrite/reference/<state>.png with
target/ui-snapshots/<state>.png and prints the number and share of differing
pixels outside and inside text regions. Text regions come from
docs/rewrite/ui-spec.json (states.<state>.text_regions). Outside text a pixel
differs when any channel differs by more than --tolerance; inside text, by more
than --text-tolerance. It writes target/ui-diff/<state>.png: the reference
dimmed, text regions outlined in dark cyan, differences outside text in red and
inside text in yellow, and lists the elements (from ui-spec.json) that hold the
differing pixels outside text.

Exit status is non-zero when a file is missing, sizes differ, or any state has
differing pixels outside text regions.
"""

from __future__ import annotations

import argparse
import json
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

OUTSIDE_COLOR = (255, 40, 60)
INSIDE_COLOR = (255, 210, 0)
REGION_COLOR = (0, 90, 110)
MAX_ATTRIBUTED = 50_000  # pixels; beyond this, attribution samples


def text_mask(size, regions) -> Image.Image:
    mask = Image.new("L", size, 0)
    draw = ImageDraw.Draw(mask)
    for r in regions:
        draw.rectangle((r["x"], r["y"], r["x"] + r["w"] - 1, r["y"] + r["h"] - 1), fill=255)
    return mask


def count_on(mask: Image.Image) -> int:
    hist = mask.histogram()
    return sum(hist) - hist[0]


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


def diff_state(state: str, spec: dict, ref_dir: Path, new_dir: Path, out_dir: Path,
               tol: int, text_tol: int, quiet: bool = False) -> dict:
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
    regions = st["text_regions"]
    r, g, b = ImageChops.difference(ref, new).split()
    delta = ImageChops.lighter(ImageChops.lighter(r, g), b)  # max channel difference
    tmask = text_mask(ref.size, regions)
    outside = ImageChops.multiply(delta.point(lambda v: 255 if v > tol else 0), ImageChops.invert(tmask))
    inside = ImageChops.multiply(delta.point(lambda v: 255 if v > text_tol else 0), tmask)

    text_px = count_on(tmask)
    other_px = ref.width * ref.height - text_px
    n_out, n_in = count_on(outside), count_on(inside)
    res.update({
        "outside": n_out, "outside_pct": 100 * n_out / other_px if other_px else 0.0,
        "inside": n_in, "inside_pct": 100 * n_in / text_px if text_px else 0.0,
        "outside_bbox": outside.getbbox(), "inside_bbox": inside.getbbox(),
        "elements": attribute(outside, st["elements"]) if n_out else [],
    })

    out_dir.mkdir(parents=True, exist_ok=True)
    img = ref.point(lambda v: v * 30 // 100)
    draw = ImageDraw.Draw(img)
    for reg in regions:
        draw.rectangle((reg["x"], reg["y"], reg["x"] + reg["w"] - 1, reg["y"] + reg["h"] - 1), outline=REGION_COLOR)
    img.paste(INSIDE_COLOR, mask=inside)
    img.paste(OUTSIDE_COLOR, mask=outside)
    res["diff_image"] = out_dir / f"{state}.png"
    img.save(res["diff_image"])
    res["ok"] = n_out == 0
    return res


def report(res: dict) -> None:
    if "error" in res:
        print(f"{res['state']:<14} ERROR  {res['error']}")
        return
    flag = "ok  " if res["ok"] else "FAIL"
    print(f"{res['state']:<14} {flag}  outside text: {res['outside']:>7} px ({res['outside_pct']:.3f}%)"
          f"   inside text: {res['inside']:>6} px ({res['inside_pct']:.2f}%)   -> {res['diff_image']}")
    for name, n, (x, y, w, h) in res["elements"][:10]:
        print(f"{'':<20}{n:>7} px in {name}  bbox x={x} y={y} w={w} h={h}")
    if len(res["elements"]) > 10:
        print(f"{'':<20}... {len(res['elements']) - 10} more elements")


def states_from_fixtures() -> list[str]:
    return list(json.loads(FIXTURES.read_text(encoding="utf-8")))


def self_test(spec: dict, tol: int, text_tol: int) -> int:
    """Identical copy -> zero everywhere; planted block and text change -> found where planted."""
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        same, altered, out = tmp / "same", tmp / "altered", tmp / "out"
        shutil.copytree(REF_DIR, same)
        shutil.copytree(REF_DIR, altered)
        for state in states_from_fixtures():
            res = diff_state(state, spec, REF_DIR, same, out, tol, text_tol)
            if not res["ok"] or res["outside"] or res["inside"]:
                failures.append(f"{state}: identical copy reported {res}")

        # Plant a 20x10 block inside the Fanatec card, right of its text, and recolour a text region.
        state = "idle"
        el = spec["states"][state]["elements"]
        card, name = el["apps.fanatec.card"]["rect"], spec["states"][state]["text"]["apps.fanatec.name"]["ink"]
        bx, by = name["x"] + name["w"] + 40, card["y"] + 20
        img = Image.open(altered / f"{state}.png").convert("RGB")
        draw = ImageDraw.Draw(img)
        draw.rectangle((bx, by, bx + 19, by + 9), fill=(200, 0, 200))
        draw.rectangle((name["x"], name["y"], name["x"] + name["w"] - 1, name["y"] + name["h"] - 1), fill=(0, 255, 0))
        img.save(altered / f"{state}.png")
        res = diff_state(state, spec, REF_DIR, altered, out, tol, text_tol)
        print("self-test, planted change in idle:")
        report(res)
        owners = {n for n, _, _ in res["elements"]}
        if res["outside"] != 200:
            failures.append(f"planted block: expected 200 px outside text, got {res['outside']}")
        if res["outside_bbox"] != (bx, by, bx + 20, by + 10):
            failures.append(f"planted block: bbox {res['outside_bbox']} != {(bx, by, bx + 20, by + 10)}")
        if not owners or not all(o.startswith("apps.fanatec") for o in owners):
            failures.append(f"planted block attributed to {owners}, expected apps.fanatec.*")
        if res["inside"] < name["w"] * name["h"] // 2:
            failures.append(f"recoloured text: only {res['inside']} px reported inside text")
        if res["ok"]:
            failures.append("planted block did not fail the state")

        # A size mismatch and a missing file must fail.
        Image.new("RGB", (100, 100)).save(altered / "mixed.png")
        (altered / "log-full.png").unlink()
        for st, want in (("mixed", "size differs"), ("log-full", "missing")):
            r = diff_state(st, spec, REF_DIR, altered, out, tol, text_tol)
            if r["ok"] or want not in r.get("error", ""):
                failures.append(f"{st}: expected '{want}' error, got {r}")
    for f in failures:
        print("SELF-TEST FAIL:", f)
    print("self-test passed" if not failures else f"self-test failed ({len(failures)})")
    return 1 if failures else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0].strip())
    ap.add_argument("--state", help="compare one state only")
    ap.add_argument("--tolerance", type=int, default=8, help="per-channel tolerance outside text (default 8)")
    ap.add_argument("--text-tolerance", type=int, default=0, help="per-channel tolerance inside text (default 0)")
    ap.add_argument("--ref-dir", type=Path, default=REF_DIR)
    ap.add_argument("--new-dir", type=Path, default=NEW_DIR)
    ap.add_argument("--out-dir", type=Path, default=OUT_DIR)
    ap.add_argument("--self-test", action="store_true", help="check the tool against the reference set itself")
    args = ap.parse_args()

    spec = json.loads(SPEC.read_text(encoding="utf-8"))
    if args.self_test:
        return self_test(spec, args.tolerance, args.text_tolerance)

    states = [args.state] if args.state else states_from_fixtures()
    print(f"reference {args.ref_dir}  vs  snapshots {args.new_dir}  (tolerance {args.tolerance}, "
          f"text tolerance {args.text_tolerance})")
    failed = 0
    for state in states:
        res = diff_state(state, spec, args.ref_dir, args.new_dir, args.out_dir, args.tolerance, args.text_tolerance)
        report(res)
        failed += not res["ok"]
    print(f"{len(states) - failed}/{len(states)} states clean outside text")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
