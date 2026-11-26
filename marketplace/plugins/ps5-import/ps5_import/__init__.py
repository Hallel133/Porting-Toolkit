"""PS5 title import: what a dumped PS5 title is, and whether AnyPS5 can convert it.

Only names, param.json and the first four bytes of each executable are
read. Nothing is decrypted, and nothing from the title goes into the
project: the recipe holds the title's ID, names and file list.
"""

from ptk_engine.sdk import ImportPlugin

from . import dump


class Plugin(ImportPlugin):
    recipe_name = "ps5"

    def detect(self, ctx):
        ctx.progress(0.2, "Reading the title")
        try:
            d = dump.read_dump(ctx.game)
        except dump.DumpError as e:
            ctx.fail(str(e))
        for note in d.notes:
            ctx.warn(note)
        if d.encrypted:
            ctx.warn(f"{len(d.encrypted)} executable file(s) are encrypted SELF containers ({', '.join(d.encrypted[:3])}"
                     f"{'...' if len(d.encrypted) > 3 else ''}). AnyPS5 converts decrypted ELF files only, and "
                     "Porting Toolkit does not decrypt.")
        recipe = {"engine": "ps5", **d.to_dict()}
        recipe.pop("root", None)
        self.found = d
        return recipe

    def report(self, ctx, recipe):
        d = self.found
        state = "ready for AnyPS5" if not d.encrypted else "encrypted: not convertible as it is"
        return {
            "summary": f"PS5 title {d.title_id or '(no title ID)'}, {len(d.modules)} bundled module(s), {state}",
            "recipe": recipe,
            "title": d.title or None,
            "engine_version": d.content_version,
        }
