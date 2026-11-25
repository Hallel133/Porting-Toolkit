"""The plugin runtime and the built-in Godot and HTML5 plugins."""

from __future__ import annotations

import json
import os
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import zipfile
from pathlib import Path

import pytest

from ptk_engine.formats import godot, html5
from ptk_engine.runtime import BUILTIN_DIR, find_plugins, run_plugin

MACOS = {"id": "macos-arm64", "name": "macOS (Apple Silicon)", "os": "macos", "arch": "arm64"}
WEB = {"id": "web", "name": "Web", "os": "web", "arch": "wasm32"}
GAME_FILES = {"project.godot": b"config_version=5\n", "main.tscn": b"[gd_scene]\n", "main.gd": b"extends Node\n"}


def run(tmp: Path, plugin: str, game: Path, project: Path, kind: str, target=None, settings=None, output=None):
    events: list[tuple[str, dict]] = []
    out = run_plugin(
        {
            "plugin": str(BUILTIN_DIR / plugin),
            "kind": kind,
            "engine": "godot" if plugin.startswith("godot") else "html5",
            "target": target,
            "manifest": {"name": "demo-port", "version": "1.2.0", "game": {"title": "Ptk Demo"}},
            "settings": settings or {},
            "folders": {"$GAME": str(game), "$PROJECT": str(project), "$OUTPUT": str(output) if output else None},
            "cache_root": str(tmp / "cache"),
        },
        lambda m, p: events.append((m, p)),
    )
    return out, events


# -- pck format ------------------------------------------------------------


@pytest.mark.parametrize("fmt,version", [(1, (3, 5, 3)), (2, (4, 2, 2)), (3, (4, 7, 2))])
@pytest.mark.parametrize("embedded", [False, True])
def test_pck_read_and_extract(tmp_path, fmt, version, embedded):
    exe = b"MZ" + b"\x90" * 1000 if embedded else None
    path = godot.write_pck(tmp_path / ("game.exe" if embedded else "game.pck"), GAME_FILES, version, fmt, exe)
    pck = godot.read(path)
    assert pck.version == version and pck.format == fmt and pck.embedded == embedded
    assert sorted(pck.paths()) == sorted(GAME_FILES)
    with open(path, "rb") as f:
        for e in pck.files:
            f.seek(e.offset)
            assert f.read(e.size) == GAME_FILES[e.path]
    alone = godot.read(godot.extract(pck, tmp_path / "out.pck"))
    assert not alone.embedded
    data = (tmp_path / "out.pck").read_bytes()
    for e in alone.files:
        assert data[e.offset:e.offset + e.size] == GAME_FILES[e.path]


def test_godot3_absolute_offsets_are_rewritten(tmp_path):
    path = godot.write_pck(tmp_path / "g.exe", GAME_FILES, (3, 4, 0), 1, b"\x7fELF" + b"\0" * 500, v1_absolute=True)
    pck = godot.read(path)
    assert pck.v1_absolute
    alone = godot.read(godot.extract(pck, tmp_path / "g.pck"))
    data = (tmp_path / "g.pck").read_bytes()
    assert all(data[e.offset:e.offset + e.size] == GAME_FILES[e.path] for e in alone.files)


def test_find_prefers_standalone_pck(tmp_path):
    godot.write_pck(tmp_path / "Game.exe", GAME_FILES, embed_into=b"MZ" + b"\0" * 100)
    assert godot.find(tmp_path) == [tmp_path / "Game.exe"]
    godot.write_pck(tmp_path / "Game.pck", GAME_FILES)
    assert godot.find(tmp_path)[0] == tmp_path / "Game.pck"
    assert godot.find(tmp_path / "Game.exe") == [tmp_path / "Game.exe"]


# -- fake Godot templates ------------------------------------------------------

SHELL = """<html><head><title>$GODOT_PROJECT_NAME</title>$GODOT_HEAD_INCLUDE
<style>body{background:$GODOT_SPLASH_COLOR}</style></head><body>
<img class="$GODOT_SPLASH_CLASSES" src="$GODOT_SPLASH">
<script src="$GODOT_URL"></script><script>
const GODOT_CONFIG = $GODOT_CONFIG;
const GODOT_THREADS_ENABLED = $GODOT_THREADS_ENABLED;
</script></body></html>"""

PLIST = """<plist><dict>
<key>CFBundleExecutable</key><string>$binary</string>
<key>CFBundleName</key><string>$name</string>
<key>CFBundleIdentifier</key><string>$bundle_identifier</string>
$liquid_glass_icon
<key>CFBundleShortVersionString</key><string>$short_version</string>
<key>LSApplicationCategoryType</key><string>public.app-category.$app_category</string>
<key>NSHighResolutionCapable</key>
$highres
$additional_plist_content
<key>DTXcode</key><string>$xcodever</string>
</dict></plist>"""


