#!/usr/bin/env python3
"""Génère assets/quark.png : la bague de Quark sur un carré AMOLED.

Aucune dépendance : PNG écrit à la main (zlib), tracé en supersampling 3x pour
des bords lisses.
"""
import math
import struct
import zlib
from pathlib import Path

SIZE = 512
S = 3  # supersampling
N = SIZE * S


def lerp(a, b, t):
    return a + (b - a) * t


def rounded_rect(x, y, left, top, right, bottom, radius):
    cx = min(max(x, left + radius), right - radius)
    cy = min(max(y, top + radius), bottom - radius)
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius


# Couleur du fond : noir AMOLED, très légèrement bleuté.
BG = (5, 7, 11)
BLUE = (26, 159, 255)
BLUE_LIGHT = (79, 192, 255)
WHITE = (232, 245, 255)


def sample(x, y):
    """Couleur RGBA d'un point, en coordonnées de l'image finale."""
    n = SIZE
    margin = n * 0.055
    if not rounded_rect(x, y, margin, margin, n - margin, n - margin, n * 0.215):
        return (0, 0, 0, 0)
    # Dégradé vertical discret : noir pur en bas, bleu nuit en haut.
    t = y / n
    r = lerp(BG[0], 13, t)
    g = lerp(BG[1], 24, t)
    b = lerp(BG[2], 42, t)

    cx, cy = n / 2, n / 2
    dx, dy = x - cx, y - cy
    dist = math.hypot(dx, dy)
    outer = n * 0.265
    inner = n * 0.165
    # Bague principale, dégradée du bleu clair (haut) au bleu Steam (bas).
    if inner <= dist <= outer:
        k = (dy + outer) / (2 * outer)
        r = lerp(BLUE_LIGHT[0], BLUE[0], k)
        g = lerp(BLUE_LIGHT[1], BLUE[1], k)
        b = lerp(BLUE_LIGHT[2], BLUE[2], k)
        # Halo doux sur le bord intérieur.
        return (int(r), int(g), int(b), 255)
    # Queue du Q : un trait diagonal en bas à droite.
    qx, qy = cx + n * 0.150, cy + n * 0.150
    rx, ry = cx + n * 0.360, cy + n * 0.360
    vx, vy = rx - qx, ry - qy
    length2 = vx * vx + vy * vy
    proj = max(0.0, min(1.0, ((x - qx) * vx + (y - qy) * vy) / length2))
    px, py = qx + vx * proj, qy + vy * proj
    if math.hypot(x - px, y - py) <= n * 0.042:
        return (*BLUE, 255)
    # Point en orbite, en haut à droite de la bague.
    ox, oy = cx + n * 0.318, cy - n * 0.318
    if math.hypot(x - ox, y - oy) <= n * 0.052:
        return (*WHITE, 255)
    # Petit cœur lumineux au centre.
    if dist <= n * 0.052:
        return (*WHITE, 255)
    return (int(r), int(g), int(b), 255)


rows = []
for py in range(SIZE):
    row = bytearray()
    for px in range(SIZE):
        acc = [0.0, 0.0, 0.0, 0.0]
        for sy in range(S):
            for sx in range(S):
                x = px + (sx + 0.5) / S
                y = py + (sy + 0.5) / S
                c = sample(x, y)
                a = c[3] / 255.0
                acc[0] += c[0] * a
                acc[1] += c[1] * a
                acc[2] += c[2] * a
                acc[3] += a
        count = S * S
        alpha = acc[3] / count
        if alpha <= 0.002:
            row += bytes((0, 0, 0, 0))
        else:
            row += bytes(
                (
                    int(acc[0] / acc[3]),
                    int(acc[1] / acc[3]),
                    int(acc[2] / acc[3]),
                    int(alpha * 255),
                )
            )
    rows.append(bytes(row))


def chunk(kind, data):
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


raw = b"".join(b"\x00" + row for row in rows)
png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(raw, 9))
png += chunk(b"IEND", b"")

out = Path(__file__).resolve().parent.parent / "assets/quark.png"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_bytes(png)
print(f"{out} ({len(png)} octets)")
