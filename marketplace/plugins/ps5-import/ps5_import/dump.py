"""Reads a PS5 game folder (an app0 dump): names and headers only.

A PS5 title's folder holds ``eboot.bin`` (its executable), ``sce_sys/``
(``param.json``: title ID, names, version) and ``sce_module/`` (bundled
libraries). On the console these executables are SELF containers, which
are encrypted; AnyPS5 needs them decrypted (plain ELF). Porting Toolkit
does not decrypt anything: it only reads the first bytes to say which
kind each file is.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from pathlib import Path

ELF = b"\x7fELF"
SELF = b"\x4f\x15\x3d\x1d"  # PS4/PS5 SELF container
MODULE_DIRS = ("sce_module", "sce_modules", "prx")
MODULE_SUFFIXES = (".prx", ".sprx", ".suprx", ".elf")


def kind_of(path: Path) -> str:
    try:
        with open(path, "rb") as f:
            head = f.read(4)
    except OSError:
        return "unreadable"
    if head == ELF:
        return "elf"
    if head == SELF:
        return "self"
    return "unknown"


@dataclass
class Dump:
    root: Path
    executable: str
    executable_kind: str
    title_id: str | None = None
    title: str | None = None
    content_version: str | None = None
    module_dir: str | None = None
    modules: dict[str, str] = field(default_factory=dict)  # name -> elf/self/unknown
    notes: list[str] = field(default_factory=list)

    @property
    def encrypted(self) -> list[str]:
        out = [self.executable] if self.executable_kind == "self" else []
        return out + [f"{self.module_dir}/{n}" for n, k in self.modules.items() if k == "self"]

    def to_dict(self) -> dict:
        d = asdict(self)
        d["root"] = str(self.root)
        d["encrypted"] = self.encrypted
        return d


class DumpError(Exception):
    pass


def find_root(game: Path) -> Path:
    """The folder holding eboot.bin: the game folder, its app0/, or one level down."""
    for cand in (game, game / "app0", *sorted(p for p in game.iterdir() if p.is_dir())):
        if (cand / "eboot.bin").is_file():
            return cand
    raise DumpError("no eboot.bin here: choose the folder of a PS5 title (the app0 folder of your own dump)")


def read_param(path: Path) -> tuple[str | None, str | None, str | None]:
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, ValueError):
        return None, None, None
    names = data.get("localizedParameters") or {}
    lang = names.get("defaultLanguage") or data.get("defaultLanguage") or "en-US"
    entry = names.get(lang) or next((v for v in names.values() if isinstance(v, dict)), {}) or {}
    return data.get("titleId"), entry.get("titleName"), data.get("contentVersion")


def read_dump(game: Path) -> Dump:
    root = find_root(Path(game))
    d = Dump(root=root, executable="eboot.bin", executable_kind=kind_of(root / "eboot.bin"))
    param = root / "sce_sys" / "param.json"
    if param.is_file():
        d.title_id, d.title, d.content_version = read_param(param)
        if not d.title_id:
            d.notes.append("sce_sys/param.json has no titleId; AnyPS5 needs it at run time")
    elif (root / "sce_sys" / "param.sfo").is_file():
        raise DumpError("this is a PS4 title (sce_sys/param.sfo): AnyPS5 converts PS5 titles only")
    else:
        d.notes.append("no sce_sys/param.json: some titles need it at run time")
    dirs = [n for n in MODULE_DIRS if (root / n).is_dir()]
    if "sce_module" in dirs and "sce_modules" in dirs:
        raise DumpError("both sce_module/ and sce_modules/ exist; AnyPS5 accepts only one of them")
    if dirs:
        d.module_dir = dirs[0]
        for p in sorted((root / d.module_dir).iterdir()):
            if p.is_file() and p.suffix.lower() in MODULE_SUFFIXES:
                d.modules[p.name] = kind_of(p)
    return d
