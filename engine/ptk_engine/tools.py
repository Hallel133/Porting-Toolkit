"""External programs that re-encode assets (ffmpeg, cwebp, oxipng, ...).

A tool is looked up in this order:

1. ``--set tool.<id>=PATH`` (an explicit choice);
2. the ``PATH``, then ``/opt/homebrew/bin`` and ``/usr/local/bin`` (so a
   Homebrew install is found even when the app starts with a short PATH);
3. a copy Porting Toolkit downloaded before, in ``<tools>/<id>/<version>/``;
4. a pinned download: a fixed release URL whose SHA-256 is written below.
   Nothing else is ever downloaded, and a file that does not match its
   hash is thrown away.

The hashes come from ``tools/pin_tools.py`` (it reads the digests GitHub
publishes for release assets); they are never typed by hand.

The tools run as separate programs and are never copied into a game's
output, so their licences do not travel with the game.
"""

from __future__ import annotations

import os
import platform
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import zipfile
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import urlparse

#: Hosts a pinned download may come from (GitHub release assets redirect
#: to the two githubusercontent hosts).
ALLOWED_HOSTS = ("github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com")

EXTRA_DIRS = ("/opt/homebrew/bin", "/usr/local/bin")


class ToolError(Exception):
    pass


@dataclass(frozen=True)
class Pin:
    version: str
    url: str
    sha256: str
    archive: str  # "tar.gz" | "zip" | "raw"


@dataclass(frozen=True)
class Tool:
    id: str
    name: str
    binary: str
    purpose: str
    license: str
    version_args: tuple[str, ...] | None = ("-version",)
    version_re: str = r"(\d+(?:\.\d+)+)"
    homebrew: str | None = None
    macos_only: bool = False
    pins: dict[str, Pin] = field(default_factory=dict)

    def hint(self) -> str:
        if self.pins.get(platform_key()):
            return f"ptk tools install {self.id}"
        if self.macos_only:
            return "part of macOS"
        if self.homebrew:
            return f"brew install {self.homebrew}"
        return ""


def platform_key() -> str:
    os_name = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}.get(platform.system(), platform.system().lower())
    arch = platform.machine().lower()
    arch = {"aarch64": "arm64", "amd64": "x64", "x86_64": "x64"}.get(arch, arch)
    return f"{os_name}-{arch}"


_GH = "https://github.com/oxipng/oxipng/releases/download"
_ASTC = "https://github.com/ARM-software/astc-encoder/releases/download"

TOOLS: dict[str, Tool] = {t.id: t for t in [
    Tool("ffmpeg", "FFmpeg", "ffmpeg", "audio, video and JPEG", "LGPL-2.1-or-later (GPL with some codecs)",
         ("-version",), r"ffmpeg version n?([\w.\-]+)", homebrew="ffmpeg"),
    Tool("ffprobe", "FFprobe", "ffprobe", "checking audio and video after re-encoding",
         "LGPL-2.1-or-later", ("-version",), r"ffprobe version n?([\w.\-]+)", homebrew="ffmpeg"),
    Tool("cwebp", "cwebp (libwebp)", "cwebp", "WebP images", "BSD-3-Clause",
         ("-version",), r"(\d+\.\d+\.\d+)", homebrew="webp"),
    Tool("oxipng", "oxipng", "oxipng", "lossless PNG", "MIT",
         ("--version",), r"oxipng (\S+)", homebrew="oxipng", pins={
             "macos-arm64": Pin("10.2.1", f"{_GH}/v10.2.1/oxipng-10.2.1-aarch64-apple-darwin.tar.gz",
                                "7039fcfc78e8aa1ed2b57d848057a0296f082e92b3e1807ac65402d10d926764", "tar.gz"),
             "macos-x64": Pin("10.2.1", f"{_GH}/v10.2.1/oxipng-10.2.1-x86_64-apple-darwin.tar.gz",
                              "111883bbe42b25e01cb1ca41f39f8094e4830b173f0e42be342ecc4ca48f3131", "tar.gz"),
         }),
    Tool("astcenc", "astcenc (ARM ASTC Encoder)", "astcenc", "ASTC textures for phones (Godot on Android)",
         "Apache-2.0", ("-help",), r"astcenc v(\d+(?:\.\d+)+)", pins={
             "macos-arm64": Pin("5.7.1", f"{_ASTC}/5.7.1/astcenc-5.7.1-macos-universal.zip",
                                "cb2dd75e33fcc12ebaae9e2230649e30da10c152b029a6d2f3440f6d45ed5c45", "zip"),
             "macos-x64": Pin("5.7.1", f"{_ASTC}/5.7.1/astcenc-5.7.1-macos-universal.zip",
                              "cb2dd75e33fcc12ebaae9e2230649e30da10c152b029a6d2f3440f6d45ed5c45", "zip"),
         }),
    Tool("pngquant", "pngquant", "pngquant", "lossy PNG (fewer colours)", "GPL-3.0-or-later",
         ("--version",), r"(\d+\.\d+\.\d+)", homebrew="pngquant"),
    Tool("oggenc", "oggenc (vorbis-tools)", "oggenc", "Ogg Vorbis audio, when FFmpeg has no libvorbis",
         "GPL-2.0", ("--version",), r"(\d+\.\d+(?:\.\d+)?)", homebrew="vorbis-tools"),
    Tool("afconvert", "afconvert", "afconvert", "AAC audio", "part of macOS",
         None, macos_only=True),
]}


