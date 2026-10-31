#!/usr/bin/env python3
"""Build assets/gif/, the GIFs the chat composer offers: the animated Noto emoji (Google, CC BY 4.0,
https://googlefonts.github.io/noto-emoji-animation/), shrunk to small GIFs, a few from every group of the emoji pack.

    python3 tools/build_gifs.py [CACHE_DIR]

Needs ffmpeg and network access. The emoji come from assets/emoji/pack.json (run tools/build_emoji.py first), so a GIF
carries the same name, search words and shortcodes as its emoji. CACHE_DIR keeps the downloads between runs.

Output: assets/gif/<codepoints>.gif and assets/gif/pack.json = { "gifs": [{ "n": name, "f": "gif/<codepoints>.gif",
"t": [words], "s": [shortcodes] }] }.
"""
import json
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
EMOJI = HERE / "assets" / "emoji" / "pack.json"
OUT = HERE / "assets" / "gif"
URL = "https://fonts.gstatic.com/s/e/notoemoji/latest/{code}/512.gif"
SIZE = 96           # pixels, square
FPS = 10
MAX_ONE = 75_000    # bytes: a GIF larger than this is left out
BUDGET = 5_000_000  # bytes: the whole pack
NOTICE = f"""Animated emoji: Noto Emoji Animation, (c) Google, licensed under Creative Commons Attribution 4.0 International (CC BY 4.0):
https://creativecommons.org/licenses/by/4.0/
Source: https://googlefonts.github.io/noto-emoji-animation/

The files here are the originals scaled to {SIZE} x {SIZE} pixels, at {FPS} frames per second, with a reduced palette
(tools/build_gifs.py). Names and search words come from Emojibase (MIT), see ../emoji/LICENSE-emojibase.
"""


def codes(char: str) -> list[str]:
    """The names Noto may use for this emoji: with and without the emoji-presentation selector."""
    full = [f"{ord(c):x}" for c in char]
    plain = [c for c in full if c != "fe0f"]
    return list(dict.fromkeys(["_".join(full), "_".join(plain)]))


def exists(url: str) -> bool:
    try:
        with urllib.request.urlopen(urllib.request.Request(url, method="HEAD"), timeout=30) as reply:
            return reply.status == 200
    except urllib.error.HTTPError:
        return False
    except (urllib.error.URLError, TimeoutError):
        return False


def find(emoji: dict) -> str | None:
    for code in codes(emoji["c"]):
        if exists(URL.format(code=code)):
            return code
    return None


def fetch(code: str, cache: Path) -> Path:
    raw = cache / f"{code}.512.gif"
    if not raw.exists():
        with urllib.request.urlopen(URL.format(code=code), timeout=60) as reply:
            raw.write_bytes(reply.read())
    small = cache / f"{code}.{SIZE}.gif"
    if not small.exists():
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-i", str(raw), "-vf",
             f"fps={FPS},scale={SIZE}:{SIZE}:flags=lanczos,split[a][b];[a]palettegen=reserve_transparent=1:max_colors=64[p];"
             f"[b][p]paletteuse=dither=none", "-loop", "0", str(small)],
            check=True,
        )
    return small


def main() -> None:
    if not shutil.which("ffmpeg"):
        sys.exit("ffmpeg is needed")
    cache = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(tempfile.mkdtemp(prefix="noto-gif-"))
    cache.mkdir(parents=True, exist_ok=True)
    emoji = json.loads(EMOJI.read_text())["emoji"]
    print(f"looking for animated versions of {len(emoji)} emoji...", flush=True)
    with ThreadPoolExecutor(24) as pool:
        found = [(e, c) for e, c in zip(emoji, pool.map(find, emoji)) if c]
    print(f"{len(found)} are animated; downloading and shrinking...", flush=True)
    with ThreadPoolExecutor(6) as pool:
        files = list(pool.map(lambda ec: fetch(ec[1], cache), found))
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    # Take from every group in turn, so the pack is not all faces: the budget runs out across the whole range.
    by_group: dict[int, list] = {}
    for (e, code), path in zip(found, files):
        if path.stat().st_size <= MAX_ONE:
            by_group.setdefault(e["g"], []).append((e, code, path))
    queues = [by_group[g] for g in sorted(by_group)]
    gifs, total, turn = [], 0, 0
    while any(queues):
        queue = queues[turn % len(queues)]
        turn += 1
        if not queue:
            continue
        e, code, path = queue.pop(0)
        size = path.stat().st_size
        if total + size > BUDGET:
            continue
        shutil.copyfile(path, OUT / f"{code}.gif")
        total += size
        row = {"n": e["n"], "f": f"gif/{code}.gif"}
        if e.get("t"):
            row["t"] = e["t"]
        if e.get("s"):
            row["s"] = e["s"]
        gifs.append(row)
    (OUT / "pack.json").write_text(json.dumps({"gifs": gifs}, ensure_ascii=False, separators=(",", ":")))
    (OUT / "NOTICE.txt").write_text(NOTICE)
    print(f"{len(gifs)} GIFs, {total // 1024} KB -> {OUT}")


main()
