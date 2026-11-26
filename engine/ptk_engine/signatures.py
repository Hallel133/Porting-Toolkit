"""Engine and format detection from file signatures.

Detection only looks at file and folder names, plus the first four bytes of
executables (to recognise PE, ELF and Mach-O) and, for Twine stories, the
``<tw-storydata`` tag in top-level HTML pages. It never opens, decrypts or
decodes game archives or other protected content.

Each rule adds weight when it matches; an engine is reported once its score
reaches ``THRESHOLD``. Weak hints (a lone ``.pak``, which Quake also uses)
need a second hint before they count.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from fnmatch import translate
from pathlib import Path

THRESHOLD = 0.5

# Rules are (weight, kind, pattern) or, for "head", (weight, "head", name glob,
# header prefix). Kinds:
#   name     file name (case-insensitive glob)
#   path     "/" + relative file path (so "*/x" also matches a top-level x)
#   dir      folder name
#   dirpath  "/" + relative folder path
#   magic    a file whose header is "pe", "elf", "mach-o" or "godot-pck"
#   embedded an executable with a Godot .pck appended (ends with GDPC)
#   mark     a top-level HTML page carrying a marker ("twine": <tw-storydata)
#   head     a file matching the glob whose first bytes are the prefix
#            (at most HEAD_SIZE bytes are read; nothing is decoded)
# A negative weight counts against an engine (Morrowind's data is not a
# Gamebryo game for Porting Toolkit's purposes, but OpenMW's).
Rule = tuple

HEAD_SIZE = 16
CATALOG_FILE = Path(__file__).with_name("data") / "engines.json"


@dataclass(frozen=True)
class EngineSignature:
    id: str
    name: str
    kind: str  # "engine" or "format"
    rules: tuple[Rule, ...]
    website: str = ""


def load_catalog() -> list[dict]:
    """Every engine Porting Toolkit knows, with its porting status
    (``data/engines.json``; also read by the Rust side)."""
    return json.loads(CATALOG_FILE.read_text(encoding="utf-8"))["engines"]


CATALOG: list[dict] = load_catalog()
ENGINES: tuple[EngineSignature, ...] = tuple(
    EngineSignature(e["id"], e["name"], e.get("kind", "engine"), tuple(tuple(r) for r in e["rules"]))
    for e in CATALOG if e["rules"]
)
#: Name globs whose first bytes a "head" rule looks at (lower case).
HEAD_GLOBS: tuple[str, ...] = tuple(sorted({r[2] for s in ENGINES for r in s.rules if r[1] == "head"}))


def wants_head(name: str) -> bool:
    """Whether a scan should read the first bytes of a file with this name."""
    low = name.lower()
    return any(_compile(g).match(low) for g in HEAD_GLOBS)


ENGINE_IDS = [e.id for e in ENGINES]


@dataclass
class GameIndex:
    """Names found in a game folder, for matching."""

    files: list[str]
    dirs: list[str] = field(default_factory=list)
    magic: dict[str, str] = field(default_factory=dict)
    embedded_pck: list[str] = field(default_factory=list)
    marks: dict[str, str] = field(default_factory=dict)
    #: First bytes of the files a "head" rule names.
    heads: dict[str, bytes] = field(default_factory=dict)

    def __post_init__(self) -> None:
        self._names = [f.rsplit("/", 1)[-1].lower() for f in self.files]
        self._name_set: dict[str, str] = {}
        for name, path in zip(self._names, self.files):
            self._name_set.setdefault(name, path)
        self._paths = ["/" + f.lower() for f in self.files]
        self._dir_names = [d.rsplit("/", 1)[-1].lower() for d in self.dirs]
        self._dir_paths = ["/" + d.lower() for d in self.dirs]
        self._cache: dict[tuple[str, str], str | None] = {}

    def find(self, kind: str, pattern: str, prefix: str | None = None) -> str | None:
        """First path matching a rule, or None."""
        key = (kind, pattern, prefix)
        if key not in self._cache:
            self._cache[key] = self._find(kind, pattern, prefix)
        return self._cache[key]

    def _find(self, kind: str, pattern: str, prefix: str | None = None) -> str | None:
        if kind == "head":
            want = (prefix or "").encode("latin-1")
            rx = _compile(pattern)
            return next((p for p, h in sorted(self.heads.items())
                         if h.startswith(want) and rx.match(p.rsplit("/", 1)[-1].lower())), None)
        if kind == "magic":
            return next((p for p, m in self.magic.items() if m == pattern), None)
        if kind == "embedded":
            return next((f"{p} (embedded .pck)" for p in self.embedded_pck), None)
        if kind == "mark":
            return next((p for p, m in self.marks.items() if m == pattern), None)
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
        if (f("name", "gameassembly.dll") or f("name", "gameassembly.dylib") or f("name", "gameassembly.so")
                or f("path", "*/il2cpp_data/*")):
            return "IL2CPP"
        if f("path", "*/managed/assembly-csharp.dll"):
            return "Mono"
    if engine_id == "godot" and index.embedded_pck:
        return "embedded .pck"
    if engine_id == "html5":
        framework = None
        for kind, pattern, name in (
            ("name", "c3runtime.js", "Construct 3"),
            ("name", "c3main.js", "Construct 3"),
            ("name", "c2runtime.js", "Construct 2"),
            ("name", "gd.js", "GDevelop"),
            ("name", "phaser*.js", "Phaser"),
            ("name", "cocos2d-js*.js", "Cocos"),
            ("name", "dmloader.js", "Defold"),
            ("mark", "twine", "Twine"),
            ("name", "three*.min.js", "Three.js"),
            ("name", "three.module*.js", "Three.js"),
            ("name", "babylon*.js", "Babylon.js"),
            ("name", "playcanvas*.js", "PlayCanvas"),
            ("name", "aframe*.js", "A-Frame"),
            ("name", "laya.core*.js", "LayaAir"),
            ("name", "b4w*.js", "Blend4Web"),
            ("name", "turbulenz*.js", "Turbulenz"),
            ("name", "*.tzjs", "Turbulenz"),
            ("path", "*/js/gse/gse-export*.js", "GameSalad"),
        ):
            if f(kind, pattern):
                framework = name
                break
        runtime = None
        if f("name", "package.nw") or f("name", "nw.dll") or f("path", "*/resources/app.nw/*"):
            runtime = "NW.js"
        elif f("path", "*/resources/app.asar") or f("path", "*/resources/app/package.json"):
            runtime = "Electron"
        if framework and runtime:
            return f"{framework} ({runtime})"
        return framework or runtime
    if engine_id == "unreal" and (f("name", "*.utoc") or f("name", "*.ucas")):
        return "IoStore (UE 4.26+ / UE5)"
    if engine_id == "rpgmaker":
        if f("path", "*/js/rmmz_core.js"):
            return "MZ"
        if f("path", "*/js/rpg_core.js"):
            return "MV"
    if engine_id == "rgss":
        if f("name", "rgss3*.dll"):
            return "VX Ace"
        if f("name", "rgss2*.dll"):
            return "VX"
        if f("name", "rgss1*.dll"):
            return "XP"
        for pattern, kind, name in (
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
        for weight, kind, pattern, *rest in sig.rules:
            hit = index.find(kind, pattern, rest[0] if rest else None)
            if hit is not None:
                score += weight
                if weight > 0 and hit not in evidence:
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
