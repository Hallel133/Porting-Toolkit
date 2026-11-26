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
PLUGINS = sorted(p for p in ROOT.iterdir() if (p / "plugin.toml").is_file()) if ROOT.is_dir() else []
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
