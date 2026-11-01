# Bakes the desktop's D textures (stone, hide, paper, ribbon, bronze) into
# TGAs an addon can ship. Colours are _d.css's tokens.
import sys
import numpy as np
from PIL import Image

out = sys.argv[1]
theme = sys.argv[2] if len(sys.argv) > 2 else "forever"
rng = np.random.default_rng(7)

# One palette per theme pack. "forever" is the desktop's D language.
THEMES = {
    "forever": dict(
        stone=("#2b2926", "#1a1917"), mottle=(26, 24, 20),
        hide=("#3b2a1e", "#2a1d14"), hideTint=(30, 22, 14), stitch="#d6b886",
        paper=("#eadcb8", "#e0cda2"), fiber=(18, 14, 8), stain=(60, 80, 110),
        ribbon=("#b42a22", "#7a1410"),
        metal=("#8a6a3e", "#4e3a20"), metalHi="#c9a26a", metalLo="#2a1d14", metalSide="#6a5030",
        card=("#1e1c19", "#171614"),
    ),
    # Warlock: violet-black stone, plum hide, a dark grimoire page, fel green.
    "warlock": dict(
        stone=("#221c2a", "#120e17"), mottle=(26, 16, 34),
        hide=("#2e1d33", "#1c1121"), hideTint=(26, 14, 30), stitch="#8fd14f",
        paper=("#3a3140", "#2a2330"), fiber=(14, 10, 18), stain=(-20, 30, -10),
        ribbon=("#5fae2e", "#2f6b16"),
        metal=("#3f6e2a", "#1d3812"), metalHi="#a6e36a", metalLo="#0c1a07", metalSide="#2c5019",
        card=("#1b1620", "#141018"),
    ),
}
T = THEMES[theme]


def hexc(h):
    h = h.lstrip("#")
    return np.array([int(h[i:i + 2], 16) for i in (0, 2, 4)], float)


