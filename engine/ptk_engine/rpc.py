"""JSON-RPC 2.0 server over stdin/stdout, one JSON message per line.

stdout carries only protocol messages: while a request runs, ``sys.stdout``
points at stderr, so a stray ``print`` (from us or, later, from a plugin)
cannot corrupt the stream.
"""

from __future__ import annotations

import json
import os
import platform
import sys
import traceback
from collections.abc import Callable
from pathlib import Path
from typing import BinaryIO

from . import PROTOCOL_VERSION, EngineError, __version__
from .categories import CATEGORY_IDS
from .media.encoders import ENCODERS
from .plugins import list_plugins, new_plugin, validate_plugin
from .runtime import builtin_dir, find_plugins, run_plugin
from .scan import scan
from .signatures import ENGINE_IDS

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603
ENGINE_ERROR = -32000


class RpcError(Exception):
    def __init__(self, code: int, message: str, data: object = None):
        super().__init__(message)
        self.code = code
        self.message = message
        self.data = data


def _require(params: dict, key: str, kind: type = str):
    value = params.get(key)
    if not isinstance(value, kind):
        raise RpcError(INVALID_PARAMS, f"missing or invalid parameter {key!r}")
    return value


def _optional_list(params: dict, key: str) -> list[str] | None:
    value = params.get(key)
    if value is None:
        return None
    if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
        raise RpcError(INVALID_PARAMS, f"parameter {key!r} must be a list of strings")
    return value


