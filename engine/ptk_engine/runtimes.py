"""Open-source runtimes for export plugins (ScummVM, source ports, GemRB...).

A runtime plugin pairs the game's own data with an open-source program
that runs it. Everything a plugin needs is described in a ``runtime.toml``
next to its ``plugin.toml``::

    [runtime]
    id = "scummvm"
    name = "ScummVM"
    version = "2.9.1"
    source = "https://github.com/scummvm/scummvm"
    license = "GPL-3.0-or-later"

    [runtime.executable]          # inside the runtime, per OS
    macos = "ScummVM.app"         # an app: its Info.plist names the binary
    windows = "scummvm.exe"
    linux = "scummvm.AppImage"

    [runtime.installed]           # copies to use before downloading
    macos = ["/Applications/ScummVM.app", "~/Applications/ScummVM.app"]

    [download.macos-arm64]        # or [download.macos] for every Mac
    url = "https://..."
    sha256 = "..."
    root = "ScummVM.app"          # the runtime inside the archive

    [launch]
    args = ["--path={game}", "--auto-detect"]
    env = { SOME_DIR = "{game}" }  # optional environment variables

    [game]
    required = ["*.000", "*.la0"] # at least one must exist (globs, any depth)
    exclude = ["*.exe", "*.dll"]  # left out of the build
    main = ["*.wad"]              # {main}: the first match, as a path in {game}
    dir = "game"                  # "runtime" merges the game into the runtime's folder

``{game}`` is the game folder of the build and ``{runtime}`` the runtime's
folder, both resolved when the game starts, so a build can be moved.

Where a runtime comes from, in order: ``--set runtime.<target>=PATH`` or
``--set runtime=PATH`` (remembered on this computer for that target; a
``runtime`` for another platform is skipped), an installed copy (macOS apps only: a
system install on Windows or Linux is not portable), then the pinned
download, checked against its SHA-256 and unpacked once into the plugin's
cache. Homebrew builds are refused for macOS builds: they load libraries
from /opt/homebrew, which other Macs do not have.

Builds:

- macOS: ``<Title>.app`` whose executable is a small shell launcher; the
  runtime is in ``Contents/Resources/runtime`` and the game in
  ``Contents/Resources/game``. Signed ad hoc when it changed.
- Windows: ``<Title>/`` with ``runtime/``, ``game/`` and ``<Title>.cmd``,
  plus a zip.
- Linux: ``<Title>/`` with ``runtime/``, ``game/`` and ``<Title>.sh``, plus a
  zip.

Every build carries ``LICENSE-<runtime>.txt`` with the runtime's licence and
source address, which GPL runtimes require when they are passed on.
"""

from __future__ import annotations

import fnmatch
import os
import plistlib
import re
import shutil
import stat
import tarfile
import tomllib
import zipfile
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath

from .api import Context, PluginError

HOMEBREW_PREFIXES = ("/opt/homebrew/", "/usr/local/Cellar/", "/usr/local/opt/", "/home/linuxbrew/")
ARCHIVES = (".zip", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".dmg", ".appimage", ".exe")


class RuntimeError_(PluginError):
    """A runtime that cannot be found, unpacked or used."""


