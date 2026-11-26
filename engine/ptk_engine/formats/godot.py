"""Godot ``.pck`` files: find, read the directory, and extract.

A Godot game keeps its scripts and assets in a ``.pck`` (or inside the
executable, with the pck appended and a ``<u64 size>GDPC`` trailer). The
same pck runs with the official export template of the same Godot version
on any platform, which is how the Godot plugins port a build without its
source. Only the header and the directory are read here; game files are
never decoded.

Supported pack formats: 1 (Godot 3.x), 2 (Godot 4.0-4.3), 3 (Godot 4.4-4.6),
4 (Godot 4.7+; same layout as 3, plus sparse bundles and patch entries).
"""

from __future__ import annotations

import re
import shutil
import struct
from dataclasses import dataclass, field
from pathlib import Path

MAGIC = b"GDPC"
DIR_ENCRYPTED = 1
REL_FILEBASE = 2
SPARSE_BUNDLE = 4  # format 4: files are stored next to the pck, not inside it
FILE_ENCRYPTED = 1
FILE_REMOVAL = 2  # format 4 patch: removes a file of an earlier pack
FILE_DELTA = 4  # format 4 patch: a delta against a file of an earlier pack
#: Highest pack format this reader understands (Godot's PACK_FORMAT_VERSION).
LATEST_FORMAT = 4


class PckError(Exception):
    pass


@dataclass
class Entry:
    path: str
    offset: int  # absolute, in the container file
    size: int
    flags: int = 0
    ofs_field: int = 0  # where the offset is stored (v1 rewriting)


@dataclass
class Pck:
    container: Path
    start: int
    size: int
    format: int
    version: tuple[int, int, int]
    flags: int = 0
    embedded: bool = False
    file_base: int = 0  # absolute
    files: list[Entry] = field(default_factory=list)
    v1_absolute: bool = False
    removed: list[str] = field(default_factory=list)  # format 4 patch entries

    @property
    def sparse(self) -> bool:
        return bool(self.flags & SPARSE_BUNDLE)

    @property
    def patch(self) -> bool:
        """A format 4 patch pack: it changes another pack instead of standing alone."""
        return bool(self.removed) or any(e.flags & FILE_DELTA for e in self.files)

    @property
    def version_string(self) -> str:
        major, minor, patch = self.version
        return f"{major}.{minor}" + (f".{patch}" if patch else "")

    @property
    def encrypted(self) -> bool:
        return bool(self.flags & DIR_ENCRYPTED) or any(e.flags & FILE_ENCRYPTED for e in self.files)

    def paths(self) -> list[str]:
        return [e.path for e in self.files]

    def uses_csharp(self) -> bool:
        return any(p.endswith(".cs") or "/.godot/mono/" in "/" + p for p in self.paths())

    def extensions(self) -> list[str]:
        return sorted(p for p in self.paths() if p.endswith(".gdextension"))

    def summary(self) -> dict:
        return {
            "format": self.format,
            "godot_version": self.version_string,
            "embedded": self.embedded,
            "encrypted": self.encrypted,
            "sparse": self.sparse,
            "patch": self.patch,
            "files": len(self.files),
            "size": self.size,
            "csharp": self.uses_csharp(),
            "extensions": self.extensions(),
        }


def _u32(b: bytes, o: int) -> int:
    return struct.unpack_from("<I", b, o)[0]


def _u64(b: bytes, o: int) -> int:
    return struct.unpack_from("<Q", b, o)[0]


def locate(path: Path) -> tuple[int, int, bool] | None:
    """(start, size, embedded) of a pck in ``path``, or None."""
    try:
        total = path.stat().st_size
        with open(path, "rb") as f:
            if f.read(4) == MAGIC:
                return 0, total, False
            if total < 16:
                return None
            f.seek(total - 12)
            tail = f.read(12)
    except OSError:
        return None
    if tail[8:] != MAGIC:
        return None
    size = _u64(tail, 0)
    start = total - 12 - size
    if start < 0:
        return None
    with open(path, "rb") as f:
        f.seek(start)
        if f.read(4) != MAGIC:
            return None
    return start, size, True