def fake_templates(root: Path) -> Path:
    d = root / "templates"
    d.mkdir()
    for name in ("web_nothreads_release.zip", "web_release.zip"):
        with zipfile.ZipFile(d / name, "w") as z:
            z.writestr("godot.html", SHELL)
            z.writestr("godot.js", "// engine")
            z.writestr("godot.wasm", b"\0asm" + b"\0" * 60)
            z.writestr("godot.audio.worklet.js", "// worklet")
            z.writestr("godot.service.worker.js", "const CACHE_VERSION = '___GODOT_VERSION___';\nconst CACHED = ___GODOT_CACHE___;")
            z.writestr("godot.offline.html", "<p>offline</p>")
    with zipfile.ZipFile(d / "macos.zip", "w") as z:
        z.writestr("macos_template.app/Contents/Info.plist", PLIST)
        z.writestr("macos_template.app/Contents/PkgInfo", "APPL????")
        z.writestr("macos_template.app/Contents/Resources/icon.icns", b"icns")
        z.writestr("macos_template.app/Contents/MacOS/godot_macos_release.universal", b"release")
        z.writestr("macos_template.app/Contents/MacOS/godot_macos_debug.universal", b"debug")
    return d


@pytest.fixture
def godot_game(tmp_path):
    game = tmp_path / "game"
    godot.write_pck(game / "PtkDemo.exe", GAME_FILES, (4, 7, 2), 3, b"MZ" + b"\0" * 4000)
    project = tmp_path / "project"
    project.mkdir()
    return game, project


def test_godot_import_writes_a_recipe(tmp_path, godot_game):
    game, project = godot_game
    out, events = run(tmp_path, "godot-import", game, project, "import")
    recipe = json.loads((project / "recipes" / "godot.json").read_text())
    assert recipe["pck"] == "PtkDemo.exe" and recipe["godot_version"] == "4.7.2" and recipe["embedded"]
    assert "Godot 4.7.2" in out["result"]["summary"]
    assert any(m == "progress" for m, _ in events)
    assert not any((project / "recipes").glob("*.pck")), "no game data in the project"


def test_godot_export_web(tmp_path, godot_game):
    game, project = godot_game
    templates = fake_templates(tmp_path)
    run(tmp_path, "godot-import", game, project, "import")
    out, _ = run(tmp_path, "godot-export", game, project, "export", WEB, {"templates": str(templates)}, tmp_path / "web")
    web = tmp_path / "web"
    page = (web / "index.html").read_text()
    assert "$GODOT" not in page and "<title>Ptk Demo</title>" in page
    config = json.loads(page.split("const GODOT_CONFIG = ")[1].split(";\n")[0])
    assert config["executable"] == "index"
    assert config["fileSizes"]["index.pck"] == (web / "index.pck").stat().st_size
    assert "GODOT_THREADS_ENABLED = false" in page and "serviceWorker" not in config
    assert {p.name for p in web.iterdir()} >= {"index.html", "index.js", "index.wasm", "index.pck", "index.audio.worklet.js"}
    assert not (web / "index.service.worker.js").exists()
    assert sorted(godot.read(web / "index.pck").paths()) == sorted(GAME_FILES)
    assert out["result"]["serve"] == str(web)


def test_godot_export_web_threads_adds_service_worker(tmp_path, godot_game):
    game, project = godot_game
    templates = fake_templates(tmp_path)
    run(tmp_path, "godot-export", game, project, "export", WEB,
        {"templates": str(templates), "threads": "true"}, tmp_path / "web")
    page = (tmp_path / "web" / "index.html").read_text()
    assert "GODOT_THREADS_ENABLED = true" in page and '"serviceWorker": "index.service.worker.js"' in page
    sw = (tmp_path / "web" / "index.service.worker.js").read_text()
    assert "___" not in sw and "index.offline.html" in sw


def test_godot_export_macos(tmp_path, godot_game):
    game, project = godot_game
    templates = fake_templates(tmp_path)
    out, _ = run(tmp_path, "godot-export", game, project, "export", MACOS,
                 {"templates": str(templates), "sign": "false", "thin": "false"}, tmp_path / "mac")
    app = tmp_path / "mac" / "Ptk Demo.app"
    assert out["result"]["outputs"] == [str(app)]
    exe = app / "Contents" / "MacOS" / "Ptk Demo"
    assert exe.read_bytes() == b"release" and os.access(exe, os.X_OK)
    assert [p.name for p in (app / "Contents" / "MacOS").iterdir()] == ["Ptk Demo"]
    plist = (app / "Contents" / "Info.plist").read_text()
    assert "$" not in plist
    assert "<string>Ptk Demo</string>" in plist and "org.portingtoolkit.ptk-demo" in plist
    assert "<string>1.2.0</string>" in plist and "public.app-category.games" in plist and "<true/>" in plist
    assert godot.read(app / "Contents" / "Resources" / "Ptk Demo.pck").version == (4, 7, 2)