@dataclass
class RuntimeSpec:
    id: str
    name: str
    version: str
    source: str
    license: str
    executable: dict[str, str]
    installed: dict[str, list[str]] = field(default_factory=dict)
    downloads: dict[str, dict] = field(default_factory=dict)
    args: list[str] = field(default_factory=list)
    env: dict[str, str] = field(default_factory=dict)
    required: list[str] = field(default_factory=list)
    exclude: list[str] = field(default_factory=list)
    main: list[str] = field(default_factory=list)
    game_dir: str = "game"
    #: The whole folder around the program is the runtime (its data folders
    #: sit next to it), also on macOS where the program is an app.
    whole_folder: bool = False
    #: The program is one file (a portable .exe): copy only it, not its folder.
    single_file: bool = False

    @classmethod
    def load(cls, path: Path) -> RuntimeSpec:
        data = tomllib.loads(Path(path).read_text(encoding="utf-8"))
        rt = data.get("runtime") or {}
        for key in ("id", "name", "version", "source", "license", "executable"):
            if not rt.get(key):
                raise RuntimeError_(f"{path}: [runtime] {key} is missing")
        game = data.get("game") or {}
        return cls(
            id=rt["id"], name=rt["name"], version=str(rt["version"]), source=rt["source"], license=rt["license"],
            executable=dict(rt["executable"]),
            installed={k: list(v) for k, v in (rt.get("installed") or {}).items()},
            downloads={k: dict(v) for k, v in (data.get("download") or {}).items()},
            args=list((data.get("launch") or {}).get("args") or []),
            env={str(k): str(v) for k, v in ((data.get("launch") or {}).get("env") or {}).items()},
            required=list(game.get("required") or []), exclude=list(game.get("exclude") or []),
            main=list(game.get("main") or []), game_dir=game.get("dir", "game"),
            whole_folder=bool(rt.get("whole_folder", False)),
            single_file=bool(rt.get("single_file", False)),
        )

    def download_for(self, target: dict) -> dict | None:
        """The pinned download for a target (``macos-arm64``, then ``macos``)."""
        return self.downloads.get(target.get("id", "")) or self.downloads.get(target.get("os", ""))

    def targets(self) -> list[str]:
        return sorted(self.downloads)


# -- names --------------------------------------------------------------------------------------------------------

def safe_name(title: str) -> str:
    return re.sub(r"[^A-Za-z0-9 _.-]+", "", title).strip(" .")[:64] or "Game"


