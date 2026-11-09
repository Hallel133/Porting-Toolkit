import pytest

from ptk_engine import EngineError
from ptk_engine.categories import CATEGORY_IDS, classify
from ptk_engine.scan import scan
from ptk_engine.signatures import ENGINE_IDS, GameIndex, detect

# fixture folder -> (primary engine, variant, platforms)
EXPECTED = {
    "unity-windows": ("unity", "Mono", ["windows"]),
    "unreal": ("unreal", None, ["windows"]),
    "godot": ("godot", None, ["windows"]),
    "rage": ("rage", None, ["windows"]),
    "source": ("source", None, ["windows"]),
    "gamemaker": ("gamemaker", None, ["windows"]),
    "renpy": ("renpy", None, ["windows"]),
    "rpgmaker-mv": ("rpgmaker", "MV", ["windows"]),
    "rpgmaker-vxace": ("rpgmaker", "VX Ace", ["windows"]),
    "macos-app": ("macos", None, ["macos"]),
}


@pytest.mark.parametrize("folder", sorted(EXPECTED))
def test_detects_engine_from_names(games, folder):
    engine, variant, platforms = EXPECTED[folder]
    result = scan(str(games / folder))
    assert result["primary"]["id"] == engine, result["engines"]
    assert result["primary"]["variant"] == variant
    assert result["primary"]["confidence"] >= 0.5
    assert result["primary"]["evidence"]
    assert result["platforms"] == platforms
    # Only one engine (plus at most the macOS format) per fixture.
    others = [e["id"] for e in result["engines"] if e["kind"] == "engine" and e["id"] != engine]
    assert others == []


def test_unknown_folder_has_no_engine(games):
    result = scan(str(games / "unknown"))
    assert result["engines"] == []
    assert result["primary"] is None
    assert result["file_count"] == 2
    assert result["dir_count"] == 1


def test_counts_sizes_and_categories(tmp_path):
    (tmp_path / "Game_Data").mkdir()
    (tmp_path / "Game_Data" / "globalgamemanagers").write_bytes(b"x" * 10)
    (tmp_path / "UnityPlayer.dll").write_bytes(b"MZ" + b"\0" * 98)
    (tmp_path / "music.ogg").write_bytes(b"o" * 50)
    (tmp_path / "._music.ogg").write_bytes(b"junk")
    (tmp_path / ".DS_Store").write_bytes(b"junk")
    (tmp_path / "link.ogg").symlink_to(tmp_path / "music.ogg")
    result = scan(str(tmp_path))
    assert result["file_count"] == 3
    assert result["total_size"] == 160
    assert result["symlinks_skipped"] == 1
    cats = {c["id"]: c for c in result["categories"]}
    assert cats["audio"] == {"id": "audio", "files": 1, "size": 50}
    assert cats["executables"]["size"] == 100
    assert cats["data"]["files"] == 1
    assert result["largest"][0] == {"path": "UnityPlayer.dll", "size": 100, "category": "executables"}
    assert result["primary"]["id"] == "unity"


def test_lone_pak_is_not_enough_for_unreal():
    # Quake and others use .pak too.
    assert detect(GameIndex(["id1/pak0.pak"])) == []
    assert detect(GameIndex(["Game/Content/Paks/a.pak"]))[0]["id"] == "unreal"


def test_max_files_truncates(games):
    result = scan(str(games / "unity-windows"), max_files=2)
    assert result["truncated"] is True
    assert result["file_count"] == 2


def test_errors(tmp_path):
    with pytest.raises(EngineError):
        scan(str(tmp_path / "missing"))


def test_classify():
    assert classify("a/b/tex.DDS") == "textures"
    assert classify("data.win") == "archives"
    assert classify("Demo.app/Contents/MacOS/Demo", "mach-o") == "executables"
    assert classify("Demo.app/Contents/MacOS/Demo") == "other"
    assert classify("x.assets.resS") == "archives"
    assert classify("level12") == "data"
    assert classify("README") == "other"
    assert CATEGORY_IDS[-1] == "other"


def test_engine_ids_are_unique():
    assert len(ENGINE_IDS) == len(set(ENGINE_IDS))


def test_single_exe_with_embedded_godot_pack(tmp_path):
    exe = tmp_path / "MyGame.exe"
    exe.write_bytes(b"MZ" + b"\0" * 100 + b"pack data" + (9).to_bytes(8, "little") + b"GDPC")
    result = scan(str(exe))
    assert result["single_file"] is True
    assert result["file_count"] == 1
    assert result["name"] == "MyGame.exe"
    assert result["primary"]["id"] == "godot"
    assert result["primary"]["variant"] == "embedded .pck"
    assert result["primary"]["evidence"] == ["MyGame.exe (embedded .pck)"]
    assert result["platforms"] == ["windows"]


def test_plain_exe_is_not_godot(tmp_path):
    exe = tmp_path / "tool.exe"
    exe.write_bytes(b"MZ" + b"\0" * 100)
    result = scan(str(exe))
    assert result["engines"] == []
    assert result["platforms"] == ["windows"]


def test_standalone_pck_header(tmp_path):
    (tmp_path / "data.pck").write_bytes(b"GDPC" + b"\0" * 20)
    assert scan(str(tmp_path))["primary"]["id"] == "godot"