def noise(w, h, cell, octaves=4):
    acc = np.zeros((h, w))
    amp, tot = 1.0, 0.0
    for o in range(octaves):
        c = max(1, int(cell / 2 ** o))
        g = rng.random((h // c + 2, w // c + 2))
        img = Image.fromarray((g * 255).astype(np.uint8)).resize(((w // c + 2) * c, (h // c + 2) * c), Image.BICUBIC)
        acc += amp * np.asarray(img, float)[:h, :w] / 255
        tot += amp
        amp *= 0.5
    return acc / tot


def grit(w, h, k=1.0):
    return (rng.random((h, w)) - 0.5) * k


def vgrad(w, h, top, bot):
    t = np.linspace(0, 1, h)[:, None, None]
    return np.broadcast_to(top * (1 - t) + bot * t, (h, w, 3)).copy()


def vignette(w, h, power=2.2):
    y, x = np.mgrid[0:h, 0:w]
    dx, dy = (x / (w - 1) - 0.5) * 2, (y / (h - 1) - 0.5) * 2
    return np.clip(1 - (np.abs(dx) ** power + np.abs(dy) ** power) ** 0.5 * 0.5, 0, 1)


def save(arr, name, alpha=None):
    arr = np.clip(arr, 0, 255).astype(np.uint8)
    if alpha is not None:
        a = np.clip(alpha * 255, 0, 255).astype(np.uint8)
        Image.fromarray(np.dstack([arr, a]), "RGBA").save(f"{out}/{name}.tga")
    else:
        Image.fromarray(arr, "RGB").save(f"{out}/{name}.tga")
    Image.fromarray(arr).save(f"{out}/{name}.png")


# stone: the app's background
W, H = 512, 512
s = vgrad(W, H, hexc(T["stone"][0]), hexc(T["stone"][1]))
m = noise(W, H, 96)[..., None]
s += (m - 0.5) * np.array(T["mottle"])
s += grit(W, H, 14)[..., None]
s *= (0.72 + 0.28 * vignette(W, H))[..., None]
save(s, "Stone")

# hide: the rail, with its stitch baked in at the rail's size (148x452)
W, H, RW, RH = 256, 512, 148, 452
h = vgrad(W, H, hexc(T["hide"][0]), hexc(T["hide"][1]))
h += (noise(W, H, 40, 3) - 0.5)[..., None] * np.array(T["hideTint"])
h += grit(W, H, 18)[..., None]
creases = noise(W, H, 6, 2)
h *= (0.9 + 0.1 * creases)[..., None]
y, x = np.mgrid[0:H, 0:W]
inset = 7
stitch = np.zeros((H, W), bool)
for (x0, y0, x1, y1) in [(inset, inset, RW - inset, inset), (inset, RH - inset, RW - inset, RH - inset)]:
    stitch |= (y == y0) & (x >= x0) & (x <= x1) & ((x // 4) % 2 == 0)
for xx in (inset, RW - inset - 1):
    stitch |= (x == xx) & (y >= inset) & (y <= RH - inset) & ((y // 4) % 2 == 0)
st = hexc(T["stitch"])
h[stitch] = h[stitch] * 0.62 + st * 0.38
# a darker groove under each stitch
groove = np.roll(stitch, 1, axis=0) & ~stitch
h[groove] *= 0.8
save(h, "Hide")

# paper: the journal page
W, H = 512, 512
p = vgrad(W, H, hexc(T["paper"][0]), hexc(T["paper"][1]))
p += (noise(W, H, 4, 2) - 0.5)[..., None] * np.array(T["fiber"])
stain = np.clip(noise(W, H, 160, 3) - 0.52, 0, 1)[..., None]
p -= stain * np.array(T["stain"])
edge = vignette(W, H, 3.5)
p *= (0.8 + 0.2 * edge)[..., None]
p += grit(W, H, 8)[..., None]
save(p, "Paper")

# ribbon: the selected rail row, swallowtail at the right
W, H = 256, 32
r = vgrad(W, H, hexc(T["ribbon"][0]), hexc(T["ribbon"][1]))
r[:2] += 30
r += grit(W, H, 10)[..., None]
y, x = np.mgrid[0:H, 0:W]
notch = 14
a = np.ones((H, W))
tail = x > W - notch - 1
depth = (W - 1 - x) / notch
a[tail & (np.abs(y - (H - 1) / 2) / ((H - 1) / 2) < 1 - depth)] = 0
r[-1] *= 0.6
save(r, "Ribbon", a)

# bronze: the one primary button
W, H = 128, 32
b = vgrad(W, H, hexc(T["metal"][0]), hexc(T["metal"][1]))
b[:1] = hexc(T["metalHi"])
b[-1:] = hexc(T["metalLo"])
b[:, :1] = hexc(T["metalSide"])
b[:, -1:] = hexc(T["metalSide"])
b += grit(W, H, 8)[..., None]
for cx in (7, W - 8):
    b[15:17, cx:cx + 2] = hexc(T["metalHi"])
save(b, "Bronze")

# card: dark panel with a hide header strip (for module cards)
W, H = 256, 256
c = vgrad(W, H, hexc(T["card"][0]), hexc(T["card"][1]))
c += grit(W, H, 8)[..., None]
save(c, "Card")

# sigil: a theme's emblem, drawn faint behind the page (white, tinted in game)
if "sigil" in T or theme == "warlock":
    import math
    from PIL import ImageDraw, ImageFilter
    S = 512
    img = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(img)
    c0 = S / 2
    for rad, w in ((230, 6), (206, 3), (120, 3)):
        d.ellipse((c0 - rad, c0 - rad, c0 + rad, c0 + rad), outline=255, width=w)
    pts = [(c0 + 206 * math.sin(math.radians(i * 144)), c0 - 206 * math.cos(math.radians(i * 144))) for i in range(6)]
    d.line(pts, fill=255, width=4, joint="curve")
    for i in range(24):
        a = math.radians(i * 15)
        r1, r2 = 212, 228 if i % 2 else 222
        d.line((c0 + r1 * math.sin(a), c0 - r1 * math.cos(a), c0 + r2 * math.sin(a), c0 - r2 * math.cos(a)), fill=255, width=3)
    glow = img.filter(ImageFilter.GaussianBlur(10))
    a = np.clip(np.asarray(img, float) / 255 + np.asarray(glow, float) / 255 * 0.8, 0, 1)
    save(np.full((S, S, 3), 255.0), "Sigil", a)
print("ok")
