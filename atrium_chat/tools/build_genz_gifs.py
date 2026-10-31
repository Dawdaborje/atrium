#!/usr/bin/env python3
"""Build assets/genz/, original animated text reactions in Gen Z slang for the chat composer.

    python3 tools/build_genz_gifs.py

Needs Pillow and Noto Sans Black (SIL OFL 1.1). Nothing is downloaded: every GIF is drawn here, so there is nothing to
attribute. Output: assets/genz/<slug>.gif and assets/genz/pack.json, in the shape of assets/gif/pack.json.
"""
import colorsys
import json
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent.parent
OUT = HERE / "assets" / "genz"
FONT = "/usr/share/fonts/noto/NotoSans-Black.ttf"
W, H = 200, 125
FRAMES = 12
DELAY = 70  # ms

# text, words it can be found by, animation: pulse | shake | bounce | spin | wave
REACTIONS = [
    ("no cap", ["truth", "honest", "fr", "real"], "pulse"),
    ("cap", ["lie", "lying", "fake", "false"], "shake"),
    ("bruh", ["bro", "dude", "seriously", "disbelief"], "shake"),
    ("fr fr", ["for real", "agree", "true"], "pulse"),
    ("rizz", ["charm", "flirt", "charisma"], "wave"),
    ("W", ["win", "good", "yes", "congrats"], "bounce"),
    ("L", ["loss", "lose", "bad", "fail"], "shake"),
    ("slay", ["queen", "nailed", "great", "yass"], "pulse"),
    ("it's giving", ["vibe", "energy", "looks like"], "wave"),
    ("ate", ["killed it", "nailed", "great", "perfect"], "bounce"),
    ("delulu", ["delusional", "dream", "hope"], "wave"),
    ("sus", ["suspicious", "sketchy", "among us"], "shake"),
    ("bet", ["ok", "deal", "sure", "agreed"], "bounce"),
    ("lowkey", ["kinda", "secretly", "quiet"], "pulse"),
    ("highkey", ["really", "openly", "very"], "pulse"),
    ("yeet", ["throw", "toss", "gone"], "spin"),
    ("mid", ["average", "meh", "boring", "okay"], "wave"),
    ("based", ["agree", "cool", "own opinion"], "pulse"),
    ("ick", ["gross", "turnoff", "ew"], "shake"),
    ("main character", ["protagonist", "hero", "spotlight"], "bounce"),
    ("touch grass", ["go outside", "offline", "chill"], "wave"),
    ("skibidi", ["silly", "random", "toilet"], "spin"),
    ("gyatt", ["wow", "shock", "damn"], "bounce"),
    ("sigma", ["lone wolf", "alpha", "boss"], "pulse"),
    ("ratio", ["owned", "dunk", "more likes"], "shake"),
    ("vibe check", ["mood", "energy", "check"], "wave"),
    ("period", ["periodt", "end of story", "final"], "pulse"),
    ("simp", ["crush", "fan", "devoted"], "wave"),
    ("big yikes", ["cringe", "awkward", "oops"], "shake"),
    ("lit", ["fire", "awesome", "party"], "bounce"),
    ("glow up", ["improved", "transformation", "level up"], "pulse"),
    ("i'm dead", ["dying", "funny", "lol", "lmao"], "spin"),
    ("not me", ["relatable", "guilty", "oops"], "wave"),
    ("ok boomer", ["old", "dismiss", "whatever"], "shake"),
    ("say less", ["understood", "got it", "ok"], "bounce"),
]


def slug(text: str) -> str:
    return "".join(c if c.isalnum() else "-" for c in text.lower()).strip("-")


def fit(draw: ImageDraw.ImageDraw, text: str) -> ImageFont.FreeTypeFont:
    size = 80
    while size > 18:
        font = ImageFont.truetype(FONT, size)
        box = draw.textbbox((0, 0), text, font=font)
        if box[2] - box[0] <= W - 28:
            return font
        size -= 4
    return ImageFont.truetype(FONT, 18)


def frame(text: str, motion: str, i: int, hue: float) -> Image.Image:
    t = i / FRAMES
    h = (hue + t * 0.25) % 1
    top = tuple(int(c * 255) for c in colorsys.hsv_to_rgb(h, 0.75, 0.95))
    bottom = tuple(int(c * 255) for c in colorsys.hsv_to_rgb((h + 0.15) % 1, 0.8, 0.6))
    img = Image.new("RGB", (W, H))
    px = ImageDraw.Draw(img)
    for y in range(H):
        k = y / H
        px.line([(0, y), (W, y)], fill=tuple(int(top[c] * (1 - k) + bottom[c] * k) for c in range(3)))
    font = fit(px, text)
    layer = Image.new("RGBA", (W * 2, H * 2), (0, 0, 0, 0))
    ld = ImageDraw.Draw(layer)
    box = ld.textbbox((0, 0), text, font=font)
    tw, th = box[2] - box[0], box[3] - box[1]
    x, y = (W * 2 - tw) / 2 - box[0], (H * 2 - th) / 2 - box[1]
    ld.text((x + 4, y + 5), text, font=font, fill=(0, 0, 0, 120))
    ld.text((x, y), text, font=font, fill="white", stroke_width=3, stroke_fill=(20, 20, 30))
    a = t * 2 * math.pi
    scale, angle, dx, dy = 1.0, 0.0, 0.0, 0.0
    if motion == "pulse":
        scale = 1 + 0.12 * math.sin(a)
    elif motion == "shake":
        dx, angle = 7 * math.sin(a * 3), 5 * math.sin(a * 3)
    elif motion == "bounce":
        dy = -14 * abs(math.sin(a / 2))
        scale = 1 + 0.06 * math.sin(a)
    elif motion == "spin":
        angle = 360 * t
        scale = 0.9 + 0.1 * math.sin(a)
    elif motion == "wave":
        angle, dy = 8 * math.sin(a), 6 * math.sin(a * 2)
    layer = layer.resize((int(layer.width * scale), int(layer.height * scale)), Image.LANCZOS)
    layer = layer.rotate(angle, resample=Image.BICUBIC, expand=False)
    img.paste(layer, (int((W - layer.width) / 2 + dx), int((H - layer.height) / 2 + dy)), layer)
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    pack = []
    for n, (text, words, motion) in enumerate(REACTIONS):
        hue = (n * 0.137) % 1
        frames = [frame(text, motion, i, hue) for i in range(FRAMES)]
        name = slug(text)
        frames = [f.quantize(48, dither=Image.Dither.NONE) for f in frames]
        frames[0].save(OUT / f"{name}.gif", save_all=True, append_images=frames[1:], duration=DELAY, loop=0,
                       optimize=True, disposal=2)
        pack.append({"n": text, "f": f"genz/{name}.gif", "t": words, "s": [name.replace("-", "_")]})
    (OUT / "pack.json").write_text(json.dumps({"gifs": pack}, separators=(",", ":")) + "\n")
    (OUT / "NOTICE.txt").write_text(
        "Original text animations drawn by tools/build_genz_gifs.py; no third-party artwork.\n"
        "Lettering: Noto Sans Black, licensed under the SIL Open Font License 1.1 (https://openfontlicense.org).\n")
    print(len(pack), "GIFs,", sum(p.stat().st_size for p in OUT.glob("*.gif")) // 1024, "KB")


main()
