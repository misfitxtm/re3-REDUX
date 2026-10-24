#!/usr/bin/env python3
"""Draws the Re3 Redux logo and banner as SVG, with the lettering converted to paths so it looks
the same everywhere. Needs fontTools and the Bebas Neue font (SIL OFL).

usage: gen_branding.py <BebasNeue-Regular.ttf> <out-dir>
"""
import sys
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

font = TTFont(sys.argv[1])
out = sys.argv[2]
gs, cmap, upm = font.getGlyphSet(), font.getBestCmap(), font["head"].unitsPerEm


def text(s, size, x, y, anchor="middle", tracking=0.0, skew=0.0):
    """Path data for `s` with its baseline at `y`; returns (d, width)."""
    scale = size / upm
    adv = [font["hmtx"][cmap[ord(c)]][0] * scale + tracking for c in s]
    width = sum(adv) - tracking
    x0 = x - width / 2 if anchor == "middle" else x
    pen, cx = SVGPathPen(gs), x0
    for c, a in zip(s, adv):
        # flip y, scale, apply a horizontal skew about the baseline
        t = (scale, 0, -skew * scale, -scale, cx + skew * 0, y)
        gs[cmap[ord(c)]].draw(TransformPen(pen, (scale, 0, skew * -scale * -1, -scale, cx, y)))
        cx += a
    return pen.getCommands(), width


def fit(s, size, max_w, **kw):
    d, w = text(s, size, 0, 0, **kw)
    return min(size, size * max_w / w)


def palm(cx, base, h, color):
    """A simple palm silhouette: curved trunk and six fronds."""
    top = (cx + h * 0.12, base - h)
    fronds = ""
    for dx, dy, lift in [(-0.55, 0.10, 0.22), (-0.38, -0.05, 0.28), (-0.15, -0.14, 0.26), (0.15, -0.14, 0.26), (0.38, -0.05, 0.28), (0.55, 0.10, 0.22)]:
        ex, ey = top[0] + dx * h * 0.8, top[1] + dy * h * 0.8 + h * 0.18
        fronds += f'<path d="M{top[0]:.1f} {top[1]:.1f} Q{top[0] + dx * h * 0.45:.1f} {top[1] - lift * h * 0.6:.1f} {ex:.1f} {ey:.1f} Q{top[0] + dx * h * 0.45:.1f} {top[1] - lift * h * 0.25:.1f} {top[0]:.1f} {top[1] + 2:.1f}Z" fill="{color}"/>'
    trunk = f'<path d="M{cx - 5:.1f} {base} Q{cx + h * 0.02:.1f} {base - h * 0.5:.1f} {top[0] - 3:.1f} {top[1]:.1f} L{top[0] + 3:.1f} {top[1]:.1f} Q{cx + h * 0.06:.1f} {base - h * 0.5:.1f} {cx + 5:.1f} {base}Z" fill="{color}"/>'
    return trunk + fronds


# ---------------------------------------------------------------- logo
d_re3, w_re3 = text("RE3", 168, 128, 160)
d_redux, _ = text("REDUX", 30, 128, 226, tracking=9)
logo = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1b212b"/><stop offset="1" stop-color="#0a0c11"/></linearGradient>
    <linearGradient id="ink" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#9aa3af"/></linearGradient>
    <linearGradient id="miami" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff4fa3"/><stop offset="1" stop-color="#2ee6e6"/></linearGradient>
    <linearGradient id="la" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ffb347"/><stop offset="1" stop-color="#ff5e62"/></linearGradient>
  </defs>
  <rect x="8" y="8" width="240" height="240" rx="56" fill="url(#bg)"/>
  <rect x="8.5" y="8.5" width="239" height="239" rx="55.5" fill="none" stroke="#ffffff" stroke-opacity="0.08"/>
  <path d="{d_re3}" fill="#000" opacity="0.45" transform="translate(4 5)"/>
  <path d="{d_re3}" fill="url(#ink)"/>
  <rect x="52" y="178" width="46" height="9" rx="4.5" fill="#a9b1bd"/>
  <rect x="105" y="178" width="46" height="9" rx="4.5" fill="url(#miami)"/>
  <rect x="158" y="178" width="46" height="9" rx="4.5" fill="url(#la)"/>
  <path d="{d_redux}" fill="#8b94a3"/>
