"""RuntimeExportPlugin: open-source runtimes found, laid out and launched.

Fake runtimes only (shell scripts and empty apps made here); nothing is
downloaded and no runtime is run.
"""

from __future__ import annotations

import io
import os
import plistlib
import tarfile
import zipfile
from pathlib import Path

import pytest

from ptk_engine import EngineError
from ptk_engine import runtimes as rt
from ptk_engine.api import Context
from ptk_engine.runtime import run_plugin

TARGETS = {
    "macos-arm64": {"id": "macos-arm64", "os": "macos", "arch": "arm64", "name": "macOS (Apple Silicon)"},
    "windows-x64": {"id": "windows-x64", "os": "windows", "arch": "x64", "name": "Windows (x64)"},
    "linux-x64": {"id": "linux-x64", "os": "linux", "arch": "x64", "name": "Linux (x64)"},
}

RUNTIME_TOML = """
[runtime]
id = "fakevm"
name = "FakeVM"
version = "1.2.3"
source = "https://example.org/fakevm"
license = "GPL-3.0-or-later"

[runtime.executable]
macos = "FakeVM.app"
windows = "fakevm.exe"
linux = "fakevm.AppImage"

[launch]
args = ["--path={game}", "--auto-detect", "--iwad={main}"]

[game]
required = ["*.000", "*.la0"]
exclude = ["*.exe", "*.dll"]
main = ["*.000"]
"""

PLUGIN_TOML = """
schema_version = 1

[plugin]
id = "fakevm-export"
name = "FakeVM export"
version = "1.0.0"
kind = "export"
description = "Test"
license = "MIT"
authors = ["test"]
entry = "fakevm_export:Plugin"
api = 2

[formats]
inputs = ["scumm"]
targets = ["macos-arm64", "windows-x64", "linux-x64"]

[permissions]
read = ["$GAME", "$PROJECT"]
write = ["$OUTPUT"]
network = false
subprocess = true
"""


def make_plugin(tmp: Path, runtime_toml: str = RUNTIME_TOML) -> Path:
    plugin = tmp / "fakevm-export"
    (plugin / "fakevm_export").mkdir(parents=True)
    (plugin / "plugin.toml").write_text(PLUGIN_TOML)
    (plugin / "runtime.toml").write_text(runtime_toml)
    (plugin / "fakevm_export" / "__init__.py").write_text(
        "from ptk_engine.sdk import RuntimeExportPlugin\n\n\nclass Plugin(RuntimeExportPlugin):\n    pass\n")
    return plugin


def fake_app(where: Path) -> Path:
    app = where / "FakeVM.app"
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "Resources").mkdir()
    (app / "Contents" / "Info.plist").write_bytes(plistlib.dumps(
        {"CFBundleExecutable": "fakevm", "CFBundleIconFile": "FakeVM", "CFBundleIdentifier": "org.example.fakevm"}))
    exe = app / "Contents" / "MacOS" / "fakevm"
    exe.write_text("#!/bin/sh\necho fake\n")
    exe.chmod(0o755)
    (app / "Contents" / "Resources" / "FakeVM.icns").write_bytes(b"icns")
    os.symlink("fakevm", app / "Contents" / "MacOS" / "link-to-fakevm")
    return app


def fake_game(tmp: Path) -> Path:
    g = tmp / "Monkey Island 2"
    g.mkdir()
    (g / "MONKEY2.000").write_bytes(b"x" * 100)
    (g / "MONKEY2.001").write_bytes(b"y" * 200)
    (g / "SETUP.EXE").write_bytes(b"MZ")
    (g / ".DS_Store").write_bytes(b"junk")
    return g


