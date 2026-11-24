"""HTML5 games: RPG Maker MV/MZ, NW.js and Electron builds.

These games are web pages plus JavaScript, wrapped in a desktop runtime
(NW.js or Electron). Porting them means finding the web root (the folder
with the game's ``index.html``), taking it out of its container if needed
(``package.nw`` is a zip, ``app.asar`` is Electron's archive format), and
giving it a new host: a browser, or a small native macOS app.
"""

from __future__ import annotations

import html
import json
import os
import re
import shutil
import struct
import zipfile
from dataclasses import asdict, dataclass, field
from pathlib import Path, PurePosixPath

SKIP_DIRS = {"node_modules", "__MACOSX", ".git", "locales", "swiftshader"}


class Html5Error(Exception):
    pass


@dataclass
class Html5Game:
    flavor: str  # rpgmaker-mv, rpgmaker-mz, nwjs, electron, web
    container: str  # folder, package.nw, asar
    container_path: str  # relative to the game folder ("" for the folder itself)
    root: str  # web root, relative to the container
    entry: str = "index.html"  # page inside the root
    title: str | None = None
    width: int | None = None
    height: int | None = None
    icon: str | None = None  # relative to the root
    uses_node: bool = False
    #: The tool that made the game, when known: Construct 3, GDevelop, Twine, ...
    framework: str | None = None
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)


# -- containers -------------------------------------------------------------


def read_asar_header(path: Path) -> tuple[dict, int]:
    """(file tree, offset of the file data) of an Electron ``.asar``."""
    with open(path, "rb") as f:
        head = f.read(16)
        if len(head) < 16:
            raise Html5Error(f"{path.name} is not an asar archive")
        _, header_size, _, json_len = struct.unpack("<IIII", head)
        raw = f.read(json_len)
    try:
        tree = json.loads(raw.decode("utf-8"))
    except ValueError as e:
        raise Html5Error(f"{path.name}: damaged asar header ({e})") from e
    return tree, 8 + header_size


def _safe_join(base: Path, rel: str) -> Path:
    parts = PurePosixPath(rel.replace("\\", "/")).parts
    if any(p in ("..", "") for p in parts) or rel.startswith(("/", "\\")) or ":" in rel:
        raise Html5Error(f"unsafe path in archive: {rel!r}")
    return base.joinpath(*parts)


def extract_asar(path: Path, dest: Path) -> int:
    """Extracts an ``.asar`` (plus ``.asar.unpacked``) into ``dest``."""
    tree, base = read_asar_header(path)
    unpacked = path.with_name(path.name + ".unpacked")
    count = 0
    with open(path, "rb") as f:

        def walk(node: dict, rel: str) -> None:
            nonlocal count
            for name, child in (node.get("files") or {}).items():
                sub = f"{rel}/{name}" if rel else name
                if "files" in child:
                    _safe_join(dest, sub).mkdir(parents=True, exist_ok=True)
                    walk(child, sub)
                elif "link" in child:
                    continue
                else:
                    out = _safe_join(dest, sub)
                    out.parent.mkdir(parents=True, exist_ok=True)
                    if child.get("unpacked"):
                        src = _safe_join(unpacked, sub)
                        if src.is_file():
                            shutil.copyfile(src, out)
                            count += 1
                        continue
                    f.seek(base + int(child.get("offset", 0)))
                    left = int(child.get("size", 0))
                    with open(out, "wb") as o:
                        while left:
                            chunk = f.read(min(left, 1 << 22))
                            if not chunk:
                                raise Html5Error(f"{path.name} ends early")
                            o.write(chunk)
                            left -= len(chunk)
                    count += 1

        walk(tree, "")
    return count


def asar_read(path: Path, rel: str) -> bytes | None:
    """One file from an ``.asar`` without extracting the rest."""
    tree, base = read_asar_header(path)
    node = tree
    for part in rel.split("/"):
        node = (node.get("files") or {}).get(part)
        if node is None:
            return None
    if "offset" not in node:
        return None
    with open(path, "rb") as f:
        f.seek(base + int(node["offset"]))
        return f.read(int(node["size"]))


def asar_names(path: Path) -> list[str]:
    tree, _ = read_asar_header(path)
    out: list[str] = []

    def walk(node: dict, rel: str) -> None:
        for name, child in (node.get("files") or {}).items():
            sub = f"{rel}/{name}" if rel else name
            if "files" in child:
                walk(child, sub)
            elif "link" not in child:
                out.append(sub)

    walk(tree, "")
    return out


