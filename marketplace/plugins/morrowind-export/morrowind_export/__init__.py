"""Morrowind export (OpenMW): the game's data with OpenMW (see runtime.toml)."""

from pathlib import PurePosixPath

from ptk_engine.sdk import RuntimeExportPlugin

# Content files OpenMW loads, in this order, when the game has them.
CONTENT = ["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"]


class Plugin(RuntimeExportPlugin):
    def launch_args(self, ctx, files, main):
        """--data is the folder holding Morrowind.esm; the official content
        files and archives the game has are loaded from it."""
        data = PurePosixPath(main).parent
        here = {PurePosixPath(f).name.lower(): PurePosixPath(f).name for f in files
                if PurePosixPath(f).parent == data}
        folder = "{game}" if str(data) == "." else "{game}/" + data.as_posix()
        args = ["--data", folder]
        for name in CONTENT:
            if name.lower() in here:
                args += ["--content", here[name.lower()]]
        for name in sorted(n for n in here.values() if n.lower().endswith(".bsa")):
            args += ["--fallback-archive", name]
        return args
