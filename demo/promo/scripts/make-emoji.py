# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Rasterize one emoji to a transparent PNG, for compose.sh to overlay.
#
# ffmpeg cannot do this itself. Color emoji are bitmap glyphs (CBDT/CBLC) and
# drawtext rejects the font outright — "Monocromatic (1bpp) fonts are not
# supported" at the one size it will accept, and "invalid library handle" at
# every other. Pillow reads the embedded bitmaps happily.
#
# Usage:  make-emoji.py --emoji 🌈 --out rainbow.png [--size 300]

import argparse
import pathlib
import sys

FONT = pathlib.Path("/usr/share/fonts/noto/NotoColorEmoji.ttf")
# The font carries one bitmap strike, at 109px. Asking for any other size fails,
# so render at that and scale the result.
STRIKE = 109


def main() -> int:
    ap = argparse.ArgumentParser(description="Rasterize an emoji to a PNG.")
    ap.add_argument("--emoji", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--size", type=int, default=300)
    args = ap.parse_args()

    from PIL import Image, ImageDraw, ImageFont

    if not FONT.exists():
        print(f"make-emoji: no emoji font at {FONT}", file=sys.stderr)
        return 1

    font = ImageFont.truetype(str(FONT), STRIKE)
    canvas = Image.new("RGBA", (STRIKE * 2, STRIKE * 2), (0, 0, 0, 0))
    ImageDraw.Draw(canvas).text((0, 0), args.emoji, font=font, embedded_color=True)

    # Crop to the glyph so the caller positions the picture, not the padding.
    box = canvas.getbbox()
    if box is None:
        print(f"make-emoji: {args.emoji!r} rendered nothing", file=sys.stderr)
        return 1
    glyph = canvas.crop(box)

    scale = args.size / max(glyph.size)
    glyph = glyph.resize(
        (max(1, round(glyph.width * scale)), max(1, round(glyph.height * scale))),
        Image.LANCZOS,
    )
    pathlib.Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    glyph.save(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
