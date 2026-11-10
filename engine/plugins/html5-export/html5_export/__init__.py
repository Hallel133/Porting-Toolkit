"""HTML5 export: a web folder, or a native macOS app around the game.

- Web: the game's web root, ready for any static host.
- macOS: ``<Title>.app`` with a small WebKit wrapper (``wrapper.swift``,
  compiled once per architecture with the Xcode command line tools and
  cached), the game in ``Contents/Resources/app``, its icon converted to
  ``.icns``, and an ad-hoc signature.

Settings (``--set key=value``): ``bundle_id``, ``width``, ``height``,
``sign`` (default true).
"""

from __future__ import annotations

import hashlib
import html
import json
import re
import shutil
from pathlib import Path

from ptk_engine.formats import html5

HERE = Path(__file__).resolve().parent
WRAPPER = HERE / "wrapper.swift"
MIN_MACOS = "12.0"
DESKTOP_ONLY = {"nw.dll", "nw_elf.dll", "node.dll", "ffmpeg.dll", "libEGL.dll", "libGLESv2.dll", "d3dcompiler_47.dll"}

INFO_PLIST = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>{binary}</string>
	<key>CFBundleIdentifier</key>
	<string>{bundle}</string>
	<key>CFBundleName</key>
	<string>{name}</string>
	<key>CFBundleDisplayName</key>
	<string>{name}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
{icon}	<key>LSApplicationCategoryType</key>
	<string>public.app-category.games</string>
	<key>LSMinimumSystemVersion</key>
	<string>{min_os}</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
	<key>NSHumanReadableCopyright</key>
	<string>Made with Porting Toolkit</string>
