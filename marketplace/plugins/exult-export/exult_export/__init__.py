"""Ultima VII export (Exult): the game's data with Exult (see runtime.toml)."""

from ptk_engine.sdk import RuntimeExportPlugin

CONFIG = """<config>
 <disk>
  <game>
   <{game}>
    <path>.</path>
   </{game}>
  </game>
 </disk>
</config>
"""


class Plugin(RuntimeExportPlugin):
    def _game(self, files):
        low = {f.lower() for f in files}
        return "serpentisle" if any(f.endswith("static/sispeech.spc") for f in low) else "blackgate"

    def launch_args(self, ctx, files, main):
        """Exult reads an exult.cfg written into the game folder; --bg or --si
        starts that game directly."""
        return ["-c", "{game}/exult.cfg", "--si" if self._game(files) == "serpentisle" else "--bg"]

    def extra_layout(self, ctx, build, game_dest):
        build.add_text(f"{game_dest}/exult.cfg", CONFIG.format(game=self._game(self._files)))
