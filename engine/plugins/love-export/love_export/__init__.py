"""LÖVE export: the game's .love with the official LÖVE build of a platform.

- macOS: ``<Title>.app`` from ``love.app`` with ``game.love`` in
  ``Contents/Resources`` (LÖVE runs a .love it finds there), Info.plist
  renamed to the game, an ad-hoc signature.
- Windows: ``<Title>.exe`` = ``love.exe`` followed by ``game.love`` (a
  "fused" game), next to LÖVE's DLLs and licence.
- Linux: LÖVE's AppImage, ``game.love`` and a ``<Title>`` start script.

The runtime is a pinned LÖVE release from GitHub, checked against its
SHA-256. ``--set love=PATH`` uses another build (an archive, an AppImage or
an unpacked folder), for games made for an older LÖVE.

Other settings: ``bundle_id``, ``icon``, ``sign`` (default true), ``zip``
(default true), ``assets`` (apply/plan/off).
"""

from __future__ import annotations

import hashlib
import plistlib
import re
import shutil
from pathlib import Path

from ptk_engine.sdk import StagedExportPlugin, error
from ptk_engine.formats import love

LOVE_VERSION = "11.5"
RELEASES = "https://github.com/love2d/love/releases/download"
#: SHA-256 of the official 11.5 assets (computed from the release files).
LOVE = {
    "macos": ("love-11.5-macos.zip", "6795bb3a1656af6a2fdfe741e150787b481886d3a280327a261a3fdded586913"),
    "windows": ("love-11.5-win64.zip", "ba6e56be2685e53c817749c4a5007f51137136fe5a3ab64920508babc2e74369"),
    "linux": ("love-11.5-x86_64.AppImage", "65a673406431eff7167a15a032bf7a2e4ba50108e091eb7b176465831f9b5e00"),
}
WINDOWS_SKIP = {"love.exe", "lovec.exe", "readme.txt", "changes.txt", "game.ico", "love.ico"}