def slug(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "game"


# -- game files ---------------------------------------------------------------------------------------------------

def _match(rel: str, patterns: list[str]) -> bool:
    low = rel.lower()
    name = low.rsplit("/", 1)[-1]
    return any(fnmatch.fnmatchcase(name, p.lower()) or fnmatch.fnmatchcase(low, p.lower()) for p in patterns)


def game_files(game: Path, spec: RuntimeSpec) -> list[str]:
    """The game's files (relative, sorted) that go into the build."""
    out = []
    for dirpath, dirnames, filenames in os.walk(game):
        dirnames[:] = sorted(d for d in dirnames if not d.startswith(".") and d != "__MACOSX")
        for name in sorted(filenames):
            if name.startswith(".") or name in ("Thumbs.db", "desktop.ini"):
                continue
            rel = (Path(dirpath) / name).relative_to(game).as_posix()
            if spec.exclude and _match(rel, spec.exclude):
                continue
            out.append(rel)
    return out


def required_found(files: list[str], spec: RuntimeSpec) -> str | None:
    """The first file matching ``[game] required`` (None: not this kind of game)."""
    if not spec.required:
        return files[0] if files else None
    return next((f for f in files if _match(f, spec.required)), None)


def main_file(files: list[str], spec: RuntimeSpec) -> str | None:
    """``{main}``: the first file matching the ``[game] main`` patterns, in pattern order."""
    for pattern in spec.main:
        hit = next((f for f in files if _match(f, [pattern])), None)
        if hit:
            return hit
    return None


# -- finding the runtime --------------------------------------------------------------------------------------------

def is_homebrew(path: Path) -> bool:
    p = str(Path(path).expanduser())
    return any(p.startswith(prefix) for prefix in HOMEBREW_PREFIXES)


def executable_in(root: Path, spec: RuntimeSpec, os_: str) -> Path | None:
    """The runtime's program inside ``root`` (a folder, an app or a file)."""
    want = spec.executable.get(os_)
    if not want:
        return None
    root = Path(root)
    if any(c in want for c in "*?["):
        return _glob_executable(root, want)
    if root.is_file():
        return root if root.name.lower() == Path(want).name.lower() or os_ == "linux" else None
    if root.suffix == ".app" and want.endswith(".app"):
        return _app_binary(root)
    candidate = root / want
    if candidate.suffix == ".app":
        return _app_binary(candidate)
    if candidate.is_file():
        return candidate
    # the archive may hold one extra folder level
    for sub in sorted(p for p in root.iterdir() if p.is_dir()) if root.is_dir() else []:
        inner = sub / want
        if inner.suffix == ".app":
            found = _app_binary(inner)
            if found:
                return found
        elif inner.is_file():
            return inner
    return None


def _glob_executable(root: Path, pattern: str) -> Path | None:
    """For names with a wildcard (``*.app``, ``*.AppImage``): ``root`` itself,
    or the first match in it or one folder below."""
    low = pattern.lower()

    def resolve(p: Path) -> Path | None:
        if p.suffix == ".app":
            return _app_binary(p)
        return p if p.is_file() else None

    if fnmatch.fnmatchcase(root.name.lower(), low):
        return resolve(root)
    if not root.is_dir():
        return None
    levels = [root] + sorted(p for p in root.iterdir() if p.is_dir() and p.suffix != ".app")
    for folder in levels:
        # The shortest match first: "fs2_open_x64.exe" before "fs2_open_x64-FASTDBG.exe".
        matches = sorted((p for p in folder.iterdir()
                          if not p.is_symlink() and fnmatch.fnmatchcase(p.name.lower(), low)),
                         key=lambda p: (len(p.name), p.name))
        for p in matches:
            if found := resolve(p):
                return found
    return None


def _app_binary(app: Path) -> Path | None:
    plist = app / "Contents" / "Info.plist"
    if not plist.is_file():
        return None
    try:
        name = plistlib.loads(plist.read_bytes()).get("CFBundleExecutable")
    except Exception:  # noqa: BLE001 - a broken plist is "not an app"
        return None
    binary = app / "Contents" / "MacOS" / str(name or "")
    return binary if name and binary.is_file() else None


def runtime_root(executable: Path, os_: str, whole_folder: bool = False, single_file: bool = False) -> Path:
    """What goes into the build: the app (macOS), the program's folder, or the
    single file (an AppImage, or any program with ``single_file``).
    ``whole_folder``: the folder around the app."""
    if os_ == "macos" and len(executable.parents) > 2 and executable.parents[2].suffix == ".app":
        return executable.parents[3] if whole_folder else executable.parents[2]  # X.app/Contents/MacOS/x
    if (os_ == "linux" and executable.suffix.lower() == ".appimage") or single_file:
        return executable
    return executable.parent


@dataclass
class Found:
    root: Path  # what is copied into the build
    executable: Path
    how: str  # "setting", "remembered", "installed", "download", "cached"


def _remembered(ctx: Context, target: dict) -> Path:
    return ctx.cache / f"runtime-{target.get('id', 'target')}.txt"


def locate(ctx: Context, spec: RuntimeSpec, target: dict, download: bool) -> Found | None:
    """Finds the runtime for ``target``. With ``download=False`` (checks), a
    pinned download that is not cached yet is reported as None plus
    :func:`can_download`."""
    os_ = target.get("os", "")
    key = f"runtime.{target.get('id', os_)}"
    missed = None  # a general runtime=PATH that is for another platform
    for setting, strict in ((key, True), ("runtime", False)):
        chosen = ctx.setting(setting)
        if not chosen:
            continue
        p = Path(chosen).expanduser()
        if os_ == "macos" and is_homebrew(p):
            raise RuntimeError_(
                f"{chosen} is a Homebrew build: it loads libraries from Homebrew, so the app would not start on "
                f"other Macs. Use the official {spec.name} app instead ({spec.source}).")
        found = _from_path(ctx, p, spec, os_, unpack=download)
        if found is None:
            message = (f"{setting}={chosen} is not {spec.name} for {target.get('name') or os_} "
                       f"(expected {spec.executable.get(os_, '?')}).")
            if strict:
                raise RuntimeError_(message)
            missed = message  # runtime= may be meant for another target: try the other ways first
            continue
        if download:
            ctx.write_text(_remembered(ctx, target), str(p) + "\n")
        return Found(runtime_root(found.executable, os_, spec.whole_folder, spec.single_file), found.executable, "setting")
    found = _locate_other(ctx, spec, target, download)
    if found is None and missed and spec.download_for(target) is None:
        raise RuntimeError_(missed + f" Use {key}=PATH to give one per target.")
    return found


def _locate_other(ctx: Context, spec: RuntimeSpec, target: dict, download: bool) -> Found | None:
    os_ = target.get("os", "")
    rem = _remembered(ctx, target)
    if rem.is_file():
        p = Path(rem.read_text(encoding="utf-8").strip())
        found = _from_path(ctx, p, spec, os_, unpack=download) if p.exists() else None
        if found:
            return Found(runtime_root(found.executable, os_, spec.whole_folder, spec.single_file), found.executable, "remembered")
    for guess in spec.installed.get(os_, []):
        p = Path(guess).expanduser()
        if p.exists() and not (os_ == "macos" and is_homebrew(p)):
            exe = executable_in(p, spec, os_)
            if exe:
                return Found(runtime_root(exe, os_, spec.whole_folder, spec.single_file), exe, "installed")
    pin = spec.download_for(target)
    if pin is None:
        return None
    cached = _cache_dir(ctx, spec, target)
    if (cached / ".complete").is_file():
        exe = executable_in(cached / "runtime", spec, os_)
        if exe:
            return Found(runtime_root(exe, os_, spec.whole_folder, spec.single_file), exe, "cached")
    if not download:
        return None
    return _download(ctx, spec, target, pin)


def can_download(spec: RuntimeSpec, target: dict) -> bool:
    return spec.download_for(target) is not None


def _cache_dir(ctx: Context, spec: RuntimeSpec, target: dict) -> Path:
    return ctx.cache / "runtimes" / f"{spec.id}-{spec.version}-{target.get('id', target.get('os', 'any'))}"


def _from_path(ctx: Context, p: Path, spec: RuntimeSpec, os_: str, unpack: bool) -> Found | None:
    if not p.exists():
        return None
    lower = p.name.lower()
    if p.is_file() and lower.endswith(ARCHIVES) and not lower.endswith((".appimage", ".exe")):
        if not unpack:
            return Found(p, p, "setting")  # checked when unpacked
        st = p.stat()
        dest = ctx.cache / "runtimes" / f"local-{slug(p.stem)}-{st.st_size}-{st.st_mtime_ns}"
        if not (dest / ".complete").is_file():
            unpack_archive(ctx, p, dest / "runtime")
            (dest / ".complete").write_text(p.name + "\n")
        p = dest / "runtime"
    exe = executable_in(p, spec, os_)
    return Found(runtime_root(exe, os_), exe, "setting") if exe else None


def _download(ctx: Context, spec: RuntimeSpec, target: dict, pin: dict) -> Found:
    os_ = target.get("os", "")
    url, sha = pin.get("url"), pin.get("sha256")
    if not url or not sha:
        raise RuntimeError_(f"runtime.toml: the {target.get('id')} download needs url and sha256")
    ctx.progress(0.1, f"Downloading {spec.name} {spec.version} for {target.get('name') or os_} (once)")
    archive = ctx.download(url, sha256=sha)
    dest = _cache_dir(ctx, spec, target)
    shutil.rmtree(dest, ignore_errors=True)
    unpack_archive(ctx, archive, dest / "runtime")
    root = dest / "runtime"
    if pin.get("root"):
        root = root / pin["root"]
    exe = executable_in(root, spec, os_) or executable_in(dest / "runtime", spec, os_)
    if exe is None:
        raise RuntimeError_(f"{archive.name} has no {spec.executable.get(os_)}")
    (dest / ".complete").write_text(archive.name + "\n")
    return Found(runtime_root(exe, os_, spec.whole_folder, spec.single_file), exe, "download")


# -- unpacking --------------------------------------------------------------------------------------------------

def _safe_join(root: Path, rel: str) -> Path:
    pure = PurePosixPath(rel.replace("\\", "/"))
    if pure.is_absolute() or ".." in pure.parts:
        raise RuntimeError_(f"unsafe path in archive: {rel}")
    return root.joinpath(*pure.parts)


def unpack_archive(ctx: Context, archive: Path, dest: Path) -> None:
    """Unpacks a zip, tarball, .dmg or AppImage into ``dest``, keeping
    executable bits and the symlinks inside macOS frameworks (links must
    stay inside ``dest``). Nothing is run."""
    name = archive.name.lower()
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    if name.endswith(".zip"):
        _unzip(archive, dest)
    elif name.endswith((".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2")):
        with tarfile.open(archive) as t:
            t.extractall(dest, filter="data")
    elif name.endswith((".appimage", ".exe")):  # a single-file program (portable .exe)
        out = dest / archive.name
        shutil.copyfile(archive, out)
        out.chmod(0o755)
    elif name.endswith(".dmg"):
        _undmg(ctx, archive, dest)
    else:
        raise RuntimeError_(f"{archive.name}: unsupported archive (zip, tar.gz, tar.xz, dmg or AppImage)")


def _unzip(archive: Path, dest: Path) -> None:
    with zipfile.ZipFile(archive) as z:
        for i in z.infolist():
            if i.filename.startswith("__MACOSX/") or i.is_dir():
                continue
            out = _safe_join(dest, i.filename)
            out.parent.mkdir(parents=True, exist_ok=True)
            mode = i.external_attr >> 16
            if stat.S_ISLNK(mode):
                target = z.read(i).decode("utf-8")
                if os.path.isabs(target):
                    raise RuntimeError_(f"absolute link in archive: {i.filename}")
                _safe_join(dest, str(PurePosixPath(i.filename).parent / target))
                if out.is_symlink() or out.exists():
                    out.unlink()
                os.symlink(target, out)
                continue
            with z.open(i) as f, open(out, "wb") as o:
                shutil.copyfileobj(f, o, 1 << 20)
            if mode & 0o111:
                out.chmod(0o755)


def _undmg(ctx: Context, archive: Path, dest: Path) -> None:
    """Copies the apps out of a disk image (mounted read-only, never opened)."""
    if not ctx.which("hdiutil"):
        raise RuntimeError_(f"{archive.name} is a disk image, which only macOS can open")
    mount = ctx.temp / f"dmg-{os.getpid()}"
    mount.mkdir(parents=True, exist_ok=True)
    ctx.run(["hdiutil", "attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint", mount, archive])
    try:
        apps = sorted(p for p in mount.iterdir() if p.suffix == ".app" and not p.is_symlink())
        if not apps:
            raise RuntimeError_(f"{archive.name} holds no app")
        for app in apps:
            shutil.copytree(app, dest / app.name, symlinks=True)
    finally:
        ctx.run(["hdiutil", "detach", mount, "-force"], check=False)


# -- launchers ----------------------------------------------------------------------------------------------------

_TOKEN = re.compile(r"\{(game|runtime|main)\}")


def _sh_arg(arg: str, values: dict[str, str]) -> str:
    """One argument for sh: literal text escaped, placeholders as variables."""
    out, pos = ['"'], 0
    for m in _TOKEN.finditer(arg):
        out.append(_sh_escape(arg[pos:m.start()]))
        out.append(values[m.group(1)])
        pos = m.end()
    out.append(_sh_escape(arg[pos:]))
    out.append('"')
    return "".join(out)


def _sh_escape(text: str) -> str:
    return re.sub(r'([\\"$`])', r"\\\1", text)


def sh_launcher(exe_rel: str, args: list[str], main: str | None, resources: str,
                env: dict[str, str] | None = None, game_dir: str = "game") -> str:
    """A POSIX shell launcher. ``resources`` is the folder holding runtime/ and
    game/, relative to the launcher's own folder."""
    values = {"game": '${GAME}', "runtime": '${RUNTIME}', "main": '${GAME}/' + _sh_escape(main or "")}
    line = " ".join([f'"${{RUNTIME}}/{_sh_escape(exe_rel)}"', *(_sh_arg(a, values) for a in args), '"$@"'])
    exports = "".join(f"export {k}={_sh_arg(v, values)}\n" for k, v in sorted((env or {}).items())
                      if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k))
    return ("#!/bin/sh\n"
            "# Made by Porting Toolkit: starts the game with its open-source runtime.\n"
            'HERE="$(cd "$(dirname "$0")" && pwd)"\n'
            f'BASE="$(cd "$HERE/{_sh_escape(resources)}" && pwd)"\n'
            'RUNTIME="$BASE/runtime"\n'
            f'GAME="$BASE/{_sh_escape(game_dir)}"\n'
            f"{exports}"
            'cd "$GAME" || exit 1\n'
            f"exec {line}\n")


