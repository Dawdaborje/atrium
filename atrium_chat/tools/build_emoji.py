#!/usr/bin/env python3
"""Build assets/emoji/pack.json, the emoji the chat composer offers, from Emojibase (https://github.com/milesj/emojibase).

    python3 tools/build_emoji.py            # download from GitHub, then build
    python3 tools/build_emoji.py DIR        # build from files already in DIR (compact.json, github.json, messages.json)

Output: { "groups": [{ "key", "name" }], "emoji": [{ "c": char, "n": name, "t": [tags], "s": [shortcodes], "g": group, "k": [five skin tones] }] }
"""
import json
import sys
import urllib.request
from pathlib import Path

BASE = "https://raw.githubusercontent.com/milesj/emojibase/master/packages/data/en"
FILES = {"compact.json": "compact.raw.json", "github.json": "shortcodes/github.raw.json", "messages.json": "messages.raw.json"}
OUT = Path(__file__).resolve().parent.parent / "assets" / "emoji" / "pack.json"
SKIP_GROUPS = {2}  # "components": skin-tone swatches and hair styles, not emoji to send


def load(folder: Path | None) -> dict:
    out = {}
    for local, remote in FILES.items():
        if folder:
            out[local] = json.loads((folder / local).read_text())
        else:
            with urllib.request.urlopen(f"{BASE}/{remote}", timeout=60) as reply:
                out[local] = json.load(reply)
    return out


def main() -> None:
    src = load(Path(sys.argv[1]) if len(sys.argv) > 1 else None)
    codes = src["github.json"]
    groups = sorted((g for g in src["messages.json"]["groups"] if g["order"] not in SKIP_GROUPS), key=lambda g: g["order"])
    index = {g["order"]: i for i, g in enumerate(groups)}
    emoji = []
    for e in sorted(src["compact.json"], key=lambda e: e.get("order", 0)):
        if e.get("group") not in index:
            continue
        short = codes.get(e["hexcode"], [])
        row = {"c": e["unicode"], "n": e["label"], "g": index[e["group"]]}
        if e.get("tags"):
            row["t"] = e["tags"]
        if short:
            row["s"] = [short] if isinstance(short, str) else short
        skins = e.get("skins", [])
        if len(skins) == 5:  # people with five tones; pairs of people have 25 and are left out
            row["k"] = [s["unicode"] for s in skins]
        emoji.append(row)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({"groups": [{"key": g["key"], "name": g["message"]} for g in groups], "emoji": emoji}, ensure_ascii=False, separators=(",", ":")))
    print(f"{len(emoji)} emoji in {len(groups)} groups -> {OUT} ({OUT.stat().st_size // 1024} KB)")


main()
