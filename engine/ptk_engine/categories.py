"""Asset categories.

The ids must match ``CATEGORIES`` in ``crates/ptk-core/src/category.rs``;
the Rust integration test compares them through ``engine.info``.
Classification looks at names and extensions only.
"""

from __future__ import annotations

CATEGORY_IDS: list[str] = [
    "textures",
    "models",
    "audio",
    "video",
    "shaders",
    "scripts",
    "data",
    "fonts",
    "localization",
    "archives",
    "executables",
    "other",
]

_EXTENSIONS: dict[str, tuple[str, ...]] = {
    "textures": (
        "png", "jpg", "jpeg", "dds", "ktx", "ktx2", "tga", "bmp", "tif", "tiff",
        "webp", "exr", "hdr", "psd", "astc", "pvr", "vtf", "ytd", "tex", "texture2d",
        "icns", "ico", "gif",
    ),
    "models": (
        "fbx", "obj", "gltf", "glb", "dae", "3ds", "blend", "mdl", "smd", "ydr",
        "yft", "ydd", "ybn", "mesh", "anim", "ycd", "skel",
    ),
    "audio": (
        "wav", "ogg", "mp3", "flac", "aiff", "aif", "m4a", "opus", "wma", "bank",
        "fsb", "wem", "bnk", "awc", "xwm", "xwb", "xsb", "mid", "midi",
    ),
    "video": ("mp4", "webm", "bik", "bk2", "usm", "mov", "avi", "mkv", "ogv", "wmv", "m4v"),
    "shaders": (
        "hlsl", "glsl", "spv", "cso", "fxc", "fx", "metal", "metallib", "shader",
        "ush", "usf", "vert", "frag", "comp", "ysc_shader",
    ),
    "scripts": (
        "lua", "luac", "py", "pyc", "js", "mjs", "cs", "rpy", "rpyc", "gd", "gdc",
        "nut", "ysc", "rb", "as", "sh", "bat", "ps1",
    ),
    "data": (
        "json", "xml", "yaml", "yml", "ini", "cfg", "toml", "csv", "dat", "db",
        "sqlite", "umap", "tscn", "scn", "tres", "res", "bsp", "ymap", "ytyp",
        "meta", "rxdata", "rvdata", "rvdata2", "plist", "txt", "unity", "prefab",
    ),
    "fonts": ("ttf", "otf", "ttc", "fnt", "woff", "woff2", "fon"),
    "localization": ("po", "mo", "pot", "locres", "locmeta", "gxt2", "gxt", "strings", "xliff"),
    "archives": (
        "pak", "utoc", "ucas", "rpf", "vpk", "pck", "rpa", "rgss3a", "rgss2a",
        "rgssad", "assets", "ress", "resource", "unity3d", "bundle", "bsa", "ba2",
        "wad", "gcf", "zip", "7z", "rar", "cab", "arc", "big", "win", "unx",
    ),
    "executables": ("exe", "dll", "so", "dylib", "bin", "elf", "app", "framework", "x86_64"),
}

EXTENSION_CATEGORY: dict[str, str] = {
    ext: cat for cat, exts in _EXTENSIONS.items() for ext in exts
}

# Whole names that the extension alone would get wrong.
NAME_CATEGORY: dict[str, str] = {
    "data.win": "archives",
    "game.unx": "archives",
    "game.ios": "archives",
    "game.droid": "archives",
    "globalgamemanagers": "data",
    "info.plist": "data",
    "pkginfo": "data",
}


def classify(rel_path: str, magic: str | None = None) -> str:
    """Category id for a file, from its name, extension and magic bytes."""
    name = rel_path.rsplit("/", 1)[-1].lower()
    if name in NAME_CATEGORY:
        return NAME_CATEGORY[name]
    if magic in ("mach-o", "elf", "pe"):
        return "executables"
    if "." in name:
        ext = name.rsplit(".", 1)[1]
        if ext in EXTENSION_CATEGORY:
            return EXTENSION_CATEGORY[ext]
        # Numbered archive parts such as `pak01_000.vpk` are covered above;
        # `level0`, `sharedassets0.assets.resS` etc. fall through to here.
        if ext.startswith("ress"):
            return "archives"
    if name.startswith("level") and name[5:].isdigit():
        return "data"
    return "other"
