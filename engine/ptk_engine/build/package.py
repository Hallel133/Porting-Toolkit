"""Zips of an output folder, rebuilt incrementally.

The zip is written by hand (the format is small), so members that did not
change since the last zip are copied from it as they are: no reading the
file, no compressing it again. Changed members are compressed in parallel
(zlib releases the GIL). Files that are already compressed (images, sound,
video, archives) are stored. Dates are fixed and entries sorted, so the same
folder always gives the same bytes, whether incremental or not.

Next to the zip, ``<zip>.ptkindex`` remembers each member: its source's
size and date, and where its data is in the zip.
"""

from __future__ import annotations

import json
import os
import shutil
import struct
import tempfile
import zlib
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

#: Already compressed: deflating them again gains next to nothing.
STORED = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".ogg", ".opus", ".mp3", ".m4a", ".aac", ".mp4",
          ".m4v", ".webm", ".mkv", ".mov", ".zip", ".7z", ".gz", ".xz", ".bz2", ".rar", ".jar", ".apk",
          ".pck", ".bank", ".unity3d", ".bundle", ".rpa", ".woff", ".woff2", ".ktx2", ".basis", ".astc"}
DOS_TIME, DOS_DATE = 0, (2020 - 1980) << 9 | 1 << 5 | 1  # 2020-01-01 00:00
INDEX_VERSION = 1
SMALL = 8 << 20  # compressed in memory below this size


@dataclass
class Member:
    name: str
    path: Path
    mode: int
    size: int
    mtime_ns: int
    method: int = 8
    crc: int = 0
    csize: int = 0
    data: bytes | None = None  # compressed bytes, or
    data_file: Path | None = None  # in a temporary file, or
    reuse_offset: int | None = None  # at this offset of the previous zip


def _compress(m: Member, tmpdir: Path) -> None:
    store = m.path.suffix.lower() in STORED
    crc = 0
    with open(m.path, "rb") as f:
        if m.size <= SMALL:
            raw = f.read()
            crc = zlib.crc32(raw)
            if store:
                m.method, m.data = 0, raw
            else:
                c = zlib.compressobj(6, zlib.DEFLATED, -15)
                data = c.compress(raw) + c.flush()
                m.method, m.data = (8, data) if len(data) < len(raw) else (0, raw)
            m.crc, m.csize = crc & 0xFFFFFFFF, len(m.data)
            return
        out_path = Path(tempfile.mkstemp(dir=tmpdir)[1])
        c = None if store else zlib.compressobj(6, zlib.DEFLATED, -15)
        with open(out_path, "wb") as out:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                crc = zlib.crc32(chunk, crc)
                out.write(chunk if c is None else c.compress(chunk))
            if c is not None:
                out.write(c.flush())
        csize = out_path.stat().st_size
        if c is not None and csize >= m.size:  # did not shrink: store it
            f.seek(0)
            with open(out_path, "wb") as out:
                shutil.copyfileobj(f, out, 1 << 20)
            csize, store = m.size, True
        m.method = 0 if (store or c is None) else 8
        m.crc, m.csize, m.data_file = crc & 0xFFFFFFFF, csize, out_path


def _local_header(m: Member) -> bytes:
    name = m.name.encode("utf-8")
    flags = 0x800 if not m.name.isascii() else 0
    zip64 = m.size >= 0xFFFFFFFF or m.csize >= 0xFFFFFFFF
    extra = struct.pack("<HHQQ", 1, 16, m.size, m.csize) if zip64 else b""
    return struct.pack("<IHHHHHIIIHH", 0x04034B50, 45 if zip64 else 20, flags, m.method, DOS_TIME, DOS_DATE,
                       m.crc, 0xFFFFFFFF if zip64 else m.csize, 0xFFFFFFFF if zip64 else m.size,
                       len(name), len(extra)) + name + extra


def _central(m: Member, offset: int) -> bytes:
    name = m.name.encode("utf-8")
    flags = 0x800 if not m.name.isascii() else 0
    big_size = m.size >= 0xFFFFFFFF or m.csize >= 0xFFFFFFFF
    big_off = offset >= 0xFFFFFFFF
    fields = b""
    if big_size:
        fields += struct.pack("<QQ", m.size, m.csize)
    if big_off:
        fields += struct.pack("<Q", offset)
    extra = struct.pack("<HH", 1, len(fields)) + fields if fields else b""
    zip64 = bool(fields)
    return struct.pack("<IHHHHHHIIIHHHHHII", 0x02014B50, (3 << 8) | 45, 45 if zip64 else 20, flags, m.method,
                       DOS_TIME, DOS_DATE, m.crc, 0xFFFFFFFF if big_size else m.csize,
                       0xFFFFFFFF if big_size else m.size, len(name), len(extra), 0, 0, 0,
                       (0o100000 | m.mode) << 16, 0xFFFFFFFF if big_off else offset) + name + extra


