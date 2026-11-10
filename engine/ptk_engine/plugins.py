"""Plugin format: definition, validation and templates.

A plugin is a Python package with a ``plugin.toml`` next to it. This module
validates the format without running plugin code; ``runtime.py`` runs them.
See ``docs/plugins.md`` for the full description and the security model.
"""

from __future__ import annotations

import re
import tomllib
from pathlib import Path

from . import EngineError
from .signatures import ENGINE_IDS

PLUGIN_FILE = "plugin.toml"
SCHEMA_VERSION = 1
#: Version of the API a plugin is written against (see api.py).
PLUGIN_API = 1
KINDS = ("import", "export")

ID_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+([-+][0-9A-Za-z.-]+)?$")
ENTRY_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*:[A-Za-z_][A-Za-z0-9_]*$")
HOST_RE = re.compile(r"^(\*\.)?[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?(\.[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?)*(:\d+)?$")

#: Folders a plugin may ask for. ptk resolves them when the plugin runs.
PATH_VARIABLES = {
    "$GAME": "the game folder you choose",
    "$PROJECT": "the porting project folder",
    "$OUTPUT": "the output folder you choose",
    "$CACHE": "the plugin's own cache folder",
    "$TEMP": "a temporary folder",
}

KNOWN_KEYS = {
    "": {"schema_version", "plugin", "formats", "permissions"},
    "plugin": {"id", "name", "version", "kind", "description", "license", "authors", "entry", "api", "homepage"},
    "formats": {"inputs", "outputs", "targets"},
    "permissions": {"read", "write", "network", "subprocess"},
}


def _check_paths(field: str, values: object, errors: list[str], warnings: list[str]) -> list[str]:
    if values is None:
        return []
    if not isinstance(values, list) or not all(isinstance(v, str) for v in values):
        errors.append(f"permissions.{field} must be a list of paths")
        return []
    for v in values:
        if v.startswith("$"):
            var = v.split("/", 1)[0]
            if var not in PATH_VARIABLES:
                errors.append(f"permissions.{field}: unknown folder {var!r} (use {', '.join(PATH_VARIABLES)})")
            elif ".." in v.split("/"):
                errors.append(f"permissions.{field}: {v!r} must not contain '..'")
        elif v in ("/", "~", "~/") or re.fullmatch(r"[A-Za-z]:[\\/]?", v):
            warnings.append(f"permissions.{field}: {v!r} is your whole disk or home folder")
        elif v.startswith(("/", "~")) or re.match(r"^[A-Za-z]:[\\/]", v):
            warnings.append(f"permissions.{field}: {v!r} is outside the folders ptk manages")
        else:
            errors.append(f"permissions.{field}: {v!r} must start with a folder variable such as $GAME, or be absolute")
    return values


def _describe_path(v: str) -> str:
    var = v.split("/", 1)[0]
    if var in PATH_VARIABLES:
        rest = v[len(var):]
        return f"{PATH_VARIABLES[var]}{f' ({rest.lstrip(chr(47))})' if rest.strip('/') else ''}"
    return v


