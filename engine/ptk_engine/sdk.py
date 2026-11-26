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


def options_set(ctx: Context) -> bool:
    """Whether the project asks to drop or recompress any asset category."""
    options = (getattr(ctx, "manifest", None) or {}).get("options") or {}
    return any(isinstance(o, dict) and (o.get("action") or "keep") != "keep" for o in options.values())


class ExportPlugin:
    #: Media profile for ``ctx.process_assets`` (None: this plugin never
    #: re-encodes assets). See ``ptk_engine.media.profiles``.
    media_profile: str | None = None
    #: False when this exporter never applies the advanced options (drop or
    #: recompress assets); a warning then says they are ignored.
    applies_options: bool = True
    #: Why the advanced options are ignored, shown in that warning.
    options_ignored_reason: str = "this exporter keeps the game's files as they are"

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

    def problems(self, ctx: Context) -> list[Problem]:
        """``check(ctx)`` plus the base class's own notes."""
        problems = list(self.check(ctx) or [])
        if not self.applies_options and options_set(ctx):
            problems.append(warning(
                f"The advanced options (drop or recompress) are ignored: {self.options_ignored_reason}. "
                "The export is the same as with every option on Keep."))
        return problems

    def check_result(self, ctx: Context) -> dict:
        problems = self.problems(ctx)
        return {"problems": [p.to_dict() for p in problems],
                "ok": not any(p.severity == "error" for p in problems)}

    def run(self, ctx: Context) -> dict:
        problems = self.problems(ctx)
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


class StagedExportPlugin(ExportPlugin):
    """For exporters that build their output step by step (templates to
    patch, runtimes to unpack) rather than declare it.

    ``stage(ctx)`` builds into ``ctx.output`` as an API 1 ``run`` would, but
    ``ctx.output`` is a staging folder of APFS clones (instant on macOS). The
    base class then syncs it into the real output, so only files that
    changed are written; zips (``ctx.zip_dir``) and ad-hoc signing
    (``ctx.run(["codesign", ...])``) are done afterwards on the real output,
    incrementally. Paths in the result are mapped to the real output.
    """

    def stage(self, ctx: Context) -> dict:
        raise NotImplementedError

    def layout(self, ctx: Context, build: Build) -> None:  # pragma: no cover - see run
        raise NotImplementedError

    def run(self, ctx: Context) -> dict:
        import subprocess

        from .build import codesign_if_changed

        problems = self.problems(ctx)
        errors = [p for p in problems if p.severity == "error"]
        if errors:
            raise ProblemsError(errors)
        for p in problems:
            ctx.warn(p.message)
        self.prepare(ctx)
        if ctx.output is None:  # an estimate: stage() returns the report
            return self.stage(ctx)
        real = Path(ctx.output)
        staging = ctx.temp / "ptk-stage"
        staging.mkdir(parents=True, exist_ok=True)
        zips: list[tuple[str, str, set | None]] = []
        signs: list[tuple[str, list]] = []
        real_zip, real_run = ctx.zip_dir, ctx.run

        def rel(path) -> str:
            return Path(path).resolve().relative_to(staging.resolve()).as_posix()

        def staged_zip(src, dest, executables=None):
            zips.append((rel(src), rel(dest), executables))
            return real / rel(dest)

        def staged_run(args, *a, **kw):
            argv = [str(x) for x in args]
            if argv and Path(argv[0]).name == "codesign" and "--sign" in argv and argv[argv.index("--sign") + 1] == "-":
                ent = argv[argv.index("--entitlements") + 1] if "--entitlements" in argv else None
                signs.append((rel(argv[-1]), [ent] if ent else []))
                return subprocess.CompletedProcess(argv, 0, "", "")
            return real_run(args, *a, **kw)

        ctx.output, ctx.zip_dir, ctx.run = staging, staged_zip, staged_run
        try:
            result = self.stage(ctx) or {}
        finally:
            ctx.output, ctx.zip_dir, ctx.run = real, real_zip, real_run
        ctx.progress(None, "Writing the output")
        build = ctx.build(root=real, variant=self.variant(ctx))
        build.add_tree(staging, "", links=True, by_date=True, skip=lambda p: p.name == ".ptk-output")
        for _, dest, _ in zips:
            build.artifact(dest)
        report = build.sync()
        for app, ent in signs:
            codesign_if_changed(real / app, ctx.run, Path(ent[0]) if ent else None)
        for src, dest, executables in zips:
            ctx.zip_dir(real / src, real / dest, executables)
        build.seal()
        result = _remap(result, str(staging.resolve()), str(real))
        result = _remap(result, str(staging), str(real))
        result.setdefault("outputs", [])
        result["build"] = {"written": len(report.written), "unchanged": report.unchanged,
                           "removed": len(report.removed), "cloned": report.cloned, "rebuilt": report.rebuilt}
        return result


def _remap(value, old: str, new: str):
    if isinstance(value, str):
        return new + value[len(old):] if value == old or value.startswith(old + "/") else value
    if isinstance(value, list):
        return [_remap(v, old, new) for v in value]
    if isinstance(value, dict):
        return {k: _remap(v, old, new) for k, v in value.items()}
    return value


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


__all__ = ["ExportPlugin", "ImportPlugin", "StagedExportPlugin", "Problem", "ProblemsError", "error", "warning", "options_set"]
