#!/usr/bin/env python3
"""Generate the packaged app icons: assets/AppIcon.icns and assets/AppIcon.ico.

Pure Python: no image libraries and no macOS tools (iconutil) are needed.
The geometry matches src/ui/icons.rs (tile, gradient and the two arrows), so
the window, tray and packaged icons stay identical. Edges are anti-aliased
from signed distances.

- macOS: the tile follows the Big Sur icon grid (824/1024 of the canvas) and
  carries a soft drop shadow, like other Mac apps.
- Windows: the tile fills the canvas, as the previous icon did.
"""
import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Same values as ARROWS / ARROW_WIDTH / TILE_* in src/ui/icons.rs (on a unit
# square where the tile spans 0.03 – 0.97).
ARROWS = [
    ((0.24, 0.36), (0.75, 0.36)),
    ((0.58, 0.20), (0.76, 0.36)),
    ((0.76, 0.36), (0.58, 0.52)),
    ((0.76, 0.64), (0.25, 0.64)),
    ((0.42, 0.48), (0.24, 0.64)),
    ((0.24, 0.64), (0.42, 0.80)),
]
ARROW_WIDTH = 0.085
TILE_HALF, TILE_RADIUS = 0.47, 0.16
TOP, BOTTOM = (76, 141, 255), (29, 84, 216)


def rounded_rect_distance(x, y, half, radius):
    qx, qy = abs(x) - (half - radius), abs(y) - (half - radius)
    return math.hypot(max(qx, 0.0), max(qy, 0.0)) + min(max(qx, qy), 0.0) - radius


def segment_distance(x, y, a, b):
    (ax, ay), (bx, by) = a, b
    vx, vy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((x - ax) * vx + (y - ay) * vy) / (vx * vx + vy * vy)))
    return math.hypot(x - ax - t * vx, y - ay - t * vy)


def coverage(distance, pixel):
    """Anti-aliased coverage of a shape at `distance` (negative = inside)."""
    return max(0.0, min(1.0, 0.5 - distance / pixel))


def render(size, tile_scale, shadow):
    """RGBA pixels. `tile_scale` is the tile's width relative to the canvas."""
    pixel = 1.0 / size
    half = tile_scale / 2
    k = TILE_HALF / half  # canvas → icon-unit scale
    rows = []
    for py in range(size):
        y = (py + 0.5) * pixel
        row = bytearray()
        t = (y - (0.5 - half)) / (2 * half)
        base = [TOP[c] + (BOTTOM[c] - TOP[c]) * max(0.0, min(1.0, t)) for c in range(3)]
        for px in range(size):
            x = (px + 0.5) * pixel
            d_tile = rounded_rect_distance(x - 0.5, y - 0.5, half, TILE_RADIUS / k)
            tile = coverage(d_tile, pixel)
            arrow = 0.0
            if tile > 0:
                ux, uy = 0.5 + (x - 0.5) * k, 0.5 + (y - 0.5) * k
                d = min(segment_distance(ux, uy, a, b) for a, b in ARROWS) - ARROW_WIDTH / 2
                arrow = coverage(d / k, pixel)
            rgb = [base[c] + (255 - base[c]) * arrow for c in range(3)]
            alpha = tile
            if shadow and tile < 1:
                d_shadow = rounded_rect_distance(x - 0.5, y - 0.5 - 0.012, half, TILE_RADIUS / k)
                blur = 0.03
                s = 0.32 * max(0.0, min(1.0, 1 - (d_shadow + blur * 0.25) / blur)) ** 2
                # Tile over shadow ("over" compositing with a black shadow).
                out_a = tile + s * (1 - tile)
                if out_a > 0:
                    rgb = [c * tile / out_a for c in rgb]
                alpha = out_a
            a = round(alpha * 255)
            row += bytes([round(rgb[0]), round(rgb[1]), round(rgb[2]), a] if a else [0, 0, 0, 0])
        rows.append(bytes(row))
    return rows


def png(rows, size):
    def chunk(kind, data):
        return (struct.pack('>I', len(data)) + kind + data
                + struct.pack('>I', zlib.crc32(kind + data) & 0xFFFFFFFF))
    raw = b''.join(b'\0' + r for r in rows)
    return (b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))


def packbits(channel):
    """The run-length encoding icns uses for each ARGB channel."""
    out, i, n = bytearray(), 0, len(channel)
    while i < n:
        run = 1
        while i + run < n and run < 130 and channel[i + run] == channel[i]:
            run += 1
        if run >= 3:
            out += bytes([0x80 + run - 3, channel[i]])
            i += run
            continue
        start = i
        while i < n and i - start < 128 and not (
                i + 2 < n and channel[i] == channel[i + 1] == channel[i + 2]):
            i += 1
        out += bytes([i - start - 1]) + channel[start:i]
    return bytes(out)


def argb(rows):
    """16 / 32 px entries (ic04 / ic05) in the ARGB format iconutil writes."""
    pixels = b''.join(rows)
    channels = [pixels[c::4] for c in (3, 0, 1, 2)]
    return b'ARGB' + b''.join(packbits(ch) for ch in channels)


def icns(pngs, small):
    entries = [('ic04', small[16]), ('ic05', small[32]), ('ic11', pngs[32]),
               ('ic12', pngs[64]), ('ic07', pngs[128]), ('ic08', pngs[256]),
               ('ic13', pngs[256]), ('ic09', pngs[512]), ('ic14', pngs[512]),
               ('ic10', pngs[1024])]
    body = b''.join(kind.encode() + struct.pack('>I', len(data) + 8) + data
                    for kind, data in entries)
    return b'icns' + struct.pack('>I', len(body) + 8) + body


def ico(images):
    sizes = sorted(images)
    offset = 6 + 16 * len(sizes)
    headers, payloads = [], []
    for n in sizes:
        data = images[n]
        headers.append(struct.pack('<BBBBHHII', n % 256, n % 256, 0, 0, 1, 32, len(data), offset))
        payloads.append(data)
        offset += len(data)
    return struct.pack('<HHH', 0, 1, len(sizes)) + b''.join(headers) + b''.join(payloads)


def main():
    mac = {n: render(n, 0.805, True) for n in [16, 32, 64, 128, 256, 512, 1024]}
    pngs = {n: png(rows, n) for n, rows in mac.items()}
    small = {n: argb(mac[n]) for n in [16, 32]}
    (ROOT / 'assets/AppIcon.icns').write_bytes(icns(pngs, small))
    win = {n: png(render(n, 0.94, False), n) for n in [16, 24, 32, 48, 64, 128, 256]}
    (ROOT / 'assets/AppIcon.ico').write_bytes(ico(win))
    for name in ['AppIcon.icns', 'AppIcon.ico']:
        print(f'assets/{name}: {(ROOT / "assets" / name).stat().st_size / 1024:.1f} KiB')


if __name__ == '__main__':
    main()
