#!/usr/bin/env python3
"""Compare captured screenshots against the golden set (S3).

Usage:
  scripts/golden_compare.py GOLDEN.png CAPTURE.png
  scripts/golden_compare.py GOLDENS_DIR CAPTURE_DIR [--masks DIR] [--diff-dir DIR]

A pair passes when BOTH hold (values in 8-bit channel units):
  - at most 1.5% of pixels have any channel differing by more than 8
  - the RMS error over every channel (after the +/-2/255 deadzone) is <= 3.0

Per-channel differences of 2 or less count as zero (encode noise).
Optional foliage masks (DIR/name.png, same size as the golden) hold the
regions to ignore: black pixels are skipped, everything else is compared.
A failing pair writes an amplified diff heatmap into --diff-dir (default:
printed but not written unless --diff-dir is given).

Missing, unreadable, size-mismatched or blank captures fail. Exit code is
non-zero when any pair fails.
"""

import argparse
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageStat

# Tolerances (8-bit channel units).
DEADZONE = 2
PIXEL_FAIL = 8  # a "different" pixel has some channel gap over this
MAX_DIFFERENT_FRACTION = 0.015
MAX_RMSE = 3.0

# A capture with less than this channel stddev is "blank" (a failed load, an
# all-black frame, a window that never drew).
MIN_STDDEV = 1.0


def fail(message):
    print(f"fail: {message}")
    return 1


def load(path):
    try:
        img = Image.open(path)
        img.load()
        return img.convert("RGB")
    except Exception as e:  # noqa: BLE001 - any open/decode failure fails the pair
        return e


def blank(img):
    return all(s < MIN_STDDEV for s in ImageStat.Stat(img).stddev)


def compare(golden_path, capture_path, mask_path, diff_dir):
    name = Path(capture_path).stem
    expected = load(golden_path)
    if isinstance(expected, Exception):
        return fail(f"{name}: golden unreadable: {expected}")
    actual = load(capture_path)
    if isinstance(actual, Exception):
        return fail(f"{name}: capture unreadable: {actual}")
    if expected.size != actual.size:
        return fail(f"{name}: size mismatch {expected.size} vs {actual.size}")
    if blank(actual):
        return fail(f"{name}: capture is blank")

    red, green, blue = ImageChops.difference(expected, actual).split()
    gap = ImageChops.lighter(ImageChops.lighter(red, green), blue)

    mask_data = None
    if mask_path and Path(mask_path).exists():
        mask = load(mask_path)
        if isinstance(mask, Exception) or mask.size != expected.size:
            return fail(f"{name}: mask unreadable or size-mismatched")
        mask_data = mask.convert("L").tobytes()

    r_data, g_data, b_data = red.tobytes(), green.tobytes(), blue.tobytes()
    gap_data = gap.tobytes()

    compared = 0
    different = 0
    square_sum = 0
    for i, mx in enumerate(gap_data):
        if mask_data is not None and mask_data[i] == 0:
            continue
        compared += 1
        if mx > PIXEL_FAIL:
            different += 1
        for ch in (r_data[i], g_data[i], b_data[i]):
            d = ch - DEADZONE if ch > DEADZONE else 0
            square_sum += d * d
    if compared == 0:
        return fail(f"{name}: mask ignores every pixel")

    rmse = (square_sum / (3 * compared)) ** 0.5
    fraction = different / compared

    if fraction > MAX_DIFFERENT_FRACTION or rmse > MAX_RMSE:
        if diff_dir:
            out = Path(diff_dir) / f"{name}.diff.png"
            out.parent.mkdir(parents=True, exist_ok=True)
            gap.point(lambda p: min(255, p * 8)).save(out)
            print(f"     diff heatmap: {out}")
        return fail(
            f"{name}: {fraction:.2%} of pixels differ (>8/255, limit "
            f"{MAX_DIFFERENT_FRACTION:.1%}), rmse {rmse:.2f} (limit {MAX_RMSE})"
        )
    print(f"ok:   {name}: {fraction:.2%} differ, rmse {rmse:.2f}")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("golden", help="golden file or directory")
    parser.add_argument("capture", help="capture file or directory")
    parser.add_argument("--masks", help="directory of <name>.png ignore-masks")
    parser.add_argument("--diff-dir", help="where to write diff heatmaps")
    args = parser.parse_args()

    golden, capture = Path(args.golden), Path(args.capture)
    if golden.is_dir() and capture.is_dir():
        names = sorted(
            p.name for p in capture.glob("*.png") if not p.name.endswith(".diff.png")
        )
        if not names:
            return fail(f"{capture} has no captures")
        status = 0
        for name in names:
            mask = Path(args.masks) / name if args.masks else None
            if not (golden / name).exists():
                status |= fail(f"{Path(name).stem}: no golden at {golden / name}")
                continue
            status |= compare(golden / name, capture / name, mask, args.diff_dir)
        return status
    if golden.is_file() and capture.is_file():
        mask = Path(args.masks) if args.masks else None
        return compare(golden, capture, mask, args.diff_dir)
    return fail("both arguments must be files, or both directories")


if __name__ == "__main__":
    sys.exit(main())
