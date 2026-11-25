"""Copying files fast: APFS clones on macOS, a plain copy elsewhere.

``clonefile(2)`` makes a copy-on-write clone: it takes no time and no disk
space until one side changes, and keeps the file's dates and mode. It works
only within one APFS volume; anything else (another volume, Linux, Windows,
``PTK_NO_CLONE=1``) falls back to ``shutil.copy2``.
"""

from __future__ import annotations

import ctypes
import errno
import os
import shutil
import sys
from pathlib import Path

CLONE_NOFOLLOW = 0x0001
CLONE_NOOWNERCOPY = 0x0002

_clonefile = None
_loaded = False


def _load():
    global _clonefile, _loaded
    if _loaded:
        return _clonefile
    _loaded = True
    if sys.platform != "darwin":
        return None
    try:
        lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
        fn = lib.clonefile
        fn.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint32]
        fn.restype = ctypes.c_int
        _clonefile = fn
    except (OSError, AttributeError):
        _clonefile = None
    return _clonefile


def can_clone() -> bool:
    return os.environ.get("PTK_NO_CLONE") != "1" and _load() is not None


def copy_file(src: Path, dst: Path) -> str:
    """Copies ``src`` to ``dst`` (replaced if it exists) and returns
    ``"clone"`` or ``"copy"``."""
    src, dst = Path(src), Path(dst)
    if dst.is_symlink() or dst.exists():
        dst.unlink()
    if can_clone():
        if _clonefile(os.fsencode(src), os.fsencode(dst), CLONE_NOFOLLOW | CLONE_NOOWNERCOPY) == 0:
            return "clone"
        err = ctypes.get_errno()
        # Another volume or a file system without clones: copy instead.
        if err not in (errno.EXDEV, errno.ENOTSUP, errno.EOPNOTSUPP, errno.EINVAL, errno.EPERM):
            raise OSError(err, os.strerror(err), str(src))
    shutil.copy2(src, dst)
    return "copy"