def _end(count: int, cd_offset: int, cd_size: int) -> bytes:
    out = b""
    if count >= 0xFFFF or cd_offset >= 0xFFFFFFFF or cd_size >= 0xFFFFFFFF:
        z64 = cd_offset + cd_size
        out += struct.pack("<IQHHIIQQQQ", 0x06064B50, 44, (3 << 8) | 45, 45, 0, 0, count, count, cd_size, cd_offset)
        out += struct.pack("<IIQI", 0x07064B50, 0, z64, 1)
    out += struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, min(count, 0xFFFF), min(count, 0xFFFF),
                       min(cd_size, 0xFFFFFFFF), min(cd_offset, 0xFFFFFFFF), 0)
    return out


def _members(src: Path, executables: set[str], skip_names: set[str]) -> list[Member]:
    """Files under ``src`` named ``<src.name>/<relative path>``."""
    out = []
    for dirpath, dirnames, filenames in os.walk(src):
        dirnames.sort()
        d = Path(dirpath)
        for name in sorted(filenames):
            p = d / name
            if name in skip_names or p.is_symlink() or not p.is_file():
                continue
            rel = p.relative_to(src).as_posix()
            st = p.stat()
            mode = 0o755 if rel in executables or os.access(p, os.X_OK) else 0o644
            out.append(Member(f"{src.name}/{rel}", p, mode, st.st_size, st.st_mtime_ns))
    out.sort(key=lambda m: m.name)
    return out


def _index_path(dest: Path) -> Path:
    return dest.with_name(dest.name + ".ptkindex")


def _load_index(dest: Path) -> dict:
    idx = _index_path(dest)
    try:
        data = json.loads(idx.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    if data.get("version") != INDEX_VERSION or not dest.is_file() or dest.stat().st_size != data.get("zip_size"):
        return {}
    return data.get("members") or {}


def zip_dir(src: Path, dest: Path, executables: set[str] | None = None,
            skip_names: set[str] | None = None, workers: int | None = None,
            progress=None) -> dict:
    """Zips the folder ``src`` (entries start with its name) into ``dest``,
    reusing the unchanged members of the previous ``dest``. Returns
    ``{"members", "reused", "compressed", "bytes"}``."""
    src, dest = Path(src), Path(dest)
    members = _members(src, executables or set(), (skip_names or set()) | {".ptk-output", ".DS_Store"})
    old = _load_index(dest)
    todo = []
    for m in members:
        o = old.get(m.name)
        if o and o["size"] == m.size and o["mtime_ns"] == m.mtime_ns and o["mode"] == m.mode:
            m.method, m.crc, m.csize, m.reuse_offset = o["method"], o["crc"], o["csize"], o["data_offset"]
        else:
            todo.append(m)
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmpdir = Path(tempfile.mkdtemp(prefix=".ptk-zip-", dir=dest.parent))
    try:
        done = 0

        def one(m: Member) -> None:
            nonlocal done
            _compress(m, tmpdir)
            done += 1
            if progress:
                progress(done, len(todo))

        with ThreadPoolExecutor(max_workers=workers or min(8, os.cpu_count() or 2)) as pool:
            list(pool.map(one, todo))
        part = dest.with_name(dest.name + ".part")
        index: dict[str, dict] = {}
        with open(part, "wb") as out:
            prev = open(dest, "rb") if any(m.reuse_offset is not None for m in members) else None
            try:
                central = []
                for m in members:
                    offset = out.tell()
                    out.write(_local_header(m))
                    data_offset = out.tell()
                    if m.reuse_offset is not None:
                        prev.seek(m.reuse_offset)
                        left = m.csize
                        while left:
                            b = prev.read(min(1 << 20, left))
                            if not b:
                                raise OSError("the previous zip is truncated")
                            out.write(b)
                            left -= len(b)
                    elif m.data is not None:
                        out.write(m.data)
                        m.data = None
                    else:
                        with open(m.data_file, "rb") as f:
                            shutil.copyfileobj(f, out, 1 << 20)
                        m.data_file.unlink()
                    central.append(_central(m, offset))
                    index[m.name] = {"size": m.size, "mtime_ns": m.mtime_ns, "mode": m.mode, "method": m.method,
                                     "crc": m.crc, "csize": m.csize, "data_offset": data_offset}
                cd_offset = out.tell()
                for c in central:
                    out.write(c)
                out.write(_end(len(members), cd_offset, out.tell() - cd_offset))
            finally:
                if prev:
                    prev.close()
        os.replace(part, dest)
        _index_path(dest).write_text(json.dumps({"version": INDEX_VERSION, "zip_size": dest.stat().st_size,
                                                 "members": index}), encoding="utf-8")
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)
    return {"members": len(members), "reused": len(members) - len(todo), "compressed": len(todo),
            "bytes": dest.stat().st_size}
