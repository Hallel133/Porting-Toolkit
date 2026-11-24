"""Ren'Py export: the game with the Ren'Py runtime for another platform.

The layout follows Ren'Py's own distribution builder
(``launcher/game/distribute.rpy``):

- Windows: ``<name>.exe`` (Ren'Py's ``renpy.exe``) and
  ``lib/py3-windows-x86_64/`` next to ``game/``, ``renpy/`` and
  ``lib/python3.x``.
- Linux: ``<name>.sh`` and ``lib/py3-linux-x86_64/<name>``.
- macOS: ``<Title>.app`` with the native files in ``Contents/MacOS``, the
  standard library in ``Contents/Resources/lib`` and the game in
  ``Contents/Resources/autorun``; ad-hoc signed.

Platform files come from the game itself when the build has them (a "PC"
build carries Windows and Linux). Otherwise the official SDK of the game's
Ren'Py version is downloaded once from renpy.org and checked against the
SHA-256 in that version's ``checksums.txt``; only the platform files are
unpacked from it. ``--set sdk=PATH`` uses an SDK folder or zip you have.

The player's saves (``game/saves``) and logs are never copied.

Settings: ``renpy_version``, ``sdk``, ``bundle_id``, ``icon``, ``sign``,
``zip``, ``assets``.
"""

from __future__ import annotations

import hashlib
import plistlib
import re
import shutil
import stat
import zipfile
from pathlib import Path

from ptk_engine.formats import renpy

DL = "https://www.renpy.org/dl"
PLATFORM = {"windows": "windows-x86_64", "linux": "linux-x86_64", "macos": "mac-universal"}
SKIP_TOP = {"log.txt", "traceback.txt", "errors.txt", "files.txt", ".DS_Store"}
SKIP_GAME = {"saves", "cache"}


