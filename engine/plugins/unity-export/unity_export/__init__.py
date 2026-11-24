"""Unity export (Mono, experimental): the game's Data with another platform's player.

The player comes from the user's own Unity installation of the game's exact
version (Unity Hub installs it, under Unity's terms, which the user accepts
there): ``PlaybackEngines/<Platform>StandaloneSupport/Variations/<release
mono player>``. Layouts, as Unity 2022.3 installs them:

- Windows: ``WindowsPlayer.exe`` (renamed ``<Name>.exe``), ``UnityPlayer.dll``,
  ``UnityCrashHandler64.exe``, ``MonoBleedingEdge/``; the game in ``<Name>_Data``.
- Linux: ``LinuxPlayer`` (``<Name>.x86_64``), ``UnityPlayer.so``; the game in
  ``<Name>_Data``, with ``MonoBleedingEdge`` inside it.
- macOS: ``UnityPlayer.app`` (``<Name>.app``), ``MonoBleedingEdge`` from
  MacStandaloneSupport; the game in ``Contents/Resources/Data``.

The game's assemblies (``Managed/``) stay; the engine's own
(``UnityEngine*.dll``) and ``unity default resources`` come from the target
player. Shaders cannot be added: a build only has those of its platform's
graphics APIs, which is why this is experimental.

Settings: ``unity=PATH`` (a Unity install folder), ``bundle_id``, ``sign``,
``zip``.
"""

from __future__ import annotations

import json
import plistlib
import re
import shutil
from pathlib import Path

from ptk_engine.formats import unity

NAMES = {"windows": "Windows", "linux": "Linux", "macos": "Mac"}
#: What the source build's shaders most likely cover, and what the target needs.
APIS = {"windows": "Direct3D 11/12", "linux": "Vulkan or OpenGL", "macos": "Metal"}


