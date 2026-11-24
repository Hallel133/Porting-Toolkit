"""Ren'Py games: where the engine and the game are, which version, which
platforms the build already has.

A Ren'Py distribution is laid out like this (``<name>`` is the build's
executable name):

    <name>.exe / <name>.sh / <name>.py    launchers (Windows, Linux, all)
    game/                                 scripts (.rpyc), images, .rpa archives
    renpy/                                the engine (Python)
    lib/python3.9/ (lib/python2.7/)       the Python standard library
    lib/py3-windows-x86_64/ ...           native files per platform (py2- on Ren'Py 7)
    <name>.app/                           macOS: everything under Contents/Resources/autorun

Archives (``.rpa``) are only listed: their index is a zlib-compressed pickle,
read with an unpickler that refuses every class, so no code from the game
can run. Their contents are never extracted.
"""

from __future__ import annotations

import codecs
import io
import pickle
import re
import zlib
from dataclasses import asdict, dataclass, field
from pathlib import Path

PLATFORMS = ("windows-x86_64", "linux-x86_64", "mac-universal")


class RenpyError(Exception):
    pass


@dataclass
class RenpyGame:
    root: str  # folder with game/ and renpy/, relative to the game folder ("" = itself)
    version: str | None = None  # e.g. 8.3.7.25031702
    py: int = 3  # Python major version of the engine (Ren'Py 7: 2, Ren'Py 8: 3)
    executable_name: str = "game"
    python_lib: str | None = None  # relative to the game folder, e.g. lib/python3.9
    platforms: dict[str, str] = field(default_factory=dict)  # platform -> folder with its native files
    mac_app: str | None = None  # the .app, when the build is a macOS one
    title: str | None = None
    archives: list[dict] = field(default_factory=list)
    script_source: bool = False  # .rpy files are present
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)

    @property
    def short_version(self) -> str | None:
        """8.3.7 (the download name on renpy.org)."""
        if not self.version:
            return None
        return ".".join(self.version.split(".")[:3])


# -- archives -------------------------------------------------------------------


#: The only globals an index may name: constructors of plain values that
#: protocol-2 pickles use for bytes and strings. Nothing that can run code.
_SAFE_GLOBALS = {
    ("_codecs", "encode"): codecs.encode,
    ("__builtin__", "bytes"): bytes,
    ("builtins", "bytes"): bytes,
    ("__builtin__", "unicode"): str,
    ("__builtin__", "str"): str,
    ("builtins", "str"): str,
}


class _SafeUnpickler(pickle.Unpickler):
    """Plain data only (dicts, lists, tuples, numbers, strings, bytes)."""

    def find_class(self, module, name):
        try:
            return _SAFE_GLOBALS[(module, name)]
        except KeyError:
            raise RenpyError(f"the archive index refers to {module}.{name}; refusing it") from None


def read_rpa_index(path: Path) -> tuple[str, dict]:
    """(format, {name: [(offset, length, prefix)]}) of an .rpa archive."""
    with open(path, "rb") as f:
        head = f.readline(200)
        try:
            text = head.decode("ascii").strip()
        except UnicodeDecodeError as e:
            raise RenpyError(f"{path.name}: not a Ren'Py archive") from e
        parts = text.split()
        if not parts or not parts[0].startswith("RPA-"):
            raise RenpyError(f"{path.name}: not a Ren'Py archive")
        fmt = parts[0]
        if fmt == "RPA-3.0" and len(parts) >= 3:
            offset, key = int(parts[1], 16), int(parts[2], 16)
        elif fmt == "RPA-2.0" and len(parts) >= 2:
            offset, key = int(parts[1], 16), 0
        else:
            raise RenpyError(f"{path.name}: archive format {fmt} is not supported")
        f.seek(offset)
        raw = f.read(64 << 20)
    try:
        data = _SafeUnpickler(io.BytesIO(zlib.decompress(raw)), encoding="bytes").load()
    except (zlib.error, pickle.UnpicklingError, EOFError, ValueError) as e:
        raise RenpyError(f"{path.name}: unreadable archive index ({e})") from e
    if not isinstance(data, dict):
        raise RenpyError(f"{path.name}: unexpected archive index")
    index: dict[str, list[tuple[int, int, bytes]]] = {}
    for name, entries in data.items():
        if isinstance(name, bytes):
            name = name.decode("utf-8", "replace")
        out = []
        for e in entries:
            off, length = e[0] ^ key, e[1] ^ key
            prefix = e[2] if len(e) > 2 else b""
            out.append((off, length, prefix if isinstance(prefix, bytes) else str(prefix).encode("latin-1")))
        index[str(name)] = out
    return fmt, index


