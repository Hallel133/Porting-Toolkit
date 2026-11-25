# Plugins

Import and export plugins do the actual conversion work.

- An **import** plugin reads a game (an engine or format) and records what
  it learned in the project's `recipes/` (never game files).
- An **export** plugin builds the game for one or more targets, from your
  own copy of the game, into an output folder you choose.

`ptk import` runs the import plugin for the detected engine; `ptk export`
picks the export plugin for the engine and target (`--plugin ID` chooses
one). Installed plugins with the same id replace the built-in ones.

## Built-in plugins

They ship in `engine/plugins/` and are listed as built-in (and reviewed).

| Plugin | Engines | Targets | What it does |
|---|---|---|---|
| `godot-import` | godot | | Finds the `.pck` (standalone, appended to an `.exe`, or in an `.app`), reads its Godot version and flags into `recipes/godot.json` |
| `godot-export` | godot | macos-arm64, macos-x64, windows-x64, linux-x64, web, android | `.pck` + the official Godot export template of the same version |
| `html5-import` | rpgmaker, html5 | | Finds the web root of RPG Maker MV/MZ, NW.js (`package.nw`) and Electron (`app.asar`) games into `recipes/html5.json` |
| `renpy-import` | renpy | | Finds game/ and renpy/ (also in a Mac app), the Ren'Py version, the platform files the build has, and lists `.rpa` archives into `recipes/renpy.json` |
| `renpy-export` | renpy | macos-arm64, macos-x64, windows-x64, linux-x64 | Ren'Py's own distribution layout, with platform files from the game or the SDK of the same version |
| `love-import` | love | | Finds the `.love` (alone, fused into an `.exe`, in a Mac app, or a source folder) and reads `conf.lua` into `recipes/love.json` |
| `love-export` | love | macos-arm64, macos-x64, windows-x64, linux-x64 | The game with the official LÖVE 11.5 build: Mac app, fused `.exe`, or AppImage + `game.love` |
| `rgss-import` | rgss | | Reads RPG Maker XP/VX/VX Ace `Game.ini` and the archive header into `recipes/rgss.json` |
| `rgss-export` | rgss | macos-arm64, macos-x64, windows-x64, linux-x64 | mkxp-z for macOS and Linux (you provide it once), the original game for Windows |
| `unity-import` | unity | | Reports the build: Data folder, Unity version, Mono or IL2CPP, native plugins, Apple-only services, App Store / Apple Arcade |
| `unity-export` | unity | macos-arm64, macos-x64, windows-x64, linux-x64 | Mono builds only, experimental: the game's Data with the Unity player of the same version (from a Unity install, or downloaded once from Unity) |
| `html5-export` | rpgmaker, html5 | macos-arm64, macos-x64, windows-x64, linux-x64, web, android | The web root as a web folder, inside a native WebKit `.app`, with the NW.js runtime for Windows and Linux, or in a WebView player APK |

**Android (experimental).** Shared code is in `engine/ptk_engine/android.py`
(SDK and JDK discovery, `write_aligned`: stored entries on 4-byte and `.so` on
16 KB boundaries with an `0xD935` alignment field, apksigner signing with a
per-computer debug key or `--set keystore=`, `keystore_pass`, `key_alias`,
`key_pass`) and `formats/axml.py` (binary manifest and `resources.arsc`
string pools: rename the package, versions, orientation, provider
authorities, add permissions, replace the app name).

