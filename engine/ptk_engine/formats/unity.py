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
    #: Graphics APIs of the build's shaders (``ShaderReport.to_dict()``), or None.
    shaders: dict | None = None
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
    try:
        report = shader_report(data)
    except OSError:
        report = None
    if report and report.codes:
        g.shaders = report.to_dict()
        missing = [NAMES[t] for t in ("windows", "linux", "macos") if not report.covers(t)]
        if missing:
            g.notes.append(f"Shaders: {', '.join(report.apis)} only ({report.shaders} shaders). "
                           f"{' and '.join(missing)} cannot draw them, so objects would show pink or "
                           "invisible there; only the game's developer can add those graphics APIs.")
    return g


NAMES = {"windows": "Windows", "linux": "Linux", "macos": "macOS"}


def can_draw(info: dict, target_os: str) -> bool | None:
    """Whether a target can draw a detected game's shaders (None: not known)."""
    codes = (info.get("shaders") or {}).get("codes") or []
    if not codes:
        return None
    return bool(set(codes) & TARGET_APIS.get(target_os, set()))


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


# -- which graphics APIs the build's shaders cover -------------------------------------------
#
# A Unity build keeps compiled shader code only for the graphics APIs it was
# built with. Each Shader object (class 48) in the game's serialized files
# ends with ``platforms`` (ShaderCompilerPlatform values), the per-platform
# offsets and lengths, and an LZ4 ``compressedBlob``. Player builds usually
# have no type trees, so the object is not parsed field by field: its tail is
# matched instead (the counts must agree and the blob must fit the object).

SHADER_CLASS = 48
#: ShaderCompilerPlatform -> name (Unity 2019.3+).
SHADER_PLATFORMS = {4: "Direct3D 11/12", 5: "OpenGL ES 2", 8: "Direct3D 11 9.x", 9: "OpenGL ES 3",
                    11: "PS4", 12: "Xbox One", 14: "Metal", 15: "OpenGL Core", 18: "Vulkan",
                    19: "Switch", 20: "Xbox One D3D12", 21: "Game Core Xbox One",
                    22: "Game Core Series X", 23: "PS5", 24: "PS5 NGGC", 25: "WebGPU"}
#: What each desktop target can draw with (Unity picks the first it supports).
TARGET_APIS = {"windows": {4, 18, 15}, "linux": {18, 15}, "macos": {14},
               "web": {9, 25}, "android": {9, 18}}


class _Reader:
    def __init__(self, data: bytes, little: bool):
        self.d, self.p, self.e = data, 0, "<" if little else ">"

    def u8(self) -> int:
        self.p += 1
        return self.d[self.p - 1]

    def _n(self, fmt: str, size: int) -> int:
        import struct
        v = struct.unpack_from(self.e + fmt, self.d, self.p)[0]
        self.p += size
        return v

    def i16(self) -> int:
        return self._n("h", 2)

    def i32(self) -> int:
        return self._n("i", 4)

    def u32(self) -> int:
        return self._n("I", 4)

    def i64(self) -> int:
        return self._n("q", 8)

    def cstr(self) -> str:
        end = self.d.index(b"\0", self.p)
        s = self.d[self.p:end].decode("utf-8", "replace")
        self.p = end + 1
        return s

    def align(self) -> None:
        self.p = (self.p + 3) & ~3