def write_rpa(path: Path, files: dict[str, bytes], key: int = 0x42424242) -> Path:
    """An RPA-3.0 archive (for tests; Ren'Py's own layout)."""
    body = io.BytesIO()
    body.write(b"RPA-3.0 0000000000000000 00000000\n")
    index = {}
    for name, data in files.items():
        off = body.tell()
        body.write(data)
        index[name] = [(off ^ key, len(data) ^ key, b"")]
    offset = body.tell()
    body.write(zlib.compress(pickle.dumps(index, protocol=2)))
    out = bytearray(body.getvalue())
    out[:34] = f"RPA-3.0 {offset:016x} {key:08x}\n".encode()
    path.write_bytes(bytes(out))
    return path


# -- the build --------------------------------------------------------------------


def parse_version(text: str) -> str | None:
    """From renpy/vc_version.py (``version = '8.3.7.25031702'``) or an older
    ``renpy/__init__.py`` (``version_tuple = (7, 4, 11, vc_version)``)."""
    m = re.search(r"""^version\s*=\s*['"](\d+(?:\.\d+)+)['"]""", text, re.M)
    if m:
        return m.group(1)
    m = re.search(r"version_tuple\s*=\s*\((\d+),\s*(\d+),\s*(\d+)", text)
    return ".".join(m.groups()) if m else None


def parse_title(text: str) -> str | None:
    """``define config.name = _("Title")`` from options.rpy."""
    m = re.search(r"""^\s*define\s+config\.name\s*=\s*_?\(?\s*(?:u|r)?(['"])(.+?)\1""", text, re.M)
    return m.group(2).strip() if m else None


def _roots(game: Path):
    """Folders that hold both game/ and renpy/, shallowest first."""
    if (game / "game").is_dir() and (game / "renpy").is_dir():
        yield game
    for depth in ("*", "*/*", "*/*/*", "*/*/*/*"):
        for p in sorted(game.glob(depth)):
            if p.is_dir() and (p / "game").is_dir() and (p / "renpy").is_dir():
                yield p


def detect(game: Path) -> RenpyGame:
    game = Path(game)
    if not game.is_dir():
        raise RenpyError("choose the game's folder (Ren'Py games are folders, or a Mac .app)")
    root = next(_roots(game), None)
    if root is None:
        raise RenpyError("no Ren'Py game found: no folder with both game/ and renpy/")
    rel = "" if root == game else root.relative_to(game).as_posix()
    g = RenpyGame(rel)

    version_text = ""
    for name in ("vc_version.py", "__init__.py"):
        p = root / "renpy" / name
        if p.is_file():
            version_text += p.read_text(encoding="utf-8", errors="replace") + "\n"
    g.version = parse_version(version_text)
    if g.version:
        g.py = 3 if int(g.version.split(".")[0]) >= 8 else 2

    app = next((p for p in [root, *root.parents] if p.suffix == ".app"), None)
    lib_bases = [root / "lib"]
    if app is not None:
        g.mac_app = app.relative_to(game).as_posix() if app != game else ""
        g.platforms["mac-universal"] = (app / "Contents" / "MacOS").relative_to(game).as_posix()
        lib_bases.append(app / "Contents" / "Resources" / "lib")
        exes = [p.name for p in (app / "Contents" / "MacOS").iterdir() if p.is_file() and not p.suffix]
        if exes:
            g.executable_name = sorted(exes, key=lambda n: (n in ("python", "pythonw", "zsync", "zsyncmake"), n))[0]
    else:
        launchers = sorted(p.stem for p in root.glob("*.py") if p.is_file())
        sh = sorted(p.stem for p in root.glob("*.sh") if p.is_file())
        exe = sorted(p.stem for p in root.glob("*.exe") if p.is_file())
        names = [n for n in launchers if n in sh or n in exe] or launchers or exe or sh
        if names:
            g.executable_name = names[0]
    for base in lib_bases:
        if not base.is_dir():
            continue
        for d in sorted(base.iterdir()):
            if d.is_dir() and re.fullmatch(r"python[23]\.\d+", d.name) and g.python_lib is None:
                g.python_lib = d.relative_to(game).as_posix()
            for plat in PLATFORMS:
                if d.name == f"py{g.py}-{plat}" and plat not in g.platforms:
                    g.platforms[plat] = d.relative_to(game).as_posix()

    opts = root / "game" / "options.rpy"
    if opts.is_file():
        g.title = parse_title(opts.read_text(encoding="utf-8", errors="replace"))
    g.script_source = any((root / "game").rglob("*.rpy"))
    for rpa in sorted((root / "game").rglob("*.rpa")):
        entry = {"path": rpa.relative_to(root).as_posix(), "size": rpa.stat().st_size}
        try:
            fmt, index = read_rpa_index(rpa)
            entry.update(format=fmt, files=len(index))
        except (RenpyError, OSError) as e:
            entry.update(format=None, error=str(e))
        g.archives.append(entry)
    if g.version is None:
        g.notes.append("The Ren'Py version was not found (renpy/vc_version.py); exports need --set renpy_version=X.Y.Z")
    elif g.py == 2 and int(g.version.split(".")[1]) < 5:
        g.notes.append(f"Ren'Py {g.short_version} predates the py2- library layout (7.5); exports may need "
                       "the game's own files for each platform")
    return g
