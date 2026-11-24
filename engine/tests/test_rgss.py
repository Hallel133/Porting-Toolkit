"""RPG Maker XP/VX/VX Ace: Game.ini, archives, and the export with mkxp-z."""

from __future__ import annotations

import json
import os
import plistlib
import zipfile
from pathlib import Path

import pytest

from ptk_engine.formats import rgss
from ptk_engine.runtime import BUILTIN_DIR, find_plugins, run_plugin
from ptk_engine.signatures import GameIndex, detect

WINDOWS = {"id": "windows-x64", "name": "Windows x64", "os": "windows", "arch": "x86_64"}
LINUX = {"id": "linux-x64", "name": "Linux x64", "os": "linux", "arch": "x86_64"}
MAC = {"id": "macos-arm64", "name": "macOS (Apple Silicon)", "os": "macos", "arch": "arm64"}


def ace_game(tmp: Path, ini_encoding: str = "utf-8") -> Path:
    g = tmp / "StarlightAce"
    (g / "System").mkdir(parents=True)
    (g / "Audio" / "BGM").mkdir(parents=True)
    ini = "[Game]\r\nRTP=RPGVXAce\r\nLibrary=System\\RGSS301.dll\r\nScripts=Data\\Scripts.rvdata2\r\nTitle=星の灯\r\n"
    (g / "Game.ini").write_bytes(ini.encode(ini_encoding))
    (g / "Game.exe").write_bytes(b"MZ")
    (g / "System" / "RGSS301.dll").write_bytes(b"MZ")
    (g / "Game.rgss3a").write_bytes(b"RGSSAD\x00\x03" + b"\0" * 64)
    (g / "Audio" / "BGM" / "theme.ogg").write_bytes(b"OggS")
    (g / "Save01.rvdata2").write_bytes(b"my progress")
    return g


def xp_game(tmp: Path) -> Path:
    g = tmp / "OldQuest"
    (g / "Data").mkdir(parents=True)
    (g / "Game.ini").write_text("[Game]\nLibrary=RGSS104E.dll\nScripts=Data\\Scripts.rxdata\nTitle=Old Quest\n"
                                "RTP1=Standard\nRTP2=\nRTP3=\n")
    (g / "RGSS104E.dll").write_bytes(b"MZ")
    (g / "Data" / "Scripts.rxdata").write_bytes(b"\x04\x08")
    return g


def fake_mac_mkxp(tmp: Path) -> Path:
    app = tmp / "Z-universal.app"
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "Game").mkdir()
    (app / "Contents" / "Game" / "placeholder.txt").write_text("x")
    (app / "Contents" / "Info.plist").write_bytes(plistlib.dumps({"CFBundleExecutable": "Z-universal",
                                                                   "CFBundleIdentifier": "org.zoro.mkxp-z"}))
    exe = app / "Contents" / "MacOS" / "Z-universal"
    exe.write_bytes(b"\xcf\xfa\xed\xfe")
    exe.chmod(0o755)
    return app


def fake_linux_mkxp(tmp: Path) -> Path:
    d = tmp / "mkxp-z-linux"
    (d / "lib64").mkdir(parents=True)
    exe = d / "mkxp-z.x86_64"
    exe.write_bytes(b"\x7fELF")
    exe.chmod(0o755)
    (d / "lib64" / "libruby.so").write_bytes(b"\x7fELF")
    (d / "mkxp.json").write_text("{}")
    return d


def run(tmp: Path, plugin: str, game: Path, kind: str, target=None, settings=None, output=None):
    project = tmp / "project"
    project.mkdir(exist_ok=True)
    return run_plugin({
        "plugin": str(BUILTIN_DIR / plugin), "kind": kind, "engine": "rgss", "target": target,
        "manifest": {"name": "p", "version": "1.0.0", "game": {"title": "Starlight"}},
        "settings": settings or {},
        "folders": {"$GAME": str(game), "$PROJECT": str(project), "$OUTPUT": str(output) if output else None},
        "cache_root": str(tmp / "cache"),
    }, lambda m, p: None)["result"]


@pytest.mark.parametrize("encoding", ["utf-8", "cp932"])
def test_detect_vx_ace_with_a_shift_jis_ini(tmp_path, encoding):
    g = rgss.detect(ace_game(tmp_path, encoding))
    assert (g.rgss, g.variant, g.title, g.rtp) == (3, "VX Ace", "星の灯", ["RPGVXAce"])
    assert g.archive == {"path": "Game.rgss3a", "format": 3}
    assert any("RTP" in n for n in g.notes)


def test_detect_xp(tmp_path):
    g = rgss.detect(xp_game(tmp_path))
    assert (g.rgss, g.variant, g.title, g.rtp, g.archive) == (1, "XP", "Old Quest", ["Standard"], None)


