# Bakes the desktop's D textures (stone, hide, paper, ribbon, bronze) into
# TGAs an addon can ship. Colours are _d.css's tokens.
import sys
import numpy as np
from PIL import Image

out = sys.argv[1]
rng = np.random.default_rng(7)


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
s = vgrad(W, H, hexc("#2b2926"), hexc("#1a1917"))
m = noise(W, H, 96)[..., None]
s += (m - 0.5) * np.array([26, 24, 20])
s += grit(W, H, 14)[..., None]
s *= (0.72 + 0.28 * vignette(W, H))[..., None]
save(s, "Stone")

# hide: the rail, with its stitch baked in at the rail's size (148x452)
W, H, RW, RH = 256, 512, 148, 452
h = vgrad(W, H, hexc("#3b2a1e"), hexc("#2a1d14"))
h += (noise(W, H, 40, 3) - 0.5)[..., None] * np.array([30, 22, 14])
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
st = hexc("#d6b886")
h[stitch] = h[stitch] * 0.62 + st * 0.38
# a darker groove under each stitch
groove = np.roll(stitch, 1, axis=0) & ~stitch
h[groove] *= 0.8
save(h, "Hide")

# paper: the journal page
W, H = 512, 512
p = vgrad(W, H, hexc("#eadcb8"), hexc("#e0cda2"))
p += (noise(W, H, 4, 2) - 0.5)[..., None] * np.array([18, 14, 8])
stain = np.clip(noise(W, H, 160, 3) - 0.52, 0, 1)[..., None]
p -= stain * np.array([60, 80, 110])
edge = vignette(W, H, 3.5)
p *= (0.8 + 0.2 * edge)[..., None]
p += grit(W, H, 8)[..., None]
save(p, "Paper")

# ribbon: the selected rail row, swallowtail at the right
W, H = 256, 32
r = vgrad(W, H, hexc("#b42a22"), hexc("#7a1410"))
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
b = vgrad(W, H, hexc("#8a6a3e"), hexc("#4e3a20"))
b[:1] = hexc("#c9a26a")
b[-1:] = hexc("#2a1d14")
b[:, :1] = hexc("#6a5030")
b[:, -1:] = hexc("#6a5030")
b += grit(W, H, 8)[..., None]
for cx in (7, W - 8):
    b[15:17, cx:cx + 2] = hexc("#c9a26a")
save(b, "Bronze")

# card: dark panel with a hide header strip (for module cards)
W, H = 256, 256
c = vgrad(W, H, hexc("#1e1c19"), hexc("#171614"))
c += grit(W, H, 8)[..., None]
save(c, "Card")
print("ok")
