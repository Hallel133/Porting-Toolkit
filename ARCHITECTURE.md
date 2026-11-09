# Architecture

Porting Toolkit has two halves:

- **Rust** for the application: the `ptk` CLI and the `ptk-gui` desktop app. It owns
  the project format, packing, hashing, the target registry and everything
  the user touches.
- **Python** for the algorithms: engine detection, import/export pipelines
  and plugins. It runs as a child process.

```
┌──────────────────────────┐   JSON-RPC 2.0, one JSON per line   ┌───────────────────────┐
│ ptk (crates/ptk-cli)     │ ─────────── stdin ────────────────▶ │ python -m ptk_engine  │
│   └ ptk-core             │ ◀────────── stdout ──────────────── │   rpc                 │
│     manifest, targets,   │       stderr: passed through        │ scan, signatures,     │
│     pack, verify, hash,  │                                     │ categories, plugins   │
│     engine client        │                                     │ (engine/ptk_engine)   │
└──────────────────────────┘                                     └───────────────────────┘
```

## Workspace

| Path | What |
|---|---|
| `crates/ptk-core` | Library shared by CLI and GUI |
| `crates/ptk-core/src/manifest.rs` | `ptk.json` model, schema version, validation, licenses |
| `crates/ptk-core/src/targets.rs` | Target registry: built-ins + `[[target]]` TOML files |
| `crates/ptk-core/src/pack.rs` | `.ptk` pack/unpack, third-party rules, `plugins.lock` |
| `crates/ptk-core/src/verify.rs` | Compare a game folder with recorded hashes |
| `crates/ptk-core/src/hash.rs` | SHA-256, tree hashing, OS clutter filter |
| `crates/ptk-core/src/category.rs` | Asset categories (shared ids with Python) |
| `crates/ptk-core/src/engine.rs` | Locating and talking to the Python engine |
| `crates/ptk-core/src/paths.rs` | Per-user data folder (`PTK_HOME` overrides) |
| `crates/ptk-cli` | `ptk` binary (clap), banner, colors, tables |
| `crates/ptk-core/src/{scan,project,doctor,plugins}.rs` | Operations shared by CLI and GUI (scan + record, init, environment checks, plugin list/validate/create/install) |
| `crates/ptk-core/src/banner.rs` | The wordmark art, shared by the CLI banner and the GUI |
| `crates/ptk-gui` | Desktop app (egui 0.33 + eframe/glow, rfd dialogs) |
| `engine/ptk_engine` | Python engine (standard library only, 3.11+) |
| `fixtures/games` | Fake game folders (empty files with the right names) |
| `tools/gen_banner.py` | Generator of the banner art |

## The engine protocol

- Transport: the engine's stdin/stdout, UTF-8, one JSON-RPC 2.0 message per
  line. stderr carries diagnostics. While a request runs, Python's
  `sys.stdout` points at stderr, so a stray `print` cannot corrupt the stream.
- Handshake: `ptk` calls `engine.info` first and refuses an engine whose
  `protocol` differs from `PROTOCOL_VERSION` (Rust `engine.rs`, Python
  `__init__.py`). Bump both on breaking changes.
- Notifications (no `id`) may arrive before a response, e.g.
  `progress {"phase": "scan", "files": 2000}`.
- Errors: standard codes (-32700 parse, -32600 request, -32601 method,
  -32602 params, -32603 internal) and -32000 for expected failures such as a
  missing folder. The engine keeps serving after an error.

| Method | Params | Result |
|---|---|---|
| `engine.info` | – | name, version, protocol, python, categories, engines, methods |
| `scan` | `path`, `max_files?` | counts, sizes, `engines[]`, `primary`, `platforms`, `categories[]`, `largest[]` |
| `plugin.validate` | `path`, `known_targets?` | `valid`, `errors`, `warnings`, `plugin`, `permissions`, `risk` |
| `plugin.list` | `dirs`, `known_targets?` | summaries of plugin folders |
| `plugin.new` | `path`, `kind`, `id?` | created files |
| `shutdown` | – | `{}`, then the engine exits |

### Finding the engine

