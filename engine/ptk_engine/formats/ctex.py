"""Godot 4 ``.ctex`` files (CompressedTexture2D, header ``GST2``).

Layout, from ``scene/resources/compressed_texture.cpp`` (Godot 4.4):

    "GST2", u32 version (1), u32 width, u32 height, u32 flags,
    u32 mipmap_limit, 3 x u32 reserved,
    u32 data_format (0 image, 1 PNG, 2 WebP, 3 Basis Universal),
    u16 width, u16 height, u32 mipmaps, u32 Image::Format,
    then for DATA_FORMAT_IMAGE every mip level back to back.

Each level is ``max(w >> i, 1)`` by ``max(h >> i, 1)`` pixels, padded up to
the format's block size; its size in bytes follows ``Image::_get_dst_image_size``.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass

MAGIC = b"GST2"
HEADER = struct.Struct("<4sIIIIIIII")  # magic, version, w, h, flags, mipmap_limit, 3 reserved
IMAGE = struct.Struct("<IHHII")  # data_format, w, h, mipmaps, format

DATA_FORMAT_IMAGE = 0

# Image::Format values (core/io/image.h, Godot 4.x).
DXT1, DXT3, DXT5, RGTC_R, RGTC_RG, BPTC_RGBA = 17, 18, 19, 20, 21, 22
ASTC_4x4, ASTC_8x8 = 35, 37

#: format -> (block size in pixels, right shift of bytes per pixel)
#: (bytes per level = padded_w * padded_h >> shift)
LAYOUT = {
    DXT1: (4, 1), DXT3: (4, 0), DXT5: (4, 0), RGTC_R: (4, 1), RGTC_RG: (4, 0), BPTC_RGBA: (4, 0),
    ASTC_4x4: (4, 0), ASTC_8x8: (8, 2),
}

#: Formats we can decode with bcdec (colour textures; normal-map swizzles and
#: HDR formats stay as they are).
DECODABLE = {DXT1: "bc1", DXT3: "bc2", DXT5: "bc3", BPTC_RGBA: "bc7"}
NAMES = {DXT1: "S3TC DXT1", DXT3: "S3TC DXT3", DXT5: "S3TC DXT5", BPTC_RGBA: "BPTC (BC7)",
         ASTC_4x4: "ASTC 4x4", ASTC_8x8: "ASTC 8x8"}


class CtexError(Exception):
    pass


def level_sizes(width: int, height: int, fmt: int, mipmaps: int) -> list[tuple[int, int, int]]:
    """[(w, h, bytes)] for the base image and each mipmap."""
    if fmt not in LAYOUT:
        raise CtexError(f"image format {fmt} is not handled")
    block, shift = LAYOUT[fmt]
    out = []
    w, h = width, height
    for _ in range(mipmaps + 1):
        bw = -(-w // block) * block
        bh = -(-h // block) * block
        out.append((w, h, (bw * bh) >> shift))
        w, h = max(1, w >> 1), max(1, h >> 1)
    return out


@dataclass
class Ctex:
    width: int
    height: int
    flags: int
    mipmap_limit: int
    data_format: int
    format: int
    mipmaps: int
    levels: list[bytes]  # DATA_FORMAT_IMAGE only
    version: int = 1

    @classmethod
    def parse(cls, data: bytes) -> Ctex:
        if len(data) < HEADER.size + IMAGE.size or data[:4] != MAGIC:
            raise CtexError("not a Godot 4 texture (GST2)")
        _, version, w, h, flags, limit, *_ = HEADER.unpack_from(data, 0)
        df, iw, ih, mipmaps, fmt = IMAGE.unpack_from(data, HEADER.size)
        tex = cls(w, h, flags, limit, df, fmt, mipmaps, [], version)
        if df != DATA_FORMAT_IMAGE:
            return tex
        pos = HEADER.size + IMAGE.size
        for lw, lh, size in level_sizes(iw, ih, fmt, mipmaps) if fmt in LAYOUT else []:
            if pos + size > len(data):
                raise CtexError("the texture data is shorter than its mipmaps need")
            tex.levels.append(data[pos:pos + size])
            pos += size
        return tex

    def level_dims(self) -> list[tuple[int, int]]:
        return [(w, h) for w, h, _ in level_sizes(self.width, self.height, self.format, self.mipmaps)]

    def build(self) -> bytes:
        if self.data_format != DATA_FORMAT_IMAGE:
            raise CtexError("only VRAM (image) textures can be written")
        expected = [s for _, _, s in level_sizes(self.width, self.height, self.format, self.mipmaps)]
        if [len(x) for x in self.levels] != expected:
            raise CtexError(f"mip sizes {[len(x) for x in self.levels]} do not match {expected}")
        head = HEADER.pack(MAGIC, self.version, self.width, self.height, self.flags, self.mipmap_limit, 0, 0, 0)
        return head + IMAGE.pack(DATA_FORMAT_IMAGE, self.width, self.height, self.mipmaps, self.format) \
            + b"".join(self.levels)
