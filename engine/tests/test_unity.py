"""Unity: what a build is, and the experimental Mono retarget."""

from __future__ import annotations

import json
import os
import plistlib
import zipfile
from pathlib import Path

import pytest

from ptk_engine.formats import unity
from ptk_engine.runtime import BUILTIN_DIR, find_plugins, run_plugin
from ptk_engine.signatures import GameIndex, detect

WINDOWS = {"id": "windows-x64", "name": "Windows x64", "os": "windows", "arch": "x86_64"}
LINUX = {"id": "linux-x64", "name": "Linux x64", "os": "linux", "arch": "x86_64"}
MAC = {"id": "macos-arm64", "name": "macOS (Apple Silicon)", "os": "macos", "arch": "arm64"}
VERSION = "2022.3.62f2"


def write(root: Path, files: dict[str, bytes], mode: int | None = None) -> None:
    for rel, data in files.items():
        (root / rel).parent.mkdir(parents=True, exist_ok=True)
        (root / rel).write_bytes(data)
        if mode:
            (root / rel).chmod(mode)


def data_files(prefix: str) -> dict[str, bytes]:
    return {
        f"{prefix}/globalgamemanagers": b"\0" * 20 + VERSION.encode() + b"\0" + b"\0" * 100,
        f"{prefix}/app.info": b"Moon Studio\nMoon Garden",
        f"{prefix}/level0": b"scene",
        f"{prefix}/Managed/Assembly-CSharp.dll": b"MZ-game-code",
        f"{prefix}/Managed/UnityEngine.CoreModule.dll": b"MZ-engine-from-source",
        f"{prefix}/Resources/unity default resources": b"source-resources",
    }


def windows_game(tmp: Path) -> Path:
    g = tmp / "MoonGarden"
    write(g, {"MoonGarden.exe": b"MZ", "UnityPlayer.dll": b"MZ", **data_files("MoonGarden_Data"),
              "MoonGarden_Data/Plugins/x86_64/AkSoundEngine.dll": b"MZ",
              "MonoBleedingEdge/EmbedRuntime/mono-2.0-bdwgc.dll": b"MZ"})
    return g


def mac_game(tmp: Path, il2cpp: bool = False) -> Path:
    app = tmp / "Moon Garden.app"
    files = {"Contents/MacOS/Moon Garden": b"\xcf\xfa", "Contents/Frameworks/UnityPlayer.dylib": b"\xcf\xfa",
             **data_files("Contents/Resources/Data"),
             "Contents/PlugIns/GameCenterPluginMac.bundle/Contents/Info.plist": b"x",
             "Contents/PlugIns/AkUnitySoundEngine.bundle/Contents/Info.plist": b"x",
             "Contents/PlugIns/lib_burst_generated.bundle/Contents/Info.plist": b"x",
             "Contents/Resources/altoArcade.entitlements": b"x"}
    if il2cpp:
        files["Contents/Frameworks/GameAssembly.dylib"] = b"\xcf\xfa"
    write(app, files)
    return app


