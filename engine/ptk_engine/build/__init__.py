"""Building export outputs incrementally.

- :class:`Build` (``stage``): declare the output, then ``sync()`` writes only
  what changed (APFS clones on macOS) and removes what is gone.
- :func:`zip_dir` (``package``): a deterministic zip that reuses the
  unchanged members of the previous one.
- :func:`codesign_if_changed` (``sign``): ad-hoc signing only when needed.
"""

from .clone import can_clone, copy_file
from .package import zip_dir
from .sign import codesign_if_changed, needs_signing
from .stage import STATE_FILE, Build, BuildError, SyncReport

__all__ = ["Build", "BuildError", "STATE_FILE", "SyncReport", "can_clone", "codesign_if_changed", "copy_file",
           "needs_signing", "zip_dir"]
