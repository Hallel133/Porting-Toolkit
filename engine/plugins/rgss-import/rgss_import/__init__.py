"""RPG Maker XP/VX/VX Ace import: Game.ini and the archive header.

Nothing from the game is copied into the project, and archives are never
decrypted.
"""

from __future__ import annotations

from ptk_engine.sdk import ImportPlugin
from ptk_engine.formats import rgss


class Plugin(ImportPlugin):
    recipe_name = "rgss"

    def detect(self, ctx):
        ctx.progress(0.2, "Reading Game.ini")
        try:
            game = rgss.detect(ctx.game)
        except rgss.RgssError as e:
            ctx.fail(str(e))
        for note in game.notes:
            ctx.warn(note)
        recipe = {"engine": "rgss", **game.to_dict()}
        self.found = {"game": game}
        return recipe

    def report(self, ctx, recipe):
        game = self.found["game"]
        packed = f", packed in {game.archive['path']}" if game.archive else ""
        return {
            "summary": f"RPG Maker {game.variant} game (RGSS{game.rgss}){packed}",
            "recipe": recipe,
            "title": game.title,
        }
