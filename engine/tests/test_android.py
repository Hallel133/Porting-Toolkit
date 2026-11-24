"""Android: binary manifests, resource strings, aligned APKs and the exports."""

from __future__ import annotations

import io
import shutil
import struct
import zipfile
from pathlib import Path

import pytest

from ptk_engine import android
from ptk_engine.formats import axml, godot
from ptk_engine.runtime import BUILTIN_DIR, find_plugins, run_plugin

ANDROID = {"id": "android", "name": "Android", "os": "android", "arch": "arm64"}


def arsc(strings: list[str]) -> bytes:
    """A resource table with a string pool and a stand-in package chunk."""
    pool = axml.StringPool(strings).build()
    package = struct.pack("<HHI", 0x0200, 8, 16) + b"PKGDATA!"
    body = pool + package
    return struct.pack("<HHII", axml.RES_TABLE, 12, 12 + len(body), 1) + body


# -- formats ----------------------------------------------------------------------------


def test_manifest_round_trip_and_edits():
    raw = axml.build_manifest("com.godot.game", "com.godot.game.GodotApp", provider="com.godot.game.fileprovider")
    m = axml.Manifest(raw)
    assert m.to_dict() == {"package": "com.godot.game", "versionCode": 1, "versionName": "1.0", "permissions": [],
                           "activities": ["com.godot.game.GodotApp"], "providers": ["com.godot.game.fileprovider"]}
    assert axml.Manifest(m.build()).build() == m.build()
    patched = android.patch_manifest(raw, "org.portingtoolkit.moon", 10203, "1.2.3", 6, ["INTERNET", "a.b.C"])
    d = axml.Manifest(patched).to_dict()
    assert d["package"] == "org.portingtoolkit.moon" and d["versionCode"] == 10203 and d["versionName"] == "1.2.3"
    assert d["providers"] == ["org.portingtoolkit.moon.fileprovider"]
    assert d["activities"] == ["com.godot.game.GodotApp"], "class names never change"
    assert d["permissions"] == ["android.permission.INTERNET", "a.b.C"]
    act = axml.Manifest(patched).elements("activity")[0]
    assert axml.Manifest(patched).value(act, "screenOrientation") == 6


def test_utf8_string_pools_are_read():
    strings = ["godot-project-name", "héllo", ""]
    body = bytearray()
    offsets = []
    for s in strings:
        offsets.append(len(body))
        b = s.encode("utf-8")
        body += bytes([len(s), len(b)]) + b + b"\0"
    while len(body) % 4:
        body += b"\0"
    start = 28 + 4 * len(strings)
    pool = struct.pack("<HHIIIIII", 1, 28, start + len(body), len(strings), 0, axml.UTF8_FLAG, start, 0)
    pool += struct.pack(f"<{len(strings)}I", *offsets) + bytes(body)
    data = struct.pack("<HHII", axml.RES_TABLE, 12, 12 + len(pool), 1) + pool
    assert axml.table_strings(data) == strings
    out = axml.replace_table_strings(data, lambda s: "Moon Garden" if s.startswith("godot-project-name") else s)
    assert axml.table_strings(out) == ["Moon Garden", "héllo", ""]
    assert struct.unpack_from("<I", out, 4)[0] == len(out)


def test_resource_table_strings_keep_what_follows():
    data = arsc(["godot-project-name", "godot-project-name-de", "other"])
    out = axml.replace_table_strings(data, lambda s: "Spiel" if s.startswith("godot-project-name") else s)
    assert axml.table_strings(out) == ["Spiel", "Spiel", "other"]
    assert out.endswith(b"PKGDATA!")


def test_names_and_versions():
    assert android.package_name("Moon Garden 2") == "org.portingtoolkit.moon_garden_2"
    assert android.package_name("2048!") == "org.portingtoolkit.g2048"
    assert android.package_name("x", "com.you.game") == "com.you.game"
    with pytest.raises(android.AndroidError):
        android.package_name("x", "not a package")
    assert android.version_code("1.2.3") == 10203 and android.version_code("0.1") == 100
    assert android.version_code("1.0", "77") == 77


