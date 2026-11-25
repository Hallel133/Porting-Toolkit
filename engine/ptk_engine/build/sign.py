"""Ad-hoc code signing of a macOS app, only when the app changed.

``codesign`` rewrites the executables it signs, so the app is
fingerprinted (paths, sizes, dates) right after signing; the next export
signs again only when that fingerprint no longer matches.
"""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
from typing import Callable


def fingerprint(app: Path) -> str:
    h = hashlib.sha256()
    for dirpath, dirnames, filenames in os.walk(app):
        dirnames.sort()
        for name in sorted(filenames):
            p = Path(dirpath) / name
            st = p.lstat()
            h.update(f"{p.relative_to(app).as_posix()}|{st.st_size}|{st.st_mtime_ns}\n".encode())
    return h.hexdigest()


def _stamp(app: Path) -> Path:
    return app.with_name(app.name + ".ptksign")


def needs_signing(app: Path) -> bool:
    s = _stamp(app)
    return not (s.is_file() and s.read_text(encoding="utf-8").strip() == fingerprint(app))


def codesign_if_changed(app: Path, run: Callable[[list], object], entitlements: Path | None = None) -> bool:
    """Signs ``app`` ad hoc with ``run`` (``ctx.run``) unless it is unchanged
    since it was last signed here. Returns True when it signed."""
    app = Path(app)
    if not needs_signing(app):
        return False
    args = ["codesign", "--force", "--deep", "--sign", "-", "--timestamp=none"]
    if entitlements:
        args += ["--entitlements", str(entitlements)]
    run([*args, str(app)])
    _stamp(app).write_text(fingerprint(app) + "\n", encoding="utf-8")
    return True
