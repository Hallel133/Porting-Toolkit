"""``ptk scan``: what is in a game folder.

Walks the folder without following symbolic links, classifies files by name
and extension, reads the first four bytes of likely executables, and detects
the engine. It never reads or decodes game archives.
"""

from __future__ import annotations

import os
from collections.abc import Callable
from pathlib import Path

from . import EngineError
from .categories import CATEGORY_IDS, classify
from .signatures import GameIndex, detect, guess_platforms

JUNK_NAMES = {".DS_Store", "Thumbs.db", "desktop.ini"}
MAGIC_EXTENSIONS = {"", "exe", "dll", "so", "dylib", "bin", "x86_64", "pck", "arm64"}
MAX_MAGIC_READS = 4000


def is_junk(name: str) -> bool:
    return name in JUNK_NAMES or name.startswith("._")


def read_magic(path: str) -> str | None:
    """'pe', 'elf' or 'mach-o' from the first four bytes, else None."""
    try:
        with open(path, "rb") as f:
            head = f.read(4)
    except OSError:
        return None
    if head == b"GDPC":
        return "godot-pck"
    if head[:2] == b"MZ":
        return "pe"
    if head == b"\x7fELF":
        return "elf"
    if head in (
        b"\xcf\xfa\xed\xfe",
        b"\xce\xfa\xed\xfe",
        b"\xfe\xed\xfa\xcf",
        b"\xfe\xed\xfa\xce",
        b"\xca\xfe\xba\xbe",  # universal binary
    ):
        return "mach-o"
    return None


def has_embedded_pck(path: str) -> bool:
    """True if a Godot export appended its .pck to this executable: the
    file then ends with the 4-byte magic `GDPC` (a standalone .pck starts
    with it). Only these 4 bytes are read; the pack is never decoded."""
    try:
        with open(path, "rb") as f:
            f.seek(0, os.SEEK_END)
            if f.tell() < 8:
                return False
            f.seek(-4, os.SEEK_END)
            return f.read(4) == b"GDPC"
    except OSError:
        return False


def _ext(name: str) -> str:
    return name.rsplit(".", 1)[1].lower() if "." in name[1:] else ""


def scan(
    path: str,
    *,
    max_files: int | None = None,
    progress: Callable[[int], None] | None = None,
) -> dict:
    root = Path(path).expanduser()
    if not root.exists():
        raise EngineError(f"{path} does not exist")
    root = root.resolve()

    files: list[tuple[str, int]] = []
    dirs: list[str] = []
    magic: dict[str, str] = {}
    embedded: list[str] = []
    symlinks = unreadable = magic_reads = 0
    truncated = False

    # A single file (a Windows .exe, a Linux binary, a .pck) is scanned as a
    # folder holding only that file.
    if root.is_file():
        single = root
        root = root.parent
        stack: list[tuple[str, str]] = []
        files.append((single.name, single.stat().st_size))
        kind = read_magic(str(single))
        if kind:
            magic[single.name] = kind
        if kind in ("pe", "elf", "mach-o") and has_embedded_pck(str(single)):
            embedded.append(single.name)
    else:
        single = None
        stack = [("", str(root))]
    while stack and not truncated:
        rel_dir, abs_dir = stack.pop()
        try:
            with os.scandir(abs_dir) as it:
                entries = sorted(it, key=lambda e: e.name)
        except OSError:
            unreadable += 1
            continue
        for entry in entries:
            name = entry.name
            if is_junk(name):
                continue
            rel = f"{rel_dir}/{name}" if rel_dir else name
            try:
                if entry.is_symlink():
                    symlinks += 1
                    continue
                if entry.is_dir(follow_symlinks=False):
                    dirs.append(rel)
                    stack.append((rel, entry.path))
                    continue
                if not entry.is_file(follow_symlinks=False):
                    continue
                size = entry.stat(follow_symlinks=False).st_size
            except OSError:
                unreadable += 1
                continue
            files.append((rel, size))
            if _ext(name) in MAGIC_EXTENSIONS and magic_reads < MAX_MAGIC_READS:
                magic_reads += 1
                kind = read_magic(entry.path)
                if kind:
                    magic[rel] = kind
                if kind in ("pe", "elf", "mach-o") and has_embedded_pck(entry.path):
                    embedded.append(rel)
            if progress and len(files) % 2000 == 0:
                progress(len(files))
            if max_files and len(files) >= max_files:
                truncated = True
                break

    index = GameIndex([f for f, _ in files], dirs, magic, embedded)
    engines = detect(index)

    per_cat = {c: {"id": c, "files": 0, "size": 0} for c in CATEGORY_IDS}
    classified = []
    for rel, size in files:
        cat = classify(rel, magic.get(rel))
        per_cat[cat]["files"] += 1
        per_cat[cat]["size"] += size
        classified.append((rel, size, cat))
    largest = sorted(classified, key=lambda x: (-x[1], x[0]))[:5]

    shown = single if single is not None else root
    return {
        "path": str(shown),
        "name": shown.name,
        "single_file": single is not None,
        "file_count": len(files),
        "dir_count": len(dirs),
        "total_size": sum(s for _, s in files),
        "symlinks_skipped": symlinks,
        "unreadable": unreadable,
        "truncated": truncated,
        "engines": engines,
        "primary": engines[0] if engines else None,
        "platforms": guess_platforms(index),
        "categories": [c for c in per_cat.values() if c["files"]],
        "largest": [{"path": p, "size": s, "category": c} for p, s, c in largest if s > 0],
    }
