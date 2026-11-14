#!/usr/bin/env python3
"""Draws assets/xljs-file.ico, the icon of .xljs files (the exe resource 2,
see src/cli/xljs.rc and src/cli/assoc.rs): a sheet of paper with a folded
corner and the xljs logo (assets/xljs.png) on it.

usage: python3 tools/make_file_icon.py
"""
import os

from PIL import Image, ImageDraw, ImageFilter

SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
S = 1024  # drawing size; each icon size is scaled down from it
ROOT = os.path.join(os.path.dirname(__file__), "..")
LOGO = os.path.join(ROOT, "assets", "xljs.png")
OUT = os.path.join(ROOT, "assets", "xljs-file.ico")

PAPER = (255, 255, 255, 255)
EDGE = (150, 158, 170, 255)
FOLD = (222, 227, 234, 255)


def page(small):
    """The paper (wider at small sizes, where the margins would be lost)."""
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    left, right = (120, 904) if small else (168, 856)
    top, bottom = 24, 1000
    fold = 260 if small else 230
    w = 40 if small else 26
    outline = [(left, top), (right - fold, top), (right, top + fold), (right, bottom), (left, bottom)]
    # A soft shadow under the paper.
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).polygon([(x + 10, y + 16) for x, y in outline], fill=(0, 0, 0, 70))
    im.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(14)))
    d = ImageDraw.Draw(im)
    d.polygon(outline, fill=PAPER, outline=EDGE, width=w)
    corner = [(right - fold, top), (right - fold, top + fold), (right, top + fold)]
    d.polygon(corner, fill=FOLD, outline=EDGE, width=w)
    d.line([(right - fold, top), (right, top + fold)], fill=EDGE, width=w)
    return im, (left, top, right, bottom)


def draw(small):
    im, (left, top, right, bottom) = page(small)
    logo = Image.open(LOGO).convert("RGBA")
    # The logo over the lower part of the paper, sticking out to the right
    # at large sizes like an app badge; filling the width at small sizes.
    size = 700 if small else 620
    logo = logo.resize((size, size), Image.LANCZOS)
    x = (S - size) // 2 + (0 if small else 120)
    y = bottom - size + (40 if small else 20)
    im.alpha_composite(logo, (x, y))
    return im


def main():
    frames = []
    for s in SIZES:
        frames.append(draw(s <= 24).resize((s, s), Image.LANCZOS))
    frames[-1].save(OUT, sizes=[(s, s) for s in SIZES], append_images=frames[:-1])
    print(OUT)


if __name__ == "__main__":
    main()