def unity_install(tmp: Path) -> Path:
    """Unity Hub's layout for one version, with Windows, Linux and Mac players."""
    root = tmp / "Hub" / "Editor" / VERSION
    win = "PlaybackEngines/WindowsStandaloneSupport/Variations/win64_player_nondevelopment_mono"
    lin = "PlaybackEngines/LinuxStandaloneSupport/Variations/linux64_player_nondevelopment_mono"
    mac = "Unity.app/Contents/PlaybackEngines/MacStandaloneSupport"
    write(root, {
        f"{win}/WindowsPlayer.exe": b"MZ-player", f"{win}/UnityPlayer.dll": b"MZ-unityplayer",
        f"{win}/UnityPlayer_Win64_player_mono_x64.pdb": b"pdb",
        f"{win}/UnityCrashHandler64.exe": b"MZ", f"{win}/MonoBleedingEdge/EmbedRuntime/mono-2.0-bdwgc.dll": b"MZ-mono",
        f"{win}/Data/Managed/UnityEngine.CoreModule.dll": b"MZ-engine-windows",
        f"{win}/Data/Resources/unity default resources": b"windows-resources",
        "PlaybackEngines/WindowsStandaloneSupport/Variations/win64_player_development_mono/WindowsPlayer.exe": b"dev",
        f"{lin}/UnityPlayer.so": b"\x7fELF", f"{lin}/Data/Managed/UnityEngine.CoreModule.dll": b"MZ-engine-linux",
        f"{lin}/Data/MonoBleedingEdge/x86_64/libmonobdwgc-2.0.so": b"\x7fELF",
        f"{lin}/Data/Resources/unity default resources": b"linux-resources",
        f"{mac}/Variations/macos_x64arm64_player_nondevelopment_mono/UnityPlayer.app/Contents/MacOS/UnityPlayer": b"\xcf\xfa",
        f"{mac}/Variations/macos_x64arm64_player_nondevelopment_mono/UnityPlayer.app/Contents/Frameworks/UnityPlayer.dylib": b"\xcf\xfa",
        f"{mac}/Variations/macos_x64arm64_player_nondevelopment_mono/UnityPlayer.app/Contents/Resources/unity default resources": b"mac-resources",
        f"{mac}/Variations/mono/Managed/UnityEngine.CoreModule.dll": b"MZ-engine-mac",
        f"{mac}/MonoBleedingEdge/etc/mono/config": b"<config/>",
    })
    write(root, {f"{lin}/LinuxPlayer": b"\x7fELF-player"}, mode=0o755)
    return root


def run(tmp: Path, plugin: str, game: Path, kind: str, target=None, settings=None, output=None):
    (tmp / "project").mkdir(exist_ok=True)
    return run_plugin({
        "plugin": str(BUILTIN_DIR / plugin), "kind": kind, "engine": "unity", "target": target,
        "manifest": {"name": "p", "version": "1.0.0", "game": {"title": "Moon Garden"}},
        "settings": {"unity_lookup": "false", **(settings or {})},
        "folders": {"$GAME": str(game), "$PROJECT": str(tmp / "project"), "$OUTPUT": str(output) if output else None},
        "cache_root": str(tmp / "cache"),
    }, lambda m, p: None)["result"]


# -- detection -------------------------------------------------------------------------




def test_detect_windows_and_mac_builds(tmp_path):
    g = unity.detect(windows_game(tmp_path))
    assert (g.platform, g.data, g.name, g.version, g.backend) == ("windows", "MoonGarden_Data", "MoonGarden", VERSION, "mono")
    assert (g.company, g.product, g.plugins) == ("Moon Studio", "Moon Garden", ["AkSoundEngine"])
    m = unity.detect(mac_game(tmp_path))
    assert (m.platform, m.name, m.backend, m.app_store) == ("macos", "Moon Garden", "mono", True)
    assert "lib_burst_generated" not in m.plugins and any("Burst" in n for n in m.notes)
    assert m.apple_services == ["GameCenterPluginMac"]
    il2 = unity.detect(mac_game(tmp_path / "x", il2cpp=True))
    assert il2.backend == "il2cpp"


def test_scan_variant_on_mac():
    index = GameIndex(["Moon.app/Contents/Frameworks/UnityPlayer.dylib", "Moon.app/Contents/Frameworks/GameAssembly.dylib",
                       "Moon.app/Contents/Resources/Data/globalgamemanagers"], ["Moon.app"])
    unity_hit = next(e for e in detect(index) if e["id"] == "unity")
    assert unity_hit["variant"] == "IL2CPP"


def test_import_reports_what_is_possible(tmp_path):
    r = run(tmp_path, "unity-import", mac_game(tmp_path), "import")
    assert r["title"] == "Moon Garden" and r["engine_version"] == VERSION
    assert "can move" in r["summary"]
    assert any("Apple Arcade" in w for w in r["warnings"]) and any("AkUnitySoundEngine" in w for w in r["warnings"])
    r = run(tmp_path, "unity-import", mac_game(tmp_path / "x", il2cpp=True), "import")
    assert "cannot move" in r["summary"]


# -- the player from a Unity install ---------------------------------------------------------


