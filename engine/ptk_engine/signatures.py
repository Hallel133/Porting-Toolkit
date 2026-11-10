"""Engine and format detection from file signatures.

Detection only looks at file and folder names, plus the first four bytes of
executables (to recognise PE, ELF and Mach-O). It never opens, decrypts or
decodes game archives or other protected content.

Each rule adds weight when it matches; an engine is reported once its score
reaches ``THRESHOLD``. Weak hints (a lone ``.pak``, which Quake also uses)
need a second hint before they count.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from fnmatch import translate

THRESHOLD = 0.5

# (weight, kind, pattern). Kinds:
#   name     file name (case-insensitive glob)
#   path     "/" + relative file path (so "*/x" also matches a top-level x)
#   dir      folder name
#   dirpath  "/" + relative folder path
#   magic    a file whose header is "pe", "elf", "mach-o" or "godot-pck"
#   embedded an executable with a Godot .pck appended (ends with GDPC)
Rule = tuple[float, str, str]


@dataclass(frozen=True)
class EngineSignature:
    id: str
    name: str
    kind: str  # "engine" or "format"
    rules: tuple[Rule, ...]
    website: str = ""


ENGINES: tuple[EngineSignature, ...] = (
    EngineSignature(
        "unity",
        "Unity",
        "engine",
        (
            (0.6, "name", "unityplayer.dll"),
            (0.6, "name", "unityplayer.so"),
            (0.6, "name", "unityplayer.dylib"),
            (0.6, "name", "unityframework"),
            (0.6, "name", "globalgamemanagers"),
            (0.5, "name", "data.unity3d"),
            (0.3, "dir", "*_data"),
            (0.3, "name", "sharedassets*.assets"),
            (0.3, "path", "*/managed/assembly-csharp.dll"),
            (0.3, "name", "gameassembly.dll"),
            (0.2, "path", "*/il2cpp_data/*"),
        ),
    ),
    EngineSignature(
        "unreal",
        "Unreal Engine",
        "engine",
        (
            (0.4, "name", "*.pak"),
            (0.5, "name", "*.utoc"),
            (0.5, "name", "*.ucas"),
            (0.5, "path", "*/content/paks/*"),
            (0.2, "dirpath", "/engine"),
            (0.3, "path", "/engine/binaries/*"),
            (0.5, "name", "*-shipping.exe"),
            (0.3, "name", "*.uproject"),
        ),
    ),
    EngineSignature(
        "godot",
        "Godot",
        "engine",
        (
            (0.6, "name", "*.pck"),
            (0.6, "magic", "godot-pck"),
            (0.9, "embedded", "pck"),
            (0.8, "name", "project.godot"),
            (0.3, "dir", ".godot"),
            (0.3, "name", "*.gdc"),
            (0.2, "name", "*.gd"),
        ),
    ),
    EngineSignature(
        "rage",
        "RAGE (Rockstar Advanced Game Engine)",
        "engine",
        (
            (0.7, "name", "*.rpf"),
            (0.3, "path", "*/update/update.rpf"),
            (0.2, "name", "x64*.rpf"),
        ),
    ),
    EngineSignature(
        "source",
        "Source",
        "engine",
        (
            (0.5, "name", "*.vpk"),
            (0.3, "name", "*_dir.vpk"),
            (0.5, "name", "gameinfo.txt"),
            (0.2, "name", "hl2.exe"),
            (0.2, "name", "*.bsp"),
        ),
    ),
    EngineSignature(
        "gamemaker",
        "GameMaker",
        "engine",
        (
            (0.8, "name", "data.win"),
            (0.8, "name", "game.unx"),
            (0.8, "name", "game.ios"),
            (0.8, "name", "game.droid"),
            (0.2, "name", "audiogroup*.dat"),
            (0.1, "name", "options.ini"),
        ),
    ),
    EngineSignature(
        "renpy",
        "Ren'Py",
        "engine",
        (
            (0.5, "dirpath", "/renpy"),
            (0.5, "name", "*.rpa"),
            (0.4, "name", "*.rpyc"),
            (0.3, "name", "*.rpy"),
            (0.2, "path", "*/game/script.rpy*"),
        ),
    ),
    EngineSignature(
        "rpgmaker",
        "RPG Maker",
        "engine",
        (
            (0.8, "path", "*/js/rpg_core.js"),
            (0.8, "path", "*/js/rmmz_core.js"),
            (0.2, "path", "*/data/system.json"),
            (0.8, "name", "*.rgss3a"),
            (0.8, "name", "*.rgss2a"),
            (0.8, "name", "*.rgssad"),
            (0.4, "name", "rgss*.dll"),
            (0.5, "name", "*.rvdata2"),
            (0.5, "name", "*.rvdata"),
            (0.5, "name", "*.rxdata"),
        ),
    ),
    EngineSignature(
        "html5",
        "HTML5 (NW.js / Electron)",
        "engine",
        (
            (0.8, "name", "package.nw"),
            (0.5, "name", "nw.dll"),
            (0.4, "name", "nw_elf.dll"),
            (0.4, "path", "*/resources/app.nw/*"),
            (0.6, "path", "*/resources/app.asar"),
            (0.5, "path", "*/resources/app/package.json"),
            (0.4, "path", "*/contents/resources/app.nw/package.json"),
            (0.2, "name", "package.json"),
            (0.1, "name", "index.html"),
        ),
    ),
    EngineSignature(
        "macos",
        "macOS app / Mach-O",
        "format",
        (
            (0.6, "path", "*.app/contents/info.plist"),
            (0.3, "path", "*.app/contents/macos/*"),
            (0.5, "magic", "mach-o"),
            (0.3, "name", "*.dylib"),
        ),
    ),
)

ENGINE_IDS = [e.id for e in ENGINES]


@dataclass
class GameIndex:
    """Names found in a game folder, for matching."""

    files: list[str]
    dirs: list[str] = field(default_factory=list)
    magic: dict[str, str] = field(default_factory=dict)
    embedded_pck: list[str] = field(default_factory=list)

    def __post_init__(self) -> None:
        self._names = [f.rsplit("/", 1)[-1].lower() for f in self.files]
        self._name_set: dict[str, str] = {}
        for name, path in zip(self._names, self.files):
            self._name_set.setdefault(name, path)
        self._paths = ["/" + f.lower() for f in self.files]
        self._dir_names = [d.rsplit("/", 1)[-1].lower() for d in self.dirs]
        self._dir_paths = ["/" + d.lower() for d in self.dirs]
        self._cache: dict[tuple[str, str], str | None] = {}

    def find(self, kind: str, pattern: str) -> str | None:
        """First path matching a rule, or None."""
        key = (kind, pattern)
        if key not in self._cache:
            self._cache[key] = self._find(kind, pattern)
        return self._cache[key]

    def _find(self, kind: str, pattern: str) -> str | None:
        if kind == "magic":
            return next((p for p, m in self.magic.items() if m == pattern), None)
        if kind == "embedded":
            return next((f"{p} (embedded .pck)" for p in self.embedded_pck), None)
        has_wildcard = any(c in pattern for c in "*?[")
        if kind == "name" and not has_wildcard:
            return self._name_set.get(pattern)
        rx = _compile(pattern)
        if kind == "name":
            haystack, originals = self._names, self.files
        elif kind == "path":
            haystack, originals = self._paths, self.files
        elif kind == "dir":
            haystack, originals = self._dir_names, [d + "/" for d in self.dirs]
        elif kind == "dirpath":
            haystack, originals = self._dir_paths, [d + "/" for d in self.dirs]
        else:
            raise ValueError(f"unknown rule kind {kind!r}")
        for value, original in zip(haystack, originals):
            if rx.match(value):
                return original
        return None


_COMPILED: dict[str, re.Pattern[str]] = {}


def _compile(pattern: str) -> re.Pattern[str]:
    if pattern not in _COMPILED:
        _COMPILED[pattern] = re.compile(translate(pattern))
    return _COMPILED[pattern]


def _variant(engine_id: str, index: GameIndex) -> str | None:
    f = index.find
    if engine_id == "unity":
        if f("name", "gameassembly.dll") or f("path", "*/il2cpp_data/*"):
            return "IL2CPP"
        if f("path", "*/managed/assembly-csharp.dll"):
            return "Mono"
    if engine_id == "godot" and index.embedded_pck:
        return "embedded .pck"
    if engine_id == "html5":
        if f("name", "package.nw") or f("name", "nw.dll") or f("path", "*/resources/app.nw/*"):
            return "NW.js"
        if f("path", "*/resources/app.asar") or f("path", "*/resources/app/package.json"):
            return "Electron"
    if engine_id == "unreal" and (f("name", "*.utoc") or f("name", "*.ucas")):
        return "IoStore (UE 4.26+ / UE5)"
    if engine_id == "rpgmaker":
        for pattern, kind, name in (
            ("*/js/rmmz_core.js", "path", "MZ"),
            ("*/js/rpg_core.js", "path", "MV"),
            ("*.rgss3a", "name", "VX Ace"),
            ("*.rvdata2", "name", "VX Ace"),
            ("*.rgss2a", "name", "VX"),
            ("*.rvdata", "name", "VX"),
            ("*.rgssad", "name", "XP"),
            ("*.rxdata", "name", "XP"),
        ):
            if f(kind, pattern):
                return name
    return None


def confidence_label(score: float) -> str:
    if score >= 0.9:
        return "high"
    if score >= 0.6:
        return "medium"
    return "low"


def detect(index: GameIndex) -> list[dict]:
    """Engines and formats found, best first."""
    results = []
    for sig in ENGINES:
        score = 0.0
        evidence: list[str] = []
        for weight, kind, pattern in sig.rules:
            hit = index.find(kind, pattern)
            if hit is not None:
                score += weight
                if hit not in evidence:
                    evidence.append(hit)
        if score + 1e-9 < THRESHOLD:
            continue
        confidence = round(min(score, 1.0), 2)
        results.append(
            {
                "id": sig.id,
                "name": sig.name,
                "kind": sig.kind,
                "confidence": confidence,
                "label": confidence_label(confidence),
                "variant": _variant(sig.id, index),
                "evidence": evidence[:5],
            }
        )
    results.sort(key=lambda r: (r["kind"] != "engine", -r["confidence"], r["id"]))
    return results


def guess_platforms(index: GameIndex) -> list[str]:
    """Platforms the game files were built for."""
    found = []
    f = index.find
    if f("name", "*.exe") or f("name", "*.dll") or f("magic", "pe"):
        found.append("windows")
    if f("dir", "*.app") or f("name", "*.dylib") or f("magic", "mach-o"):
        found.append("macos")
    if f("magic", "elf") or f("name", "*.x86_64") or (f("name", "*.so") and "macos" not in found):
        found.append("linux")
    if f("name", "*.wasm") and f("name", "index.html"):
        found.append("web")
    return found
