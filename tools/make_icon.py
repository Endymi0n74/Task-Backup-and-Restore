"""Génère l'icône de tsbak-gui : carré arrondi bleu, horloge blanche et coche verte.

Usage :
    python tools/make_icon.py

Produit :
    app-icon.png          (1024x1024, source)
    src-tauri/icons/32x32.png
    src-tauri/icons/128x128.png
    src-tauri/icons/128x128@2x.png
    src-tauri/icons/icon.ico
"""
import math
import os

from PIL import Image, ImageDraw

SIZE = 1024
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
ICONS = os.path.join(ROOT, "src-tauri", "icons")

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# --- fond : carré arrondi avec dégradé vertical ---
radius = 180
rect = (32, 32, SIZE - 32, SIZE - 32)
top = (30, 86, 168)
bottom = (18, 58, 122)

def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))

for y in range(rect[1], rect[3]):
    t = (y - rect[1]) / (rect[3] - rect[1])
    d.line([(rect[0] + radius, y), (rect[2] - radius, y)], fill=lerp(top, bottom, t) + (255,))

d.pieslice([rect[0], rect[1], rect[0] + 2 * radius, rect[1] + 2 * radius], 180, 270, fill=top + (255,))
d.pieslice([rect[2] - 2 * radius, rect[1], rect[2], rect[1] + 2 * radius], 270, 360, fill=top + (255,))
d.pieslice([rect[0], rect[3] - 2 * radius, rect[0] + 2 * radius, rect[3]], 90, 180, fill=bottom + (255,))
d.pieslice([rect[2] - 2 * radius, rect[3] - 2 * radius, rect[2], rect[3]], 0, 90, fill=bottom + (255,))

# --- horloge ---
cx, cy = 470, 420
r_out, r_in = 280, 205
d.ellipse([cx - r_out, cy - r_out, cx + r_out, cy + r_out], fill=(255, 255, 255, 255))
d.ellipse([cx - r_in, cy - r_in, cx + r_in, cy + r_in], fill=(233, 241, 250, 255))

def hand(angle_deg, length, width, color):
    rad = math.radians(angle_deg - 90)
    ex = cx + length * math.cos(rad)
    ey = cy + length * math.sin(rad)
    d.line([(cx, cy), (ex, ey)], fill=color, width=width)

hand(-40, 145, 52, (30, 86, 168))   # aiguille des heures
hand(25, 205, 40, (30, 86, 168))    # aiguille des minutes
d.ellipse([cx - 40, cy - 40, cx + 40, cy + 40], fill=(30, 86, 168, 255))

# --- coche verte (export/import validés) ---
gx, gy = 700, 700
gr = 195
d.ellipse([gx - gr, gy - gr, gx + gr, gy + gr], fill=(255, 255, 255, 255))
green = (62, 178, 92, 255)
d.line([(gx - 115, gy), (gx - 30, gy + 88), (gx + 150, gy - 82)],
       fill=green, width=54, joint="curve")

# --- écriture des fichiers ---
os.makedirs(ICONS, exist_ok=True)
img.save(os.path.join(ROOT, "app-icon.png"))

img.resize((32, 32), Image.LANCZOS).save(os.path.join(ICONS, "32x32.png"))
img.resize((128, 128), Image.LANCZOS).save(os.path.join(ICONS, "128x128.png"))
img.resize((256, 256), Image.LANCZOS).save(os.path.join(ICONS, "128x128@2x.png"))
img.save(os.path.join(ICONS, "icon.ico"),
         sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])

print("Icônes générées dans", ICONS)