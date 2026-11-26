"""Every runtime plugin in marketplace/plugins/: valid, pinned, consistent
with the engine catalog, and able to build each of its engines' sample
games with a fake runtime (nothing is downloaded or run)."""

from __future__ import annotations

import plistlib
import re
from pathlib import Path
from urllib.parse import urlparse

import pytest
import tomllib

from ptk_engine.plugins import validate_plugin
from ptk_engine.runtime import run_plugin
from ptk_engine.runtimes import RuntimeSpec
from ptk_engine.signatures import CATALOG

ROOT = Path(__file__).resolve().parents[2] / "marketplace" / "plugins"
ALL_PLUGINS = sorted(p for p in ROOT.iterdir() if (p / "plugin.toml").is_file()) if ROOT.is_dir() else []
# Plugins driven by runtime.toml (RuntimeExportPlugin); others have their own tests.
PLUGINS = [p for p in ALL_PLUGINS if (p / "runtime.toml").is_file()]
BY_ID = {e["id"]: e for e in CATALOG}
TARGETS = {
    "macos-arm64": {"id": "macos-arm64", "os": "macos", "arch": "arm64", "name": "macOS (Apple Silicon)"},
    "macos-x64": {"id": "macos-x64", "os": "macos", "arch": "x64", "name": "macOS (Intel)"},
    "windows-x64": {"id": "windows-x64", "os": "windows", "arch": "x64", "name": "Windows (x64)"},
    "linux-x64": {"id": "linux-x64", "os": "linux", "arch": "x64", "name": "Linux (x64)"},
}


def manifest(plugin: Path) -> dict:
    return tomllib.loads((plugin / "plugin.toml").read_text(encoding="utf-8"))


def test_there_are_runtime_plugins():
    assert PLUGINS, "marketplace/plugins is empty"


@pytest.mark.parametrize("plugin", PLUGINS, ids=lambda p: p.name)
def test_plugin_is_valid_and_matches_the_catalog(plugin):
    v = validate_plugin(str(plugin))
    assert v["valid"], v
    m = manifest(plugin)
    pid = m["plugin"]["id"]
    assert pid == plugin.name and m["plugin"]["api"] == 2
    inputs = m["formats"]["inputs"]
    claimed = sorted(e["id"] for e in CATALOG if e["plugin"] == pid)
    assert sorted(inputs) == claimed, f"catalog engines for {pid}: {claimed}"
    spec = RuntimeSpec.load(plugin / "runtime.toml")
    assert set(m["formats"]["targets"]) <= set(TARGETS)
    hosts = m["permissions"]["network"]
    for key, pin in spec.downloads.items():
        assert key in TARGETS or key in {"macos", "windows", "linux"}, key
        url = urlparse(pin["url"])
        assert url.scheme == "https" and url.hostname in hosts, (key, pin["url"])
        assert re.fullmatch(r"[0-9a-f]{64}", pin["sha256"]), key
    for target in m["formats"]["targets"]:
        assert TARGETS[target]["os"] in spec.executable, target


def fake_runtime(tmp: Path, spec: RuntimeSpec, os_: str) -> Path:
    want = spec.executable[os_].replace("*", "Fake")
    base = tmp / f"fake-{os_}"
    if want.endswith(".app"):
        app = base / want
        (app / "Contents" / "MacOS").mkdir(parents=True)
        (app / "Contents" / "Info.plist").write_bytes(plistlib.dumps({"CFBundleExecutable": "runtime"}))
        exe = app / "Contents" / "MacOS" / "runtime"
        exe.write_text("#!/bin/sh\n")
        exe.chmod(0o755)
        return app
    exe = base / want
    exe.parent.mkdir(parents=True, exist_ok=True)
    exe.write_bytes(b"fake")
    exe.chmod(0o755)
    if spec.id == "fna":  # an FNA kit has more than its launcher
        for name in ("FNA.dll", "monoconfig", "monomachineconfig", "mscorlib.dll", "Microsoft.Xna.Framework.dll",
                     "osx/libFNA3D.0.dylib", "lib64/libFNA3D.so.0"):
            (base / name).parent.mkdir(parents=True, exist_ok=True)
            (base / name).write_bytes(b"kit")
    return base


def sample_game(tmp: Path, engine: dict) -> Path:
    game = tmp / f"game-{engine['id']}"
    for s in engine["samples"]:
        rel, _, head = s.partition("=")
        p = game / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(head.encode("latin-1") + b"\0" * 64)
    return game


CASES = [(p, e, t) for p in PLUGINS for e in manifest(p)["formats"]["inputs"] for t in manifest(p)["formats"]["targets"]]


