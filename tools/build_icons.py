#!/usr/bin/env python3
"""Génère assets/icons.txt (icônes pixel-art 12x12, dessinées pour ce projet) + un aperçu PNG."""
import math, sys, os
from PIL import Image
N = 12
PAL = dict(k="2b1a0e", w="fff6dc", r="d8402e", R="9e2a20", y="f6d445", o="ec9a2e", b="9a6330", B="5e3a1c", n="dcb67f",
           g="4fae3e", G="2f7a2c", l="9ad84f", c="52c4ee", d="2f72d8", D="20449a", p="f08bb0", s="cfd3d8", a="8d939b", v="8e5cd6", t="3a3a46")
icons = {}

def rows(name, *r):
    assert len(r) == N, f"{name}: {len(r)} lignes"
    for i, x in enumerate(r):
        assert len(x) == N, f"{name} ligne {i}: {len(x)} car. « {x} »"
    icons[name] = list(r)

def canvas():
    return [["."] * N for _ in range(N)]

def put(c, x, y, ch):
    if 0 <= x < N and 0 <= y < N: c[y][x] = ch

def disc(c, cx, cy, r, fill, edge="k"):
    for y in range(N):
        for x in range(N):
            d = math.hypot(x - cx, y - cy)
            if d <= r: c[y][x] = edge if d > r - 1 else fill

def line(c, x0, y0, x1, y1, ch):
    n = max(abs(x1 - x0), abs(y1 - y0))
    for i in range(n + 1):
        t = i / n if n else 0
        put(c, round(x0 + (x1 - x0) * t), round(y0 + (y1 - y0) * t), ch)

def done(name, c):
    icons[name] = ["".join(r) for r in c]

rows("house", ".....kk.....", "....krrk....", "...krrrrk...", "..krrrrrrk..", ".krrrrrrrrk.", "kkkkkkkkkkkk", ".knnnnnnnnk.", ".knwwnnbbnk.", ".knwwnnbbnk.", ".knnnnnbbnk.", ".knnnnnbbnk.", ".kkkkkkkkkk.")
c = canvas(); disc(c, 5.5, 5.5, 5.6, "y"); disc(c, 5.5, 5.5, 3.6, "o", "o"); disc(c, 5.5, 5.5, 2.6, "y", "y"); put(c, 4, 4, "w"); done("coin", c)
rows("skull", "...kkkkkk...", "..kwwwwwwk..", ".kwwwwwwwwk.", ".kwwwwwwwwk.", ".kwkkwwkkwk.", ".kwkkwwkkwk.", ".kwwwwwwwwk.", "..kwwkkwwk..", "...kwwwwk...", "...kwkwkwk..", "...kkkkkkk..", "............")
c = canvas(); disc(c, 5.5, 5.5, 5.6, "a"); 
for i in range(8):
    a = i * math.pi / 4; put(c, round(5.5 + 5.2 * math.cos(a) - .5), round(5.5 + 5.2 * math.sin(a) - .5), "a")
disc(c, 5.5, 5.5, 4.4, "s"); disc(c, 5.5, 5.5, 1.9, "t", "k")
for i in range(8):
    a = i * math.pi / 4; x = round(5.5 + 5.3 * math.cos(a) - .5); y = round(5.5 + 5.3 * math.sin(a) - .5); put(c, x, y, "k")
