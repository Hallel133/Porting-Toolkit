"""LÖVE: finding the .love, conf.lua, and the export to macOS, Windows and Linux."""

from __future__ import annotations

import io
import os
import plistlib
import re
import struct
import zipfile
import zlib
from pathlib import Path

import pytest

from ptk_engine.formats import love
from ptk_engine.runtime import BUILTIN_DIR, find_plugins, run_plugin

MAC = {"id": "macos-arm64", "name": "macOS (Apple Silicon)", "os": "macos", "arch": "arm64"}
WINDOWS = {"id": "windows-x64", "name": "Windows x64", "os": "windows", "arch": "x86_64"}
LINUX = {"id": "linux-x64", "name": "Linux x64", "os": "linux", "arch": "x86_64"}

CONF = """-- configuration
function love.conf(t)
    t.identity = "moon-jumper"   -- save folder
    t.version = "11.4"
    --[[ t.window.title = "Not this one" ]]
    t.window.title = "Moon Jumper"
    t.window.width = 960
    t.window.height = 540
    t.window.icon = "assets/icon.png"
end
"""


def png(w: int = 4, h: int = 4) -> bytes:
    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    rows = b"".join(b"\0" + bytes((x * 9 + y) % 256 for x in range(w * 4)) for y in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 0)) + chunk(b"IEND", b""))


GAME = {"main.lua": b"function love.draw() end\n", "conf.lua": CONF.encode(), "assets/icon.png": png(),
        "assets/big.png": png(96, 64)}


def love_bytes(files: dict = GAME, root: str = "") -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(f"{root}/{name}" if root else name, data)
    return buf.getvalue()


def run(tmp: Path, plugin: str, game: Path, kind: str, target=None, settings=None, output=None, options=None):
    project = tmp / "project"
    project.mkdir(exist_ok=True)
    return run_plugin({
        "plugin": str(BUILTIN_DIR / plugin), "kind": kind, "engine": "love", "target": target,
        "manifest": {"name": "p", "version": "2.0.0", "game": {"title": "Moon Jumper"}, "options": options or {}},
        "settings": settings or {},
        "folders": {"$GAME": str(game), "$PROJECT": str(project), "$OUTPUT": str(output) if output else None},
        "cache_root": str(tmp / "cache"),
    }, lambda m, p: None)["result"]


# -- finding the game ----------------------------------------------------------------


def test_conf_lua_is_read_without_running_it():
    c = love.parse_conf(CONF)
    assert c == {"love_version": "11.4", "identity": "moon-jumper", "title": "Moon Jumper",
                 "width": 960, "height": 540, "icon": "assets/icon.png"}


def test_detect_love_file_fused_exe_app_and_folder(tmp_path):
    a = tmp_path / "a"
    a.mkdir()
    (a / "game.love").write_bytes(love_bytes())
    g = love.detect(a)
    assert (g.kind, g.source, g.title, g.love_version) == ("love", "game.love", "Moon Jumper", "11.4")

    b = tmp_path / "b"
    b.mkdir()
    (b / "MoonJumper.exe").write_bytes(b"MZ" + b"\0" * 5000 + love_bytes())
    (b / "love.dll").write_bytes(b"MZ")
    g = love.detect(b)
    assert (g.kind, g.source, g.identity) == ("fused", "MoonJumper.exe", "moon-jumper")

    c = tmp_path / "c" / "Moon.app" / "Contents" / "Resources"
    c.mkdir(parents=True)
    (c / "game.love").write_bytes(love_bytes())
    assert love.detect(tmp_path / "c").kind == "app"

    d = tmp_path / "d"
    for name, data in GAME.items():
        (d / name).parent.mkdir(parents=True, exist_ok=True)
        (d / name).write_bytes(data)
    g = love.detect(d)
    assert (g.kind, g.files) == ("folder", 4)

    e = tmp_path / "e"
    e.mkdir()
    (e / "game.love").write_bytes(love_bytes(root="MoonJumper"))
    g = love.detect(e)
    assert g.root == "MoonJumper" and g.notes

    empty = tmp_path / "f"
    empty.mkdir()
    (empty / "readme.txt").write_text("not a game")
    with pytest.raises(love.LoveError):
        love.detect(empty)