@pytest.mark.parametrize("plugin,engine,target", CASES, ids=lambda x: x.name if isinstance(x, Path) else x)
def test_builds_each_engine_sample_with_a_fake_runtime(tmp_path, plugin, engine, target):
    spec = RuntimeSpec.load(plugin / "runtime.toml")
    t = TARGETS[target]
    runtime = fake_runtime(tmp_path, spec, t["os"])
    game = sample_game(tmp_path, BY_ID[engine])
    (tmp_path / "project").mkdir()
    request = {"plugin": str(plugin), "kind": "export", "engine": engine, "target": t,
               "manifest": {"name": "p", "version": "1.0.0", "game": {"title": "Sample Game"}},
               "settings": {"runtime": str(runtime), "sign": "false", "zip": "false"},
               "folders": {"$GAME": str(game), "$PROJECT": str(tmp_path / "project"), "$OUTPUT": str(tmp_path / "out")},
               "cache_root": str(tmp_path / "cache")}
    check = run_plugin({**request, "phase": "check"}, lambda m, p: None)["result"]
    assert check["ok"], check["problems"]
    r = run_plugin(request, lambda m, p: None)["result"]
    built = Path(r["outputs"][0])
    assert built.exists()
    lic = next(built.rglob(f"LICENSE-{spec.id}.txt"))
    assert spec.source in lic.read_text()
    assert run_plugin(request, lambda m, p: None)["result"]["build"]["written"] == 0


def build_with_fake(tmp: Path, plugin_id: str, engine: str, target: str, files: dict[str, bytes]) -> Path:
    plugin = ROOT / plugin_id
    spec = RuntimeSpec.load(plugin / "runtime.toml")
    t = TARGETS[target]
    runtime = fake_runtime(tmp, spec, t["os"])
    game = tmp / "game"
    for rel, data in files.items():
        (game / rel).parent.mkdir(parents=True, exist_ok=True)
        (game / rel).write_bytes(data)
    (tmp / "project").mkdir()
    r = run_plugin({"plugin": str(plugin), "kind": "export", "engine": engine, "target": t,
                    "manifest": {"name": "p", "version": "1.0.0", "game": {"title": "Test"}},
                    "settings": {"runtime": str(runtime), "sign": "false", "zip": "false"},
                    "folders": {"$GAME": str(game), "$PROJECT": str(tmp / "project"), "$OUTPUT": str(tmp / "out")},
                    "cache_root": str(tmp / "cache")}, lambda m, p: None)["result"]
    return Path(r["outputs"][0])


def test_openmw_loads_the_content_the_game_has(tmp_path):
    out = build_with_fake(tmp_path, "morrowind-export", "openmw", "linux-x64",
                          {"Data Files/Morrowind.esm": b"x", "Data Files/Tribunal.esm": b"x",
                           "Data Files/Morrowind.bsa": b"x", "Data Files/Tribunal.bsa": b"x"})
    sh = (out / "Test.sh").read_text()
    assert ('"--data" "${GAME}/Data Files" "--content" "Morrowind.esm" "--content" "Tribunal.esm" '
            '"--fallback-archive" "Morrowind.bsa" "--fallback-archive" "Tribunal.bsa"') in sh
    assert "Bloodmoon" not in sh


def test_exult_writes_its_config_for_the_right_game(tmp_path):
    out = build_with_fake(tmp_path, "exult-export", "exult", "linux-x64",
                          {"STATIC/U7CHUNKS": b"x", "STATIC/SISPEECH.SPC": b"x"})
    assert "<serpentisle>" in (out / "game" / "exult.cfg").read_text()
    assert '"-c" "${GAME}/exult.cfg" "--si"' in (out / "Test.sh").read_text()


def test_ikemen_merges_mugen_content_into_its_folder(tmp_path):
    out = build_with_fake(tmp_path, "mugen-export", "mugen", "windows-x64",
                          {"mugen.exe": b"MZ", "data/mugen.cfg": b"cfg", "chars/kfm/kfm.def": b"def"})
    assert (out / "runtime" / "Ikemen_GO.exe").is_file()
    assert (out / "runtime" / "chars" / "kfm" / "kfm.def").is_file() and not (out / "game").exists()
    assert not (out / "runtime" / "mugen.exe").exists()
    assert 'cd /d "%~dp0runtime"' in (out / "Test.cmd").read_bytes().decode()


