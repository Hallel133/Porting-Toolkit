"""Godot export: the game's .pck plus the official export template.

- macOS: ``<Title>.app`` from ``macos.zip`` (``osx.zip`` on Godot 3), with
  the pck in ``Contents/Resources``, a filled-in Info.plist, the binary
  thinned to the target's architecture and an ad-hoc signature.
- Web: ``index.html`` + ``index.js`` / ``.wasm`` / ``.pck`` from the web
  template. The single-threaded template is used when the version has one
  (Godot 4.3+), so the build runs on any static host.
- Android (experimental): ``<Title>.apk`` from ``android_release.apk``
  the way Godot's own exporter does it without Gradle: the game's files in
  ``assets/``, the manifest and app name rewritten, aligned and signed with
  the Android SDK's apksigner (a debug key unless ``keystore=`` is set).
- Windows / Linux: ``<Title>/<Title>.exe`` (or ``.x86_64``) from the
  release template with ``<Title>.pck`` next to it (Godot loads the pck
  named after the executable), the game's GDExtension libraries for that
  platform, and a zip of the folder.

Settings (``--set key=value``): ``templates`` (a folder or .tpz),
``godot_version`` (e.g. ``4.3`` or ``4.3.rc1``), ``bundle_id``,
``threads`` (web), ``thin`` and ``sign`` (macOS, default true),
``zip`` (Windows/Linux, default true), and for Android ``package``,
``version_code``, ``orientation``, ``permissions`` (e.g. ``INTERNET,VIBRATE``),
``abis`` (default ``arm64-v8a,armeabi-v7a``), ``android_textures``
(``astc``, the default: desktop-only textures are converted to ASTC with
astcenc; ``decode``: phones decode them), ``astc_block`` (4x4 or 8x8),
``astc_quality`` (fastest/fast/medium/thorough), ``keystore``,
``keystore_pass``, ``key_alias``, ``key_pass``, ``android_sdk``.
"""

from __future__ import annotations

import html
import json
import os
import re
import shutil
import struct
import sys
import zipfile
from pathlib import Path

from ptk_engine.sdk import StagedExportPlugin, error
from ptk_engine import android, texconv
from ptk_engine.formats import axml, ctex, godot

RELEASES = "https://github.com/godotengine/godot/releases/download"


def safe_name(title: str) -> str:
    name = re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .") or "Game"
    return name[:64]


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


def editor_template_dirs() -> list[Path]:
    """Where the Godot editor installs export templates."""
    home = Path.home()
    if sys.platform == "darwin":
        return [home / "Library/Application Support/Godot/export_templates"]
    if os.name == "nt":
        return [Path(os.environ.get("APPDATA", home)) / "Godot/export_templates"]
    base = Path(os.environ.get("XDG_DATA_HOME", home / ".local/share"))
    return [base / "godot/export_templates"]


class Templates:
    """Finds export template files for one Godot version."""

    def __init__(self, ctx, version: str, status: str):
        self.ctx = ctx
        self.version = version
        self.status = status
        self.dir_name = f"{version}.{status}"
        self.cache = ctx.cache / "templates" / self.dir_name
        self._remote = None
        self._tpz = None
        self.sources: list[Path] = []
        custom = ctx.setting("templates")
        if custom:
            p = Path(custom).expanduser()
            if p.is_file() and p.suffix.lower() in (".tpz", ".zip"):
                self._tpz = zipfile.ZipFile(p)
            elif p.is_dir():
                self.sources += [p, p / self.dir_name, p / "templates"]
            else:
                ctx.fail(f"templates={custom} is not a folder or a .tpz file")
        self.sources += [d / self.dir_name for d in editor_template_dirs()]
        self.sources.append(self.cache)

    @property
    def url(self) -> str:
        tag = f"{self.version}-{self.status}"
        return f"{RELEASES}/{tag}/Godot_v{tag}_export_templates.tpz"

    def _archive(self) -> zipfile.ZipFile:
        if self._tpz is not None:
            return self._tpz
        if self._remote is None:
            self.ctx.log(f"Export templates for Godot {self.dir_name} are not installed; fetching only the needed files from {self.url}")
            try:
                self._remote = self.ctx.remote_zip(self.url)
            except Exception as e:  # noqa: BLE001 - explain, with the fix
                self.ctx.fail(
                    f"could not open the Godot {self.dir_name} export templates ({e}). "
                    "Install them from the Godot editor (Editor > Manage Export Templates) "
                    "or pass --set templates=PATH."
                )
        return self._remote

    def names(self) -> set[str]:
        found = set()
        for s in self.sources:
            if s.is_dir():
                found |= {p.name for p in s.iterdir() if p.is_file()}
        try:
            found |= {Path(n).name for n in self._archive().namelist()}
        except Exception:  # noqa: BLE001 - offline: use what is local
            pass
        return found

    def has(self, name: str) -> bool:
        return any((s / name).is_file() for s in self.sources) or name in self.names()

    def get(self, name: str) -> Path:
        for s in self.sources:
            if (s / name).is_file():
                return s / name
        z = self._archive()
        member = next((n for n in z.namelist() if Path(n).name == name), None)
        if member is None:
            self.ctx.fail(f"the Godot {self.dir_name} templates have no {name}")
        dest = self.cache / name
        partial = dest.with_name(name + ".partial")
        self.ctx.check_write(dest)
        dest.parent.mkdir(parents=True, exist_ok=True)
        info = z.getinfo(member)
        done = 0
        with z.open(member) as src, open(partial, "wb") as out:
            while chunk := src.read(1 << 22):
                out.write(chunk)
                done += len(chunk)
                self.ctx.progress(0.05 + 0.6 * done / max(info.file_size, 1),
                                  f"Downloading {name} ({done >> 20} of {info.file_size >> 20} MB)")
        partial.replace(dest)
        return dest


