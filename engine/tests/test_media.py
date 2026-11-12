"""The asset pipeline (advanced options), its encoders and the tools manager."""

from __future__ import annotations

import hashlib
import io
import os
import shutil
import struct
import tarfile
import zlib
from pathlib import Path

import pytest

from ptk_engine import tools
from ptk_engine.media import Pipeline, describe, encoders
from ptk_engine.media.encoders import Encoder, EncodeError, redeflate_png
from ptk_engine.media.probe import image_info
from ptk_engine.media.profiles import PROFILES, decide
from ptk_engine.runtime import BUILTIN_DIR, run_plugin

WEB = {"id": "web", "name": "Web", "os": "web", "arch": "wasm32",
       "media": {"textures": ["webp", "png"], "audio": ["opus", "vorbis", "aac"], "video": ["vp9", "h264"]}}
RECOMPRESS = {"textures": {"action": "recompress"}}


def png_bytes(w: int = 64, h: int = 32, alpha: bool = True, level: int = 0) -> bytes:
    """A real PNG; level 0 leaves room for re-deflating."""
    channels = 4 if alpha else 3
    rows = b"".join(b"\x00" + bytes(((x * 7 + y * 3 + c * 50) % 256) for x in range(w) for c in range(channels))
                    for y in range(h))

    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6 if alpha else 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"tEXt", b"Comment\x00kept")
            + chunk(b"IDAT", zlib.compress(rows, level)) + chunk(b"IEND", b""))


def idat(data: bytes) -> bytes:
    raw = b""
    for kind, body in encoders._chunks(data):
        if kind == b"IDAT":
            raw += body
    return zlib.decompress(raw)


def toolbox(tmp: Path, **kw) -> tools.Toolbox:
    # No real tools: an empty PATH and no downloads.
    tb = tools.Toolbox(tmp / "tools", auto_install=False, **kw)
    for t in tools.TOOLS:
        tb._found[t] = None
    return tb


# -- profiles ----------------------------------------------------------------------


@pytest.mark.parametrize("profile,ext,requested,preferred,source,codec,noted", [
    # Loose-file games keep each file's codec unless told otherwise.
    ("html5", ".ogg", None, ["opus", "vorbis"], "vorbis", "vorbis", False),
    ("html5", ".ogg", "opus", [], "vorbis", "opus", False),
    ("html5", ".png", "webp", [], None, "png", True),
    # RPG Maker MZ decodes .ogg itself: Vorbis only.
    ("rpgmaker-mz", ".ogg", "opus", ["opus"], "vorbis", "vorbis", True),
    ("rpgmaker-mz", ".ogg", None, ["opus"], "vorbis", "vorbis", False),
    ("love", ".ogg", "opus", [], "vorbis", "vorbis", True),
    # Ren'Py follows the target's preference.
    ("renpy", ".ogg", None, ["opus", "vorbis"], "vorbis", "opus", False),
    ("renpy", ".m4a", None, ["opus"], "aac", "aac", False),
    ("html5", ".webm", None, ["h264"], "vp9", "vp9", False),
    ("html5", ".wav", None, ["opus"], None, None, False),
])
def test_profile_decisions(profile, ext, requested, preferred, source, codec, noted):
    cat = "audio" if ext in (".ogg", ".m4a", ".wav") else ("textures" if ext == ".png" else "video")
    d = decide(PROFILES[profile], cat, ext, requested, preferred, source)
    assert d.codec == codec
    assert bool(d.note) == noted


def test_godot_profile_leaves_files_alone():
    d = decide(PROFILES["godot"], "textures", ".png", None, ["astc"], None)
    assert d.codec is None and "pck" in d.note


# -- images ---------------------------------------------------------------------------


def test_image_headers(tmp_path):
    p = tmp_path / "a.png"
    p.write_bytes(png_bytes(64, 32, alpha=True))
    assert image_info(p) == image_info(p) and image_info(p).width == 64 and image_info(p).alpha
    p.write_bytes(png_bytes(10, 20, alpha=False))
    i = image_info(p)
    assert (i.width, i.height, i.alpha) == (10, 20, False)
    jpeg = (b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00"
            b"\xff\xc0\x00\x11\x08\x00\x30\x00\x40\x03\x01\x22\x00\x02\x11\x01\x03\x11\x01\xff\xd9")
    (tmp_path / "b.jpg").write_bytes(jpeg)
    assert image_info(tmp_path / "b.jpg").width == 64 and image_info(tmp_path / "b.jpg").height == 48
    vp8x = b"RIFF\x00\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00\x10\x00\x00\x00" + (99).to_bytes(3, "little") \
        + (49).to_bytes(3, "little")
    (tmp_path / "c.webp").write_bytes(vp8x)
    i = image_info(tmp_path / "c.webp")
    assert (i.width, i.height, i.alpha) == (100, 50, True)
    (tmp_path / "d.png").write_bytes(b"not an image")
    assert image_info(tmp_path / "d.png") is None