- Godot 4 follows `platform/android/export/export_plugin.cpp` (4.4) without
  Gradle: `android_release.apk` from the templates, `META-INF/` and the ABIs
  not in `--set abis=` (default `arm64-v8a,armeabi-v7a`) dropped, the game's
  files as `assets/<res path>` (compression per Godot's list), `assets/_cl_`,
  `godot-project-name*` strings set to the title, launcher icons replaced.
  Textures imported only as S3TC/BPTC get a plain `path=` fallback in their
  `.import` remap (Godot's loader uses it when no `path.<feature>` matches, and
  both renderers decode BC formats a GPU lacks; templates have the decoder
  since 4.2). Godot 3, and Godot 4.0/4.1 games that need the fallback, are
  refused (`--set force=true` tries anyway). `--set sign=false` leaves the APK
  unsigned.
- ASTC for those textures (`android_textures=astc`, the default;
  `ptk_engine/texconv`, `formats/ctex.py`): the `.ctex` (`GST2` header, then
  `data_format`, size, mip count, `Image::Format` and every mip level padded to
  its block size, as in Godot's `compressed_texture.cpp` and
  `Image::_get_dst_image_size`) is read; each level of DXT1/DXT3/DXT5/BC7 is
  decoded by `ptk-bcdec` (`engine/third_party/bcdec`, MIT/Unlicense, compiled
  once with `cc`) and encoded by astcenc `-cl` (the LDR profile Godot's own
  astcenc module uses), 4x4 or `astc_block=8x8`, preset `astc_quality=`; the
  new `.astc.ctex` keeps the size, flags and mip count. The remap gets
  `path.astc=` before the `path=` fallback, so ASTC phones load it and others
  still decode the desktop file. Normal-map swizzles, BC4/BC5 and HDR formats
  stay on the fallback. Results are cached by content in the plugin cache.
- HTML5: `android/` holds the player (one Activity with a WebView serving
  `assets/www/` at `https://appassets.androidplatform.net/www/`, with a
  case-insensitive fallback from `assets/ptk-files.txt`); it is built once
  with aapt2, javac (`--release 11`) and d8 against the newest platform and
  cached in the plugin cache. Orientation follows the game's window (portrait
  when taller than wide) unless `--set orientation=` is given.

**Unity (experimental).** Mono builds keep the game's code in
`Data/Managed` (IL), which runs with another platform's Unity player of the
same version, from
`PlaybackEngines/<Platform>StandaloneSupport/Variations/<win64|linux64|macos_x64arm64>_player_nondevelopment_mono`.
A Unity install is used when there is one (Unity Hub,
`/Applications/Unity/Hub/Editor/<v>`, Hub's secondary install path, or
`--set unity=PATH`). Otherwise the player is downloaded once, after the user
accepts the Unity Terms of Service (`accept_unity_terms=true`, remembered;
the GUI shows an Accept button): Unity's release service gives the version's
"Build Support (Mono)" module (a .pkg on download.unity3d.com, checked
against Unity's published MD5), and `ptk_engine.formats.xar` takes only the
release Mono player out of the .pkg's gzip'd cpio Payload, without
installing anything. It is cached in the plugin's cache under
`players/<version>`. The import also reads which graphics APIs the build's shaders were compiled for (from the Shader objects in the Data folder's serialized files) and says which platforms can draw the game; a Mac build is usually Metal only. `unity_download=false` turns this off; the error then
names the module and gives a `unityhub://<version>/<changeset>` link
(`unity_lookup=false` skips it). The game's Data goes to `<Name>_Data`
(Windows, Linux; MonoBleedingEdge inside it on Linux) or
`Contents/Resources/Data` (macOS); `UnityEngine*.dll` and `unity default
resources` come from the target player; the source platform's native
`Plugins` are left out. Shaders cannot be added, so a build without the
target's graphics API (Metal, Direct3D, Vulkan/OpenGL) shows pink or
invisible objects; the export says so, and about native plugins, Apple
services and App Store / Apple Arcade terms. IL2CPP builds are refused.