def test_aligned_apk(tmp_path):
    big = tmp_path / "big.bin"
    big.write_bytes(b"x" * 5000)
    entries = [
        android.Entry("AndroidManifest.xml", b"m" * 33, True),
        android.Entry("assets/a.png", b"p" * 7, False),
        android.Entry("lib/arm64-v8a/libgame.so", b"\x7fELF" * 3, False),
        android.Entry("assets/b.ogg", b"o" * 3, False),
        android.Entry("assets/big.bin", None, False, src=(big, 1000, 2000)),
    ]
    apk = android.write_aligned(entries, tmp_path / "a.apk")
    assert android.check_alignment(apk) == []
    with zipfile.ZipFile(apk) as z:
        assert z.read("assets/big.bin") == b"x" * 2000 and z.testzip() is None
    with pytest.raises(android.AndroidError):
        android.write_aligned([entries[1], entries[1]], tmp_path / "dup.apk")


# -- Godot ----------------------------------------------------------------------------------


IMPORT_DESKTOP = (b'[remap]\n\nimporter="texture"\ntype="CompressedTexture2D"\n'
                  b'path.s3tc="res://.godot/imported/a.s3tc.ctex"\nmetadata={\n"vram_texture": true\n}\n')
IMPORT_MOBILE = (b'[remap]\n\nimporter="texture"\npath.s3tc="res://.godot/imported/b.s3tc.ctex"\n'
                 b'path.etc2="res://.godot/imported/b.etc2.ctex"\n')
IMPORT_PLAIN = b'[remap]\n\nimporter="texture"\npath="res://.godot/imported/c.ctex"\n'


def godot_game(tmp: Path, version=(4, 4, 1)) -> Path:
    files = {"project.binary": b"ECFG\0\0\0\0", "main.tscn": b"[gd_scene]\n",
             "a.png.import": IMPORT_DESKTOP, "b.png.import": IMPORT_MOBILE, "c.png.import": IMPORT_PLAIN,
             ".godot/imported/a.s3tc.ctex": b"GST2", ".godot/imported/b.s3tc.ctex": b"GST2",
             ".godot/imported/b.etc2.ctex": b"GST2", ".godot/imported/c.ctex": b"GST2"}
    godot.write_pck(tmp / "game" / "Moon.pck", files, version, 3 if version >= (4, 4) else 2)
    return tmp / "game"


def fake_template(tmp: Path) -> Path:
    d = tmp / "templates"
    d.mkdir()
    with zipfile.ZipFile(d / "android_release.apk", "w") as z:
        z.writestr("AndroidManifest.xml", axml.build_manifest("com.godot.game", "com.godot.game.GodotApp",
                                                              provider="com.godot.game.fileprovider"))
        z.writestr(zipfile.ZipInfo("resources.arsc"), arsc(["godot-project-name", "x"]))
        for abi in ("arm64-v8a", "armeabi-v7a", "x86", "x86_64"):
            z.writestr(f"lib/{abi}/libgodot_android.so", b"\x7fELF")
        z.writestr("classes.dex", b"dex\n035")
        z.writestr("META-INF/CERT.SF", b"old signature")
    return d


def run(tmp: Path, plugin: str, game: Path, settings: dict, engine="godot"):
    (tmp / "project").mkdir(exist_ok=True)
    return run_plugin({
        "plugin": str(BUILTIN_DIR / plugin), "kind": "export", "engine": engine, "target": ANDROID,
        "manifest": {"name": "p", "version": "1.2.3", "game": {"title": "Moon Garden"}},
        "settings": {"download_tools": "false", **settings},
        "folders": {"$GAME": str(game), "$PROJECT": str(tmp / "project"), "$OUTPUT": str(tmp / "out")},
        "cache_root": str(tmp / "cache"),
    }, lambda m, p: None)["result"]


def test_godot_imports_get_a_fallback_for_phones(tmp_path):
    import importlib
    import sys
    sys.path.insert(0, str(BUILTIN_DIR / "godot-export"))
    Plugin = importlib.import_module("godot_export").Plugin
    pck = godot.read(godot_game(tmp_path) / "Moon.pck")
    changed, fixed, need = Plugin.android_imports(pck)
    assert (fixed, need) == (1, 1) and set(changed) == {"a.png.import"}
    text = changed["a.png.import"].decode()
    assert 'path.s3tc="res://.godot/imported/a.s3tc.ctex"\npath="res://.godot/imported/a.s3tc.ctex"\n' in text


