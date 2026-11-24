"""Godot .ctex files and the BC -> ASTC conversion for Android."""

from __future__ import annotations

import shutil
import struct
import sys
import zipfile
from pathlib import Path

import pytest

from ptk_engine import tools
from ptk_engine.formats import ctex, godot
from ptk_engine.runtime import BUILTIN_DIR, run_plugin
from ptk_engine.texconv import Transcoder, astc_blocks, tga

ANDROID = {"id": "android", "name": "Android", "os": "android", "arch": "arm64"}

# A BC1 block: colour0 pure red, colour1 pure blue, every pixel index 0 (red).
BC1_RED = struct.pack("<HHI", 0xF800, 0x001F, 0)
# Index 1 everywhere: blue.
BC1_BLUE = struct.pack("<HHI", 0xF800, 0x001F, 0x55555555)

FAKE_ASTCENC = r"""#!/usr/bin/env python3
# Stands in for astcenc in tests: reads the TGA size, writes an .astc file
# with the right header and number of (empty) blocks.
import sys
args = sys.argv[1:]
if "-help" in args:
    print("astcenc v9.9.9")
    sys.exit(0)
mode, src, dst, block = args[0], args[1], args[2], args[3]
data = open(src, "rb").read()
w = int.from_bytes(data[12:14], "little"); h = int.from_bytes(data[14:16], "little")
assert len(data) == 18 + w * h * 4, "bad TGA"
bx = int(block.split("x")[0]); by = int(block.split("x")[1])
n = -(-w // bx) * -(-h // by)
head = bytes([0x13, 0xAB, 0xA1, 0x5C, bx, by, 1]) + w.to_bytes(3, "little") + h.to_bytes(3, "little") + (1).to_bytes(3, "little")
open(dst, "wb").write(head + b"\xfc" * (16 * n))
"""


def fake_astcenc(folder: Path) -> Path:
    folder.mkdir(parents=True, exist_ok=True)
    p = folder / "astcenc"
    p.write_text(FAKE_ASTCENC.replace("/usr/bin/env python3", sys.executable))
    p.chmod(0o755)
    return p


