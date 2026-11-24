"""LÖVE import: records where the game's .love is and what conf.lua says.

Nothing from the game is copied into the project; the game's icon (when
conf.lua names one) is cached for this computer only.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

from ptk_engine.formats import love

KINDS = {"love": "a .love file", "fused": "fused into the executable", "app": "inside a Mac app",
         "folder": "a source folder"}


class Plugin:
    def run(self, ctx):
        ctx.progress(0.2, "Looking for the .love")
        try:
            game = love.detect(ctx.game)
        except love.LoveError as e:
            ctx.fail(str(e))
        for note in game.notes:
            ctx.warn(note)
        v = game.love_version
        if v and not v.startswith("11."):
            ctx.warn(f"The game was made for LÖVE {v}; exports use LÖVE 11.5, which may not run it. "
                     "Pass --set love=PATH with a LÖVE build of the game's version if it does not start.")
        recipe = {"engine": "love", **game.to_dict()}
        saved = ctx.save_recipe("love", recipe)
        icon = None
        if game.icon and game.icon.lower().endswith((".png", ".jpg", ".jpeg")):
            data = love.read(ctx.game, recipe, game.icon)
            if data:
                key = hashlib.sha256(f"{Path(ctx.game).resolve()}|{game.source}|{game.size}".encode()).hexdigest()[:16]
                icon = ctx.write_bytes(ctx.cache / "icons" / f"{key}{Path(game.icon).suffix.lower()}", data)
        ctx.progress(1.0, "Done")
        return {
            "summary": f"LÖVE {v or '(version not set)'} game, {game.files} files, {KINDS[game.kind]}",
            "recipe": recipe,
            "outputs": [str(saved)],
            "title": game.title or None,
            "engine_version": game.love_version,
            "icon": str(icon) if icon else None,
        }
