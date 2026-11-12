#!/usr/bin/env python3
# Usage: python3 dist/brand/make.py assets/fonts/Bricolage-800.ttf dist/brand  (needs fontTools)
# Writes dist/brand/icon.svg: a big Bricolage ExtraBold "S" on an Adobe-style tile.
import sys
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.boundsPen import BoundsPen
f = TTFont(sys.argv[1]); gs = f.getGlyphSet(); g = gs[f.getBestCmap()[ord("S")]]
pen = SVGPathPen(gs); g.draw(pen); bp = BoundsPen(gs); g.draw(bp)
x0, y0, x1, y1 = bp.bounds
def place(h, cx, cy):
    s = h / (y1 - y0)
    # Font units are y-up: flip, scale to height h, centre on (cx, cy).
    return f'translate({cx - (x0 + x1) / 2 * s:.2f} {cy + (y0 + y1) / 2 * s:.2f}) scale({s:.5f} {-s:.5f})'
d = pen.getCommands()
icon = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect x="24" y="24" width="464" height="464" rx="92" fill="#FFB547"/>
  <rect x="44" y="44" width="424" height="424" rx="74" fill="#16142A"/>
  <path transform="{place(300, 256, 262)}" fill="#FFB547" d="{d}"/>
</svg>
'''
open(sys.argv[2] + "/icon.svg", "w").write(icon)