def test_png_redeflate_keeps_pixels_and_chunks(tmp_path):
    src = tmp_path / "a.png"
    src.write_bytes(png_bytes(128, 64, level=0))
    dst = tmp_path / "b.png"
    redeflate_png(src, dst)
    assert dst.stat().st_size < src.stat().st_size / 2
    assert idat(dst.read_bytes()) == idat(src.read_bytes())
    kinds = [k for k, _ in encoders._chunks(dst.read_bytes())]
    assert kinds == [b"IHDR", b"tEXt", b"IDAT", b"IEND"]
    (tmp_path / "bad.png").write_bytes(b"\x89PNG\r\n\x1a\n")
    with pytest.raises(EncodeError):
        redeflate_png(tmp_path / "bad.png", tmp_path / "x.png")


def test_quality_mappings_stay_in_range():
    for q in (1, 50, 100):
        assert -1 <= encoders.vorbis_q(q) <= 10
        assert 2 <= encoders.jpeg_qscale(q) <= 31
        assert 0 <= encoders.mp3_vbr(q) <= 9
        for c in ("h264", "hevc", "vp9", "av1"):
            assert 0 <= encoders.video_crf(c, q) <= 63
    assert encoders.vorbis_q(100) > encoders.vorbis_q(10)
    assert encoders.opus_kbps(100) > encoders.opus_kbps(10)


# -- the pipeline ----------------------------------------------------------------------


def game(tmp: Path) -> Path:
    root = tmp / "game"
    (root / "img").mkdir(parents=True)
    (root / "img" / "a.png").write_bytes(png_bytes(128, 64, level=0))
    (root / "img" / "b.png").write_bytes(png_bytes(32, 32, level=9))
    (root / "data.json").write_text("{}")
    return root


def test_lossless_png_without_any_tool(tmp_path):
    root = game(tmp_path)
    before = (root / "img" / "a.png").read_bytes()
    report = Pipeline(toolbox(tmp_path), tmp_path / "media").run(root, "html5", RECOMPRESS, WEB)
    t = report["categories"]["textures"]
    assert t["files"] == 2 and t["converted"] >= 1 and t["after"] < t["before"]
    after = (root / "img" / "a.png").read_bytes()
    assert len(after) < len(before) and idat(after) == idat(before)
    assert report["before"] - report["after"] == t["before"] - t["after"]
    assert "files re-encoded" in describe(report)
    # Second run: the converted files hit the cache, nothing changes.
    again = Pipeline(toolbox(tmp_path), tmp_path / "media").run(root, "html5", RECOMPRESS, WEB)
    assert again["cache_hits"] >= 1


def test_plan_changes_nothing(tmp_path):
    root = game(tmp_path)
    snapshot = {p: p.read_bytes() for p in root.rglob("*") if p.is_file()}
    report = Pipeline(toolbox(tmp_path), None).run(root, "html5", RECOMPRESS, WEB, mode="plan")
    assert report["estimated"] and report["after"] < report["before"]
    assert report["categories"]["textures"]["sampled"] == 2
    assert snapshot == {p: p.read_bytes() for p in root.rglob("*") if p.is_file()}


def test_keep_everything_means_no_report(tmp_path):
    root = game(tmp_path)
    assert Pipeline(toolbox(tmp_path), None).run(root, "html5", {"textures": {"action": "keep"}}, WEB) is None
    assert Pipeline(toolbox(tmp_path), None).run(root, "html5", RECOMPRESS, WEB, mode="off") is None


def test_drop_is_refused_for_loose_files(tmp_path):
    root = game(tmp_path)
    report = Pipeline(toolbox(tmp_path), None).run(root, "html5", {"textures": {"action": "drop"}}, WEB)
    assert (root / "img" / "a.png").is_file()
    assert report["categories"]["textures"]["kept"] == 2
    assert any("can't be dropped" in n for n in report["categories"]["textures"]["notes"])


def test_gpu_format_is_explained(tmp_path):
    root = game(tmp_path)
    report = Pipeline(toolbox(tmp_path), None).run(
        root, "html5", {"textures": {"action": "recompress", "format": "astc"}}, WEB)
    assert any("GPU texture format" in n for n in report["categories"]["textures"]["notes"])


