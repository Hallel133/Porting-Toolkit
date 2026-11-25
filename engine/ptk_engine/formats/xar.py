"""macOS installer packages (``.pkg``), read without installing them.

A flat ``.pkg`` is a xar archive: a 28-byte header, a zlib-compressed XML
table of contents, then the files' data. Its ``Payload`` is a gzip'd cpio
archive (``odc``, magic ``070707``) of the files the installer would copy.
Only the members you ask for are written, so taking one Unity player out of
a 180 MB module needs no installer, no admin rights and no Unity.
"""

from __future__ import annotations

import os
import re
import struct
import zlib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import BinaryIO, Callable, Iterator

MAGIC = b"xar!"
CPIO_ODC = b"070707"
TRAILER = "TRAILER!!!"


class XarError(Exception):
    pass


@dataclass
class XarFile:
    name: str
    offset: int  # from the start of the heap
    length: int  # stored (compressed) size
    encoding: str  # "application/x-gzip", "application/octet-stream", ...


@dataclass
class Xar:
    heap: int  # absolute offset of the heap
    files: list[XarFile]

    def get(self, name: str) -> XarFile:
        for f in self.files:
            if f.name == name:
                return f
        raise XarError(f"the package has no {name}")


def read_toc(f: BinaryIO) -> Xar:
    """The top-level files of a xar archive."""
    head = f.read(28)
    if len(head) < 28 or head[:4] != MAGIC:
        raise XarError("not a flat .pkg (xar) file")
    _, size, _, clen, _ulen, _alg = struct.unpack(">4sHHQQI", head)
    f.seek(size)
    try:
        toc = zlib.decompress(f.read(clen)).decode("utf-8")
    except zlib.error as e:
        raise XarError(f"unreadable table of contents: {e}") from None
    files = []
    # Top-level entries only (a .pkg's Payload is at the top).
    body = re.search(r"<toc>(.*)</toc>", toc, re.S)
    for m in re.finditer(r"<file id=\"\d+\">(.*?)</file>", body.group(1) if body else "", re.S):
        part = m.group(1)
        name = re.search(r"<name>(.*?)</name>", part)
        off = re.search(r"<offset>(\d+)</offset>", part)
        length = re.search(r"<length>(\d+)</length>", part)
        enc = re.search(r"<encoding style=\"([^\"]+)\"", part)
        if name and off and length:
            files.append(XarFile(name.group(1), int(off.group(1)), int(length.group(1)),
                                 enc.group(1) if enc else "application/octet-stream"))
    return Xar(size + clen, files)


def _chunks(f: BinaryIO, start: int, length: int, size: int = 1 << 20) -> Iterator[bytes]:
    f.seek(start)
    left = length
    while left > 0:
        b = f.read(min(size, left))
        if not b:
            raise XarError("the package is truncated")
        left -= len(b)
        yield b


class _Stream:
    """Decompressed bytes of the Payload, read in order."""

    def __init__(self, chunks: Iterator[bytes]):
        self.chunks = chunks
        self.z = zlib.decompressobj(31)  # gzip
        self.buf = bytearray()
        self.done = False

    def read(self, n: int) -> bytes:
        while len(self.buf) < n and not self.done:
            c = next(self.chunks, None)
            if c is None:
                self.buf += self.z.flush()
                self.done = True
            else:
                self.buf += self.z.decompress(c)
        out = bytes(self.buf[:n])
        del self.buf[:n]
        return out


def _safe(name: str) -> PurePosixPath | None:
    p = PurePosixPath(name.removeprefix("./"))
    if name in (".", "./") or not p.parts or p.is_absolute() or ".." in p.parts:
        return None
    return p


def extract_payload(pkg: Path, dest: Path, want: Callable[[PurePosixPath], bool],
                    progress: Callable[[int], None] | None = None) -> int:
    """Writes the Payload members for which ``want(path)`` is true into
    ``dest`` (directories, files with their exec bits, and symlinks that stay
    inside ``dest``). Returns how many files were written."""
    dest = Path(dest)
    written = 0
    with open(pkg, "rb") as f:
        x = read_toc(f)
        payload = x.get("Payload")
        s = _Stream(_chunks(f, x.heap + payload.offset, payload.length))
        while True:
            h = s.read(76)
            if len(h) < 76 or h[:6] != CPIO_ODC:
                raise XarError("the Payload is not a cpio (odc) archive")
            mode = int(h[18:24], 8)
            namesize = int(h[59:65], 8)
            filesize = int(h[65:76], 8)
            name = s.read(namesize)[:-1].decode("utf-8", "replace")
            if name == TRAILER:
                break
            data_left = filesize
            path = _safe(name)
            kind = mode & 0o170000
            take = path is not None and want(path)
            out = dest.joinpath(*path.parts) if take else None
            if take and kind == 0o040000:
                out.mkdir(parents=True, exist_ok=True)
            elif take and kind == 0o120000:
                target = s.read(filesize).decode("utf-8", "replace")
                data_left = 0
                resolved = os.path.normpath(os.path.join(os.path.dirname(str(out)), target))
                if os.path.isabs(target) or not resolved.startswith(os.path.normpath(dest) + os.sep):
                    raise XarError(f"{name} links outside the package: {target}")
                out.parent.mkdir(parents=True, exist_ok=True)
                if out.is_symlink() or out.exists():
                    out.unlink()
                os.symlink(target, out)
            elif take and kind == 0o100000:
                out.parent.mkdir(parents=True, exist_ok=True)
                with open(out, "wb") as o:
                    while data_left:
                        b = s.read(min(1 << 20, data_left))
                        if not b:
                            raise XarError("the Payload is truncated")
                        o.write(b)
                        data_left -= len(b)
                os.chmod(out, 0o755 if mode & 0o111 else 0o644)
                written += 1
                if progress:
                    progress(written)
            while data_left:  # skip what is not wanted
                b = s.read(min(1 << 20, data_left))
                if not b:
                    raise XarError("the Payload is truncated")
                data_left -= len(b)
    return written


# -- writing (tests build small packages with this) ------------------------------------

def cpio_odc(entries: list[tuple[str, int, bytes]]) -> bytes:
    """A cpio (odc) archive of ``(name, mode, data)``."""
    out = bytearray()
    for i, (name, mode, data) in enumerate([*entries, (TRAILER, 0, b"")]):
        n = name.encode() + b"\0"
        out += b"%s%06o%06o%06o%06o%06o%06o%06o%011o%06o%011o" % (
            CPIO_ODC, 0, i, mode, 0, 0, 1, 0, 0, len(n), len(data))
        out += n + data
    return bytes(out)


def build_pkg(payload: bytes, extra: dict[str, bytes] | None = None) -> bytes:
    """A minimal flat .pkg whose Payload is ``payload`` gzip'd."""
    import gzip

    files = {"Payload": gzip.compress(payload), **(extra or {})}
    heap = bytearray()
    entries = []
    for i, (name, data) in enumerate(files.items(), 1):
        entries.append(f'<file id="{i}"><name>{name}</name><type>file</type><data>'
                       f"<offset>{len(heap)}</offset><length>{len(data)}</length><size>{len(data)}</size>"
                       '<encoding style="application/octet-stream"/></data></file>')
        heap += data
    toc = zlib.compress(f"<?xml version=\"1.0\"?><xar><toc>{''.join(entries)}</toc></xar>".encode())
    head = struct.pack(">4sHHQQI", MAGIC, 28, 1, len(toc), 0, 0)
    return head + toc + bytes(heap)
