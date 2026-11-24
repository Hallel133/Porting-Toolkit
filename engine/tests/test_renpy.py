"""Ren'Py: archives, detection and the export to Windows, Linux and macOS."""

from __future__ import annotations

import os
import pickle
import plistlib
import struct
import zipfile
import zlib
from pathlib import Path

import pytest

from ptk_engine.formats import renpy
from ptk_engine.runtime import BUILTIN_DIR, find_plugins, run_plugin

WINDOWS = {"id": "windows-x64", "name": "Windows x64", "os": "windows", "arch": "x86_64"}
LINUX = {"id": "linux-x64", "name": "Linux x64", "os": "linux", "arch": "x86_64"}
MAC = {"id": "macos-arm64", "name": "macOS (Apple Silicon)", "os": "macos", "arch": "arm64"}


def png(w: int = 64, h: int = 48) -> bytes:
    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    rows = b"".join(b"\0" + bytes((x * 5 + y) % 256 for x in range(w * 4)) for y in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 0)) + chunk(b"IEND", b""))


def pc_game(tmp: Path) -> Path:
    """A Ren'Py 8 "PC" build (Windows + Linux) of a game named Moonlit."""
    g = tmp / "Moonlit-1.0-pc"
    files = {
        "Moonlit.exe": b"MZ-renpy", "Moonlit.sh": b"#!/bin/sh\n", "Moonlit.py": b"# launcher\n",
        "log.txt": b"personal log", "traceback.txt": b"x",
        "game/script.rpyc": b"rpyc", "game/options.rpy": b'define config.name = _("Moonlit Garden")\n',
        "game/images/bg.png": png(), "game/saves/1-1-LT1.save": b"my save", "game/cache/bytecode.rpyb": b"c",
        "renpy/vc_version.py": b"branch = 'fix'\nversion = '8.3.7.25031702'\n",
        "renpy/__init__.py": b"# engine\n",
        "lib/python3.9/os.py": b"# stdlib\n",
        "lib/py3-windows-x86_64/renpy.exe": b"MZ-renpy", "lib/py3-windows-x86_64/librenpython.dll": b"MZ",
        "lib/py3-linux-x86_64/Moonlit": b"\x7fELF", "lib/py3-linux-x86_64/librenpython.so": b"\x7fELF",
    }
    for rel, data in files.items():
        (g / rel).parent.mkdir(parents=True, exist_ok=True)
        (g / rel).write_bytes(data)
    (g / "lib/py3-linux-x86_64/Moonlit").chmod(0o755)
    renpy.write_rpa(g / "game" / "archive.rpa", {"images/a.png": png(8, 8), "script.rpyc": b"x"})
    return g


def fake_sdk(tmp: Path) -> Path:
    path = tmp / "renpy-8.3.7-sdk.zip"
    with zipfile.ZipFile(path, "w") as z:
        for rel, data, mode in [
            ("lib/py3-mac-universal/renpy", b"\xcf\xfa\xed\xfe", 0o755),
            ("lib/py3-mac-universal/librenpython.dylib", b"\xcf\xfa\xed\xfe", 0o644),
            ("lib/py3-mac-universal/python", b"\xcf\xfa\xed\xfe", 0o755),
            ("lib/python3.9/os.py", b"# stdlib\n", 0o644),
            ("renpy.py", b"# launcher\n", 0o644),
            ("renpy.sh", b"#!/bin/sh\n", 0o755),
            ("LICENSE.txt", b"MIT", 0o644),
            ("launcher/game/big.rpy", b"not needed", 0o644),
        ]:
            info = zipfile.ZipInfo(f"renpy-8.3.7-sdk/{rel}")
            info.external_attr = (0o100000 | mode) << 16
            z.writestr(info, data)
    return path


def run(tmp: Path, plugin: str, game: Path, kind: str, target=None, settings=None, output=None, options=None):
    project = tmp / "project"
    project.mkdir(exist_ok=True)
    return run_plugin({
        "plugin": str(BUILTIN_DIR / plugin), "kind": kind, "engine": "renpy", "target": target,
        "manifest": {"name": "p", "version": "1.0.0", "game": {"title": "Moonlit Garden"}, "options": options or {}},
        "settings": {"download_tools": "false", **(settings or {})},
        "folders": {"$GAME": str(game), "$PROJECT": str(project), "$OUTPUT": str(output) if output else None},
        "cache_root": str(tmp / "cache"),
    }, lambda m, p: None)["result"]


# -- archives and detection ------------------------------------------------------------


def test_rpa_index_round_trip(tmp_path):
    p = renpy.write_rpa(tmp_path / "a.rpa", {"x.png": b"12345", "y.txt": b"hello"})
    fmt, index = renpy.read_rpa_index(p)
    assert fmt == "RPA-3.0" and sorted(index) == ["x.png", "y.txt"]
    off, length, _ = index["y.txt"][0]
    assert p.read_bytes()[off:off + length] == b"hello"


def test_a_malicious_archive_index_is_refused(tmp_path):
    class Evil:
        def __reduce__(self):
            return (os.system, ("echo pwned",))
    body = zlib.compress(pickle.dumps({"x": Evil()}, protocol=2))
    p = tmp_path / "evil.rpa"
    p.write_bytes(f"RPA-3.0 {34:016x} {0:08x}\n".encode() + body)
    with pytest.raises(renpy.RenpyError, match="refusing"):
        renpy.read_rpa_index(p)


