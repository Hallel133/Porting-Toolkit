"""The API that import and export plugins are written against (``api = 1``).

A plugin's entry class gets one :class:`Context` and returns a dict:

```python
class Plugin:
    def run(self, ctx):
        ctx.progress(0.5, "Copying files")
        out = ctx.output / "MyGame.app"
        ...
        return {"outputs": [str(out)], "summary": "Built MyGame.app"}
```

The helpers check what the plugin declared in ``[permissions]``: writing
outside the declared folders, downloading from an undeclared host or
starting a program without ``subprocess = true`` raises
:class:`PermissionDenied`. This is a guard rail for honest plugins, not a
sandbox (plain ``open()`` is not intercepted); see docs/plugins.md.
"""

from __future__ import annotations

import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path
from urllib.parse import urlparse

API_VERSION = 1


class PluginError(Exception):
    """A failure the plugin explains to the user (no traceback shown)."""


class PermissionDenied(PluginError):
    """The plugin tried something its plugin.toml does not declare."""


def _emit(kind: str, **fields) -> None:
    """Sends one message to the runtime (the host's stdout is the channel)."""
    stream = _CHANNEL or sys.stdout
    stream.write(json.dumps({"type": kind, **fields}, ensure_ascii=False) + "\n")
    stream.flush()


_CHANNEL = None


def _within(path: Path, roots: list[Path]) -> bool:
    p = Path(os.path.abspath(path))
    for r in roots:
        r = Path(os.path.abspath(r))
        if p == r or r in p.parents:
            return True
    return False


