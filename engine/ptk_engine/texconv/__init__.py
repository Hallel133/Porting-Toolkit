"""Desktop GPU textures to ASTC for phones (Godot ``.ctex``).

Each mip level of an S3TC/BPTC texture is decoded to RGBA with
``ptk-bcdec`` (bcdec, compiled once on this computer with the system C
compiler) and encoded with ARM's ``astcenc``; the result is a new ``.ctex``
with the same size, flags and mip count in ASTC 4x4 (or 8x8). Results are
cached by content.
"""

from __future__ import annotations

import hashlib
import shutil
import subprocess
import tempfile
from pathlib import Path

from ..formats import ctex
from ..tools import Toolbox

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "ptk_bcdec.c"
BCDEC_H = HERE.parent.parent / "third_party" / "bcdec" / "bcdec.h"
ASTC_MAGIC = b"\x13\xab\xa1\x5c"
BLOCKS = {"4x4": ctex.ASTC_4x4, "8x8": ctex.ASTC_8x8}
PRESETS = ("fastest", "fast", "medium", "thorough")


class TexconvError(Exception):
    pass


def tga(width: int, height: int, rgba: bytes) -> bytes:
    """An uncompressed 32-bit TGA (top-left origin) that astcenc reads."""
    bgra = bytearray(rgba)
    bgra[0::4], bgra[2::4] = rgba[2::4], rgba[0::4]
    head = bytes([0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]) + width.to_bytes(2, "little") \
        + height.to_bytes(2, "little") + bytes([32, 0x28])
    return head + bytes(bgra)


def astc_blocks(data: bytes) -> bytes:
    """The blocks of an .astc file (without its 16-byte header)."""
    if data[:4] != ASTC_MAGIC or len(data) < 16:
        raise TexconvError("astcenc wrote an unexpected file")
    return data[16:]


class Transcoder:
    def __init__(self, cache: Path, toolbox: Toolbox, block: str = "4x4", preset: str = "fast"):
        if block not in BLOCKS:
            raise TexconvError(f"astc_block={block}: use 4x4 or 8x8")
        if preset not in PRESETS:
            raise TexconvError(f"astc_quality={preset}: use one of {', '.join(PRESETS)}")
        self.cache = Path(cache)
        self.tb = toolbox
        self.block = block
        self.preset = preset
        self._helper: Path | None = None

    # -- tools ---------------------------------------------------------------------------

    def problem(self) -> str | None:
        """Why converting is not possible here, or None."""
        if not self.tb.get("astcenc"):
            from ..tools import TOOLS
            return f"astcenc is needed ({TOOLS['astcenc'].hint() or 'see ptk tools'})"
        if not (shutil.which("cc") or shutil.which("clang")):
            return "a C compiler is needed (on macOS: xcode-select --install)"
        if not BCDEC_H.is_file():
            return "bcdec.h is missing from the engine"
        return None

    def helper(self) -> Path:
        """ptk-bcdec, compiled once per source and computer."""
        if self._helper:
            return self._helper
        h = hashlib.sha256(SOURCE.read_bytes() + BCDEC_H.read_bytes()).hexdigest()[:16]
        exe = self.cache / "bcdec" / h / "ptk-bcdec"
        if not exe.is_file():
            cc = shutil.which("cc") or shutil.which("clang")
            exe.parent.mkdir(parents=True, exist_ok=True)
            tmp = exe.with_name("ptk-bcdec.part")
            r = subprocess.run([cc, "-O2", "-I", str(BCDEC_H.parent), "-o", str(tmp), str(SOURCE)],
                               capture_output=True, text=True)
            if r.returncode != 0:
                raise TexconvError("could not compile ptk-bcdec: " + (r.stderr or r.stdout).strip()[-300:])
            tmp.replace(exe)
        self._helper = exe
        return exe

    # -- one texture -----------------------------------------------------------------------

    def key(self, data: bytes) -> str:
        return hashlib.sha256(data + f"|{self.block}|{self.preset}|{self.tb.version('astcenc')}|1".encode()).hexdigest()

    def to_astc(self, data: bytes) -> bytes | None:
        """The ASTC version of a ``.ctex`` (None when its format is not one
        we convert: PNG/WebP textures, normal-map swizzles, HDR)."""
        tex = ctex.Ctex.parse(data)
        if tex.data_format != ctex.DATA_FORMAT_IMAGE or tex.format not in ctex.DECODABLE:
            return None
        cached = self.cache / "astc" / f"{self.key(data)}.ctex"
        if cached.is_file():
            return cached.read_bytes()
        target = BLOCKS[self.block]
        expected = [s for _, _, s in ctex.level_sizes(tex.width, tex.height, target, tex.mipmaps)]
        astcenc = self.tb.path("astcenc")
        out_levels = []
        with tempfile.TemporaryDirectory(prefix="ptk-astc-") as tmp:
            t = Path(tmp)
            for i, ((w, h), level) in enumerate(zip(tex.level_dims(), tex.levels)):
                (t / "in.bc").write_bytes(level)
                r = subprocess.run([str(self.helper()), ctex.DECODABLE[tex.format], str(w), str(h),
                                    str(t / "in.bc"), str(t / "out.rgba")], capture_output=True, text=True)
                if r.returncode != 0:
                    raise TexconvError(f"decoding mip {i} failed: {r.stderr.strip()}")
                (t / "in.tga").write_bytes(tga(w, h, (t / "out.rgba").read_bytes()))
                r = subprocess.run([astcenc, "-cl", str(t / "in.tga"), str(t / "out.astc"), self.block,
                                    f"-{self.preset}", "-silent"], capture_output=True, text=True)
                if r.returncode != 0 or not (t / "out.astc").is_file():
                    raise TexconvError(f"astcenc failed on mip {i}: {(r.stderr or r.stdout).strip()[-200:]}")
                blocks = astc_blocks((t / "out.astc").read_bytes())
                if len(blocks) != expected[i]:
                    raise TexconvError(f"mip {i}: astcenc wrote {len(blocks)} bytes, Godot expects {expected[i]}")
                out_levels.append(blocks)
                (t / "out.astc").unlink()
        out = ctex.Ctex(tex.width, tex.height, tex.flags, tex.mipmap_limit, ctex.DATA_FORMAT_IMAGE, target,
                        tex.mipmaps, out_levels, tex.version).build()
        cached.parent.mkdir(parents=True, exist_ok=True)
        part = cached.with_name(cached.name + ".part")
        part.write_bytes(out)
        part.replace(cached)
        return out
