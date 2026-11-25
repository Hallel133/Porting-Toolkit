"""RPG Maker XP/VX/VX Ace export.

- Windows: the game as it is (its own Game.exe runs RGSS), without the
  player's saves, plus a zip.
- macOS: mkxp-z's app renamed to the game, with the game in
  ``Contents/Game`` (where mkxp-z looks for it) and an ``mkxp.json``;
  ad-hoc signed.
- Linux: mkxp-z's folder with the game next to its executable and an
  ``mkxp.json``, plus a zip.

mkxp-z (https://github.com/mkxp-z/mkxp-z, GPL-2.0) publishes no release
builds, only CI builds that need a GitHub sign-in, so there is nothing to
pin: download it once and pass ``--set mkxp=PATH`` (the Mac app, or the
Linux folder); the path is remembered on this computer. Its licence and a
link to its source go into the build.

Settings: ``mkxp``, ``rtp`` (a folder with the RPG Maker RTP the game
needs; copied into the build), ``bundle_id``, ``sign``, ``zip``, ``assets``.
"""

from __future__ import annotations

import json
import os
import plistlib
import re
import shutil
from pathlib import Path

from ptk_engine.sdk import StagedExportPlugin, error
from ptk_engine.formats import rgss

SOURCE = "https://github.com/mkxp-z/mkxp-z"
MAC_GUESSES = ("/Applications/Z-universal.app", "~/Applications/Z-universal.app", "/Applications/mkxp-z.app",
               "~/Applications/mkxp-z.app")
GPL_NOTE = """mkxp-z: an open-source RGSS player
https://github.com/mkxp-z/mkxp-z

This build includes mkxp-z, which is free software under the GNU General
Public License, version 2 (GPL-2.0). Its complete source code is available
at the address above; the version used is the one named in the mkxp-z
build that was packaged. You may redistribute and change mkxp-z under the
terms of the GPL-2.0: https://www.gnu.org/licenses/old-licenses/gpl-2.0.html

The game itself is not covered by this licence.
"""