def test_godot_export_refuses_encrypted(tmp_path, godot_game):
    game, project = godot_game
    pck = game / "PtkDemo.exe"
    data = bytearray(pck.read_bytes())
    start = godot.read(pck).start
    struct.pack_into("<I", data, start + 20, godot.DIR_ENCRYPTED | godot.REL_FILEBASE)
    pck.write_bytes(data)
    with pytest.raises(Exception, match="encrypted"):
        run(tmp_path, "godot-export", game, project, "export", WEB, {"templates": str(fake_templates(tmp_path))}, tmp_path / "w")


def test_output_folder_is_never_wiped_unless_ptk_made_it(tmp_path, godot_game):
    game, project = godot_game
    mine = tmp_path / "web"
    mine.mkdir()
    (mine / "precious.txt").write_text("keep me")
    with pytest.raises(Exception, match="not made by Porting Toolkit"):
        run(tmp_path, "godot-export", game, project, "export", WEB, {"templates": str(fake_templates(tmp_path))}, mine)
    assert (mine / "precious.txt").read_text() == "keep me"


# -- HTML5 ---------------------------------------------------------------------

PAGE = """<!doctype html><title>boot</title><script>
fetch('data/info.json').then(r => r.json()).then(j => { document.title = 'loaded:' + j.value; });
</script>"""


def nwjs_folder(root: Path) -> Path:
    (root / "data").mkdir(parents=True)
    (root / "index.html").write_text(PAGE)
    (root / "data" / "info.json").write_text('{"value": 42}')
    (root / "package.json").write_text(json.dumps({"name": "demo", "main": "index.html",
                                                    "window": {"title": "Nw Demo", "width": 800, "height": 600}}))
    (root / "nw.dll").write_bytes(b"MZ")
    return root


def write_asar(path: Path, files: dict[str, bytes]) -> Path:
    tree: dict = {"files": {}}
    blob = b""
    for name, data in files.items():
        node = tree
        parts = name.split("/")
        for p in parts[:-1]:
            node = node["files"].setdefault(p, {"files": {}})
        node["files"][parts[-1]] = {"size": len(data), "offset": str(len(blob))}
        blob += data
    raw = json.dumps(tree).encode()
    padded = raw + b"\0" * (-len(raw) % 4)
    header = struct.pack("<II", 4 + len(padded), len(raw)) + padded
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(struct.pack("<II", 4, len(header)) + header + blob)
    return path


def test_detect_nwjs_folder_package_nw_and_asar(tmp_path):
    g = html5.detect(nwjs_folder(tmp_path / "nw"))
    assert (g.flavor, g.container, g.root, g.entry, g.title, g.width) == ("nwjs", "folder", "", "index.html", "Nw Demo", 800)

    zipped = tmp_path / "zipped"
    zipped.mkdir()
    with zipfile.ZipFile(zipped / "package.nw", "w") as z:
        z.writestr("package.json", json.dumps({"main": "game/start.html"}))
        z.writestr("game/start.html", PAGE)
    g = html5.detect(zipped)
    assert (g.flavor, g.container, g.root, g.entry) == ("nwjs", "package.nw", "game", "start.html")

    files = {"package.json": b'{"main": "main.js", "productName": "Electro"}', "main.js": b"require('electron')",
             "app/index.html": PAGE.encode(), "app/data/info.json": b'{"value": 7}'}
    asar = write_asar(tmp_path / "el" / "resources" / "app.asar", files)
    g = html5.detect(tmp_path / "el")
    assert (g.flavor, g.container, g.root, g.title) == ("electron", "asar", "app", "Electro")
    dest = tmp_path / "unpacked"
    assert html5.extract_asar(asar, dest) == 4
    assert (dest / "app" / "data" / "info.json").read_bytes() == b'{"value": 7}'


def test_detect_rpgmaker_mv(tmp_path):
    root = tmp_path / "mv"
    (root / "www" / "js").mkdir(parents=True)
    (root / "www" / "js" / "rpg_core.js").write_text("//")
    (root / "www" / "index.html").write_text(PAGE)
    (root / "www" / "icon").mkdir()
    (root / "www" / "icon" / "icon.png").write_bytes(b"png")
    (root / "package.json").write_text(json.dumps({"main": "www/index.html", "window": {"title": "Quest", "width": 816, "height": 624}}))
    g = html5.detect(root)
    assert (g.flavor, g.root, g.title, g.width, g.icon) == ("rpgmaker-mv", "www", "Quest", 816, "icon/icon.png")


def test_unsafe_asar_paths_are_refused(tmp_path):
    asar = write_asar(tmp_path / "bad.asar", {"ok.txt": b"x"})
    data = asar.read_bytes().replace(b'"ok.txt"', b'"../x.t"')
    asar.write_bytes(data)
    with pytest.raises(html5.Html5Error):
        html5.extract_asar(asar, tmp_path / "out")


