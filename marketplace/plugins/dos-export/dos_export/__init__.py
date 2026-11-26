"""Gold Box export (DOSBox Staging): the game's data with DOSBox Staging (see runtime.toml)."""

from pathlib import PurePosixPath

from ptk_engine.sdk import RuntimeExportPlugin

# Gold Box games start from one of these; otherwise DOSBox opens a prompt in the game folder.
STARTERS = ["start.exe", "game.exe", "start.bat", "game.bat"]


class Plugin(RuntimeExportPlugin):
    def launch_args(self, ctx, files, main):
        top = {f.lower(): f for f in files if "/" not in f}
        for name in STARTERS:
            if name in top:
                return ["{game}/" + top[name]]
        return ["{game}"]
