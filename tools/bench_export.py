#!/usr/bin/env python3
"""Times the export copy + zip, the old way and with ``ptk_engine.build``.

    python3 tools/bench_export.py WORKDIR [GAME_FOLDER]

Without a game folder, a synthetic ~500 MB game is made in WORKDIR (half
compressible data files, half already-compressed media). Three runs per
way: a first export, an export with nothing changed, and one with a single
file changed. Nothing outside WORKDIR is written.
"""

from __future__ import annotations

import os
import shutil
import sys
import time
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "engine"))

from ptk_engine.build import Build, can_clone, zip_dir  # noqa: E402


def synthetic(root: Path) -> Path:
    game = root / "game"
    if game.is_dir():
        return game
    rnd = os.urandom(1 << 20)
    for i in range(120):  # ~250 MB of data that compresses
        p = game / "Data" / f"level{i // 10}" / f"chunk{i}.assets"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes((b"unity serialized " * 64 + rnd[: 1 << 12]) * 500)
    for i in range(60):  # ~250 MB of media (random: does not compress)
        p = game / "Data" / "StreamingAssets" / f"track{i}.ogg"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(os.urandom(4 << 20))
    return game


def old_way(game: Path, out: Path) -> None:
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(game, out / "Game")
    with zipfile.ZipFile(out / "Game.zip", "w", zipfile.ZIP_DEFLATED, compresslevel=6) as zf:
        for p in sorted((out / "Game").rglob("*")):
            if p.is_file():
                zf.write(p, p.relative_to(out).as_posix())


def new_way(game: Path, out: Path) -> str:
    b = Build(out)
    b.add_tree(game, "Game")
    b.artifact("Game.zip")
    b.artifact("Game.zip.ptkindex")
    r = b.sync()
    z = zip_dir(out / "Game", out / "Game.zip")
    return f"{r.describe()}; zip {z['reused']} reused / {z['compressed']} compressed"


def timed(fn, *args) -> tuple[float, object]:
    t = time.perf_counter()
    r = fn(*args)
    return time.perf_counter() - t, r


def main(argv: list[str]) -> int:
    work = Path(argv[0]).resolve()
    work.mkdir(parents=True, exist_ok=True)
    game = Path(argv[1]).resolve() if len(argv) > 1 else synthetic(work)
    size = sum(p.stat().st_size for p in game.rglob("*") if p.is_file())
    print(f"game: {game} ({size / 1e6:.0f} MB), APFS clones: {can_clone()}")
    victim = next(p for p in sorted(game.rglob("*")) if p.is_file() and p.suffix != ".ogg")
    for name, fn, out in (("old", old_way, work / "out-old"), ("new", new_way, work / "out-new")):
        for label in ("first export", "nothing changed", "one file changed"):
            if label == "one file changed" and len(argv) < 2:
                victim.write_bytes(victim.read_bytes() + b"!")
            if label == "first export" and out.exists():
                shutil.rmtree(out)
            t, r = timed(fn, game, out)
            print(f"{name:3}  {label:17} {t:7.2f} s  {r or ''}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
