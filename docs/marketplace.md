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

## Publishing

1. **Create**: `ptk sdk new …`, `ptk plugin new …`, or pack a project with
   `ptk pack`. In the app: Marketplace → Publish → Create.
2. **Validate & package**: `ptk publish <folder or .ptk>` validates it,
   writes a reproducible zip to `dist/`, computes its SHA-256 and prints
   the index entry.
3. **Submit**: upload the file (a GitHub release of your own repository
   works well), put its URL in `download`, and open a pull request that
   adds the entry to `index.json` in the marketplace repository.

Never publish game files or decompiled game code; see [legal.md](legal.md).
