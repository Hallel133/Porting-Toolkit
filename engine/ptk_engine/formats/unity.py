"""Unity games: layout, version, scripting backend, native plugins.

A Unity build keeps the game in a *Data* folder:

- Windows / Linux: ``<Name>_Data/`` next to ``<Name>.exe`` / ``<Name>.x86_64``
  and ``UnityPlayer.dll`` / ``UnityPlayer.so``;
- macOS: ``<Name>.app/Contents/Resources/Data`` with ``UnityPlayer.dylib``
  in ``Contents/Frameworks``.

With the **Mono** scripting backend the game's code is .NET assemblies in
``Data/Managed`` (``Assembly-CSharp.dll``), the same on every platform, so
the Data folder can run with the Unity player of the same version for
another platform. **IL2CPP** builds compile the code to native code
(``GameAssembly.dll`` / ``.dylib`` / ``.so``) and cannot be moved.

Only file names and the first bytes of ``globalgamemanagers`` (or
``data.unity3d``) are read, for the Unity version.
"""

from __future__ import annotations

import re
from dataclasses import asdict, dataclass, field
from pathlib import Path

VERSION_RE = re.compile(rb"((?:20\d\d|6\d{3}|[45])\.\d+\.\d+[abfpx]\d+)")
#: Native plugins that only exist on Apple platforms (App Store, Apple Arcade).
APPLE_ONLY = ("gamecenter", "cloudkit", "icloud", "fairplay", "storekit", "applearcade", "polyspatial")


class UnityError(Exception):
    pass


@dataclass
class UnityGame:
    platform: str  # "windows" | "linux" | "macos"
    data: str  # the Data folder, relative to the game folder
    name: str  # the executable's name (without extension)
    app: str | None = None  # the .app, for macOS builds
    version: str | None = None
    backend: str = "mono"  # "mono" | "il2cpp"
    company: str | None = None
    product: str | None = None
    plugins: list[str] = field(default_factory=list)  # native plugins of the source platform
    apple_services: list[str] = field(default_factory=list)
    app_store: bool = False  # a Mac App Store / Apple Arcade build
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)


def read_version(data_dir: Path) -> str | None:
    for name in ("globalgamemanagers", "data.unity3d", "mainData"):
        p = data_dir / name
        if p.is_file():
            with open(p, "rb") as f:
                m = VERSION_RE.search(f.read(4096))
            if m:
                return m.group(1).decode()
    return None


def _app_info(data_dir: Path) -> tuple[str | None, str | None]:
    p = data_dir / "app.info"
    if not p.is_file():
        return None, None
    lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
    company = lines[0].strip() if lines else None
    product = lines[1].strip() if len(lines) > 1 else None
    return company or None, product or None


def _is_data(d: Path) -> bool:
    return d.is_dir() and any((d / n).exists() for n in ("globalgamemanagers", "data.unity3d", "mainData", "Managed"))


def detect(game: Path) -> UnityGame:
    game = Path(game)
    if game.is_file():
        game = game.parent
    # macOS: an .app (the game itself, or inside the folder)
    apps = [game] if game.suffix == ".app" else sorted(game.glob("*.app"))
    for app in apps:
        data = app / "Contents" / "Resources" / "Data"
        if _is_data(data):
            exe = sorted(p.name for p in (app / "Contents" / "MacOS").iterdir() if p.is_file()) \
                if (app / "Contents" / "MacOS").is_dir() else []
            g = UnityGame("macos", data.relative_to(game).as_posix() if app != game else "Contents/Resources/Data",
                          exe[0] if exe else app.stem, app=app.relative_to(game).as_posix() if app != game else "")
            g.backend = "il2cpp" if (app / "Contents" / "Frameworks" / "GameAssembly.dylib").exists() else "mono"
            plugin_dir = app / "Contents" / "PlugIns"
            g.plugins = sorted(p.stem for p in plugin_dir.iterdir() if p.suffix in (".bundle", ".dylib")) \
                if plugin_dir.is_dir() else []
            contents = app / "Contents"
            g.app_store = (contents / "_MASReceipt").exists() or any(
                "alto" in p.name.lower() for p in (contents / "Resources").glob("*.entitlements"))
            return _finish(g, data)
    # Windows / Linux: <Name>_Data next to the executable
    for data in sorted(game.glob("*_Data")):
        if not _is_data(data):
            continue
        name = data.name[: -len("_Data")]
        if (game / f"{name}.exe").exists() or (game / "UnityPlayer.dll").exists():
            platform = "windows"
            g = UnityGame(platform, data.relative_to(game).as_posix(), name)
            g.backend = "il2cpp" if (game / "GameAssembly.dll").exists() else "mono"
            plugin_dir = data / "Plugins"
            g.plugins = sorted({p.stem for p in plugin_dir.rglob("*.dll")}) if plugin_dir.is_dir() else []
        else:
            g = UnityGame("linux", data.relative_to(game).as_posix(), name)
            g.backend = "il2cpp" if (game / "GameAssembly.so").exists() else "mono"
            plugin_dir = data / "Plugins"
            g.plugins = sorted({p.name.removesuffix(".so") for p in plugin_dir.rglob("*.so")}) \
                if plugin_dir.is_dir() else []
        return _finish(g, data)
    raise UnityError("no Unity game found: no <Name>_Data folder or .app with Contents/Resources/Data")


