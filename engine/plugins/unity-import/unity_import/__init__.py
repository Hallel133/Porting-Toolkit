"""Unity import: what the build is made of, and whether it can move.

Nothing from the game is copied into the project.
"""

from __future__ import annotations

from ptk_engine.formats import unity

PLATFORMS = {"windows": "Windows", "linux": "Linux", "macos": "macOS"}


class Plugin:
    def run(self, ctx):
        ctx.progress(0.2, "Looking for the Data folder")
        try:
            game = unity.detect(ctx.game)
        except unity.UnityError as e:
            ctx.fail(str(e))
        for note in game.notes:
            ctx.warn(note)
        if game.backend == "mono" and game.plugins:
            others = [p for p in game.plugins if p not in game.apple_services]
            if others:
                ctx.warn(f"Native plugins ({', '.join(others[:6])}{'…' if len(others) > 6 else ''}) are built for "
                         f"{PLATFORMS[game.platform]} only; another platform needs their own builds, so the "
                         "features that use them will not work there.")
        recipe = {"engine": "unity", **game.to_dict()}
        saved = ctx.save_recipe("unity", recipe)
        drawable = [PLATFORMS[t] for t in PLATFORMS
                    if t != game.platform and unity.can_draw(game.to_dict(), t) is not False]
        if game.backend != "mono":
            verdict = "cannot move without source code (IL2CPP)"
        elif not drawable:
            verdict = (f"its code could move, but its shaders are {', '.join(game.shaders['apis'])} only, "
                       "so other platforms would not draw it")
        elif game.shaders:
            verdict = (f"can move to {', '.join(drawable)} with the Unity player of the same version "
                       "(experimental)")
        else:
            verdict = "can move to other platforms with the Unity player of the same version (experimental)"
        ctx.progress(1.0, "Done")
        return {
            "summary": f"Unity {game.version or '?'} {game.backend.upper() if game.backend == 'il2cpp' else 'Mono'} "
                       f"game for {PLATFORMS[game.platform]}: {verdict}",
            "recipe": recipe,
            "outputs": [str(saved)],
            "title": game.product,
            "engine_version": game.version,
        }