def safe_name(title: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .")[:64] or "Game"


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


class Plugin:
    def run(self, ctx):
        os_ = (ctx.target or {}).get("os")
        if os_ not in NAMES:
            ctx.fail(f"unity-export builds for macOS, Windows and Linux, not {(ctx.target or {}).get('id')}")
        info = ctx.recipe("unity")
        if not info:
            try:
                info = unity.detect(ctx.game).to_dict()
            except unity.UnityError as e:
                ctx.fail(str(e))
        if info.get("backend") != "mono":
            ctx.fail("this is an IL2CPP build: its code is native to its platform, so it cannot be moved "
                     "without the source")
        version = info.get("version")
        if not version:
            ctx.fail("the Unity version was not found (globalgamemanagers)")
        game_root = ctx.game if ctx.game.is_dir() else ctx.game.parent
        data = game_root / info["data"]
        if not data.is_dir():
            ctx.fail(f"the game's Data folder is missing: {data}")
        support, variation = self.player(ctx, version, os_)
        title = ctx.title if ctx.title not in ("", "Game") else (info.get("product") or info.get("name") or "Game")
        name = safe_name(title)
        ctx.progress(0.2, "Copying the game's data")
        if os_ == "macos":
            out = self.macos(ctx, support, variation, data, name, title, info)
        else:
            out = self.pc(ctx, variation, data, name, os_)
        self.warnings(ctx, info, os_)
        ctx.progress(1.0, "Done")
        out["notes"].insert(0, f"Unity {version} {NAMES[os_]} player (Mono) from {support.parent.parent if os_ != 'macos' else support}.")
        return out

    # -- the player ------------------------------------------------------------------

    def player(self, ctx, version: str, os_: str) -> tuple[Path, Path]:
        extra = [ctx.setting("unity")] if ctx.setting("unity") else []
        checked = []
        for root in unity.editor_roots(version, extra):
            if not root.is_dir():
                continue
            checked.append(str(root))
            support = unity.support_dir(root, os_)
            if support:
                variation = unity.player_variation(support, os_)
                if variation:
                    return support, variation
        module = {"windows": "Windows Build Support (Mono)", "linux": "Linux Build Support (Mono)",
                  "macos": "Mac Build Support (Mono)"}[os_]
        link = self.hub_link(ctx, version)
        where = f" (found Unity in {', '.join(checked)}, without it)" if checked else ""
        ctx.fail(f"Unity {version} with \"{module}\" is needed{where}. Install it in Unity Hub"
                 + (f": {link}" if link else f" (Installs > Install Editor > Archive > {version})")
                 + ", or pass --set unity=PATH to a Unity install folder.")

    @staticmethod
    def hub_link(ctx, version: str) -> str | None:
        """unityhub://<version>/<changeset>, from Unity's public release service
        (``unity_lookup=false`` skips it)."""
        if not ctx.flag("unity_lookup", True):
            return None
        try:
            r = ctx.download(f"https://services.api.unity.com/unity/editor/release/v1/releases?version={version}&limit=1",
                             dest=ctx.cache / "releases" / f"{version}.json")
            rel = json.loads(r.read_text(encoding="utf-8"))["results"][0]
            return f"unityhub://{version}/{rel['shortRevision']}"
        except Exception:  # noqa: BLE001 - the link is a convenience
            return None

    # -- Windows and Linux --------------------------------------------------------------

    def pc(self, ctx, variation: Path, data: Path, name: str, os_: str) -> dict:
        ctx.clean_dir(ctx.output)
        folder = ctx.output / name
        folder.mkdir()
        data_out = folder / f"{name}_Data"
        self.copy_data(ctx, data, data_out, variation / "Data", os_)
        if os_ == "windows":
            exe = folder / f"{name}.exe"
            shutil.copy2(variation / "WindowsPlayer.exe", exe)
            for p in sorted(variation.iterdir()):
                if p.name in ("WindowsPlayer.exe", "Data") or p.suffix == ".pdb":
                    continue
                if p.is_dir():
                    shutil.copytree(p, folder / p.name)
                else:
                    shutil.copy2(p, folder / p.name)
            start = exe.name
        else:
            exe = folder / f"{name}.x86_64"
            shutil.copy2(variation / "LinuxPlayer", exe)
            exe.chmod(0o755)
            shutil.copy2(variation / "UnityPlayer.so", folder / "UnityPlayer.so")
            mono = variation / "Data" / "MonoBleedingEdge"
            if mono.is_dir():
                shutil.copytree(mono, data_out / "MonoBleedingEdge", dirs_exist_ok=True)
            start = f"./{exe.name}"
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            ctx.progress(0.9, "Zipping")
            outputs.append(str(ctx.zip_dir(folder, ctx.output / f"{name}-{ctx.target.get('id')}.zip",
                                           executables={exe.name})))
        notes = [f"Start it with {start}."]
        if os_ == "windows":
            notes.append("The .exe keeps Unity's icon and is not signed, so Windows SmartScreen may ask once.")
        return {"outputs": outputs, "open": str(folder), "notes": notes,
                "summary": f"Built {folder.name}/ for {ctx.target.get('name')} - experimental"}

    def copy_data(self, ctx, src: Path, dst: Path, player_data: Path, os_: str) -> None:
        """The game's Data without the source platform's native plugins, with
        the target player's engine assemblies and default resources."""
        def skip(p: Path) -> bool:
            rel = p.relative_to(src).as_posix()
            return rel == "Plugins" or rel == "MonoBleedingEdge" or p.name.startswith("._") or p.name == ".DS_Store"

        ctx.copy_tree(src, dst, skip=skip)
        managed = player_data / "Managed"
        if managed.is_dir():
            replaced = 0
            for dll in managed.glob("UnityEngine*.dll"):
                shutil.copy2(dll, dst / "Managed" / dll.name)
                replaced += 1
            ctx.log(f"Replaced {replaced} Unity engine assemblies with the {NAMES[os_]} player's.")
        res = player_data / "Resources"
        if res.is_dir():
            shutil.copytree(res, dst / "Resources", dirs_exist_ok=True)

    # -- macOS ----------------------------------------------------------------------------

    def macos(self, ctx, support: Path, variation: Path, data: Path, name: str, title: str, info: dict) -> dict:
        player_app = next(iter(sorted(variation.glob("*.app"))), None)
        if player_app is None:
            ctx.fail(f"{variation} has no UnityPlayer.app")
        app = ctx.clean_dir(ctx.output / f"{name}.app")
        shutil.copytree(player_app, app, symlinks=True, dirs_exist_ok=True)
        contents = app / "Contents"
        exe = contents / "MacOS" / name
        (contents / "MacOS" / "UnityPlayer").rename(exe)
        mono = support / "MonoBleedingEdge"
        if mono.is_dir():
            shutil.copytree(mono, contents / "MonoBleedingEdge", dirs_exist_ok=True)
        engine_data = support / "Variations" / "mono"
        self.copy_data(ctx, data, contents / "Resources" / "Data", engine_data, "macos")
        if (contents / "Resources" / "unity default resources").exists() and not \
                (contents / "Resources" / "Data" / "Resources" / "unity default resources").exists():
            (contents / "Resources" / "Data" / "Resources").mkdir(parents=True, exist_ok=True)
            shutil.copy2(contents / "Resources" / "unity default resources",
                         contents / "Resources" / "Data" / "Resources" / "unity default resources")
        version = str(ctx.manifest.get("version") or "1.0.0")
        plist = {
            "CFBundleDevelopmentRegion": "English",
            "CFBundleExecutable": name,
            "CFBundleIdentifier": ctx.setting("bundle_id") or f"org.portingtoolkit.{slug(title)}",
            "CFBundleInfoDictionaryVersion": "6.0",
            "CFBundleName": title[:15],
            "CFBundleDisplayName": title,
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": version,
            "CFBundleVersion": version,
            "LSApplicationCategoryType": "public.app-category.games",
            "LSMinimumSystemVersion": "10.13",
            "NSHighResolutionCapable": True,
            "NSPrincipalClass": "NSApplication",
        }
        (contents / "Info.plist").write_bytes(plistlib.dumps(plist))
        (contents / "PkgInfo").write_text("APPL????")
        signed = False
        if ctx.flag("sign", True) and ctx.which("codesign"):
            ctx.run(["codesign", "--force", "--deep", "--sign", "-", "--timestamp=none", app])
            signed = True
        return {"outputs": [str(app)], "open": str(app),
                "summary": f"Built {app.name} - experimental",
                "notes": ["Signed ad-hoc: it runs on this Mac." if signed else "Not signed."]}

    # -- what to expect -----------------------------------------------------------------------

    @staticmethod
    def warnings(ctx, info: dict, os_: str) -> None:
        src = info.get("platform")
        if src != os_:
            ctx.warn(f"Unity builds only carry shaders for their own platform's graphics APIs. This {NAMES[src]} "
                     f"build must also have {APIS[os_]} shaders, or objects will show pink or invisible on "
                     f"{NAMES[os_]}. Test it before sharing.")
        plugins = [p for p in info.get("plugins") or [] if p not in (info.get("apple_services") or [])]
        if plugins:
            ctx.warn(f"Native plugins not carried over (built for {NAMES[src]}): {', '.join(plugins)}. Features "
                     "that use them (sound engines, controllers, video...) may fail.")
        if info.get("apple_services"):
            ctx.warn("Apple services (" + ", ".join(info["apple_services"]) + ") do not exist on other platforms.")
        if info.get("app_store"):
            ctx.warn("This is an App Store / Apple Arcade build; its terms limit it to Apple devices.")
