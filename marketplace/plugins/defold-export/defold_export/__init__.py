"""Defold export: the game's archive with the Defold engine of its version.

Defold games are data (game.projectc, game.arcd, game.arci, game.dmanifest)
for the engine they were built with: another engine version cannot read
them. Every engine embeds its version and the 40-character sha1 of its
source next to each other ("1.14.1\\0...ee283d4d..."), so the sha1 is read
from the game's own engine (only its bytes; it is never run) and the
engine for the target is downloaded from d.defold.com, which publishes
every version. Defold publishes no SHA-256 for these files: the download
relies on HTTPS, and its SHA-256 is shown in the export notes.

Settings: ``defold_sha1`` (when the game's engine cannot be read),
``runtime`` / ``runtime.<target>`` (an engine you already have).
"""

from __future__ import annotations

import dataclasses
import hashlib
import re
from pathlib import Path

from ptk_engine import runtimes
from ptk_engine.sdk import RuntimeExportPlugin, error

ARCHIVE = "https://d.defold.com/archive/stable/{sha1}/engine/{platform}/{name}"
PLATFORMS = {"macos-arm64": "arm64-macos", "macos-x64": "x86_64-macos", "windows-x64": "x86_64-win32",
             "linux-x64": "x86_64-linux"}
VERSION = re.compile(rb"(\d{1,2}\.\d{1,3}\.\d{1,3})\x00{1,32}([0-9a-f]{40})\x00")
SHA1 = re.compile(r"[0-9a-f]{40}")


def engine_candidates(game: Path, files: list[str]) -> list[Path]:
    """The game's own Defold engine: its .exe, Linux binary or Mac app binary."""
    out = []
    for f in files:
        low = f.lower()
        if low.endswith((".exe", ".x86_64")) or "/contents/macos/" in low or ("/" not in low and "." not in low):
            out.append(game / f)
    return out


def read_engine_version(paths: list[Path]) -> tuple[str, str] | None:
    """(version, sha1) found in an engine binary, or None."""
    for p in paths:
        try:
            data = p.read_bytes()
        except OSError:
            continue
        m = VERSION.search(data)
        if m:
            return m.group(1).decode(), m.group(2).decode()
    return None


class Plugin(RuntimeExportPlugin):
    def _own_runtime(self, ctx) -> bool:
        target = ctx.target or {}
        return bool(ctx.setting(f"runtime.{target.get('id')}") or ctx.setting("runtime"))

    def _version(self, ctx) -> tuple[str, str] | None:
        chosen = (ctx.setting("defold_sha1") or "").strip().lower()
        if chosen:
            return ("chosen", chosen) if SHA1.fullmatch(chosen) else None
        every = runtimes.game_files(Path(ctx.game), dataclasses.replace(self.spec(), exclude=[]))
        return read_engine_version(engine_candidates(Path(ctx.game), every))

    def check(self, ctx):
        if self._own_runtime(ctx):
            return super().check(ctx)
        target = ctx.target or {}
        if target.get("id") not in PLATFORMS:
            return [error(f"Defold engines are published for {', '.join(PLATFORMS)}, not {target.get('id')}")]
        if ctx.game is None or not Path(ctx.game).is_dir():
            return [error("choose the game folder first")]
        spec = self.spec()
        self._files = runtimes.game_files(Path(ctx.game), spec)
        if runtimes.required_found(self._files, spec) is None:
            return [error("this folder has no game.projectc; choose the folder of a Defold build")]
        self._main = runtimes.main_file(self._files, spec)
        if ctx.setting("defold_sha1") and not SHA1.fullmatch(ctx.setting("defold_sha1").strip().lower()):
            return [error("defold_sha1 must be the 40-character sha1 of a Defold release")]
        self._ver = self._version(ctx)
        if self._ver is None:
            return [error("the game's Defold engine was not found or does not name its version, so the matching "
                          "engine cannot be chosen. Pass --set defold_sha1=<sha1 of the Defold release> "
                          "(see d.defold.com) or --set runtime=PATH to a dmengine of the same version.")]
        return []

    def prepare(self, ctx):
        if self._own_runtime(ctx):
            return super().prepare(ctx)
        target = ctx.target or {}
        os_ = target.get("os")
        version, sha1 = self._ver
        name = "dmengine.exe" if os_ == "windows" else "dmengine"
        folder = ctx.cache / "engines" / sha1 / PLATFORMS[target["id"]]
        exe = folder / name
        if not exe.is_file():
            ctx.progress(0.1, f"Downloading the Defold engine {version} for {target.get('name')} (once)")
            url = ARCHIVE.format(sha1=sha1, platform=PLATFORMS[target["id"]], name=name)
            try:
                ctx.download(url, dest=exe)
            except Exception as e:  # noqa: BLE001 - a clearer message for a missing version
                raise runtimes.RuntimeError_(
                    f"Defold has no {PLATFORMS[target['id']]} engine for sha1 {sha1} ({e}). Older Defold versions "
                    "were not built for every platform (Apple Silicon engines exist only for newer releases).") from e
            exe.chmod(0o755)
        self._found = runtimes.Found(folder, exe, "download")
        self._engine_sha256 = hashlib.sha256(exe.read_bytes()).hexdigest()

    def variant(self, ctx):
        ver = getattr(self, "_ver", None)
        return super().variant(ctx) + (f"|{ver[1]}" if ver else "")

    def finish(self, ctx, build):
        result = super().finish(ctx, build)
        ver = getattr(self, "_ver", None)
        if ver and not self._own_runtime(ctx):
            built = Path(result["outputs"][0]).name
            result["summary"] = f"Built {built} with the Defold engine {ver[0]}"
            result["notes"] = [n for n in result.get("notes", []) if not n.startswith("Runs on")]
            result.setdefault("notes", []).append(
                f"Defold engine {ver[0]} ({ver[1]}) from d.defold.com; SHA-256 {self._engine_sha256}.")
        return result
