#!/usr/bin/env python3
"""Draws the installer (NSIS) images in Glyphling style: black background,
dot grid, the LED mascot and the name written in the dot font.

Usage: python3 tools/make_installer_art.py src-tauri/installer/
Writes sidebar.bmp (164x314, welcome and finish pages) and header.bmp (150x57).
"""
import math, os, sys
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(__file__))
from make_font import G  # 5x7 glyph table

BLACK, GRID, OFF, WHITE, MUTE, RED = (0, 0, 0), (28, 28, 28), (38, 38, 38), (242, 242, 242), (138, 138, 138), (215, 25, 33)
SS = 4  # supersampling for round, smooth dots


def dot(d, x, y, r, col):
    d.ellipse([(x - r) * SS, (y - r) * SS, (x + r) * SS, (y + r) * SS], fill=col)


def grid_bg(d, w, h, step=7):
    for y in range(step // 2, h, step):
        for x in range(step // 2, w, step):
            dot(d, x, y, 0.55, GRID)


def mascot(d, cx, cy, diam, n=19):
    """The LED circle with the mascot's eyes and the red accent dot."""
    d.ellipse([(cx - diam / 2 - 3) * SS, (cy - diam / 2 - 3) * SS, (cx + diam / 2 + 3) * SS, (cy + diam / 2 + 3) * SS], fill=(14, 14, 14), outline=(44, 44, 44), width=SS)
    c, cell = (n - 1) / 2, diam / n
    eyes = {(x, y) for x in (6, 7, 11, 12) for y in (7, 8, 9, 10)}
    for y in range(n):
        for x in range(n):
            if math.hypot(x - c, y - c) > c + 0.3:
                continue
            col = WHITE if (x, y) in eyes else RED if (x, y) in {(13, 13)} else OFF
            dot(d, cx + (x - c) * cell, cy + (y - c) * cell, cell * 0.36, col)


def text(d, s, x, y, pitch, col, r=None):
    """Writes with the dot font; returns the width."""
    r = r or pitch * 0.42
    x0 = x
    for ch in s:
        rows = G.get(ch, G[' '])
        used = [k for k in range(5) if any(row[k] == '#' for row in rows)]
        first, cols = (0, 5) if ch.isdigit() or not used else (used[0], used[-1] - used[0] + 1)
        if not used:
            cols = 3
        for ry, row in enumerate(rows):
            for k, v in enumerate(row):
                if v == '#':
                    dot(d, x + (k - first) * pitch + pitch / 2, y + ry * pitch + pitch / 2, r, col)
        x += (cols + 1) * pitch
    return x - x0 - pitch


def width(s, pitch):
    img = Image.new('RGB', (1, 1)); return text(ImageDraw.Draw(img), s, 0, 0, pitch, BLACK)


def save(img, path, w, h):
    img.resize((w, h), Image.LANCZOS).convert('RGB').save(path, 'BMP')


def build(out):
    os.makedirs(out, exist_ok=True)
    # sidebar of the welcome and finish pages
    w, h = 164, 314
    img = Image.new('RGB', (w * SS, h * SS), BLACK); d = ImageDraw.Draw(img)
    grid_bg(d, w, h)
    mascot(d, w / 2, 112, 108)
    p = 2.6
    tw = width('GLYPHLING', p)
    text(d, 'GLYPHLING', (w - tw) / 2, 196, p, WHITE)
    sub = 'widget a punti'
    p2 = 1.5
    text(d, sub, (w - width(sub, p2)) / 2, 226, p2, MUTE)
    dot(d, 18, h - 18, 2.2, RED)
    save(img, f'{out}/sidebar.bmp', w, h)

    # header of the other pages (right of the title)
    w, h = 150, 57
    img = Image.new('RGB', (w * SS, h * SS), BLACK); d = ImageDraw.Draw(img)
    grid_bg(d, w, h, step=6)
    mascot(d, w - 30, h / 2, 42)
    dot(d, w - 62, h / 2, 1.8, RED)
    save(img, f'{out}/header.bmp', w, h)
    print('installer images written to', out)


if __name__ == '__main__':
    build(sys.argv[1] if len(sys.argv) > 1 else 'src-tauri/installer')
