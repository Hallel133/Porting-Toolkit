# Marketplace, SDKs and publishing

The marketplace lists community **plugins** (importers and exporters),
**SDKs** (new platforms plus their exporters) and **projects** (`.ptk`).
It is a plain JSON index in a GitHub repository, so anyone can browse it,
and publishing is a pull request.

- Repository: `https://github.com/Hallel133/ptk-marketplace`
- Index: `index.json` on its `main` branch
- `PTK_MARKETPLACE_URL` points Porting Toolkit at another index (an
  `https://` or `file://` URL), for testing or a private catalog.

## Browsing and installing

In the app: **Marketplace → Browse**, then **Install…**. From the command line:

```bash
ptk market list --search unity
ptk market install unity-import
```

Every download is checked against the SHA-256 in the index; a mismatch is
refused. Plugins and SDKs are then validated, their declared permissions are
shown, and nothing is installed without your consent. Reviewed entries carry
a ✔ (see [plugins.md](plugins.md) for what review does and does not mean).

## Game ports

A game port is a project (`.ptk`): recipes, settings, community patches and
the SHA-256 hashes of the game files it was made for. **It never contains the
game.** Players need their own copy; after **Get port** the app unpacks the
project and asks them to scan their copy, which is checked against the
hashes. A port of a paid game therefore does not make the game free: link
where to buy it in `store`.

Port entries have three extra fields:

| Field | Meaning |
|---|---|
| `game` | The game's title |
| `poster` | https URL of the poster image (upload the one `ptk publish` writes next to your `.ptk`) |
| `store` | https URL of a page where players get the game |
| `fingerprint` | A few of the game's largest files (`path`, `size`, `sha256`), written by `ptk publish` from the project's recorded hashes |

**Check my copy** (on the port's card, or `ptk market check <id> <game
folder>`) compares the fingerprint with a player's game before they download
anything: "Matches your copy", "Another version of the game" (some files
differ), "Different files" or "Game files not found". Each file is looked up
at its path, then by name and size anywhere in the folder, so picking a
parent folder works. Only those few files are hashed.

Give the project a poster and icon first: `ptk art --poster cover.png
--icon icon.png -p my-port` (or the Icon & poster card on the Game page).
Use images you made or may share. For now the marketplace asks for no
payment and no identity checks; reviewed entries are marked by maintainers.

## Index format

```json
{
  "schema_version": 1,
  "entries": [
    {
      "id": "nova",
      "name": "NoVA SDK",
      "kind": "sdk",
      "version": "0.1.0",
      "description": "Adds NoVA boards and their exporter.",
      "author": "Hallel133",
      "license": "MIT",
      "homepage": "https://github.com/Hallel133/nova-sdk",
      "download": "https://github.com/Hallel133/nova-sdk/releases/download/v0.1.0/nova-0.1.0.zip",
      "sha256": "…64 hex characters…",
      "reviewed": false,
      "engines": [],
      "targets": ["nova-device"]
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `kind` | `plugin`, `sdk` or `project` |
| `download` | `https://` URL of the zip (plugins, SDKs) or `.ptk` (projects) |
| `sha256` | SHA-256 of that file |
| `reviewed` | set by maintainers when they merge after review |
| `engines` | engines an import plugin reads, or a project ports |
| `targets` | targets an export plugin or SDK builds for |

Broken entries (bad id, bad hash, non-https download) are skipped.

## SDKs

An SDK teaches Porting Toolkit about new platforms:

```
my-sdk/
├── ptk-sdk.toml          [sdk] id, name, version, license, description
├── targets/my.toml       [[target]] id = "my-device", export_backend = "my-export", …
└── plugins/my-export/    the export plugin for those targets
```

```bash
ptk sdk new my-sdk        # skeleton: one target and its export plugin
ptk sdk validate my-sdk   # targets load, plugins validate, ids do not clash
ptk sdk install my-sdk    # copies targets into <data>/targets, installs plugins
```

Target ids should start with the SDK id (`nova-…` for the NoVA SDK).
Installing an SDK adds its targets to `ptk targets` and to the app.

## Updates

`ptk market outdated` lists installed plugins and SDKs, and port projects in
your projects folder (`--projects DIR`), whose index entry has a newer
version. The app shows the same as a banner and an "⬆ v1 → v2" badge on each
card, whose button turns into **Update**. Updating a plugin or SDK installs
the new version over the old one (with the usual permission check); a port
update arrives as a new project folder, so your own changes are kept.

## Publishing

1. **Create**: `ptk sdk new …`, `ptk plugin new …`, or pack a project with
   `ptk pack`. In the app: Marketplace → Publish → Create.
2. **Validate & package**: `ptk publish <folder or .ptk>` validates it,
   writes a reproducible zip to `dist/`, computes its SHA-256 and prints
   the index entry.
3. **Submit**: `ptk market submit <folder or .ptk> [--store URL]` (or
   Marketplace → Publish → **Publish to the marketplace…**) does the rest
   with the GitHub CLI (`gh auth login` first): it shows what will be made
   public, then uploads the file (and a port's poster) to a GitHub release,
   writes the entry with those URLs to `index.json` on a new branch, and
   opens a pull request. The marketplace's owner publishes in the
   marketplace repository itself; everyone else through a fork that `gh`
   creates. `--dry-run` shows the plan and changes nothing. A game port
   needs `--store`, a page where players get the game.
   You can still do it by hand: upload the file, put its URL in
   `download`, and open a pull request adding the entry to `index.json`.

Never publish game files or decompiled game code; see [legal.md](legal.md).
