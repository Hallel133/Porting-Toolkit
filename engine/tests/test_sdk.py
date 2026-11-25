"""Plugin API 2: the template, the lifecycle, check, and API 1 still running."""

from __future__ import annotations

from pathlib import Path

import pytest

from ptk_engine import EngineError
from ptk_engine.plugins import new_plugin, validate_plugin
from ptk_engine.runtime import run_plugin


def run(plugin: Path, game: Path, out: Path | None, tmp: Path, phase: str = "run", settings=None) -> dict:
    (tmp / "project").mkdir(exist_ok=True)
    folders = {"$GAME": str(game), "$PROJECT": str(tmp / "project")}
    if out:
        folders["$OUTPUT"] = str(out)
    return run_plugin({"plugin": str(plugin), "kind": None, "engine": "html5",
                       "target": {"id": "macos-arm64", "os": "macos"}, "phase": phase,
                       "manifest": {"name": "p", "version": "1.0.0", "game": {}},
                       "settings": settings or {}, "folders": folders, "cache_root": str(tmp / "cache")},
                      lambda m, p: None)["result"]


def test_template_export_plugin_builds_incrementally(tmp_path):
    plugin = tmp_path / "my-export"
    new_plugin(str(plugin), "export")
    v = validate_plugin(str(plugin))
    assert v["valid"] and not any("deprecated" in w for w in v["warnings"]), v
    game = tmp_path / "game"
    game.mkdir()
    (game / "index.html").write_text("<h1>hi</h1>")
    out = tmp_path / "out"
    r = run(plugin, game, out, tmp_path)
    assert r["build"]["written"] == 2 and r["build"]["rebuilt"]
    assert (out / "My Export" / "index.html").is_file() and (out / "My Export.zip").is_file()
    r = run(plugin, game, out, tmp_path)
    assert r["build"]["written"] == 0 and r["build"]["unchanged"] == 2
    assert (out / "My Export.zip").is_file()
    r = run(plugin, game, out, tmp_path, settings={"x": "1"})  # other settings: everything again
    assert r["build"]["written"] == 2


def test_check_phase_reports_problems_without_building(tmp_path):
    plugin = tmp_path / "picky"
    new_plugin(str(plugin), "export")
    mod = plugin / "picky" / "__init__.py"
    mod.write_text(mod.read_text().replace(
        'return []\n',
        'return [error("templates are missing", {"setting": "templates", "label": "Choose templates"}),\n'
        '                warning("no icon")]\n', 1))
    game = tmp_path / "game"
    game.mkdir()
    r = run(plugin, game, None, tmp_path, phase="check")
    assert not r["ok"] and [p["severity"] for p in r["problems"]] == ["error", "warning"]
    assert r["problems"][0]["action"]["setting"] == "templates"
    with pytest.raises(EngineError, match="templates are missing") as e:
        run(plugin, game, tmp_path / "out", tmp_path)
    assert e.value.problems[0]["message"] == "templates are missing"
    assert not (tmp_path / "out" / "Picky").exists()


def test_template_import_plugin(tmp_path):
    plugin = tmp_path / "my-import"
    new_plugin(str(plugin), "import")
    toml = plugin / "plugin.toml"
    toml.write_text(toml.read_text().replace('inputs = ["unity"]', 'inputs = ["html5"]'))
    game = tmp_path / "game"
    game.mkdir()
    (game / "a.txt").write_text("x")
    r = run(plugin, game, None, tmp_path)
    assert r["summary"] == "1 files" and (tmp_path / "project" / "recipes" / "my_import.json").is_file()


def test_api_1_plugins_still_run_with_a_warning(tmp_path):
    plugin = tmp_path / "old"
    new_plugin(str(plugin), "export")
    toml = plugin / "plugin.toml"
    toml.write_text(toml.read_text().replace("api = 2", "api = 1"))
    (plugin / "old" / "__init__.py").write_text(
        "class Plugin:\n    def run(self, ctx):\n        return {'summary': 'old style'}\n")
    v = validate_plugin(str(plugin))
    assert v["valid"] and any("deprecated" in w for w in v["warnings"])
    game = tmp_path / "game"
    game.mkdir()
    assert run(plugin, game, tmp_path / "out", tmp_path)["summary"] == "old style"
    assert run(plugin, game, None, tmp_path, phase="check")["api"] == 1
    toml.write_text(toml.read_text().replace("api = 1", "api = 3"))
    assert not validate_plugin(str(plugin))["valid"]
