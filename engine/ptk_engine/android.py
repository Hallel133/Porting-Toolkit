"""Building Android APKs: the SDK tools, a debug key, aligning and signing.

Export plugins assemble an APK's entries (from a template APK plus the
game), then call :func:`write_aligned` and :func:`sign`. The Android SDK
(build-tools: ``apksigner``, ``zipalign``, ``aapt2``, ``d8``; a platform's
``android.jar``) and a JDK (``keytool``, ``javac``) must be installed; see
:func:`status` and ``ptk doctor``. Signing is never reimplemented here.
"""

from __future__ import annotations

import os
import re
import shutil
import struct
import subprocess
import zipfile
from dataclasses import dataclass
from pathlib import Path

#: Stored entries are aligned to 4 bytes; native libraries to 16 KB pages
#: (what Android 15's 16 KB page devices need when they are not extracted).
ALIGN = 4
SO_ALIGN = 16384
ALIGNMENT_EXTRA_ID = 0xD935  # the "Android zipalign" extra field

SDK_GUESSES = ("~/Library/Android/sdk", "/opt/homebrew/share/android-commandlinetools",
               "/usr/local/share/android-commandlinetools", "~/Android/Sdk", "/usr/lib/android-sdk")

#: Extensions Android's aapt stores uncompressed (already compressed data),
#: plus Godot's own (``export_plugin.cpp`` ``_should_compress_asset``).
NO_COMPRESS = (".jpg", ".jpeg", ".png", ".gif", ".wav", ".mp2", ".mp3", ".ogg", ".aac", ".mpg", ".mpeg",
               ".mid", ".midi", ".smf", ".jet", ".rtttl", ".imy", ".xmf", ".mp4", ".m4a", ".m4v", ".3gp",
               ".3gpp", ".3g2", ".3gpp2", ".amr", ".awb", ".wma", ".wmv", ".webp", ".cfb", ".scn", ".ctex",
               ".webm", ".opus", ".pck")


class AndroidError(Exception):
    pass


def _version_key(name: str) -> tuple:
    return tuple(int(x) if x.isdigit() else 0 for x in re.split(r"[.\-]", name))


@dataclass
class Sdk:
    root: Path
    build_tools: Path
    platform_jar: Path | None

    def tool(self, name: str) -> str:
        p = self.build_tools / name
        if not p.is_file():
            raise AndroidError(f"the Android build-tools in {self.build_tools} have no {name}")
        return str(p)


def find_sdk(override: str | None = None) -> Sdk | None:
    """``override``, $ANDROID_HOME, $ANDROID_SDK_ROOT, then the usual places;
    the newest build-tools that have apksigner and the newest platform."""
    candidates = [override, os.environ.get("ANDROID_HOME"), os.environ.get("ANDROID_SDK_ROOT"), *SDK_GUESSES]
    for c in candidates:
        if not c:
            continue
        root = Path(c).expanduser()
        bt_dir = root / "build-tools"
        if not bt_dir.is_dir():
            continue
        tools = sorted((d for d in bt_dir.iterdir() if (d / "apksigner").is_file()),
                       key=lambda d: _version_key(d.name), reverse=True)
        if not tools:
            continue
        jars = sorted(((root / "platforms").glob("android-*/android.jar")),
                      key=lambda p: _version_key(p.parent.name.split("-", 1)[1]), reverse=True)
        return Sdk(root, tools[0], jars[0] if jars else None)
    return None