@dataclass
class Found:
    id: str
    path: str
    source: str  # "setting" | "system" | "downloaded"
    version: str | None

    def to_dict(self) -> dict:
        return {"id": self.id, "path": self.path, "source": self.source, "version": self.version}


_VERSIONS: dict[str, str | None] = {}


def _version(tool: Tool, path: str) -> str | None:
    if tool.version_args is None:
        return None
    if path in _VERSIONS:
        return _VERSIONS[path]
    try:
        r = subprocess.run([path, *tool.version_args], capture_output=True, text=True, timeout=15)
        text = (r.stdout or "") + (r.stderr or "")
        m = re.search(tool.version_re, text)
        v = m.group(1) if m else None
    except (OSError, subprocess.SubprocessError):
        v = None
    _VERSIONS[path] = v
    return v


def _executable(p: Path) -> bool:
    return p.is_file() and os.access(p, os.X_OK)


def _installed_dir(tools_dir: Path, tool: Tool, pin: Pin) -> Path:
    return tools_dir / tool.id / pin.version / platform_key()


def find(tool_id: str, tools_dir: Path | None = None, override: str | None = None,
         search_path: str | None = None) -> Found | None:
    """Where a tool is, without downloading anything."""
    tool = TOOLS[tool_id]
    if override:
        p = Path(override).expanduser()
        if not _executable(p):
            raise ToolError(f"tool.{tool_id} is set to {override}, which is not a program")
        return Found(tool_id, str(p), "setting", _version(tool, str(p)))
    if tool.macos_only and platform.system() != "Darwin":
        return None
    path = search_path if search_path is not None else os.environ.get("PATH", "")
    hit = shutil.which(tool.binary, path=os.pathsep.join([path, *EXTRA_DIRS]))
    if hit:
        return Found(tool_id, hit, "system", _version(tool, hit))
    pin = tool.pins.get(platform_key())
    if tools_dir and pin:
        p = _installed_dir(Path(tools_dir), tool, pin) / tool.binary
        if _executable(p):
            return Found(tool_id, str(p), "downloaded", pin.version)
    return None


def _check_url(url: str) -> None:
    u = urlparse(url)
    if u.scheme != "https" or (u.hostname or "").lower() not in ALLOWED_HOSTS:
        raise ToolError(f"{url} is not an allowed tool download")


def _sha256(path: Path) -> str:
    import hashlib
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _extract_binary(archive: Path, kind: str, binary: str, dest: Path) -> None:
    """Takes only the program itself out of the archive (by its file name)."""
    if kind == "raw":
        shutil.copyfile(archive, dest)
        return
    if kind == "tar.gz":
        with tarfile.open(archive, "r:gz") as tf:
            for m in tf.getmembers():
                if m.isfile() and Path(m.name).name == binary:
                    f = tf.extractfile(m)
                    assert f is not None
                    with f, open(dest, "wb") as out:
                        shutil.copyfileobj(f, out)
                    return
    elif kind == "zip":
        with zipfile.ZipFile(archive) as zf:
            for name in zf.namelist():
                if not name.endswith("/") and Path(name).name == binary:
                    with zf.open(name) as f, open(dest, "wb") as out:
                        shutil.copyfileobj(f, out)
                    return
    raise ToolError(f"{binary} was not found in the download")


