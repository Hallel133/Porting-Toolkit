import pytest

from ptk_engine import EngineError
from ptk_engine.plugins import list_plugins, new_plugin, validate_plugin


@pytest.mark.parametrize("kind", ["import", "export"])
def test_new_plugin_is_valid(tmp_path, kind):
    created = new_plugin(str(tmp_path / f"my-{kind}"), kind)
    assert created["id"] == f"my-{kind}"
    assert "plugin.toml" in created["files"]
    report = validate_plugin(created["path"], known_targets=["macos-arm64"])
    assert report["valid"], report["errors"]
    assert report["warnings"] == []
    assert report["plugin"]["kind"] == kind
    assert "No network access" in report["permissions"]
    assert report["risk"] in ("low", "medium")


def test_new_plugin_refuses_non_empty_folder(tmp_path):
    (tmp_path / "x").mkdir()
    (tmp_path / "x" / "file").write_text("keep me")
    with pytest.raises(EngineError):
        new_plugin(str(tmp_path / "x"), "import")
    with pytest.raises(EngineError):
        new_plugin(str(tmp_path / "y"), "convert")


def write_plugin(folder, toml, module=True):
    folder.mkdir()
    (folder / "plugin.toml").write_text(toml)
    if module:
        (folder / "my_mod.py").write_text("class Plugin: pass\n")
    (folder / "pyproject.toml").write_text("[project]\nname='x'\n")
    return str(folder)


GOOD = """
schema_version = 1
[plugin]
id = "tex-export"
name = "Texture export"
version = "1.0.0"
kind = "export"
license = "MIT"
entry = "my_mod:Plugin"
[formats]
targets = ["macos-arm64"]
[permissions]
read = ["$PROJECT"]
write = ["$OUTPUT/textures"]
network = ["cdn.example.org"]
"""


def test_permissions_summary_and_risk(tmp_path):
    report = validate_plugin(write_plugin(tmp_path / "p", GOOD))
    assert report["valid"], report["errors"]
    assert report["permissions"] == [
        "Read the porting project folder",
        "Write the output folder you choose (textures)",
        "Network: cdn.example.org",
    ]
    assert report["risk"] == "medium"


def test_broad_permissions_are_high_risk(tmp_path):
    toml = GOOD.replace('read = ["$PROJECT"]', 'read = ["/"]').replace(
        'network = ["cdn.example.org"]', "network = true\nsubprocess = true"
    )
    report = validate_plugin(write_plugin(tmp_path / "p", toml))
    assert report["valid"]
    assert report["risk"] == "high"
    assert any("whole disk" in w for w in report["warnings"])
    assert any("any host" in w for w in report["warnings"])
    assert "Start other programs" in report["permissions"]


@pytest.mark.parametrize(
    "change, message",
    [
        (('id = "tex-export"', 'id = "Tex Export"'), "plugin.id"),
        (('kind = "export"', 'kind = "convert"'), "plugin.kind"),
        (('version = "1.0.0"', 'version = "one"'), "plugin.version"),
        (('targets = ["macos-arm64"]', "targets = []"), "formats.targets"),
        (('read = ["$PROJECT"]', 'read = ["$HOME"]'), "unknown folder"),
        (('read = ["$PROJECT"]', 'read = ["relative/path"]'), "must start with a folder variable"),
        (('read = ["$PROJECT"]', 'read = ["$GAME/../.."]'), "'..'"),
        (('network = ["cdn.example.org"]', 'network = ["not a host"]'), "not a host name"),
        (('entry = "my_mod:Plugin"', 'entry = "missing_mod:Plugin"'), "was not found"),
        (("schema_version = 1", "schema_version = 7"), "schema_version"),
    ],
)
def test_invalid_plugins(tmp_path, change, message):
    report = validate_plugin(write_plugin(tmp_path / "p", GOOD.replace(*change)))
    assert not report["valid"]
    assert any(message in e for e in report["errors"]), report["errors"]


def test_missing_and_broken_files(tmp_path):
    assert "no plugin.toml" in validate_plugin(str(tmp_path))["errors"][0]
    assert not validate_plugin(str(tmp_path / "nope"))["valid"]
    report = validate_plugin(write_plugin(tmp_path / "p", "this is = = not toml"))
    assert not report["valid"]


def test_list_plugins(tmp_path):
    new_plugin(str(tmp_path / "plugins" / "a-import"), "import")
    write_plugin(tmp_path / "plugins" / "broken", "schema_version = 1\n")
    (tmp_path / "plugins" / "not-a-plugin").mkdir()
    found = list_plugins([str(tmp_path / "plugins"), str(tmp_path / "missing")])
    assert [p["id"] for p in found] == ["a-import", "broken"]
    assert found[0]["valid"] and not found[1]["valid"]
    assert found[0]["reviewed"] is False
