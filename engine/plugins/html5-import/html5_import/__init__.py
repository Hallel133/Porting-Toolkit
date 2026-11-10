"""HTML5 import: records where the game's web root is.

Nothing from the game is copied into the project; the recipe holds the
container (folder, package.nw or app.asar), the web root inside it and the
window size from package.json.
"""

from __future__ import annotations

from ptk_engine.formats import html5

FLAVORS = {
    "rpgmaker-mv": "RPG Maker MV",
    "rpgmaker-mz": "RPG Maker MZ",
    "nwjs": "NW.js",
    "electron": "Electron",
    "web": "HTML5",
}


class Plugin:
    def run(self, ctx):
        ctx.progress(0.2, "Looking for the web root")
        try:
            game = html5.detect(ctx.game)
        except html5.Html5Error as e:
            if (ctx.engine or "") == "rpgmaker":
                ctx.fail(f"{e}. RPG Maker XP, VX and VX Ace games are not HTML5; they need an RGSS runtime instead.")
            ctx.fail(str(e))
        for note in game.notes:
            ctx.warn(note)
        if game.uses_node and game.flavor != "electron":
            ctx.warn("The page calls Node.js (require/process); those parts will not work in a browser.")
        recipe = {"engine": ctx.engine or "html5", **game.to_dict()}
        saved = ctx.save_recipe("html5", recipe)
        where = game.container if game.container != "folder" else "a folder"
        root = game.root or "(top level)"
        ctx.progress(1.0, "Done")
        return {
            "summary": f"{FLAVORS.get(game.flavor, game.flavor)} game: {game.entry} in {root}, from {where}",
            "recipe": recipe,
            "outputs": [str(saved)],
        }
