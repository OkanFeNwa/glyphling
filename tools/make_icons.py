#!/usr/bin/env python3
"""Disegna le icone a matrice di punti (app e area di notifica) senza librerie esterne."""
import math, struct, zlib, sys

def png(path, w, h, px):
    raw = b"".join(b"\0" + bytes(px[y * w * 4:(y + 1) * w * 4]) for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

def disc(size, n, on, red, bg, ss=4, lit=(0xf2, 0xf2, 0xf2, 255)):
    """Cerchio pieno (bg) con una griglia n x n di punti quadrati arrotondati."""
    W = size * ss
    acc = [[0.0] * 4 for _ in range(size * size)]
    c = (n - 1) / 2
    cell = W / (n + 1.6); off = (W - cell * n) / 2; s = cell * 0.74
    R = W / 2
    for Y in range(W):
        for X in range(W):
            col = None
            if bg and math.hypot(X + .5 - R, Y + .5 - R) <= R:
                col = bg
            gx, gy = (X - off) / cell, (Y - off) / cell
            ix, iy = int(gx), int(gy)
            if 0 <= ix < n and 0 <= iy < n and math.hypot(ix - c, iy - c) <= c + .35:
                fx, fy = gx - ix - .5, gy - iy - .5
                if abs(fx) * cell <= s / 2 and abs(fy) * cell <= s / 2:
                    col = red if (ix, iy) in red_set(n) else lit if (ix, iy) in on else (0x26, 0x26, 0x26, 255) if bg else None
            if col:
                a = acc[(Y // ss) * size + X // ss]
                for k in range(3): a[k] += col[k] * col[3] / 255
                a[3] += col[3] / 255
    out = []
    for a in acc:
        al = a[3] / (ss * ss)
        out += [int(a[k] / a[3]) if a[3] else 0 for k in range(3)] + [int(al * 255)]
    return out

_red = {}
def red_set(n):
    return _red.get(n, set())

WHITE = (0xf2, 0xf2, 0xf2, 255); RED = (0xd7, 0x19, 0x21, 255); BLACK = (0, 0, 0, 255)
# icona dell'app: la mascotte (due occhi) sul cerchio nero, con il punto rosso dell'accento
n = 11
eyes = {(3, 4), (3, 5), (3, 6), (7, 4), (7, 5), (7, 6)}
_red[n] = {(8, 8)}
png(sys.argv[1] + "/app-1024.png", 1024, 1024, disc(1024, n, eyes, RED, BLACK, ss=1))
# icona dell'area di notifica: disco di punti bianchi su trasparente
n = 5
_red[n] = {(3, 1)}
allp = {(x, y) for x in range(n) for y in range(n)}
png(sys.argv[1] + "/tray.png", 32, 32, disc(32, n, allp, RED, None, ss=8))
print("icone scritte")