def test_player_lookup(tmp_path):
    root = unity_install(tmp_path)
    assert unity.player_variation(unity.support_dir(root, "windows"), "windows").name == "win64_player_nondevelopment_mono"
    assert unity.player_variation(unity.support_dir(root, "macos"), "macos").name.startswith("macos_x64arm64")
    assert unity.support_dir(tmp_path, "linux") is None


def test_mac_build_to_windows(tmp_path):
    root = unity_install(tmp_path)
    r = run(tmp_path, "unity-export", mac_game(tmp_path), "export", WINDOWS, {"unity": str(root)}, tmp_path / "win")
    folder = tmp_path / "win" / "Moon Garden"
    assert (folder / "Moon Garden.exe").read_bytes() == b"MZ-player"
    assert (folder / "UnityPlayer.dll").is_file() and (folder / "MonoBleedingEdge/EmbedRuntime/mono-2.0-bdwgc.dll").is_file()
    assert not list(folder.glob("*.pdb"))
    data = folder / "Moon Garden_Data"
    assert (data / "Managed/Assembly-CSharp.dll").read_bytes() == b"MZ-game-code", "the game's code is kept"
    assert (data / "Managed/UnityEngine.CoreModule.dll").read_bytes() == b"MZ-engine-windows"
    assert (data / "Resources/unity default resources").read_bytes() == b"windows-resources"
    assert any("shaders" in w for w in r["warnings"]) and any("Apple Arcade" in w for w in r["warnings"])
    assert (tmp_path / "win" / "Moon Garden-windows-x64.zip").is_file()


def test_windows_build_to_linux(tmp_path):
    root = unity_install(tmp_path)
    run(tmp_path, "unity-export", windows_game(tmp_path), "export", LINUX, {"unity": str(root)}, tmp_path / "linux")
    folder = tmp_path / "linux" / "Moon Garden"
    assert os.access(folder / "Moon Garden.x86_64", os.X_OK) and (folder / "UnityPlayer.so").is_file()
    data = folder / "Moon Garden_Data"
    assert (data / "MonoBleedingEdge/x86_64/libmonobdwgc-2.0.so").is_file()
    assert not (data / "Plugins").exists(), "Windows-only native plugins stay behind"
    with zipfile.ZipFile(tmp_path / "linux" / "Moon Garden-linux-x64.zip") as z:
        modes = {i.filename: (i.external_attr >> 16) & 0o777 for i in z.infolist()}
    assert modes["Moon Garden/Moon Garden.x86_64"] == 0o755


def test_windows_build_to_mac(tmp_path):
    root = unity_install(tmp_path)
    run(tmp_path, "unity-export", windows_game(tmp_path), "export", MAC, {"unity": str(root), "sign": "false"},
        tmp_path / "mac")
    app = tmp_path / "mac" / "Moon Garden.app"
    plist = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    assert plist["CFBundleExecutable"] == "Moon Garden" and (app / "Contents/MacOS/Moon Garden").is_file()
    assert (app / "Contents/MonoBleedingEdge/etc/mono/config").is_file()
    assert (app / "Contents/Resources/Data/Managed/UnityEngine.CoreModule.dll").read_bytes() == b"MZ-engine-mac"
    assert (app / "Contents/Resources/Data/Managed/Assembly-CSharp.dll").read_bytes() == b"MZ-game-code"


def test_missing_unity_and_il2cpp_are_explained(tmp_path):
    from ptk_engine import EngineError
    with pytest.raises(EngineError, match="Windows Build Support \\(Mono\\)"):
        run(tmp_path, "unity-export", mac_game(tmp_path), "export", WINDOWS,
            {"unity": str(tmp_path / "nowhere"), "unity_download": "false"}, tmp_path / "out")
    with pytest.raises(EngineError, match="IL2CPP"):
        run(tmp_path, "unity-export", mac_game(tmp_path / "x", il2cpp=True), "export", WINDOWS, {}, tmp_path / "o2")