def test_html5_import_and_web_export(tmp_path):
    game = nwjs_folder(tmp_path / "game")
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "html5-import", game, project, "import")
    assert "NW.js game" in out["result"]["summary"]
    out, _ = run(tmp_path, "html5-export", game, project, "export", WEB, output=tmp_path / "web")
    web = tmp_path / "web"
    assert (web / "index.html").read_text() == PAGE and (web / "data" / "info.json").is_file()
    assert not (web / "nw.dll").exists()


@pytest.mark.skipif(sys.platform != "darwin" or not shutil.which("swiftc"), reason="needs macOS and swiftc")
def test_html5_macos_app_builds_and_loads_the_game(tmp_path):
    game = nwjs_folder(tmp_path / "game")
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "html5-export", game, project, "export", MACOS, output=tmp_path / "mac")
    app = Path(out["result"]["outputs"][0])
    assert app.name == "Ptk Demo.app"
    info = plistlib.loads((app / "Contents" / "Info.plist").read_bytes())
    assert info["CFBundleExecutable"] == "Ptk Demo" and info["CFBundleIdentifier"] == "org.portingtoolkit.ptk-demo"
    config = json.loads((app / "Contents" / "Resources" / "ptk-app.json").read_text())
    assert (config["width"], config["height"]) == (800, 600)
    assert subprocess.run(["codesign", "--verify", "--deep", str(app)], capture_output=True).returncode == 0
    if os.environ.get("PTK_GUI_TESTS") == "1":
        r = subprocess.run([str(app / "Contents" / "MacOS" / "Ptk Demo")], capture_output=True, text=True,
                           timeout=60, env={**os.environ, "PTK_WRAPPER_SELFTEST": "1"})
        assert "PTK_WRAPPER_LOADED loaded:42" in r.stdout, r.stdout + r.stderr


# -- runtime -------------------------------------------------------------------


def test_find_plugins_by_engine_and_target():
    ids = lambda **kw: [r["plugin"]["id"] for r in find_plugins(dirs=[], **kw)]  # noqa: E731
    assert ids(kind="export", engine="godot", target="web") == ["godot-export"]
    assert ids(kind="export", engine="rpgmaker", target="macos-arm64") == ["html5-export"]
    assert ids(kind="import", engine="html5", target=None) == ["html5-import"]
    assert ids(kind="export", engine="unreal", target="web") == []
    assert all(r["builtin"] for r in find_plugins("export", "godot", "web", []))


def test_permissions_are_enforced(tmp_path):
    plugin = tmp_path / "plugins" / "sneaky"
    (plugin / "sneaky").mkdir(parents=True)
    (plugin / "plugin.toml").write_text("""schema_version = 1
[plugin]
id = "sneaky"
name = "Sneaky"
version = "0.1.0"
kind = "export"
license = "MIT"
entry = "sneaky:Plugin"
[formats]
targets = ["web"]
[permissions]
read = ["$GAME"]
write = ["$OUTPUT"]
network = false
subprocess = false
""")
    (plugin / "sneaky" / "__init__.py").write_text(f"""
from pathlib import Path
class Plugin:
    def run(self, ctx):
        print("stray output must not break the channel")
        what = ctx.settings["try"]
        if what == "write":
            ctx.write_text(Path({str(tmp_path / 'elsewhere.txt')!r}), "x")
        elif what == "net":
            ctx.download("https://example.org/x")
        elif what == "run":
            ctx.run(["true"])
        return {{"summary": "ok"}}
""")
    base = {"plugin": str(plugin), "kind": "export", "target": WEB, "cache_root": str(tmp_path / "cache"),
            "folders": {"$GAME": str(tmp_path), "$OUTPUT": str(tmp_path / "out")}}
    for what, words in (("write", "may not write"), ("net", "may not contact"), ("run", "may not start")):
        with pytest.raises(Exception, match=words):
            run_plugin({**base, "settings": {"try": what}}, lambda *_: None)
    assert not (tmp_path / "elsewhere.txt").exists()
    assert run_plugin({**base, "settings": {"try": "none"}}, lambda *_: None)["result"]["summary"] == "ok"


# -- project settings, native libraries, icons -------------------------------------


def ecfg(values: dict) -> bytes:
    out = b"ECFG" + struct.pack("<I", len(values))
    for k, v in values.items():
        if isinstance(v, bool):
            enc = struct.pack("<II", 1, int(v))
        elif isinstance(v, str):
            raw = v.encode()
            enc = struct.pack("<II", 4, len(raw)) + raw + b"\0" * (-len(raw) % 4)
        elif isinstance(v, tuple):
            enc = struct.pack("<I", 20) + struct.pack("<ffff", *v)
        else:
            enc = struct.pack("<Ii", 2, v)
        key = k.encode()
        out += struct.pack("<I", len(key)) + key + struct.pack("<I", len(enc)) + enc
    return out


PNG_1PX = bytes.fromhex(
    "89504e470d0a1a0a0000000d4948445200000001000000010806000000"
    "1f15c4890000000d49444154789c6360f8cf00000301010018dd8db40000000049454e44ae426082")

