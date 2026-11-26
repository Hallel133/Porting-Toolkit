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


class RuntimeExportPlugin(ExportPlugin):
    """Exports a game's data with an open-source runtime (ScummVM, GemRB...),
    described by ``runtime.toml`` next to the plugin's ``plugin.toml`` (see
    :mod:`ptk_engine.runtimes`). A plugin usually only sets nothing at all;
    it may override :meth:`launch_args` or :meth:`extra_layout`."""

    def spec(self):
        from .runtimes import RuntimeSpec
        if not hasattr(self, "_spec"):
            import inspect
            root = Path(inspect.getfile(type(self))).resolve().parent.parent
            self._spec = RuntimeSpec.load(root / "runtime.toml")
        return self._spec

    # -- steps a runtime plugin may change --------------------------------------------------------------

    def launch_args(self, ctx: Context, files: list[str], main: str | None) -> list[str]:
        """The runtime's arguments (``{game}``, ``{runtime}``, ``{main}`` are
        filled in when the game starts)."""
        return list(self.spec().args)

    def extra_layout(self, ctx: Context, build: Build, game_dest: str) -> None:
        """More files for the build (``game_dest`` is the game folder in it)."""

    # -- the lifecycle -------------------------------------------------------------------------------------

    def check(self, ctx: Context) -> list[Problem]:
        from . import runtimes
        spec = self.spec()
        target = ctx.target or {}
        os_ = target.get("os", "")
        if os_ not in spec.executable:
            return [error(f"{spec.name} builds are made for {', '.join(sorted(spec.executable))}, "
                          f"not {target.get('name') or target.get('id')}")]
        if ctx.game is None or not Path(ctx.game).is_dir():
            return [error("choose the game folder first")]
        self._files = runtimes.game_files(Path(ctx.game), spec)
        if runtimes.required_found(self._files, spec) is None:
            return [error(f"this folder has none of the files {spec.name} needs ({', '.join(spec.required)}); "
                          "choose the folder with the game's data")]
        self._main = runtimes.main_file(self._files, spec)
        if spec.main and self._main is None:
            return [error(f"no {' or '.join(spec.main)} in the game folder")]
        try:
            found = runtimes.locate(ctx, spec, target, download=False)
        except runtimes.RuntimeError_ as e:
            return [error(str(e))]
        if found is None and not runtimes.can_download(spec, target):
            return [error(f"{spec.name} has no official build for {target.get('name') or os_} that Porting "
                          f"Toolkit knows; download one from {spec.source} and export with --set runtime=PATH")]
        return []

    def prepare(self, ctx: Context) -> None:
        from . import runtimes
        self._found = runtimes.locate(ctx, self.spec(), ctx.target or {}, download=True)
        if self._found is None:  # check() made sure there is a way; keep the error clear anyway
            raise PluginError(f"{self.spec().name} is not available for this target")

    def variant(self, ctx: Context) -> str:
        found = getattr(self, "_found", None)
        return f"{self.spec().id}|{self.spec().version}|{found.executable if found else ''}"

    def estimate(self, ctx: Context) -> dict:
        return {"summary": f"Nothing to estimate: {self.spec().name} uses the game's files as they are."}

    def layout(self, ctx: Context, build: Build) -> None:
        from . import runtimes
        spec, found = self.spec(), self._found
        os_ = (ctx.target or {}).get("os", "")
        title = ctx.title
        name = runtimes.safe_name(title)
        game = Path(ctx.game)
        args = self.launch_args(ctx, self._files, self._main)
        licence = runtimes.licence_note(spec)
        if os_ == "macos":
            app = f"{name}.app"
            res = f"{app}/Contents/Resources"
            build.add_tree(found.root, f"{res}/runtime/{found.root.name}", links=True)
            exe_rel = f"{found.root.name}/" + found.executable.relative_to(found.root).as_posix()
            build.add_text(f"{app}/Contents/MacOS/{name}",
                           runtimes.sh_launcher(exe_rel, args, self._main, "../Resources", spec.env), mode=0o755)
            icon = runtimes.app_icon(found.root) if found.root.suffix == ".app" else None
            if icon:
                build.add_file(icon, f"{res}/icon.icns")
            bundle = ctx.setting("bundle_id") or f"org.portingtoolkit.{runtimes.slug(title)}"
            version = str(ctx.manifest.get("version") or "1.0.0")
            build.add_bytes(f"{app}/Contents/Info.plist",
                            runtimes.info_plist(title, name, bundle, version, "icon" if icon else None))
            build.add_text(f"{res}/LICENSE-{spec.id}.txt", licence)
            game_dest = f"{res}/{spec.game_dir}"
            self._outputs = {"app": app}
        else:
            if found.root.is_file():  # an AppImage
                build.add_file(found.root, f"{name}/runtime/{found.root.name}", mode=0o755)
                exe_rel = found.root.name
            else:
                build.add_tree(found.root, f"{name}/runtime", links=True)
                exe_rel = found.executable.relative_to(found.root).as_posix()
                build.executable(f"{name}/runtime/{exe_rel}")
            if os_ == "windows":
                build.add_text(f"{name}/{name}.cmd", runtimes.cmd_launcher(exe_rel, args, self._main, spec.env))
            else:
                build.add_text(f"{name}/{name}.sh", runtimes.sh_launcher(exe_rel, args, self._main, ".", spec.env), mode=0o755)
            build.add_text(f"{name}/LICENSE-{spec.id}.txt", licence)
            game_dest = f"{name}/{spec.game_dir}"
            if ctx.flag("zip", True):
                build.artifact(f"{name}.zip")
            self._outputs = {"folder": name}
        for rel in self._files:
            build.add_file(game / rel, f"{game_dest}/{rel}")
        self.extra_layout(ctx, build, game_dest)

    def finish(self, ctx: Context, build: Build) -> dict:
        from .build import codesign_if_changed
        spec = self.spec()
        notes = [f"Runs on {spec.name} {spec.version} ({spec.license}; source: {spec.source})."]
        if "app" in self._outputs:
            app = build.root / self._outputs["app"]
            if ctx.flag("sign", True) and ctx.which("codesign"):
                ctx.progress(0.9, "Signing (ad hoc)")
                codesign_if_changed(app, ctx.run)
            return {"outputs": [str(app)], "open": str(app), "notes": notes,
                    "summary": f"Built {app.name} with {spec.name} {spec.version}"}
        folder = build.root / self._outputs["folder"]
        outputs = [str(folder)]
        if ctx.flag("zip", True):
            outputs.append(str(self.zip(ctx, build, self._outputs["folder"], f"{folder.name}.zip")))
        return {"outputs": outputs, "notes": notes,
                "summary": f"Built {folder.name}/ for {(ctx.target or {}).get('name')} with {spec.name} {spec.version}"}


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


__all__ = ["ExportPlugin", "ImportPlugin", "RuntimeExportPlugin", "StagedExportPlugin", "Problem", "ProblemsError", "error", "warning", "options_set"]