done("gear", c)
rows("bag", "....kkkk....", "...kbbbbk...", "..kbbbbbbk..", ".kbbbbbbbbk.", ".kbbkkkkbbk.", ".kbkooookbk.", ".kbkoyyokbk.", ".kbbkkkkbbk.", ".kbbbbbbbbk.", ".kbBBBBBBbk.", "..kbbbbbbk..", "...kkkkkk...")
rows("pick", "..kkssssk...", ".ksaaaaask..", "ksa......ask", "ks.......sk.", "k.....bk..k.", ".....bk.....", "....bk......", "...bk.......", "..bk........", ".bk.........", ".kk.........", "............")
rows("star", ".....kk.....", ".....kyk....", "....kyyk....", "kkkkkyyykkkk", ".kyyyyyyyyk.", "..kyyyyyyk..", "...kyyyyk...", "...kyyyyk...", "..kyykkyyk..", "..kyk..kyk..", ".kyk....kyk.", ".kk......kk.")
rows("heart", "............", ".kkk....kkk.", "krrrk..krrrk", "krwrrkkrrrrk", "krwrrrrrrrrk", "krrrrrrrrrRk", ".krrrrrrrRk.", "..krrrrrRk..", "...krrrRk...", "....krRk....", ".....kk.....", "............")
c = canvas(); line(c, 2, 9, 9, 2, "s"); line(c, 3, 9, 9, 3, "a"); line(c, 1, 8, 4, 11, "b"); line(c, 0, 10, 2, 8, "k"); line(c, 2, 9, 1, 10, "y"); put(c, 10, 1, "w"); put(c, 11, 0, "k"); put(c, 10, 2, "a"); line(c, 2, 8, 4, 10, "o"); done("sword", c)
c = canvas(); disc(c, 5.5, 5.5, 5.6, "w"); line(c, 5, 5, 5, 2, "k"); line(c, 5, 5, 8, 6, "k"); put(c, 5, 5, "r"); done("clock", c)
rows("strawberry", "....GG.GG...", ".....GGG....", "...rrGGrr...", "..rrrrrrrr..", ".rrywrrywrr.", ".rrrrrrrrrr.", "..rrywrrrr..", "..rrrrrrrr..", "...rrywrr...", "....rrrr....", ".....rr.....", "............")
rows("diamond", "............", "...kkkkkk...", "..kcccwcck..", ".kcwccdcccdk", "kkkkkkkkkkkk", ".kcccdcdcck.", "..kccdcddk..", "...kcddck...", "....kdck....", ".....kk.....", "............", "............")
rows("flag", ".kk.........", ".kbkkkkkkk..", ".kbkrrrrrrk.", ".kbkrwrrrrk.", ".kbkrrrrrk..", ".kbkkkkkk...", ".kb.........", ".kb.........", ".kb.........", ".kb.........", "kkkkk.......", "............")
rows("mask", "..kkkkkkkk..", ".kwwwwwwwwk.", "kwwwwwwwwwwk", "kwwwwwwwwwwk", "kwkkkwwkkkwk", "kwkkkwwkkkwk", "kwwkwwwwkwwk", ".kwwwwwwwwk.", ".kwwwkkwwwk.", "..kwwwwwwk..", "...kkwwkk...", ".....kk.....")
rows("wing", "...........k", "..........kw", ".........kwk", "k.......kwwk", "kk.....kwwk.", "kwkk..kwwwk.", ".kwwkkwwwk..", "..kwwwwwwk..", "...kwwwwk...", "....kwwk....", ".....kk.....", "............")
rows("key", "..kkkk......", ".kyyyyk.....", "kyykkyyk....", "kyk..kyk....", "kyyk.kyk....", ".kyyyyk.....", "...kyk......", "...kyk......", "...kykkk....", "...kyyyk....", "...kykkk....", "...kkk......")
rows("charm", "...kkkkkk...", "..kvvvvvvk..", ".kvvwvvvvvk.", "kvvwvvvvvvvk", "kvvvvvvvvvvk", "kvvvvvvvvvvk", "kvvvvvvvvvvk", ".kvvvvvvvvk.", "..kvvvvvvk..", "...kvvvvk...", "....kvvk....", ".....kk.....")
rows("gem", "............", "..kkkkkkkk..", ".kgwggggggk.", "kgwggGgggGgk", "kkkkkkkkkkkk", ".kggGgGgGgk.", "..kgGgGgGk..", "...kgGgGk...", "....kgGk....", ".....kk.....", "............", "............")
rows("cube", "....kkkk....", "..kkllllkk..", ".kllllllllk.", "kllllllllllk", "kkkkkkkkkkkk", "kbbbbkbbbbbk", "kbbbbkbbbbbk", "kbbbbkbbbbbk", "kbbbbkbbbbbk", ".kbbbkbbbbk.", "..kbbkbbbk..", "...kkkkkk..."[:12])
c = canvas(); disc(c, 5.5, 5.5, 5.6, "d"); 
for (x0, y0, x1, y1) in [(2, 3, 5, 3), (1, 4, 4, 6), (3, 7, 5, 9), (7, 2, 9, 4), (6, 5, 10, 6), (7, 8, 9, 9)]: line(c, x0, y0, x1, y1, "g")
done("globe", c)
rows("map", "............", ".kkkkkkkkkk.", ".kwwkwwkwwk.", ".kwrkwwkwwk.", ".kwwkwbkwwk.", ".kwwkwwkgwk.", ".kwwkwwkwwk.", ".kwwkwwkwwk.", ".kkkkkkkkkk.", "............", "............", "............")
rows("steve", "kkkkkkkkkkkk", "kBBBBBBBBBBk", "kBBBBBBBBBBk", "knnnnnnnnnnk", "knwdnnnndwnk", "knddnnnnddnk", "knnnnbbnnnnk", "knnnbbbbnnnk", "knnnbkkbnnnk", "knnnnnnnnnnk", "kkkkkkkkkkkk", "............")
rows("chest", "............", "..kkkkkkkk..", ".kbbbbbbbbk.", "kbbbbbbbbbbk", "kkkkkkkkkkkk", "kbbbkyykbbbk", "kbbbkyykbbbk", "kbbbbkkbbbbk", "kbbbbbbbbbbk", "kBBBBBBBBBBk", "kkkkkkkkkkkk", "............")
rows("folder", "............", ".kkkk.......", "kyyyykkkkkk.", "kyyyyyyyyyyk", "kyyyyyyyyyyk", "koooooooooyk", "kooooooooook", "kooooooooook", "kooooooooook", "kooooooooook", ".kkkkkkkkkk.", "............")
rows("refresh", "............", "...kkkkk....", "..kwwwwwk.k.", ".kwk...kwkkk", ".kwk....kwwk", ".kwk.....kk.", ".kk.....kwk.", "kkkwk...kwk.", "kwwk....kwk.", ".kkk.kwwwk..", "....kkkkk...", "............")
rows("disk", "kkkkkkkkkkk.", "kddkwwwwkdkk", "kddkwwwwkddk", "kddkwkkwkddk", "kddkwwwwkddk", "kddddddddddk", "kdwwwwwwwwdk", "kdwkkkkkkwdk", "kdwwwwwwwwdk", "kdwkkkkkkwdk", "kkkkkkkkkkkk", "............")
rows("restore", "............", "....kkkk....", "..kkwwwwkk..", ".kwwkkkkwwk.", ".kwk....kwk.", "kwwk.kk.kwwk", "kwk..kk..kwk", ".kk..kkkk.kk", "..k..kkkk..k", "...k.......k", "....kkkkkkk.", "............")
rows("hammer", "...kkkkkk...", "..ksssssak..", "..ksssssak..", "..kkkkkkkk..", "....kbbk....", "....kbbk....", "....kbbk....", "....kbbk....", "....kbbk....", "....kbbk....", "....kkkk....", "............")
rows("leaf", ".........kkk", "......kkkgGk", "....kkgggGGk", "...kgggggGGk", "..kgglggGGk.", "..kglgggGGk.", ".kgggggGGk..", ".kgggGGGk...", "kkGGGGkk....", "kbkkkk......", "kk..........", "............")
rows("back", "............", "....k.......", "...kk.......", "..kwk.......", ".kwwkkkkkkk.", "kwwwwwwwwwwk", "kwwwwwwwwwwk", ".kwwkkkkkkk.", "..kwk.......", "...kk.......", "....k.......", "............")
rows("search", "..kkkk......", ".kwwwwk.....", "kwcccwwk....", "kwcwwcwk....", "kwcwwcwk....", "kwwcccwk....", ".kwwwwk.....", "..kkkkkk....", ".....kbbk...", "......kbbk..", ".......kkk..", "............")

here = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(here, "..", "assets", "icons.txt"), "w") as f:
    f.write("\n".join(n + "\n" + "\n".join(r) + "\n" for n, r in icons.items()))
# aperçu
S = 8; names = list(icons); W = 9
im = Image.new("RGBA", (W * (N + 2) * S, ((len(names) + W - 1) // W) * (N + 2) * S), (220, 190, 140, 255))
for i, nm in enumerate(names):
    ox, oy = (i % W) * (N + 2) * S + S, (i // W) * (N + 2) * S + S
    for y, r in enumerate(icons[nm]):
        for x, ch in enumerate(r):
            if ch != ".":
                col = tuple(int(PAL[ch][j:j + 2], 16) for j in (0, 2, 4)) + (255,)
                for dy in range(S):
                    for dx in range(S): im.putpixel((ox + x * S + dx, oy + y * S + dy), col)
im.save(sys.argv[1] if len(sys.argv) > 1 else "/tmp/icons_preview.png")
print(len(icons), "icônes")