GDEXT = b"""[configuration]
entry_symbol = "steam_init"
[libraries]
macos.debug = "res://addons/steam/osx/libsteam.framework"
windows.x86_64 = "res://addons/steam/win64/steam.dll"
"""


def mac_game(tmp_path: Path) -> Path:
    """A fake macOS Godot build with an extension in Contents/Frameworks."""
    files = {
        **GAME_FILES,
        "project.binary": ecfg({
            "application/config/name": "Real Title",
            "application/config/icon": "res://icon.png",
            "application/boot_splash/image": "res://splash.png",
            "application/boot_splash/bg_color": (0.0, 0.0, 0.0, 1.0),
            "autoload/Steam": "*res://steam.gd",
        }),
        "icon.png": PNG_1PX,
        "splash.png": PNG_1PX,
        "addons/steam/steam.gdextension": GDEXT,
    }
    app = tmp_path / "Real Title.app"
    godot.write_pck(app / "Contents/Resources/Real Title.pck", files, (4, 6, 3), 3)
    fw = app / "Contents/Frameworks/libsteam.framework"
    fw.mkdir(parents=True)
    (fw / "libsteam").write_bytes(b"\xcf\xfa\xed\xfe")
    (app / "Contents/Resources/icon.icns").write_bytes(b"icns-original")
    return app


def test_project_settings_are_read(tmp_path):
    app = mac_game(tmp_path)
    pck = godot.read(app / "Contents/Resources/Real Title.pck")
    s = godot.project_settings(pck)
    assert s["application/config/name"] == "Real Title"
    assert s["application/boot_splash/bg_color"] == [0.0, 0.0, 0.0, 1.0]
    assert godot.renderer(s, 4) == "forward_plus"
    assert godot.color_hex(s["application/boot_splash/bg_color"]) == "#000000"
    assert godot.extension_libraries(pck, "macos") == {
        "addons/steam/steam.gdextension": ["res://addons/steam/osx/libsteam.framework"]}


def test_import_finds_title_icon_and_autoloads(tmp_path):
    app = mac_game(tmp_path)
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "godot-import", app, project, "import")
    r = out["result"]
    assert r["title"] == "Real Title"
    assert Path(r["icon"]).read_bytes() == PNG_1PX
    recipe = json.loads((project / "recipes/godot.json").read_text())
    assert recipe["renderer"] == "forward_plus" and recipe["autoloads"] == ["Steam"]
    assert not list(project.rglob("*.png")), "the game's icon stays out of the project"


def test_macos_export_keeps_native_libraries_and_icon(tmp_path):
    app = mac_game(tmp_path)
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "godot-export", app, project, "export", MACOS,
                 {"templates": str(fake_templates(tmp_path)), "sign": "false", "thin": "false"}, tmp_path / "mac")
    built = Path(out["result"]["outputs"][0])
    assert (built / "Contents/Frameworks/libsteam.framework/libsteam").is_file()
    assert (built / "Contents/Resources/icon.icns").read_bytes() == b"icns-original"
    assert not any("steam" in w for w in out["result"]["warnings"])


def test_macos_export_warns_when_a_library_is_missing(tmp_path):
    app = mac_game(tmp_path)
    shutil.rmtree(app / "Contents/Frameworks")
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "godot-export", app, project, "export", MACOS,
                 {"templates": str(fake_templates(tmp_path)), "sign": "false", "thin": "false"}, tmp_path / "mac")
    assert any("steam" in w and "blank screen" in w for w in out["result"]["warnings"])


def test_web_export_uses_splash_icon_and_warns_about_renderer(tmp_path):
    app = mac_game(tmp_path)
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "godot-export", app, project, "export", WEB,
                 {"templates": str(fake_templates(tmp_path))}, tmp_path / "web")
    web = tmp_path / "web"
    page = (web / "index.html").read_text()
    assert 'src="index.png"' in page and "show-image--true" in page
    assert 'rel="icon" href="index.icon.png"' in page and "background:#000000" in page
    assert (web / "index.png").read_bytes() == PNG_1PX
    warnings = " ".join(out["result"]["warnings"])
    assert "Compatibility renderer" in warnings and "no web version" in warnings


def test_project_icon_wins(tmp_path):
    app = mac_game(tmp_path)
    project = tmp_path / "project"
    (project / "art").mkdir(parents=True)
    (project / "art/icon.png").write_bytes(b"\x89PNG mine")
    events = []
    out = run_plugin({
        "plugin": str(BUILTIN_DIR / "godot-export"), "kind": "export", "engine": "godot", "target": WEB,
        "manifest": {"game": {"title": "X", "icon": "art/icon.png"}},
        "settings": {"templates": str(fake_templates(tmp_path))},
        "folders": {"$GAME": str(app), "$PROJECT": str(project), "$OUTPUT": str(tmp_path / "w")},
        "cache_root": str(tmp_path / "cache")}, lambda m, p: events.append(p))
    assert (tmp_path / "w/index.icon.png").read_bytes() == b"\x89PNG mine"
    assert out["result"]["outputs"]


