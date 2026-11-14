#!/usr/bin/env python3
"""Builds assets/xljs-file.png (transparent) and assets/xljs-file.ico, the
icon of .xljs files (the exe resource 2, see src/cli/xljs.rc and
src/cli/assoc.rs), from the original picture of the sheet of paper with the
xljs logo on a flat white background.

usage: python3 tools/make_file_icon.py <original.png>
       python3 tools/make_file_icon.py            (only the .ico, from assets/xljs-file.png)
"""
import os
import sys
from collections import deque

import numpy as np
from PIL import Image, ImageFilter

SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
ROOT = os.path.join(os.path.dirname(__file__), "..")
PNG = os.path.join(ROOT, "assets", "xljs-file.png")
ICO = os.path.join(ROOT, "assets", "xljs-file.ico")


def transparent(src):
    """The picture with the background around the paper made transparent,
    cropped to the paper and padded to a square."""
    im = np.asarray(Image.open(src).convert("RGB")).astype(np.float32)
    h, w, _ = im.shape
    bg = np.median(np.concatenate([im[:8, :8].reshape(-1, 3), im[:8, -8:].reshape(-1, 3),
                                   im[-8:, :8].reshape(-1, 3), im[-8:, -8:].reshape(-1, 3)]), axis=0)
    dist = np.sqrt(((im - bg) ** 2).sum(axis=2))

    # Background = pixels close to the background color connected to the
    # border of the picture (the paper's outline keeps the fill out of the
    # paper, whose white is a little darker anyway).
    near = dist < 5
    seen = np.zeros((h, w), bool)
    q = deque()
    for x in range(w):
        for y in (0, h - 1):
            if near[y, x]:
                seen[y, x] = True
                q.append((y, x))
    for y in range(h):
        for x in (0, w - 1):
            if near[y, x] and not seen[y, x]:
                seen[y, x] = True
                q.append((y, x))
    while q:
        y, x = q.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < h and 0 <= nx < w and near[ny, nx] and not seen[ny, nx]:
                seen[ny, nx] = True
                q.append((ny, nx))

    # Soft alpha in the anti-aliased band outside the outline (and the faint
    # shadow): how far a pixel is from the background towards the outline's
    # gray.
    band = np.asarray(Image.fromarray((seen * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(7))) > 0
    inside = ~seen
    # Fill the paper: everything enclosed by the outline is opaque.
    alpha = np.ones((h, w), np.float32)
    soft = np.clip(dist / 80, 0, 1)
    edge = band & inside
    # Only pixels touching the background are softened; the paper's own
    # white next to the outline stays opaque.
    outer = edge & (dist < 60) & (np.asarray(Image.fromarray((seen * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(3))) > 0)
    alpha[outer] = soft[outer]
    alpha[seen] = 0

    a = alpha[..., None]
    rgb = np.where(a > 0.01, (im - (1 - a) * bg) / np.maximum(a, 0.01), 0)
    img = Image.fromarray(np.dstack([np.clip(rgb, 0, 255), alpha * 255]).astype(np.uint8), "RGBA")

    bbox = img.getchannel("A").point(lambda v: 255 if v > 8 else 0).getbbox()
    img = img.crop(bbox)
    side = max(img.size)
    out = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    out.alpha_composite(img, ((side - img.width) // 2, (side - img.height) // 2))
    return out


def main():
    if len(sys.argv) > 1:
        transparent(sys.argv[1]).resize((1024, 1024), Image.LANCZOS).save(PNG)
        print(PNG)
    img = Image.open(PNG).convert("RGBA")
    frames = [img.resize((s, s), Image.LANCZOS) for s in SIZES]
    frames[-1].save(ICO, sizes=[(s, s) for s in SIZES], append_images=frames[:-1])
    print(ICO)


if __name__ == "__main__":
    main()
