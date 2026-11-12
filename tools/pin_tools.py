#!/usr/bin/env python3
"""Prints `Pin(...)` lines for the compression tools that have GitHub releases.

The SHA-256 values come from the digests GitHub publishes for release assets
(`gh api repos/OWNER/REPO/releases/tags/TAG`), so nothing is downloaded and
no hash is typed by hand. Paste the output into `TOOLS` in
`engine/ptk_engine/tools.py`.

    python3 tools/pin_tools.py              # latest releases
    python3 tools/pin_tools.py oxipng v10.2.1
"""

from __future__ import annotations

import json
import re
import subprocess
import sys

#: tool id -> (repo, asset pattern per platform key; {v} is the version without "v")
RELEASES = {
    "oxipng": ("oxipng/oxipng", {
        "macos-arm64": r"oxipng-{v}-aarch64-apple-darwin\.tar\.gz",
        "macos-x64": r"oxipng-{v}-x86_64-apple-darwin\.tar\.gz",
    }),
}


def release(repo: str, tag: str | None) -> dict:
    path = f"repos/{repo}/releases/" + (f"tags/{tag}" if tag else "latest")
    return json.loads(subprocess.run(["gh", "api", path], check=True, capture_output=True, text=True).stdout)


def main(argv: list[str]) -> int:
    wanted = argv[:1] or list(RELEASES)
    tag = argv[1] if len(argv) > 1 else None
    for tool in wanted:
        repo, assets = RELEASES[tool]
        rel = release(repo, tag)
        version = rel["tag_name"].lstrip("v")
        print(f"# {tool} {rel['tag_name']} ({repo})")
        for key, pattern in assets.items():
            rx = re.compile(pattern.format(v=re.escape(version)) + "$")
            asset = next((a for a in rel["assets"] if rx.match(a["name"])), None)
            digest = (asset or {}).get("digest") or ""
            if not asset or not digest.startswith("sha256:"):
                print(f"#   {key}: no asset with a published sha256 digest")
                continue
            kind = "zip" if asset["name"].endswith(".zip") else "tar.gz"
            print(f'"{key}": Pin("{version}", "{asset["browser_download_url"]}",\n'
                  f'    "{digest[7:]}", "{kind}"),')
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
