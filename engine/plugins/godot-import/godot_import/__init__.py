"""Godot import: records where the game's .pck is and which Godot made it.

Nothing from the game is copied into the project; the recipe only holds
the pck's location (relative to the game folder) and its header facts.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

from ptk_engine.formats import godot


def describe(pck: godot.Pck) -> str:
    where = "embedded in the executable" if pck.embedded else "a separate .pck"
    return f"Godot {pck.version_string} game, {len(pck.files)} files ({where})"


class Plugin:
    def run(self, ctx):
        ctx.progress(0.1, "Looking for the .pck")
        candidates = godot.find(ctx.game)
        if not candidates:
            ctx.fail("no Godot .pck found: neither a .pck file nor an executable with an embedded pck")
        path = candidates[0]
        try:
            pck = godot.read(path)
        except godot.PckError as e:
            ctx.fail(str(e))
        ctx.progress(0.7, "Reading the pack header")
        rel = path.name if ctx.game.is_file() else str(path.relative_to(ctx.game))
        info = pck.summary()
        if pck.encrypted:
            ctx.warn("The pck is encrypted. Exporting needs templates compiled with the game's key, which only its developer has.")
        if pck.uses_csharp():
            ctx.warn("This is a C# (.NET) Godot game. The built-in exporter supports GDScript games only.")
        if pck.extensions():
            ctx.warn("Uses GDExtension native libraries (" + ", ".join(Path(e).stem for e in pck.extensions())
                     + "): every platform needs its own build of these.")
        if len(candidates) > 1:
            ctx.log("Other pcks: " + ", ".join(p.name for p in candidates[1:4]))
        settings = godot.project_settings(pck)
        major = pck.version[0]
        title = str(settings.get("application/config/name") or "").strip() or None
        icon_res = str(settings.get("application/config/icon") or "")
        recipe = {
            "engine": "godot",
            "pck": rel,
            **info,
            "title": title,
            "renderer": godot.renderer(settings, major) if settings else None,
            "icon": icon_res or None,
            "boot_splash": str(settings.get("application/boot_splash/image") or "") or None,
            "autoloads": sorted(k.split("/", 1)[1] for k in settings if k.startswith("autoload/")),
        }
        saved = ctx.save_recipe("godot", recipe)
        # The game's own icon, for this computer only (never in the project).
        icon = None
        data = godot.read_file(pck, icon_res) if icon_res.lower().endswith((".png", ".jpg", ".jpeg", ".webp")) else None
        if data:
            key = hashlib.sha256(f"{path.resolve()}|{pck.size}".encode()).hexdigest()[:16]
            icon = ctx.cache / "icons" / f"{key}{Path(icon_res).suffix.lower()}"
            ctx.write_bytes(icon, data)
        ctx.progress(1.0, "Done")
        return {
            "summary": describe(pck),
            "recipe": recipe,
            "outputs": [str(saved)],
            "title": title,
            "icon": str(icon) if icon else None,
        }