def find_jdk_tool(name: str) -> str | None:
    """keytool / javac / java: $JAVA_HOME, macOS's java_home, then PATH
    (not /usr/bin, which on macOS only asks to install Java)."""
    homes = []
    if os.environ.get("JAVA_HOME"):
        homes.append(Path(os.environ["JAVA_HOME"]))
    if Path("/usr/libexec/java_home").exists():
        r = subprocess.run(["/usr/libexec/java_home"], capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            homes.append(Path(r.stdout.strip()))
    for h in homes:
        if (h / "bin" / name).is_file():
            return str(h / "bin" / name)
    for d in ("/opt/homebrew/opt/openjdk/bin", "/usr/local/opt/openjdk/bin"):
        if (Path(d) / name).is_file():
            return str(Path(d) / name)
    found = shutil.which(name)
    return found if found and not found.startswith("/usr/bin/") else None


def status() -> list[dict]:
    """Rows for ``ptk tools`` / doctor."""
    sdk = find_sdk()
    keytool = find_jdk_tool("keytool")
    javac = find_jdk_tool("javac")
    return [
        {"id": "android-sdk", "name": "Android SDK build-tools", "purpose": "Android APKs",
         "license": "Android SDK License",
         "found": {"id": "android-sdk", "path": str(sdk.build_tools), "source": "system",
                   "version": sdk.build_tools.name} if sdk else None,
         "installable": False,
         "hint": "brew install --cask android-commandlinetools, then: sdkmanager \"build-tools;35.0.0\" "
                 "\"platforms;android-35\""},
        {"id": "jdk", "name": "Java (JDK)", "purpose": "signing Android APKs", "license": "GPL-2.0 with CPE",
         "found": {"id": "jdk", "path": keytool, "source": "system", "version": None} if keytool and javac else None,
         "installable": False, "hint": "brew install openjdk"},
    ]


# -- naming -----------------------------------------------------------------------------


def package_name(title: str, custom: str | None = None) -> str:
    """A valid Java package name: org.portingtoolkit.<title>."""
    if custom:
        if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+", custom):
            raise AndroidError(f"{custom} is not a valid Android package name (like com.you.game)")
        return custom
    part = re.sub(r"[^a-z0-9]+", "_", title.lower()).strip("_") or "game"
    if part[0].isdigit():
        part = "g" + part
    return f"org.portingtoolkit.{part}"


def version_code(version: str, custom: str | None = None) -> int:
    """1.2.3 -> 10203 (Android needs a growing integer)."""
    if custom:
        return int(custom)
    nums = [int(x) for x in re.findall(r"\d+", version)[:3]] + [0, 0, 0]
    return max(1, nums[0] * 10000 + min(nums[1], 99) * 100 + min(nums[2], 99))


ORIENTATIONS = {"landscape": 0, "portrait": 1, "sensor": 4, "sensor_landscape": 6, "sensor_portrait": 7,
                "unspecified": -1, "user": 2, "reverse_landscape": 8, "reverse_portrait": 9}


# -- the APK file ------------------------------------------------------------------------


@dataclass
class Entry:
    """One file of the APK: ``data``, or ``src`` = (file, offset, size) to
    stream from (a game's .pck can be gigabytes)."""

    name: str
    data: bytes | None
    compress: bool
    src: tuple[Path, int, int] | None = None
    date_time: tuple = (2020, 1, 1, 0, 0, 0)

    @property
    def size(self) -> int:
        return self.src[2] if self.src else len(self.data or b"")


def should_compress(name: str) -> bool:
    return not name.lower().endswith(NO_COMPRESS)


def write_aligned(entries: list[Entry], dest: Path) -> Path:
    """Writes an APK whose stored entries start on 4-byte boundaries (and
    native libraries on 16 KB ones), like zipalign, ready for apksigner."""
    seen: set[str] = set()
    with zipfile.ZipFile(dest, "w") as z:
        for e in entries:
            if e.name in seen:
                raise AndroidError(f"duplicate APK entry {e.name}")
            seen.add(e.name)
            info = zipfile.ZipInfo(e.name, date_time=e.date_time)
            info.external_attr = 0o644 << 16
            if e.compress:
                info.compress_type = zipfile.ZIP_DEFLATED
            else:
                info.compress_type = zipfile.ZIP_STORED
                align = SO_ALIGN if e.name.endswith(".so") else ALIGN
                header = z.fp.tell()
                name_len = len(e.name.encode("utf-8"))
                base = header + 30 + name_len
                pad = (-(base + 6)) % align  # room for the 6-byte alignment field itself
                info.extra = struct.pack("<HHH", ALIGNMENT_EXTRA_ID, 2 + pad, align) + b"\0" * pad
            info.file_size = e.size
            with z.open(info, "w", force_zip64=e.size > 0x7FFF0000) as out:
                if e.src:
                    path, offset, size = e.src
                    with open(path, "rb") as f:
                        f.seek(offset)
                        left = size
                        while left:
                            chunk = f.read(min(left, 1 << 20))
                            if not chunk:
                                raise AndroidError(f"{path} ended early while copying {e.name}")
                            out.write(chunk)
                            left -= len(chunk)
                else:
                    out.write(e.data or b"")
    return dest


def check_alignment(apk: Path) -> list[str]:
    """Entries that are stored but not aligned (empty when all is well)."""
    bad = []
    with zipfile.ZipFile(apk) as z, open(apk, "rb") as f:
        for i in z.infolist():
            if i.compress_type != zipfile.ZIP_STORED:
                continue
            f.seek(i.header_offset)
            head = f.read(30)
            n, m = struct.unpack("<HH", head[26:30])
            data = i.header_offset + 30 + n + m
            align = SO_ALIGN if i.filename.endswith(".so") else ALIGN
            if data % align:
                bad.append(i.filename)
    return bad


def debug_keystore(folder: Path) -> tuple[Path, str, str]:
    """A per-computer debug key (made once with keytool): (path, password, alias)."""
    ks = folder / "debug.keystore"
    password, alias = "android", "androiddebugkey"
    if ks.is_file():
        return ks, password, alias
    keytool = find_jdk_tool("keytool")
    if not keytool:
        raise AndroidError("Java (keytool) is needed to sign Android apps: brew install openjdk")
    folder.mkdir(parents=True, exist_ok=True)
    r = subprocess.run([keytool, "-genkeypair", "-v", "-keystore", str(ks), "-storepass", password,
                        "-keypass", password, "-alias", alias, "-keyalg", "RSA", "-keysize", "2048",
                        "-validity", "10000", "-dname", "CN=Porting Toolkit Debug,O=Android,C=US"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise AndroidError("keytool could not make a debug key: " + (r.stderr or r.stdout).strip()[-300:])
    return ks, password, alias


def java_env() -> dict:
    """JAVA_HOME and PATH for the SDK's Java tools (apksigner, d8)."""
    java = find_jdk_tool("java")
    if not java:
        return {}
    return {"JAVA_HOME": str(Path(java).parent.parent),
            "PATH": str(Path(java).parent) + os.pathsep + os.environ.get("PATH", "")}


def sign(apk: Path, sdk: Sdk, keystore: Path, password: str, alias: str, key_password: str | None = None) -> None:
    """apksigner (v1 + v2 + v3 signatures), then a verification."""
    env = {**os.environ, **java_env()}
    r = subprocess.run([sdk.tool("apksigner"), "sign", "--ks", str(keystore), "--ks-pass", f"pass:{password}",
                        "--ks-key-alias", alias, "--key-pass", f"pass:{key_password or password}", str(apk)],
                       capture_output=True, text=True, env=env)
    if r.returncode != 0:
        raise AndroidError("apksigner failed: " + (r.stderr or r.stdout).strip()[-400:])
    r = subprocess.run([sdk.tool("apksigner"), "verify", str(apk)], capture_output=True, text=True, env=env)
    if r.returncode != 0:
        raise AndroidError("the signed APK does not verify: " + (r.stderr or r.stdout).strip()[-400:])
    for leftover in (apk.with_name(apk.name + ".idsig"),):
        leftover.unlink(missing_ok=True)


def png_size(data: bytes) -> tuple[int, int] | None:
    if data[:8] != b"\x89PNG\r\n\x1a\n" or len(data) < 24:
        return None
    return struct.unpack(">II", data[16:24])


def resized_png(src: Path, width: int, height: int, dest: Path) -> bytes | None:
    """The image as a PNG of the given size (macOS sips), or None."""
    sips = shutil.which("sips")
    if not sips:
        return None
    r = subprocess.run([sips, "-s", "format", "png", "-z", str(height), str(width), str(src), "--out", str(dest)],
                       capture_output=True, text=True)
    return dest.read_bytes() if r.returncode == 0 and dest.is_file() else None


def replace_icons(entries: list[Entry], icon: Path, temp: Path,
                  names=("icon.png", "icon_foreground.png")) -> int:
    """Swaps the template's launcher icons (res/mipmap-*/icon*.png) for
    ``icon``, each at the size of the one it replaces."""
    done = 0
    for e in entries:
        base = e.name.rsplit("/", 1)[-1]
        if not (e.name.startswith("res/mipmap") and base in names) or e.data is None:
            continue
        size = png_size(e.data)
        if not size:
            continue
        data = resized_png(icon, size[0], size[1], temp / f"icon-{done}.png")
        if data:
            e.data = data
            done += 1
    return done


def sign_for_export(ctx, apk: Path, sdk: Sdk) -> str:
    """Signs with ``--set keystore=...`` (``keystore_pass``, ``key_alias``,
    ``key_pass``) or this computer's debug key; returns what was used."""
    ks = ctx.setting("keystore")
    try:
        if ks:
            sign(apk, sdk, Path(ks).expanduser(), ctx.setting("keystore_pass") or "", ctx.setting("key_alias") or "",
                 ctx.setting("key_pass"))
            return f"your key ({Path(ks).name})"
        path, password, alias = debug_keystore(ctx.cache / "android")
        sign(apk, sdk, path, password, alias)
    except AndroidError as e:
        ctx.fail(str(e))
    return "this computer's debug key (fine for testing; Google Play needs your own key: --set keystore=...)"


def settings_for_export(ctx, title: str, default_orientation: int) -> dict:
    """Package, version and orientation from the project and ``--set``."""
    version = str(ctx.manifest.get("version") or "1.0.0")
    orientation = ctx.setting("orientation")
    if orientation and orientation not in ORIENTATIONS:
        ctx.fail(f"orientation={orientation}: use one of {', '.join(ORIENTATIONS)}")
    try:
        package = package_name(title, ctx.setting("package"))
    except AndroidError as e:
        ctx.fail(str(e))
    return {
        "package": package,
        "version": version,
        "code": version_code(version, ctx.setting("version_code")),
        "orientation": ORIENTATIONS[orientation] if orientation else default_orientation,
        "permissions": [p.strip() for p in (ctx.setting("permissions") or "").split(",") if p.strip()],
    }


def patch_manifest(data: bytes, package: str, code: int, version: str, orientation: int,
                   permissions: list[str]) -> bytes:
    """Renames a template APK's manifest the way Godot's exporter does:
    package, versions, the main activity's orientation, provider
    authorities, and added permissions."""
    from .formats import axml
    m = axml.Manifest(data)
    old = m.to_dict()["package"]
    man = m.elements("manifest")[0]
    m.set_string(man, "package", package)
    m.set_int(man, "versionCode", code)
    m.set_string(man, "versionName", version)
    for act in m.elements("activity"):
        if m.attr(act, "screenOrientation") is not None and str(m.value(act, "name")).startswith(old or "\0"):
            m.set_int(act, "screenOrientation", orientation)
    for prov in m.elements("provider"):
        auth = m.value(prov, "authorities")
        if isinstance(auth, str) and old and auth.startswith(old + "."):
            m.set_string(prov, "authorities", package + auth[len(old):])
    for p in permissions:
        m.add_permission(p if "." in p else f"android.permission.{p.upper()}")
    return m.build()
