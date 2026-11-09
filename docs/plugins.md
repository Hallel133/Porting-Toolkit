# Plugins

Import and export plugins do the actual conversion work. **Phase 1 defines
and validates the format only; running plugins arrives in phase 3.**

- An **import** plugin reads a game (an engine or format) into a project.
- An **export** plugin builds a project for one or more targets.

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
2. **Separate process.** Plugins will run in their own Python process, never
   inside `ptk`, with the working directory set to a scratch folder and only
   the declared folders passed in.
3. **Reviewed plugins.** The marketplace (phase 3) marks plugins that the
   community reviewed. `ptk plugin list` already has a REVIEWED column; in
   phase 1 nothing is reviewed.
4. **Locked versions.** Projects record exact plugin versions (and hashes)
   in `plugins.lock`, so a pack never silently picks up a different plugin.

What it does **not** do: it does not stop a malicious plugin that ignores
its declaration. Install plugins from people you trust, read the code of
unreviewed ones, and prefer plugins with narrow permissions. Stronger
isolation (macOS sandbox profiles, containers) may come later; until then we
won't call it a sandbox.

## Where plugins live

- Installed: `<data>/plugins/<id>/` (`~/Library/Application Support/PortingToolkit/plugins` on macOS).
- `ptk plugin list` also shows `./plugins/*` in the current folder.

## Targets from SDKs

An export plugin can build for targets that an SDK registers (see
ARCHITECTURE.md, "Extending targets"), e.g. a future `nova-*` target whose
`export_backend` names the plugin.
