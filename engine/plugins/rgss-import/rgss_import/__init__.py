"""RPG Maker XP/VX/VX Ace import: Game.ini and the archive header.

Nothing from the game is copied into the project, and archives are never
decrypted.
"""

from __future__ import annotations

from ptk_engine.formats import rgss


class Plugin:
    def run(self, ctx):
        ctx.progress(0.2, "Reading Game.ini")
        try:
            game = rgss.detect(ctx.game)
        except rgss.RgssError as e:
            ctx.fail(str(e))
        for note in game.notes:
            ctx.warn(note)
        recipe = {"engine": "rgss", **game.to_dict()}
        saved = ctx.save_recipe("rgss", recipe)
        packed = f", packed in {game.archive['path']}" if game.archive else ""
        ctx.progress(1.0, "Done")
        return {
            "summary": f"RPG Maker {game.variant} game (RGSS{game.rgss}){packed}",
            "recipe": recipe,
            "outputs": [str(saved)],
            "title": game.title,
        }
