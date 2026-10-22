"""Generate the inkmd logo, icon and social preview (PIL only)."""
import math, sys
from PIL import Image, ImageDraw, ImageFont

# Usage: python3 scripts/logo.py [output dir, default: doc]
OUT = sys.argv[1] if len(sys.argv) > 1 else "doc"
FONT = "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-ExtraBold.ttf"
FONT_REG = "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf"
BG = (16, 18, 28)
FG = (230, 233, 242)
MUTED = (122, 130, 150)
PURPLE = (199, 146, 234)
BLUE = (130, 170, 255)
SS = 4  # supersampling


def gradient(w, h, c1, c2):
    """Diagonal gradient from top-left c1 to bottom-right c2."""
    g = Image.new("RGB", (w, h))
    px = g.load()
    for y in range(h):
        for x in range(w):
            t = (x / w * 0.4 + y / h * 0.6)
            px[x, y] = tuple(int(a + (b - a) * t) for a, b in zip(c1, c2))
    return g


def drop_mask(w, h, cx, cy, r):
    m = Image.new("L", (w, h), 0)
    d = r * 1.85
    phi = math.asin(r / d)
    pts = [(cx, cy - d)]
    steps = 120
    a0, a1 = -phi, math.pi + phi  # right tangent, around the bottom, to left tangent
    for i in range(steps + 1):
        a = a0 + (a1 - a0) * i / steps
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    ImageDraw.Draw(m).polygon(pts, fill=255)
    return m


def icon(size, rounded=True):
    S = size * SS
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if rounded:
        d.rounded_rectangle((0, 0, S - 1, S - 1), radius=int(S * 0.22), fill=BG)
    r = S * 0.27
    cx, cy = S / 2, S * 0.60
    mask = drop_mask(S, S, cx, cy, r)
    img.paste(gradient(S, S, PURPLE, BLUE), (0, 0), mask)
    # The Markdown heading mark, cut out of the drop.
    f = ImageFont.truetype(FONT, int(r * 1.4))
    d.text((cx, cy + r * 0.05), "#", font=f, fill=BG, anchor="mm")
    return img.resize((size, size), Image.LANCZOS)


def wordmark(height):
    """'ink' in light text, 'md' in the gradient."""
    H = height * SS
    f = ImageFont.truetype(FONT, H)
    ink_w = f.getlength("ink")
    md_w = f.getlength("md")
    W = int(ink_w + md_w) + 4
    asc, desc = f.getmetrics()
    img = Image.new("RGBA", (W, asc + desc), (0, 0, 0, 0))
    ImageDraw.Draw(img).text((0, 0), "ink", font=f, fill=FG)
    mask = Image.new("L", img.size, 0)
    ImageDraw.Draw(mask).text((ink_w, 0), "md", font=f, fill=255)
    img.paste(gradient(img.size[0], img.size[1], PURPLE, BLUE), (0, 0), mask)
    bbox = img.getbbox()
    img = img.crop(bbox)
    return img.resize((img.size[0] // SS, img.size[1] // SS), Image.LANCZOS)


def banner(w, h, tagline=None, icon_px=None, rounded=True):
    img = Image.new("RGBA", (w, h), BG + (255,))
    ic = icon(icon_px or int(h * 0.62), rounded=False)
    wm = wordmark(int(h * 0.30))
    gap = int(h * 0.06)
    total = ic.size[0] + gap + wm.size[0]
    x = (w - total) // 2
    y_icon = (h - ic.size[1]) // 2 - (int(h * 0.06) if tagline else 0)
    img.alpha_composite(ic, (x, y_icon))
    y_wm = y_icon + (ic.size[1] - wm.size[1]) // 2 + int(ic.size[1] * 0.08)
    img.alpha_composite(wm, (x + ic.size[0] + gap, y_wm))
    if tagline:
        d = ImageDraw.Draw(img)
        f = ImageFont.truetype(FONT_REG, int(h * 0.055))
        d.text((w / 2, y_icon + ic.size[1] + int(h * 0.07)), tagline, font=f, fill=MUTED, anchor="mm")
    if not rounded:
        return img
    # Rounded corners for the README banner.
    m = Image.new("L", (w * SS, h * SS), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, w * SS - 1, h * SS - 1), radius=int(h * 0.10 * SS), fill=255)
    img.putalpha(m.resize((w, h), Image.LANCZOS))
    return img


icon(512).save(f"{OUT}/icon.png", optimize=True)
banner(1200, 360).save(f"{OUT}/logo.png", optimize=True)
banner(1280, 640, "Fast, minimal Markdown for your terminal", rounded=False).save(f"{OUT}/social.png", optimize=True)
print("ok")