def run(plugin: Path, game: Path, out: Path | None, tmp: Path, target: str, phase: str = "run",
        settings: dict | None = None) -> dict:
    (tmp / "project").mkdir(exist_ok=True)
    folders = {"$GAME": str(game), "$PROJECT": str(tmp / "project")}
    if out:
        folders["$OUTPUT"] = str(out)
    return run_plugin({"plugin": str(plugin), "kind": "export", "engine": "scumm", "target": TARGETS[target],
                       "phase": phase, "manifest": {"name": "p", "version": "1.0.0",
                                                    "game": {"title": "Monkey Island 2"}},
                       "settings": settings or {}, "folders": folders, "cache_root": str(tmp / "cache")},
                      lambda m, p: None)["result"]


def test_macos_app_wraps_the_runtime_and_the_game(tmp_path):
    plugin, game = make_plugin(tmp_path), fake_game(tmp_path)
    app_src = fake_app(tmp_path / "rt")
    out = tmp_path / "out"
    r = run(plugin, game, out, tmp_path, "macos-arm64", settings={"runtime": str(app_src), "sign": "false"})
    app = out / "Monkey Island 2.app"
    assert r["open"] == str(app) and "FakeVM 1.2.3" in r["summary"]
    res = app / "Contents" / "Resources"
    assert (res / "game" / "MONKEY2.000").is_file() and (res / "game" / "MONKEY2.001").is_file()
    assert not (res / "game" / "SETUP.EXE").exists() and not (res / "game" / ".DS_Store").exists()
    assert (res / "runtime" / "FakeVM.app" / "Contents" / "MacOS" / "link-to-fakevm").is_symlink()
    assert (res / "icon.icns").read_bytes() == b"icns"
    info = plistlib.loads((app / "Contents" / "Info.plist").read_bytes())
    assert info["CFBundleExecutable"] == "Monkey Island 2" and info["CFBundleIconFile"] == "icon"
    assert info["CFBundleIdentifier"] == "org.portingtoolkit.monkey-island-2"
    launcher = app / "Contents" / "MacOS" / "Monkey Island 2"
    text = launcher.read_text()
    assert os.access(launcher, os.X_OK)
    assert 'exec "${RUNTIME}/FakeVM.app/Contents/MacOS/fakevm" "--path=${GAME}" "--auto-detect"' in text
    assert '"--iwad=${GAME}/MONKEY2.000" "$@"' in text
    assert "GPL-3.0-or-later" in (res / "LICENSE-fakevm.txt").read_text()
    r = run(plugin, game, out, tmp_path, "macos-arm64", settings={"runtime": str(app_src), "sign": "false"})
    assert r["build"]["written"] == 0


def test_the_launcher_starts_the_runtime_with_the_game(tmp_path):
    """Runs the generated sh launcher with a fake runtime that prints its arguments."""
    plugin, game = make_plugin(tmp_path), fake_game(tmp_path)
    app_src = fake_app(tmp_path / "rt")
    (app_src / "Contents" / "MacOS" / "fakevm").write_text('#!/bin/sh\nfor a in "$@"; do echo "$a"; done\npwd\n')
    out = tmp_path / "out"
    run(plugin, game, out, tmp_path, "macos-arm64", settings={"runtime": str(app_src), "sign": "false"})
    import subprocess
    app = out / "Monkey Island 2.app"
    r = subprocess.run([str(app / "Contents" / "MacOS" / "Monkey Island 2"), "--extra"], capture_output=True,
                       text=True, check=True)
    game_dir = (app / "Contents" / "Resources" / "game").resolve()
    lines = r.stdout.splitlines()
    assert lines[:4] == [f"--path={game_dir}", "--auto-detect", f"--iwad={game_dir}/MONKEY2.000", "--extra"]
    assert Path(lines[4]).resolve() == game_dir


