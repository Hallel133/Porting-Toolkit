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
| `godot-export` | godot | macos-arm64, macos-x64, windows-x64, linux-x64, web | `.pck` + the official Godot export template of the same version |
| `html5-import` | rpgmaker, html5 | | Finds the web root of RPG Maker MV/MZ, NW.js (`package.nw`) and Electron (`app.asar`) games into `recipes/html5.json` |
| `renpy-import` | renpy | | Finds game/ and renpy/ (also in a Mac app), the Ren'Py version, the platform files the build has, and lists `.rpa` archives into `recipes/renpy.json` |
| `renpy-export` | renpy | macos-arm64, macos-x64, windows-x64, linux-x64 | Ren'Py's own distribution layout, with platform files from the game or the SDK of the same version |
| `love-import` | love | | Finds the `.love` (alone, fused into an `.exe`, in a Mac app, or a source folder) and reads `conf.lua` into `recipes/love.json` |
| `love-export` | love | macos-arm64, macos-x64, windows-x64, linux-x64 | The game with the official LÖVE 11.5 build: Mac app, fused `.exe`, or AppImage + `game.love` |
| `rgss-import` | rgss | | Reads RPG Maker XP/VX/VX Ace `Game.ini` and the archive header into `recipes/rgss.json` |
| `rgss-export` | rgss | macos-arm64, macos-x64, windows-x64, linux-x64 | mkxp-z for macOS and Linux (you provide it once), the original game for Windows |
| `html5-export` | rpgmaker, html5 | macos-arm64, macos-x64, windows-x64, linux-x64, web | The web root as a web folder, inside a native WebKit `.app`, or with the NW.js runtime for Windows and Linux |

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

The entry class gets a context and returns a dict:

```python
class Plugin:
    def run(self, ctx):
        ctx.progress(0.1, "Reading the game")
        data = (ctx.game / "data.bin").read_bytes()
        out = ctx.clean_dir(ctx.output)          # only folders ptk made are wiped
        ctx.write_bytes(out / "game.bin", data)
        return {"outputs": [str(out)], "summary": "Built it", "notes": ["…"]}
```

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
| `zip_dir(src, dest, executables)` | A reproducible zip (sorted, 0755 for programs) |

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
api = 1                           # plugin API version

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
