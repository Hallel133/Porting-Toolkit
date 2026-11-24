"""RPG Maker XP, VX and VX Ace games (RGSS 1, 2 and 3).

A game folder holds ``Game.exe``, ``Game.ini``, the RGSS library
(``RGSS104E.dll``, ``System/RGSS301.dll``...), ``Data/`` with the Ruby
scripts and maps (``.rxdata``/``.rvdata``/``.rvdata2``), ``Graphics/`` and
``Audio/``, or an encrypted archive (``Game.rgssad``/``.rgss2a``/``.rgss3a``)
instead of the folders.

Only ``Game.ini`` and the first 8 bytes of an archive are read: archives are
never decrypted.
"""

from __future__ import annotations

import configparser
import re
from dataclasses import asdict, dataclass, field
from pathlib import Path

VARIANTS = {1: "XP", 2: "VX", 3: "VX Ace"}
ARCHIVES = {".rgssad": 1, ".rgss2a": 2, ".rgss3a": 3}
DATA_EXT = {".rxdata": 1, ".rvdata": 2, ".rvdata2": 3}


class RgssError(Exception):
    pass


@dataclass
class RgssGame:
    root: str  # folder with Game.ini, relative to the game folder ("" = itself)
    rgss: int  # 1 = XP, 2 = VX, 3 = VX Ace
    title: str | None = None
    ini: str = "Game.ini"
    library: str | None = None
    scripts: str | None = None
    archive: dict | None = None  # {"path", "format"}
    rtp: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    @property
    def variant(self) -> str:
        return VARIANTS[self.rgss]

    def to_dict(self) -> dict:
        return {**asdict(self), "variant": self.variant}


def read_ini(path: Path) -> dict[str, str]:
    """The [Game] section; Japanese games often use Shift-JIS."""
    raw = path.read_bytes()
    for enc in ("utf-8-sig", "cp932", "latin-1"):
        try:
            text = raw.decode(enc)
            break
        except UnicodeDecodeError:
            continue
    parser = configparser.ConfigParser(interpolation=None, strict=False)
    parser.optionxform = str
    try:
        parser.read_string(text)
    except configparser.Error as e:
        raise RgssError(f"{path.name}: {e}") from e
    section = next((s for s in parser.sections() if s.lower() == "game"), None)
    return dict(parser[section]) if section else {}


def archive_format(path: Path) -> int | None:
    """1 (XP/VX) or 3 (VX Ace) from an archive's header, None if not one."""
    try:
        with open(path, "rb") as f:
            head = f.read(8)
    except OSError:
        return None
    if head[:7] == b"RGSSAD\0" and head[7] in (1, 3):
        return head[7]
    return None


def _version(library: str | None, root: Path) -> int | None:
    if library:
        m = re.search(r"rgss(\d)", library, re.I)
        if m and int(m.group(1)) in VARIANTS:
            return int(m.group(1))
    for ext, v in ARCHIVES.items():
        if any(root.glob(f"*{ext}")):
            return v
    for ext, v in DATA_EXT.items():
        if any((root / "Data").glob(f"*{ext}")) if (root / "Data").is_dir() else False:
            return v
    return None


def detect(game: Path) -> RgssGame:
    game = Path(game)
    if not game.is_dir():
        raise RgssError("choose the game's folder (the one with Game.exe and Game.ini)")
    inis = sorted((p for p in [*game.glob("*.ini"), *game.glob("*/*.ini")]
                   if p.is_file() and not p.name.startswith("._")), key=lambda p: (len(p.parts), p.name))
    for ini in inis:
        try:
            values = read_ini(ini)
        except (RgssError, OSError):
            continue
        if not values:
            continue
        root = ini.parent
        version = _version(values.get("Library"), root)
        if version is None:
            continue
        g = RgssGame(
            root="" if root == game else root.relative_to(game).as_posix(),
            rgss=version,
            title=(values.get("Title") or "").strip() or None,
            ini=ini.name,
            library=values.get("Library"),
            scripts=values.get("Scripts"),
            rtp=[v.strip() for k, v in sorted(values.items()) if re.fullmatch(r"RTP\d?", k) and v.strip()],
        )
        for p in sorted(root.iterdir()):
            fmt = ARCHIVES.get(p.suffix.lower())
            if fmt and p.is_file():
                g.archive = {"path": p.name, "format": archive_format(p)}
                if g.archive["format"] is None:
                    g.notes.append(f"{p.name} does not start like an RGSS archive")
                break
        if g.rtp:
            g.notes.append("The game uses the RPG Maker RTP (" + ", ".join(g.rtp) + "); other platforms need a copy "
                           "of it: pass --set rtp=PATH when exporting.")
        return g
    raise RgssError("no RPG Maker XP/VX/VX Ace game found: no Game.ini with an RGSS library")


def is_save(name: str) -> bool:
    """The player's save files (Save01.rxdata ...), which never travel."""
    return re.fullmatch(r"save\d*\.(rxdata|rvdata2?)", name.lower()) is not None