def read(path: Path) -> Pck:
    """Reads the header and directory of the pck in ``path``."""
    loc = locate(path)
    if loc is None:
        raise PckError(f"{path.name} does not contain a Godot pck")
    start, size, embedded = loc
    with open(path, "rb") as f:
        f.seek(start)
        head = f.read(min(size, 1 << 16))
        fmt = _u32(head, 4)
        version = (_u32(head, 8), _u32(head, 12), _u32(head, 16))
        pck = Pck(path, start, size, fmt, version, embedded=embedded)
        if fmt == 1:
            dir_pos = start + 20 + 64
        elif fmt in (2, 3, 4):
            pck.flags = _u32(head, 20)
            base = _u64(head, 24)
            if fmt >= 3:
                pck.file_base = start + base
                dir_pos = start + _u64(head, 32)
            else:
                pck.file_base = start + base if pck.flags & REL_FILEBASE else base
                dir_pos = start + 32 + 64
        else:
            raise PckError(f"pck format {fmt} (Godot {pck.version_string}) is newer than this Porting Toolkit "
                           f"understands (formats 1-{LATEST_FORMAT}); update Porting Toolkit")
        if pck.flags & DIR_ENCRYPTED:
            return pck  # the directory itself is encrypted
        f.seek(dir_pos)
        count = struct.unpack("<I", f.read(4))[0]
        if count > 5_000_000:
            raise PckError("pck directory is damaged")
        for _ in range(count):
            n = struct.unpack("<I", f.read(4))[0]
            name = f.read(n).split(b"\0", 1)[0].decode("utf-8", "replace")
            ofs_field = f.tell()
            ofs, fsize = struct.unpack("<QQ", f.read(16))
            f.read(16)  # md5
            flags = struct.unpack("<I", f.read(4))[0] if fmt >= 2 else 0
            name = name.removeprefix("res://")
            if flags & FILE_REMOVAL:
                pck.removed.append(name)
                continue
            pck.files.append(Entry(name, ofs, fsize, flags, ofs_field))
    if fmt == 1:
        _fix_v1_offsets(pck)
    else:
        for e in pck.files:
            e.offset += pck.file_base
    return pck


def _fix_v1_offsets(pck: Pck) -> None:
    """Godot 3 offsets are relative to the pck, or (some embedded builds) to the file."""
    end = pck.start + pck.size

    def fits(base: int) -> bool:
        return all(pck.start <= base + e.offset and base + e.offset + e.size <= end for e in pck.files)

    if fits(pck.start):
        base = pck.start
    elif fits(0):
        base, pck.v1_absolute = 0, True
    else:
        raise PckError("pck file offsets point outside the pack")
    for e in pck.files:
        e.offset += base


