"""Unity export (Mono, experimental): the game's Data with another platform's player.

The player is Unity's own, of the game's exact version:
``PlaybackEngines/<Platform>StandaloneSupport/Variations/<release mono
player>``. It comes from a Unity installation when there is one (Unity Hub,
or ``unity=PATH``); otherwise, once the user accepts Unity's terms
(``accept_unity_terms=true``, remembered), from Unity's "Build Support"
module for that version on download.unity3d.com: the URL and MD5 come from
Unity's release service, and only the player is taken out of the module's
.pkg (``ptk_engine.formats.xar``), without installing anything. Layouts, as
Unity 2022.3 installs them:

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

Settings: ``unity=PATH`` (a Unity install folder), ``accept_unity_terms``,
``unity_download=false`` (never download), ``bundle_id``, ``sign``, ``zip``.
"""

from __future__ import annotations

import base64
import hashlib
import json
import plistlib
import re
import shutil
from pathlib import Path, PurePosixPath

from ptk_engine.build import codesign_if_changed
from ptk_engine.formats import unity, xar
from ptk_engine.sdk import ExportPlugin, error, warning

NAMES = {"windows": "Windows", "linux": "Linux", "macos": "Mac"}
#: What the source build's shaders most likely cover, and what the target needs.
APIS = {"windows": "Direct3D 11/12", "linux": "Vulkan or OpenGL", "macos": "Metal"}
#: Unity's Build Support module per target, and the editor host whose
#: release entry lists it as a .pkg (any computer can read a .pkg here).
MODULES = {"windows": ("windows-mono", "MAC_OS"), "linux": ("linux-mono", "MAC_OS"),
           "macos": ("mac-mono", "LINUX")}
SUPPORT = {"windows": "WindowsStandaloneSupport", "linux": "LinuxStandaloneSupport",
           "macos": "MacStandaloneSupport"}
PLAYER_PREFIXES = {"windows": ("win64",), "linux": ("linux64",),
                   "macos": ("macos_x64arm64", "macosx64arm64", "macos_arm64", "macos_x64", "macosx64")}
TERMS_URL = "https://unity.com/legal/terms-of-service"