Interpreter: `PTK_PYTHON` → the managed environment (`<data>/engine-env`,
created by `ptk doctor` with `uv venv` + `uv pip install -e engine/`) →
`python3` on `PATH`. The engine source comes from `PTK_ENGINE_DIR`, a folder
next to the executable (`engine/` or `../share/porting-toolkit/engine`), or
this checkout; it is put on `PYTHONPATH`. The engine needs only the standard
library, so any Python 3.11+ works.

Data folder: `~/Library/Application Support/PortingToolkit` (macOS),
`$XDG_DATA_HOME/porting-toolkit` (Linux), `%APPDATA%\PortingToolkit`
(Windows). Inside: `engine-env/`, `plugins/`, `targets/`.

## Extending targets (NoVA and others)

The target registry is open. An SDK adds platforms by dropping a TOML file
into `<data>/targets/`:

```toml
[[target]]
id = "nova-example"          # lower-case, digits, single dashes
name = "NoVA example board"
os = "nova"
arch = "riscv64"
status = "experimental"      # active | experimental | planned
export_backend = "nova-export"   # id of the export plugin that builds it
```

Projects can then list `nova-example` in `targets`, and an export plugin
declaring `targets = ["nova-example"]` validates without warnings. The NoVA
SDK is the owner's separate project and is not part of phase 1.

## Shared ids

- Asset categories: `category.rs` and `categories.py`. The integration test
  `engine_rpc.rs` compares them through `engine.info`.
- Engine ids: `signatures.py` (`ENGINE_IDS`). The manifest stores the
  detected id in `game.engine`.

## Tests

| Where | What |
|---|---|
| `ptk-core` unit tests | manifest round-trip, schema rules, licenses, globs, targets, hashing |
| `ptk-core/tests/pack.rs` | pack/unpack/verify, third-party refusal, hostile zips |
| `ptk-core/tests/engine_rpc.rs` | Rust ↔ Python over a real subprocess |
| `ptk-cli` unit tests | banner snapshot, color decision, tables |
| `ptk-cli/tests/cli.rs` | the `ptk` binary end to end (isolated `PTK_HOME`) |
| `engine/tests` | detection on fixtures, plugin validation, RPC server |

## Desktop GUI (`crates/ptk-gui`)

- `theme.rs`: deep navy surfaces with code-editor accent colors (blue
  #007ACC / #3794FF, teal, soft yellow, orange, green, red). On macOS the
  system UI font is loaded from `/System/Library/Fonts` at runtime (never
  bundled); `PTK_GUI_FONTS=builtin` uses egui's fonts.
- Layout (`app.rs`): a header with the logo, the project switcher (new,
  open, recent, close), navigation pills (labels collapse to icons in narrow
  windows) and Targets/Doctor buttons; a busy pill shows background work.
  Pages (`views.rs`) are centred columns of cards that reflow: two or three
  columns when there is room, one when narrow.
- `menu.rs`: every action is a `Command`, listed once in `menus()`. On
  macOS the native menu bar is built with `muda` (app menu with the native
  About panel, File, Edit, View, Tools, Window, Help, plus Open Recent) and
  items are enabled from `MenuState`. Menu clicks arrive on a channel and
  wake the UI. Edit items are turned into egui events (copy/cut/paste/undo/
  select all), so they act on the focused text field. Other systems get the
  same commands from a ☰ button in the header and egui shortcuts.
- Background jobs: engine calls, hashing, packing and verifying run on a
  thread and send a `Msg` back; one job at a time (`busy`). Recent projects
  and games live in `<data>/gui-state.json`.
- Edits go to an in-memory `Manifest`; `saved` holds the on-disk copy, so an
  "Unsaved changes" bar and a dot on the project chip mark edits. Save (⌘S)
  validates first and refuses invalid values.
- `ptk-gui --view scan --project P --game G --run --screenshot out.png
  [--size WxH]` opens a page, runs its job, saves a screenshot when done and
  closes. `--smoke` opens the window for three seconds.
- Tests (`app.rs`, `menu.rs`) render every page headlessly through
  `egui::Context::run`, drive the real jobs, and check command ids,
  shortcuts and enabled states.