def validate_plugin(path: str, known_targets: list[str] | None = None) -> dict:
    """Validates a plugin folder. Never imports or runs plugin code."""
    folder = Path(path).expanduser()
    errors: list[str] = []
    warnings: list[str] = []
    report: dict = {
        "path": str(folder),
        "valid": False,
        "errors": errors,
        "warnings": warnings,
        "plugin": None,
        "permissions": [],
        "risk": "unknown",
    }
    manifest = folder / PLUGIN_FILE
    if not folder.is_dir():
        errors.append(f"{folder} is not a folder")
        return report
    if not manifest.is_file():
        errors.append(f"no {PLUGIN_FILE} in {folder}")
        return report
    try:
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    except (tomllib.TOMLDecodeError, UnicodeDecodeError) as e:
        errors.append(f"{PLUGIN_FILE}: {e}")
        return report

    for section, keys in KNOWN_KEYS.items():
        table = data if section == "" else data.get(section, {})
        if isinstance(table, dict):
            for key in table:
                if key not in keys:
                    warnings.append(f"unknown key {(section + '.') if section else ''}{key}")

    if data.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION}")

    p = data.get("plugin")
    if not isinstance(p, dict):
        errors.append("missing [plugin] table")
        return report
    pid = p.get("id")
    if not isinstance(pid, str) or not ID_RE.match(pid) or len(pid) > 64:
        errors.append("plugin.id must be lower-case letters, digits and single dashes")
    if not isinstance(p.get("name"), str) or not p.get("name", "").strip():
        errors.append("plugin.name is required")
    if not isinstance(p.get("version"), str) or not VERSION_RE.match(p.get("version", "")):
        errors.append("plugin.version must look like 1.2.3")
    kind = p.get("kind")
    if kind not in KINDS:
        errors.append("plugin.kind must be \"import\" or \"export\"")
    if not isinstance(p.get("license"), str) or not p.get("license", "").strip():
        errors.append("plugin.license is required (an SPDX id such as MIT)")
    if not isinstance(p.get("description", ""), str):
        errors.append("plugin.description must be text")
    authors = p.get("authors", [])
    if not isinstance(authors, list) or not all(isinstance(a, str) for a in authors):
        errors.append("plugin.authors must be a list of names")
    api = p.get("api", PLUGIN_API)
    if api != PLUGIN_API:
        errors.append(f"plugin.api {api!r} is not supported (this ptk supports {PLUGIN_API})")

    entry = p.get("entry")
    if not isinstance(entry, str) or not ENTRY_RE.match(entry):
        errors.append('plugin.entry must look like "package.module:ClassName"')
    else:
        module = entry.split(":", 1)[0].replace(".", "/")
        candidates = [folder / f"{module}.py", folder / module / "__init__.py",
                      folder / "src" / f"{module}.py", folder / "src" / module / "__init__.py"]
        if not any(c.is_file() for c in candidates):
            errors.append(f"plugin.entry module {entry.split(':')[0]!r} was not found in the plugin folder")
    if not (folder / "pyproject.toml").is_file():
        warnings.append("no pyproject.toml: a plugin should be an installable Python package")

    formats = data.get("formats", {})
    if not isinstance(formats, dict):
        errors.append("[formats] must be a table")
        formats = {}
    lists = {}
    for key in ("inputs", "outputs", "targets"):
        v = formats.get(key, [])
        if not isinstance(v, list) or not all(isinstance(x, str) for x in v):
            errors.append(f"formats.{key} must be a list of ids")
            v = []
        lists[key] = v
    if kind == "import" and not lists["inputs"]:
        errors.append("an import plugin must list formats.inputs (engines or formats it reads)")
    if kind == "export" and not lists["targets"]:
        errors.append("an export plugin must list formats.targets")
    for i in lists["inputs"]:
        if i not in ENGINE_IDS:
            warnings.append(f"formats.inputs: {i!r} is not an engine ptk detects ({', '.join(ENGINE_IDS)})")
    for t in lists["targets"]:
        if not ID_RE.match(t):
            errors.append(f"formats.targets: invalid target id {t!r}")
        elif known_targets is not None and t not in known_targets:
            warnings.append(f"formats.targets: unknown target {t!r} (see `ptk targets`)")

    perms = data.get("permissions", {})
    if not isinstance(perms, dict):
        errors.append("[permissions] must be a table")
        perms = {}
    read = _check_paths("read", perms.get("read", []), errors, warnings)
    write = _check_paths("write", perms.get("write", []), errors, warnings)
    network = perms.get("network", [])
    if network is True:
        network = ["*"]
    elif network is False:
        network = []
    if not isinstance(network, list) or not all(isinstance(h, str) for h in network):
        errors.append("permissions.network must be false, true or a list of host names")
        network = []
    for h in network:
        if h == "*":
            warnings.append("permissions.network allows any host")
        elif not HOST_RE.match(h):
            errors.append(f"permissions.network: {h!r} is not a host name")
    subprocess = perms.get("subprocess", False)
    if not isinstance(subprocess, bool):
        errors.append("permissions.subprocess must be true or false")
        subprocess = False
    if subprocess:
        warnings.append("permissions.subprocess lets the plugin start other programs")

    summary = []
    summary += [f"Read {_describe_path(v)}" for v in read]
    summary += [f"Write {_describe_path(v)}" for v in write]
    if not network:
        summary.append("No network access")
    elif "*" in network:
        summary.append("Network: any host")
    else:
        summary.append("Network: " + ", ".join(network))
    if subprocess:
        summary.append("Start other programs")

    broad = any(not v.startswith("$") for v in [*read, *write])
    if broad or "*" in network or subprocess:
        risk = "high"
    elif network or write:
        risk = "medium"
    else:
        risk = "low"

    report.update(
        valid=not errors,
        plugin={
            "id": pid,
            "name": p.get("name"),
            "version": p.get("version"),
            "kind": kind,
            "description": p.get("description", ""),
            "license": p.get("license"),
            "authors": authors if isinstance(authors, list) else [],
            "entry": entry,
            "inputs": lists["inputs"],
            "outputs": lists["outputs"],
            "targets": lists["targets"],
        },
        permissions=summary,
        risk=risk,
    )
    return report


def _is_builtin(path: Path) -> bool:
    from .runtime import is_builtin

    return is_builtin(path)