def make_icns(ctx, image: Path, dest: Path) -> bool:
    """PNG/JPEG → .icns with the macOS sips and iconutil tools."""
    if not (ctx.which("sips") and ctx.which("iconutil")):
        return False
    iconset = ctx.temp / "icon.iconset"
    shutil.rmtree(iconset, ignore_errors=True)
    iconset.mkdir(parents=True)
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            px = size * scale
            name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
            ctx.run(["sips", "-s", "format", "png", "-z", str(px), str(px), image, "--out", iconset / name])
    ctx.run(["iconutil", "-c", "icns", iconset, "-o", dest])
    return True


def fill_plist(text: str, values: dict[str, str]) -> str:
    for key in sorted(values, key=len, reverse=True):
        text = text.replace("$" + key, values[key])
    # Placeholders we do not fill: drop lines made of one, empty the rest.
    text = re.sub(r"(?m)^[ \t]*\$[a-z_0-9]+[ \t]*\n", "", text)
    return re.sub(r"\$[a-z][a-z_0-9]*", "", text)


class Plugin(StagedExportPlugin):
    def check(self, ctx):
        target = ctx.target or {}
        if target.get("os") not in ("macos", "windows", "linux", "android", "web"):
            return [error(f"godot-export builds for macOS, Windows, Linux, Android and the web, not {target.get('id')}")]
        if not godot.find(ctx.game):
            return [error("no Godot .pck found in the game (run the import again?)")]
        return []

    def stage(self, ctx):
        assets = ctx.process_assets(ctx.temp, "godot")
        if ctx.assets_mode == "plan":
            return {"summary": "Godot games keep their imported resources", "assets": assets}
        result = self.build(ctx)
        if assets:
            result["assets"] = assets
        return result

    def build(self, ctx):
        target = ctx.target or {}
        os_ = target.get("os")
        if os_ not in ("macos", "web", "windows", "linux", "android"):
            ctx.fail(f"godot-export builds for macOS, Windows, Linux, Android and the web, not {target.get('id')}")
        recipe = ctx.recipe("godot") or {}
        pck_path = None
        if recipe.get("pck"):
            p = ctx.game if ctx.game.is_file() else ctx.game / recipe["pck"]
            if p.is_file():
                pck_path = p
        if pck_path is None:
            found = godot.find(ctx.game)
            if not found:
                ctx.fail("no Godot .pck found in the game (run the import again?)")
            pck_path = found[0]
        try:
            pck = godot.read(pck_path)
        except godot.PckError as e:
            ctx.fail(str(e))
        if pck.encrypted:
            ctx.fail("the .pck is encrypted; it only runs with templates built with the game's own key")
        if pck.sparse:
            ctx.fail("this .pck is a sparse bundle (Godot 4.7+): its files are stored next to it, which the "
                     "exporter does not support yet")
        if pck.patch:
            ctx.fail(f"{pck_path.name} is a patch pack (Godot 4.7+) that changes another pack; it does not run on its own")
        if pck.uses_csharp():
            ctx.fail("C# (.NET) Godot games are not supported by the built-in exporter yet (GDScript games are)")
        self.settings = godot.project_settings(pck)

        override = ctx.setting("godot_version")
        if override:
            m = re.fullmatch(r"(\d+\.\d+(?:\.\d+)?)(?:[.-]([a-z]+\d*))?", override)
            if not m:
                ctx.fail(f"godot_version={override} should look like 4.3 or 4.3.rc1")
            version, status = m.group(1), m.group(2) or "stable"
        else:
            version, status = pck.version_string, "stable"
        if pck.version[0] < 3:
            ctx.fail(f"Godot {version} is too old (Godot 3 and 4 are supported)")
        templates = Templates(ctx, version, status)
        title = ctx.title
        ctx.progress(0.02, f"Godot {version}: {len(pck.files)} files")

        staged = godot.extract(pck, ctx.temp / "game.pck")
        if os_ == "macos":
            out = self.macos(ctx, templates, pck, staged, title)
        elif os_ in ("windows", "linux"):
            out = self.desktop(ctx, templates, pck, staged, title, os_)
        elif os_ == "android":
            out = self.android(ctx, templates, pck, staged, title)
        else:
            out = self.web(ctx, templates, pck, staged, title)
        ctx.progress(1.0, "Done")
        return out

    # -- macOS ---------------------------------------------------------------

    def macos(self, ctx, templates, pck, staged, title):
        name = "macos.zip" if pck.version[0] >= 4 else "osx.zip"
        archive = templates.get(name)
        ctx.progress(0.7, "Building the app")
        work = ctx.temp / "macos"
        with zipfile.ZipFile(archive) as z:
            z.extractall(work)
        apps = [p for p in work.iterdir() if p.suffix == ".app"]
        if not apps:
            ctx.fail(f"{name} has no .app template")
        app_name = safe_name(title)
        binary = app_name
        app = ctx.clean_dir(ctx.output / f"{app_name}.app")
        shutil.copytree(apps[0], app, dirs_exist_ok=True)
        macos_dir = app / "Contents" / "MacOS"
        bins = sorted(macos_dir.iterdir())
        release = next((b for b in bins if "release" in b.name), None) or (bins[0] if bins else None)
        if release is None:
            ctx.fail("the macOS template has no executable")
        for b in bins:
            if b != release:
                b.unlink()
        exe = macos_dir / binary
        release.rename(exe)
        exe.chmod(0o755)

        arch = {"arm64": "arm64", "x86_64": "x86_64"}.get(ctx.target.get("arch"))
        if arch and ctx.flag("thin", True) and ctx.which("lipo"):
            archs = ctx.run(["lipo", "-archs", exe], check=False).stdout.split()
            if arch in archs and len(archs) > 1:
                ctx.run(["lipo", exe, "-thin", arch, "-output", exe])
            elif arch not in archs and archs:
                ctx.fail(f"the template has no {arch} code (it has {', '.join(archs)})")

        major, minor, _ = pck.version
        version = str((ctx.manifest.get("version") or "1.0.0"))
        bundle = ctx.setting("bundle_id") or f"org.portingtoolkit.{slug(title)}"
        plist = app / "Contents" / "Info.plist"
        plist.write_text(fill_plist(plist.read_text(encoding="utf-8"), {
            "binary": binary,
            "name": html.escape(title),
            "bundle_identifier": bundle,
            "identifier": bundle,
            "short_version": version,
            "version": version,
            "signature": "????",
            "copyright": html.escape(ctx.setting("copyright", "")),
            "app_category": ctx.setting("app_category", "games"),
            "min_version_arm64": "11.00",
            "min_version_x86_64": "10.13",
            "min_version": "10.13",
            "highres": "<true/>",
            "platfbuild": "",
            "sdkver": "",
            "sdkbuild": "",
            "sdkname": "macosx",
            "xcodever": "",
            "xcodebuild": "",
        }), encoding="utf-8")
        shutil.copyfile(staged, app / "Contents" / "Resources" / f"{binary}.pck")
        self.native_libraries(ctx, pck, app)
        self.app_icon(ctx, pck, app)

        signed = False
        if ctx.flag("sign", True) and ctx.which("codesign"):
            ctx.progress(0.9, "Signing (ad-hoc)")
            ctx.run(["codesign", "--force", "--deep", "--sign", "-", "--timestamp=none", app])
            signed = True
        elif arch == "arm64":
            why = "sign=false" if not ctx.flag("sign", True) else "codesign was not found; install the Xcode command line tools"
            ctx.warn(f"The app is not signed ({why}); Apple Silicon Macs refuse to run unsigned apps.")
        notes = [
            f"Built with the Godot {templates.dir_name} template.",
            "Signed ad-hoc: it runs on this Mac. To share it, sign and notarize it with an Apple Developer ID."
            if signed else "Not signed.",
        ]
        return {
            "outputs": [str(app)],
            "open": str(app),
            "summary": f"Built {app.name} for {ctx.target.get('name')}",
            "notes": notes,
        }

    @staticmethod
    def source_app(pck) -> Path | None:
        """The .app the pck came from, if the game is a macOS build."""
        for parent in Path(pck.container).parents:
            if parent.suffix == ".app":
                return parent
        return None

    def native_libraries(self, ctx, pck, app: Path) -> None:
        """GDExtension libraries (GodotSteam, ...) go to Contents/Frameworks.

        Godot looks for them there in an exported app. They come from the
        game's own macOS build; a Windows or Linux build has no macOS
        versions, so the extension (and what needs it) will not load.
        """
        wanted = godot.extension_libraries(pck, "macos")
        if not wanted:
            return
        frameworks = app / "Contents" / "Frameworks"
        source = self.source_app(pck)
        copied: list[str] = []
        if source and (source / "Contents" / "Frameworks").is_dir():
            for item in sorted((source / "Contents" / "Frameworks").iterdir()):
                if item.name.startswith("._"):
                    continue
                dest = frameworks / item.name
                frameworks.mkdir(parents=True, exist_ok=True)
                if item.is_dir():
                    shutil.copytree(item, dest, symlinks=True, dirs_exist_ok=True)
                else:
                    shutil.copy2(item, dest)
                copied.append(item.name)
        else:
            game_root = ctx.game if ctx.game.is_dir() else ctx.game.parent
            for libs in wanted.values():
                for lib in libs:
                    name = Path(lib.removeprefix("res://")).name
                    hits = [p for p in game_root.rglob(name) if p.exists()] if name else []
                    if hits:
                        frameworks.mkdir(parents=True, exist_ok=True)
                        src = hits[0]
                        if src.is_dir():
                            shutil.copytree(src, frameworks / name, symlinks=True, dirs_exist_ok=True)
                        else:
                            shutil.copy2(src, frameworks / name)
                        copied.append(name)
        from_app = bool(source and copied)
        missing = []
        for ext, libs in wanted.items():
            names = {Path(lib.removeprefix("res://")).name for lib in libs}
            if not from_app and not names & set(copied):
                missing.append(Path(ext).stem)
        if copied:
            ctx.log("Native libraries copied into the app: " + ", ".join(copied))
        for name in missing:
            ctx.warn(f"No macOS version of the {name} extension was found (it ships with the game's macOS build). "
                     "Features that need it will not work, and the game may stop on a blank screen if it requires it.")

    def app_icon(self, ctx, pck, app: Path) -> None:
        """Icon: --set icon=, the project's icon, the source app's, or the game's own."""
        dest = app / "Contents" / "Resources" / "icon.icns"
        chosen = ctx.setting("icon")
        candidate = Path(chosen).expanduser() if chosen else ctx.art("icon")
        source = self.source_app(pck)
        if candidate is None and source:
            for name in ("icon.icns", "AppIcon.icns"):
                if (source / "Contents" / "Resources" / name).is_file():
                    shutil.copyfile(source / "Contents" / "Resources" / name, dest)
                    return
        if candidate is None:
            res = str(self.settings.get("application/config/icon") or "")
            data = godot.read_file(pck, res) if res.lower().endswith((".png", ".jpg", ".jpeg")) else None
            if data:
                candidate = ctx.write_bytes(ctx.temp / f"game-icon{Path(res).suffix}", data)
        if candidate is None or not candidate.is_file():
            return
        if candidate.suffix.lower() == ".icns":
            shutil.copyfile(candidate, dest)
        elif not make_icns(ctx, candidate, dest):
            ctx.warn("Could not convert the icon (needs sips and iconutil); the app keeps Godot's icon.")

    # -- Windows and Linux ---------------------------------------------------

    @staticmethod
    def desktop_template(major: int, os_: str, arch: str) -> str:
        if major >= 4:
            return f"windows_release_{arch}.exe" if os_ == "windows" else f"linux_release.{arch}"
        bits = {"x86_64": "64", "x86_32": "32", "arm64": "arm64", "arm32": "arm32"}.get(arch, "64")
        return f"windows_{bits}_release.exe" if os_ == "windows" else f"linux_x11_{bits}_release"

    def desktop(self, ctx, templates, pck, staged, title, os_):
        arch = {"x86_64": "x86_64", "arm64": "arm64"}.get(ctx.target.get("arch"), "x86_64")
        name = self.desktop_template(pck.version[0], os_, arch)
        template = templates.get(name)
        ctx.progress(0.75, "Building the game folder")
        app_name = safe_name(title)
        ctx.clean_dir(ctx.output)
        folder = ctx.output / app_name
        folder.mkdir(parents=True)
        exe = folder / (f"{app_name}.exe" if os_ == "windows" else f"{app_name}.{arch}")
        shutil.copyfile(template, exe)
        exe.chmod(0o755)
        shutil.copyfile(staged, folder / f"{app_name}.pck")
        copied, missing = self.desktop_libraries(ctx, pck, folder, os_, arch)
        if os_ == "linux":
            self.linux_icon(ctx, pck, folder)
        ctx.write_text(folder / "README.txt", DESKTOP_README[os_].format(title=title, exe=exe.name))
        notes = [f"Built with the Godot {templates.dir_name} {name} template."]
        if os_ == "windows":
            notes.append("The .exe keeps Godot's icon and is not signed, so Windows SmartScreen may ask "
                         "before the first start (More info, then Run anyway).")
        else:
            notes.append(f"Start it with ./{exe.name}; the zip keeps the executable bit.")
        if copied:
            notes.append("Native libraries: " + ", ".join(copied))
        for ext in missing:
            ctx.warn(f"No {os_.title()} version of the {ext} extension was found next to the game "
                     f"(it ships with the game's {os_.title()} build). Features that need it will not work, "
                     "and the game may stop on a blank screen if it requires it.")
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            ctx.progress(0.92, "Zipping")
            z = ctx.zip_dir(folder, ctx.output / f"{app_name}-{ctx.target.get('id')}.zip",
                            executables={exe.name})
            outputs.append(str(z))
        return {
            "outputs": outputs,
            "open": str(folder),
            "summary": f"Built {folder.name}/ for {ctx.target.get('name')}",
            "notes": notes,
        }

    def desktop_libraries(self, ctx, pck, folder: Path, os_: str, arch: str) -> tuple[list[str], list[str]]:
        """GDExtension libraries (and their dependencies, e.g. steam_api64.dll)
        go next to the executable, where an exported Godot game looks for them.
        They are taken from the game folder, which only has them when the
        game is a build for the same platform."""
        wanted = godot.extension_files(pck, os_, arch)
        game_root = ctx.game if ctx.game.is_dir() else ctx.game.parent
        copied: list[str] = []
        missing: list[str] = []
        for ext, names in wanted.items():
            found_any = False
            for name in names:
                hits = sorted(p for p in game_root.rglob(name) if p.is_file() and not p.name.startswith("._"))
                if hits:
                    shutil.copy2(hits[0], folder / name)
                    copied.append(name)
                    found_any = True
            if names and not found_any:
                missing.append(Path(ext).stem)
        # GodotSteam and other Steam builds read the app id from this file.
        appid = game_root / "steam_appid.txt"
        if wanted and appid.is_file():
            shutil.copy2(appid, folder / appid.name)
        return copied, missing

    def linux_icon(self, ctx, pck, folder: Path) -> None:
        icon = ctx.art("icon")
        if icon is None or icon.suffix.lower() != ".png":
            res = str(self.settings.get("application/config/icon") or "")
            data = godot.read_file(pck, res) if res.lower().endswith(".png") else None
            if data:
                ctx.write_bytes(folder / "icon.png", data)
            return
        shutil.copyfile(icon, folder / "icon.png")

    # -- Android -------------------------------------------------------------

    #: DisplayServer.ScreenOrientation -> android:screenOrientation
    #: (``_get_android_orientation_value`` in Godot's exporter).
    ORIENTATION = {0: 0, 1: 1, 2: 8, 3: 9, 4: 6, 5: 7, 6: 10}

    def android(self, ctx, templates, pck, staged, title):
        major, minor, _ = pck.version
        if major < 4:
            ctx.fail("Android exports support Godot 4 games; this one is Godot 3")
        sdk = None
        if ctx.flag("sign", True):
            sdk = android.find_sdk(ctx.setting("android_sdk"))
            if sdk is None:
                ctx.fail("the Android SDK build-tools are needed to sign the APK (apksigner): brew install --cask "
                         "android-commandlinetools, then sdkmanager \"build-tools;35.0.0\" (or set android_sdk=PATH, "
                         "or sign=false to sign it yourself)")
        template = templates.get("android_release.apk")
        ctx.progress(0.7, "Building the APK")
        staged_pck = godot.read(staged)
        imports, fixed, missing = self.android_imports(staged_pck)
        if missing and (major, minor) < (4, 2) and not ctx.flag("force"):
            ctx.fail(f"{missing} textures only have desktop formats (S3TC/BPTC), and Godot {major}.{minor} cannot "
                     "decode those on phones. Only the developer can fix it (Project Settings > Rendering > Textures "
                     "> Import ETC2 ASTC), or try anyway with --set force=true.")
        astc_files: dict[str, bytes] = {}
        converted = self.android_astc(ctx, staged_pck, imports, astc_files) if fixed else 0
        if converted:
            ctx.log(f"Converted {converted} textures to ASTC for phones.")
        if fixed - converted:
            ctx.warn(f"{fixed - converted} textures only have desktop formats (S3TC/BPTC). Godot decodes them on phones "
                     "without those formats, which uses more memory and loads slower. The developer can avoid it "
                     "by enabling Project Settings > Rendering > Textures > Import ETC2 ASTC.")
        for ext in pck.extensions():
            ctx.warn(f"{Path(ext).stem} is a native extension; an Android build of it is not included, so features "
                     "that need it will not work.")
        abis = [a.strip() for a in (ctx.setting("abis") or "arm64-v8a,armeabi-v7a").split(",") if a.strip()]
        game_orient = self.ORIENTATION.get(int(self.settings.get("display/window/handheld/orientation", 0) or 0), 0)
        app = android.settings_for_export(ctx, title, game_orient)
        package, version, code = app["package"], app["version"], app["code"]

        entries: list[android.Entry] = []
        found_abis = set()
        with zipfile.ZipFile(template) as z:
            for info in z.infolist():
                name = info.filename
                if name.endswith("/") or name.startswith("META-INF/"):
                    continue
                if name.startswith("lib/"):
                    abi = name.split("/")[1]
                    if abi not in abis:
                        continue
                    found_abis.add(abi)
                data = z.read(info)
                if name == "AndroidManifest.xml":
                    data = android.patch_manifest(data, package, code, version, app["orientation"],
                                                  app["permissions"])
                elif name == "resources.arsc":
                    data = axml.replace_table_strings(
                        data, lambda s: title if s.startswith("godot-project-name") else s)
                entries.append(android.Entry(name, data, info.compress_type != zipfile.ZIP_STORED))
        if missing_abis := [a for a in abis if a not in found_abis]:
            ctx.fail(f"the template has no libraries for {', '.join(missing_abis)}")
        icon = self.android_icon(ctx, pck)
        if icon and not android.replace_icons(entries, icon, ctx.temp):
            ctx.warn("Could not resize the icon (needs macOS sips); the app keeps Godot's icon.")
        for e in staged_pck.files:
            name = "assets/" + e.path
            if e.path in imports:
                entries.append(android.Entry(name, imports[e.path], android.should_compress(e.path)))
            else:
                entries.append(android.Entry(name, None, android.should_compress(e.path),
                                             src=(staged, e.offset, e.size)))
        for name, data in astc_files.items():
            entries.append(android.Entry(name, data, False))
        entries.append(android.Entry("assets/_cl_", struct.pack("<I", 0), False))  # no extra command line

        ctx.clean_dir(ctx.output)
        apk = ctx.output / f"{safe_name(title)}.apk"
        android.write_aligned(entries, apk)
        if sdk is not None:
            ctx.progress(0.92, "Signing")
            signed_with = android.sign_for_export(ctx, apk, sdk)
        else:
            signed_with = "nothing (sign=false): sign it with apksigner before installing"
        return {
            "outputs": [str(apk)],
            "open": str(ctx.output),
            "summary": f"Built {apk.name} ({package}, version {version} / {code}) - experimental",
            "notes": [
                f"Built with the Godot {templates.dir_name} android_release.apk template for "
                f"{', '.join(abis)}.",
                f"Signed with {signed_with}. Install it with: adb install -r \"{apk.name}\" "
                "(or copy it to the phone and allow installing unknown apps).",
                "Android builds are experimental: test on a phone before sharing.",
            ],
        }

    @staticmethod
    def android_imports(pck) -> tuple[dict[str, bytes], int, int]:
        """``.import`` remaps for phones. A texture imported only in desktop
        GPU formats has ``path.s3tc``/``path.bptc`` lines and no
        ``path.etc2``/``path.astc``; Android matches none of them, so it gets
        a plain ``path=`` to the desktop file as the fallback Godot's loader
        uses (``ResourceFormatImporter``), and Godot decodes it at load.
        Returns (new file contents, fixed count, count needing a fix)."""
        present = {e.path for e in pck.files}
        changed: dict[str, bytes] = {}
        need = 0
        for e in pck.files:
            if not e.path.endswith(".import") or e.size > 1 << 20:
                continue
            raw = godot.read_file(pck, e.path)
            if raw is None:
                continue
            text = raw.decode("utf-8", "replace")
            keys = dict(re.findall(r'(?m)^(path(?:\.[a-z0-9_]+)?)="([^"]*)"', text))
            if not keys or "path" in keys or {"path.etc2", "path.astc", "path.etc"} & set(keys):
                continue
            desktop = keys.get("path.s3tc") or keys.get("path.bptc")
            if not desktop:
                continue
            need += 1
            if desktop.removeprefix("res://") not in present:
                continue
            last = list(re.finditer(r'(?m)^path\.[a-z0-9_]+="[^"]*"\n?', text))[-1]
            text = text[:last.end()] + ("" if text[last.end() - 1:last.end()] == "\n" else "\n") \
                + f'path="{desktop}"\n' + text[last.end():]
            changed[e.path] = text.encode("utf-8")
        return changed, len(changed), need

    @staticmethod
    def android_astc(ctx, pck, imports: dict[str, bytes], out: dict[str, bytes]) -> int:
        """Converts the desktop-only textures to ASTC (``android_textures=astc``,
        the default): a new ``.astc.ctex`` next to each, and ``path.astc`` in
        its remap ahead of the ``path=`` fallback, so phones with ASTC (most)
        load it and the others still decode the desktop one."""
        mode = (ctx.setting("android_textures") or "astc").lower()
        if mode not in ("astc", "decode"):
            ctx.fail(f"android_textures={mode}: use astc or decode")
        if mode == "decode":
            return 0
        try:
            tc = texconv.Transcoder(ctx.cache / "texconv", ctx.toolbox, ctx.setting("astc_block") or "4x4",
                                    ctx.setting("astc_quality") or "fast")
        except texconv.TexconvError as e:
            ctx.fail(str(e))
        problem = tc.problem()
        if problem:
            ctx.warn(f"Textures were not converted to ASTC: {problem}.")
            return 0
        converted = 0
        failed: list[str] = []
        items = list(imports.items())
        for i, (path, raw) in enumerate(items):
            text = raw.decode("utf-8")
            m = re.search(r'(?m)^path="([^"]*)"', text)
            if not m:
                continue
            desktop = m.group(1)
            data = godot.read_file(pck, desktop)
            if data is None:
                continue
            ctx.progress(0.72 + 0.15 * i / max(len(items), 1), f"ASTC {i + 1}/{len(items)}: {Path(desktop).name}")
            try:
                astc = tc.to_astc(data)
            except (texconv.TexconvError, ctex.CtexError) as e:
                failed.append(f"{Path(path).stem}: {e}")
                continue
            if astc is None:
                continue
            new = re.sub(r"\.(s3tc|bptc)\.ctex$", ".astc.ctex", desktop)
            if new == desktop:
                new = desktop[:-len(".ctex")] + ".astc.ctex"
            out["assets/" + new.removeprefix("res://")] = astc
            imports[path] = (text[:m.start()] + f'path.astc="{new}"\n' + text[m.start():]).encode("utf-8")
            converted += 1
        if failed:
            ctx.warn(f"{len(failed)} textures could not be converted to ASTC and stay in desktop formats "
                     f"(first: {failed[0]}).")
        return converted

    def android_icon(self, ctx, pck) -> Path | None:
        chosen = ctx.setting("icon")
        candidate = Path(chosen).expanduser() if chosen else ctx.art("icon")
        if candidate is None:
            res = str(self.settings.get("application/config/icon") or "")
            data = godot.read_file(pck, res) if res.lower().endswith((".png", ".jpg", ".jpeg")) else None
            if data:
                candidate = ctx.write_bytes(ctx.temp / f"game-icon{Path(res).suffix}", data)
        return candidate if candidate and candidate.is_file() else None

    # -- web -----------------------------------------------------------------

    def web(self, ctx, templates, pck, staged, title):
        major, minor, _ = pck.version
        threads = ctx.flag("threads", False)
        if major >= 4:
            candidates = ["web_release.zip"] if threads else ["web_nothreads_release.zip", "web_release.zip"]
        else:
            candidates = ["webassembly_threads_release.zip"] if threads else []
            candidates.append("webassembly_release.zip")
        choice = next((c for c in candidates if templates.has(c)), None)
        if choice is None:
            ctx.fail(f"the Godot {templates.dir_name} templates have no web template ({', '.join(candidates)})")
        threaded = choice in ("web_release.zip", "webassembly_threads_release.zip")
        archive = templates.get(choice)
        ctx.progress(0.75, "Building the web page")
        out = ctx.output
        ctx.clean_dir(out)
        exe = "index"
        with zipfile.ZipFile(archive) as z:
            members = {Path(n).name: n for n in z.namelist() if not n.endswith("/")}
            shell = z.read(members.pop("godot.html")).decode("utf-8")
            sw = members.pop("godot.service.worker.js", None)
            offline = members.pop("godot.offline.html", None)
            written = []
            for base, member in members.items():
                dest = out / (exe + base[len("godot"):] if base.startswith("godot.") else base)
                ctx.write_bytes(dest, z.read(member))
                written.append(dest.name)
            if threaded and sw:
                ctx.write_bytes(out / f"{exe}.offline.html", z.read(offline) if offline else b"<p>Offline</p>")
                worker = z.read(sw).decode("utf-8")
            else:
                worker = None
        shutil.copyfile(staged, out / f"{exe}.pck")
        settings = self.settings
        render = godot.renderer(settings, major) if settings else None
        if render and render not in ("gl_compatibility", "GLES2", "GLES3"):
            ctx.warn(f"The game uses the {render.replace('_', ' ').title()} renderer, but browsers only have Godot's "
                     "Compatibility renderer: lighting, glow, fog, some particles and screen shaders can look "
                     "different or stay on screen. Only the game's developer can switch the renderer.")
        for ext in pck.extensions():
            ctx.warn(f"{Path(ext).stem} is a native extension with no web version; features that need it "
                     "(for example Steam) will not work in the browser.")
        splash_res = str(settings.get("application/boot_splash/image") or "")
        splash = godot.read_file(pck, splash_res) if splash_res.lower().endswith(".png") else None
        if splash:
            ctx.write_bytes(out / f"{exe}.png", splash)
        icon_path = ctx.art("icon")
        icon_res = str(settings.get("application/config/icon") or "")
        icon = icon_path.read_bytes() if icon_path else (
            godot.read_file(pck, icon_res) if icon_res.lower().endswith(".png") else None)
        if icon:
            ctx.write_bytes(out / f"{exe}.icon.png", icon)
        sizes = {f"{exe}.pck": (out / f"{exe}.pck").stat().st_size}
        if (out / f"{exe}.wasm").is_file():
            sizes[f"{exe}.wasm"] = (out / f"{exe}.wasm").stat().st_size
        config = {
            "args": [],
            "canvasResizePolicy": 2,
            "executable": exe,
            "experimentalVK": False,
            "fileSizes": sizes,
            "focusCanvas": True,
        }
        if major >= 4:
            config["gdextensionLibs"] = []
            config["ensureCrossOriginIsolationHeaders"] = bool(worker)
            if worker:
                config["serviceWorker"] = f"{exe}.service.worker.js"
        else:
            config["gdnativeLibs"] = []
        values = {
            "GODOT_URL": f"{exe}.js",
            "GODOT_PROJECT_NAME": html.escape(title),
            "GODOT_HEAD_INCLUDE": '<meta name="generator" content="Porting Toolkit">'
            + (f'\n<link rel="icon" href="{exe}.icon.png">' if icon else ""),
            "GODOT_CONFIG": json.dumps(config),
            "GODOT_THREADS_ENABLED": "true" if threaded else "false",
            "GODOT_SPLASH_COLOR": ctx.setting("background")
            or godot.color_hex(settings.get("application/boot_splash/bg_color"), "#000000"),
            "GODOT_SPLASH_CLASSES": (
                f"show-image--true fullsize--{str(bool(settings.get('application/boot_splash/fullsize', True))).lower()} use-filter--true"
                if splash else "show-image--false fullsize--true use-filter--true"),
            "GODOT_SPLASH": f"{exe}.png" if splash else "data:image/gif;base64,R0lGODlhAQABAAAAACw=",
            "GODOT_BASENAME": exe,
            "GODOT_DEBUG_ENABLED": "false",
        }
        for key in sorted(values, key=len, reverse=True):
            shell = shell.replace("$" + key, values[key])
        left = sorted(set(re.findall(r"\$GODOT_[A-Z_]+", shell)))
        if left:
            ctx.warn("Unknown template placeholders left empty: " + ", ".join(left))
            shell = re.sub(r"\$GODOT_[A-Z_]+", "", shell)
        ctx.write_text(out / "index.html", shell)
        if worker:
            files = ["index.html", *sorted(n for n in written if not n.endswith((".wasm", ".pck"))), f"{exe}.offline.html"]
            for key, value in {
                "___GODOT_VERSION___": f"ptk-{pck.version_string}-{sizes[f'{exe}.pck']}",
                "___GODOT_NAME___": exe,
                "___GODOT_OFFLINE_PAGE___": f"{exe}.offline.html",
                "___GODOT_ENSURE_CROSSORIGIN_ISOLATION_HEADERS___": "true",
                "___GODOT_CACHE___": json.dumps(files),
                "___GODOT_OPT_CACHE___": json.dumps([f"{exe}.pck", f"{exe}.wasm"]),
            }.items():
                worker = worker.replace(key, value)
            ctx.write_text(out / f"{exe}.service.worker.js", worker)
        ctx.write_text(out / "README.txt", WEB_README.format(
            title=title, template=templates.dir_name,
            kind="multi-threaded (needs cross-origin isolation)" if threaded else "single-threaded",
        ))
        notes = [f"Built with the Godot {templates.dir_name} {choice} template."]
        if threaded:
            notes.append("Threaded build: the host must send COOP/COEP headers (the included service worker adds them after the first load).")
        return {
            "outputs": [str(out / "index.html")],
            "open": str(out / "index.html"),
            "serve": str(out),
            "summary": f"Built a web version in {out.name}/ ({'threads' if threaded else 'no threads'})",
            "notes": notes,
        }


WEB_README = """\
{title}: web build made by Porting Toolkit
Template: Godot {template}, {kind}

Browsers do not run this from a file:// address. Serve the folder:

    ptk serve .            (or: python3 -m http.server)

and open http://localhost:8000/. To publish it, upload this folder to any
static host (for itch.io: zip it and choose "HTML").
"""

DESKTOP_README = {
    "windows": """\
{title}: Windows build made by Porting Toolkit

Start {exe}. Keep the .pck (and any .dll files) in the same folder.
Windows may warn about an unknown publisher the first time: choose
"More info", then "Run anyway".
""",
    "linux": """\
{title}: Linux build made by Porting Toolkit

Start ./{exe} (if it does not start: chmod +x {exe}). Keep the .pck
(and any .so files) in the same folder.
""",
}
