#!/usr/bin/env python3
"""Derive the monochrome macOS tray template from the Delta brand source.

macOS draws a menu-bar "template image" in the menu bar's own colour, so it must be a single
colour with transparency: here, black pixels whose alpha is the glyph coverage. Windows and
Linux keep the full-colour brand tray icon (`tray.png` / `tray.rgba`).

Source of truth: resources/brand/delta-logo-512x512.png (the glyph is #FBF9F9 on a #286F78 tile).

Outputs, written next to the other tray icons:

    apps/desktop/src-tauri/icons/tray-template.png   44x44 RGBA, for review
    apps/desktop/src-tauri/icons/tray-template.rgba  44x44 raw RGBA, embedded by the desktop shell

Python 3 standard library only, like check_brand_icons.py, so it runs anywhere Python runs.
Usage:  python scripts/brand_tray_template.py
"""

from __future__ import annotations

import os
import struct
import sys
import zlib

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BRAND_SOURCE = os.path.join(REPO_ROOT, "resources", "brand", "delta-logo-512x512.png")
ICON_DIR = os.path.join(REPO_ROOT, "apps", "desktop", "src-tauri", "icons")

# 22pt menu-bar slot at @2x; the glyph fills 36px of it and leaves 4px on every side.
CANVAS_SIZE = 44
GLYPH_SIZE = 36

# Tile and glyph red channel: the glyph coverage is the position between the two.
TILE_RED = 0x28
GLYPH_RED = 0xFB


def read_rgba(path: str) -> tuple[int, int, list[bytearray]]:
    """Decode an 8-bit, non-interlaced RGBA PNG into rows of RGBA bytes."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG file")
    width, height, depth, colour_type, _, _, interlace = struct.unpack(">IIBBBBB", data[16:29])
    if (depth, colour_type, interlace) != (8, 6, 0):
        raise ValueError("expected an 8-bit non-interlaced RGBA PNG")

    pixels = bytearray()
    cursor = 8
    while cursor < len(data):
        length, kind = struct.unpack(">I4s", data[cursor : cursor + 8])
        if kind == b"IDAT":
            pixels += data[cursor + 8 : cursor + 8 + length]
        cursor += 12 + length
    raw = zlib.decompress(bytes(pixels))

    stride = width * 4
    rows: list[bytearray] = []
    previous = bytearray(stride)
    offset = 0
    for _ in range(height):
        filter_type = raw[offset]
        line = bytearray(raw[offset + 1 : offset + 1 + stride])
        offset += 1 + stride
        for index in range(stride):
            left = line[index - 4] if index >= 4 else 0
            up = previous[index]
            upper_left = previous[index - 4] if index >= 4 else 0
            if filter_type == 1:
                line[index] = (line[index] + left) & 0xFF
            elif filter_type == 2:
                line[index] = (line[index] + up) & 0xFF
            elif filter_type == 3:
                line[index] = (line[index] + (left + up) // 2) & 0xFF
            elif filter_type == 4:
                estimate = left + up - upper_left
                distances = (abs(estimate - left), abs(estimate - up), abs(estimate - upper_left))
                nearest = (left, up, upper_left)[distances.index(min(distances))]
                line[index] = (line[index] + nearest) & 0xFF
            elif filter_type != 0:
                raise ValueError(f"unknown PNG filter {filter_type}")
        rows.append(line)
        previous = line
    return width, height, rows


def glyph_coverage(width: int, height: int, rows: list[bytearray]) -> list[list[float]]:
    """Coverage 0..1 of the light glyph over the teal tile; transparent corners count as 0."""
    span = GLYPH_RED - TILE_RED
    coverage = []
    for row in rows:
        line = []
        for x in range(width):
            red, alpha = row[x * 4], row[x * 4 + 3]
            ramp = min(1.0, max(0.0, (red - TILE_RED) / span))
            line.append(ramp * alpha / 255.0)
        coverage.append(line)
    return coverage


def render_template(coverage: list[list[float]]) -> bytes:
    """Crop to the glyph, scale it to GLYPH_SIZE and centre it on the canvas (box filter)."""
    height, width = len(coverage), len(coverage[0])
    solid = [(x, y) for y in range(height) for x in range(width) if coverage[y][x] > 0.5]
    if not solid:
        raise ValueError("no glyph found in the brand source")
    left = min(x for x, _ in solid)
    right = max(x for x, _ in solid) + 1
    top = min(y for _, y in solid)
    bottom = max(y for _, y in solid) + 1
    scale = GLYPH_SIZE / max(right - left, bottom - top)
    inset_x = (CANVAS_SIZE - (right - left) * scale) / 2
    inset_y = (CANVAS_SIZE - (bottom - top) * scale) / 2

    out = bytearray()
    for oy in range(CANVAS_SIZE):
        for ox in range(CANVAS_SIZE):
            x0 = left + (ox - inset_x) / scale
            x1 = left + (ox + 1 - inset_x) / scale
            y0 = top + (oy - inset_y) / scale
            y1 = top + (oy + 1 - inset_y) / scale
            total = weight = 0.0
            for sy in range(max(0, int(y0)), min(height, int(y1) + 1)):
                cover_y = min(sy + 1, y1) - max(sy, y0)
                if cover_y <= 0:
                    continue
                for sx in range(max(0, int(x0)), min(width, int(x1) + 1)):
                    cover_x = min(sx + 1, x1) - max(sx, x0)
                    if cover_x <= 0:
                        continue
                    area = cover_x * cover_y
                    total += coverage[sy][sx] * area
                    weight += area
            alpha = round(255 * total / weight) if weight else 0
            out += bytes((0, 0, 0, alpha))
    return bytes(out)


def write_png(path: str, size: int, rgba: bytes) -> None:
    def chunk(kind: bytes, payload: bytes) -> bytes:
        body = kind + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body))

    stride = size * 4
    scanlines = b"".join(b"\x00" + rgba[row * stride : (row + 1) * stride] for row in range(size))
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(scanlines, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as handle:
        handle.write(png)


def main() -> int:
    if not os.path.isfile(BRAND_SOURCE):
        print(f"error: brand source not found: {BRAND_SOURCE}", file=sys.stderr)
        return 2
    width, height, rows = read_rgba(BRAND_SOURCE)
    rgba = render_template(glyph_coverage(width, height, rows))
    write_png(os.path.join(ICON_DIR, "tray-template.png"), CANVAS_SIZE, rgba)
    with open(os.path.join(ICON_DIR, "tray-template.rgba"), "wb") as handle:
        handle.write(rgba)
    print(f"wrote tray-template.png and tray-template.rgba ({CANVAS_SIZE}x{CANVAS_SIZE}) to {ICON_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
