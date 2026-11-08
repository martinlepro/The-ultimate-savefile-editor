#!/usr/bin/env python3
"""Génère les polices embarquées (assets/fonts) à partir des fichiers fournis.

  python3 tools/build_fonts.py <dossier_des_sources> <feuille_de_glyphes.png>

* sous-ensemble Latin-1 (+ quelques symboles) de chaque police -> binaire bien plus léger
* PixelifySans (variable) figé en 400 (texte) et 700 (titres)
* "blocks.ttf" : police vectorielle reconstruite depuis la feuille de glyphes en blocs
Dépendances : fonttools, pillow, numpy, scipy.
"""
import sys, os, numpy as np
from PIL import Image
from scipy import ndimage as ndi
from fontTools.ttLib import TTFont
from fontTools.subset import Subsetter, Options
from fontTools.varLib import instancer
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

SRC, SHEET = sys.argv[1], sys.argv[2]
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "fonts")
os.makedirs(OUT, exist_ok=True)
UNI = list(range(0x20, 0x7F)) + list(range(0xA0, 0x100)) + [0x2013, 0x2014, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2026, 0x20AC, 0x2665, 0x25B2, 0x25BC]


def subset(font, out, drop=()):
    o = Options(); o.layout_features = ["kern", "liga"]; o.hinting = False; o.notdef_outline = True
    s = Subsetter(o); s.populate(unicodes=[u for u in UNI if u not in drop]); s.subset(font)
    font.save(os.path.join(OUT, out))
    print(out, os.path.getsize(os.path.join(OUT, out)), "octets")


def find(name):
    for r, _, fs in os.walk(SRC):
        if name in fs:
            return os.path.join(r, name)
    raise SystemExit("introuvable: " + name)


def open_c(font, old, new):
    """Pixelify Sans (OFL) : l'encoche du « C » ne fait que 80 unités et le rend identique au « O »
    aux petites tailles. On raccourcit les deux crochets pour ouvrir la bouche (old -> new = (haut, bas))."""
    g = font["glyf"][font.getBestCmap()[ord("C")]]
    for i, (x, y) in enumerate(list(g.coordinates)):
        if y == old[0]: g.coordinates[i] = (x, new[0])
        elif y == old[1]: g.coordinates[i] = (x, new[1])
    return font


# --- Stardew : Pixelify Sans (variable) -> 400 / 700 ; Minecraft : VT323
pix = find("PixelifySans.ttf")
subset(open_c(instancer.instantiateVariableFont(TTFont(pix), {"wght": 400}), (350, 270), (440, 180)), "pixelify.ttf")
subset(open_c(instancer.instantiateVariableFont(TTFont(pix), {"wght": 700}), (331, 297), (450, 190)), "pixelify-bold.ttf")
subset(TTFont(find("VT323-Regular.ttf")), "vt323.ttf")
# --- Hollow Knight : Trajan Pro
subset(TTFont(find("TrajanPro-Regular.ttf")), "trajan.ttf")
subset(TTFont(find("TrajanPro-Bold.otf")), "trajan-bold.otf")
# --- Celeste
# les chiffres de CELESTE.otf sont des runes décoratives illisibles dans un champ numérique :
# on les retire du sous-ensemble, egui retombe alors sur Pixelify Sans pour 0-9
DIGITS = range(0x30, 0x3A)
subset(TTFont(find("CELESTE.otf")), "celeste.otf", DIGITS)
subset(TTFont(find("CELESTE-BOLD.otf")), "celeste-bold.otf", DIGITS)

# --- blocs : extraction des cellules de la plus grande des 3 feuilles
a = np.array(Image.open(SHEET).convert("RGB"))[:, 771:]
mask = (a[:, :, 0] >= 245) & (a[:, :, 1] >= 250) & (a[:, :, 2] >= 250)
lab, _ = ndi.label(mask)
cells = []
for i, o in enumerate(ndi.find_objects(lab)):
    h, w = o[0].stop - o[0].start, o[1].stop - o[1].start
    if w <= 18 and h <= 18 and (lab[o] == i + 1).sum() > 200:
        cells.append((o[1].start + w / 2, o[0].start + h / 2))
cells = np.array(cells); N = len(cells); P = 22.0
par = list(range(N))
def fnd(x):
    while par[x] != x: par[x] = par[par[x]]; x = par[x]
    return x
for i in range(N):
    for j in range(i + 1, N):
        if abs(cells[i][0] - cells[j][0]) <= 26 and abs(cells[i][1] - cells[j][1]) <= 26:
            par[fnd(i)] = fnd(j)
groups = {}
for i in range(N): groups.setdefault(fnd(i), []).append(i)
gl = []
for g in groups.values():
    p = cells[g]; gl.append((p[:, 1].min(), p[:, 0].min(), g))
