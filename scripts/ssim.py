#!/usr/bin/env python3
"""Compares two PNGs with a block SSIM (the metric blade's snapshot tests use).

Usage: ssim.py <golden.png> <actual.png> [threshold]
Exit code 0 when the SSIM is at least the threshold (default 0.95).
"""
import sys

import numpy as np
from PIL import Image

C1 = 6.5025  # (0.01 * 255)^2
C2 = 58.5225  # (0.03 * 255)^2
BLOCK = 8


def load(path: str) -> np.ndarray:
    return np.asarray(Image.open(path).convert("L"), dtype=np.float64)


def ssim(a: np.ndarray, b: np.ndarray) -> float:
    if a.shape != b.shape:
        return -1.0
    height, width = a.shape
    blocks_y = height // BLOCK
    blocks_x = width // BLOCK
    if blocks_x == 0 or blocks_y == 0:
        return 1.0
    values = []
    for by in range(blocks_y):
        for bx in range(blocks_x):
            y0 = by * BLOCK
            x0 = bx * BLOCK
            x = a[y0 : y0 + BLOCK, x0 : x0 + BLOCK]
            y = b[y0 : y0 + BLOCK, x0 : x0 + BLOCK]
            mean_x = x.mean()
            mean_y = y.mean()
            var_x = x.var()
            var_y = y.var()
            covariance = ((x - mean_x) * (y - mean_y)).mean()
            values.append(
                ((2 * mean_x * mean_y + C1) * (2 * covariance + C2))
                / ((mean_x * mean_x + mean_y * mean_y + C1) * (var_x + var_y + C2))
            )
    return float(np.mean(values))


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: ssim.py <golden.png> <actual.png> [threshold]")
        return 2
    golden = load(sys.argv[1])
    actual = load(sys.argv[2])
    threshold = float(sys.argv[3]) if len(sys.argv) > 3 else 0.95
    value = ssim(golden, actual)
    print(f"SSIM {value:.6f} ({sys.argv[2]})")
    return 0 if value >= threshold else 1


if __name__ == "__main__":
    raise SystemExit(main())