class Context:
    """Everything a plugin may use. Built by the runtime from a request."""

    def __init__(self, request: dict):
        self.request = request
        self.kind: str = request["kind"]
        self.plugin: dict = request.get("plugin", {})
        self.engine: str | None = request.get("engine")
        self.target: dict = request.get("target") or {}
        self.manifest: dict = request.get("manifest") or {}
        self.settings: dict = {str(k): str(v) for k, v in (request.get("settings") or {}).items()}
        folders = request["folders"]
        self.game = Path(folders["$GAME"]) if folders.get("$GAME") else None
        self.project = Path(folders["$PROJECT"]) if folders.get("$PROJECT") else None
        self.output = Path(folders["$OUTPUT"]) if folders.get("$OUTPUT") else None
        self.cache = Path(folders["$CACHE"])
        self.temp = Path(folders["$TEMP"])
        self.temp.mkdir(parents=True, exist_ok=True)  # $CACHE is created when used
        self.tools_dir = Path(folders["$TOOLS"]) if folders.get("$TOOLS") else None
        self.media_cache = Path(folders["$MEDIA"]) if folders.get("$MEDIA") else None
        perms = request.get("permissions") or {}
        self._write_roots = [self.cache, self.temp] + self._resolve(perms.get("write", []), folders)
        self._toolbox = None
        net = perms.get("network", [])
        self._network: list[str] = ["*"] if net is True else ([] if net is False else list(net))
        self._subprocess = bool(perms.get("subprocess", False))
        self.warnings: list[str] = []

    @staticmethod
    def _resolve(values: list[str], folders: dict) -> list[Path]:
        out = []
        for v in values:
            if v.startswith("$"):
                var, _, rest = v.partition("/")
                base = folders.get(var)
                if base:
                    out.append(Path(base) / rest if rest else Path(base))
            else:
                out.append(Path(v).expanduser())
        return out

    # -- reporting ---------------------------------------------------------

    @property
    def title(self) -> str:
        game = self.manifest.get("game") or {}
        return game.get("title") or self.manifest.get("name") or "Game"

    def progress(self, fraction: float | None, message: str = "") -> None:
        """Reports progress (0..1, or None when unknown) and a status line."""
        if fraction is not None:
            fraction = max(0.0, min(1.0, float(fraction)))
        _emit("progress", fraction=fraction, message=message)

    def log(self, message: str) -> None:
        _emit("log", level="info", message=message)

    def warn(self, message: str) -> None:
        self.warnings.append(message)
        _emit("log", level="warning", message=message)

    def fail(self, message: str) -> None:
        raise PluginError(message)

    def setting(self, key: str, default: str | None = None) -> str | None:
        return self.settings.get(key, default)

    def flag(self, key: str, default: bool = False) -> bool:
        v = self.settings.get(key)
        if v is None:
            return default
        return v.strip().lower() in ("1", "true", "yes", "on")

    def art(self, kind: str) -> Path | None:
        """The project's ``icon`` or ``poster`` image (chosen by the user), if any."""
        rel = (self.manifest.get("game") or {}).get(kind)
        if not rel or not self.project:
            return None
        p = self.project / rel
        return p if p.is_file() else None

    # -- project recipes (what an import plugin learned) -------------------

    def recipe(self, name: str) -> dict | None:
        if not self.project:
            return None
        p = self.project / "recipes" / f"{name}.json"
        try:
            return json.loads(p.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None

    def save_recipe(self, name: str, data: dict) -> Path:
        if not self.project:
            self.fail("no project folder")
        p = self.project / "recipes" / f"{name}.json"
        self.check_write(p)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        return p

    # -- guarded helpers ---------------------------------------------------

    def check_write(self, path: Path) -> Path:
        if not _within(path, self._write_roots):
            raise PermissionDenied(f"the plugin may not write {path} (not in [permissions] write)")
        return path

    def check_host(self, url: str) -> None:
        u = urlparse(url)
        if u.scheme == "file":
            return
        if u.scheme != "https":
            raise PermissionDenied(f"only https downloads are allowed, not {url}")
        host = (u.hostname or "").lower()
        for allowed in self._network:
            allowed = allowed.lower().split(":")[0]
            if allowed == "*" or host == allowed or (allowed.startswith("*.") and host.endswith(allowed[1:])):
                return
        raise PermissionDenied(f"the plugin may not contact {host} (not in [permissions] network)")

    def run(self, args: list[str], cwd: Path | None = None, check: bool = True,
            env: dict | None = None) -> subprocess.CompletedProcess:
        """Starts a program (needs ``subprocess = true``); ``env`` adds variables."""
        if not self._subprocess:
            raise PermissionDenied(f"the plugin may not start programs ({args[0]})")
        full_env = {**os.environ, **env} if env else None
        r = subprocess.run([str(a) for a in args], cwd=cwd, capture_output=True, text=True, env=full_env)
        if check and r.returncode != 0:
            detail = (r.stderr or r.stdout).strip().splitlines()[-5:]
            raise PluginError(f"{Path(str(args[0])).name} failed: " + " / ".join(detail))
        return r

    def which(self, name: str) -> str | None:
        return shutil.which(name)

    def copy_tree(self, src: Path, dst: Path, skip=None) -> int:
        """Copies a folder (no links), skipping names for which ``skip`` is true."""
        self.check_write(dst)
        n = 0
        dst.mkdir(parents=True, exist_ok=True)
        for entry in sorted(Path(src).iterdir()):
            if skip and skip(entry):
                continue
            if entry.is_symlink():
                continue
            target = dst / entry.name
            if entry.is_dir():
                n += self.copy_tree(entry, target, skip)
            elif entry.is_file():
                shutil.copy2(entry, target)
                n += 1
        return n

    def write_text(self, path: Path, text: str) -> Path:
        self.check_write(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def write_bytes(self, path: Path, data: bytes) -> Path:
        self.check_write(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    # -- assets ("advanced options") ---------------------------------------

    @property
    def toolbox(self):
        """The compression tools (installed ones first, then pinned downloads)."""
        if self._toolbox is None:
            from .tools import Toolbox
            overrides = {k[5:]: v for k, v in self.settings.items() if k.startswith("tool.")}
            self._toolbox = Toolbox(self.tools_dir, overrides, auto_install=self.flag("download_tools", True),
                                    log=self.log)
        return self._toolbox

    def tool(self, tool_id: str) -> str | None:
        """Path of a compression tool (see ``ptk tools``), or None."""
        return self.toolbox.path(tool_id)

    @property
    def assets_mode(self) -> str:
        """``apply`` (default), ``plan`` (estimate only) or ``off`` (``--set assets=``)."""
        mode = (self.setting("assets") or "apply").strip().lower()
        return mode if mode in ("apply", "plan", "off") else "apply"

    def process_assets(self, root: Path, profile: str) -> dict | None:
        """Applies the project's advanced options to the files under ``root``.

        ``root`` must be the plugin's own copy in ``$OUTPUT`` (it is changed
        in place), except in ``plan`` mode, which only reads. ``profile``
        names the engine's rules (``html5``, ``rpgmaker-mz``, ``renpy``...,
        see ``ptk_engine.media.profiles``). Returns the report to put in the
        result under ``"assets"`` (None when every option is "keep").
        """
        from .media import Pipeline, describe
        mode = self.assets_mode
        if mode == "apply":
            self.check_write(root)
        pipeline = Pipeline(self.toolbox, self.media_cache,
                            progress=lambda f, m: self.progress(f, m), warn=self.warn)
        report = pipeline.run(Path(root), profile, self.manifest.get("options") or {}, self.target, mode)
        if report:
            self.log("Assets: " + describe(report))
            for cat, r in report["categories"].items():
                if r["failed"]:
                    self.warn(f"{r['failed']} {cat} files could not be re-encoded and were kept as they were.")
        return report

    def zip_dir(self, src: Path, dest: Path, executables: set[str] | None = None) -> Path:
        """Zips a folder (sorted, links skipped); ``executables`` (paths
        relative to ``src``) get mode 0755, everything else 0644."""
        self.check_write(dest)
        dest.parent.mkdir(parents=True, exist_ok=True)
        executables = executables or set()
        base = Path(src)
        with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as zf:
            for p in sorted(base.rglob("*")):
                if p.is_symlink() or not p.is_file() or p.name == self.MARKER:
                    continue
                rel = p.relative_to(base.parent).as_posix()
                info = zipfile.ZipInfo(rel, date_time=(2020, 1, 1, 0, 0, 0))
                mode = 0o755 if p.relative_to(base).as_posix() in executables or os.access(p, os.X_OK) else 0o644
                info.external_attr = (0o100000 | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                info.file_size = p.stat().st_size
                with open(p, "rb") as f, zf.open(info, "w", force_zip64=info.file_size > 0x7FFF0000) as out:
                    shutil.copyfileobj(f, out, 1 << 20)
        return dest

    MARKER = ".ptk-output"

    def _marker(self, path: Path) -> Path:
        if path.suffix == ".app":
            return path / "Contents" / "Resources" / self.MARKER
        return path / self.MARKER

    def clean_dir(self, path: Path) -> Path:
        """Empties (or creates) an output folder and marks it as ptk's.

        A folder that is not empty is only removed when ptk made it (it has
        a ``.ptk-output`` marker), so a wrong output path cannot wipe files.
        """
        self.check_write(path)
        if path.is_symlink() or path.is_file():
            self.fail(f"{path} exists and is not a folder")
        if path.exists() and any(path.iterdir()):
            if not self._marker(path).is_file():
                self.fail(f"{path} is not empty and was not made by Porting Toolkit; choose another output folder")
            shutil.rmtree(path)
        path.mkdir(parents=True, exist_ok=True)
        marker = self._marker(path)
        marker.parent.mkdir(parents=True, exist_ok=True)
        marker.write_text("Made by Porting Toolkit. This folder is replaced on the next export.\n", encoding="utf-8")
        return path

    # -- network -----------------------------------------------------------

    def download(self, url: str, dest: Path | None = None, sha256: str | None = None) -> Path:
        """Downloads with the system curl (it uses the OS certificate store).

        Files land in ``$CACHE/downloads`` unless ``dest`` is given, and a
        cached copy is reused when its SHA-256 matches (or no hash is given).
        """
        self.check_host(url)
        name = Path(urlparse(url).path).name or "download"
        dest = dest or self.cache / "downloads" / name
        self.check_write(dest)
        if dest.is_file() and (sha256 is None or _sha256(dest) == sha256.lower()):
            return dest
        dest.parent.mkdir(parents=True, exist_ok=True)
        partial = dest.with_name(dest.name + ".partial")
        self.log(f"Downloading {url}")
        r = subprocess.run(_curl() + ["-o", str(partial), url], capture_output=True, text=True)
        if r.returncode != 0:
            partial.unlink(missing_ok=True)
            raise PluginError(f"download failed: {url} ({r.stderr.strip() or r.returncode})")
        if sha256 and _sha256(partial) != sha256.lower():
            partial.unlink(missing_ok=True)
            raise PluginError(f"download of {url} does not match its SHA-256")
        partial.replace(dest)
        return dest

    def remote_zip(self, url: str) -> zipfile.ZipFile:
        """Opens a zip on a web server without downloading all of it.

        Only the central directory and the members you read are fetched
        (HTTP range requests), so taking two files out of a 1 GB archive
        costs only those files. Each member is checked against its CRC-32.
        """
        self.check_host(url)
        return zipfile.ZipFile(RemoteFile(url))


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _curl() -> list[str]:
    curl = shutil.which("curl")
    if not curl:
        raise PluginError("curl is needed for downloads and was not found")
    return [curl, "-fsSL", "--proto", "=https,file", "--retry", "2", "--connect-timeout", "20"]


class RemoteFile(io.RawIOBase):
    """A read-only, seekable view of a remote file, using HTTP ranges."""

    BLOCK = 1 << 20

    def __init__(self, url: str):
        super().__init__()
        r = subprocess.run(_curl() + ["-I", "-o", "/dev/null", "-w", "%{url_effective}", url],
                           capture_output=True, text=True)
        if r.returncode != 0 or not r.stdout.strip():
            raise PluginError(f"cannot reach {url} ({r.stderr.strip() or r.returncode})")
        self.url = r.stdout.strip()
        head = subprocess.run(_curl() + ["-I", self.url], capture_output=True, text=True).stdout
        size = None
        ranges = False
        for line in head.splitlines():
            k, _, v = line.partition(":")
            if k.strip().lower() == "content-length":
                size = int(v.strip())
            if k.strip().lower() == "accept-ranges" and "bytes" in v.lower():
                ranges = True
        if size is None or not ranges:
            raise PluginError(f"{url} does not support partial downloads")
        self.size = size
        self.pos = 0
        self._buf_start = 0
        self._buf = b""
        self.fetched = 0

    def readable(self) -> bool:
        return True

    def seekable(self) -> bool:
        return True

    def tell(self) -> int:
        return self.pos

    def seek(self, offset: int, whence: int = 0) -> int:
        if whence == 0:
            self.pos = offset
        elif whence == 1:
            self.pos += offset
        else:
            self.pos = self.size + offset
        return self.pos

    def _fetch(self, start: int, end: int) -> bytes:
        r = subprocess.run(_curl() + ["-r", f"{start}-{end}", self.url], capture_output=True)
        if r.returncode != 0 or len(r.stdout) != end - start + 1:
            raise PluginError(f"partial download failed ({r.stderr.decode(errors='replace').strip()})")
        self.fetched += len(r.stdout)
        return r.stdout

    def read(self, n: int = -1) -> bytes:
        if n is None or n < 0:
            n = self.size - self.pos
        n = min(n, self.size - self.pos)
        if n <= 0:
            return b""
        start, end = self.pos, self.pos + n
        buf_end = self._buf_start + len(self._buf)
        if not (self._buf_start <= start and end <= buf_end):
            want = max(n, self.BLOCK) if n < self.BLOCK else n
            fetch_end = min(self.size, start + want) - 1
            self._buf = self._fetch(start, fetch_end)
            self._buf_start = start
        data = self._buf[start - self._buf_start:end - self._buf_start]
        self.pos += len(data)
        return data

    def readinto(self, b) -> int:
        data = self.read(len(b))
        b[: len(data)] = data
        return len(data)