def test_godot_android_apk_unsigned(tmp_path):
    game = godot_game(tmp_path)
    r = run(tmp_path, "godot-export", game, {"templates": str(fake_template(tmp_path)), "sign": "false"})
    apk = tmp_path / "out" / "Moon Garden.apk"
    assert r["outputs"] == [str(apk)] and any("1 textures" in w for w in r["warnings"])
    with zipfile.ZipFile(apk) as z:
        names = set(z.namelist())
        assert {"assets/main.tscn", "assets/project.binary", "assets/.godot/imported/a.s3tc.ctex",
                "assets/_cl_", "lib/arm64-v8a/libgodot_android.so"} <= names
        assert not any(n.startswith(("lib/x86", "META-INF/")) for n in names)
        assert b'path="res://.godot/imported/a.s3tc.ctex"' in z.read("assets/a.png.import")
        assert z.read("assets/_cl_") == b"\0\0\0\0"
        d = axml.Manifest(z.read("AndroidManifest.xml")).to_dict()
        assert d["package"] == "org.portingtoolkit.moon_garden" and d["versionCode"] == 10203
        assert axml.table_strings(z.read("resources.arsc"))[0] == "Moon Garden"
    assert android.check_alignment(apk) == []


def test_old_godot_without_mobile_textures_is_refused(tmp_path):
    from ptk_engine import EngineError
    game = godot_game(tmp_path, (4, 1, 3))
    with pytest.raises(EngineError, match="cannot decode"):
        run(tmp_path, "godot-export", game, {"templates": str(fake_template(tmp_path)), "sign": "false"})


def test_android_target_finds_exporters():
    ids = lambda **kw: [r["plugin"]["id"] for r in find_plugins(dirs=[], **kw)]  # noqa: E731
    assert ids(kind="export", engine="godot", target="android") == ["godot-export"]
    assert ids(kind="export", engine="rpgmaker", target="android") == ["html5-export"]
    assert ids(kind="export", engine="renpy", target="android") == []


# -- with the real Android SDK (skipped without it) ----------------------------------------------

needs_sdk = pytest.mark.skipif(android.find_sdk() is None or android.find_jdk_tool("javac") is None
                               or android.find_sdk().platform_jar is None,
                               reason="needs the Android SDK (build-tools + a platform) and a JDK")


@needs_sdk
def test_html5_android_apk_builds_and_verifies(tmp_path):
    game = tmp_path / "www"
    (game / "img").mkdir(parents=True)
    (game / "index.html").write_text("<!doctype html><title>t</title><img src='img/A.png'>")
    (game / "img" / "a.png").write_bytes(b"\x89PNG\r\n\x1a\n")
    (game / "package.json").write_text('{"name": "moon", "main": "index.html", "window": {"width": 600, "height": 900}}')
    r = run(tmp_path, "html5-export", game, {}, engine="html5")
    apk = Path(r["outputs"][0])
    sdk = android.find_sdk()
    assert android.check_alignment(apk) == []
    env = {**__import__("os").environ, **android.java_env()}
    import subprocess
    assert subprocess.run([sdk.tool("apksigner"), "verify", str(apk)], capture_output=True, env=env).returncode == 0
    badging = subprocess.run([sdk.tool("aapt2"), "dump", "badging", str(apk)], capture_output=True, text=True).stdout
    assert "name='org.portingtoolkit.moon_garden'" in badging and "application-label:'Moon Garden'" in badging
    with zipfile.ZipFile(apk) as z:
        assert z.read("assets/www/index.html").startswith(b"<!doctype")
        assert "www/img/a.png" in z.read("assets/ptk-files.txt").decode()
        m = axml.Manifest(z.read("AndroidManifest.xml"))
        act = m.elements("activity")[0]
        assert m.value(act, "screenOrientation") == android.ORIENTATIONS["sensor_portrait"]  # 600x900