</svg>
'''
open(f"{out}/re3-redux-logo.svg", "w").write(logo)

# ---------------------------------------------------------------- banner
W, H, P = 1600, 420, 1600 / 3
size_base = 250
t1 = fit("Re3", size_base, 430)
t2 = fit("re3 Miami", size_base, 470, skew=0.16)
t3 = fit("Re3 LA", size_base, 430)
BASE = 335
d1, _ = text("Re3", t1, P * 0.5, BASE)
d2, _ = text("re3 Miami", t2, P * 1.5, BASE, skew=0.16)
d3, _ = text("Re3 LA", t3, P * 2.5, BASE)
dsub, _ = text("RE3 REDUX  -  A RUST REWRITE", 30, W / 2, 400, tracking=8)

skyline = "".join(
    f'<rect x="{x}" y="{340 - h}" width="{w}" height="{h + 80}" fill="#0c0e11"/>'
    for x, w, h in [(10, 46, 90), (62, 34, 140), (104, 58, 70), (170, 40, 120), (216, 52, 85), (276, 30, 150), (314, 60, 100), (382, 44, 60), (434, 36, 130), (478, 50, 80)]
)
grid = "".join(f'<line x1="{x}" y1="0" x2="{x}" y2="{H}" />' for x in range(0, int(P), 48)) + "".join(f'<line x1="0" y1="{y}" x2="{P}" y2="{y}" />' for y in range(0, H, 48))
sun_stripes = "".join(f'<rect x="{P * 0.5 - 112}" y="{y}" width="224" height="{3 + i}" fill="#2a0b4a"/>' for i, y in enumerate(range(150, 238, 14)))

banner = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">
  <defs>
    <linearGradient id="p1" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2f353d"/><stop offset="1" stop-color="#111418"/></linearGradient>
    <linearGradient id="p2" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b1060"/><stop offset="0.6" stop-color="#1a0836"/><stop offset="1" stop-color="#0a0f2c"/></linearGradient>
    <linearGradient id="p3" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5a2a86"/><stop offset="0.45" stop-color="#e0457b"/><stop offset="0.8" stop-color="#ff9a3c"/><stop offset="1" stop-color="#ffd27a"/></linearGradient>
    <linearGradient id="ink1" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#8e97a3"/></linearGradient>
    <linearGradient id="ink2" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff4fa3"/><stop offset="1" stop-color="#2ee6e6"/></linearGradient>
    <linearGradient id="ink3" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff2b0"/><stop offset="1" stop-color="#ff7a2f"/></linearGradient>
    <linearGradient id="sun" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffe45e"/><stop offset="1" stop-color="#ff3d8b"/></linearGradient>
    <filter id="glow" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="9"/></filter>
    <clipPath id="c1"><rect x="0" y="0" width="{P}" height="{H}"/></clipPath>
  </defs>

  <!-- Re3: concrete, grey and white -->
  <g clip-path="url(#c1)">
    <rect width="{P}" height="{H}" fill="url(#p1)"/>
    <g stroke="#ffffff" stroke-opacity="0.045" stroke-width="2">{grid}</g>
    {skyline}
    <path d="{d1}" fill="#000" opacity="0.5" transform="translate(6 7)"/>
    <path d="{d1}" fill="url(#ink1)"/>
  </g>

  <!-- re3 Miami: neon dusk -->
  <g transform="translate({P} 0)">
    <rect width="{P}" height="{H}" fill="url(#p2)"/>
    <circle cx="{P * 0.5}" cy="150" r="112" fill="url(#sun)"/>
    {sun_stripes}
    <rect x="0" y="262" width="{P}" height="{H}" fill="#0a0f2c"/>
    <g stroke="#2ee6e6" stroke-opacity="0.3" stroke-width="2"><line x1="0" y1="282" x2="{P}" y2="282"/><line x1="0" y1="306" x2="{P}" y2="306"/><line x1="0" y1="340" x2="{P}" y2="340"/></g>
    <g transform="translate({-P} 0)">
      <path d="{d2}" fill="url(#ink2)" opacity="0.65" filter="url(#glow)"/>
      <path d="{d2}" fill="#12052a" opacity="0.8" transform="translate(5 6)"/>
      <path d="{d2}" fill="url(#ink2)"/>
    </g>
  </g>

  <!-- Re3 LA: sunset -->
  <g transform="translate({P * 2} 0)">
    <rect width="{P}" height="{H}" fill="url(#p3)"/>
    <path d="M0 330 Q90 290 190 318 T380 300 T{P} 322 L{P} {H} L0 {H}Z" fill="#3a1b52"/>
    <path d="M0 360 Q120 330 260 350 T{P} 346 L{P} {H} L0 {H}Z" fill="#1c0d2e"/>
    {palm(38, 372, 205, "#12091f")}
    {palm(P - 34, 372, 170, "#12091f")}
    <g transform="translate({-P * 2} 0)">
      <path d="{d3}" fill="#3a1050" opacity="0.55" transform="translate(5 6)"/>
      <path d="{d3}" fill="url(#ink3)"/>
    </g>
  </g>

  <!-- panel dividers and caption strip -->
  <rect x="{P - 2}" y="0" width="4" height="{H}" fill="#000" opacity="0.55"/>
  <rect x="{P * 2 - 2}" y="0" width="4" height="{H}" fill="#000" opacity="0.55"/>
  <rect x="0" y="360" width="{W}" height="60" fill="#05060a" opacity="0.72"/>
  <path d="{dsub}" fill="#d7dce5"/>
</svg>
'''
open(f"{out}/banner.svg", "w").write(banner)
print("wrote", out)
