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
| `godot-export` | godot | macos-arm64, macos-x64, web | `.pck` + the official Godot export template of the same version |
| `html5-import` | rpgmaker, html5 | | Finds the web root of RPG Maker MV/MZ, NW.js (`package.nw`) and Electron (`app.asar`) games into `recipes/html5.json` |
| `html5-export` | rpgmaker, html5 | macos-arm64, macos-x64, web | The web root as a web folder, or inside a native WebKit `.app` |

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
| `kind`, `engine`, `target` | `target` is `{id, name, os, arch}` for exports |
| `manifest`, `title` | The project's `ptk.json` (a dict) and the game title |
| `settings`, `setting(k, default)`, `flag(k, default)` | `--set key=value` from the command line or the app |
| `recipe(name)`, `save_recipe(name, dict)` | `recipes/<name>.json` in the project |
| `progress(fraction, message)`, `log(msg)`, `warn(msg)`, `fail(msg)` | Reporting; `fail` stops with a clear message |
| `write_text`, `write_bytes`, `copy_tree`, `clean_dir`, `check_write` | File helpers that check `[permissions] write` |
| `download(url, sha256=None)`, `remote_zip(url)` | https only, hosts from `[permissions] network`; uses the system `curl` |
| `run(args)`, `which(name)` | Start programs (needs `subprocess = true`) |

Result keys: `outputs` (paths), `summary`, `notes`, `warnings`, `open` (the
main file, e.g. the `.app`), `serve` (a folder to serve for web builds).

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

## Targets from SDKs

An export plugin can build for targets that an SDK registers (see
ARCHITECTURE.md, "Extending targets"), e.g. a future `nova-*` target whose
`export_backend` names the plugin.
