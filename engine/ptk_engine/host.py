"""Runs one plugin in its own process: ``python -m ptk_engine.host``.

The runtime writes one JSON request on stdin. This process imports the
plugin's entry class, calls ``run(ctx)`` and writes JSON lines on its real
stdout: ``progress`` and ``log`` messages, then exactly one ``result`` or
``error``. Anything the plugin prints goes to stderr, so it cannot corrupt
the channel.
"""

from __future__ import annotations

import importlib
import json
import os
import sys
import traceback


def main() -> int:
    from . import api

    channel = sys.stdout
    sys.stdout = sys.stderr
    api._CHANNEL = channel
    try:
        request = json.loads(sys.stdin.read())
        folder = request["plugin_dir"]
        for p in (os.path.join(folder, "src"), folder):
            if os.path.isdir(p):
                sys.path.insert(0, p)
        module_name, _, class_name = request["entry"].partition(":")
        os.chdir(request["folders"]["$TEMP"])
        module = importlib.import_module(module_name)
        plugin = getattr(module, class_name)()
        ctx = api.Context(request)
        if request.get("phase") == "check":
            check = getattr(plugin, "check_result", None)
            result = check(ctx) if check else {"problems": [], "ok": True, "api": 1}
            api._emit("result", result=result)
            return 0
        result = plugin.run(ctx) or {}
        if not isinstance(result, dict):
            raise api.PluginError("the plugin returned something other than a dict")
        result.setdefault("outputs", [])
        result["warnings"] = list(dict.fromkeys([*ctx.warnings, *result.get("warnings", [])]))
        api._emit("result", result=result)
        return 0
    except api.PluginError as e:
        problems = [p.to_dict() for p in getattr(e, "problems", [])]
        api._emit("error", message=str(e), denied=isinstance(e, api.PermissionDenied), problems=problems)
        return 1
    except Exception as e:  # noqa: BLE001 - report everything to the runtime
        traceback.print_exc(file=sys.stderr)
        api._emit("error", message=f"{type(e).__name__}: {e}", traceback=traceback.format_exc())
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