</dict>
</plist>
"""


def safe_name(title: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .")[:64] or "Game"


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


def skip_desktop_only(path: Path) -> bool:
    return path.name in DESKTOP_ONLY or path.name.startswith("._") or path.name == ".DS_Store"


class Plugin:
    def run(self, ctx):
        target = ctx.target or {}
        if target.get("os") not in ("macos", "web"):
            ctx.fail(f"html5-export builds for macOS and the web, not {target.get('id')}")
        info = ctx.recipe("html5")
        if not info:
            try:
                info = html5.detect(ctx.game).to_dict()
            except html5.Html5Error as e:
                ctx.fail(str(e))
        ctx.progress(0.05, "Preparing the game files")
        try:
            root = html5.materialize(ctx.game, info, ctx.temp)
        except (html5.Html5Error, OSError) as e:
            ctx.fail(str(e))
        title = info.get("title") if ctx.title in ("", "Game") else ctx.title
        title = title or "Game"
        if info.get("uses_node"):
            ctx.warn("The game uses Node.js APIs (NW.js/Electron); those parts will not work outside its desktop runtime.")
        if target["os"] == "web":
            out = self.web(ctx, root, info, title)
        else:
            out = self.macos(ctx, root, info, title)
        ctx.progress(1.0, "Done")
        return out

    def web(self, ctx, root: Path, info: dict, title: str) -> dict:
        out = ctx.clean_dir(ctx.output)
        ctx.progress(0.2, "Copying the web root")
        n = ctx.copy_tree(root, out, skip=skip_desktop_only)
        entry = info.get("entry", "index.html")
        if entry != "index.html" and not (out / "index.html").exists():
            ctx.write_text(out / "index.html", REDIRECT.format(entry=html.escape(entry), title=html.escape(title)))
        ctx.write_text(out / "README-ptk.txt", WEB_README.format(title=title))
        return {
            "outputs": [str(out / "index.html")],
            "open": str(out / "index.html"),
            "serve": str(out),
            "summary": f"Copied {n} files for the web into {out.name}/",
            "notes": ["Saves use the browser's local storage (per site address)."],
        }

    def wrapper_binary(self, ctx, arch: str) -> Path:
        source = WRAPPER.read_bytes()
        swiftc = ctx.which("swiftc")
        if not swiftc:
            ctx.fail("the macOS app needs the Xcode command line tools (run: xcode-select --install)")
        version = ctx.run([swiftc, "--version"], check=False).stdout.strip().splitlines()[:1]
        key = hashlib.sha256(source + "|".join([arch, MIN_MACOS, *version]).encode()).hexdigest()[:16]
        cached = ctx.cache / "wrapper" / f"{arch}-{key}"
        if cached.is_file():
            return cached
        ctx.progress(0.3, f"Compiling the app wrapper for {arch} (once)")
        cached.parent.mkdir(parents=True, exist_ok=True)
        tmp = ctx.temp / f"wrapper-{arch}"
        ctx.run([swiftc, "-O", "-swift-version", "5", "-target", f"{arch}-apple-macos{MIN_MACOS}",
                 "-o", tmp, WRAPPER])
        shutil.move(str(tmp), cached)
        return cached

    def icon(self, ctx, root: Path, info: dict, resources: Path) -> bool:
        rel = info.get("icon")
        src = root / rel if rel else None
        if not src or not src.is_file() or src.suffix.lower() not in (".png", ".jpg", ".jpeg"):
            return False
        if not (ctx.which("sips") and ctx.which("iconutil")):
            return False
        iconset = ctx.temp / "icon.iconset"
        iconset.mkdir(exist_ok=True)
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                px = size * scale
                name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
                ctx.run(["sips", "-s", "format", "png", "-z", str(px), str(px), src, "--out", iconset / name])
        ctx.run(["iconutil", "-c", "icns", iconset, "-o", resources / "icon.icns"])
        return True

    def macos(self, ctx, root: Path, info: dict, title: str) -> dict:
        arch = {"arm64": "arm64", "x86_64": "x86_64"}.get(ctx.target.get("arch"), "arm64")
        binary_src = self.wrapper_binary(ctx, arch)
        name = safe_name(title)
        app = ctx.clean_dir(ctx.output / f"{name}.app")
        contents = app / "Contents"
        resources = contents / "Resources"
        (contents / "MacOS").mkdir(parents=True, exist_ok=True)
        resources.mkdir(parents=True, exist_ok=True)
        exe = contents / "MacOS" / name
        shutil.copyfile(binary_src, exe)
        exe.chmod(0o755)
        ctx.progress(0.5, "Copying the game into the app")
        n = ctx.copy_tree(root, resources / "app", skip=skip_desktop_only)
        width = int(ctx.setting("width") or info.get("width") or 1280)
        height = int(ctx.setting("height") or info.get("height") or 720)
        ctx.write_text(resources / "ptk-app.json", json.dumps(
            {"entry": info.get("entry", "index.html"), "title": title, "width": width, "height": height}, indent=2))
        has_icon = self.icon(ctx, root, info, resources)
        bundle = ctx.setting("bundle_id") or f"org.portingtoolkit.{slug(title)}"
        version = str(ctx.manifest.get("version") or "1.0.0")
        icon_xml = "\t<key>CFBundleIconFile</key>\n\t<string>icon.icns</string>\n" if has_icon else ""
        ctx.write_text(contents / "Info.plist", INFO_PLIST.format(
            binary=html.escape(name), bundle=html.escape(bundle), name=html.escape(title),
            version=html.escape(version), icon=icon_xml, min_os=MIN_MACOS))
        ctx.write_text(contents / "PkgInfo", "APPL????")
        signed = False
        if ctx.flag("sign", True) and ctx.which("codesign"):
            ctx.progress(0.9, "Signing (ad-hoc)")
            ctx.run(["codesign", "--force", "--deep", "--sign", "-", "--timestamp=none", app])
            signed = True
        return {
            "outputs": [str(app)],
            "open": str(app),
            "summary": f"Built {app.name} ({n} game files, {arch})",
            "notes": [
                "Runs the game in WebKit (Safari's engine). Saves live in the app's own storage.",
                "Signed ad-hoc: it runs on this Mac. To share it, sign and notarize it with an Apple Developer ID."
                if signed else "Not signed.",
            ],
        }


REDIRECT = """<!doctype html>
<meta charset="utf-8">
<title>{title}</title>
<meta http-equiv="refresh" content="0; url={entry}">
<a href="{entry}">Start</a>
"""

WEB_README = """\
{title}: web build made by Porting Toolkit

Browsers block games that load their data from a file:// address. Serve
this folder:

    ptk serve .            (or: python3 -m http.server)

and open http://localhost:8000/. To publish it, upload the folder to any
static host (for itch.io: zip it and choose "HTML").
"""