@pytest.mark.skipif(not Path("/usr/bin/codesign").exists(), reason="macOS only")
def test_macos_app_is_signed_once(tmp_path):
    plugin, game = make_plugin(tmp_path), fake_game(tmp_path)
    app_src = fake_app(tmp_path / "rt")
    out = tmp_path / "out"
    run(plugin, game, out, tmp_path, "macos-arm64", settings={"runtime": str(app_src)})
    app = out / "Monkey Island 2.app"
    import subprocess
    assert subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], capture_output=True).returncode == 0
    stamp = (out / "Monkey Island 2.app.ptksign").read_text()
    r = run(plugin, game, out, tmp_path, "macos-arm64", settings={"runtime": str(app_src)})
    assert r["build"]["written"] == 0 and (out / "Monkey Island 2.app.ptksign").read_text() == stamp


def test_windows_folder_and_zip(tmp_path):
    plugin, game = make_plugin(tmp_path), fake_game(tmp_path)
    win = tmp_path / "rt-win"
    win.mkdir()
    (win / "fakevm.exe").write_bytes(b"MZ")
    (win / "SDL2.dll").write_bytes(b"MZ")
    out = tmp_path / "out"
    r = run(plugin, game, out, tmp_path, "windows-x64", settings={"runtime": str(win)})
    folder = out / "Monkey Island 2"
    assert (folder / "runtime" / "SDL2.dll").is_file() and (folder / "game" / "MONKEY2.001").is_file()
    cmd = (folder / "Monkey Island 2.cmd").read_bytes().decode()
    assert cmd.startswith("@echo off\r\n") and 'cd /d "%~dp0game"' in cmd
    assert ('start "" "%~dp0runtime\\fakevm.exe" "--path=%~dp0game" --auto-detect '
            '"--iwad=%~dp0game\\MONKEY2.000" %*') in cmd
    with zipfile.ZipFile(out / "Monkey Island 2.zip") as z:
        assert "Monkey Island 2/runtime/fakevm.exe" in z.namelist()
    assert len(r["outputs"]) == 2
    assert run(plugin, game, out, tmp_path, "windows-x64", settings={"runtime": str(win)})["build"]["written"] == 0


def test_linux_appimage_from_a_remembered_path(tmp_path):
    plugin, game = make_plugin(tmp_path), fake_game(tmp_path)
    image = tmp_path / "fakevm.AppImage"
    image.write_bytes(b"\x7fELF")
    out = tmp_path / "out"
    run(plugin, game, out, tmp_path, "linux-x64", settings={"runtime": str(image), "zip": "false"})
    folder = out / "Monkey Island 2"
    assert os.access(folder / "runtime" / "fakevm.AppImage", os.X_OK)
    sh = (folder / "Monkey Island 2.sh").read_text()
    assert 'BASE="$(cd "$HERE/." && pwd)"' in sh and 'exec "${RUNTIME}/fakevm.AppImage"' in sh
    assert not (out / "Monkey Island 2.zip").exists()
    # The path was remembered for this target: no setting needed the second time.
    run(plugin, game, out, tmp_path, "linux-x64", settings={"zip": "false"})
    r = run(plugin, game, out, tmp_path, "linux-x64", settings={"zip": "false"})
    assert r["build"]["written"] == 0


def test_checks(tmp_path):
    plugin, game = make_plugin(tmp_path), fake_game(tmp_path)
    r = run(plugin, game, None, tmp_path, "macos-arm64", phase="check")
    assert not r["ok"] and "no official build" in r["problems"][0]["message"]
    r = run(plugin, game, None, tmp_path, "macos-arm64", phase="check",
            settings={"runtime": "/opt/homebrew/bin/fakevm"})
    assert "Homebrew" in r["problems"][0]["message"]
    empty = tmp_path / "empty"
    empty.mkdir()
    (empty / "readme.txt").write_text("x")
    r = run(plugin, empty, None, tmp_path, "macos-arm64", phase="check", settings={"runtime": str(fake_app(tmp_path))})
    assert "none of the files FakeVM needs" in r["problems"][0]["message"]
    r = run(plugin, game, None, tmp_path, "macos-arm64", phase="check", settings={"runtime": str(tmp_path / "nope")})
    assert not r["ok"]
    with pytest.raises(EngineError, match="no official build"):
        run(plugin, game, tmp_path / "out", tmp_path, "macos-arm64")


