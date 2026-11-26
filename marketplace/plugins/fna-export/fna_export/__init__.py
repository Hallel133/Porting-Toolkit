"""XNA export: an XNA 4 game with FNA, run by MonoKickstart.

The build is one folder holding the game and the FNA kit:

- the kit's FNA.dll and ``Microsoft.Xna.Framework*.dll`` (FNA's XNA
  assemblies, which forward to FNA, so the game runs unchanged);
- the kit's Mono class library (mscorlib.dll, System*.dll...), its
  ``monoconfig`` and ``monomachineconfig``;
- ``osx/`` (macOS) or ``lib64/`` (Linux): libmono and fnalibs;
- ``kick.bin.osx`` / ``kick.bin.x86_64`` renamed ``<Game>.bin.osx`` /
  ``<Game>.bin.x86_64``: MonoKickstart starts the .exe of the same name,
  and reads monoconfig from the folder it starts in.

macOS gets ``<Title>.app`` (the folder in Resources/game, a launcher that
starts the .bin.osx there), Linux ``<Title>/`` with ``<Title>.sh`` and a
zip. Settings: ``runtime`` (the kit folder), ``exe`` (the game's .exe when
there are several).
"""

from __future__ import annotations

from pathlib import Path, PurePosixPath

from ptk_engine import runtimes
from ptk_engine.sdk import RuntimeExportPlugin, error, warning

KICK = {"macos": ("kick.bin.osx", "bin.osx", "osx"), "linux": ("kick.bin.x86_64", "bin.x86_64", "lib64")}
KIT_FILES = ["FNA.dll", "monoconfig", "monomachineconfig", "mscorlib.dll"]
NOT_GAMES = ("setup", "unins", "install", "crash", "launcher", "dxsetup", "vcredist", "xnafx", "redist")

LAUNCHER = """#!/bin/sh
# Made by Porting Toolkit: starts the XNA game with FNA (MonoKickstart).
HERE="$(cd "$(dirname "$0")" && pwd)"
GAME="$(cd "$HERE/{base}" && pwd)"
cd "$GAME" || exit 1
exec "$GAME/{kick}" "$@"
"""


class Plugin(RuntimeExportPlugin):
    def _exe(self, ctx, files):
        chosen = ctx.setting("exe")
        top = [f for f in files if "/" not in f and f.lower().endswith(".exe")]
        if chosen:
            return next((f for f in top if f.lower() == chosen.lower()), None)
        games = [f for f in top if not any(w in f.lower() for w in NOT_GAMES)]
        return games[0] if len(games) == 1 else None

    def check(self, ctx):
        problems = super().check(ctx)
        if any(p.severity == "error" for p in problems):
            return [error("XNA games need an FNA kit, built once (see KIT.md of the fna-export plugin): export "
                          "with --set runtime=PATH/TO/fna-kit") if "no official build" in p.message else p
                    for p in problems]
        os_ = (ctx.target or {}).get("os")
        self._game_exe = self._exe(ctx, self._files)
        if self._game_exe is None:
            top = [f for f in self._files if "/" not in f and f.lower().endswith(".exe")]
            return [error("choose the game's .exe with --set exe=NAME.exe" + (f" (found {', '.join(top)})" if top
                          else ": no .exe at the top of the game folder"))]
        found = runtimes.locate(ctx, self.spec(), ctx.target or {}, download=False)
        kit = found.root if found else None
        if kit is not None:
            libdir = KICK[os_][2]
            missing = [n for n in KIT_FILES if not (kit / n).is_file()]
            if not (kit / libdir).is_dir():
                missing.append(f"{libdir}/")
            if not any(kit.glob("Microsoft.Xna.Framework*.dll")):
                missing.append("Microsoft.Xna.Framework*.dll (FNA's XNA assemblies)")
            if missing:
                return [error(f"the FNA kit at {kit} is missing {', '.join(missing)} (see KIT.md of this plugin)")]
        low = [f.lower() for f in self._files]
        if any(f.endswith(".wmv") for f in low):
            problems.append(warning("The game has .wmv videos, which FNA cannot play: convert them to .ogv (Theora) "
                                    "with the same names, or the videos are skipped."))
        if any(PurePosixPath(f).name in ("steam_api.dll", "steamworks.net.dll") for f in low):
            problems.append(warning("The game uses Steamworks: Steam features need the macOS/Linux build of "
                                    "steam_api, which is not part of the kit."))
        if os_ == "macos" and (ctx.target or {}).get("arch") == "arm64":
            problems.append(warning("MonoKickstart builds are usually Intel-only: on Apple Silicon the game runs "
                                    "through Rosetta 2 unless your kit is built for arm64."))
        return problems

    def layout(self, ctx, build):
        os_ = (ctx.target or {}).get("os")
        kick_name, suffix, _ = KICK[os_]
        kit = self._found.root
        title = ctx.title
        name = runtimes.safe_name(title)
        stem = PurePosixPath(self._game_exe).stem
        kick = f"{stem}.{suffix}"
        if os_ == "macos":
            app = f"{name}.app"
            folder = f"{app}/Contents/Resources/game"
            build.add_text(f"{app}/Contents/MacOS/{name}", LAUNCHER.format(base="../Resources/game", kick=kick),
                           mode=0o755)
            bundle = ctx.setting("bundle_id") or f"org.portingtoolkit.{runtimes.slug(title)}"
            build.add_bytes(f"{app}/Contents/Info.plist", runtimes.info_plist(
                title, name, bundle, str(ctx.manifest.get("version") or "1.0.0"), None))
            self._outputs = {"app": app}
        else:
            folder = f"{name}/game"
            build.add_text(f"{name}/{name}.sh", LAUNCHER.format(base="game", kick=kick), mode=0o755)
            if ctx.flag("zip", True):
                build.artifact(f"{name}.zip")
            self._outputs = {"folder": name}
        game = Path(ctx.game)
        for rel in self._files:
            build.add_file(game / rel, f"{folder}/{rel}")
        # The kit after the game: its FNA assemblies replace any the game ships.
        skip = {k for k, _, _ in KICK.values()}
        build.add_tree(kit, folder, links=True, skip=lambda p: p.name in skip or p.name == ".DS_Store")
        build.add_file(kit / kick_name, f"{folder}/{kick}", mode=0o755)
        build.add_text(f"{folder}/LICENSE-fna.txt", runtimes.licence_note(self.spec()))
