"""PS5 title export: AnyPS5 relinks the title's executable into a native one.

AnyPS5 (https://github.com/boykopovar/AnyPS5, GPL-2.0) converts a decrypted
PS5 ELF executable and its bundled modules into a Linux ELF, a Windows .exe
or an x86-64 Mac executable, and ships the PS5 system libraries as native
libraries (``prx-linux`` / ``prx-windows``). No emulator runs: the title's
own code runs natively, so the CPU needs the PS5's x86 extensions (AVX2,
FMA, F16C, BMI1/2, LZCNT, MOVBE).

The build follows AnyPS5's runtime layout:

    <Title>/<Title>.elf | <Title>.exe     the relinked executable
    <Title>/libs/*.prx                    AnyPS5's system libraries
    <Title>/app0/...                      the title's files (eboot.bin, sce_sys/...)
    <Title>/app0/sce_module/*.guest.prx   the relinked bundled modules

The relinker runs on this computer at export time (the official Linux or
Windows build, SHA-256 checked; on macOS a build you made with
``--set relinker=PATH``). Its output is cached, so a re-export of an
unchanged title does not relink. Nothing is decrypted: an encrypted
executable is refused.

Settings: ``relinker`` (a relinker you built), ``libs`` (a system-library
folder you built; needed for macOS), ``zip`` (default off: titles are large).
"""

from __future__ import annotations

import hashlib
import platform
import shutil
from pathlib import Path

from ptk_engine import runtimes
from ptk_engine.sdk import ExportPlugin, error, warning

from . import dump

VERSION = "v0.1.5"
RELEASES = f"https://github.com/boykopovar/AnyPS5/releases/download/{VERSION}/"
RELINKER = {  # host system -> (file, sha256), from the release's published digests
    "Linux": (f"relinker-{VERSION}", "6501ce4f4f28b26ebeebd34b79b84dbcd10a0ffdf331d21a766006d49e2e2bd1"),
    "Windows": (f"relinker-{VERSION}.exe", "541fa50fb7255be93111b72ebe5c320f3ccb936905f8f522914f233177ca17e7"),
}
LIBS = {  # target os -> (file, sha256)
    "linux": (f"prx-linux-{VERSION}.tar.gz", "b2769731214e0315d70ad682cd1ded1433d754f9e5286290dfdc768b43caff37"),
    "windows": (f"prx-windows-{VERSION}.zip", "d026931807f977d9ce21d07bcecd8dd229e652ec849414ac69cefcab4cc96a57"),
}
FLAGS = {"linux": [], "windows": ["--windows"], "macos": ["--macos"]}
LICENCE = f"""AnyPS5 {VERSION}
Licence: GPL-2.0-only
Source code: https://github.com/boykopovar/AnyPS5

This build contains the title's executable relinked by AnyPS5 and AnyPS5's system
libraries (libs/). AnyPS5 is free software under the GNU GPL version 2; its complete
source code is available at the address above. The title's own files belong to their
owners. AnyPS5 is not affiliated with or endorsed by Sony Interactive Entertainment.
"""