gl.sort(key=lambda t: (round(t[0] / 150), t[1]))
BAND = [22, 198, 330, 462]
G = []
for y, x, g in gl:
    p = cells[g]; x0, y0 = p[:, 0].min(), p[:, 1].min()
    gx = np.round((p[:, 0] - x0) / P).astype(int); gy = np.round((p[:, 1] - y0) / P).astype(int)
    b = min(range(4), key=lambda i: abs(y - BAND[i]) if y >= BAND[i] - 5 else 1e9)
    G.append(dict(cells=list(zip(gx.tolist(), gy.tolist())), y=round((y - BAND[b]) / P), x=x))
assert len(G) == 73, len(G)

# index de glyphe -> caractères ; "+" = composition de plusieurs composants (même ligne)
MAP = {0: "$", 1: "(", 2: ")", 3: "[", 4: "]", 5: "|", 6: "Q", 7: "M", 8: "W", 9: "#", 10: "&"}
for i, c in zip(range(11, 22), "ABCDEFGHJKL"): MAP[i] = c
for i, c in zip(range(22, 33), "NOPRSTUVXYZ"): MAP[i] = c
for i, c in zip(range(33, 42), "234567890"): MAP[i] = c
MAP.update({43: "<", 44: ">", 45: "/", 46: "\\", 55: "+", 69: "-", 62: "~", 65: "^", 63: "'", 50: "I", 51: "1"})
UNIT, GAP = 100, 6
glyphs, adv, cmap, order = {}, {}, {}, [".notdef", "space"]


def box(pen, x0, y0, x1, y1):
    pen.moveTo((x0, y0)); pen.lineTo((x0, y1)); pen.lineTo((x1, y1)); pen.lineTo((x1, y0)); pen.closePath()


def add(name, cs, codes):
    pen = TTGlyphPen(None)
    for x, y in cs:  # y=0 : haut de capitale ; ligne de base = rangée 5
        box(pen, x * UNIT + GAP, (5 - y - 1) * UNIT + GAP, (x + 1) * UNIT - GAP, (5 - y) * UNIT - GAP)
    glyphs[name] = pen.glyph(); w = (max([c[0] for c in cs] + [0]) + 2) * UNIT
    adv[name] = w; order.append(name)
    for c in codes: cmap[ord(c)] = name


glyphs[".notdef"] = TTGlyphPen(None).glyph(); adv[".notdef"] = 500
pen = TTGlyphPen(None); glyphs["space"] = pen.glyph(); adv["space"] = 300; cmap[32] = "space"
for i, ch in MAP.items():
    add("g%d" % i, G[i]["cells"], [ch] + ([ch.lower()] if ch.isalpha() else []))
add("c63", G[42]["cells"] + [(1, 4)], ["?"])
add("c61", [(0, 1), (1, 1), (2, 1), (0, 3), (1, 3), (2, 3)], ["="])
add("c33", [(0, 0), (0, 1), (0, 2), (0, 4)], ["!"])
add("c58", [(0, 1), (0, 3)], [":", ";"])
add("c46", [(0, 4)], ["."])
add("c44", [(0, 4), (0, 5)], [","])
add("under", [(0, 5), (1, 5), (2, 5)], ["_"])
add("pct", [(0, 0), (2, 0), (1, 2), (0, 4), (2, 4)], ["%"])
add("star", [(1, 0), (0, 1), (2, 1), (1, 2), (0, 3), (2, 3), (1, 4)], ["*"])
add("at", [(0, 0), (1, 0), (2, 0), (0, 1), (2, 1), (0, 2), (1, 2), (2, 2), (0, 3), (0, 4), (1, 4), (2, 4)], ["@"])
add("quote", [(0, 0), (2, 0), (0, 1), (2, 1)], ['"'])
add("block", [(x, y) for x in range(3) for y in range(3)], ["■"])

fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(order); fb.setupCharacterMap(cmap); fb.setupGlyf(glyphs)
met = {n: (adv[n], GAP if glyphs[n].numberOfContours else 0) for n in order}
fb.setupHorizontalMetrics(met)
fb.setupHorizontalHeader(ascent=620, descent=-300)
fb.setupNameTable({"familyName": "SaveStudio Blocks", "styleName": "Regular"})
fb.setupOS2(sTypoAscender=620, sTypoDescender=-300, usWinAscent=620, usWinDescent=300, sxHeight=500, sCapHeight=500)
fb.setupPost(); fb.save(os.path.join(OUT, "blocks.ttf"))
print("blocks.ttf", os.path.getsize(os.path.join(OUT, "blocks.ttf")), "octets,", len(order), "glyphes")