def test_unity_plugins_are_found():
    ids = lambda **kw: [r["plugin"]["id"] for r in find_plugins(dirs=[], **kw)]  # noqa: E731
    assert ids(kind="import", engine="unity", target=None) == ["unity-import"]
    assert ids(kind="export", engine="unity", target="linux-x64") == ["unity-export"]


# -- the player downloaded from Unity -------------------------------------------------------


def fake_module(tmp: Path) -> Path:
    """Unity's release service answer, with a Windows Mono module .pkg."""
    import base64
    import hashlib

    from ptk_engine.formats import xar
    v = "./Variations/win64_player_nondevelopment_mono"
    payload = xar.cpio_odc([
        (".", 0o40755, b""), ("./Variations", 0o40755, b""), (v, 0o40755, b""),
        (f"{v}/WindowsPlayer.exe", 0o100755, b"MZ-player"), (f"{v}/UnityPlayer.dll", 0o100644, b"MZ-unityplayer"),
        (f"{v}/Data/Managed/UnityEngine.CoreModule.dll", 0o100644, b"MZ-engine-windows"),
        ("./Variations/win64_player_development_mono/WindowsPlayer.exe", 0o100755, b"dev"),
        ("./Bee/Bee.Toolchain.Windows.dll", 0o100644, b"editor only"),
    ])
    pkg = tmp / "UnitySetup-Windows-Mono-Support-for-Editor.pkg"
    pkg.write_bytes(xar.build_pkg(payload))
    md5 = base64.b64encode(hashlib.md5(pkg.read_bytes()).digest()).decode()
    module = {"id": "windows-mono", "url": pkg.as_uri(), "integrity": f"md5-{md5}",
              "downloadSize": {"value": pkg.stat().st_size, "unit": "BYTE"}}
    release = {"results": [{"version": VERSION, "downloads": [
        {"platform": "MAC_OS", "architecture": "ARM64", "modules": [module]}]}]}
    service = tmp / "releases.json"
    service.write_text(json.dumps(release))
    return service


def test_player_is_downloaded_once_after_accepting_unitys_terms(tmp_path):
    from ptk_engine import EngineError
    service = fake_module(tmp_path)
    settings = {"unity": str(tmp_path / "nowhere"), "unity_service": service.as_uri()}
    with pytest.raises(EngineError, match="accept_unity_terms=true"):
        run(tmp_path, "unity-export", mac_game(tmp_path), "export", WINDOWS, settings, tmp_path / "o1")
    r = run(tmp_path, "unity-export", mac_game(tmp_path), "export", WINDOWS,
            {**settings, "accept_unity_terms": "true"}, tmp_path / "o2")
    assert "downloaded from Unity" in r["notes"][0]
    exe = tmp_path / "o2" / "Moon Garden" / "Moon Garden.exe"
    assert exe.read_bytes() == b"MZ-player"
    players = tmp_path / "cache" / "unity-export" / "players" / VERSION / "PlaybackEngines" / "WindowsStandaloneSupport"
    assert not (players / "Bee").exists() and not (players / "Variations" / "win64_player_development_mono").exists()
    # Remembered: no second acceptance, no second download.
    (tmp_path / "UnitySetup-Windows-Mono-Support-for-Editor.pkg").unlink()
    run(tmp_path, "unity-export", mac_game(tmp_path), "export", WINDOWS, settings, tmp_path / "o3")
    assert (tmp_path / "o3" / "Moon Garden" / "Moon Garden.exe").is_file()


def test_a_module_with_the_wrong_checksum_is_refused(tmp_path):
    from ptk_engine import EngineError
    service = fake_module(tmp_path)
    data = json.loads(service.read_text())
    data["results"][0]["downloads"][0]["modules"][0]["integrity"] = "md5-AAAAAAAAAAAAAAAAAAAAAA=="
    service.write_text(json.dumps(data))
    with pytest.raises(EngineError, match="checksum"):
        run(tmp_path, "unity-export", mac_game(tmp_path), "export", WINDOWS,
            {"unity": str(tmp_path / "nowhere"), "unity_service": service.as_uri(), "accept_unity_terms": "true"},
            tmp_path / "o")