def _sha(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class Plugin(ExportPlugin):
    def check(self, ctx):
        target = ctx.target or {}
        os_ = target.get("os")
        if os_ not in FLAGS:
            return [error(f"AnyPS5 converts PS5 titles for Linux, Windows and macOS, not {target.get('id')}")]
        try:
            self._dump = dump.read_dump(Path(ctx.game))
        except (dump.DumpError, OSError) as e:
            return [error(str(e))]
        d = self._dump
        if d.encrypted:
            return [error(f"{', '.join(d.encrypted[:3])} {'is' if len(d.encrypted) == 1 else 'are'} encrypted (SELF). "
                          "AnyPS5 converts decrypted ELF executables only, and Porting Toolkit does not decrypt.")]
        if d.executable_kind != "elf":
            return [error("eboot.bin is not an ELF executable")]
        problems = [warning(n) for n in d.notes]
        relinker = ctx.setting("relinker")
        if relinker and not Path(relinker).expanduser().is_file():
            return [error(f"relinker={relinker} is not a file")]
        if not relinker and platform.system() not in RELINKER:
            return [error("AnyPS5 publishes its relinker for Linux and Windows only. On a Mac, build the "
                          "relinker-only version (AnyPS5's docs/dev/BUILD.md; it runs on Apple silicon) and pass "
                          "--set relinker=PATH.")]
        libs = ctx.setting("libs")
        if libs and not any(Path(libs).expanduser().rglob("libc.prx")):
            return [error(f"libs={libs} has no libc.prx: give the folder of AnyPS5's system libraries")]
        if not libs and os_ not in LIBS:
            return [error("AnyPS5 publishes no macOS system libraries: build them (AnyPS5's docs/dev/BUILD.md) "
                          "and pass --set libs=PATH to the folder holding libc.prx.")]
        problems.append(warning("Experimental: AnyPS5's compatibility differs per title (see its Game bring-up "
                                "discussions). The title runs natively, with no sandbox: run only titles you trust, "
                                "on a CPU with AVX2, FMA, F16C, BMI1/2, LZCNT and MOVBE."))
        if os_ == "linux" and not libs:
            problems.append(warning(f"AnyPS5's {VERSION} Linux libraries need glibc 2.43 (Ubuntu 26.04, Fedora 44 "
                                    "or newer)."))
        if os_ == "macos":
            problems.append(warning("The Mac build is an x86-64 program: on Apple silicon it runs through Rosetta 2."))
        return problems

    # -- prepare: relinker, system libraries, and the relink itself ------------------------------------------

    def _relinker(self, ctx) -> Path:
        chosen = ctx.setting("relinker")
        if chosen:
            return Path(chosen).expanduser()
        name, sha = RELINKER[platform.system()]
        ctx.progress(0.05, f"Downloading AnyPS5's relinker {VERSION} (once)")
        path = ctx.download(RELEASES + name, sha256=sha)
        path.chmod(0o755)
        return path

    def _libs(self, ctx, os_: str) -> Path:
        chosen = ctx.setting("libs")
        if chosen:
            root = Path(chosen).expanduser()
        else:
            name, sha = LIBS[os_]
            root = ctx.cache / "libs" / f"{os_}-{VERSION}"
            if not (root / ".complete").is_file():
                ctx.progress(0.1, f"Downloading AnyPS5's {os_.title()} system libraries {VERSION} (once, about 120 MB)")
                archive = ctx.download(RELEASES + name, sha256=sha)
                runtimes.unpack_archive(ctx, archive, root / "files")
                (root / ".complete").write_text(name + "\n")
            root = root / "files"
        libc = next(iter(sorted(root.rglob("libc.prx"))), None)
        if libc is None:
            raise runtimes.RuntimeError_(f"{root} has no libc.prx")
        return libc.parent

    def prepare(self, ctx):
        os_ = ctx.target["os"]
        d = self._dump
        own = (ctx.manifest.get("game") or {}).get("title")
        self._name = runtimes.safe_name(own or d.title or d.title_id or ctx.title)
        self._exe = {"linux": f"{self._name}.elf", "windows": f"{self._name}.exe", "macos": self._name}[os_]
        relinker = self._relinker(ctx)
        self._libs_dir = self._libs(ctx, os_)
        key = hashlib.sha256()
        key.update(f"{os_}|{self._exe}|{_sha(relinker)}".encode())
        for rel in [d.executable, *(f"{d.module_dir}/{m}" for m in d.modules)]:
            st = (d.root / rel).stat()
            key.update(f"|{rel}|{st.st_size}|{st.st_mtime_ns}".encode())
        out = ctx.cache / "relinked" / key.hexdigest()[:24]
        if not (out / ".complete").is_file():
            shutil.rmtree(out, ignore_errors=True)
            out.mkdir(parents=True)
            ctx.progress(0.3, "Relinking the title with AnyPS5")
            r = ctx.run([relinker, *FLAGS[os_], d.root / d.executable, out / self._exe], cwd=d.root)
            for line in (r.stdout or "").splitlines()[-20:]:
                ctx.log(line)
            if not (out / self._exe).is_file():
                raise runtimes.RuntimeError_("AnyPS5's relinker wrote no executable; see the log")
            (out / ".complete").write_text("done\n")
        self._relinked = out

    def variant(self, ctx):
        return f"anyps5|{VERSION}|{getattr(self, '_relinked', '')}"

    def estimate(self, ctx):
        return {"summary": "Nothing to estimate: AnyPS5 uses the title's files as they are."}

    def layout(self, ctx, build):
        name, d = self._name, self._dump
        build.add_tree(self._relinked, name, links=True, skip=lambda p: p.name == ".complete")
        build.add_tree(self._libs_dir, f"{name}/libs", links=True)
        for p in sorted(d.root.rglob("*")):
            if p.is_file() and not p.name.startswith("."):
                build.add_file(p, f"{name}/app0/{p.relative_to(d.root).as_posix()}")
        build.add_text(f"{name}/LICENSE-anyps5.txt", LICENCE)
        if ctx.flag("zip", False):
            build.artifact(f"{name}.zip")

    def finish(self, ctx, build):
        folder = build.root / self._name
        outputs = [str(folder)]
        if ctx.flag("zip", False):
            outputs.append(str(self.zip(ctx, build, self._name, f"{self._name}.zip")))
        run = {"linux": f"./{self._exe}", "windows": f".\\{self._exe}", "macos": f"./{self._exe}"}[ctx.target["os"]]
        return {"outputs": outputs, "summary": f"Converted {self._dump.title or self._name} with AnyPS5 {VERSION}",
                "notes": [f"Start it from {folder.name}/ with {run}.",
                          "Compatibility differs per title: AnyPS5's Game bring-up discussions list reports."]}