def safe_name(title: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .")[:64] or "Game"


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


def sha256_for(checksums: str, filename: str) -> str | None:
    """The SHA-256 line for a file in renpy.org's checksums.txt."""
    section = None
    for line in checksums.splitlines():
        line = line.strip()
        if line.startswith("#"):
            section = line.lstrip("# ").lower()
            continue
        parts = line.split()
        if section == "sha256" and len(parts) == 2 and parts[1] == filename and re.fullmatch(r"[0-9a-f]{64}", parts[0]):
            return parts[0]
    return None


class Plugin:
    def run(self, ctx):
        os_ = (ctx.target or {}).get("os")
        if os_ not in PLATFORM:
            ctx.fail(f"renpy-export builds for macOS, Windows and Linux, not {(ctx.target or {}).get('id')}")
        info = ctx.recipe("renpy")
        if not info:
            try:
                info = renpy.detect(ctx.game).to_dict()
            except renpy.RenpyError as e:
                ctx.fail(str(e))
        self.info = info
        self.name = info.get("executable_name") or "game"
        self.py = int(info.get("py") or 3)
        root = ctx.game / info["root"] if info.get("root") else ctx.game
        ctx.progress(0.05, "Copying the game")
        stage = self.stage(ctx, root)
        assets = ctx.process_assets(stage / "game", "renpy")
        if ctx.assets_mode == "plan":
            return {"summary": "Estimated the advanced options (nothing was built)", "assets": assets}
        title = ctx.title if ctx.title not in ("", "Game") else (info.get("title") or self.name)
        native = self.native(ctx, os_)
        if os_ == "macos":
            out = self.macos(ctx, stage, native, title)
        else:
            out = self.pc(ctx, stage, native, title, os_)
        if assets:
            out["assets"] = assets
        archives = info.get("archives") or []
        if archives:
            out["notes"].append(f"{len(archives)} .rpa archives were copied unchanged; the advanced options only "
                                "apply to loose files in game/.")
        ctx.progress(1.0, "Done")
        return out

    # -- shared files -----------------------------------------------------------------

    def stage(self, ctx, root: Path) -> Path:
        """game/, renpy/, <name>.py and the other shared files, without saves or logs."""
        stage = ctx.temp / "stage"
        stage.mkdir()

        def skip(p: Path) -> bool:
            if p.name.startswith("._") or p.name in SKIP_TOP:
                return True
            if p.parent == root:
                return p.name == "lib" or p.suffix in (".app", ".exe", ".sh")
            return p.parent == root / "game" and p.name in SKIP_GAME

        ctx.copy_tree(root, stage, skip=skip)
        lib = self.info.get("python_lib")
        if lib and (ctx.game / lib).is_dir():
            ctx.copy_tree(ctx.game / lib, stage / "lib" / Path(lib).name)
        return stage

    # -- platform files -----------------------------------------------------------------

    def native(self, ctx, os_: str) -> dict:
        """{"dir": native files, "python": stdlib folder or None, "launcher": SDK root or None}."""
        plat = PLATFORM[os_]
        have = (self.info.get("platforms") or {}).get(plat)
        if have and (ctx.game / have).is_dir():
            ctx.log(f"Using the game's own {plat} files")
            return {"dir": ctx.game / have, "sdk": None}
        sdk = self.sdk(ctx)
        folder = sdk / "lib" / f"py{self.py}-{plat}"
        if not folder.is_dir():
            ctx.fail(f"the Ren'Py SDK has no lib/py{self.py}-{plat}")
        return {"dir": folder, "sdk": sdk}

    def sdk(self, ctx) -> Path:
        custom = ctx.setting("sdk")
        version = ctx.setting("renpy_version") or (
            ".".join(str(self.info.get("version") or "").split(".")[:3]) or None)
        if custom:
            src = Path(custom).expanduser()
            if src.is_dir():
                return src
            if not src.is_file():
                ctx.fail(f"sdk={custom} is not a folder or a zip")
            archive = src
            key = hashlib.sha256(f"{src.resolve()}|{src.stat().st_size}".encode()).hexdigest()[:16]
        else:
            if not version:
                ctx.fail("the game's Ren'Py version is unknown; pass --set renpy_version=X.Y.Z")
            name = f"renpy-{version}-sdk.zip"
            sums = ctx.download(f"{DL}/{version}/checksums.txt", dest=ctx.cache / "sums" / f"{version}.txt")
            sha = sha256_for(sums.read_text(encoding="utf-8", errors="replace"), name)
            if not sha:
                ctx.fail(f"renpy.org lists no SHA-256 for {name}; pass --set sdk=PATH")
            ctx.progress(0.2, f"Downloading the Ren'Py {version} SDK (about 150 MB, once)")
            archive = ctx.download(f"{DL}/{version}/{name}", sha256=sha)
            key = version
        dest = ctx.cache / "sdk" / key
        if not (dest / ".complete").is_file():
            ctx.progress(0.4, "Unpacking the Ren'Py platform files")
            shutil.rmtree(dest, ignore_errors=True)
            dest.mkdir(parents=True)
            self.unpack_sdk(archive, dest)
            (dest / ".complete").write_text(archive.name + "\n")
            if not custom:
                archive.unlink(missing_ok=True)  # the 150 MB zip is not needed once unpacked
        return dest

    @staticmethod
    def unpack_sdk(archive: Path, dest: Path) -> None:
        """Only lib/ and the launchers, without the SDK's top folder."""
        with zipfile.ZipFile(archive) as z:
            for i in z.infolist():
                parts = i.filename.split("/", 1)
                if len(parts) < 2 or i.is_dir():
                    continue
                rel = parts[1]
                if not (rel.startswith("lib/") or rel in ("renpy.py", "renpy.sh", "LICENSE.txt")):
                    continue
                out = renpy_safe_join(dest, rel)
                out.parent.mkdir(parents=True, exist_ok=True)
                if stat.S_ISLNK(i.external_attr >> 16):
                    continue
                with z.open(i) as f, open(out, "wb") as o:
                    shutil.copyfileobj(f, o, 1 << 20)
                if (i.external_attr >> 16) & 0o111:
                    out.chmod(0o755)

    def python_lib(self, ctx, folder: Path, native: dict) -> None:
        """The standard library (lib/python3.x), from the game or the SDK."""
        if any(p.name.startswith("python") for p in (folder / "lib").glob("python*")):
            return
        sdk = native["sdk"] or self.sdk(ctx)
        libs = [p for p in (sdk / "lib").glob(f"python{self.py}.*") if p.is_dir()]
        if not libs:
            ctx.fail("no Python standard library (lib/python3.x) in the game or the SDK")
        ctx.copy_tree(libs[0], folder / "lib" / libs[0].name)

    def launcher_py(self, ctx, folder: Path, native: dict) -> None:
        """<name>.py: what Ren'Py's executables start."""
        if (folder / f"{self.name}.py").is_file():
            return
        sdk = native["sdk"] or self.sdk(ctx)
        shutil.copyfile(sdk / "renpy.py", folder / f"{self.name}.py")

    # -- Windows and Linux ---------------------------------------------------------------

    def pc(self, ctx, stage: Path, native: dict, title: str, os_: str) -> dict:
        name = safe_name(title)
        ctx.clean_dir(ctx.output)
        folder = ctx.output / name
        shutil.copytree(stage, folder)
        plat = PLATFORM[os_]
        lib = folder / "lib" / f"py{self.py}-{plat}"
        ctx.copy_tree(native["dir"], lib)
        self.python_lib(ctx, folder, native)
        self.launcher_py(ctx, folder, native)
        root = ctx.game / self.info["root"] if self.info.get("root") else ctx.game
        if os_ == "windows":
            exe = folder / f"{self.name}.exe"
            src = root / exe.name if (root / exe.name).is_file() else lib / "renpy.exe"
            if not src.is_file():
                ctx.fail("no renpy.exe in the Windows platform files")
            shutil.copyfile(src, exe)
            executables = {exe.name}
            start = exe.name
        else:
            if not (lib / self.name).is_file() and (lib / "renpy").is_file():
                shutil.copy2(lib / "renpy", lib / self.name)
            for p in lib.iterdir():
                if p.is_file() and p.suffix in ("", ".so") and p.name not in ("zsync", "zsyncmake"):
                    p.chmod(0o755)
            sh = folder / f"{self.name}.sh"
            src = root / sh.name if (root / sh.name).is_file() else (native["sdk"] or self.sdk(ctx)) / "renpy.sh"
            shutil.copyfile(src, sh)
            sh.chmod(0o755)
            executables = {sh.name}
            start = f"./{sh.name}"
        ctx.write_text(folder / "README-port.txt", README[os_].format(title=title, start=start))
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            ctx.progress(0.9, "Zipping")
            outputs.append(str(ctx.zip_dir(folder, ctx.output / f"{name}-{ctx.target.get('id')}.zip",
                                           executables=executables)))
        notes = [f"Start it with {start}."]
        if os_ == "windows":
            notes.append("The .exe keeps Ren'Py's icon and is not signed, so Windows SmartScreen may ask once.")
        return {"outputs": outputs, "open": str(folder), "notes": notes,
                "summary": f"Built {folder.name}/ for {ctx.target.get('name')} (Ren'Py {self.version_label()})"}

    def version_label(self) -> str:
        return ".".join(str(self.info.get("version") or "?").split(".")[:3])

    # -- macOS -------------------------------------------------------------------------------

    def macos(self, ctx, stage: Path, native: dict, title: str) -> dict:
        name = safe_name(title)
        app = ctx.clean_dir(ctx.output / f"{name}.app")
        contents = app / "Contents"
        macos = contents / "MacOS"
        resources = contents / "Resources"
        ctx.progress(0.6, "Building the app")
        ctx.copy_tree(native["dir"], macos)
        if not (macos / self.name).is_file():
            if not (macos / "renpy").is_file():
                ctx.fail("no renpy executable in the macOS platform files")
            shutil.copy2(macos / "renpy", macos / self.name)
        for p in macos.iterdir():
            if p.is_file() and p.suffix in ("", ".dylib"):
                p.chmod(0o755)
        autorun = resources / "autorun"
        for p in sorted(stage.iterdir()):
            if p.name == "lib":
                for lib in p.iterdir():
                    ctx.copy_tree(lib, resources / "lib" / lib.name)
            elif p.is_dir():
                ctx.copy_tree(p, autorun / p.name)
            else:
                autorun.mkdir(parents=True, exist_ok=True)
                shutil.copy2(p, autorun / p.name)
        if not any((resources / "lib").glob("python*")):
            sdk = native["sdk"] or self.sdk(ctx)
            libs = [p for p in (sdk / "lib").glob(f"python{self.py}.*") if p.is_dir()]
            if not libs:
                ctx.fail("no Python standard library (lib/python3.x) in the game or the SDK")
            ctx.copy_tree(libs[0], resources / "lib" / libs[0].name)
        if not (autorun / f"{self.name}.py").is_file():
            sdk = native["sdk"] or self.sdk(ctx)
            shutil.copyfile(sdk / "renpy.py", autorun / f"{self.name}.py")
        has_icon = self.icon(ctx, resources)
        bundle = ctx.setting("bundle_id") or f"org.portingtoolkit.{slug(title)}"
        plist = {
            "CFBundleDevelopmentRegion": "English",
            "CFBundleDisplayName": title,
            "CFBundleExecutable": self.name,
            "CFBundleIdentifier": bundle,
            "CFBundleInfoDictionaryVersion": "6.0",
            "CFBundleName": title[:15],
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": str(ctx.manifest.get("version") or "1.0.0"),
            "CFBundleVersion": str(ctx.manifest.get("version") or "1.0.0"),
            "LSApplicationCategoryType": "public.app-category.simulation-games",
            "NSHighResolutionCapable": True,
        }
        if has_icon:
            plist["CFBundleIconFile"] = "icon"
        (contents / "Info.plist").write_bytes(plistlib.dumps(plist))
        signed = False
        if ctx.flag("sign", True) and ctx.which("codesign"):
            ctx.progress(0.9, "Signing (ad-hoc)")
            ctx.run(["codesign", "--force", "--deep", "--sign", "-", "--timestamp=none", app])
            signed = True
        return {
            "outputs": [str(app)],
            "open": str(app),
            "summary": f"Built {app.name} (Ren'Py {self.version_label()})",
            "notes": [
                "The app is universal (Apple Silicon and Intel).",
                "Signed ad-hoc: it runs on this Mac. To share it, sign and notarize it with an Apple Developer ID."
                if signed else "Not signed.",
            ],
        }

    def icon(self, ctx, resources: Path) -> bool:
        """--set icon=, the project's icon, or the game's own Mac icon."""
        chosen = ctx.setting("icon")
        src = Path(chosen).expanduser() if chosen else ctx.art("icon")
        app = self.info.get("mac_app")
        if src is None and app is not None:
            own = (ctx.game / app if app else ctx.game) / "Contents" / "Resources" / "icon.icns"
            if own.is_file():
                shutil.copyfile(own, resources / "icon.icns")
                return True
        if src is None or not src.is_file():
            return False
        if src.suffix.lower() == ".icns":
            shutil.copyfile(src, resources / "icon.icns")
            return True
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


def renpy_safe_join(base: Path, rel: str) -> Path:
    out = (base / rel).resolve()
    if base.resolve() != out and base.resolve() not in out.parents:
        raise renpy.RenpyError(f"unsafe path in the SDK archive: {rel}")
    return out


README = {
    "windows": """\
{title}: Windows build made by Porting Toolkit

Start {start}. Saves go to %APPDATA%\\RenPy\\<game>. Windows may warn about an
unknown publisher the first time: choose "More info", then "Run anyway".
""",
    "linux": """\
{title}: Linux build made by Porting Toolkit

Start {start} (if it does not start: chmod +x {start}). Saves go to
~/.renpy/<game>.
""",
}