def safe_name(title: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .")[:64] or "Game"


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


class Plugin(ExportPlugin):
    # -- check: everything that would stop or limit the export ---------------------------

    def check(self, ctx):
        os_ = (ctx.target or {}).get("os")
        if os_ not in NAMES:
            return [error(f"unity-export builds for macOS, Windows and Linux, not {(ctx.target or {}).get('id')}")]
        info = ctx.recipe("unity")
        if not info or "shaders" not in info:  # recipes from before the shader check
            try:
                info = unity.detect(ctx.game).to_dict()
            except unity.UnityError as e:
                return [error(str(e))]
        self.os, self.info = os_, info
        if info.get("backend") != "mono":
            return [error("this is an IL2CPP build: its code is native to its platform, so it cannot be moved "
                          "without the source")]
        self.version = info.get("version")
        if not self.version:
            return [error("the Unity version was not found (globalgamemanagers)")]
        game_root = ctx.game if ctx.game.is_dir() else ctx.game.parent
        self.game_root = game_root
        self.data = game_root / info["data"]
        if not self.data.is_dir():
            return [error(f"the game's Data folder is missing: {self.data}")]
        # The same platform: the game's own player, plugins and all, is reused.
        self.same = info.get("platform") == os_
        problems = []
        self.found = None if self.same else self.installed_player(ctx, self.version, os_)
        if not self.same and not self.found:
            if not ctx.flag("unity_download", True):
                problems.append(error(self.no_player_message(ctx, self.version, os_)))
            elif not (self.players_dir(ctx, self.version) / f".{os_}-complete").is_file() \
                    and not self.accepted(ctx):
                problems.append(error(
                    f"Unity's {NAMES[os_]} player for {self.version} is downloaded from Unity once (no Unity "
                    f"install needed). It is covered by the Unity Terms of Service ({TERMS_URL}); to accept "
                    "them, use the button in the app or export with --set accept_unity_terms=true (asked once).",
                    {"setting": "accept_unity_terms", "value": "true", "label": "Accept Unity's terms",
                     "url": TERMS_URL}))
        problems += [warning(w) for w in self.limits(info, os_)]
        return problems

    def prepare(self, ctx):
        if self.same:
            self.support = self.variation = None
            return
        if self.found:
            self.support, self.variation = self.found
            return
        root = self.download_player(ctx, self.version, self.os)
        support = unity.support_dir(root, self.os)
        variation = unity.player_variation(support, self.os) if support else None
        if not (support and variation):
            ctx.fail(f"Unity's {NAMES[self.os]} module for {self.version} has no release Mono player")
        self.support, self.variation = support, variation

    def layout(self, ctx, build):
        info = self.info
        title = ctx.title if ctx.title not in ("", "Game") else (info.get("product") or info.get("name") or "Game")
        self.title, self.name = title, safe_name(title)
        ctx.progress(0.2, "Laying out the build")
        if self.os == "macos" and self.same:
            self.layout_mac_repack(ctx, build)
        elif self.os == "macos":
            self.layout_mac(ctx, build)
        else:
            self.layout_pc(ctx, build)

    def finish(self, ctx, build):
        out = self.finish_mac(ctx, build) if self.os == "macos" else self.finish_pc(ctx, build)
        if self.same:
            source = "the game's own"
        elif self.players_dir(ctx, self.version) in self.support.parents:
            source = "downloaded from Unity"
        else:
            source = f"from {self.support.parent.parent if self.os != 'macos' else self.support}"
        out["notes"].insert(0, f"Unity {self.version} {NAMES[self.os]} player (Mono), {source}.")
        out["notes"].append(self.log_hint())
        return out

    def log_hint(self) -> str:
        company = self.info.get("company") or "<Company>"
        product = self.info.get("product") or "<Product>"
        where = {"macos": f"~/Library/Logs/{company}/{product}/Player.log",
                 "windows": f"%USERPROFILE%\\AppData\\LocalLow\\{company}\\{product}\\Player.log",
                 "linux": f"~/.config/unity3d/{company}/{product}/Player.log"}[self.os]
        return f"If the game shows nothing or has no sound, Unity's log says why: {where}"

    def installed_player(self, ctx, version: str, os_: str) -> tuple[Path, Path] | None:
        extra = [ctx.setting("unity")] if ctx.setting("unity") else []
        extra.append(str(self.players_dir(ctx, version)))
        for root in unity.editor_roots(version, extra):
            if not root.is_dir():
                continue
            support = unity.support_dir(root, os_)
            if support:
                variation = unity.player_variation(support, os_)
                if variation:
                    return support, variation
        return None

    @staticmethod
    def players_dir(ctx, version: str) -> Path:
        return ctx.cache / "players" / version

    # -- downloading the player from Unity -------------------------------------------------

    @staticmethod
    def release(ctx, version: str) -> dict:
        url = ctx.setting("unity_service") or \
            f"https://services.api.unity.com/unity/editor/release/v1/releases?version={version}&limit=1"
        r = ctx.download(url, dest=ctx.cache / "releases" / f"{version}.json")
        try:
            results = json.loads(r.read_text(encoding="utf-8"))["results"]
        except (ValueError, KeyError):
            r.unlink(missing_ok=True)
            ctx.fail("Unity's release service sent an unexpected answer")
        match = [x for x in results if x.get("version") == version]
        if not match:
            r.unlink(missing_ok=True)
            ctx.fail(f"Unity's release service does not know Unity {version}")
        return match[0]

    @staticmethod
    def module(release: dict, os_: str) -> dict:
        mod_id, host = MODULES[os_]
        for d in release.get("downloads") or []:
            if d.get("platform") != host:
                continue
            for m in d.get("modules") or []:
                if m.get("id") == mod_id and str(m.get("url", "")).endswith(".pkg"):
                    return m
        raise LookupError(mod_id)

    def accepted(self, ctx) -> bool:
        marker = ctx.cache / "unity-terms-accepted"
        if ctx.flag("accept_unity_terms", False):
            marker.parent.mkdir(parents=True, exist_ok=True)
            marker.write_text(TERMS_URL + "\n")
            return True
        return marker.is_file()

    def download_player(self, ctx, version: str, os_: str) -> Path:
        root = self.players_dir(ctx, version)
        done = root / f".{os_}-complete"
        if done.is_file():
            return root
        rel = self.release(ctx, version)
        try:
            mod = self.module(rel, os_)
        except LookupError:
            ctx.fail(f"Unity {version} has no {NAMES[os_]} Mono Build Support module to download")
        size = ((mod.get("downloadSize") or {}).get("value") or 0) / 1e6
        if not self.accepted(ctx):
            ctx.fail(f"Unity's {NAMES[os_]} player for {version} is downloaded from Unity "
                     f"({size:.0f} MB, {mod['url'].rsplit('/', 1)[-1]}) and is covered by the Unity Terms "
                     f"of Service ({TERMS_URL}). To accept them and continue, export again with "
                     "accept_unity_terms=true (asked once).")
        ctx.progress(0.05, f"Downloading Unity's {NAMES[os_]} player ({size:.0f} MB, once)")
        pkg = ctx.download(mod["url"])
        try:
            self.check_md5(pkg, str(mod.get("integrity") or ""))
        except ValueError as e:
            pkg.unlink(missing_ok=True)
            ctx.fail(str(e))
        ctx.progress(0.12, "Taking the player out of Unity's module")
        support = root / "PlaybackEngines" / SUPPORT[os_]
        shutil.rmtree(support, ignore_errors=True)

        def want(p: PurePosixPath) -> bool:
            parts = p.parts
            if parts[0] == "MonoBleedingEdge":
                return True
            if parts[0] != "Variations" or len(parts) == 1:
                return False
            v = parts[1].lower()
            return v == "mono" or ("nondevelopment" in v and "mono" in v
                                   and v.startswith(PLAYER_PREFIXES[os_]))

        try:
            n = xar.extract_payload(pkg, support, want)
        except xar.XarError as e:
            shutil.rmtree(support, ignore_errors=True)
            ctx.fail(f"could not read Unity's module {pkg.name}: {e}")
        ctx.log(f"Took {n} files of Unity's {NAMES[os_]} player out of {pkg.name}.")
        pkg.unlink(missing_ok=True)
        done.write_text(mod["url"] + "\n")
        return root

    @staticmethod
    def check_md5(path: Path, integrity: str) -> None:
        """Unity publishes ``md5-<base64>`` for each module."""
        if not integrity.startswith("md5-"):
            raise ValueError(f"Unity published no checksum for {path.name}")
        h = hashlib.md5(usedforsecurity=False)
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        if base64.b64encode(h.digest()).decode() != integrity[4:]:
            raise ValueError(f"{path.name} does not match the checksum Unity published")

    def no_player_message(self, ctx, version: str, os_: str) -> str:
        extra = [ctx.setting("unity")] if ctx.setting("unity") else []
        checked = []
        for root in unity.editor_roots(version, extra):
            if not root.is_dir():
                continue
            checked.append(str(root))
        module = {"windows": "Windows Build Support (Mono)", "linux": "Linux Build Support (Mono)",
                  "macos": "Mac Build Support (Mono)"}[os_]
        link = self.hub_link(ctx, version)
        where = f" (found Unity in {', '.join(checked)}, without it)" if checked else ""
        return (f"Unity {version} with \"{module}\" is needed{where}. Remove unity_download=false to "
                f"download only its player, or install it in Unity Hub"
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

    def add_data(self, build, dest: str, player_data: Path) -> None:
        """The game's Data without the source platform's native plugins, with
        the target player's engine assemblies and default resources."""
        src = self.data

        def skip(p: Path) -> bool:
            rel = p.relative_to(src).as_posix()
            return rel in ("Plugins", "MonoBleedingEdge") or p.name.startswith("._") or p.name == ".DS_Store"

        build.add_tree(src, dest, skip=skip)
        managed = player_data / "Managed"
        if managed.is_dir():
            for dll in sorted(managed.glob("UnityEngine*.dll")):
                build.add_file(dll, f"{dest}/Managed/{dll.name}")
        res = player_data / "Resources"
        if res.is_dir():
            build.add_tree(res, f"{dest}/Resources")

    def layout_pc(self, ctx, build) -> None:
        name, variation = self.name, self.variation
        data_out = f"{name}/{name}_Data"
        self.add_data(build, data_out, variation / "Data")
        if self.os == "windows":
            self.exe = f"{name}.exe"
            build.add_file(variation / "WindowsPlayer.exe", f"{name}/{self.exe}", mode=0o755)
            for p in sorted(variation.iterdir()):
                if p.name in ("WindowsPlayer.exe", "Data") or p.suffix == ".pdb":
                    continue
                if p.is_dir():
                    build.add_tree(p, f"{name}/{p.name}")
                else:
                    build.add_file(p, f"{name}/{p.name}")
            self.start = self.exe
        else:
            self.exe = f"{name}.x86_64"
            build.add_file(variation / "LinuxPlayer", f"{name}/{self.exe}", mode=0o755)
            build.add_file(variation / "UnityPlayer.so", f"{name}/UnityPlayer.so")
            mono = variation / "Data" / "MonoBleedingEdge"
            if mono.is_dir():
                build.add_tree(mono, f"{data_out}/MonoBleedingEdge")
            self.start = f"./{self.exe}"
        if ctx.flag("zip", True):
            build.artifact(self.zip_name(ctx))

    def zip_name(self, ctx) -> str:
        return f"{self.name}-{ctx.target.get('id')}.zip"

    def finish_pc(self, ctx, build) -> dict:
        folder = build.root / self.name
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            ctx.progress(0.9, "Zipping")
            outputs.append(str(self.zip(ctx, build, self.name, self.zip_name(ctx), {self.exe})))
        notes = [f"Start it with {self.start}."]
        if self.os == "windows":
            notes.append("The .exe keeps Unity's icon and is not signed, so Windows SmartScreen may ask once.")
        return {"outputs": outputs, "open": str(folder), "notes": notes,
                "summary": f"Built {folder.name}/ for {ctx.target.get('name')} - experimental"}

    # -- macOS ----------------------------------------------------------------------------

    def layout_mac(self, ctx, build) -> None:
        """Another platform's game in Unity's own macOS player."""
        player_app = next(iter(sorted(self.variation.glob("*.app"))), None)
        if player_app is None:
            ctx.fail(f"{self.variation} has no UnityPlayer.app")
        app = f"{self.name}.app"
        self.app = app
        build.add_tree(player_app, app, links=True, skip=lambda p: p.name == "_CodeSignature")
        build.remove(f"{app}/Contents/MacOS/UnityPlayer")
        build.add_file(player_app / "Contents" / "MacOS" / "UnityPlayer", f"{app}/Contents/MacOS/{self.name}",
                       mode=0o755)
        mono = self.support / "MonoBleedingEdge"
        if mono.is_dir():
            build.add_tree(mono, f"{app}/Contents/MonoBleedingEdge")
        data = f"{app}/Contents/Resources/Data"
        self.add_data(build, data, self.support / "Variations" / "mono")
        resources = player_app / "Contents" / "Resources" / "unity default resources"
        if resources.is_file() and f"{data}/Resources/unity default resources" not in build.entries:
            build.add_file(resources, f"{data}/Resources/unity default resources")
        build.add_bytes(f"{app}/Contents/Info.plist", plistlib.dumps(self.plist(ctx, {
            "CFBundleExecutable": self.name, "CFBundleDevelopmentRegion": "English",
            "LSMinimumSystemVersion": "10.13", "NSHighResolutionCapable": True,
            "NSPrincipalClass": "NSApplication"})))
        build.add_text(f"{app}/Contents/PkgInfo", "APPL????")

    def layout_mac_repack(self, ctx, build) -> None:
        """A Mac game for macOS again: the original app as it is (player,
        frameworks, native plugins such as sound engines), with a new
        identity and without the App Store's receipt and signature."""
        src_app = self.game_root / self.info["app"] if self.info.get("app") else self.game_root
        app = f"{self.name}.app"
        self.app = app
        drop = {"_CodeSignature", "_MASReceipt", "embedded.provisionprofile"}
        build.add_tree(src_app, app, links=True, skip=lambda p: p.name in drop or p.name.startswith("._"))
        with open(src_app / "Contents" / "Info.plist", "rb") as f:
            original = plistlib.load(f)
        build.add_bytes(f"{app}/Contents/Info.plist", plistlib.dumps(self.plist(ctx, original)))

    def plist(self, ctx, base: dict) -> dict:
        version = str(ctx.manifest.get("version") or base.get("CFBundleShortVersionString") or "1.0.0")
        out = dict(base)
        out.update({
            "CFBundleIdentifier": ctx.setting("bundle_id") or f"org.portingtoolkit.{slug(self.title)}",
            "CFBundleName": self.title[:15],
            "CFBundleDisplayName": self.title,
            "CFBundleInfoDictionaryVersion": "6.0",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": version,
            "CFBundleVersion": version,
            "LSApplicationCategoryType": "public.app-category.games",
        })
        return out

    def finish_mac(self, ctx, build) -> dict:
        app = build.root / self.app
        notes = []
        if ctx.flag("sign", True) and ctx.which("codesign"):
            ctx.progress(0.9, "Signing")
            signed = codesign_if_changed(app, ctx.run)
            notes.append("Signed ad-hoc: it runs on this Mac." if signed else "Already signed (nothing changed).")
        else:
            notes.append("Not signed.")
        if self.same:
            notes.append("The game's own player, frameworks and native plugins are kept.")
        return {"outputs": [str(app)], "open": str(app), "summary": f"Built {app.name} - experimental",
                "notes": notes}

    # -- what to expect -----------------------------------------------------------------------

    @staticmethod
    def limits(info: dict, os_: str) -> list[str]:
        out = []
        src = info.get("platform")
        drawable = unity.can_draw(info, os_)
        if drawable is False:
            apis = ", ".join((info.get("shaders") or {}).get("apis") or [])
            out.append(f"This build's shaders are {apis} only, and {NAMES[os_]} needs {APIS[os_]}: objects will "
                       "show pink or invisible. Only the game's developer can add that graphics API.")
        elif src != os_ and drawable is None:
            out.append(f"Unity builds only carry shaders for their own platform's graphics APIs. This {NAMES[src]} "
                       f"build must also have {APIS[os_]} shaders, or objects will show pink or invisible on "
                       f"{NAMES[os_]}. Test it before sharing.")
        plugins = [p for p in info.get("plugins") or [] if p not in (info.get("apple_services") or [])]
        if plugins and src != os_:
            out.append(f"Native plugins not carried over (built for {NAMES[src]}): {', '.join(plugins)}. Features "
                       "that use them (sound engines, controllers, video...) may fail.")
        if info.get("apple_services"):
            out.append("Apple services (" + ", ".join(info["apple_services"]) + ") need the App Store's signature: "
                       "outside the App Store they may stop the game at start (FairPlay) or fail (Game Center, "
                       "iCloud).")
        if info.get("app_store"):
            out.append("This is an App Store / Apple Arcade build; its terms limit it to Apple devices, and its "
                       "subscription check may refuse to start a re-signed copy.")
        return out