def extract(pck: Pck, dest: Path) -> Path:
    """Writes the pck as a standalone ``.pck`` file at ``dest``."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    if not pck.embedded and pck.start == 0:
        shutil.copyfile(pck.container, dest)
        return dest
    with open(pck.container, "rb") as src, open(dest, "wb") as out:
        src.seek(pck.start)
        left = pck.size
        while left:
            chunk = src.read(min(left, 1 << 22))
            if not chunk:
                raise PckError("the executable ends before its pck does")
            out.write(chunk)
            left -= len(chunk)
    if pck.format == 2 and not pck.flags & REL_FILEBASE:
        with open(dest, "r+b") as out:
            out.seek(24)
            out.write(struct.pack("<Q", pck.file_base - pck.start))
    elif pck.format == 1 and pck.v1_absolute:
        with open(dest, "r+b") as out:
            for e in pck.files:
                out.seek(e.ofs_field - pck.start)
                out.write(struct.pack("<Q", e.offset - pck.start))
    return dest


def find(game: Path) -> list[Path]:
    """Files under ``game`` that contain a pck, best first.

    A standalone ``*.pck`` wins over an executable with an embedded one.
    Inside a ``.app``, ``Contents/Resources/*.pck`` is used.
    """
    if game.is_file():
        return [game] if locate(game) else []
    standalone: list[Path] = []
    embedded: list[Path] = []
    for p in sorted(game.rglob("*")):
        if not p.is_file() or p.is_symlink() or p.name.startswith("._"):
            continue
        if p.suffix.lower() == ".pck":
            if locate(p):
                standalone.append(p)
        elif p.suffix.lower() in (".exe", ".x86_64", ".x86_32", ".arm64", "") or p.parent.name == "MacOS":
            try:
                if p.stat().st_size > 64 and locate(p):
                    embedded.append(p)
            except OSError:
                pass
    by_size = lambda p: -p.stat().st_size  # noqa: E731
    return sorted(standalone, key=by_size) + sorted(embedded, key=by_size)


def write_pck(dest: Path, files: dict[str, bytes], version=(4, 4, 0), fmt: int = 3,
              embed_into: bytes | None = None, v1_absolute: bool = False,
              pack_flags: int = 0, file_flags: dict[str, int] | None = None) -> Path:
    """Writes a pck (tests and fixtures). With ``embed_into``, appends it to
    those executable bytes with the ``GDPC`` trailer, like Godot does."""
    import hashlib

    prefix = embed_into or b""
    names = list(files)
    header_len = {1: 20 + 64, 2: 32 + 64, 3: 40 + 64, 4: 40 + 64}[fmt]

    def entry_len(name: str) -> int:
        raw = len(name.encode())
        padded = raw + (-raw % 4)
        return 4 + padded + 16 + 16 + (4 if fmt >= 2 else 0)

    dir_len = 4 + sum(entry_len(n) for n in names)
    data_start = header_len + (dir_len if fmt in (1, 2) else 0)
    data_start += -data_start % 16
    blobs = bytearray()
    offsets = []
    for n in names:
        offsets.append(len(blobs))
        blobs += files[n]
        blobs += b"\0" * (-len(blobs) % 16)
    directory = bytearray(struct.pack("<I", len(names)))
    for n, ofs in zip(names, offsets):
        raw = n.encode()
        padded = raw + b"\0" * (-len(raw) % 4)
        if fmt == 1:
            value = ofs + data_start + (len(prefix) if v1_absolute else 0)
        else:
            value = ofs
        directory += struct.pack("<I", len(padded)) + padded + struct.pack("<QQ", value, len(files[n]))
        directory += hashlib.md5(files[n]).digest()
        if fmt >= 2:
            directory += struct.pack("<I", (file_flags or {}).get(n, 0))
    head = bytearray(MAGIC + struct.pack("<IIII", fmt, *version))
    if fmt == 1:
        head += b"\0" * 64
        body = bytes(head) + bytes(directory)
        body += b"\0" * (data_start - len(body)) + bytes(blobs)
    elif fmt == 2:
        head += struct.pack("<IQ", REL_FILEBASE, data_start) + b"\0" * 64
        body = bytes(head) + bytes(directory)
        body += b"\0" * (data_start - len(body)) + bytes(blobs)
    else:
        dir_offset = data_start + len(blobs)
        head += struct.pack("<IQQ", REL_FILEBASE | pack_flags, data_start, dir_offset) + b"\0" * 64
        body = bytes(head) + b"\0" * (data_start - len(head)) + bytes(blobs) + bytes(directory)
    if prefix:
        body = prefix + body + struct.pack("<Q", len(body)) + MAGIC
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(body)
    return dest


# -- project settings ----------------------------------------------------------

#: Variant type ids (Godot 4 / Godot 3) that project settings use.
_STRING = 4
_COLOR = {4: 20, 3: 14}


def read_file(pck: Pck, path: str) -> bytes | None:
    """One unencrypted file from the pck (``res://`` optional), or None."""
    path = path.removeprefix("res://")
    entry = next((e for e in pck.files if e.path == path), None)
    if entry is None or entry.flags & FILE_ENCRYPTED or entry.size > 64 << 20:
        return None
    with open(pck.container, "rb") as f:
        f.seek(entry.offset)
        return f.read(entry.size)


def parse_settings(data: bytes, major: int = 4) -> dict:
    """Decodes ``project.binary`` (``ECFG``): strings, numbers, bools and
    colors; other values are skipped (each one is length-prefixed)."""
    if data[:4] != b"ECFG":
        raise PckError("project.binary has no ECFG header")
    out: dict = {}
    count = _u32(data, 4)
    o = 8
    for _ in range(count):
        klen = _u32(data, o)
        key = data[o + 4:o + 4 + klen].decode("utf-8", "replace")
        o += 4 + klen
        vlen = _u32(data, o)
        v = data[o + 4:o + 4 + vlen]
        o += 4 + vlen
        if len(v) < 4:
            continue
        header = _u32(v, 0)
        kind, wide = header & 0xFFFF, bool(header & (1 << 16))
        try:
            if kind == 1:
                out[key] = bool(_u32(v, 4))
            elif kind == 2:
                out[key] = struct.unpack_from("<q" if wide else "<i", v, 4)[0]
            elif kind == 3:
                out[key] = struct.unpack_from("<d" if wide else "<f", v, 4)[0]
            elif kind == _STRING:
                n = _u32(v, 4)
                out[key] = v[8:8 + n].decode("utf-8", "replace")
            elif kind == _COLOR.get(major, 20):
                out[key] = list(struct.unpack_from("<ffff", v, 4))
        except struct.error:
            continue
    return out


def project_settings(pck: Pck) -> dict:
    """The game's project settings (empty if they cannot be read)."""
    raw = read_file(pck, "project.binary")
    if raw:
        try:
            return parse_settings(raw, pck.version[0])
        except PckError:
            return {}
    text = read_file(pck, "project.godot")
    return _parse_text_settings(text.decode("utf-8", "replace")) if text else {}


def _parse_text_settings(text: str) -> dict:
    out: dict = {}
    section = ""
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
        elif "=" in line and not line.startswith(";"):
            k, _, v = line.partition("=")
            v = v.strip()
            if v.startswith('"') and v.endswith('"'):
                v = v[1:-1]
            out[f"{section}/{k.strip()}" if section else k.strip()] = v
    return out


def renderer(settings: dict, major: int) -> str:
    """The renderer the game asked for (Godot's default when unset)."""
    if major >= 4:
        return str(settings.get("rendering/renderer/rendering_method") or "forward_plus")
    return "GLES2" if str(settings.get("rendering/quality/driver/driver_name", "GLES3")) == "GLES2" else "GLES3"


def color_hex(rgba, default: str = "#000000") -> str:
    try:
        r, g, b = (max(0, min(255, round(float(c) * 255))) for c in list(rgba)[:3])
    except (TypeError, ValueError):
        return default
    return f"#{r:02x}{g:02x}{b:02x}"


#: Platform tags in .gdextension `[libraries]` keys.
PLATFORM_TAGS = {"macos": ("macos", "osx"), "web": ("web", "wasm32"), "windows": ("windows",),
                 "linux": ("linux", "linuxbsd", "x11"), "android": ("android",)}
ARCH_TAGS = ("x86_64", "x86_32", "arm64", "arm32", "rv64", "wasm32", "universal")


def _feature_match(key: str, platform: str, arch: str | None) -> bool:
    tags = key.strip().strip('"').split(".")
    if tags[0] not in PLATFORM_TAGS[platform]:
        return False
    archs = [t for t in tags[1:] if t in ARCH_TAGS]
    return arch is None or not archs or arch in archs


def extension_files(pck: Pck, platform: str, arch: str | None = None) -> dict[str, list[str]]:
    """``{extension: [file names]}``: the libraries for a platform (and
    architecture) plus their ``[dependencies]`` (e.g. steam_api64.dll),
    debug builds left out when a release one exists."""
    out: dict[str, list[str]] = {}
    for ext in pck.extensions():
        raw = read_file(pck, ext)
        release: list[str] = []
        debug: list[str] = []
        section = ""
        for line in (raw or b"").decode("utf-8", "replace").splitlines():
            line = line.strip()
            if line.startswith("["):
                section = line.strip("[]")
                continue
            if section not in ("libraries", "dependencies") or "=" not in line:
                continue
            k, _, v = line.partition("=")
            if not _feature_match(k, platform, arch):
                continue
            names = [Path(x.removeprefix("res://")).name for x in re.findall(r'"([^"]+)"', v)]
            is_debug = any("debug" in t for t in k.strip().strip('"').split("."))
            (debug if is_debug else release).extend(n for n in names if n)
        files = release or debug
        out[ext] = list(dict.fromkeys(files))
    return out


def extension_libraries(pck: Pck, platform: str = "macos") -> dict[str, list[str]]:
    """``{extension.gdextension: [library paths for platform]}``."""
    out: dict[str, list[str]] = {}
    tags = PLATFORM_TAGS[platform]
    for ext in pck.extensions():
        raw = read_file(pck, ext)
        libs: list[str] = []
        section = ""
        for line in (raw or b"").decode("utf-8", "replace").splitlines():
            line = line.strip()
            if line.startswith("["):
                section = line.strip("[]")
            elif section == "libraries" and "=" in line:
                k, _, v = line.partition("=")
                if k.strip().split(".")[0] in tags:
                    libs.append(v.strip().strip('"'))
        out[ext] = libs
    return out