def install(tool_id: str, tools_dir: Path, log=lambda m: None, fetch=None) -> Found:
    """Downloads the pinned build of a tool, checks its SHA-256 and unpacks it.

    ``fetch(url, dest)`` downloads a file (tests pass their own); the
    default uses the system curl.
    """
    tool = TOOLS[tool_id]
    existing = find(tool_id, tools_dir, search_path="")
    if existing and existing.source == "downloaded":
        return existing
    pin = tool.pins.get(platform_key())
    if not pin:
        hint = tool.hint()
        raise ToolError(f"{tool.name} has no download for {platform_key()}" + (f"; install it with: {hint}" if hint else ""))
    _check_url(pin.url)
    if not re.fullmatch(r"[0-9a-f]{64}", pin.sha256):
        raise ToolError(f"{tool.name}: the pinned SHA-256 is malformed")
    dest_dir = _installed_dir(Path(tools_dir), tool, pin)
    dest_dir.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=dest_dir.parent) as tmp:
        archive = Path(tmp) / "download"
        log(f"Downloading {tool.name} {pin.version}")
        (fetch or _curl_fetch)(pin.url, archive)
        got = _sha256(archive)
        if got != pin.sha256:
            raise ToolError(f"the {tool.name} download does not match its SHA-256 (got {got[:12]}…); nothing was installed")
        staged = Path(tmp) / "unpacked"
        staged.mkdir()
        binary = staged / tool.binary
        _extract_binary(archive, pin.archive, tool.binary, binary)
        binary.chmod(binary.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        if platform.system() == "Darwin" and shutil.which("codesign"):
            # Apple Silicon refuses to start unsigned code; sign ad-hoc if needed.
            if subprocess.run(["codesign", "-v", str(binary)], capture_output=True).returncode != 0:
                subprocess.run(["codesign", "-s", "-", "--force", str(binary)], capture_output=True)
        if dest_dir.exists():
            shutil.rmtree(dest_dir)
        shutil.move(str(staged), dest_dir)
    log(f"Installed {tool.name} {pin.version} in {dest_dir}")
    return Found(tool_id, str(dest_dir / tool.binary), "downloaded", pin.version)


def _curl_fetch(url: str, dest: Path) -> None:
    curl = shutil.which("curl")
    if not curl:
        raise ToolError("curl is needed for downloads and was not found")
    r = subprocess.run([curl, "-fsSL", "--proto", "=https", "--proto-redir", "=https", "--retry", "2",
                        "--connect-timeout", "20", "-o", str(dest), url], capture_output=True, text=True)
    if r.returncode != 0:
        raise ToolError(f"download failed: {url} ({r.stderr.strip() or r.returncode})")


def status(tools_dir: Path | None = None) -> list[dict]:
    """Every known tool: where it is (or how to get it)."""
    out = []
    key = platform_key()
    for tool in TOOLS.values():
        try:
            found = find(tool.id, tools_dir)
        except ToolError:
            found = None
        if tool.macos_only and not key.startswith("macos"):
            continue
        out.append({
            "id": tool.id,
            "name": tool.name,
            "purpose": tool.purpose,
            "license": tool.license,
            "found": found.to_dict() if found else None,
            "installable": key in tool.pins,
            "hint": tool.hint(),
        })
    return out


class Toolbox:
    """The tools one export may use: finds each once and remembers it.

    ``auto_install`` lets it download a pinned build when a tool is not
    installed (the user's choice: installed first, then a verified download).
    """

    def __init__(self, tools_dir: Path | None, overrides: dict[str, str] | None = None,
                 auto_install: bool = True, log=lambda m: None):
        self.tools_dir = Path(tools_dir) if tools_dir else None
        self.overrides = overrides or {}
        self.auto_install = auto_install
        self.log = log
        self._found: dict[str, Found | None] = {}
        self.missing: set[str] = set()

    def get(self, tool_id: str) -> Found | None:
        if tool_id in self._found:
            return self._found[tool_id]
        found = find(tool_id, self.tools_dir, self.overrides.get(tool_id))
        if found is None and self.auto_install and self.tools_dir and TOOLS[tool_id].pins.get(platform_key()):
            try:
                found = install(tool_id, self.tools_dir, self.log)
            except ToolError as e:
                self.log(str(e))
        if found is None:
            self.missing.add(tool_id)
        self._found[tool_id] = found
        return found

    def path(self, tool_id: str) -> str | None:
        f = self.get(tool_id)
        return f.path if f else None

    def version(self, tool_id: str) -> str:
        f = self.get(tool_id)
        return (f.version or "?") if f else "-"