def test_ikemen_on_macos_keeps_the_folder_around_its_app(tmp_path):
    out = build_with_fake(tmp_path, "mugen-export", "mugen", "macos-arm64", {"data/mugen.cfg": b"cfg"})
    res = out / "Contents" / "Resources" / "runtime"
    assert (res / "Fake.app" / "Contents" / "Info.plist").is_file() and (res / "data" / "mugen.cfg").is_file()
    assert 'exec "${RUNTIME}/Fake.app/Contents/MacOS/runtime"' in (out / "Contents" / "MacOS" / "Test").read_text()


def test_marketplace_builder_writes_a_valid_reproducible_index(tmp_path):
    import hashlib
    import importlib.util
    import json
    import zipfile as zf

    spec = importlib.util.spec_from_file_location("build_marketplace", ROOT.parents[1] / "tools" / "build_marketplace.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    base = (tmp_path / "a").as_uri()
    index = mod.build(base, tmp_path / "a")
    assert json.loads((tmp_path / "a" / "index.json").read_text()) == index
    assert {e["id"] for e in index["entries"]} == {p.name for p in ALL_PLUGINS}
    for e in index["entries"]:
        z = tmp_path / "a" / f"{e['id']}-{e['version']}.zip"
        assert e["download"] == f"{base}/{z.name}"
        assert hashlib.sha256(z.read_bytes()).hexdigest() == e["sha256"]
        names = zf.ZipFile(z).namelist()
        assert f"{e['id']}/plugin.toml" in names and not any("__pycache__" in n for n in names)
    again = mod.build(base, tmp_path / "b")
    assert [e["sha256"] for e in again["entries"]] == [e["sha256"] for e in index["entries"]]


def test_cocos2dx_runs_the_lua_folder_and_refuses_encrypted_scripts(tmp_path):
    out = build_with_fake(tmp_path / "a", "cocos2dx-export", "cocos2dx-lua", "windows-x64",
                          {"Game.exe": b"MZ", "src/main.luac": b"\x1bLJ\x02", "src/cocos/init.luac": b"\x1bLJ\x02",
                           "res/a.png": b"png"})
    cmd = (out / "Test.cmd").read_bytes().decode()
    assert '-workdir "%~dp0game" %*' in cmd and "-entry" not in cmd and not (out / "game" / "Game.exe").exists()
    plugin = ROOT / "cocos2dx-export"
    spec = RuntimeSpec.load(plugin / "runtime.toml")
    for files, why in (({"src/main.luac": b"XXTEA\x00\x01"}, "encrypted"), ({"main.jsc": b"\x00", "main.js": b"x"}, ".jsc")):
        tmp = tmp_path / why.strip(".")
        game = tmp / "game"
        for rel, data in files.items():
            (game / rel).parent.mkdir(parents=True, exist_ok=True)
            (game / rel).write_bytes(data)
        (tmp / "project").mkdir()
        r = run_plugin({"plugin": str(plugin), "kind": "export", "engine": "cocos2dx-lua", "phase": "check",
                        "target": TARGETS["windows-x64"], "manifest": {"name": "p", "version": "1", "game": {}},
                        "settings": {"runtime": str(fake_runtime(tmp, spec, "windows"))},
                        "folders": {"$GAME": str(game), "$PROJECT": str(tmp / "project")},
                        "cache_root": str(tmp / "cache")}, lambda m, p: None)["result"]
        assert not r["ok"] and why in r["problems"][0]["message"]


def test_cocos2dx_js_game_in_a_mac_app(tmp_path):
    out = build_with_fake(tmp_path, "cocos2dx-export", "cocos2dx-js", "linux-x64",
                          {"Game.app/Contents/Resources/main.js": b"x", "Game.app/Contents/Resources/project.json": b"{}",
                           "Game.app/Contents/Resources/script/jsb.js": b"x"})
    sh = (out / "Test.sh").read_text()
    assert '"-workdir" "${GAME}/Game.app/Contents/Resources" "-entry" "main.js"' in sh


def test_dosbox_starts_the_gold_box_game(tmp_path):
    out = build_with_fake(tmp_path, "dos-export", "goldbox", "linux-x64",
                          {"START.EXE": b"MZ", "GEO1.DAX": b"x", "ECL1.DAX": b"x"})
    assert 'exec "${RUNTIME}/dosbox" "${GAME}/START.EXE" "$@"' in (out / "Test.sh").read_text()


def test_a_portable_exe_runtime_is_copied_alone(tmp_path):
    out = build_with_fake(tmp_path, "scratch-export", "scratch", "windows-x64", {"project.sb3": b"PK"})
    assert [p.name for p in (out / "runtime").iterdir()] == ["TurboWarpFake.exe"]


def test_defold_reads_the_engine_version_from_the_game(tmp_path):
    import importlib.util
    spec = importlib.util.spec_from_file_location("defold_export", ROOT / "defold-export" / "defold_export" / "__init__.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    sha1 = "ee283d4dad9fa451dd6399d83602e1dc25133681"
    game = tmp_path / "game"
    game.mkdir()
    (game / "Quest.exe").write_bytes(b"MZ" + b"\0" * 100 + b"1.14.1\0\0\0" + sha1.encode() + b"\0" + b"\0" * 50)
    (game / "game.projectc").write_text("[project]\n")
    (game / "game.arcd").write_bytes(b"x")
    assert mod.read_engine_version(mod.engine_candidates(game, ["Quest.exe", "game.projectc"])) == ("1.14.1", sha1)
    (tmp_path / "project").mkdir()
    req = {"plugin": str(ROOT / "defold-export"), "kind": "export", "engine": "defold", "phase": "check",
           "target": TARGETS["macos-arm64"], "manifest": {"name": "p", "version": "1", "game": {}}, "settings": {},
           "folders": {"$GAME": str(game), "$PROJECT": str(tmp_path / "project")}, "cache_root": str(tmp_path / "cache")}
    assert run_plugin(req, lambda m, p: None)["result"]["ok"]
    (game / "Quest.exe").write_bytes(b"MZ no version here")
    r = run_plugin(req, lambda m, p: None)["result"]
    assert not r["ok"] and "defold_sha1" in r["problems"][0]["message"]
    r = run_plugin({**req, "settings": {"defold_sha1": sha1}}, lambda m, p: None)["result"]
    assert r["ok"]


def test_fna_merges_the_kit_and_names_the_launcher_after_the_game(tmp_path):
    out = build_with_fake(tmp_path, "fna-export", "xna", "linux-x64",
                          {"Bastion.exe": b"MZ", "Microsoft.Xna.Framework.dll": b"xna", "Content/a.xnb": b"x",
                           "intro.wmv": b"v"})
    game = out / "game"
    assert (game / "Bastion.bin.x86_64").is_file() and (game / "Bastion.exe").is_file()
    assert (game / "Microsoft.Xna.Framework.dll").read_bytes() == b"kit"
    assert (game / "monoconfig").is_file() and (game / "lib64" / "libFNA3D.so.0").is_file()
    assert not (game / "kick.bin.x86_64").exists()
    sh = (out / "Test.sh").read_text()
    assert 'exec "$GAME/Bastion.bin.x86_64" "$@"' in sh and 'cd "$GAME"' in sh


def test_fna_checks_the_kit_and_the_exe(tmp_path):
    plugin = ROOT / "fna-export"
    spec = RuntimeSpec.load(plugin / "runtime.toml")
    kit = fake_runtime(tmp_path, spec, "macos")
    (kit / "monoconfig").unlink()
    game = tmp_path / "game"
    (game / "Content").mkdir(parents=True)
    (game / "Content" / "a.xnb").write_bytes(b"x")
    (game / "Game.exe").write_bytes(b"MZ")
    (game / "Editor.exe").write_bytes(b"MZ")
    (tmp_path / "project").mkdir()
    req = {"plugin": str(plugin), "kind": "export", "engine": "xna", "phase": "check", "target": TARGETS["macos-arm64"],
           "manifest": {"name": "p", "version": "1", "game": {}}, "settings": {"runtime": str(kit)},
           "folders": {"$GAME": str(game), "$PROJECT": str(tmp_path / "project")}, "cache_root": str(tmp_path / "cache")}
    r = run_plugin(req, lambda m, p: None)["result"]
    assert "--set exe=" in r["problems"][0]["message"]
    r = run_plugin({**req, "settings": {"runtime": str(kit), "exe": "Game.exe"}}, lambda m, p: None)["result"]
    assert "missing monoconfig" in r["problems"][0]["message"]
    (kit / "monoconfig").write_text("x")
    r = run_plugin({**req, "settings": {"runtime": str(kit), "exe": "Game.exe"}}, lambda m, p: None)["result"]
    assert r["ok"] and any("Rosetta" in p["message"] for p in r["problems"])
    r = run_plugin({**req, "settings": {}}, lambda m, p: None)["result"]
    assert "KIT.md" in r["problems"][0]["message"]


@pytest.mark.parametrize("plugin", [p for p in ALL_PLUGINS if p not in PLUGINS], ids=lambda p: p.name)
def test_other_marketplace_plugins_are_valid(plugin):
    v = validate_plugin(str(plugin))
    assert v["valid"], v
    m = manifest(plugin)
    if m["plugin"]["kind"] == "export":
        claimed = sorted(e["id"] for e in CATALOG if e["plugin"] == m["plugin"]["id"])
        assert sorted(m["formats"]["inputs"]) == claimed
