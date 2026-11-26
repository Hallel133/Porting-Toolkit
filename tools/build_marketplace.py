#!/usr/bin/env python3
"""Builds the marketplace files for marketplace/plugins/*: one zip per plugin
and an index.json, ready to upload to any static host.

    python3 tools/build_marketplace.py BASE_URL OUT_DIR

BASE_URL is where the zips will be served from, for example
https://codeberg.org/YOU/ptk-marketplace/raw/branch/main (the zips go next
to index.json). Zips are deterministic (sorted, fixed dates), so a plugin
that did not change keeps its SHA-256. See docs/marketplace-hosting.md.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLUGINS = ROOT / "marketplace" / "plugins"
SKIP_DIRS = {".git", "__pycache__", ".pytest_cache", "node_modules"}
SKIP_FILES = {".DS_Store", "Thumbs.db"}
DATE = (2020, 1, 1, 0, 0, 0)


def zip_plugin(src: Path, out: Path) -> None:
    files = sorted(p for p in src.rglob("*")
                   if p.is_file() and not (set(p.relative_to(src).parts) & SKIP_DIRS) and p.name not in SKIP_FILES
                   and not p.name.endswith(".pyc"))
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for p in files:
            info = zipfile.ZipInfo(f"{src.name}/{p.relative_to(src).as_posix()}", DATE)
            info.external_attr = (0o100755 if p.stat().st_mode & 0o111 else 0o100644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, p.read_bytes())


def entry(plugin: Path, base_url: str, out_dir: Path) -> dict:
    m = tomllib.loads((plugin / "plugin.toml").read_text(encoding="utf-8"))
    p, formats = m["plugin"], m.get("formats", {})
    if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", p["id"]):
        raise SystemExit(f"{plugin}: bad id {p['id']!r}")
    name = f"{p['id']}-{p['version']}.zip"
    zip_plugin(plugin, out_dir / name)
    sha = hashlib.sha256((out_dir / name).read_bytes()).hexdigest()
    e = {"id": p["id"], "name": p["name"], "kind": "plugin", "version": p["version"],
         "description": p.get("description", ""), "author": ", ".join(p.get("authors", [])),
         "license": p["license"], "download": f"{base_url.rstrip('/')}/{name}", "sha256": sha,
         "reviewed": False, "engines": formats.get("inputs", []), "targets": formats.get("targets", [])}
    if p.get("homepage"):
        e["homepage"] = p["homepage"]
    return e


def build(base_url: str, out_dir: Path) -> dict:
    if not (base_url.startswith("https://") or base_url.startswith("file://")):
        raise SystemExit("BASE_URL must start with https:// (or file:// for a local test)")
    out_dir.mkdir(parents=True, exist_ok=True)
    plugins = sorted(p for p in PLUGINS.iterdir() if (p / "plugin.toml").is_file())
    index = {"schema_version": 1, "entries": [entry(p, base_url, out_dir) for p in plugins]}
    (out_dir / "index.json").write_text(json.dumps(index, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return index


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    index = build(argv[0], Path(argv[1]).resolve())
    print(f"wrote {len(index['entries'])} plugins and index.json to {argv[1]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