**Ren'Py.** The layout follows Ren'Py's `launcher/game/distribute.rpy`:
`<name>.exe` + `lib/py3-windows-x86_64/`, `<name>.sh` +
`lib/py3-linux-x86_64/<name>`, or `<Title>.app` with the native files in
`Contents/MacOS`, `lib/python3.x` in `Contents/Resources/lib` and the game in
`Contents/Resources/autorun` (Ren'Py 7 uses `py2-`). Platform files come from
the build when it has them; otherwise the version's `renpy-<v>-sdk.zip` is
downloaded once from renpy.org, checked against the SHA-256 in its
`checksums.txt`, and only `lib/` and the launchers are unpacked (the zip is
then deleted). `game/saves`, `game/cache` and logs are never copied. `.rpa`
indexes are read with an unpickler that only allows plain values; archives
are never extracted. `--set sdk=PATH`, `--set renpy_version=X.Y.Z`.

**LÖVE.** `conf.lua` is read with regular expressions (`t.version`,
`t.identity`, `t.window.title/width/height/icon`). Exports use LÖVE 11.5
(SHA-256 of the three release assets pinned in the plugin); games made for
another LÖVE get a warning and `--set love=PATH` uses a matching build. The
Mac app keeps the framework symlinks of LÖVE's app and drops its `.love`
file-type claims; Windows gets `love.exe` + `game.love` as one `.exe`; Linux
gets the AppImage, `game.love` and a `--fused` start script.

**RPG Maker XP/VX/VX Ace (`rgss`).** Only `Game.ini` (UTF-8 or Shift-JIS)
and the first 8 bytes of `.rgssad`/`.rgss2a`/`.rgss3a` are read. mkxp-z
publishes no release builds (only CI builds behind a GitHub sign-in), so
nothing is pinned: `--set mkxp=PATH` points at its Mac app or Linux folder
once and is remembered per computer. On macOS the game goes into
`Contents/Game`, on Linux next to mkxp-z's executable, both with an
`mkxp.json` (`rgssVersion`, `windowTitle`, `RTP`). `--set rtp=PATH` copies
the RPG Maker RTP the game needs. A GPL-2.0 note with mkxp-z's source link
goes into the build. Save files (`Save01.rvdata2`...) are never copied.

**HTML5 on Windows and Linux.** The game runs in NW.js, the runtime RPG
Maker MV/MZ ship with (Electron would break their `nw.gui` and save code).
The pinned NW.js release (`NWJS_VERSION` in the plugin, SHA-256 from
dl.nwjs.io's `SHASUMS256.txt`) is downloaded once and cached; `--set
nwjs=PATH` uses a local folder or archive. `nw.exe` / `nw` becomes
`<Title>.exe` / `<Title>`, the game goes into `package.nw/` with a
`package.json` whose `main` is the game's page, and `credits.html` keeps
the licences. NW.js has no H.264/AAC decoder, so `.mp4`/`.m4a` files without
a `.webm`/`.ogg` twin are reported.

**Godot.** The pck formats of Godot 3 (1), 4.0–4.3 (2) and 4.4+ (3) are
read; embedded pcks are cut out of the executable (Godot 3 offsets are
rewritten when needed). Templates are looked up in `--set templates=PATH`
(a folder or `.tpz`), then the Godot editor's own folder
(`~/Library/Application Support/Godot/export_templates/<version>.stable`),
then ptk's cache. If none has them, only the needed members of the release's
`.tpz` are fetched from GitHub with HTTP range requests and kept in the
cache. The `.pck` does not record rc/beta builds; use
`--set godot_version=4.3.rc1` for those.

- macOS: the release binary is renamed to the game's name, thinned with
  `lipo` to the target architecture (`--set thin=false` keeps it universal),
  Info.plist is filled in (`--set bundle_id=…`), and the app is ad-hoc
  signed (`--set sign=false` skips it; Apple Silicon needs a signature).
- Web: the single-threaded template (Godot 4.3+) is used, so the build runs
  on any static host such as itch.io. `--set threads=true` uses the threaded
  template and adds Godot's service worker for the COOP/COEP headers.
- Windows / Linux: `<Title>/<Title>.exe` (or `<Title>.x86_64`) from the
  release template (`windows_release_x86_64.exe`, `linux_release.x86_64`;
  Godot 3: `windows_64_release.exe`, `linux_x11_64_release`) with
  `<Title>.pck` next to it, since Godot loads the pck named after its
  executable. GDExtension libraries for that platform and architecture,
  their `[dependencies]` (such as `steam_api64.dll`) and `steam_appid.txt`
  are copied from the game folder; a missing one is a warning. A zip of the
  folder (`--set zip=false` skips it) keeps the Linux executable bit. The
  `.exe` keeps Godot's icon (changing it needs a PE resource editor) and is
  not signed, so SmartScreen may ask once.
- Import reads the game's `project.binary` (its Godot project settings):
  the real title (replacing a title guessed from a file name), the
  renderer, the icon and boot splash, and the autoloads. The game's icon is
  cached for this computer only, never put in the project.
- GDExtension libraries (GodotSteam and similar): when the game is a macOS
  `.app`, its `Contents/Frameworks` is copied into the new app, which is
  where Godot looks for them. A Windows or Linux build has no macOS
  libraries, so the export warns that features needing them will not work
  (a game that requires one at startup can stop on a blank screen).
- Icon: `--set icon=`, then the project's icon (`ptk art --icon`), then the
  source `.app`'s icon, then the game's own icon from the pck.
- Web: the loading screen uses the game's boot splash and background
  color, the page icon is the project's or the game's. Browsers only have
  Godot's Compatibility renderer, so games made for Forward+ or Mobile get a
  warning: lighting, glow, fog, some particles and screen shaders can look
  different. Only the game's developer can change the renderer.
- Refused, with an explanation: encrypted pcks (they need templates built
  with the game's key), C# (.NET) games. GDExtension libraries produce a
  warning: the target needs its own build of them.

**HTML5.** The web export copies the web root and drops desktop-only files
(`nw.dll` and friends). The macOS app compiles `wrapper.swift` once per
architecture with `swiftc` (cached), serves the game through an `app://`
scheme (so XHR/fetch work, with byte ranges for media), sizes the window
from `package.json`, converts the game's icon with `sips`/`iconutil` and
signs ad-hoc. Saves use WebKit's local storage, per app. Electron games
that use Node.js APIs get a warning: those parts only work in Electron.

Ad-hoc signed apps run on the Mac that built them. To give them to others,
sign them with a Developer ID and notarize them.

## Writing a plugin

Plugins use **API 2** (`api = 2` in `plugin.toml`): subclass a base class from
`ptk_engine.sdk` and fill in its steps. `ptk plugin new` writes a working
skeleton.

```python
from ptk_engine.sdk import ExportPlugin, error, warning


class Plugin(ExportPlugin):
    media_profile = "html5"          # rules for the advanced options (or None)

    def check(self, ctx):            # fast, writes nothing
        if not (ctx.game / "index.html").is_file():
            return [error("no index.html in the game folder")]
        if not ctx.setting("templates"):
            return [error("the export templates are missing",
                          {"setting": "templates", "value": "/path", "label": "Choose templates"})]
        return []

    def prepare(self, ctx):          # downloads, tools (may be slow)
        self.runtime = ctx.download("https://example.org/runtime.zip", sha256="…")

    def layout(self, ctx, build):    # declare the output; nothing is written yet
        build.add_tree(ctx.game, "Game", skip=lambda p: p.name == ".DS_Store")
        build.add_file(self.runtime, "Game/runtime.zip")
        build.add_text("Game/README.txt", "Built with Porting Toolkit\n")
        build.artifact("Game.zip")    # made in finish(): kept, not checked

    def finish(self, ctx, build):    # after the sync: in-place steps and the result
        assets = self.media(ctx, build, "Game", self.media_profile)
        zipped = self.zip(ctx, build, "Game", "Game.zip")
        return {"outputs": [str(build.root / "Game"), str(zipped)], "summary": "Built it", "assets": assets}
```

```
check ──▶ prepare ──▶ layout ──▶ [sync: write only what changed] ──▶ finish ──▶ [seal]
  │
  └── also run alone: `ptk export --check`, and the app's Export page
```

- **Incremental output.** Between `layout` and `finish` the base class syncs
  `$OUTPUT` with what you declared (`ptk_engine.build.Build`). Unchanged entries
  are not written again, new and changed ones are cloned (APFS copy-on-write on
  macOS, a copy elsewhere), and entries you no longer declare are removed. Files
  you change in place in `finish` (plists, re-encoded media, code signatures) are
  recorded afterwards, so the next export leaves them alone too. Changing the
  project's options, the target, the settings or the plugin's version rewrites
  everything; `variant(ctx)` adds anything else that should.
- **`build`:** `add_tree(src, dest, skip=, links=)`, `add_file(src, dest, mode=)`,
  `add_bytes` / `add_text(dest, …)`, `add_link(dest, target)`, `add_dir`,
  `executable(dest)`, `remove(prefix)`, `artifact(dest)`, `source_of(rel)`,
  `path(rel)`.
- **`check` problems** have a message, a severity (`error` stops the export,
  `warning` is shown) and an optional one-click fix `{"setting", "value",
  "label", "url"}`, which the app shows as a button that sets that plugin setting.
- **Helpers:** `self.media(ctx, build, folder, profile)` applies the advanced
  options, reading the game's own files, so a re-export re-encodes nothing;
  `self.zip(ctx, build, folder, name)` makes a zip that reuses the unchanged
  members of the previous one; `ptk_engine.build.codesign_if_changed(app, ctx.run)`
  signs a macOS app only when it changed.

An **import** plugin subclasses `ImportPlugin`: `detect(ctx)` returns the
recipe (saved as `recipes/<recipe_name>.json`) and `report(ctx, recipe)`
returns `summary`, `title`, `engine_version` and `warnings`.

**API 1** (a class with `run(ctx)` that returns the result dict) still runs.
Validation warns that it is deprecated, and `ptk export --check` reports no
problems for it. Moving to API 2 is usually: the checks at the top of `run`
→ `check`; `clean_dir` + `copy_tree` → `build.add_tree` in `layout`; the rest →
`finish`.

| `ctx.` | |
|---|---|
| `game`, `project`, `output`, `cache`, `temp` | Folders (`Path`); `output` is `<chosen>/<target id>` |
| `kind`, `engine`, `target` | `target` is `{id, name, os, arch, media}` for exports; `media` lists the formats the platform prefers |
| `manifest`, `title` | The project's `ptk.json` (a dict) and the game title |
| `settings`, `setting(k, default)`, `flag(k, default)` | `--set key=value` from the command line or the app |
| `recipe(name)`, `save_recipe(name, dict)` | `recipes/<name>.json` in the project |
| `art("icon")`, `art("poster")` | The project's chosen images (a `Path`), or `None` |
| `progress(fraction, message)`, `log(msg)`, `warn(msg)`, `fail(msg)` | Reporting; `fail` stops with a clear message |
| `write_text`, `write_bytes`, `copy_tree`, `clean_dir`, `check_write` | File helpers that check `[permissions] write` |
| `download(url, sha256=None)`, `remote_zip(url)` | https only, hosts from `[permissions] network`; uses the system `curl` |
| `run(args)`, `which(name)` | Start programs (needs `subprocess = true`) |
| `process_assets(root, profile)` | Applies the project's advanced options to the files under `root` (your copy in `output`) and returns a report for the result's `assets` key; `profile` is the engine's rules (`html5`, `rpgmaker-mv`, `rpgmaker-mz`, `renpy`, `love`, `rgss`, `godot`) |
| `assets_mode` | `apply` (default), `plan` (only estimate: build nothing, return the report) or `off` |
| `tool(id)`, `toolbox` | Path of a compression tool (`ffmpeg`, `cwebp`, `oxipng`, ...): installed first, then a pinned download |
| `zip_dir(src, dest, executables)` | A reproducible zip (sorted, 0755 for programs) that reuses the unchanged members of the previous zip at `dest` |
| `build(root=None, variant="")` | An incremental output folder (`ptk_engine.build.Build`); API 2 plugins get one from the base class |

Result keys: `outputs` (paths), `summary`, `notes`, `warnings`, `open` (the
main file, e.g. the `.app`), `serve` (a folder to serve for web builds),
`assets` (the report of `process_assets`).

### Advanced options in an export plugin

Copy the game into `ctx.output` first, then call
`ctx.process_assets(copy, profile)`: it changes files in place, so it must
never get `ctx.game`. In `plan` mode (`ptk assets`, **Estimate savings**)
return early with `{"summary": ..., "assets": ctx.process_assets(game_root,
profile)}`: plan mode only reads. The pipeline (`engine/ptk_engine/media/`)
keeps file names, checks every result (size, dimensions, transparency,
length, loop tags) and caches encodes in `<data>/cache/media/`. It runs the
compression tools itself, so the plugin needs no `subprocess` permission for
it.
Import plugins may also return `title` (the game's real title) and `icon`
(a cached image of the game's icon, kept per machine).

The plugin runs in its own Python process (`python -m ptk_engine.host`)
with a fresh temporary folder as working directory; anything it prints goes
to the log, not to ptk. Each run is recorded in `plugins.lock`.

## Layout

A plugin is a Python package with a `plugin.toml` next to it:

```
tex-export/
├── plugin.toml
├── pyproject.toml
├── tex_export/__init__.py
└── README.md
```

`ptk plugin new tex-export --kind export` creates this skeleton.

## `plugin.toml`

```toml
schema_version = 1

[plugin]
id = "tex-export"                 # lower-case letters, digits, single dashes
name = "Texture export"
version = "0.1.0"                 # semantic version
kind = "export"                   # "import" or "export"
description = "Recompresses textures to ASTC for Apple GPUs."
license = "MIT"                   # SPDX id
authors = ["you"]
entry = "tex_export:Plugin"       # module:Class
api = 2                           # plugin API version (1 still runs, deprecated)

[formats]
inputs = []                       # import: engine ids it reads (unity, godot, ...)
outputs = []                      # optional: formats it produces
targets = ["macos-arm64"]         # export: target ids it builds for

[permissions]
read = ["$PROJECT", "$GAME"]
write = ["$OUTPUT"]
network = false                   # false, true, or ["host.example.org"]
subprocess = false                # may it start other programs?
```

Folder variables: `$GAME` (the game folder you choose), `$PROJECT` (the
porting project), `$OUTPUT` (the output folder you choose), `$CACHE` (the
plugin's own cache), `$TEMP`. A subfolder such as `$OUTPUT/textures` is fine;
`..` is not. Absolute paths are allowed but flagged.

`ptk plugin validate DIR` checks everything above without importing or
running any plugin code, shows the permissions in plain words, and rates the
risk:

| Risk | When |
|---|---|
| low | only reads folder variables, no network, no programs |
| medium | writes, or talks to specific hosts |
| high | absolute paths, any host, or starting programs |

## Security model (honestly)

A Python plugin is ordinary code. Once it runs, it can do anything your user
account can: read your files, use the network, start programs. Declared
permissions are a promise by the author, not a sandbox. What ptk does:

1. **Shows and asks.** `ptk plugin install` validates the plugin, prints its
   declared permissions and risk, and asks before copying it into place.
   Without a terminal it needs `--yes`.
2. **Separate process.** Plugins run in their own Python process, never
   inside `ptk`, with the working directory set to a scratch folder and only
   the declared folders passed in. The `ctx` helpers refuse writes, hosts and
   programs the plugin did not declare.
3. **Reviewed plugins.** The marketplace marks plugins that the community
   reviewed (`ptk plugin list` has a REVIEWED column). Built-in plugins
   count as reviewed.
4. **Locked versions.** Projects record exact plugin versions (and hashes)
   in `plugins.lock`, so a pack never silently picks up a different plugin.

What it does **not** do: it does not stop a malicious plugin that ignores
its declaration (plain `open()` or `subprocess` calls are not intercepted). Install plugins from people you trust, read the code of
unreviewed ones, and prefer plugins with narrow permissions. Stronger
isolation (macOS sandbox profiles, containers) may come later; until then we
won't call it a sandbox.

## Where plugins live

- Built-in: `engine/plugins/<id>/`.
- Installed: `<data>/plugins/<id>/` (`~/Library/Application Support/PortingToolkit/plugins` on macOS).
- `ptk plugin list` also shows `./plugins/*` in the current folder.
- Caches (templates, compiled wrappers): `<data>/cache/<plugin id>/`.
- Shared by every plugin and managed by the engine: `<data>/cache/tools/`
  (pinned tool downloads, `ptk tools install`) and `<data>/cache/media/`
  (re-encoded assets).

## Targets from SDKs

An export plugin can build for targets that an SDK registers (see
ARCHITECTURE.md, "Extending targets"), e.g. a future `nova-*` target whose
`export_backend` names the plugin.
