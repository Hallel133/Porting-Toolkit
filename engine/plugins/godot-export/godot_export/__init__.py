"""Godot export: the game's .pck plus the official export template.

- macOS: ``<Title>.app`` from ``macos.zip`` (``osx.zip`` on Godot 3), with
  the pck in ``Contents/Resources``, a filled-in Info.plist, the binary
  thinned to the target's architecture and an ad-hoc signature.
- Web: ``index.html`` + ``index.js`` / ``.wasm`` / ``.pck`` from the web
  template. The single-threaded template is used when the version has one
  (Godot 4.3+), so the build runs on any static host.

Settings (``--set key=value``): ``templates`` (a folder or .tpz),
``godot_version`` (e.g. ``4.3`` or ``4.3.rc1``), ``bundle_id``,
``threads`` (web), ``thin`` and ``sign`` (macOS, default true).
"""

from __future__ import annotations

import html
import json
import os
import re
import shutil
import sys
import zipfile
from pathlib import Path

from ptk_engine.formats import godot

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


class Plugin:
    def run(self, ctx):
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
        if os_ not in ("macos", "web"):
            ctx.fail(f"godot-export builds for macOS and the web, not {target.get('id')}")
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
