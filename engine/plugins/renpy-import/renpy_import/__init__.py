"""Ren'Py import: records the game's layout, version and platforms.

Nothing from the game is copied into the project. Archives are listed with
an unpickler that refuses all classes; they are never extracted.
"""

from __future__ import annotations

from ptk_engine.sdk import ImportPlugin
from ptk_engine.formats import renpy

NAMES = {"windows-x86_64": "Windows", "linux-x86_64": "Linux", "mac-universal": "macOS"}


class Plugin(ImportPlugin):
    recipe_name = "renpy"

    def detect(self, ctx):
        ctx.progress(0.2, "Looking for game/ and renpy/")
        try:
            game = renpy.detect(ctx.game)
        except renpy.RenpyError as e:
            ctx.fail(str(e))
        for note in game.notes:
            ctx.warn(note)
        for a in game.archives:
            if a.get("error"):
                ctx.warn(f"{a['path']}: {a['error']}")
        recipe = {"engine": "renpy", **game.to_dict()}
        self.found = {"game": game}
        return recipe

    def report(self, ctx, recipe):
        game = self.found["game"]
        has = ", ".join(NAMES[p] for p in renpy.PLATFORMS if p in game.platforms) or "no platform files"
        files = sum(a.get("files") or 0 for a in game.archives)
        return {
            "summary": f"Ren'Py {game.short_version or '?'} game ({has}), "
                       f"{len(game.archives)} archives with {files} files",
            "recipe": recipe,
            "title": game.title,
            "engine_version": game.short_version,
        }