# -- Windows and Linux ---------------------------------------------------------------

WINDOWS = {"id": "windows-x64", "name": "Windows x64", "os": "windows", "arch": "x86_64"}
LINUX = {"id": "linux-x64", "name": "Linux x64", "os": "linux", "arch": "x86_64"}

GDEXT_DESKTOP = b"""[configuration]
entry_symbol = "steam_init"
[libraries]
windows.debug.x86_64 = "res://addons/steam/win64/steam_debug.dll"
windows.release.x86_64 = "res://addons/steam/win64/steam.dll"
windows.release.x86_32 = "res://addons/steam/win32/steam32.dll"
linux.release.x86_64 = "res://addons/steam/linux64/libsteam.so"
[dependencies]
windows.release.x86_64 = { "res://addons/steam/win64/steam_api64.dll": "" }
"""


def desktop_templates(root: Path) -> Path:
    d = fake_templates(root)
    (d / "windows_release_x86_64.exe").write_bytes(b"MZ-template")
    (d / "linux_release.x86_64").write_bytes(b"\x7fELF-template")
    return d


def windows_godot_game(tmp_path: Path) -> Path:
    """A Windows Godot build: the pck inside the .exe, GodotSteam next to it."""
    game = tmp_path / "game"
    files = {**GAME_FILES, "addons/steam/steam.gdextension": GDEXT_DESKTOP}
    godot.write_pck(game / "PtkDemo.exe", files, (4, 4, 1), 3, b"MZ" + b"\0" * 4000)
    (game / "steam.dll").write_bytes(b"MZ-steam")
    (game / "steam_api64.dll").write_bytes(b"MZ-steam-api")
    (game / "steam_appid.txt").write_text("480")
    return game


def zip_modes(path: Path) -> dict[str, int]:
    with zipfile.ZipFile(path) as z:
        return {i.filename: (i.external_attr >> 16) & 0o777 for i in z.infolist()}


def test_extension_files_pick_platform_arch_release_and_dependencies(tmp_path):
    game = windows_godot_game(tmp_path)
    pck = godot.read(game / "PtkDemo.exe")
    assert godot.extension_files(pck, "windows", "x86_64") == {
        "addons/steam/steam.gdextension": ["steam.dll", "steam_api64.dll"]}
    assert godot.extension_files(pck, "linux", "x86_64") == {"addons/steam/steam.gdextension": ["libsteam.so"]}
    assert godot.extension_files(pck, "macos") == {"addons/steam/steam.gdextension": []}


def plugin_module(folder: str, module: str):
    import importlib
    path = str(BUILTIN_DIR / folder)
    if path not in sys.path:
        sys.path.insert(0, path)
    return importlib.import_module(module)


def test_godot_desktop_template_names():
    Plugin = plugin_module("godot-export", "godot_export").Plugin
    assert Plugin.desktop_template(4, "windows", "x86_64") == "windows_release_x86_64.exe"
    assert Plugin.desktop_template(4, "linux", "x86_64") == "linux_release.x86_64"
    assert Plugin.desktop_template(3, "windows", "x86_64") == "windows_64_release.exe"
    assert Plugin.desktop_template(3, "linux", "x86_64") == "linux_x11_64_release"


def test_godot_export_windows(tmp_path):
    game = windows_godot_game(tmp_path)
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "godot-export", game, project, "export", WINDOWS,
                 {"templates": str(desktop_templates(tmp_path))}, tmp_path / "win")
    r = out["result"]
    folder = tmp_path / "win" / "Ptk Demo"
    assert r["open"] == str(folder)
    assert (folder / "Ptk Demo.exe").read_bytes() == b"MZ-template"
    pck = godot.read(folder / "Ptk Demo.pck")
    assert not pck.embedded and "main.gd" in pck.paths()
    assert (folder / "steam.dll").read_bytes() == b"MZ-steam"
    assert (folder / "steam_api64.dll").is_file() and (folder / "steam_appid.txt").read_text() == "480"
    assert not (folder / "steam_debug.dll").exists()
    assert not r["warnings"]
    modes = zip_modes(tmp_path / "win" / "Ptk Demo-windows-x64.zip")
    assert modes["Ptk Demo/Ptk Demo.exe"] == 0o755 and modes["Ptk Demo/Ptk Demo.pck"] == 0o644
    assert not any(".ptk-output" in n for n in modes)


def test_godot_export_linux_warns_about_missing_libraries(tmp_path):
    game = windows_godot_game(tmp_path)
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "godot-export", game, project, "export", LINUX,
                 {"templates": str(desktop_templates(tmp_path)), "zip": "false"}, tmp_path / "linux")
    folder = tmp_path / "linux" / "Ptk Demo"
    exe = folder / "Ptk Demo.x86_64"
    assert exe.read_bytes() == b"\x7fELF-template" and os.access(exe, os.X_OK)
    assert (folder / "Ptk Demo.pck").is_file() and (folder / "README.txt").is_file()
    assert not list((tmp_path / "linux").glob("*.zip"))
    assert any("No Linux version of the steam extension" in w for w in out["result"]["warnings"])


