#!/usr/bin/env python3
"""Compare the local-font public run example with an explicit HarfBuzz CLI."""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hb-shape", default="hb-shape")
    args = parser.parse_args()
    font = ROOT / "crates/ravet-surface/tests/fixtures/text/NotoSansArabic.ttf"
    subprocess.run([args.hb_shape, "--version"], check=True)
    environment = dict(os.environ)
    environment.setdefault("CARGO_BUILD_JOBS", "2")
    subprocess.run(["cargo", "build", "-p", "ravet-surface", "--features", "text", "--example", "shape-text", "--locked"], cwd=ROOT, env=environment, check=True)
    example = ROOT / "target/debug/examples/shape-text"
    for text, direction, override in [("AV", "ltr", None), ("AV", "ltr", "kern=0"), ("ffi", "ltr", None), ("Gain 12", "ltr", None), ("سلام", "rtl", None), ("بَ", "rtl", None), ("العَرَبِيَّة", "rtl", None), ("ب\u200dب", "rtl", None)]:
        lines = subprocess.check_output([str(example), str(font), text, direction] + ([override] if override else []), text=True).splitlines()
        units, advance_x, advance_y = map(float, lines[0].split())
        actual = [list(map(float, line.split())) for line in lines[1:]]
        reference = json.loads(subprocess.check_output([args.hb_shape, "--no-glyph-names", "--utf8-clusters", f"--direction={direction}", "--script=arab" if direction == "rtl" else "--script=latn", "--language=ar" if direction == "rtl" else "--language=en", "--cluster-level=0", "-O", "json", str(font), text] + ([f"--features={override}"] if override else []), text=True))
        if len(actual) != len(reference):
            raise RuntimeError(f"glyph count mismatch for {text!r}: {len(actual)} != {len(reference)}")
        pen_x = pen_y = 0.0
        scale = 24.0 / units
        for glyph, expected in zip(actual, reference):
            target = [expected["g"], expected["cl"], pen_x + expected["dx"] * scale, pen_y + expected["dy"] * scale, expected["ax"] * scale, expected["ay"] * scale]
            if glyph[:2] != target[:2] or any(not math.isclose(a, b, abs_tol=1e-9, rel_tol=1e-12) for a, b in zip(glyph[2:], target[2:])):
                raise RuntimeError(f"glyph mismatch for {text!r}: {glyph} != {target}")
            pen_x += expected["ax"] * scale
            pen_y += expected["ay"] * scale
        if not math.isclose(advance_x, pen_x, abs_tol=1e-9) or not math.isclose(advance_y, pen_y, abs_tol=1e-9):
            raise RuntimeError(f"advance mismatch for {text!r}")
        print(f"PASS independent HarfBuzz comparison: {text!r}, {len(actual)} glyphs, {advance_x:g} advance")

if __name__ == "__main__":
    main()
