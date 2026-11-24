"""LÖVE (love2d) games: where the .love archive is and what conf.lua says.

A LÖVE game is a zip of Lua scripts and assets with ``main.lua`` at its top
(a ``.love`` file). Builds ship it in one of these forms:

- ``game.love`` next to (or without) the runtime;
- *fused*: the zip appended to ``love.exe`` (Windows) - zip readers find it
  from the end of the file, so ``zipfile`` opens the .exe directly;
- inside ``Something.app/Contents/Resources/`` (macOS);
- a plain folder with ``main.lua`` (the source).

``conf.lua`` is read with regular expressions, never run.
"""

from __future__ import annotations

import os
import re
import stat
import zipfile
from dataclasses import asdict, dataclass, field
from pathlib import Path


class LoveError(Exception):
    pass


@dataclass
class LoveGame:
    kind: str  # "love" | "fused" | "app" | "folder"
    source: str  # relative to the game folder ("" for the folder itself)
    root: str = ""  # folder inside the archive that holds main.lua (normally "")
    love_version: str | None = None
    identity: str | None = None
    title: str | None = None
    width: int | None = None
    height: int | None = None
    icon: str | None = None  # inside the game
    files: int = 0
    size: int = 0
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)


_CONF = {
    "love_version": r"""\bt\.version\s*=\s*["']([^"']+)["']""",
    "identity": r"""\bt\.identity\s*=\s*["']([^"']+)["']""",
    "title": r"""\bt\.window\.title\s*=\s*["']([^"']*)["']""",
    "width": r"""\bt\.window\.width\s*=\s*(\d+)""",
    "height": r"""\bt\.window\.height\s*=\s*(\d+)""",
    "icon": r"""\bt\.window\.icon\s*=\s*["']([^"']+)["']""",
}


def parse_conf(text: str) -> dict:
    """The settings a conf.lua sets with plain literals (comments ignored)."""
    text = re.sub(r"--\[(=*)\[.*?\]\1\]", "", text, flags=re.S)  # block comments
    text = re.sub(r"--[^\n]*", "", text)
    out: dict = {}
    for key, rx in _CONF.items():
        m = re.search(rx, text)
        if m:
            v = m.group(1)
            out[key] = int(v) if key in ("width", "height") else v
    return out


def _zip_root(names: list[str]) -> str | None:
    """The folder holding main.lua inside an archive ('' for the top)."""
    mains = sorted((n for n in names if n.rsplit("/", 1)[-1] == "main.lua"), key=lambda n: n.count("/"))
    if not mains:
        return None
    return mains[0].rsplit("/", 1)[0] if "/" in mains[0] else ""


def _from_zip(path: Path, kind: str, rel: str) -> LoveGame | None:
    try:
        z = zipfile.ZipFile(path)
    except (zipfile.BadZipFile, OSError):
        return None
    with z:
        names = [i.filename for i in z.infolist() if not i.is_dir()]
        root = _zip_root(names)
        if root is None:
            return None
        g = LoveGame(kind, rel, root, files=len(names), size=sum(i.file_size for i in z.infolist()))
        conf = f"{root}/conf.lua" if root else "conf.lua"
        if conf in names:
            g.__dict__.update(parse_conf(z.read(conf).decode("utf-8", "replace")))
        return g


def _from_folder(path: Path, rel: str) -> LoveGame:
    files = [p for p in path.rglob("*") if p.is_file() and not p.name.startswith("._")]
    g = LoveGame("folder", rel, files=len(files), size=sum(p.stat().st_size for p in files))
    conf = path / "conf.lua"
    if conf.is_file():
        g.__dict__.update(parse_conf(conf.read_text(encoding="utf-8", errors="replace")))
    return g


