"""PS5 import and export (AnyPS5) with a fake relinker: nothing real is run."""

from __future__ import annotations

import json
import os
from pathlib import Path

import pytest

from ptk_engine import EngineError
from ptk_engine.runtime import run_plugin

ROOT = Path(__file__).resolve().parents[2] / "marketplace" / "plugins"
ELF, SELF = b"\x7fELF", b"\x4f\x15\x3d\x1d"
TARGETS = {"linux-x64": {"id": "linux-x64", "os": "linux", "arch": "x64", "name": "Linux (x64)"},
           "windows-x64": {"id": "windows-x64", "os": "windows", "arch": "x64", "name": "Windows (x64)"},
           "macos-arm64": {"id": "macos-arm64", "os": "macos", "arch": "arm64", "name": "macOS (Apple Silicon)"}}

FAKE_RELINKER = """#!/bin/sh
# Fake AnyPS5 relinker: writes the output executable and one converted module.
while [ "$#" -gt 2 ]; do shift; done
out="$2"
dir="$(dirname "$out")"
mkdir -p "$dir/app0/sce_module"
printf 'relinked %s' "$1" > "$out"
chmod +x "$out"
printf 'guest' > "$dir/app0/sce_module/libFoo.prx.guest.prx"
echo "relinked $1"
"""


def title(tmp: Path, kind: bytes = ELF, ps4: bool = False) -> Path:
    g = tmp / "PPSA01234-app0"
    (g / "sce_sys").mkdir(parents=True)
    (g / "sce_module").mkdir()
    (g / "eboot.bin").write_bytes(kind + b"\0" * 60)
    (g / "sce_module" / "libFoo.prx").write_bytes(ELF + b"\0" * 60)
    (g / "data.pak").write_bytes(b"x" * 100)
    if ps4:
        (g / "sce_sys" / "param.sfo").write_bytes(b"\0PSF")
    else:
        (g / "sce_sys" / "param.json").write_text(json.dumps({
            "titleId": "PPSA01234", "contentVersion": "01.000.000",
            "localizedParameters": {"defaultLanguage": "en-US", "en-US": {"titleName": "Astro Test"}}}))
    return g


def request(tmp: Path, plugin: str, game: Path, target: str | None = None, phase: str = "run",
            settings: dict | None = None, out: bool = True) -> dict:
    (tmp / "project").mkdir(exist_ok=True)
    folders = {"$GAME": str(game), "$PROJECT": str(tmp / "project")}
    if out and plugin == "ps5-export":
        folders["$OUTPUT"] = str(tmp / "out")
    return {"plugin": str(ROOT / plugin), "kind": "import" if plugin == "ps5-import" else "export",
            "engine": "ps5", "target": TARGETS.get(target), "phase": phase,
            "manifest": {"name": "astro", "version": "1.0.0", "game": {}}, "settings": settings or {},
            "folders": folders, "cache_root": str(tmp / "cache")}


def fakes(tmp: Path) -> dict:
    relinker = tmp / "relinker"
    relinker.write_text(FAKE_RELINKER)
    relinker.chmod(0o755)
    libs = tmp / "prx-linux" / "libs"
    libs.mkdir(parents=True)
    (libs / "libc.prx").write_bytes(b"lib")
    (libs / "libkernel.prx").write_bytes(b"lib")
    return {"relinker": str(relinker), "libs": str(tmp / "prx-linux")}


def test_import_reads_the_title_without_decrypting(tmp_path):
    r = run_plugin(request(tmp_path, "ps5-import", title(tmp_path)), lambda m, p: None)["result"]
    rec = r["recipe"]
    assert r["title"] == "Astro Test" and rec["title_id"] == "PPSA01234" and rec["encrypted"] == []
    assert rec["modules"] == {"libFoo.prx": "elf"} and "ready for AnyPS5" in r["summary"]
    enc = tmp_path / "enc"
    r = run_plugin(request(enc, "ps5-import", title(enc, SELF)), lambda m, p: None)["result"]
    assert r["recipe"]["encrypted"] == ["eboot.bin"] and any("does not decrypt" in w for w in r["warnings"])


def test_export_relinks_once_and_lays_out_anyps5s_runtime(tmp_path):
    game, s = title(tmp_path), fakes(tmp_path)
    r = run_plugin(request(tmp_path, "ps5-export", game, "linux-x64", settings=s), lambda m, p: None)["result"]
    out = tmp_path / "out" / "Astro Test"
    assert os.access(out / "Astro Test.elf", os.X_OK)
    assert (out / "libs" / "libc.prx").is_file() and (out / "app0" / "eboot.bin").read_bytes().startswith(ELF)
    assert (out / "app0" / "sce_sys" / "param.json").is_file() and (out / "app0" / "data.pak").is_file()
    assert (out / "app0" / "sce_module" / "libFoo.prx.guest.prx").read_text() == "guest"
    assert "GPL-2.0" in (out / "LICENSE-anyps5.txt").read_text()
    assert "AnyPS5" in r["summary"]
    relinked = list((tmp_path / "cache").rglob("relinked/*/.complete"))
    assert len(relinked) == 1
    r = run_plugin(request(tmp_path, "ps5-export", game, "linux-x64", settings=s), lambda m, p: None)["result"]
    assert r["build"]["written"] == 0
    assert len(list((tmp_path / "cache").rglob("relinked/*/.complete"))) == 1  # not relinked again


def test_windows_output_is_an_exe(tmp_path):
    game, s = title(tmp_path), fakes(tmp_path)
    run_plugin(request(tmp_path, "ps5-export", game, "windows-x64", settings=s), lambda m, p: None)
    assert (tmp_path / "out" / "Astro Test" / "Astro Test.exe").is_file()


def check(tmp: Path, game: Path, target: str, settings: dict | None = None) -> dict:
    return run_plugin(request(tmp, "ps5-export", game, target, phase="check", settings=settings, out=False),
                      lambda m, p: None)["result"]


def test_checks_refuse_what_anyps5_cannot_take(tmp_path):
    s = fakes(tmp_path)
    r = check(tmp_path / "a", title(tmp_path / "a", SELF), "linux-x64", s)
    assert not r["ok"] and "does not decrypt" in r["problems"][0]["message"]
    r = check(tmp_path / "b", title(tmp_path / "b", ps4=True), "linux-x64", s)
    assert not r["ok"] and "PS4" in r["problems"][0]["message"]
    r = check(tmp_path / "c", title(tmp_path / "c"), "macos-arm64", {"relinker": s["relinker"]})
    assert not r["ok"] and "no macOS system libraries" in r["problems"][0]["message"]
    r = check(tmp_path / "d", title(tmp_path / "d"), "macos-arm64", s)
    assert r["ok"] and any("Rosetta" in p["message"] for p in r["problems"])
    r = check(tmp_path / "e", title(tmp_path / "e"), "linux-x64", s)
    assert r["ok"] and any("glibc" in p["message"] for p in r["problems"]) is False  # own libs: no glibc note
    with pytest.raises(EngineError):
        run_plugin(request(tmp_path / "f", "ps5-export", title(tmp_path / "f", SELF), "linux-x64", settings=s),
                   lambda m, p: None)