def cmd_launcher(exe_rel: str, args: list[str], main: str | None, env: dict[str, str] | None = None,
                 game_dir: str = "game") -> str:
    """A Windows .cmd next to runtime\\ and game\\."""
    def fill(a: str) -> str:
        a = a.replace("{game}", f"%~dp0{game_dir}").replace("{runtime}", "%~dp0runtime")
        return a.replace("{main}", f"%~dp0{game_dir}\\" + (main or "").replace("/", "\\"))

    def win(a: str) -> str:
        a = fill(a)
        return f'"{a}"' if (" " in a or "%~dp0" in a) else a
    exe = "%~dp0runtime\\" + exe_rel.replace("/", "\\")
    line = " ".join(["start", '""', f'"{exe}"', *(win(a) for a in args), "%*"])
    sets = "".join(f'set "{k}={fill(v)}"\r\n' for k, v in sorted((env or {}).items())
                   if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k))
    return ("@echo off\r\n"
            "rem Made by Porting Toolkit: starts the game with its open-source runtime.\r\n"
            f"{sets}"
            f'cd /d "%~dp0{game_dir}"\r\n'
            f"{line}\r\n")


def licence_note(spec: RuntimeSpec) -> str:
    return (f"{spec.name} {spec.version}\n"
            f"Licence: {spec.license}\n"
            f"Source code: {spec.source}\n\n"
            f"This build includes {spec.name}, an open-source program that runs the game's data.\n"
            f"{spec.name} is free software: you may redistribute and change it under its licence above;\n"
            "its complete source code for this version is available at the address above.\n"
            "The game's own files are not covered by this licence and belong to their owners.\n")


def info_plist(title: str, executable: str, bundle_id: str, version: str, icon: str | None) -> bytes:
    plist = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": title,
        "CFBundleExecutable": executable,
        "CFBundleIdentifier": bundle_id,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": title[:15],
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": "11.0",
        "NSHighResolutionCapable": True,
        "LSApplicationCategoryType": "public.app-category.games",
    }
    if icon:
        plist["CFBundleIconFile"] = icon
    return plistlib.dumps(plist)


def app_icon(app: Path) -> Path | None:
    """The .icns of a runtime app, to reuse for the game's app."""
    try:
        info = plistlib.loads((app / "Contents" / "Info.plist").read_bytes())
    except Exception:  # noqa: BLE001
        return None
    name = info.get("CFBundleIconFile")
    if not name:
        return None
    icns = app / "Contents" / "Resources" / (name if name.endswith(".icns") else f"{name}.icns")
    return icns if icns.is_file() else None