def test_runtime_links_must_stay_inside(tmp_path):
    bad = tmp_path / "bad.zip"
    with zipfile.ZipFile(bad, "w") as z:
        info = zipfile.ZipInfo("love.app/Contents/Frameworks/x")
        info.external_attr = (0o120777 << 16)
        z.writestr(info, "../../../../../etc/passwd")
    with pytest.raises(love.LoveError):
        love.extract_runtime(bad, tmp_path / "out")


def test_import_records_the_game_and_caches_its_icon(tmp_path):
    game = tmp_path / "game"
    game.mkdir()
    (game / "MoonJumper.exe").write_bytes(b"MZ" + b"\0" * 100 + love_bytes())
    r = run(tmp_path, "love-import", game, "import")
    assert r["title"] == "Moon Jumper" and "LÖVE 11.4" in r["summary"]
    assert Path(r["icon"]).read_bytes() == GAME["assets/icon.png"]
    assert not list((tmp_path / "project").rglob("*.png")), "the icon stays out of the project"


# -- exports ---------------------------------------------------------------------------


def fake_windows_runtime(tmp: Path) -> Path:
    path = tmp / "love-win64.zip"
    with zipfile.ZipFile(path, "w") as z:
        for name, data in {"love.exe": b"MZ-love", "lovec.exe": b"MZ-c", "love.dll": b"MZ-dll",
                           "SDL2.dll": b"MZ-sdl", "license.txt": b"zlib", "readme.txt": b"r"}.items():
            z.writestr(f"love-11.5-win64/{name}", data)
    return path


def fake_mac_runtime(tmp: Path) -> Path:
    path = tmp / "love-macos.zip"
    plist = plistlib.dumps({"CFBundleExecutable": "love", "CFBundleIdentifier": "org.love2d.love",
                            "CFBundleName": "LÖVE", "CFBundleIconName": "OS X AppIcon",
                            "UTExportedTypeDeclarations": [{"UTTypeIdentifier": "org.love2d.love-game"}],
                            "CFBundleDocumentTypes": [{"CFBundleTypeName": "LÖVE Project"}]})
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("love.app/Contents/Info.plist", plist)
        exe = zipfile.ZipInfo("love.app/Contents/MacOS/love")
        exe.external_attr = 0o100755 << 16
        z.writestr(exe, b"\xcf\xfa\xed\xfe")
        z.writestr("love.app/Contents/Frameworks/SDL2.framework/Versions/A/SDL2", b"\xcf\xfa\xed\xfe")
        link = zipfile.ZipInfo("love.app/Contents/Frameworks/SDL2.framework/SDL2")
        link.external_attr = 0o120777 << 16
        z.writestr(link, "Versions/A/SDL2")
    return path


def love_game(tmp: Path) -> Path:
    game = tmp / "game"
    game.mkdir()
    (game / "game.love").write_bytes(love_bytes())
    return game


def test_windows_export_fuses_the_game(tmp_path):
    game = love_game(tmp_path)
    r = run(tmp_path, "love-export", game, "export", WINDOWS, {"love": str(fake_windows_runtime(tmp_path))},
            tmp_path / "win")
    folder = tmp_path / "win" / "Moon Jumper"
    exe = (folder / "Moon Jumper.exe").read_bytes()
    assert exe.startswith(b"MZ-love")
    with zipfile.ZipFile(folder / "Moon Jumper.exe") as z:  # the fused game is readable from the .exe
        assert z.read("main.lua") == GAME["main.lua"]
    assert {p.name for p in folder.iterdir()} == {"Moon Jumper.exe", "love.dll", "SDL2.dll", "license.txt",
                                                  "README.txt"}
    assert (tmp_path / "win" / "Moon Jumper-windows-x64.zip").is_file()
    assert r["open"] == str(folder)