def bc1_ctex(size: int = 16, mipmaps: int = 4) -> bytes:
    levels = []
    for w, h, n in ctex.level_sizes(size, size, ctex.DXT1, mipmaps):
        levels.append((BC1_RED * (n // 8))[:n])
    return ctex.Ctex(size, size, 1 << 23, 0, ctex.DATA_FORMAT_IMAGE, ctex.DXT1, mipmaps, levels).build()


# -- the file format --------------------------------------------------------------------


def test_level_sizes_follow_godot():
    # 256x256 DXT1 with 8 mips: 4 bits per pixel, small levels padded to 4x4 blocks.
    sizes = [s for _, _, s in ctex.level_sizes(256, 256, ctex.DXT1, 8)]
    assert sizes == [32768, 8192, 2048, 512, 128, 32, 8, 8, 8]
    assert [s for _, _, s in ctex.level_sizes(256, 256, ctex.BPTC_RGBA, 2)] == [65536, 16384, 4096]
    assert [s for _, _, s in ctex.level_sizes(20, 12, ctex.ASTC_8x8, 1)] == [6 * 16, 2 * 16]  # 3x2 then 2x1 blocks
    assert ctex.level_sizes(5, 3, ctex.ASTC_4x4, 0) == [(5, 3, 32)]


def test_ctex_round_trip():
    data = bc1_ctex(16, 4)
    tex = ctex.Ctex.parse(data)
    assert (tex.width, tex.format, tex.mipmaps, len(tex.levels)) == (16, ctex.DXT1, 4, 5)
    assert tex.build() == data
    with pytest.raises(ctex.CtexError):
        ctex.Ctex.parse(data[:-4])
    with pytest.raises(ctex.CtexError):
        ctex.Ctex.parse(b"RSRC" + data[4:])


def test_tga_and_astc_helpers():
    t = tga(2, 1, bytes([1, 2, 3, 4, 5, 6, 7, 8]))
    assert t[2] == 2 and t[12:16] == bytes([2, 0, 1, 0]) and t[16:18] == bytes([32, 0x28])
    assert t[18:] == bytes([3, 2, 1, 4, 7, 6, 5, 8])  # BGRA
    assert astc_blocks(b"\x13\xab\xa1\x5c" + b"\0" * 12 + b"BLOCK") == b"BLOCK"


# -- the decoder (compiled for real) -----------------------------------------------------------

needs_cc = pytest.mark.skipif(not (shutil.which("cc") or shutil.which("clang")), reason="needs a C compiler")


@needs_cc
def test_bcdec_helper_decodes_bc1(tmp_path):
    import subprocess
    tc = Transcoder(tmp_path / "cache", tools.Toolbox(None, auto_install=False))
    exe = tc.helper()
    # 6x4 pixels = 2x1 blocks: red then blue, cropped to 6 columns.
    (tmp_path / "in").write_bytes(BC1_RED + BC1_BLUE)
    r = subprocess.run([str(exe), "bc1", "6", "4", str(tmp_path / "in"), str(tmp_path / "out")], capture_output=True)
    assert r.returncode == 0, r.stderr
    px = (tmp_path / "out").read_bytes()
    assert len(px) == 6 * 4 * 4
    assert px[0:4] == bytes([255, 0, 0, 255]) and px[4 * 4:4 * 4 + 4] == bytes([0, 0, 255, 255])
    bad = subprocess.run([str(exe), "bc1", "64", "64", str(tmp_path / "in"), str(tmp_path / "o2")], capture_output=True)
    assert bad.returncode != 0, "a short input is refused"


@needs_cc
def test_transcoder_writes_an_astc_ctex(tmp_path):
    tb = tools.Toolbox(None, overrides={"astcenc": str(fake_astcenc(tmp_path / "bin"))}, auto_install=False)
    tc = Transcoder(tmp_path / "cache", tb, "4x4", "fast")
    assert tc.problem() is None
    out = tc.to_astc(bc1_ctex(16, 4))
    tex = ctex.Ctex.parse(out)
    assert (tex.format, tex.mipmaps, tex.flags) == (ctex.ASTC_4x4, 4, 1 << 23)
    assert [len(x) for x in tex.levels] == [256, 64, 16, 16, 16]
    assert list((tmp_path / "cache" / "astc").glob("*.ctex")), "cached"
    assert Transcoder(tmp_path / "cache", tb, "8x8").to_astc(bc1_ctex(16, 0)) is not None
    png_like = ctex.HEADER.pack(ctex.MAGIC, 1, 4, 4, 0, 0, 0, 0, 0) + ctex.IMAGE.pack(1, 4, 4, 0, 5) + b"png"
    assert tc.to_astc(png_like) is None, "PNG/WebP textures are not VRAM data"


def test_missing_astcenc_is_explained(tmp_path, monkeypatch):
    monkeypatch.setattr(tools, "EXTRA_DIRS", ())
    monkeypatch.setenv("PATH", "")
    tc = Transcoder(tmp_path, tools.Toolbox(None, auto_install=False))
    assert "astcenc" in tc.problem()


# -- the Godot Android export ------------------------------------------------------------------


@needs_cc
def test_godot_android_export_converts_to_astc(tmp_path):
    sys.path.insert(0, str(Path(__file__).parent))
    from test_android import fake_template
    imp = (b'[remap]\n\nimporter="texture"\ntype="CompressedTexture2D"\n'
           b'path.s3tc="res://.godot/imported/a.png-1.s3tc.ctex"\nmetadata={\n"vram_texture": true\n}\n')
    files = {"project.binary": b"ECFG\0\0\0\0", "a.png.import": imp,
             ".godot/imported/a.png-1.s3tc.ctex": bc1_ctex(32, 5)}
    godot.write_pck(tmp_path / "game" / "Moon.pck", files, (4, 4, 1), 3)
    (tmp_path / "project").mkdir()
    r = run_plugin({
        "plugin": str(BUILTIN_DIR / "godot-export"), "kind": "export", "engine": "godot", "target": ANDROID,
        "manifest": {"name": "p", "version": "1.0.0", "game": {"title": "Moon"}},
        "settings": {"templates": str(fake_template(tmp_path)), "sign": "false", "download_tools": "false",
                     "tool.astcenc": str(fake_astcenc(tmp_path / "bin"))},
        "folders": {"$GAME": str(tmp_path / "game"), "$PROJECT": str(tmp_path / "project"),
                    "$OUTPUT": str(tmp_path / "out")},
        "cache_root": str(tmp_path / "cache"),
    }, lambda m, p: None)["result"]
    assert not r["warnings"], r["warnings"]
    with zipfile.ZipFile(tmp_path / "out" / "Moon.apk") as z:
        text = z.read("assets/a.png.import").decode()
        astc = ctex.Ctex.parse(z.read("assets/.godot/imported/a.png-1.astc.ctex"))
    lines = [ln for ln in text.splitlines() if ln.startswith("path")]
    assert lines == ['path.s3tc="res://.godot/imported/a.png-1.s3tc.ctex"',
                     'path.astc="res://.godot/imported/a.png-1.astc.ctex"',
                     'path="res://.godot/imported/a.png-1.s3tc.ctex"'], "ASTC first, desktop file as the fallback"
    assert astc.format == ctex.ASTC_4x4 and astc.mipmaps == 5