def test_bad_result_is_rejected_and_remembered(tmp_path, monkeypatch):
    root = game(tmp_path)
    calls = []

    def shrink(src, dst, q, params, tb):
        calls.append(src)
        dst.write_bytes(png_bytes(8, 8, level=9))  # smaller, but the wrong size
        return "fake"

    monkeypatch.setitem(encoders.ENCODERS, "png", Encoder("png", "textures", shrink, ("oxipng",)))
    original = (root / "img" / "a.png").read_bytes()
    report = Pipeline(toolbox(tmp_path), tmp_path / "media").run(root, "html5", RECOMPRESS, WEB)
    t = report["categories"]["textures"]
    assert t["failed"] == 2 and t["converted"] == 0 and t["after"] == t["before"]
    assert (root / "img" / "a.png").read_bytes() == original
    assert any("size changed" in n for n in t["notes"])
    # The failure is cached: the encoder does not run again.
    n = len(calls)
    Pipeline(toolbox(tmp_path), tmp_path / "media").run(root, "html5", RECOMPRESS, WEB)
    assert len(calls) == n


def test_not_smaller_keeps_the_original(tmp_path, monkeypatch):
    root = game(tmp_path)

    def same(src, dst, q, params, tb):
        shutil.copyfile(src, dst)
        return "fake"

    monkeypatch.setitem(encoders.ENCODERS, "png", Encoder("png", "textures", same, ("oxipng",)))
    report = Pipeline(toolbox(tmp_path), None).run(root, "html5", RECOMPRESS, WEB)
    t = report["categories"]["textures"]
    assert t["converted"] == 0 and t["kept"] == 2 and t["failed"] == 0


def test_missing_tool_is_reported(tmp_path):
    root = game(tmp_path)
    (root / "music.ogg").write_bytes(b"OggS" + b"\x00" * 100)
    report = Pipeline(toolbox(tmp_path), None).run(root, "html5", {"audio": {"action": "recompress"}}, WEB)
    a = report["categories"]["audio"]
    assert a["files"] == 1 and a["kept"] == 1
    assert any("FFmpeg" in n for n in a["notes"])


def test_godot_report_says_kept(tmp_path):
    report = Pipeline(toolbox(tmp_path), None).run(tmp_path, "godot", RECOMPRESS, WEB)
    assert report["categories"]["textures"]["files"] == 0
    assert "pck" in report["categories"]["textures"]["notes"][0]


def test_html5_export_applies_options(tmp_path):
    game_dir = tmp_path / "www"
    (game_dir / "img").mkdir(parents=True)
    (game_dir / "index.html").write_text("<!doctype html><title>t</title>")
    (game_dir / "img" / "a.png").write_bytes(png_bytes(128, 64, level=0))
    project = tmp_path / "project"
    project.mkdir()
    out = run_plugin({
        "plugin": str(BUILTIN_DIR / "html5-export"), "kind": "export", "engine": "html5", "target": WEB,
        "manifest": {"name": "p", "version": "1.0.0", "options": RECOMPRESS},
        "folders": {"$GAME": str(game_dir), "$PROJECT": str(project), "$OUTPUT": str(tmp_path / "out")},
        "cache_root": str(tmp_path / "cache"),
        "settings": {"download_tools": "false", **{f"tool.{t}": "" for t in ()}},
    }, lambda m, p: None)
    a = out["result"]["assets"]
    assert a["profile"] == "html5" and a["categories"]["textures"]["converted"] >= 1
    assert (tmp_path / "out" / "img" / "a.png").stat().st_size < (game_dir / "img" / "a.png").stat().st_size
    # The game itself is never touched.
    assert (game_dir / "img" / "a.png").read_bytes() == png_bytes(128, 64, level=0)
    # Plan mode builds nothing.
    plan = run_plugin({
        "plugin": str(BUILTIN_DIR / "html5-export"), "kind": "export", "engine": "html5", "target": WEB,
        "manifest": {"name": "p", "version": "1.0.0", "options": RECOMPRESS},
        "folders": {"$GAME": str(game_dir), "$PROJECT": str(project)},
        "cache_root": str(tmp_path / "cache2"), "settings": {"assets": "plan", "download_tools": "false"},
    }, lambda m, p: None)
    assert plan["result"]["assets"]["estimated"] and not plan["result"].get("outputs")


# -- tools --------------------------------------------------------------------------------


def fake_tool(folder: Path, name: str, version: str = "9.9.9") -> Path:
    folder.mkdir(parents=True, exist_ok=True)
    p = folder / name
    p.write_text(f"#!/bin/sh\necho '{name} {version}'\n")
    p.chmod(0o755)
    return p


