#!/usr/bin/env python3
"""生成 MarketLens 的占位图标源图（1024x1024 PNG）。

本机没有 PIL / ImageMagick，所以直接用 zlib + struct 手写 PNG 编码。
图样：深色圆角底 + 一条粉色上升折线（对应「粉色猫娘 + 行情」的工作代号）。
正式图标后续替换此文件即可，`npx tauri icon` 会重新派生全部尺寸。
"""

import struct
import zlib
from pathlib import Path

SIZE = 1024
BG = (17, 17, 20, 255)  # neutral-950
ACCENT = (244, 114, 182, 255)  # pink-400
GRID = (38, 38, 45, 255)

# 折线顶点（相对坐标 0..1），模拟一段行情走势
POLYLINE = [
    (0.16, 0.70),
    (0.28, 0.58),
    (0.38, 0.66),
    (0.52, 0.44),
    (0.64, 0.52),
    (0.84, 0.26),
]
STROKE = 26.0


def build_pixels() -> bytearray:
    px = bytearray(SIZE * SIZE * 4)

    def put(x: int, y: int, rgba: tuple[int, int, int, int]) -> None:
        i = (y * SIZE + x) * 4
        px[i : i + 4] = bytes(rgba)

    # 底：圆角矩形
    radius = SIZE * 0.22
    for y in range(SIZE):
        for x in range(SIZE):
            dx = max(radius - x, x - (SIZE - 1 - radius), 0)
            dy = max(radius - y, y - (SIZE - 1 - radius), 0)
            inside = (dx * dx + dy * dy) <= radius * radius
            put(x, y, BG if inside else (0, 0, 0, 0))

    # 背景网格
    step = SIZE // 8
    for k in range(1, 8):
        for y in range(SIZE):
            put(step * k, y, GRID)
        for x in range(SIZE):
            put(x, step * k, GRID)

    # 折线：对每个线段做「点到线段距离」判定，形成等宽描边
    pts = [(px_x * SIZE, px_y * SIZE) for px_x, px_y in POLYLINE]
    half = STROKE / 2
    for idx in range(len(pts) - 1):
        x0, y0 = pts[idx]
        x1, y1 = pts[idx + 1]
        min_x = max(int(min(x0, x1) - half) - 1, 0)
        max_x = min(int(max(x0, x1) + half) + 1, SIZE - 1)
        min_y = max(int(min(y0, y1) - half) - 1, 0)
        max_y = min(int(max(y0, y1) + half) + 1, SIZE - 1)
        seg_x, seg_y = x1 - x0, y1 - y0
        seg_len_sq = seg_x * seg_x + seg_y * seg_y

        for y in range(min_y, max_y + 1):
            for x in range(min_x, max_x + 1):
                if seg_len_sq == 0:
                    t = 0.0
                else:
                    t = ((x - x0) * seg_x + (y - y0) * seg_y) / seg_len_sq
                    t = min(max(t, 0.0), 1.0)
                proj_x, proj_y = x0 + t * seg_x, y0 + t * seg_y
                dist_sq = (x - proj_x) ** 2 + (y - proj_y) ** 2
                if dist_sq <= half * half:
                    put(x, y, ACCENT)

    return px


def write_png(path: Path, pixels: bytearray) -> None:
    raw = bytearray()
    stride = SIZE * 4
    for y in range(SIZE):
        raw.append(0)  # filter type 0 (None)
        raw += pixels[y * stride : (y + 1) * stride]

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


if __name__ == "__main__":
    out = Path(__file__).with_name("icon-source.png")
    write_png(out, build_pixels())
    print(f"已生成 {out} ({out.stat().st_size} bytes)")
