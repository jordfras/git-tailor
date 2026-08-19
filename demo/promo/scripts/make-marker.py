#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Draw one annotation marker to a transparent PNG, for compose.sh to overlay.
#
# A tutorial narrates things the viewer then has to find: "each column is one
# group of hunks" is only useful if you know which column. A ring drawn around
# the thing being named answers that without the narration having to describe
# where to look.
#
# ffmpeg can draw a rectangle but not a rounded one, and square corners around
# a terminal cell read as a selection rather than as a pointer, so the shape is
# drawn here and overlaid as an image -- the same arrangement as make-emoji.py.
#
# Always a rounded rectangle. A ring suits a target about as wide as it is tall,
# but stretched down a whole column it degenerates into a sliver that looks like
# a mistake -- and one shape for everything is one less thing to get wrong.
#
# The PNG is the bounding box of the box plus room for the stroke, so the caller
# positions it by its top-left corner like any other overlay.
import argparse
from PIL import Image, ImageDraw


def main() -> int:
    ap = argparse.ArgumentParser(description="Draw an annotation marker to a PNG.")
    ap.add_argument("--out", required=True)
    ap.add_argument("--width", type=int, required=True, help="marked box width")
    ap.add_argument("--height", type=int, required=True, help="marked box height")
    ap.add_argument("--gap", type=int, default=6, help="clearance around the box")
    ap.add_argument("--stroke", type=int, default=4)
    # Amber rather than red or green: those already mean something in the
    # matrix, and a marker that reuses them would read as part of the data.
    ap.add_argument("--color", default="#ffd166")
    ap.add_argument(
        "--shape",
        choices=("rect", "arrow-up", "arrow-down"),
        default="rect",
        help="a rounded box around a target, or an arrow pointing along one",
    )
    args = ap.parse_args()

    # An arrow is not drawn around anything, so it gets the size it asks for; a
    # box stands off the marked one by the gap, to read as around rather than on.
    if args.shape.startswith("arrow"):
        dw, dh = args.width, args.height
    else:
        dw = args.width + args.gap * 2
        dh = args.height + args.gap * 2

    pad = args.stroke * 2
    w = dw + pad * 2
    h = dh + pad * 2
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(im)

    # Supersample: PIL's shapes have no antialiasing, and a hard-edged outline
    # on a terminal capture looks like a rendering fault rather than like an
    # annotation.
    scale = 4
    big = Image.new("RGBA", (w * scale, h * scale), (0, 0, 0, 0))
    bd = ImageDraw.Draw(big)
    box = [
        pad * scale,
        pad * scale,
        (pad + dw) * scale,
        (pad + dh) * scale,
    ]
    if args.shape.startswith("arrow"):
        # A direction, for narration that talks about ordering rather than
        # about a thing on screen: a stem with a head at one end.
        up = args.shape == "arrow-up"
        x0, y0, x1, y1 = box
        midx = (x0 + x1) // 2
        head = (y1 - y0) // 3
        tip, tail = (y0, y1) if up else (y1, y0)
        bd.line([(midx, tip), (midx, tail)], fill=args.color, width=args.stroke * scale)
        barb = (x1 - x0) // 2
        for dx in (-barb, barb):
            bd.line(
                [(midx, tip), (midx + dx, tip + (head if up else -head))],
                fill=args.color,
                width=args.stroke * scale,
            )
    else:
        # Enough rounding to read as an annotation rather than as a border,
        # without the ends closing into a lozenge on a narrow column.
        radius = min(dw, dh) * scale // 3
        bd.rounded_rectangle(
            box, radius=radius, outline=args.color, width=args.stroke * scale
        )
    im = big.resize((w, h), Image.LANCZOS)

    im.save(args.out)
    # Where the PNG's top-left sits relative to the marked box's top-left, so
    # the caller can place it without repeating this arithmetic.
    print(f"{(dw - args.width) // 2 + pad} {(dh - args.height) // 2 + pad}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