def test_tool_lookup_order(tmp_path, monkeypatch):
    monkeypatch.setattr(tools, "EXTRA_DIRS", ())
    tools._VERSIONS.clear()
    assert tools.find("oxipng", tmp_path / "tools", search_path="") is None
    path_dir = tmp_path / "bin"
    fake_tool(path_dir, "oxipng", "1.2.3")
    found = tools.find("oxipng", tmp_path / "tools", search_path=str(path_dir))
    assert found.source == "system" and found.version == "1.2.3"
    chosen = fake_tool(tmp_path / "mine", "oxipng", "7.0.0")
    found = tools.find("oxipng", tmp_path / "tools", override=str(chosen), search_path=str(path_dir))
    assert found.source == "setting" and found.version == "7.0.0"
    with pytest.raises(tools.ToolError):
        tools.find("oxipng", None, override=str(tmp_path / "nope"))


def test_pins_are_https_github_with_a_sha256():
    for tool in tools.TOOLS.values():
        for key, pin in tool.pins.items():
            tools._check_url(pin.url)
            assert len(pin.sha256) == 64 and int(pin.sha256, 16) >= 0, (tool.id, key)
            assert pin.archive in ("tar.gz", "zip", "raw")
    with pytest.raises(tools.ToolError):
        tools._check_url("https://example.com/oxipng.tar.gz")
    with pytest.raises(tools.ToolError):
        tools._check_url("http://github.com/x")


def tarball(name: str, body: bytes) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        info = tarfile.TarInfo(f"oxipng-1.0/{name}")
        info.size = len(body)
        tf.addfile(info, io.BytesIO(body))
    return buf.getvalue()


def test_install_checks_the_hash(tmp_path, monkeypatch):
    body = b"#!/bin/sh\necho 'oxipng 1.0.0'\n"
    archive = tarball("oxipng", body)
    good = hashlib.sha256(archive).hexdigest()
    pin = tools.Pin("1.0.0", "https://github.com/oxipng/oxipng/releases/download/v1/x.tar.gz", good, "tar.gz")
    tool = tools.TOOLS["oxipng"]
    monkeypatch.setitem(tools.TOOLS, "oxipng", tools.Tool(**{**tool.__dict__, "pins": {tools.platform_key(): pin}}))
    monkeypatch.setattr(tools, "EXTRA_DIRS", ())
    monkeypatch.setenv("PATH", "")

    def fetch(url, dest):
        Path(dest).write_bytes(archive)

    found = tools.install("oxipng", tmp_path / "tools", fetch=fetch)
    assert found.source == "downloaded" and Path(found.path).read_bytes() == body
    assert os.access(found.path, os.X_OK)
    assert tools.find("oxipng", tmp_path / "tools", search_path="").source == "downloaded"

    bad = tools.Pin("2.0.0", pin.url, "0" * 64, "tar.gz")
    monkeypatch.setitem(tools.TOOLS, "oxipng", tools.Tool(**{**tool.__dict__, "pins": {tools.platform_key(): bad}}))
    with pytest.raises(tools.ToolError, match="SHA-256"):
        tools.install("oxipng", tmp_path / "tools2", fetch=fetch)
    assert not (tmp_path / "tools2" / "oxipng" / "2.0.0" / tools.platform_key()).exists()


def test_status_lists_every_tool(tmp_path):
    ids = [t["id"] for t in tools.status(tmp_path)]
    for t in ("ffmpeg", "ffprobe", "cwebp", "oxipng", "pngquant", "oggenc"):
        assert t in ids


# -- real tools (only with PTK_TOOL_TESTS=1) -------------------------------------------------

real = pytest.mark.skipif(os.environ.get("PTK_TOOL_TESTS") != "1" or not shutil.which("ffmpeg"),
                          reason="set PTK_TOOL_TESTS=1 with FFmpeg installed")


@real
def test_real_audio_keeps_loop_tags(tmp_path):
    import subprocess
    root = tmp_path / "game" / "audio"
    root.mkdir(parents=True)
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=4",
                    "-c:a", "libopus", "-b:a", "256k", "-metadata", "LOOPSTART=1000", root / "a.ogg"], check=True)
    tb = tools.Toolbox(None, auto_install=False)
    report = Pipeline(tb, None).run(tmp_path / "game", "html5", {"audio": {"action": "recompress"}}, WEB)
    a = report["categories"]["audio"]
    assert a["converted"] == 1, a
    from ptk_engine.media.probe import media_info
    info = media_info(root / "a.ogg", tb.path("ffprobe"))
    assert info.tags.get("loopstart") == "1000" and info.audio == "opus"
