"""Plugin API 2: base classes with a fixed lifecycle.

An **export** plugin subclasses :class:`ExportPlugin` and fills in steps;
the base class runs them in order and owns the output folder::

    check(ctx)            -> list[Problem]   fast, writes nothing (also run alone
                                             by ``ptk export --check`` and the GUI)
    prepare(ctx)                             downloads, tools, anything slow
    layout(ctx, build)                       declare the output (``build.add_*``)
    -- the base class syncs the output: only changed files are written --
    finish(ctx, build)    -> dict            in-place steps (plists, media,
                                             signing, zips) and the result

An **import** plugin subclasses :class:`ImportPlugin`: ``detect(ctx)``
returns the recipe (saved as ``recipes/<recipe_name>.json``) and
``report(ctx, recipe)`` returns the summary, title and engine version.

``plugin.toml`` says ``api = 2``. API 1 plugins (a class with ``run(ctx)``)
still run unchanged.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
from pathlib import Path

from .api import Context, PluginError
from .build import Build


@dataclass
class Problem:
    """Something that stops (``error``) or limits (``warning``) an export."""

    message: str
    severity: str = "error"  # "error" | "warning"
    #: A fix the user can apply in one click: ``{"setting": key, "value": "true",
    #: "label": "Accept Unity's terms", "url": optional page to read first}``.
    action: dict | None = None

    def to_dict(self) -> dict:
        return {k: v for k, v in asdict(self).items() if v is not None}


class ProblemsError(PluginError):
    def __init__(self, problems: list[Problem]):
        self.problems = problems
        super().__init__("; ".join(p.message for p in problems))


def error(message: str, action: dict | None = None) -> Problem:
    return Problem(message, "error", action)


def warning(message: str, action: dict | None = None) -> Problem:
    return Problem(message, "warning", action)


class ExportPlugin:
    #: Media profile for ``ctx.process_assets`` (None: this plugin never
    #: re-encodes assets). See ``ptk_engine.media.profiles``.
    media_profile: str | None = None

    # -- steps a plugin fills in -------------------------------------------------------------

    def check(self, ctx: Context) -> list[Problem]:
        return []

    def prepare(self, ctx: Context) -> None:
        pass

    def layout(self, ctx: Context, build: Build) -> None:
        raise NotImplementedError

    def finish(self, ctx: Context, build: Build) -> dict:
        return {}

    def variant(self, ctx: Context) -> str:
        """Anything besides the declared entries that changes the output."""
        return ""

    # -- the lifecycle -----------------------------------------------------------------------

    def check_result(self, ctx: Context) -> dict:
        problems = self.check(ctx)
        return {"problems": [p.to_dict() for p in problems],
                "ok": not any(p.severity == "error" for p in problems)}

    def run(self, ctx: Context) -> dict:
        problems = self.check(ctx)
        errors = [p for p in problems if p.severity == "error"]
        if errors:
            raise ProblemsError(errors)
        for p in problems:
            ctx.warn(p.message)
        self.prepare(ctx)
        if ctx.output is None:  # an estimate (assets=plan): nothing is built
            return self.estimate(ctx)
        build = ctx.build(variant=self.variant(ctx))
        self.layout(ctx, build)
        ctx.progress(None, "Writing the output")
        report = build.sync()
        result = self.finish(ctx, build) or {}
        build.seal()
        result.setdefault("outputs", [])
        result["build"] = {"written": len(report.written), "unchanged": report.unchanged,
                           "removed": len(report.removed), "cloned": report.cloned,
                           "rebuilt": report.rebuilt}
        return result

    def estimate(self, ctx: Context) -> dict:
        """``assets=plan``: what the advanced options would save."""
        if not self.media_profile or ctx.game is None:
            return {"summary": "Nothing to estimate: this exporter does not re-encode assets."}
        report = ctx.process_assets(ctx.game, self.media_profile)
        return {"summary": "Estimated the savings of the advanced options", "assets": report}

    # -- helpers ----------------------------------------------------------------------------------

    @staticmethod
    def media(ctx: Context, build: Build, root_rel: str, profile: str) -> dict | None:
        """Applies the advanced options to ``build.root/root_rel`` in place,
        reading the game's own files (so a re-export re-encodes nothing)."""
        return ctx.process_assets(build.root / root_rel, profile, build=build)

    @staticmethod
    def zip(ctx: Context, build: Build, folder_rel: str, name: str, executables: set[str] | None = None) -> Path:
        """``build.root/name`` from ``folder_rel`` (declare it first with
        ``build.artifact(name)`` in ``layout``)."""
        return ctx.zip_dir(build.root / folder_rel, build.root / name, executables)


class ImportPlugin:
    #: The recipe file name (``recipes/<name>.json``).
    recipe_name: str = "import"

    def detect(self, ctx: Context) -> dict:
        raise NotImplementedError

    def report(self, ctx: Context, recipe: dict) -> dict:
        return {"summary": "Imported"}

    def run(self, ctx: Context) -> dict:
        ctx.progress(0.2, "Reading the game")
        recipe = self.detect(ctx)
        saved = ctx.save_recipe(self.recipe_name, recipe)
        result = self.report(ctx, recipe) or {}
        result.setdefault("recipe", recipe)
        result["outputs"] = [*result.get("outputs", []), str(saved)]
        ctx.progress(1.0, "Done")
        return result

    def check_result(self, ctx: Context) -> dict:
        return {"problems": [], "ok": True}


__all__ = ["ExportPlugin", "ImportPlugin", "Problem", "ProblemsError", "error", "warning"]