def detect(game: Path) -> LoveGame:
    """Finds the game's .love (or source folder) inside a build."""
    game = Path(game)
    base = game if game.is_dir() else game.parent

    def rel(p: Path) -> str:
        return "" if p == game and game.is_dir() else p.relative_to(base).as_posix()

    candidates: list[tuple[str, Path]] = []
    if game.is_file():
        candidates.append(("love" if game.suffix.lower() == ".love" else "fused", game))
    else:
        for p in sorted(game.rglob("*.love"), key=lambda p: (len(p.parts), p.name)):
            if p.is_file() and not p.name.startswith("._"):
                kind = "app" if any(part.endswith(".app") for part in p.relative_to(game).parts) else "love"
                candidates.append((kind, p))
        if (game / "main.lua").is_file():
            candidates.append(("folder", game))
        for p in sorted(game.iterdir()):
            if p.is_file() and p.suffix.lower() in (".exe", "") and os.access(p, os.R_OK) and zipfile.is_zipfile(p):
                candidates.append(("fused", p))
    for kind, p in candidates:
        g = _from_folder(p, rel(p)) if kind == "folder" else _from_zip(p, kind, rel(p))
        if g is not None:
            if g.root:
                g.notes.append(f"main.lua is inside {g.root}/ in the archive, not at its top")
            return g
    raise LoveError("no LÖVE game found: no .love file, fused .exe or folder with main.lua")


def read(game: Path, info: dict, name: str) -> bytes | None:
    """One file of the game (e.g. its icon)."""
    src = Path(game) if not info.get("source") else (Path(game) if Path(game).is_file() else Path(game) / info["source"])
    rel = f"{info['root']}/{name}" if info.get("root") else name
    if info["kind"] == "folder":
        p = src / rel
        return p.read_bytes() if p.is_file() else None
    try:
        with zipfile.ZipFile(src) as z:
            return z.read(rel)
    except (KeyError, zipfile.BadZipFile, OSError):
        return None


def materialize(game: Path, info: dict, dest: Path) -> Path:
    """The game's files in a folder (``dest``), with main.lua at its top."""
    src = Path(game) if Path(game).is_file() else Path(game) / info.get("source", "")
    dest.mkdir(parents=True, exist_ok=True)
    if info["kind"] == "folder":
        for p in sorted(src.rglob("*")):
            if p.is_file() and not p.is_symlink() and not p.name.startswith("._") and p.name != ".DS_Store":
                out = dest / p.relative_to(src)
                out.parent.mkdir(parents=True, exist_ok=True)
                out.write_bytes(p.read_bytes())
        return dest
    root = info.get("root") or ""
    with zipfile.ZipFile(src) as z:
        for i in z.infolist():
            if i.is_dir():
                continue
            name = i.filename
            if root:
                if not name.startswith(root + "/"):
                    continue
                name = name[len(root) + 1:]
            out = safe_join(dest, name)
            out.parent.mkdir(parents=True, exist_ok=True)
            with z.open(i) as f, open(out, "wb") as o:
                while chunk := f.read(1 << 20):
                    o.write(chunk)
    return dest


def write_love(folder: Path, dest: Path) -> Path:
    """Zips a game folder into a .love (main.lua at the top)."""
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for p in sorted(folder.rglob("*")):
            if p.is_file() and not p.is_symlink():
                info = zipfile.ZipInfo(p.relative_to(folder).as_posix(), date_time=(2020, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o644 << 16
                with open(p, "rb") as f, z.open(info, "w") as out:
                    while chunk := f.read(1 << 20):
                        out.write(chunk)
    return dest


def safe_join(base: Path, rel: str) -> Path:
    out = (base / rel).resolve()
    if base.resolve() != out and base.resolve() not in out.parents:
        raise LoveError(f"unsafe path in archive: {rel}")
    return out


def extract_runtime(archive: Path, dest: Path) -> None:
    """Unzips a runtime keeping executable bits and the symlinks of macOS
    frameworks (links must stay inside ``dest``)."""
    with zipfile.ZipFile(archive) as z:
        for i in z.infolist():
            if i.filename.startswith("__MACOSX/") or i.is_dir():
                continue
            out = safe_join(dest, i.filename)
            out.parent.mkdir(parents=True, exist_ok=True)
            mode = i.external_attr >> 16
            if stat.S_ISLNK(mode):
                target = z.read(i).decode("utf-8")
                if os.path.isabs(target):
                    raise LoveError(f"absolute link in archive: {i.filename}")
                safe_join(dest, str(Path(i.filename).parent / target))
                if out.is_symlink() or out.exists():
                    out.unlink()
                os.symlink(target, out)
                continue
            with z.open(i) as f, open(out, "wb") as o:
                while chunk := f.read(1 << 20):
                    o.write(chunk)
            if mode & 0o111:
                out.chmod(0o755)