def safe_name(title: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .")[:64] or "Game"


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


class Plugin(StagedExportPlugin):
    def check(self, ctx):
        target = ctx.target or {}
        if target.get("os") not in LOVE:
            return [error(f"love-export builds for macOS, Windows and Linux, not {target.get('id')}")]
        if not ctx.recipe("love"):
            try:
                love.detect(ctx.game)
            except love.LoveError as e:
                return [error(str(e))]
        return []

    def stage(self, ctx):
        target = ctx.target or {}
        os_ = target.get("os")
        if os_ not in LOVE:
            ctx.fail(f"love-export builds for macOS, Windows and Linux, not {target.get('id')}")
        info = ctx.recipe("love")
        if not info:
            try:
                info = love.detect(ctx.game).to_dict()
            except love.LoveError as e:
                ctx.fail(str(e))
        ctx.progress(0.05, "Unpacking the game")
        game_dir = love.materialize(ctx.game, info, ctx.temp / "game")
        assets = ctx.process_assets(game_dir, "love")
        if ctx.assets_mode == "plan":
            return {"summary": "Estimated the advanced options (nothing was built)", "assets": assets}
        title = ctx.title if ctx.title not in ("", "Game") else (info.get("title") or "Game")
        game_love = love.write_love(game_dir, ctx.temp / "game.love")
        runtime = self.runtime(ctx, os_)
        if os_ == "macos":
            out = self.macos(ctx, runtime, game_love, game_dir, info, title)
        elif os_ == "windows":
            out = self.windows(ctx, runtime, game_love, title)
        else:
            out = self.linux(ctx, runtime, game_love, game_dir, info, title)
        if assets:
            out["assets"] = assets
        v = info.get("love_version")
        if v and not str(v).startswith("11.") and not ctx.setting("love"):
            ctx.warn(f"The game was made for LÖVE {v} and is packaged with LÖVE {LOVE_VERSION}; "
                     "if it does not start, pass --set love=PATH with a LÖVE build of its version.")
        ctx.progress(1.0, "Done")
        return out

    # -- the runtime -------------------------------------------------------------

    def runtime(self, ctx, os_: str) -> Path:
        """The LÖVE build: an unpacked folder (macOS, Windows) or the AppImage."""
        custom = ctx.setting("love")
        if custom:
            src = Path(custom).expanduser()
            if src.is_dir():
                return src
            if not src.is_file():
                ctx.fail(f"love={custom} is not a folder or a file")
            archive = src
            key = hashlib.sha256(f"{src.resolve()}|{src.stat().st_size}|{src.stat().st_mtime_ns}".encode()).hexdigest()[:16]
        else:
            name, sha = LOVE[os_]
            ctx.progress(0.2, f"Downloading LÖVE {LOVE_VERSION} for {os_.title()} (once)")
            archive = ctx.download(f"{RELEASES}/{LOVE_VERSION}/{name}", sha256=sha)
            key = f"{LOVE_VERSION}-{os_}"
        if os_ == "linux":
            return archive
        dest = ctx.cache / "runtime" / key
        if not (dest / ".complete").is_file():
            shutil.rmtree(dest, ignore_errors=True)
            dest.mkdir(parents=True)
            try:
                love.extract_runtime(archive, dest)
            except (love.LoveError, OSError, ValueError) as e:
                ctx.fail(f"could not unpack {archive.name}: {e}")
            (dest / ".complete").write_text(archive.name + "\n")
        return dest

    # -- macOS -----------------------------------------------------------------------

    def macos(self, ctx, runtime: Path, game_love: Path, game_dir: Path, info: dict, title: str) -> dict:
        source = runtime if runtime.suffix == ".app" else next(
            (p for p in runtime.rglob("*.app") if (p / "Contents" / "Info.plist").is_file()), None)
        if source is None:
            ctx.fail(f"{runtime} has no LÖVE app")
        name = safe_name(title)
        app = ctx.clean_dir(ctx.output / f"{name}.app")
        ctx.progress(0.5, "Building the app")
        shutil.copytree(source, app, symlinks=True, dirs_exist_ok=True)
        resources = app / "Contents" / "Resources"
        for old in resources.glob("*.love"):
            old.unlink()
        shutil.copyfile(game_love, resources / "game.love")
        plist_path = app / "Contents" / "Info.plist"
        plist = plistlib.loads(plist_path.read_bytes())
        bundle = ctx.setting("bundle_id") or f"org.portingtoolkit.{slug(title)}"
        plist.update({"CFBundleIdentifier": bundle, "CFBundleName": title[:15], "CFBundleDisplayName": title,
                      "CFBundleShortVersionString": str(ctx.manifest.get("version") or "1.0.0")})
        # A fused game must not claim the .love file type (LÖVE's distribution notes).
        plist.pop("UTExportedTypeDeclarations", None)
        plist.pop("CFBundleDocumentTypes", None)
        if self.icon(ctx, game_dir, info, resources):
            plist["CFBundleIconFile"] = "icon"
            plist.pop("CFBundleIconName", None)  # otherwise macOS takes LÖVE's icon from Assets.car
        plist_path.write_bytes(plistlib.dumps(plist))
        signed = False
        if ctx.flag("sign", True) and ctx.which("codesign"):
            ctx.progress(0.9, "Signing (ad-hoc)")
            ctx.run(["codesign", "--force", "--deep", "--sign", "-", "--timestamp=none", app])
            signed = True
        return {
            "outputs": [str(app)],
            "open": str(app),
            "summary": f"Built {app.name} with LÖVE {LOVE_VERSION}",
            "notes": [
                "The app is universal (Apple Silicon and Intel).",
                "Signed ad-hoc: it runs on this Mac. To share it, sign and notarize it with an Apple Developer ID."
                if signed else "Not signed.",
                "Saves go to ~/Library/Application Support/LOVE/<identity>.",
            ],
        }

    def icon(self, ctx, game_dir: Path, info: dict, resources: Path) -> bool:
        chosen = ctx.setting("icon")
        src = Path(chosen).expanduser() if chosen else ctx.art("icon")
        if src is None and info.get("icon") and (game_dir / info["icon"]).is_file():
            src = game_dir / info["icon"]
        if src is None or not src.is_file() or src.suffix.lower() not in (".png", ".jpg", ".jpeg"):
            return False
        if not (ctx.which("sips") and ctx.which("iconutil")):
            return False
        iconset = ctx.temp / "icon.iconset"
        shutil.rmtree(iconset, ignore_errors=True)
        iconset.mkdir(parents=True)
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                px = size * scale
                out = iconset / f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
                ctx.run(["sips", "-s", "format", "png", "-z", str(px), str(px), src, "--out", out])
        ctx.run(["iconutil", "-c", "icns", iconset, "-o", resources / "icon.icns"])
        return True

    # -- Windows ---------------------------------------------------------------------

    def windows(self, ctx, runtime: Path, game_love: Path, title: str) -> dict:
        exe_src = next((p for p in runtime.rglob("love.exe")), None)
        if exe_src is None:
            ctx.fail(f"{runtime} has no love.exe")
        name = safe_name(title)
        ctx.clean_dir(ctx.output)
        folder = ctx.output / name
        folder.mkdir()
        ctx.progress(0.6, "Fusing the game into the .exe")
        exe = folder / f"{name}.exe"
        with open(exe, "wb") as out:
            for part in (exe_src, game_love):
                with open(part, "rb") as f:
                    shutil.copyfileobj(f, out, 1 << 20)
        for p in sorted(exe_src.parent.iterdir()):
            if p.is_file() and p.name.lower() not in WINDOWS_SKIP:
                shutil.copy2(p, folder / p.name)
        ctx.write_text(folder / "README.txt", README["windows"].format(title=title, exe=exe.name, v=LOVE_VERSION))
        return self.package(ctx, folder, {exe.name}, [
            "The .exe keeps LÖVE's icon and is not signed, so Windows SmartScreen may ask before the first "
            "start (More info, then Run anyway).",
            "Saves go to %APPDATA%\\<identity> (the identity in conf.lua).",
        ])

    # -- Linux -------------------------------------------------------------------------

    def linux(self, ctx, appimage: Path, game_love: Path, game_dir: Path, info: dict, title: str) -> dict:
        if appimage.is_dir():
            found = next(iter(sorted(appimage.glob("*.AppImage"))), None)
            if found is None:
                ctx.fail(f"{appimage} has no LÖVE AppImage")
            appimage = found
        name = safe_name(title)
        ctx.clean_dir(ctx.output)
        folder = ctx.output / name
        folder.mkdir()
        runtime = folder / appimage.name
        shutil.copyfile(appimage, runtime)
        runtime.chmod(0o755)
        shutil.copyfile(game_love, folder / "game.love")
        script = folder / name.replace(" ", "-")
        ctx.write_text(script, START_SCRIPT.format(appimage=appimage.name))
        script.chmod(0o755)
        icon = info.get("icon")
        if icon and (game_dir / icon).is_file() and icon.lower().endswith(".png"):
            shutil.copyfile(game_dir / icon, folder / "icon.png")
        ctx.write_text(folder / "README.txt", README["linux"].format(title=title, exe=script.name, v=LOVE_VERSION))
        return self.package(ctx, folder, {script.name, runtime.name}, [
            f"Start it with ./{script.name}. LÖVE's AppImage needs FUSE 2 (most desktops and the Steam Deck "
            "have it; otherwise install libfuse2).",
        ])

    def package(self, ctx, folder: Path, executables: set[str], notes: list[str]) -> dict:
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            ctx.progress(0.9, "Zipping")
            outputs.append(str(ctx.zip_dir(folder, ctx.output / f"{folder.name}-{ctx.target.get('id')}.zip",
                                           executables=executables)))
        return {
            "outputs": outputs,
            "open": str(folder),
            "summary": f"Built {folder.name}/ for {ctx.target.get('name')} with LÖVE {LOVE_VERSION}",
            "notes": notes,
        }


START_SCRIPT = """#!/bin/sh
# Starts the game with the LÖVE AppImage next to this script.
cd "$(dirname "$0")" || exit 1
exec "./{appimage}" --fused game.love "$@"
"""

README = {
    "windows": """\
{title}: Windows build made by Porting Toolkit (LÖVE {v})

Start {exe}. Keep the .dll files next to it. Windows may warn about an
unknown publisher the first time: choose "More info", then "Run anyway".
license.txt lists LÖVE's licences.
""",
    "linux": """\
{title}: Linux build made by Porting Toolkit (LÖVE {v})

Start ./{exe}. It runs the LÖVE AppImage with game.love; keep the three
files together. If nothing happens, install FUSE 2 (libfuse2).
""",
}