def fake_nwjs(root: Path, kind: str) -> Path:
    files = {"credits.html": b"<p>licences</p>", "locales/en-US.pak": b"pak", "nw.dll": b"MZ-nw"}
    if kind == "zip":
        path = root / "nwjs-v0.1.0-win-x64.zip"
        with zipfile.ZipFile(path, "w") as z:
            z.writestr("nwjs-v0.1.0-win-x64/nw.exe", b"MZ-nwjs")
            for n, b in files.items():
                z.writestr(f"nwjs-v0.1.0-win-x64/{n}", b)
        return path
    import io
    import tarfile
    path = root / "nwjs-v0.1.0-linux-x64.tar.gz"
    with tarfile.open(path, "w:gz") as tf:
        for n, b in {"nw": b"\x7fELF-nwjs", "lib/libnw.so": b"\x7fELF", **files}.items():
            info = tarfile.TarInfo(f"nwjs-v0.1.0-linux-x64/{n}")
            info.size = len(b)
            info.mode = 0o755 if n == "nw" else 0o644
            tf.addfile(info, io.BytesIO(b))
    return path


def test_html5_export_windows_with_nwjs(tmp_path):
    game = nwjs_folder(tmp_path / "game")
    (game / "movie.mp4").write_bytes(b"\0\0\0\x18ftypmp42")
    project = tmp_path / "project"
    project.mkdir()
    run(tmp_path, "html5-import", game, project, "import")
    out, _ = run(tmp_path, "html5-export", game, project, "export", WINDOWS,
                 {"nwjs": str(fake_nwjs(tmp_path, "zip"))}, tmp_path / "win")
    folder = tmp_path / "win" / "Ptk Demo"
    assert (folder / "Ptk Demo.exe").read_bytes() == b"MZ-nwjs" and not (folder / "nw.exe").exists()
    assert (folder / "credits.html").is_file() and (folder / "nw.dll").is_file()
    pkg = folder / "package.nw"
    assert (pkg / "index.html").read_text() == PAGE and (pkg / "data" / "info.json").is_file()
    assert not (pkg / "nw.dll").exists(), "the game's own runtime files stay out"
    meta = json.loads((pkg / "package.json").read_text())
    assert meta["main"] == "index.html" and meta["name"] == "demo"
    assert meta["window"]["width"] == 800 and meta["window"]["title"] == "Nw Demo"
    assert any("movie.mp4" in w for w in out["result"]["warnings"])
    assert (tmp_path / "win" / "Ptk Demo-windows-x64.zip").is_file()


def test_html5_export_linux_with_nwjs_keeps_the_executable_bit(tmp_path):
    game = nwjs_folder(tmp_path / "game")
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "html5-export", game, project, "export", LINUX,
                 {"nwjs": str(fake_nwjs(tmp_path, "tar.gz"))}, tmp_path / "linux")
    folder = tmp_path / "linux" / "Ptk Demo"
    assert os.access(folder / "Ptk Demo", os.X_OK) and (folder / "lib" / "libnw.so").is_file()
    modes = zip_modes(tmp_path / "linux" / "Ptk Demo-linux-x64.zip")
    assert modes["Ptk Demo/Ptk Demo"] == 0o755 and modes["Ptk Demo/package.nw/index.html"] == 0o644
    assert not out["result"]["warnings"]


def test_nwjs_pins_are_sha256():
    html5_export = plugin_module("html5-export", "html5_export")
    assert re.fullmatch(r"\d+\.\d+\.\d+", html5_export.NWJS_VERSION)
    for plat, kind, sha in html5_export.NWJS.values():
        assert re.fullmatch(r"[0-9a-f]{64}", sha) and kind in ("zip", "tar.gz")


def test_desktop_targets_find_their_exporters():
    ids = lambda **kw: [r["plugin"]["id"] for r in find_plugins(dirs=[], **kw)]  # noqa: E731
    assert ids(kind="export", engine="godot", target="windows-x64") == ["godot-export"]
    assert ids(kind="export", engine="rpgmaker", target="linux-x64") == ["html5-export"]


def test_rpgmaker_title_comes_from_system_json(tmp_path):
    game = tmp_path / "game"
    (game / "js").mkdir(parents=True)
    (game / "data").mkdir()
    (game / "index.html").write_text(PAGE)
    (game / "js" / "rmmz_core.js").write_text("//")
    (game / "package.json").write_text('{"name": "rmmz-game", "main": "index.html"}')
    (game / "data" / "System.json").write_text('﻿{"gameTitle": "Real Quest"}', encoding="utf-8")
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "html5-import", game, project, "import")
    assert out["result"]["title"] == "Real Quest"


