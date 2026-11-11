# Project format

A porting project is a folder; a community project file is the same folder
packed into a `.ptk` zip.

```
my-port/
├── ptk.json        manifest (required)
├── README.md       optional
├── LICENSE*        optional
├── plugins.lock    exact plugin versions
├── recipes/        conversion recipes
├── patches/        community-written patches and code
└── extra/          other community files (checked for third-party content)
```

Only these paths are packed. Anything else in the folder (notes, a local
copy of the game, build output) is ignored by `ptk pack`.

## `ptk.json`

```json
{
  "schema_version": 1,
  "name": "my-port",
  "version": "0.1.0",
  "description": "",
  "authors": ["you"],
  "license": "MIT",
  "game": {
    "title": "My Game",
    "engine": "unity",
    "source_platform": "windows",
    "content_license": null
  },
  "targets": ["macos-arm64"],
  "options": {
    "textures": { "action": "recompress", "quality": 80, "max_resolution": 2048, "format": "astc" },
    "video": { "action": "drop" },
    "audio": { "action": "keep" }
  },
  "third_party": ["extra/vendor/"],
  "source_files": [
    { "path": "MyGame_Data/globalgamemanagers", "size": 123456, "sha256": "…" }
  ]
}
```

| Field | Meaning |
|---|---|
| `schema_version` | Required. `1` today. A newer value is refused with "update Porting Toolkit". |
| `name` | 1–64 letters, digits, `-`, `_`, `.`; starts with a letter or digit. |
| `version` | Version of the community project. |
| `license` | License of the community work (recipes, patches). Default `MIT`. |
| `game.title`, `engine`, `source_platform` | Filled by `ptk scan --record`. |
| `game.content_license` | License of the **game's own files**. Only an open license (MIT, GPL, CC0, …) or `LicenseRef-Own-Work` lets game content be packed. Missing means proprietary. |
| `targets` | Ids from `ptk targets`. Unknown ids are warnings. |
| `options` | Advanced options per asset category (below). |
| `third_party` | Globs of project files that are third-party content. |
| `source_files` | SHA-256 of the user's original game files. Hashes only. |

Unknown fields anywhere are kept when ptk rewrites the file, so newer tools'
data survives older tools.

### Advanced options

Before porting, the user decides what may be lost. Categories:
`textures`, `models`, `audio`, `video`, `shaders`, `scripts`, `data`,
`fonts`, `localization`, `archives`, `executables`, `other`.

| Key | Values |
|---|---|
| `action` | `keep` (default), `drop`, `recompress` |
| `quality` | 1–100 (lossy categories: textures, models, audio, video) |
| `max_resolution` | 16–16384 pixels, longest edge |
| `format` | Preferred output format, e.g. `astc`, `bc7`, `opus` |

`ptk options set CATEGORY …` edits them; `ptk info` shows them. Smart
(AI-assisted) suggestions will build on the same fields.

### `recipes/`

What import plugins learned, as JSON: `recipes/godot.json` (where the
`.pck` is, its Godot version, flags) or `recipes/html5.json` (container,
web root, entry page, window size). Recipes never hold game data and are
packed into `.ptk` files.

### `art/`

The port's icon and poster (`game.icon`, `game.poster` in `ptk.json`, e.g.
`art/poster.png`), set with `ptk art` or the Game page. PNG, JPEG or WebP,
at most 8 MB. They are packed, used by exports and shown in the
marketplace, so use images you made or may share.

### `.ptk-local.json`

Per-machine state, written by `ptk import`, `ptk scan --record` and
`ptk export`: where the game is on this computer and the last output
folder. It is in the project's `.gitignore` and never packed.

### `plugins.lock`

```toml
version = 1

[[plugin]]
id = "godot-export"
version = "1.0.0"
sha256 = "…"      # optional, of the plugin archive
source = "built-in"   # optional: built-in, installed, or a URL
```

## `.ptk` files

A zip archive with `ptk.json` first, the packed files, and `SHA256SUMS`
(`<sha256>  <path>` per line, sorted). Timestamps and permissions are fixed,
so packing the same folder twice gives identical bytes.

`ptk unpack`:

- refuses entries outside the layout above, absolute paths, `..`, links,
  files not listed in `SHA256SUMS`, hash mismatches, and more than 4 GiB;
- extracts into a temporary folder and only then moves it into place, so a
  failure leaves nothing behind;
- never overwrites a non-empty folder.

### Third-party content

`ptk pack` refuses a file when any of these is true, unless
`game.content_license` allows redistribution:

1. it matches a glob in `third_party`;
2. its SHA-256 equals one of the recorded `source_files` (a copied or
   renamed game file);
3. its name or extension is an engine archive (`.rpf`, `.pak`, `.vpk`,
   `.pck`, `.assets`, `data.win`, …).

See [legal.md](legal.md).