def safe_name(title: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .")[:64] or "Game"


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


class Plugin(StagedExportPlugin):
    def check(self, ctx):
        target = ctx.target or {}
        if target.get("os") not in ("macos", "windows", "linux"):
            return [error(f"rgss-export builds for macOS, Windows and Linux, not {target.get('id')}")]
        if not ctx.recipe("rgss"):
            try:
                rgss.detect(ctx.game)
            except rgss.RgssError as e:
                return [error(str(e))]
        return []

    def stage(self, ctx):
        os_ = (ctx.target or {}).get("os")
        if os_ not in ("macos", "windows", "linux"):
            ctx.fail(f"rgss-export builds for macOS, Windows and Linux, not {(ctx.target or {}).get('id')}")
        info = ctx.recipe("rgss")
        if not info:
            try:
                info = rgss.detect(ctx.game).to_dict()
            except rgss.RgssError as e:
                ctx.fail(str(e))
        root = ctx.game / info["root"] if info.get("root") else ctx.game
        title = ctx.title if ctx.title not in ("", "Game") else (info.get("title") or "Game")
        ctx.progress(0.1, "Copying the game")
        stage = ctx.temp / "game"
        ctx.copy_tree(root, stage, skip=lambda p: rgss.is_save(p.name) or p.name.startswith("._")
                      or p.name in (".DS_Store", "Thumbs.db"))
        assets = ctx.process_assets(stage, "rgss")
        if ctx.assets_mode == "plan":
            return {"summary": "Estimated the advanced options (nothing was built)", "assets": assets}
        if os_ == "windows":
            out = self.windows(ctx, stage, title)
        else:
            runtime = self.mkxp(ctx, os_)
            rtp = self.rtp(ctx, info, stage)
            config = {"rgssVersion": int(info.get("rgss") or 0), "windowTitle": title}
            if rtp:
                config["RTP"] = rtp
            if os_ == "macos":
                out = self.macos(ctx, runtime, stage, title, config)
            else:
                out = self.linux(ctx, runtime, stage, title, config)
        if info.get("archive"):
            out["notes"].append(f"{info['archive']['path']} was copied unchanged; the advanced options only apply "
                                "to files outside the archive.")
        if assets:
            out["assets"] = assets
        ctx.progress(1.0, "Done")
        return out

    # -- runtime and RTP --------------------------------------------------------------

    def mkxp(self, ctx, os_: str) -> Path:
        remembered = ctx.cache / f"mkxp-{os_}.txt"
        chosen = ctx.setting("mkxp")
        candidates = [chosen] if chosen else []
        if remembered.is_file():
            candidates.append(remembered.read_text(encoding="utf-8").strip())
        if os_ == "macos":
            candidates += list(MAC_GUESSES)
        for c in candidates:
            p = Path(c).expanduser()
            found = self._find_runtime(p, os_)
            if found:
                if chosen:
                    ctx.write_text(remembered, str(found) + "\n")
                return found
        where = "the mkxp-z app (Z-universal.app)" if os_ == "macos" else "the mkxp-z Linux folder"
        msg = (f"mkxp-z is needed to run RPG Maker XP/VX/VX Ace games on {'macOS' if os_ == 'macos' else 'Linux'}. "
               f"Download {where} from {SOURCE} (Actions, latest successful build; a GitHub sign-in is needed), "
               "unzip it, and export with --set mkxp=PATH. The path is remembered.")
        if chosen:
            msg = f"{chosen} is not {where}. " + msg
        ctx.fail(msg)

    @staticmethod
    def _find_runtime(p: Path, os_: str) -> Path | None:
        if not p.exists():
            return None
        if os_ == "macos":
            if p.suffix == ".app" and (p / "Contents" / "Info.plist").is_file():
                return p
            return next((a for a in sorted(p.glob("*.app")) if (a / "Contents" / "Info.plist").is_file()), None) \
                if p.is_dir() else None
        if p.is_dir():
            exe = Plugin._linux_exe(p)
            if exe:
                return p
            sub = [d for d in sorted(p.iterdir()) if d.is_dir() and Plugin._linux_exe(d)]
            return sub[0] if sub else None
        return None

    @staticmethod
    def _linux_exe(folder: Path) -> Path | None:
        for p in sorted(folder.iterdir()):
            if p.is_file() and p.name.lower().startswith("mkxp-z") and p.suffix not in (".json", ".txt", ".md") \
                    and os.access(p, os.X_OK):
                return p
        return None

    def rtp(self, ctx, info: dict, stage: Path) -> list[str]:
        """Copies --set rtp=PATH into the build; returns mkxp.json's RTP list."""
        chosen = ctx.setting("rtp")
        if not chosen:
            if info.get("rtp"):
                ctx.warn("The game uses the RPG Maker RTP (" + ", ".join(info["rtp"]) + "), which is not included: "
                         "graphics or sounds may be missing. Pass --set rtp=PATH with the RTP folder.")
            return []
        src = Path(chosen).expanduser()
        if not src.is_dir():
            ctx.fail(f"rtp={chosen} is not a folder")
        ctx.copy_tree(src, stage / "RTP")
        return ["RTP"]

    # -- Windows ---------------------------------------------------------------------------

    def windows(self, ctx, stage: Path, title: str) -> dict:
        name = safe_name(title)
        ctx.clean_dir(ctx.output)
        folder = ctx.output / name
        shutil.copytree(stage, folder)
        return self.package(ctx, folder, set(), [
            "The original Game.exe runs the game on Windows; nothing was changed except the advanced options.",
        ])

    # -- macOS -------------------------------------------------------------------------------

    def macos(self, ctx, runtime: Path, stage: Path, title: str, config: dict) -> dict:
        name = safe_name(title)
        app = ctx.clean_dir(ctx.output / f"{name}.app")
        ctx.progress(0.5, "Building the app")
        shutil.copytree(runtime, app, symlinks=True, dirs_exist_ok=True,
                        ignore=lambda d, names: ["Game"] if Path(d) == runtime / "Contents" else [])
        game_dir = app / "Contents" / "Game"
        shutil.copytree(stage, game_dir)
        (game_dir / "mkxp.json").write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        (app / "Contents" / "Resources").mkdir(parents=True, exist_ok=True)
        (app / "Contents" / "Resources" / "mkxp-z-LICENSE.txt").write_text(GPL_NOTE, encoding="utf-8")
        plist_path = app / "Contents" / "Info.plist"
        plist = plistlib.loads(plist_path.read_bytes())
        plist.update({"CFBundleIdentifier": ctx.setting("bundle_id") or f"org.portingtoolkit.{slug(title)}",
                      "CFBundleName": title[:15], "CFBundleDisplayName": title})
        plist_path.write_bytes(plistlib.dumps(plist))
        signed = False
        if ctx.flag("sign", True) and ctx.which("codesign"):
            ctx.progress(0.9, "Signing (ad-hoc)")
            ctx.run(["codesign", "--force", "--deep", "--sign", "-", "--timestamp=none", app])
            signed = True
        return {
            "outputs": [str(app)],
            "open": str(app),
            "summary": f"Built {app.name} with mkxp-z",
            "notes": [
                "mkxp-z refuses to start from the Downloads folder; move the app elsewhere first.",
                "Signed ad-hoc: it runs on this Mac. To share it, sign and notarize it with an Apple Developer ID."
                if signed else "Not signed.",
                "mkxp-z is GPL-2.0: when you share this app, point people to its source (see "
                "Contents/Resources/mkxp-z-LICENSE.txt).",
            ],
        }

    # -- Linux ---------------------------------------------------------------------------------

    def linux(self, ctx, runtime: Path, stage: Path, title: str, config: dict) -> dict:
        name = safe_name(title)
        ctx.clean_dir(ctx.output)
        folder = ctx.output / name
        shutil.copytree(runtime, folder, symlinks=True)
        for p in sorted(stage.iterdir()):
            dest = folder / p.name
            if p.is_dir():
                shutil.copytree(p, dest, dirs_exist_ok=True)
            else:
                shutil.copy2(p, dest)
        (folder / "mkxp.json").write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        (folder / "mkxp-z-LICENSE.txt").write_text(GPL_NOTE, encoding="utf-8")
        exe = self._linux_exe(folder)
        return self.package(ctx, folder, {exe.name} if exe else set(), [
            f"Start it with ./{exe.name if exe else 'mkxp-z'}.",
            "mkxp-z is GPL-2.0: when you share this build, point people to its source (mkxp-z-LICENSE.txt).",
        ])

    def package(self, ctx, folder: Path, executables: set[str], notes: list[str]) -> dict:
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            ctx.progress(0.9, "Zipping")
            outputs.append(str(ctx.zip_dir(folder, ctx.output / f"{folder.name}-{ctx.target.get('id')}.zip",
                                           executables=executables)))
        return {"outputs": outputs, "open": str(folder), "notes": notes,
                "summary": f"Built {folder.name}/ for {ctx.target.get('name')}"}