def test_linux_export_ships_the_appimage_and_a_start_script(tmp_path):
    game = love_game(tmp_path)
    appimage = tmp_path / "love-11.5-x86_64.AppImage"
    appimage.write_bytes(b"\x7fELF-appimage")
    run(tmp_path, "love-export", game, "export", LINUX, {"love": str(appimage)}, tmp_path / "linux")
    folder = tmp_path / "linux" / "Moon Jumper"
    script = folder / "Moon-Jumper"
    assert os.access(script, os.X_OK) and os.access(folder / appimage.name, os.X_OK)
    assert "--fused game.love" in script.read_text()
    with zipfile.ZipFile(folder / "game.love") as z:
        assert z.read("conf.lua") == GAME["conf.lua"]
    assert (folder / "icon.png").read_bytes() == GAME["assets/icon.png"]
    with zipfile.ZipFile(tmp_path / "linux" / "Moon Jumper-linux-x64.zip") as z:
        modes = {i.filename: (i.external_attr >> 16) & 0o777 for i in z.infolist()}
    assert modes["Moon Jumper/Moon-Jumper"] == 0o755 and modes["Moon Jumper/game.love"] == 0o644


def test_macos_export_renames_the_app_and_keeps_framework_links(tmp_path):
    game = love_game(tmp_path)
    r = run(tmp_path, "love-export", game, "export", MAC,
            {"love": str(fake_mac_runtime(tmp_path)), "sign": "false"}, tmp_path / "mac")
    app = tmp_path / "mac" / "Moon Jumper.app"
    assert r["outputs"] == [str(app)]
    assert zipfile.is_zipfile(app / "Contents" / "Resources" / "game.love")
    plist = plistlib.loads((app / "Contents" / "Info.plist").read_bytes())
    assert plist["CFBundleIdentifier"] == "org.portingtoolkit.moon-jumper"
    assert plist["CFBundleDisplayName"] == "Moon Jumper" and plist["CFBundleShortVersionString"] == "2.0.0"
    assert "UTExportedTypeDeclarations" not in plist and "CFBundleDocumentTypes" not in plist
    link = app / "Contents" / "Frameworks" / "SDL2.framework" / "SDL2"
    assert link.is_symlink() and os.readlink(link) == "Versions/A/SDL2"
    assert os.access(app / "Contents" / "MacOS" / "love", os.X_OK)


def test_assets_are_recompressed_inside_the_love(tmp_path):
    game = love_game(tmp_path)
    r = run(tmp_path, "love-export", game, "export", WINDOWS, {"love": str(fake_windows_runtime(tmp_path)),
                                                               "download_tools": "false"},
            tmp_path / "win", options={"textures": {"action": "recompress"}})
    assert r["assets"]["profile"] == "love" and r["assets"]["categories"]["textures"]["converted"] >= 1
    with zipfile.ZipFile(tmp_path / "win" / "Moon Jumper" / "Moon Jumper.exe") as z:
        assert len(z.read("assets/big.png")) < len(GAME["assets/big.png"])


def test_love_pins_and_plugins():
    import importlib
    import sys
    path = str(BUILTIN_DIR / "love-export")
    if path not in sys.path:
        sys.path.insert(0, path)
    m = importlib.import_module("love_export")
    for name, sha in m.LOVE.values():
        assert re.fullmatch(r"[0-9a-f]{64}", sha) and m.LOVE_VERSION in name
    ids = lambda **kw: [r["plugin"]["id"] for r in find_plugins(dirs=[], **kw)]  # noqa: E731
    assert ids(kind="import", engine="love", target=None) == ["love-import"]
    assert ids(kind="export", engine="love", target="linux-x64") == ["love-export"]
    assert ids(kind="export", engine="love", target="web") == []
