"""What each engine can load, so re-encoding never breaks a game.

Games built from loose files (HTML5, RPG Maker, Ren'Py, LÖVE, RGSS) load
assets by file name, so every file keeps its name and extension and is
re-encoded in place. Images are never resized: sprite sheets, tile sets
and LÖVE quads depend on their pixel sizes.

"Automatic" means:

- ``same``: keep each file's codec and only make it smaller (the safe
  choice when the engine's decoders are unknown or fussy);
- ``target``: the first codec the target prefers that the engine and the
  file's extension allow.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from .encoders import CONTAINERS


@dataclass(frozen=True)
class Profile:
    id: str
    name: str
    automatic: str = "same"  # "same" | "target"
    #: Narrower codec lists per extension than CONTAINERS allows.
    codecs: dict[str, tuple[str, ...]] = field(default_factory=dict)
    #: Why a codec is limited, shown when a choice falls back.
    reasons: dict[str, str] = field(default_factory=dict)
    #: Categories this engine's files are processed for at all.
    categories: tuple[str, ...] = ("textures", "audio", "video")
    #: When nothing can be processed, the reason (Godot keeps its .pck).
    untouched: str | None = None

    def allowed(self, ext: str) -> tuple[str, ...]:
        base = CONTAINERS.get(ext, ())
        narrow = self.codecs.get(ext)
        return tuple(c for c in base if narrow is None or c in narrow)


PROFILES: dict[str, Profile] = {p.id: p for p in [
    Profile("html5", "HTML5 game"),
    Profile("rpgmaker-mv", "RPG Maker MV",
            reasons={"*": "RPG Maker MV picks .ogg or .m4a at run time, so both are kept"}),
    Profile("rpgmaker-mz", "RPG Maker MZ",
            codecs={".ogg": ("vorbis",)},
            reasons={".ogg": "RPG Maker MZ decodes .ogg with its own Vorbis decoder"}),
    Profile("renpy", "Ren'Py", automatic="target",
            codecs={".ogg": ("vorbis", "opus")}),
    Profile("love", "LÖVE",
            codecs={".ogg": ("vorbis",)},
            reasons={".ogg": "LÖVE 11 decodes Ogg Vorbis only"}),
    Profile("rgss", "RPG Maker XP/VX/VX Ace",
            codecs={".ogg": ("vorbis",)},
            reasons={".ogg": "the RGSS runtime plays Ogg Vorbis"}),
    Profile("godot", "Godot", categories=(),
            untouched="Godot games keep their imported resources: re-encoding them inside the .pck "
                      "is not supported yet."),
]}


@dataclass(frozen=True)
class Decision:
    codec: str | None  # None: leave the file alone
    note: str | None = None


def decide(profile: Profile, category: str, ext: str, requested: str | None,
           preferred: list[str], source_codec: str | None) -> Decision:
    """The codec to re-encode one file with.

    ``requested`` is the user's format (None for Automatic), ``preferred``
    the target's list for the category, ``source_codec`` what the file
    holds now (from ffprobe, or None for images and unknown files).
    """
    if category not in profile.categories:
        return Decision(None, profile.untouched)
    allowed = profile.allowed(ext)
    if not allowed:
        return Decision(None)
    same = source_codec if source_codec in allowed else allowed[0]
    if requested:
        if requested in allowed:
            return Decision(requested)
        why = profile.reasons.get(ext) or (
            f"{ext} files keep their format because {profile.name} loads them by name")
        return Decision(same, f"{requested} can't be used here ({why}); kept {same}")
    if profile.automatic == "target":
        for c in preferred:
            if c in allowed:
                return Decision(c)
    return Decision(same)
