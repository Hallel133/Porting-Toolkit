"""Finding and running import/export plugins.

Plugins run in a separate Python process (``ptk_engine.host``), with the
working directory set to a fresh temporary folder. Their messages are
relayed to ptk as ``progress`` and ``log`` notifications.
"""

from __future__ import annotations

import collections
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import tomllib
from collections.abc import Callable
from pathlib import Path

from . import EngineError
from .plugins import PLUGIN_FILE, validate_plugin

#: Plugins that ship with Porting Toolkit (``engine/plugins``).
BUILTIN_DIR = Path(__file__).resolve().parent.parent / "plugins"


def builtin_dir() -> Path | None:
    return BUILTIN_DIR if BUILTIN_DIR.is_dir() else None


def is_builtin(path: str | Path) -> bool:
    b = builtin_dir()
    return b is not None and Path(path).resolve().parent == b.resolve()


def version_key(v: str | None) -> tuple[int, ...]:
    """``"1.10.0"`` -> ``(1, 10, 0)``; anything else sorts first."""
    try:
        return tuple(int(x) for x in str(v).split("-")[0].split("."))
    except ValueError:
        return ()


def _formats(path: Path) -> dict:
    try:
        data = tomllib.loads((path / PLUGIN_FILE).read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError, UnicodeDecodeError):
        return {}
    return data


def find_plugins(kind: str, engine: str | None, target: str | None, dirs: list[str],
                 known_targets: list[str] | None = None, prefer: str | None = None,
                 pins: dict[str, str] | None = None) -> list[dict]:
    """Valid plugins of ``kind`` for an engine (and, for export, a target).

    When several folders hold the same id, the newest version wins; on a
    tie user folders come first, so an installed copy replaces the
    built-in one, but an old installed copy never hides a newer built-in.
    ``pins`` (``{id: folder}``, the user's choice of version) overrides
    that when the pinned folder is still there. ``prefer`` (a target's
    ``export_backend``) moves that plugin to the front.
    """
    pinned = {k: str(Path(v).expanduser().resolve()) for k, v in (pins or {}).items()}
    search = [Path(d).expanduser() for d in dirs]
    if (b := builtin_dir()) is not None:
        search.append(b)
    best: dict[str, dict] = {}
    order: list[str] = []
    for base in search:
        if not base.is_dir():
            continue
        for child in sorted(base.iterdir()):
            if not (child / PLUGIN_FILE).is_file():
                continue
            report = validate_plugin(str(child), known_targets)
            info = report.get("plugin") or {}
            pid = info.get("id")
            if not report["valid"] or info.get("kind") != kind:
                continue
            inputs = info.get("inputs") or []
            if engine and inputs and engine not in inputs:
                continue
            if kind == "import" and not engine:
                continue
            if kind == "export" and target and target not in (info.get("targets") or []):
                continue
            report["builtin"] = is_builtin(child)
            is_pin = pinned.get(pid) == str(child.resolve())
            report["pinned"] = is_pin
            if pid not in best:
                order.append(pid)
                best[pid] = report
            elif is_pin or (not best[pid].get("pinned")
                            and version_key(info.get("version")) > version_key(best[pid]["plugin"].get("version"))):
                best[pid] = report
    found = [best[pid] for pid in order]
    if prefer:
        found.sort(key=lambda r: (r["plugin"]["id"] != prefer))
    return found


def run_plugin(params: dict, notify: Callable[[str, dict], None]) -> dict:
    """Runs a plugin to completion and returns its result."""
    folder = Path(params["plugin"]).expanduser()
    report = validate_plugin(str(folder), params.get("known_targets"))
    if not report["valid"]:
        raise EngineError(f"plugin {folder.name} is not valid: {'; '.join(report['errors'])}")
    info = report["plugin"]
    kind = params.get("kind") or info["kind"]
    if info["kind"] != kind:
        raise EngineError(f"{info['id']} is an {info['kind']} plugin, not {kind}")
    raw = _formats(folder)
    perms = raw.get("permissions", {})

    cache_root = Path(params.get("cache_root") or tempfile.gettempdir()).expanduser()
    temp = Path(tempfile.mkdtemp(prefix=f"ptk-{info['id']}-"))
    folders = {k: str(Path(v).expanduser().resolve()) for k, v in (params.get("folders") or {}).items() if v}
    folders["$CACHE"] = str(cache_root / info["id"])
    folders["$TEMP"] = str(temp)
    # Shared by every plugin and owned by the engine: tool downloads and
    # re-encoded assets (see ptk_engine.tools and ptk_engine.media).
    folders["$TOOLS"] = str(cache_root / "tools")
    folders["$MEDIA"] = str(cache_root / "media")
    if "$OUTPUT" in folders:
        Path(folders["$OUTPUT"]).mkdir(parents=True, exist_ok=True)

    request = {
        "kind": kind,
        "plugin_dir": str(folder.resolve()),
        "entry": info["entry"],
        "plugin": {k: info.get(k) for k in ("id", "name", "version", "license")},
        "engine": params.get("engine"),
        "target": params.get("target"),
        "manifest": params.get("manifest"),
        "settings": params.get("settings") or {},
        "phase": params.get("phase") or "run",
        "folders": folders,
        "permissions": {
            "write": perms.get("write", []),
            "network": perms.get("network", False),
            "subprocess": perms.get("subprocess", False),
        },
    }
    engine_root = str(Path(__file__).resolve().parent.parent)
    env = dict(os.environ)
    env["PYTHONPATH"] = os.pathsep.join([engine_root, *filter(None, [env.get("PYTHONPATH")])])
    env["PYTHONDONTWRITEBYTECODE"] = "1"

    notify("progress", {"phase": "plugin", "plugin": info["id"], "fraction": 0.0,
                        "message": f"Starting {info['name']} {info['version']}"})
    proc = subprocess.Popen(
        [sys.executable, "-m", "ptk_engine.host"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        cwd=temp, env=env,
    )
    stderr_tail: collections.deque[str] = collections.deque(maxlen=40)

    def drain() -> None:
        assert proc.stderr is not None
        for line in proc.stderr:
            stderr_tail.append(line.decode("utf-8", "replace").rstrip())

    t = threading.Thread(target=drain, daemon=True)
    t.start()
    assert proc.stdin is not None and proc.stdout is not None
    proc.stdin.write(json.dumps(request).encode("utf-8"))
    proc.stdin.close()

    outcome: dict | None = None
    logs: list[dict] = []
    for line in proc.stdout:
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        kind_ = msg.get("type")
        if kind_ == "progress":
            notify("progress", {"phase": "plugin", "plugin": info["id"],
                                "fraction": msg.get("fraction"), "message": msg.get("message", "")})
        elif kind_ == "log":
            entry = {"level": msg.get("level", "info"), "message": msg.get("message", "")}
            logs.append(entry)
            notify("log", {"plugin": info["id"], **entry})
        elif kind_ in ("result", "error"):
            outcome = msg
    proc.wait()
    t.join(timeout=5)
    if not params.get("keep_temp"):
        shutil.rmtree(temp, ignore_errors=True)

    if outcome is None:
        tail = " / ".join(list(stderr_tail)[-3:])
        raise EngineError(f"plugin {info['id']} stopped without a result (exit {proc.returncode}){': ' + tail if tail else ''}")
    if outcome["type"] == "error":
        err = EngineError(f"{info['name']}: {outcome.get('message')}")
        err.problems = outcome.get("problems") or []
        raise err
    return {
        "plugin": {**request["plugin"], "builtin": is_builtin(folder)},
        "result": outcome.get("result") or {},
        "log": logs,
    }