def list_plugins(dirs: list[str], known_targets: list[str] | None = None) -> list[dict]:
    """Every plugin folder (containing plugin.toml) directly inside ``dirs``."""
    out = []
    for d in dirs:
        base = Path(d).expanduser()
        if not base.is_dir():
            continue
        for child in sorted(base.iterdir()):
            if child.is_dir() and (child / PLUGIN_FILE).is_file():
                r = validate_plugin(str(child), known_targets)
                info = r["plugin"] or {}
                builtin = _is_builtin(child)
                out.append(
                    {
                        "id": info.get("id") or child.name,
                        "name": info.get("name") or child.name,
                        "version": info.get("version") or "?",
                        "kind": info.get("kind") or "?",
                        "path": str(child),
                        "valid": r["valid"],
                        "errors": len(r["errors"]),
                        "risk": r["risk"],
                        "permissions": r["permissions"],
                        "inputs": info.get("inputs") or [],
                        "targets": info.get("targets") or [],
                        "description": info.get("description") or "",
                        "builtin": builtin,
                        # Built-in plugins ship with ptk; the marketplace
                        # marks reviewed community plugins.
                        "reviewed": builtin,
                    }
                )
    return out


PLUGIN_TOML = """\
schema_version = 1

[plugin]
id = "{id}"
name = "{name}"
version = "0.1.0"
kind = "{kind}"
description = "Describe what this plugin {verb}."
license = "MIT"
authors = []
entry = "{module}:Plugin"
api = 1

[formats]
{formats}

# What the plugin needs. ptk shows this list and asks before installing.
# Folders: $GAME, $PROJECT, $OUTPUT, $CACHE, $TEMP.
[permissions]
read = [{read}]
write = [{write}]
network = false
subprocess = false
"""

PYPROJECT = """\
[project]
name = "{id}"
version = "0.1.0"
description = "Porting Toolkit {kind} plugin"
requires-python = ">=3.11"
dependencies = []

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["{module}"]
"""

MODULE = '''\
"""{name}: a Porting Toolkit {kind} plugin.

Keep the class name in sync with plugin.toml `entry`. The context API is
described in docs/plugins.md ("Writing a plugin").
"""


class Plugin:
    """Entry point named in plugin.toml."""

    def run(self, ctx):
        # ctx.game, ctx.project, ctx.output, ctx.cache, ctx.temp: folders
        # ctx.target: the target being built (export plugins)
        # ctx.manifest, ctx.settings: the project and `--set key=value`
        # ctx.progress(0.5, "..."), ctx.log("..."), ctx.fail("...")
        ctx.progress(1.0, "Nothing to do yet")
        return {{"outputs": [], "summary": "{name} ran"}}
'''

README = """\
# {name}

A Porting Toolkit {kind} plugin.

```bash
ptk plugin validate .
```

See https://github.com/Hallel133/Porting-Toolkit/blob/main/docs/plugins.md
"""


def new_plugin(path: str, kind: str, plugin_id: str | None = None) -> dict:
    """Creates a plugin skeleton in a new or empty folder."""
    if kind not in KINDS:
        raise EngineError('kind must be "import" or "export"')
    folder = Path(path).expanduser()
    if folder.exists() and (not folder.is_dir() or any(folder.iterdir())):
        raise EngineError(f"{folder} already exists and is not empty")
    pid = plugin_id or re.sub(r"[^a-z0-9]+", "-", folder.name.lower()).strip("-")
    if not ID_RE.match(pid or ""):
        raise EngineError(f"{pid!r} is not a valid plugin id (lower-case letters, digits, dashes)")
    module = pid.replace("-", "_")
    if module[0].isdigit():
        module = "plugin_" + module
    name = pid.replace("-", " ").title()
    if kind == "import":
        formats = 'inputs = ["unity"]   # engines or formats this plugin reads\noutputs = []'
        read, write, verb = '"$GAME"', '"$PROJECT"', "imports"
    else:
        formats = 'targets = ["macos-arm64"]   # targets this plugin builds for'
        read, write, verb = '"$PROJECT", "$GAME"', '"$OUTPUT"', "exports"
    values = dict(id=pid, name=name, kind=kind, module=module, formats=formats, read=read, write=write, verb=verb)
    files = {
        PLUGIN_FILE: PLUGIN_TOML.format(**values),
        "pyproject.toml": PYPROJECT.format(**values),
        f"{module}/__init__.py": MODULE.format(**values),
        "README.md": README.format(**values),
    }
    folder.mkdir(parents=True, exist_ok=True)
    for rel, text in files.items():
        target = folder / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
    return {"path": str(folder.resolve()), "id": pid, "files": sorted(files)}