def _finish(g: UnityGame, data: Path) -> UnityGame:
    g.version = read_version(data)
    g.company, g.product = _app_info(data)
    if g.backend == "mono" and not (data / "Managed" / "Assembly-CSharp.dll").exists() \
            and (data / "il2cpp_data").exists():
        g.backend = "il2cpp"
    g.apple_services = [p for p in g.plugins if any(k in p.lower() for k in APPLE_ONLY)]
    g.app_store = g.app_store or any(k in p.lower() for p in g.plugins for k in ("applearcade", "fairplay"))
    if any(p.startswith("lib_burst_generated") for p in g.plugins):
        g.plugins = [p for p in g.plugins if not p.startswith("lib_burst_generated")]
        g.notes.append("Uses Burst (lib_burst_generated, native code): on another platform Unity runs the same "
                       "code without Burst, which works but can be slower.")
    if g.backend == "il2cpp":
        g.notes.append("IL2CPP build: the game's code is compiled to native code for this platform only, so it "
                       "cannot be moved to another platform without the source.")
    if g.app_store:
        g.notes.append("A Mac App Store / Apple Arcade build: its terms limit it to Apple devices, and Apple "
                       "services (iCloud saves, Game Center, the subscription check) do not exist elsewhere.")
    if g.version is None:
        g.notes.append("The Unity version was not found in globalgamemanagers.")
    return g


# -- the Unity installation that provides the target players ------------------------------

HUB_ROOTS = ("/Applications/Unity/Hub/Editor", "~/Applications/Unity/Hub/Editor",
             "C:/Program Files/Unity/Hub/Editor", "~/Unity/Hub/Editor")


def editor_roots(version: str, extra: list[str] | None = None) -> list[Path]:
    """Folders that may hold Unity <version> (Hub's default and custom places)."""
    out = []
    for r in [*(extra or []), *HUB_ROOTS]:
        p = Path(r).expanduser()
        out.append(p / version if p.name != version else p)
    secondary = Path("~/Library/Application Support/UnityHub/secondaryInstallPath.json").expanduser()
    if secondary.is_file():
        text = secondary.read_text(encoding="utf-8", errors="replace").strip().strip('"')
        if text:
            out.append(Path(text) / version)
    return out


def support_dir(root: Path, target: str) -> Path | None:
    """PlaybackEngines/<Platform>StandaloneSupport for a target os."""
    names = {"windows": "WindowsStandaloneSupport", "linux": "LinuxStandaloneSupport",
             "macos": "MacStandaloneSupport"}
    for base in (root / "PlaybackEngines", root / "Unity.app" / "Contents" / "PlaybackEngines"):
        d = base / names[target]
        if (d / "Variations").is_dir():
            return d
    return None


def player_variation(support: Path, target: str) -> Path | None:
    """The release (non-development) Mono player for 64-bit x86 or, on
    macOS, the universal one."""
    prefer = {"windows": ("win64",), "linux": ("linux64",),
              "macos": ("macos_x64arm64", "macosx64arm64", "macos_arm64", "macos_x64", "macosx64")}[target]
    variations = [d for d in (support / "Variations").iterdir() if d.is_dir()]
    for p in prefer:
        for d in sorted(variations):
            n = d.name.lower()
            if n.startswith(p) and "nondevelopment" in n and "mono" in n:
                return d
    return None
