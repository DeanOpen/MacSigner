#!/usr/bin/env python3
"""Draws the DMG window background (install instructions) for macOS.

Writes packaging/macos/dmg-background.tiff with 1x and 2x images, so Finder shows it
sharply on Retina screens. Needs Pillow and macOS (system fonts, tiffutil).
Icon positions must match bundle.macOS.dmg in app/src-tauri/tauri.conf.json.
"""
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

WIDTH, HEIGHT = 660, 420
APP_X, FOLDER_X, ICONS_Y = 170, 490, 205
FONT = "/System/Library/Fonts/SFNS.ttf"
OUT = Path(__file__).resolve().parent.parent / "packaging/macos/dmg-background.tiff"


def font(size, weight):
    f = ImageFont.truetype(FONT, size)
    try:
        f.set_variation_by_name(weight)
    except (OSError, ValueError):
        pass
    return f


def draw(scale):
    s = lambda v: round(v * scale)
    img = Image.new("RGB", (s(WIDTH), s(HEIGHT)), "#f6f7fb")
    d = ImageDraw.Draw(img)

    def centered(text, y, size, weight, fill):
        f = font(s(size), weight)
        w = d.textlength(text, font=f)
        d.text(((s(WIDTH) - w) / 2, s(y)), text, font=f, fill=fill)

    centered("Cài đặt MacSigner", 34, 24, "Semibold", "#1d1d1f")
    centered("Kéo biểu tượng MacSigner thả vào thư mục Applications", 70, 14, "Regular", "#515154")

    # Arrow from the app icon to the Applications folder.
    left, right, y = s(APP_X + 80), s(FOLDER_X - 80), s(ICONS_Y)
    color = "#4f46e5"
    d.line([(left, y), (right - s(14), y)], fill=color, width=s(5))
    d.polygon([(right, y), (right - s(20), y - s(13)), (right - s(20), y + s(13))], fill=color)

    centered("Sau đó mở MacSigner trong Launchpad hoặc thư mục Applications.", 330, 13, "Regular", "#515154")
    centered("Ứng dụng đã được ký và Apple xác minh, mở được ngay.", 352, 13, "Regular", "#86868b")
    return img


def main():
    with tempfile.TemporaryDirectory() as tmp:
        one, two = Path(tmp, "bg.png"), Path(tmp, "bg@2x.png")
        draw(1).save(one, dpi=(72, 72))
        draw(2).save(two, dpi=(144, 144))
        subprocess.run(["tiffutil", "-cathidpicheck", one, two, "-out", OUT], check=True)
    print(OUT)


if __name__ == "__main__":
    main()