def serialized_objects(path: Path) -> list[tuple[int, int, int]]:
    """``(class id, absolute offset, size)`` of every object in a Unity
    serialized file (format 9..22+), read from its header only."""
    import struct
    with open(path, "rb") as f:
        head = f.read(48)
        if len(head) < 48:
            return []
        _meta, _size, version, data_off = struct.unpack(">IIII", head[:16])
        if not 9 <= version <= 40:
            return []
        little = head[16] == 0
        if version >= 22:
            meta_size, _fsize, data_off = struct.unpack(">IqQ", head[20:40])
            start = 48
        else:
            meta_size, start = _meta, 20
        f.seek(start)
        r = _Reader(f.read(meta_size + 64), little)
    try:
        r.cstr()  # unity version
        r.i32()  # target platform
        type_tree = bool(r.u8()) if version >= 13 else True
        classes = []
        for _ in range(r.i32()):
            cid = r.i32()
            if version >= 16:
                r.u8()
            if version >= 17:
                r.i16()
            if version >= 13:
                if (version < 16 and cid < 0) or (version >= 16 and cid == 114):
                    r.p += 16
                r.p += 16
            if type_tree:
                nodes, strings = r.i32(), r.i32()
                r.p += nodes * (32 if version >= 19 else 24) + strings
                if version >= 21:
                    r.p += 4 * r.i32()
            classes.append(cid)
        if 7 <= version < 14:
            r.i32()
        objects = []
        for _ in range(r.i32()):
            if version >= 14:
                r.align()
                r.i64()
            else:
                r.i32()
            off = r.i64() if version >= 22 else r.u32()
            size = r.u32()
            tid = r.i32()
            cid = classes[tid] if version >= 16 and 0 <= tid < len(classes) else tid
            if version < 16:  # the type id is the class id; a u16 copy follows
                r.p += 2
            if version < 11:
                r.p += 2
            if 11 <= version < 17:
                r.i16()
            if version in (15, 16):
                r.u8()
            objects.append((cid, data_off + off, size))
        return objects
    except (IndexError, ValueError, struct.error):
        return []


def _shader_tail(obj: bytes) -> list[int] | None:
    """The ``platforms`` of a Shader object, matched from its end."""
    import struct
    n_bytes = len(obj)

    def u32(i: int) -> int | None:
        return struct.unpack_from("<I", obj, i)[0] if i + 4 <= n_bytes else None

    def skip_lists(i: int, count: int, nested: bool) -> int | None:
        if u32(i) != count:
            return None
        i += 4
        if not nested:
            return i + 4 * count
        for _ in range(count):
            m = u32(i)
            if m is None or m > 4096:
                return None
            i += 4 + 4 * m
        return i

    best = None
    for i in range(0, n_bytes - 8, 4):
        n = u32(i)
        if not n or n > 12:
            continue
        plats = [u32(i + 4 + 4 * k) for k in range(n)]
        if any(p not in SHADER_PLATFORMS for p in plats) or len(set(plats)) != n:
            continue
        for nested in (True, False):
            j = i + 4 + 4 * n
            for _ in range(3):  # offsets, compressedLengths, decompressedLengths
                j = skip_lists(j, n, nested) if j is not None else None
            if j is None:
                continue
            blob = u32(j)
            if blob is not None and 0 < blob and j + 4 + blob <= n_bytes:
                end = j + 4 + blob
                if best is None or end > best[0]:
                    best = (end, plats)
    return best[1] if best else None


@dataclass
class ShaderReport:
    apis: list[str]  # graphics APIs found, by name
    codes: list[int]  # their ShaderCompilerPlatform values
    shaders: int  # Shader objects read
    unknown: int  # Shader objects whose platforms were not found

    def covers(self, target_os: str) -> bool | None:
        """Whether a target can draw these shaders (None: not known)."""
        if not self.codes:
            return None
        return bool(set(self.codes) & TARGET_APIS.get(target_os, set()))

    def to_dict(self) -> dict:
        return asdict(self)


def shader_report(data_dir: Path) -> ShaderReport | None:
    """The graphics APIs of the Shader objects in a Data folder (None when
    it has no loose serialized files, e.g. a compressed data.unity3d)."""
    data_dir = Path(data_dir)
    files = [p for p in sorted(data_dir.iterdir()) if p.is_file() and p.suffix not in (".resS", ".resource", ".json")
             and not p.name.startswith(".") and p.name not in ("app.info", "boot.config")] \
        if data_dir.is_dir() else []
    files += sorted((data_dir / "Resources").glob("unity_builtin_extra"))
    codes: set[int] = set()
    shaders = unknown = 0
    seen_any = False
    for path in files:
        objs = serialized_objects(path)
        if not objs:
            continue
        seen_any = True
        with open(path, "rb") as f:
            for cid, off, size in objs:
                if cid != SHADER_CLASS:
                    continue
                shaders += 1
                f.seek(off)
                plats = _shader_tail(f.read(size))
                if plats is None:
                    unknown += 1
                else:
                    codes.update(plats)
    if not seen_any:
        return None
    ordered = sorted(codes)
    return ShaderReport([SHADER_PLATFORMS[c] for c in ordered], ordered, shaders, unknown)