def extract_zip(path: Path, dest: Path) -> int:
    count = 0
    with zipfile.ZipFile(path) as z:
        for info in z.infolist():
            if info.is_dir() or info.filename.startswith("__MACOSX/"):
                continue
            out = _safe_join(dest, info.filename)
            out.parent.mkdir(parents=True, exist_ok=True)
            with z.open(info) as s, open(out, "wb") as o:
                shutil.copyfileobj(s, o, 1 << 22)
            count += 1
    return count


class Listing:
    """A uniform view of a folder, a package.nw zip or an asar."""

    def __init__(self, kind: str, path: Path):
        self.kind = kind
        self.path = path
        if kind == "folder":
            names = []
            for dirpath, dirnames, filenames in os.walk(path):
                dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS and not d.startswith(".")]
                rel = Path(dirpath).relative_to(path).as_posix()
                if rel.count("/") > 6:
                    dirnames[:] = []
                for n in filenames:
                    names.append(n if rel == "." else f"{rel}/{n}")
            self.names = names
        elif kind == "package.nw":
            with zipfile.ZipFile(path) as z:
                self.names = [n for n in z.namelist() if not n.endswith("/")]
        else:
            self.names = asar_names(path)
        self.lower = {n.lower(): n for n in self.names}

    def has(self, rel: str) -> bool:
        return rel.lower() in self.lower

    def read(self, rel: str) -> bytes | None:
        real = self.lower.get(rel.lower())
        if real is None:
            return None
        if self.kind == "folder":
            return (self.path / real).read_bytes()
        if self.kind == "package.nw":
            with zipfile.ZipFile(self.path) as z:
                return z.read(real)
        return asar_read(self.path, real)

    def extract(self, dest: Path) -> Path:
        if self.kind == "folder":
            return self.path
        dest.mkdir(parents=True, exist_ok=True)
        if self.kind == "package.nw":
            extract_zip(self.path, dest)
        else:
            extract_asar(self.path, dest)
        return dest


# -- detection ----------------------------------------------------------------


def _parent(rel: str) -> str:
    return rel.rsplit("/", 1)[0] if "/" in rel else ""


def _join(a: str, b: str) -> str:
    return f"{a}/{b}" if a else b


def _package(listing: Listing, folder: str) -> dict:
    raw = listing.read(_join(folder, "package.json"))
    if not raw:
        return {}
    try:
        data = json.loads(raw.decode("utf-8-sig"))
    except ValueError:
        return {}
    return data if isinstance(data, dict) else {}


def _from_listing(listing: Listing) -> tuple[str, str, str, dict] | None:
    """(flavor, root, entry, package.json) inside one container."""
    names = sorted(listing.names, key=lambda n: (n.count("/"), n))
    for n in names:
        low = n.lower()
        if low.endswith("js/rpg_core.js") or low.endswith("js/rmmz_core.js"):
            root = _parent(_parent(n))
            if listing.has(_join(root, "index.html")):
                flavor = "rpgmaker-mz" if low.endswith("rmmz_core.js") else "rpgmaker-mv"
                pkg_dir = _parent(root) if root.lower().endswith("www") else root
                return flavor, root, "index.html", _package(listing, pkg_dir) or _package(listing, root)
    for n in names:
        if n.lower().endswith("package.json") and "node_modules/" not in n.lower():
            folder = _parent(n)
            pkg = _package(listing, folder)
            main = str(pkg.get("main") or "")
            if main.lower().endswith((".html", ".htm")):
                page = _join(folder, main.lstrip("./"))
                if listing.has(page):
                    return "nwjs", _parent(page), page.rsplit("/", 1)[-1], pkg
    pages = [n for n in names if n.lower().endswith("index.html") and n.rsplit("/", 1)[-1].lower() == "index.html"]
    if pages:
        page = pages[0]
        pkg = _package(listing, _parent(page))
        return "web", _parent(page), page.rsplit("/", 1)[-1], pkg
    # A Twine story is one page with any name ("My Story.html").
    for n in names:
        if n.count("/") <= 1 and n.lower().endswith((".html", ".htm")) and _twine_title(listing.read(n)) is not None:
            return "web", _parent(n), n.rsplit("/", 1)[-1], {}
    return None


#: Script names that give away the tool that made an HTML5 game.
FRAMEWORKS = (
    ("c3runtime.js", "Construct 3"),
    ("c3main.js", "Construct 3"),
    ("c2runtime.js", "Construct 2"),
    ("gd.js", "GDevelop"),
    ("dmloader.js", "Defold"),
    ("phaser", "Phaser"),
    ("cocos2d-js", "Cocos"),
)


def _twine_title(page: bytes | None) -> str | None:
    """The story name of a Twine page, '' when unnamed, None if not Twine."""
    if not page or b"<tw-storydata" not in page:
        return None
    m = re.search(rb'<tw-storydata[^>]*\bname="([^"]*)"', page)
    return html.unescape(m.group(1).decode("utf-8", "replace")).strip() if m else ""


