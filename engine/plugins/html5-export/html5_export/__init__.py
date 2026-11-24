"""HTML5 export: a web folder, a native macOS app, or an NW.js game folder.

- Web: the game's web root, ready for any static host.
- macOS: ``<Title>.app`` with a small WebKit wrapper (``wrapper.swift``,
  compiled once per architecture with the Xcode command line tools and
  cached), the game in ``Contents/Resources/app``, its icon converted to
  ``.icns``, and an ad-hoc signature.
- Windows / Linux: the NW.js runtime (the one RPG Maker MV/MZ ship with;
  a pinned release from dl.nwjs.io, checked against its SHA-256) renamed
  to ``<Title>.exe`` / ``<Title>``, with the game in ``package.nw/``, and a
  zip of the folder.

Settings (``--set key=value``): ``bundle_id``, ``width``, ``height``,
``sign`` (default true), ``assets`` (apply/plan/off), ``nwjs`` (a local
NW.js folder or archive instead of the download), ``zip`` (default true).

The project's advanced options are applied to the copy in the output
(``ctx.process_assets``): files keep their names, so the game still finds
them.
"""

from __future__ import annotations

import hashlib
import html
import json
import re
import shutil
import tarfile
from pathlib import Path

from ptk_engine.formats import html5

HERE = Path(__file__).resolve().parent
WRAPPER = HERE / "wrapper.swift"
MIN_MACOS = "12.0"
#: NW.js "normal" builds (no SDK tools), from https://dl.nwjs.io/v<version>/SHASUMS256.txt
NWJS_VERSION = "0.119.2"
NWJS = {
    "windows": ("win-x64", "zip", "4fed5fafb00a1ff59e2e77f7682d7d68fe3466367b8ee351ffb1f00714e094fe"),
    "linux": ("linux-x64", "tar.gz", "eb1cb65f3dfc49966d8c9a83da1aae3c2e4e916aaae6d623a634a6860d29224e"),
}
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
        if target.get("os") not in ("macos", "web", "windows", "linux"):
            ctx.fail(f"html5-export builds for macOS, Windows, Linux and the web, not {target.get('id')}")
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
        profile = info.get("flavor") if str(info.get("flavor", "")).startswith("rpgmaker") else "html5"
        if ctx.assets_mode == "plan":
            report = ctx.process_assets(root, profile)
            return {"summary": "Estimated the advanced options (nothing was built)", "assets": report}
        if info.get("uses_node"):
            ctx.warn("The game uses Node.js APIs (NW.js/Electron); those parts will not work outside its desktop runtime.")
        self.profile = profile
        if target["os"] == "web":
            out = self.web(ctx, root, info, title)
        elif target["os"] in ("windows", "linux"):
            out = self.desktop(ctx, root, info, title, target["os"])
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
        assets = ctx.process_assets(out, self.profile)
        return {
            "assets": assets,
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
        """--set icon=, then the project's icon, then the game's own."""
        rel = info.get("icon")
        chosen = ctx.setting("icon")
        src = Path(chosen).expanduser() if chosen else ctx.art("icon") or (root / rel if rel else None)
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
        assets = ctx.process_assets(resources / "app", self.profile)
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
            "assets": assets,
            "outputs": [str(app)],
            "open": str(app),
            "summary": f"Built {app.name} ({n} game files, {arch})",
            "notes": [
                "Runs the game in WebKit (Safari's engine). Saves live in the app's own storage.",
                "Signed ad-hoc: it runs on this Mac. To share it, sign and notarize it with an Apple Developer ID."
                if signed else "Not signed.",
            ],
        }


    # -- Windows and Linux (NW.js) -------------------------------------------

    def nwjs_runtime(self, ctx, os_: str) -> Path:
        """The unpacked NW.js runtime folder (it holds nw.exe or nw)."""
        exe = "nw.exe" if os_ == "windows" else "nw"
        custom = ctx.setting("nwjs")
        if custom:
            src = Path(custom).expanduser()
            if src.is_dir():
                return self._runtime_root(ctx, src, exe)
            if not src.is_file():
                ctx.fail(f"nwjs={custom} is not a folder or an archive")
            kind = "zip" if src.suffix.lower() == ".zip" else "tar.gz"
            key = hashlib.sha256(f"{src.resolve()}|{src.stat().st_size}|{src.stat().st_mtime_ns}".encode()).hexdigest()[:16]
            archive = src
        else:
            plat, kind, sha = NWJS[os_]
            name = f"nwjs-v{NWJS_VERSION}-{plat}.{kind}"
            ctx.progress(0.1, f"Downloading NW.js {NWJS_VERSION} for {os_.title()} (once)")
            archive = ctx.download(f"https://dl.nwjs.io/v{NWJS_VERSION}/{name}", sha256=sha)
            key = f"{NWJS_VERSION}-{plat}"
        dest = ctx.cache / "nwjs" / key
        if not (dest / ".complete").is_file():
            ctx.progress(0.2, "Unpacking NW.js")
            shutil.rmtree(dest, ignore_errors=True)
            dest.mkdir(parents=True)
            if kind == "zip":
                html5.extract_zip(archive, dest)
            else:
                with tarfile.open(archive, "r:*") as tf:
                    try:
                        tf.extractall(dest, filter="data")  # refuses links and paths outside dest
                    except TypeError:  # Python without extraction filters
                        for m in tf.getmembers():
                            target = (dest / m.name).resolve()
                            if not (m.isfile() or m.isdir()) or dest.resolve() not in target.parents:
                                continue
                            tf.extract(m, dest)
            (dest / ".complete").write_text(str(archive.name) + "\n")
        return self._runtime_root(ctx, dest, exe)

    @staticmethod
    def _runtime_root(ctx, folder: Path, exe: str) -> Path:
        if (folder / exe).is_file():
            return folder
        subs = [d for d in folder.iterdir() if d.is_dir() and (d / exe).is_file()]
        if not subs:
            ctx.fail(f"{folder} has no {exe}: not an NW.js runtime for this platform")
        return subs[0]

    def desktop(self, ctx, root: Path, info: dict, title: str, os_: str) -> dict:
        runtime = self.nwjs_runtime(ctx, os_)
        name = safe_name(title)
        ctx.clean_dir(ctx.output)
        folder = ctx.output / name
        ctx.check_write(folder)
        ctx.progress(0.4, "Copying the NW.js runtime")
        shutil.copytree(runtime, folder, symlinks=True,
                        ignore=shutil.ignore_patterns(".complete", "._*", ".DS_Store"))
        exe_src = folder / ("nw.exe" if os_ == "windows" else "nw")
        exe = folder / (f"{name}.exe" if os_ == "windows" else name)
        exe_src.rename(exe)
        exe.chmod(0o755)
        ctx.progress(0.6, "Copying the game")
        pkg = folder / "package.nw"
        n = ctx.copy_tree(root, pkg, skip=skip_desktop_only)
        self.package_json(ctx, pkg, info, title)
        assets = ctx.process_assets(pkg, self.profile)
        self.codec_warnings(ctx, pkg)
        ctx.write_text(folder / "README.txt", DESKTOP_README[os_].format(title=title, exe=exe.name,
                                                                         version=NWJS_VERSION))
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            ctx.progress(0.9, "Zipping")
            outputs.append(str(ctx.zip_dir(folder, ctx.output / f"{name}-{ctx.target.get('id')}.zip",
                                           executables={exe.name})))
        notes = [f"Runs the game with NW.js {NWJS_VERSION} (Chromium), like RPG Maker's own desktop builds. "
                 "Saves stay next to the game, in package.nw."]
        if os_ == "windows":
            notes.append("The .exe keeps the NW.js icon and is not signed, so Windows SmartScreen may ask "
                         "before the first start (More info, then Run anyway).")
        else:
            notes.append(f"Start it with ./{exe.name}; the zip keeps the executable bit.")
        return {
            "assets": assets,
            "outputs": outputs,
            "open": str(folder),
            "summary": f"Built {folder.name}/ for {ctx.target.get('name')} ({n} game files)",
            "notes": notes,
        }

    @staticmethod
    def package_json(ctx, pkg: Path, info: dict, title: str) -> None:
        """NW.js starts from package.json: ``main`` must be the game's page."""
        path = pkg / "package.json"
        try:
            data = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
        except (OSError, ValueError):
            data = {}
        if not isinstance(data, dict):
            data = {}
        data["main"] = info.get("entry") or "index.html"
        if not re.fullmatch(r"[A-Za-z0-9._-]+", str(data.get("name") or "")):
            data["name"] = slug(title)
        window = data.get("window") if isinstance(data.get("window"), dict) else {}
        window.setdefault("title", title)
        if info.get("width") and info.get("height"):
            window.setdefault("width", info["width"])
            window.setdefault("height", info["height"])
        icon = info.get("icon")
        if icon and (pkg / icon).is_file():
            window.setdefault("icon", icon)
        data["window"] = window
        ctx.write_text(path, json.dumps(data, indent=2, ensure_ascii=False) + "\n")

    @staticmethod
    def codec_warnings(ctx, pkg: Path) -> None:
        """NW.js "normal" builds have no H.264 or AAC decoder."""
        alternatives = {".mp4": (".webm", ".ogv"), ".m4a": (".ogg",), ".aac": (".ogg",)}
        lonely = []
        for p in pkg.rglob("*"):
            alts = alternatives.get(p.suffix.lower())
            if alts and not any(p.with_suffix(a).exists() for a in alts):
                lonely.append(p.relative_to(pkg).as_posix())
        if lonely:
            ctx.warn(f"{len(lonely)} files ({', '.join(lonely[:3])}{'…' if len(lonely) > 3 else ''}) are H.264/AAC "
                     "with no .webm/.ogg twin; NW.js cannot play those. Convert them to WebM/Ogg if they are needed.")


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

DESKTOP_README = {
    "windows": """\
{title}: Windows build made by Porting Toolkit (NW.js {version})

Start {exe}. The game itself is in package.nw\\, and so are its saves.
Windows may warn about an unknown publisher the first time: choose
"More info", then "Run anyway". credits.html lists the NW.js licences.
""",
    "linux": """\
{title}: Linux build made by Porting Toolkit (NW.js {version})

Start ./{exe} (if it does not start: chmod +x {exe}). The game itself is
in package.nw/, and so are its saves. credits.html lists the NW.js
licences.
""",
}