def test_version_and_title_parsing():
    assert renpy.parse_version("version = '8.3.7.25031702'\n") == "8.3.7.25031702"
    assert renpy.parse_version("version_tuple = (7, 4, 11, vc_version)") == "7.4.11"
    assert renpy.parse_title('define config.name = _("Moonlit Garden")') == "Moonlit Garden"
    assert renpy.parse_title("define config.name = 'Plain'") == "Plain"


def test_detect_a_pc_build(tmp_path):
    g = renpy.detect(pc_game(tmp_path))
    assert (g.version, g.short_version, g.py, g.executable_name) == ("8.3.7.25031702", "8.3.7", 3, "Moonlit")
    assert set(g.platforms) == {"windows-x86_64", "linux-x86_64"}
    assert g.python_lib == "lib/python3.9" and g.title == "Moonlit Garden" and g.script_source
    assert g.archives[0]["format"] == "RPA-3.0" and g.archives[0]["files"] == 2


def test_import(tmp_path):
    r = run(tmp_path, "renpy-import", pc_game(tmp_path), "import")
    assert r["title"] == "Moonlit Garden"
    assert r["summary"].startswith("Ren'Py 8.3.7 game (Windows, Linux)")


# -- exports ---------------------------------------------------------------------------------


def test_windows_export_from_the_games_own_files(tmp_path):
    game = pc_game(tmp_path)
    r = run(tmp_path, "renpy-export", game, "export", WINDOWS, output=tmp_path / "win")
    folder = tmp_path / "win" / "Moonlit Garden"
    assert (folder / "Moonlit.exe").read_bytes() == b"MZ-renpy" and (folder / "Moonlit.py").is_file()
    assert (folder / "lib/py3-windows-x86_64/librenpython.dll").is_file()
    assert not (folder / "lib/py3-linux-x86_64").exists() and not (folder / "Moonlit.sh").exists()
    assert (folder / "lib/python3.9/os.py").is_file() and (folder / "game/archive.rpa").is_file()
    assert not (folder / "game/saves").exists(), "the player's saves never travel"
    assert not (folder / "game/cache").exists() and not (folder / "log.txt").exists()
    assert not (tmp_path / "cache" / "renpy-export" / "sdk").exists(), "no SDK needed"
    assert any(".rpa" in n for n in r["notes"])


def test_linux_export_keeps_executables(tmp_path):
    game = pc_game(tmp_path)
    run(tmp_path, "renpy-export", game, "export", LINUX, output=tmp_path / "linux")
    folder = tmp_path / "linux" / "Moonlit Garden"
    assert os.access(folder / "Moonlit.sh", os.X_OK) and os.access(folder / "lib/py3-linux-x86_64/Moonlit", os.X_OK)
    with zipfile.ZipFile(tmp_path / "linux" / "Moonlit Garden-linux-x64.zip") as z:
        modes = {i.filename: (i.external_attr >> 16) & 0o777 for i in z.infolist()}
    assert modes["Moonlit Garden/Moonlit.sh"] == 0o755
    assert modes["Moonlit Garden/game/script.rpyc"] == 0o644


def test_macos_export_uses_the_sdk(tmp_path):
    game = pc_game(tmp_path)
    r = run(tmp_path, "renpy-export", game, "export", MAC, {"sdk": str(fake_sdk(tmp_path)), "sign": "false"},
            tmp_path / "mac")
    app = tmp_path / "mac" / "Moonlit Garden.app"
    assert r["outputs"] == [str(app)]
    contents = app / "Contents"
    assert os.access(contents / "MacOS/Moonlit", os.X_OK) and (contents / "MacOS/librenpython.dylib").is_file()
    assert (contents / "Resources/lib/python3.9/os.py").is_file()
    autorun = contents / "Resources/autorun"
    assert (autorun / "game/script.rpyc").is_file() and (autorun / "Moonlit.py").is_file()
    assert (autorun / "renpy/vc_version.py").is_file() and not (autorun / "game/saves").exists()
    plist = plistlib.loads((contents / "Info.plist").read_bytes())
    assert plist["CFBundleExecutable"] == "Moonlit" and plist["CFBundleDisplayName"] == "Moonlit Garden"
    sdk = tmp_path / "cache" / "renpy-export" / "sdk"
    unpacked = next(sdk.iterdir())
    assert not (unpacked / "launcher").exists(), "only the platform files are unpacked"


def test_loose_images_are_recompressed(tmp_path):
    game = pc_game(tmp_path)
    r = run(tmp_path, "renpy-export", game, "export", WINDOWS, output=tmp_path / "win",
            options={"textures": {"action": "recompress"}})
    assert r["assets"]["profile"] == "renpy"
    out = tmp_path / "win" / "Moonlit Garden" / "game/images/bg.png"
    assert out.stat().st_size < (game / "game/images/bg.png").stat().st_size


def test_checksums_txt_parsing_and_plugins():
    import importlib
    import sys
    path = str(BUILTIN_DIR / "renpy-export")
    if path not in sys.path:
        sys.path.insert(0, path)
    m = importlib.import_module("renpy_export")
    text = ("# md5\nabc renpy-8.3.7-sdk.zip\n# sha256\n" + "a" * 64 + " renpy-8.3.7-sdk.zip\n"
            + "b" * 64 + " renpy-8.3.7-web.zip\n")
    assert m.sha256_for(text, "renpy-8.3.7-sdk.zip") == "a" * 64
    assert m.sha256_for(text, "nope.zip") is None
    ids = lambda **kw: [r["plugin"]["id"] for r in find_plugins(dirs=[], **kw)]  # noqa: E731
    assert ids(kind="import", engine="renpy", target=None) == ["renpy-import"]
    assert ids(kind="export", engine="renpy", target="macos-arm64") == ["renpy-export"]