@pytest.mark.parametrize("files,framework", [
    ({"index.html": PAGE, "scripts/c3runtime.js": "//", "data.json": "{}"}, "Construct 3"),
    ({"index.html": PAGE, "c2runtime.js": "//"}, "Construct 2"),
    ({"index.html": PAGE, "gd.js": "//", "pixi-renderers/x.js": "//"}, "GDevelop"),
    ({"index.html": PAGE, "js/phaser.min.js": "//"}, "Phaser"),
    ({"index.html": PAGE, "dmloader.js": "//", "archive/game.arcd": "x"}, "Defold"),
    ({"index.html": PAGE}, None),
])
def test_html5_frameworks(tmp_path, files, framework):
    game = tmp_path / "game"
    for rel, text in files.items():
        (game / rel).parent.mkdir(parents=True, exist_ok=True)
        (game / rel).write_text(text)
    g = html5.detect(game)
    assert g.flavor == "web" and g.framework == framework


def test_twine_story_without_index_html(tmp_path):
    game = tmp_path / "game"
    game.mkdir()
    (game / "My Story.html").write_text(
        '<html><body><tw-storydata name="Moon &amp; Back" startnode="1"><tw-passagedata pid="1" name="Start">Hi'
        '</tw-passagedata></tw-storydata></body></html>')
    g = html5.detect(game)
    assert (g.framework, g.entry, g.title) == ("Twine", "My Story.html", "Moon & Back")
    project = tmp_path / "project"
    project.mkdir()
    out, _ = run(tmp_path, "html5-import", game, project, "import")
    assert out["result"]["title"] == "Moon & Back" and out["result"]["summary"].startswith("Twine game")
    out, _ = run(tmp_path, "html5-export", game, project, "export", WEB, output=tmp_path / "web")
    assert "My Story.html" in (tmp_path / "web" / "index.html").read_text()


def test_an_old_installed_copy_does_not_hide_a_newer_builtin(tmp_path):
    def install(version: str) -> None:
        dst = tmp_path / version / "html5-export"
        shutil.copytree(BUILTIN_DIR / "html5-export", dst)
        toml = dst / "plugin.toml"
        toml.write_text(re.sub(r'(?m)^version = ".*"$', f'version = "{version}"', toml.read_text(), count=1))

    install("0.9.0")
    found = find_plugins("export", "html5", "web", [str(tmp_path / "0.9.0")])
    html5 = next(r for r in found if r["plugin"]["id"] == "html5-export")
    assert html5["builtin"]
    install("99.0.0")
    found = find_plugins("export", "html5", "web", [str(tmp_path / "99.0.0")])
    html5 = next(r for r in found if r["plugin"]["id"] == "html5-export")
    assert not html5["builtin"] and html5["plugin"]["version"] == "99.0.0"


def test_a_pinned_version_runs_instead_of_the_newest(tmp_path):
    dst = tmp_path / "html5-export"
    shutil.copytree(BUILTIN_DIR / "html5-export", dst)
    toml = dst / "plugin.toml"
    toml.write_text(re.sub(r'(?m)^version = ".*"$', 'version = "0.9.0"', toml.read_text(), count=1))
    pick = lambda pins: next(r for r in find_plugins("export", "html5", "web", [str(tmp_path)], pins=pins)  # noqa: E731
                             if r["plugin"]["id"] == "html5-export")
    assert pick(None)["builtin"]
    assert pick({"html5-export": str(dst)})["plugin"]["version"] == "0.9.0"
    assert pick({"html5-export": str(tmp_path / "gone")})["builtin"]


# -- API 2: a second export writes nothing ------------------------------------------------------


def test_staged_exporters_rewrite_only_what_changed(tmp_path, godot_game):
    game = nwjs_folder(tmp_path / "html5-game")
    project = tmp_path / "html5-project"
    project.mkdir()
    out = tmp_path / "web"
    first, _ = run(tmp_path, "html5-export", game, project, "export", WEB, output=out)
    assert first["result"]["build"]["rebuilt"] and first["result"]["serve"] == str(out)
    second, _ = run(tmp_path, "html5-export", game, project, "export", WEB, output=out)
    b = second["result"]["build"]
    assert b["removed"] == 0 and b["written"] <= 2, b  # only generated files with a new date
    assert (out / "index.html").read_text() == PAGE
    (game / "data" / "info.json").write_text('{"changed": true}')
    third, _ = run(tmp_path, "html5-export", game, project, "export", WEB, output=out)
    assert third["result"]["build"]["written"] >= 1
    assert json.loads((out / "data" / "info.json").read_text()) == {"changed": True}

    gdir, gproject = godot_game
    templates = fake_templates(tmp_path)
    run(tmp_path, "godot-import", gdir, gproject, "import")
    gout = tmp_path / "godot-web"
    run(tmp_path, "godot-export", gdir, gproject, "export", WEB, {"templates": str(templates)}, gout)
    again, _ = run(tmp_path, "godot-export", gdir, gproject, "export", WEB, {"templates": str(templates)}, gout)
    assert again["result"]["build"]["removed"] == 0
    assert again["result"]["build"]["unchanged"] >= 3, again["result"]["build"]
