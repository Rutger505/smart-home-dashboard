"""Print the Nextion line commands for the staircase treads.

The staircase is a box left of the corridor, with the corridor wall as
its right side. It turns a quarter at each end: at the bottom it opens
into the ground floor corridor, at the top into the second floor
corridor. Each opening is as tall as the box is wide, so the first step
off the corridor is as wide as the stair and flush with both walls.

At each turn the winders fan out from the inner corner on the corridor
wall, in equal angle steps from the opening (vertical) to the first
straight tread (horizontal). The treads between the two turns are
straight.

Usage: python3 hmi/stairs.py [--preview out.png]
"""

import math
import sys

LEFT, RIGHT = 105, 155
TOP, BOTTOM = 63, 213
WIDTH = RIGHT - LEFT
TOP_PIVOT = (RIGHT, TOP + WIDTH)
BOTTOM_PIVOT = (RIGHT, BOTTOM - WIDTH)
STEP = 10
WINDERS = 6
WHITE = 65535


def winder(pivot, angle):
    """Line from the pivot to wherever the ray hits the left, top or
    bottom wall. Negative angles point up-left."""
    px, py = pivot
    t = math.radians(angle)
    dx, dy = -math.cos(t), math.sin(t)
    hits = [(LEFT - px) / dx]
    if dy > 0:
        hits.append((BOTTOM - py) / dy)
    elif dy < 0:
        hits.append((TOP - py) / dy)
    s = min(hits)
    return [round(v) for v in (px, py, px + dx * s, py + dy * s)]


def lines():
    step = 90 / WINDERS
    upper = [winder(TOP_PIVOT, -i * step) for i in range(WINDERS - 1, 0, -1)]
    middle = [
        [RIGHT, y, LEFT, y]
        for y in range(TOP_PIVOT[1], BOTTOM_PIVOT[1] + 1, STEP)
    ]
    lower = [winder(BOTTOM_PIVOT, i * step) for i in range(1, WINDERS)]
    return upper + middle + lower


def preview(path):
    from PIL import Image, ImageDraw

    im = Image.new("RGB", (200, 180), "black")
    d = ImageDraw.Draw(im)
    box = [(LEFT, TOP, RIGHT, TOP), (LEFT, TOP, LEFT, BOTTOM),
           (LEFT, BOTTOM, RIGHT, BOTTOM), (RIGHT, TOP, RIGHT, BOTTOM)]
    for x1, y1, x2, y2 in box + lines():
        d.line((x1 - 80, y1 - 50, x2 - 80, y2 - 50), fill="white")
    im.resize((800, 720), Image.NEAREST).save(path)


if __name__ == "__main__":
    for x1, y1, x2, y2 in lines():
        print(f"line {x1},{y1},{x2},{y2},{WHITE}")
    if len(sys.argv) == 3 and sys.argv[1] == "--preview":
        preview(sys.argv[2])