def test_pinned_downloads_are_offered_per_target(tmp_path):
    plugin = make_plugin(tmp_path, RUNTIME_TOML + '\n[download.windows-x64]\nurl = "https://example.org/w.zip"\n'
                                                   'sha256 = "' + "0" * 64 + '"\n')
    game = fake_game(tmp_path)
    assert run(plugin, game, None, tmp_path, "windows-x64", phase="check")["ok"]
    assert not run(plugin, game, None, tmp_path, "linux-x64", phase="check")["ok"]
    spec = rt.RuntimeSpec.load(plugin / "runtime.toml")
    assert spec.download_for(TARGETS["windows-x64"])["url"].endswith("w.zip")
    assert spec.download_for(TARGETS["macos-arm64"]) is None


def ctx_for(tmp: Path, **perms) -> Context:
    return Context({"kind": "export", "folders": {"$CACHE": str(tmp / "cache"), "$TEMP": str(tmp / "temp"),
                                                  "$OUTPUT": str(tmp / "out")},
                    "permissions": {"write": ["$OUTPUT"], **perms}})


def test_unpack_zip_and_tar_keep_links_and_modes_and_refuse_escapes(tmp_path):
    ctx = ctx_for(tmp_path)
    z = tmp_path / "r.zip"
    with zipfile.ZipFile(z, "w") as f:
        info = zipfile.ZipInfo("FakeVM/fakevm.exe")
        info.external_attr = (0o100755) << 16
        f.writestr(info, b"MZ")
        link = zipfile.ZipInfo("FakeVM/current")
        link.external_attr = (0o120777) << 16
        f.writestr(link, "fakevm.exe")
    rt.unpack_archive(ctx, z, tmp_path / "u1")
    assert os.access(tmp_path / "u1" / "FakeVM" / "fakevm.exe", os.X_OK)
    assert (tmp_path / "u1" / "FakeVM" / "current").is_symlink()
    bad = tmp_path / "bad.zip"
    with zipfile.ZipFile(bad, "w") as f:
        f.writestr("../escape.txt", b"x")
    with pytest.raises(rt.RuntimeError_):
        rt.unpack_archive(ctx, bad, tmp_path / "u2")
    t = tmp_path / "r.tar.gz"
    with tarfile.open(t, "w:gz") as f:
        data = b"\x7fELF"
        ti = tarfile.TarInfo("fakevm/fakevm")
        ti.size, ti.mode = len(data), 0o755
        f.addfile(ti, io.BytesIO(data))
    rt.unpack_archive(ctx, t, tmp_path / "u3")
    assert os.access(tmp_path / "u3" / "fakevm" / "fakevm", os.X_OK)
    with pytest.raises(rt.RuntimeError_):
        rt.unpack_archive(ctx, tmp_path / "r.7z", tmp_path / "u4")


def test_executable_lookup_and_homebrew(tmp_path):
    spec = rt.RuntimeSpec.load(make_plugin(tmp_path) / "runtime.toml")
    app = fake_app(tmp_path / "rt")
    exe = rt.executable_in(app, spec, "macos")
    assert exe and exe.name == "fakevm" and rt.runtime_root(exe, "macos") == app
    assert rt.executable_in(tmp_path / "rt", spec, "macos") == exe  # one folder level up
    nested = tmp_path / "w" / "fakevm-1.2.3-win64"
    nested.mkdir(parents=True)
    (nested / "fakevm.exe").write_bytes(b"MZ")
    assert rt.executable_in(tmp_path / "w", spec, "windows") == nested / "fakevm.exe"
    assert rt.is_homebrew(Path("/opt/homebrew/Cellar/fakevm/1/bin/fakevm"))
    assert not rt.is_homebrew(Path("/Applications/FakeVM.app"))