def _framework(listing: Listing, root: str, entry: str) -> tuple[str | None, str | None]:
    """(framework, title from the framework's own data) for a web root."""
    prefix = root.lower() + "/" if root else ""
    for n in listing.names:
        low = n.lower()
        if not low.startswith(prefix) or low[len(prefix):].count("/") > 2:
            continue
        base = low.rsplit("/", 1)[-1]
        for marker, name in FRAMEWORKS:
            if base == marker or (not marker.endswith(".js") and base.startswith(marker) and base.endswith(".js")):
                return name, None
    title = _twine_title(listing.read(_join(root, entry)))
    if title is not None:
        return "Twine", title or None
    return None, None


def detect(game: Path) -> Html5Game:
    """Finds the web root of an HTML5 game build."""
    game = Path(game)
    containers: list[tuple[str, Path]] = []
    if game.is_file():
        suffix = game.suffix.lower()
        if suffix == ".asar":
            containers.append(("asar", game))
        elif suffix in (".nw", ".zip") or game.name == "package.nw":
            containers.append(("package.nw", game))
        else:
            raise Html5Error(f"{game.name} is not an HTML5 game (choose its folder)")
    else:
        found_nw = sorted(game.rglob("package.nw"), key=lambda p: len(p.parts))
        found_asar = sorted(game.rglob("app.asar"), key=lambda p: len(p.parts))
        for p in found_nw:
            containers.append(("folder" if p.is_dir() else "package.nw", p))
        for p in found_asar:
            if p.is_file():
                containers.append(("asar", p))
        containers.append(("folder", game))
    for kind, path in containers:
        try:
            listing = Listing(kind, path)
        except (OSError, zipfile.BadZipFile, Html5Error):
            continue
        hit = _from_listing(listing)
        if hit is None:
            continue
        flavor, root, entry, pkg = hit
        pkg = pkg or _package(listing, "")
        if kind == "asar" or "resources/app" in path.as_posix().lower():
            flavor = flavor if flavor.startswith("rpgmaker") else "electron"
        elif kind == "package.nw" and flavor == "web":
            flavor = "nwjs"
        window = pkg.get("window") if isinstance(pkg.get("window"), dict) else {}
        rel = "" if path == game else path.relative_to(game).as_posix()
        g = Html5Game(
            flavor=flavor,
            container=kind,
            container_path=rel,
            root=root,
            entry=entry,
            title=window.get("title") or pkg.get("productName") or pkg.get("name"),
            width=_int(window.get("width")),
            height=_int(window.get("height")),
        )
        icon = window.get("icon")
        if isinstance(icon, str):
            prefix = root + "/" if root else ""
            icon = icon.lstrip("./")
            g.icon = icon[len(prefix):] if icon.startswith(prefix) else icon
        elif listing.has(_join(root, "icon/icon.png")):
            g.icon = "icon/icon.png"
        g.framework, framework_title = _framework(listing, root, entry)
        if framework_title:
            g.title = framework_title
        if flavor.startswith("rpgmaker"):
            # RPG Maker keeps the real title in data/System.json.
            try:
                system = json.loads((listing.read(_join(root, "data/System.json")) or b"{}").decode("utf-8-sig"))
                if isinstance(system, dict) and str(system.get("gameTitle") or "").strip():
                    g.title = str(system["gameTitle"]).strip()
            except (ValueError, UnicodeDecodeError, OSError):
                pass
        page = listing.read(_join(root, entry)) or b""
        g.uses_node = flavor == "electron" or b"require(" in page or b"process.versions" in page
        if g.flavor == "electron":
            g.notes.append("Electron games can use Node.js; parts that do will not work in a browser.")
        return g
    raise Html5Error("no index.html found: this does not look like an HTML5 game build")


def _int(v) -> int | None:
    try:
        n = int(v)
    except (TypeError, ValueError):
        return None
    return n if 100 <= n <= 8192 else None


def materialize(game: Path, info: dict, temp: Path) -> Path:
    """The web root as a real folder (extracting the container if needed)."""
    base = game if not info.get("container_path") else game / info["container_path"]
    kind = info.get("container", "folder")
    if game.is_file():
        base = game
    if kind == "folder":
        folder = base
    else:
        folder = Listing(kind, base).extract(temp / "app")
    root = folder / info["root"] if info.get("root") else folder
    if not (root / info.get("entry", "index.html")).is_file():
        raise Html5Error(f"{info.get('entry', 'index.html')} is missing from the game (run the import again?)")
    return root
