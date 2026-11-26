#!/usr/bin/env python3
"""Writes docs/engines.md from the engine catalog (engine/ptk_engine/data/engines.json).

    python3 tools/gen_engines_doc.py          # write docs/engines.md
    python3 tools/gen_engines_doc.py --check  # fail if it is out of date
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "engine" / "ptk_engine" / "data" / "engines.json"
DOC = ROOT / "docs" / "engines.md"

SECTIONS = [
    ("supported", "Exported by built-in plugins"),
    ("web", "Web engines (exported by the HTML5 plugins)"),
    ("partial", "Depends on the build"),
    ("runtime", "Open-source runtimes"),
    ("closed", "Needs the source code"),
    ("not_engine", "Nothing to port"),
    ("unresolved", "Not recognised yet"),
]


def cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def render(engines: list[dict]) -> str:
    out = [
        "# Engines",
        "",
        "Generated from `engine/ptk_engine/data/engines.json` by `tools/gen_engines_doc.py`; do not edit by hand.",
        "",
        f"Porting Toolkit recognises {len(engines)} engines from their files' names and first bytes "
        "(nothing is decoded). `ptk engines` lists them; `ptk scan` says which one a game uses.",
        "",
    ]
    for status, title in SECTIONS:
        rows = [e for e in engines if e["status"] == status]
        if not rows:
            continue
        out += [f"## {title} ({len(rows)})", ""]
        if status == "runtime":
            out += ["| Engine | Runtime | Plugin | How |", "|---|---|---|---|"]
            for e in rows:
                rt = e["runtime"]
                runtime = f"[{rt['name']}]({rt['url']})" if rt else ""
                plugin = f"`{e['plugin']}`" if e["plugin"] else "none yet"
                out.append(f"| {cell(e['name'])} | {runtime} | {plugin} | {cell(e['how'])} |")
        else:
            out += ["| Engine | Detected | How |", "|---|---|---|"]
            for e in rows:
                detected = "yes" if e["rules"] else (f"as `{e['via']}`" if e["via"] else "no")
                out.append(f"| {cell(e['name'])} | {detected} | {cell(e['how'])} |")
        out.append("")
    return "\n".join(out)


def main(argv: list[str]) -> int:
    engines = [e for e in json.loads(CATALOG.read_text(encoding="utf-8"))["engines"] if e["kind"] == "engine"]
    text = render(engines)
    if "--check" in argv:
        if DOC.read_text(encoding="utf-8") != text:
            print("docs/engines.md is out of date: run python3 tools/gen_engines_doc.py", file=sys.stderr)
            return 1
        return 0
    DOC.write_text(text, encoding="utf-8")
    print(f"wrote {DOC.relative_to(ROOT)} ({len(engines)} engines)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