def test_pkg_reader_keeps_links_inside(tmp_path):
    from ptk_engine.formats import xar
    pkg = tmp_path / "evil.pkg"
    pkg.write_bytes(xar.build_pkg(xar.cpio_odc([("./a/link", 0o120777, b"../../../etc/passwd"),
                                                ("./a/ok", 0o100644, b"x")])))
    with pytest.raises(xar.XarError, match="outside"):
        xar.extract_payload(pkg, tmp_path / "out", lambda p: True)
    pkg.write_bytes(xar.build_pkg(xar.cpio_odc([("../escape", 0o100644, b"x"), ("./a/ok", 0o100644, b"x")])))
    assert xar.extract_payload(pkg, tmp_path / "out2", lambda p: True) == 1
    assert not (tmp_path / "escape").exists()


# -- shaders ---------------------------------------------------------------------------------


def shader_object(platforms: list[int], blob: bytes = b"lz4" * 10) -> bytes:
    """A Shader's tail as Unity 2022 writes it, after some parsed-form bytes."""
    import struct
    n = len(platforms)
    out = b"parsed form \x05\x00\x00\x00" * 3 + bytes(-len(b"parsed form \x05\x00\x00\x00" * 3) % 4)
    out += struct.pack(f"<I{n}I", n, *platforms)
    for _ in range(3):  # offsets, compressedLengths, decompressedLengths: vector<vector<UInt32>>
        out += struct.pack("<I", n) + b"".join(struct.pack("<II", 1, 7) for _ in range(n))
    out += struct.pack("<I", len(blob)) + blob + bytes(-len(blob) % 4)
    return out + struct.pack("<II?", 0, 0, True)  # dependencies, textures, baked


def serialized_file(objects: list[tuple[int, bytes]]) -> bytes:
    """A Unity serialized file (format 22, no type trees)."""
    import struct
    classes = sorted({c for c, _ in objects})
    meta = b"2022.3.62f2\0" + struct.pack("<i?i", 2, False, len(classes))
    for c in classes:
        meta += struct.pack("<i?h", c, False, -1) + bytes(16)
    meta += struct.pack("<i", len(objects))
    data, table = b"", b""
    for i, (cid, body) in enumerate(objects):
        pad = bytes(-(len(meta) + len(table)) % 4)
        table += pad + struct.pack("<qqIi", i + 1, len(data), len(body), classes.index(cid))
        data += body + bytes(-len(body) % 8)
    meta += table + struct.pack("<i", 0) * 3
    data_off = (48 + len(meta) + 15) & ~15
    head = struct.pack(">IIII", 0, 0, 22, 0) + bytes(4) + struct.pack(">IqQq", len(meta), data_off + len(data), data_off, 0)
    return head + meta + bytes(data_off - 48 - len(meta)) + data


def test_shader_apis_are_read_from_the_builds_files(tmp_path):
    data = tmp_path / "Data"
    data.mkdir()
    (data / "sharedassets0.assets").write_bytes(serialized_file(
        [(48, shader_object([14])), (28, b"texture" * 9), (48, shader_object([14, 4]))]))
    (data / "resources.assets").write_bytes(serialized_file([(48, shader_object([14]))]))
    (data / "sharedassets0.assets.resS").write_bytes(b"\x00" * 64)
    r = unity.shader_report(data)
    assert (r.codes, r.apis, r.shaders, r.unknown) == ([4, 14], ["Direct3D 11/12", "Metal"], 3, 0)
    assert r.covers("windows") and r.covers("macos") and not r.covers("linux")
    (data / "resources.assets").write_bytes(serialized_file([(48, b"not a shader tail")]))
    (data / "sharedassets0.assets").unlink()
    assert unity.shader_report(data).unknown == 1
    assert unity.shader_report(tmp_path / "nothing") is None