class Server:
    def __init__(self, out: BinaryIO):
        self.out = out
        self.running = True
        self.methods: dict[str, Callable[[dict], object]] = {
            "engine.info": self.engine_info,
            "scan": self.scan,
            "plugin.validate": self.plugin_validate,
            "plugin.list": self.plugin_list,
            "plugin.new": self.plugin_new,
            "plugin.find": self.plugin_find,
            "plugin.run": self.plugin_run,
            "plugin.check": self.plugin_check,
            "tools.status": self.tools_status,
            "tools.install": self.tools_install,
            "shutdown": self.shutdown,
        }

    # -- transport -------------------------------------------------------

    def send(self, message: dict) -> None:
        data = json.dumps(message, ensure_ascii=False, separators=(",", ":"))
        self.out.write(data.encode("utf-8", "replace") + b"\n")
        self.out.flush()

    def notify(self, method: str, params: dict) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def handle_line(self, line: bytes) -> dict | None:
        """Handles one request line; returns the response (None for notifications)."""
        try:
            msg = json.loads(line)
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            return self.error(None, PARSE_ERROR, f"parse error: {e}")
        if not isinstance(msg, dict) or msg.get("jsonrpc") != "2.0" or not isinstance(msg.get("method"), str):
            return self.error(msg.get("id") if isinstance(msg, dict) else None, INVALID_REQUEST, "invalid request")
        req_id = msg.get("id")
        params = msg.get("params", {})
        if params is None:
            params = {}
        if not isinstance(params, dict):
            return self.error(req_id, INVALID_PARAMS, "params must be an object")
        handler = self.methods.get(msg["method"])
        if handler is None:
            return self.error(req_id, METHOD_NOT_FOUND, f"unknown method {msg['method']!r}")
        try:
            result = handler(params)
        except RpcError as e:
            return self.error(req_id, e.code, e.message, e.data)
        except EngineError as e:
            problems = getattr(e, "problems", None)
            return self.error(req_id, ENGINE_ERROR, str(e), {"problems": problems} if problems else None)
        except Exception as e:  # noqa: BLE001 - report, keep serving
            traceback.print_exc(file=sys.stderr)
            return self.error(req_id, INTERNAL_ERROR, f"internal engine error: {e}")
        if req_id is None:
            return None
        return {"jsonrpc": "2.0", "id": req_id, "result": result}

    @staticmethod
    def error(req_id, code: int, message: str, data: object = None) -> dict:
        err: dict = {"code": code, "message": message}
        if data is not None:
            err["data"] = data
        return {"jsonrpc": "2.0", "id": req_id, "error": err}

    def serve(self, inp: BinaryIO) -> None:
        for line in inp:
            if not line.strip():
                continue
            response = self.handle_line(line)
            if response is not None:
                self.send(response)
            if not self.running:
                break

    # -- methods -----------------------------------------------------------

    def engine_info(self, params: dict) -> dict:
        return {
            "name": "ptk-engine",
            "version": __version__,
            "protocol": PROTOCOL_VERSION,
            "python": platform.python_version(),
            "executable": sys.executable,
            "categories": CATEGORY_IDS,
            "engines": ENGINE_IDS,
            "codecs": sorted(ENCODERS),
            "methods": sorted(self.methods),
        }

    def scan(self, params: dict) -> dict:
        path = _require(params, "path")
        max_files = params.get("max_files")
        if max_files is not None and (not isinstance(max_files, int) or max_files < 1):
            raise RpcError(INVALID_PARAMS, "max_files must be a positive integer")
        return scan(
            path,
            max_files=max_files,
            progress=lambda n: self.notify("progress", {"phase": "scan", "files": n}),
        )

    def plugin_validate(self, params: dict) -> dict:
        return validate_plugin(_require(params, "path"), _optional_list(params, "known_targets"))

    def plugin_list(self, params: dict) -> list:
        dirs = _optional_list(params, "dirs") or []
        if params.get("builtin", True) and (b := builtin_dir()) is not None:
            dirs = [*dirs, str(b)]
        return list_plugins(dirs, _optional_list(params, "known_targets"))

    def plugin_find(self, params: dict) -> list:
        kind = _require(params, "kind")
        return find_plugins(
            kind,
            params.get("engine"),
            params.get("target"),
            _optional_list(params, "dirs") or [],
            _optional_list(params, "known_targets"),
            params.get("prefer"),
            params.get("pins") if isinstance(params.get("pins"), dict) else None,
        )

    def plugin_run(self, params: dict) -> dict:
        _require(params, "plugin")
        if not isinstance(params.get("folders"), dict):
            raise RpcError(INVALID_PARAMS, "missing or invalid parameter 'folders'")
        return run_plugin(params, self.notify)

    def plugin_check(self, params: dict) -> dict:
        """Runs only the plugin's ``check`` step: what would stop or limit
        this export, without building anything."""
        _require(params, "plugin")
        if not isinstance(params.get("folders"), dict):
            raise RpcError(INVALID_PARAMS, "missing or invalid parameter 'folders'")
        folders = {k: v for k, v in params["folders"].items() if k != "$OUTPUT"}
        return run_plugin({**params, "folders": folders, "phase": "check"}, self.notify)

    def plugin_new(self, params: dict) -> dict:
        plugin_id = params.get("id")
        if plugin_id is not None and not isinstance(plugin_id, str):
            raise RpcError(INVALID_PARAMS, "id must be a string")
        return new_plugin(_require(params, "path"), _require(params, "kind"), plugin_id)

    def tools_status(self, params: dict) -> dict:
        from . import tools
        d = params.get("tools_dir")
        from . import android
        return {"platform": tools.platform_key(), "tools": tools.status(Path(d) if d else None) + android.status()}

    def tools_install(self, params: dict) -> dict:
        from . import tools
        tool_id = _require(params, "id")
        if tool_id not in tools.TOOLS:
            raise RpcError(INVALID_PARAMS, f"unknown tool {tool_id!r} (known: {', '.join(tools.TOOLS)})")
        d = _require(params, "tools_dir")
        try:
            found = tools.install(tool_id, Path(d), log=lambda m: self.notify("log", {"level": "info", "message": m}))
        except tools.ToolError as e:
            raise EngineError(str(e)) from e
        return found.to_dict()

    def shutdown(self, params: dict) -> dict:
        self.running = False
        return {}


def main() -> None:
    out = sys.stdout.buffer
    sys.stdout = sys.stderr
    try:
        Server(out).serve(sys.stdin.buffer)
    except (BrokenPipeError, KeyboardInterrupt):
        # ptk went away mid-answer (for example the app quit): nothing to report.
        try:
            sys.stderr.close()
        finally:
            os._exit(0)
