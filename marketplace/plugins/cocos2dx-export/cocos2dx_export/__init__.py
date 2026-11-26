"""Cocos2d-x export: the game's Lua or JavaScript with the Cocos2d-x simulator.

The simulator is a Cocos2d-x build with the standard Lua and JavaScript
bindings. It runs a game's scripts when they are plain or LuaJIT bytecode
and the game adds no C++ code of its own. Encrypted scripts (Cocos2d-x's
XXTEA: .luac files that start with a sign, and every .jsc) can only be read
by the game's own executable, which holds the key, so they are refused.
"""

from pathlib import Path, PurePosixPath

from ptk_engine.sdk import RuntimeExportPlugin, error, warning

LUA_BYTECODE = (b"\x1bLua", b"\x1bLJ")


class Plugin(RuntimeExportPlugin):
    def _entry(self, files):
        """The entry script and the folder the simulator runs in (it holds src/ or main.js)."""
        for want in ("src/main.lua", "src/main.luac", "main.js", "main.jsc"):
            for f in files:
                if f.lower() == want or f.lower().endswith("/" + want):
                    path = PurePosixPath(f)
                    base = path.parent.parent if want.startswith("src/") else path.parent
                    return f, base.as_posix()
        return None, "."

    def check(self, ctx):
        problems = super().check(ctx)
        if any(p.severity == "error" for p in problems):
            return problems
        entry, _ = self._entry(self._files)
        game = Path(ctx.game)
        if any(f.lower().endswith(".jsc") for f in self._files):
            return [error("the game's JavaScript is compiled to .jsc, which Cocos2d-x encrypts with a key only "
                          "the game's own executable has; the simulator cannot run it")]
        for f in [x for x in self._files if x.lower().endswith(".luac")][:20]:
            with open(game / f, "rb") as fh:
                head = fh.read(8)
            if head and not head.startswith(LUA_BYTECODE):
                return [error(f"{f} is encrypted (Cocos2d-x XXTEA); only the game's own executable has the key, "
                              "so the simulator cannot run it")]
        problems.append(warning("The simulator must be built from the same Cocos2d-x version as the game. If the "
                                "game adds its own C++ code, the parts that call it will not work."))
        return problems

    def launch_args(self, ctx, files, main):
        entry, base = self._entry(files)
        workdir = "{game}" if base in (".", "") else "{game}/" + base
        args = ["-workdir", workdir]
        if entry and not entry.lower().endswith(("src/main.lua", "src/main.luac")):
            args += ["-entry", PurePosixPath(entry).name]
        return args
