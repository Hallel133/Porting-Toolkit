"""HTML5 import: records where the game's web root is.

Nothing from the game is copied into the project; the recipe holds the
container (folder, package.nw or app.asar), the web root inside it and the
window size from package.json.
"""

from __future__ import annotations

from ptk_engine.sdk import ImportPlugin
from ptk_engine.formats import html5

FLAVORS = {
    "rpgmaker-mv": "RPG Maker MV",
    "rpgmaker-mz": "RPG Maker MZ",
    "nwjs": "NW.js",
    "electron": "Electron",
    "web": "HTML5",
}


class Plugin(ImportPlugin):
    recipe_name = "html5"

    def detect(self, ctx):
        ctx.progress(0.2, "Looking for the web root")
        try:
            game = html5.detect(ctx.game)
        except html5.Html5Error as e:
            if (ctx.engine or "") == "rpgmaker":
                ctx.fail(f"{e}. RPG Maker XP, VX and VX Ace games are not HTML5: import the game again "
                         "(ptk import) so it is recognised as an RGSS game.")
            ctx.fail(str(e))
        for note in game.notes:
            ctx.warn(note)
        if game.uses_node and game.flavor != "electron":
            ctx.warn("The page calls Node.js (require/process); those parts will not work in a browser.")
        recipe = {"engine": ctx.engine or "html5", **game.to_dict()}
        self.found = {"game": game}
        return recipe

    def report(self, ctx, recipe):
        game = self.found["game"]
        where = game.container if game.container != "folder" else "a folder"
        root = game.root or "(top level)"
        kind = FLAVORS.get(game.flavor, game.flavor)
        if game.framework:
            kind = game.framework if game.flavor == "web" else f"{game.framework} ({kind})"
        return {
            "summary": f"{kind} game: {game.entry} in {root}, from {where}",
            "recipe": recipe,
            "title": game.title,
        }