def test_metal_only_games_are_reported_and_warned(tmp_path):
    from ptk_engine import EngineError  # noqa: F401
    app = mac_game(tmp_path)
    data = app / "Contents/Resources/Data"
    (data / "sharedassets0.assets").write_bytes(serialized_file([(48, shader_object([14]))]))
    g = unity.detect(app)
    assert g.shaders["apis"] == ["Metal"] and any("Shaders: Metal only" in n for n in g.notes)
    r = run(tmp_path, "unity-import", app, "import")
    assert "would not draw it" in r["summary"]
    root = unity_install(tmp_path)
    r = run(tmp_path, "unity-export", app, "export", WINDOWS, {"unity": str(root)}, tmp_path / "win")
    assert any("Metal only, and Windows needs Direct3D" in w for w in r["warnings"])
    (data / "sharedassets0.assets").write_bytes(serialized_file([(48, shader_object([14, 4, 18]))]))
    r = run(tmp_path, "unity-import", app, "import")
    assert "can move to Windows, Linux" in r["summary"]


# -- API 2: checks, incremental output, the Mac repack ----------------------------------------


def test_check_offers_the_terms_and_a_second_export_writes_nothing(tmp_path):
    service = fake_module(tmp_path)
    settings = {"unity": str(tmp_path / "nowhere"), "unity_service": service.as_uri()}
    game = mac_game(tmp_path)
    (tmp_path / "project").mkdir(exist_ok=True)
    common = {"plugin": str(BUILTIN_DIR / "unity-export"), "kind": "export", "engine": "unity", "target": WINDOWS,
              "manifest": {"name": "p", "version": "1.0.0", "game": {"title": "Moon Garden"}},
              "cache_root": str(tmp_path / "cache")}
    folders = {"$GAME": str(game), "$PROJECT": str(tmp_path / "project")}
    r = run_plugin({**common, "phase": "check", "settings": {"unity_lookup": "false", **settings},
                    "folders": folders}, lambda m, p: None)["result"]
    assert not r["ok"]
    action = r["problems"][0]["action"]
    assert (action["setting"], action["value"]) == ("accept_unity_terms", "true")
    assert any("Native plugins" in p["message"] for p in r["problems"] if p["severity"] == "warning")
    out = tmp_path / "out"
    accepted = {**settings, "accept_unity_terms": "true"}
    first = run(tmp_path, "unity-export", game, "export", WINDOWS, accepted, out)
    assert first["build"]["rebuilt"] and first["build"]["written"] > 0
    second = run(tmp_path, "unity-export", game, "export", WINDOWS, accepted, out)
    assert second["build"]["written"] == 0 and second["build"]["removed"] == 0
    assert any("Player.log" in n for n in second["notes"])


def test_mac_to_mac_keeps_the_games_own_player_and_plugins(tmp_path):
    app = mac_game(tmp_path)
    write(app, {"Contents/PlugIns/AkUnitySoundEngine.bundle/Contents/MacOS/AkUnitySoundEngine": b"\xcf\xfawwise",
                "Contents/Frameworks/UnityPlayer.dylib": b"\xcf\xfaplayer",
                "Contents/_CodeSignature/CodeResources": b"apple",
                "Contents/_MASReceipt/receipt": b"receipt"})
    with open(app / "Contents" / "Info.plist", "wb") as f:
        plistlib.dump({"CFBundleExecutable": "Moon Garden", "CFBundleIdentifier": "com.apple.arcade.moon",
                       "NSMicrophoneUsageDescription": "voice"}, f)
    out = tmp_path / "out"
    r = run(tmp_path, "unity-export", app, "export", MAC, {"sign": "false"}, out)
    built = out / "Moon Garden.app" / "Contents"
    assert (built / "PlugIns/AkUnitySoundEngine.bundle/Contents/MacOS/AkUnitySoundEngine").read_bytes() == \
        b"\xcf\xfawwise"
    assert (built / "Frameworks/UnityPlayer.dylib").is_file()
    assert not (built / "_CodeSignature").exists() and not (built / "_MASReceipt").exists()
    plist = plistlib.loads((built / "Info.plist").read_bytes())
    assert plist["CFBundleIdentifier"] == "org.portingtoolkit.moon-garden"
    assert plist["NSMicrophoneUsageDescription"] == "voice" and plist["CFBundleExecutable"] == "Moon Garden"
    assert "the game's own" in r["notes"][0]
    assert any("FairPlay" in w or "subscription" in w for w in r["warnings"])