def test_not_an_rgss_game(tmp_path):
    (tmp_path / "x").mkdir()
    (tmp_path / "x" / "settings.ini").write_text("[Display]\nwidth=1\n")
    with pytest.raises(rgss.RgssError):
        rgss.detect(tmp_path / "x")


def test_saves_are_recognised():
    assert rgss.is_save("Save01.rvdata2") and rgss.is_save("save3.rxdata")
    assert not rgss.is_save("Scripts.rvdata2")


def test_scan_says_rgss_not_html5():
    index = GameIndex(["Game.exe", "Game.ini", "Game.rgss3a", "System/RGSS301.dll"], ["System"])
    found = detect(index)
    assert found[0]["id"] == "rgss" and found[0]["variant"] == "VX Ace"
    assert [r["plugin"]["id"] for r in find_plugins("import", "rgss", None, [])] == ["rgss-import"]
    assert [r["plugin"]["id"] for r in find_plugins("export", "rgss", "linux-x64", [])] == ["rgss-export"]
    assert find_plugins("import", "rpgmaker", None, [])[0]["plugin"]["id"] == "html5-import"


def test_import(tmp_path):
    r = run(tmp_path, "rgss-import", ace_game(tmp_path), "import")
    assert r["title"] == "星の灯" and r["summary"] == "RPG Maker VX Ace game (RGSS3), packed in Game.rgss3a"


def test_windows_export_keeps_the_game_and_drops_saves(tmp_path):
    run(tmp_path, "rgss-export", ace_game(tmp_path), "export", WINDOWS, output=tmp_path / "win")
    folder = tmp_path / "win" / "Starlight"
    assert (folder / "Game.exe").is_file() and (folder / "Game.rgss3a").is_file()
    assert not (folder / "Save01.rvdata2").exists()
    assert (tmp_path / "win" / "Starlight-windows-x64.zip").is_file()


def test_macos_export_puts_the_game_in_contents_game(tmp_path):
    game = ace_game(tmp_path)
    mkxp = fake_mac_mkxp(tmp_path)
    rtp = tmp_path / "RPGVXAce"
    (rtp / "Graphics").mkdir(parents=True)
    (rtp / "Graphics" / "a.png").write_bytes(b"png")
    r = run(tmp_path, "rgss-export", game, "export", MAC, {"mkxp": str(mkxp), "rtp": str(rtp), "sign": "false"},
            tmp_path / "mac")
    app = tmp_path / "mac" / "Starlight.app"
    game_dir = app / "Contents" / "Game"
    assert (game_dir / "Game.ini").is_file() and not (game_dir / "placeholder.txt").exists()
    assert not (game_dir / "Save01.rvdata2").exists()
    config = json.loads((game_dir / "mkxp.json").read_text())
    assert config == {"rgssVersion": 3, "windowTitle": "Starlight", "RTP": ["RTP"]}
    assert (game_dir / "RTP" / "Graphics" / "a.png").is_file()
    plist = plistlib.loads((app / "Contents" / "Info.plist").read_bytes())
    assert plist["CFBundleIdentifier"] == "org.portingtoolkit.starlight" and plist["CFBundleExecutable"] == "Z-universal"
    assert "GPL" in (app / "Contents" / "Resources" / "mkxp-z-LICENSE.txt").read_text()
    assert any("Downloads" in n for n in r["notes"])
    # The mkxp-z path is remembered for the next export.
    run(tmp_path, "rgss-export", game, "export", MAC, {"sign": "false"}, tmp_path / "mac2")
    assert (tmp_path / "mac2" / "Starlight.app" / "Contents" / "Game" / "mkxp.json").is_file()


def test_linux_export(tmp_path):
    game = ace_game(tmp_path)
    r = run(tmp_path, "rgss-export", game, "export", LINUX, {"mkxp": str(fake_linux_mkxp(tmp_path))},
            tmp_path / "linux")
    folder = tmp_path / "linux" / "Starlight"
    assert (folder / "Game.ini").is_file() and (folder / "lib64" / "libruby.so").is_file()
    assert json.loads((folder / "mkxp.json").read_text())["rgssVersion"] == 3
    assert any("RTP" in w for w in r["warnings"]), "no --set rtp: warned"
    with zipfile.ZipFile(tmp_path / "linux" / "Starlight-linux-x64.zip") as z:
        modes = {i.filename: (i.external_attr >> 16) & 0o777 for i in z.infolist()}
    assert modes["Starlight/mkxp-z.x86_64"] == 0o755 and modes["Starlight/Game.ini"] == 0o644
    assert os.access(folder / "mkxp-z.x86_64", os.X_OK)


def test_missing_mkxp_explains_where_to_get_it(tmp_path):
    from ptk_engine import EngineError
    with pytest.raises(EngineError, match="--set mkxp=PATH"):
        run(tmp_path, "rgss-export", ace_game(tmp_path), "export", LINUX, output=tmp_path / "linux")
