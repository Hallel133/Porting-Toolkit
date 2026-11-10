# CLAUDE.md: Porting Toolkit handoff notes

Read this first in every new session. The owner (Hallel133) writes in Hebrew: **reply in Hebrew**; keep code, commits, CLI/UI text and docs in English (README also has a Hebrew section).

## What this repo is
A community tool to manage, convert, export and import game projects between platforms. macOS first. Not part of Grand-V (it lives next to it, in `~/porting-toolkit`).

- Rust workspace: `crates/ptk-core` (manifest, targets, `.ptk` pack/unpack, verify, hashing, engine RPC client, plus shared operations `scan`/`project`/`doctor`/`plugins`/`banner`), `crates/ptk-cli` (binary `ptk`, clap), `crates/ptk-gui` (binary `ptk-gui`, egui 0.33 desktop app).
- Python engine: `engine/ptk_engine` (uv + pytest, standard library only, Python 3.11+). Detection, plugins; Rust runs it as a subprocess over JSON-RPC on stdin/stdout.
- Docs: `README.md`, `ARCHITECTURE.md` (protocol, layout), `docs/project-format.md`, `docs/plugins.md`, `docs/legal.md`.
- Fixtures: `fixtures/games/*` are fake game folders of **empty files** with the right names.

## Non-negotiable rules
- A tool, not a distribution channel. `.ptk` files contain recipes, settings, community patches and SHA-256 hashes, **never** game files or decompiled code. `ptk pack` refuses third-party files (manifest `third_party` globs, copies of recorded game files by hash, engine archives) unless `game.content_license` is redistributable (open license or `LicenseRef-Own-Work`). Never weaken this.
- `ptk scan` uses names and 4-byte executable headers only; never decode archives.
- NoVA (owner's RISC-V SoC) is out of scope until the owner's NoVA SDK exists. It plugs in via `[[target]]` TOML in `<data>/targets/` with `export_backend` = an export plugin id. Keep that path working.
- `LICENSE` is MIT (owner's choice, 2026-11-09).

## Git / release state (2026-11-09)
- Remote `origin` = https://github.com/Hallel133/Porting-Toolkit (public; the owner created it with a one-line README). Work on `main`.
- Phase 1 (commit 4c21302) was pushed to `origin/main` on 2026-11-09; pushing works despite the account flag.
- The owner said their GitHub account is "flagged" (Grand-V notes: GitHub Actions is disabled for the Hallel133 account, `HTTP 422`). So `.github/workflows/ci.yml` (macos-14 + ubuntu: fmt, clippy -D warnings, cargo test, uv run pytest) has probably never run. Run the same commands locally.

## Commands
```bash
export PATH="$HOME/.cargo/bin:$PATH"          # cargo is not on the default PATH on the owner's Mac
cargo test --workspace                        # 64 Rust tests, starts the real Python engine
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
(cd engine && uv run pytest)                  # 38 Python tests
cargo run -q -p ptk-cli -- scan fixtures/games/unity-windows
python3 tools/gen_banner.py                   # regenerate banner art (then update ptk-core/src/banner.rs + ptk-cli/tests/snapshots/banner.txt)
cargo run -p ptk-gui                          # the desktop app
cargo run -p ptk-gui -- --view scan --project P --game G --run --screenshot out.png   # screenshot after the job, then exits
```
- Tests set `PTK_HOME` to a temp folder; never let tests touch the real data folder.
- `PTK_PYTHON` picks the interpreter (CI sets it to `engine/.venv/bin/python`); otherwise the managed env, then `python3` + `PYTHONPATH=engine`.
- Data folder on macOS: `~/Library/Application Support/PortingToolkit/` (`engine-env/`, `plugins/`, `targets/`).

## Done (phase 1, 2026-11-09)
- `ptk`, `about`, `init`, `info`, `options [set]`, `targets`, `doctor [--no-install]`, `scan [--json] [--record P]`, `pack`, `unpack`, `verify`, `plugin list|new|validate|install`.
- Original FIGlet-style banner (`tools/gen_banner.py`), 256-color gradient; color off for pipes, `NO_COLOR`, `--no-color`, `TERM=dumb`; `FORCE_COLOR` forces it (used for screenshots).
- Detection: Unity (Mono/IL2CPP), Unreal (IoStore), Godot, RAGE, Source, GameMaker, Ren'Py, RPG Maker (MV/MZ/VX/VX Ace/XP), macOS app/Mach-O.
- Screenshots: `docs/images/banner.png`, `docs/images/scan.png` (scan of a fake sparse-file Unity game, not a real game).
- `ptk doctor` was run for real on the owner's Mac; it created the managed env with uv.

## Done (phase 2, 2026-11-09)
- Owner feedback round 1: "VS Code colors, dark blue". Round 2 (same day): **only the colors**, not VS Code's layout, and **a native macOS menu bar**. Current design: header (logo, project switcher, navigation pills, Targets/Doctor), centred responsive pages of cards, same dark navy + editor accent palette. No activity bar, side bar or status bar; do not bring them back.
- Native menu bar (`menu.rs`, `muda` 0.21, macOS only): Porting Toolkit (native About panel, Hide, Quit), File (New/Open/Open Recent/Save/Revert/Pack/Unpack/Close), Edit, View (⌘1–⌘7, full screen), Tools (scan, verify, doctor, engine setup, show folders), Window, Help (links to the docs). **Not verified visually by Claude**: macOS denied accessibility and screen capture to the session, so the owner must check the bar.
- Pages: Home, Project (details, game, targets, workflow stepper, problems, folder), Scan (stacked category bar), Options (segmented Keep/Drop/Recompress per category, presets), Share (pack/unpack/verify), Plugins, Targets, Doctor, About. Drag-and-drop of project folders, `.ptk` files and game folders.
- Shared logic lives in `ptk-core` (`scan`, `project`, `doctor`, `plugins`, `banner`, `error`); the CLI output is unchanged.
- GUI screenshots in `docs/images/gui-*.png`. Do not commit screenshots that show local paths (user name).

## Done (2026-11-10, owner feedback round 3)
Owner asked for a Godot-Project-Manager-inspired GUI (same dark blue colors; do not copy Godot's logo or art), how to export / import EXE, .app and .deb without source, and a clearer, more inviting marketplace with a way to upload an SDK.
- Core/CLI: `import` (folder, .app, executable, .zip, .deb parsed in Rust, .dmg), `routes` (per engine: portable without source or not), `sdk` (ptk-sdk.toml + targets + plugins), `market` (JSON index, SHA-256 downloads, `package` for publishing). CLI `ptk import`, `ptk sdk`, `ptk market`, `ptk publish`; `ptk scan` accepts archives/exe and prints the porting route. Engine detects a Godot .pck appended to an exe (`GDPC` at the end) and scans single files.
- GUI: tab bar (Projects, Marketplace, the open project's Overview/Scan/Options/Export, Settings) + status bar. Projects = project manager (Create, Import Game, Open, Find, filter, sort, favorites, missing projects, action column). Import Game modal (default project folder `~/Porting Toolkit/<name>-port`). Export page shows the porting route, exporters per target and a "without source code?" table, plus Pack/Unpack/Verify. Marketplace: Browse (index from GitHub, inviting header), Installed (plugins, SDKs, check a plugin), Publish (create SDK/plugin → validate & package → submit by PR). Settings: Environment (doctor), Targets, About. Install confirmation modal for plugins and SDKs.
- Menus: File gains Import Game (⌘I); View = ⌘1 Projects … ⌘6 Export; app menu has Settings… (⌘,); Tools gains Find Projects, Publish, New SDK.
- docs/marketplace.md written.
- **The marketplace repository `Hallel133/ptk-marketplace` does not exist yet**, so Browse shows "could not be loaded". The owner must create it (with an `index.json` of `{"schema_version": 1, "entries": []}`) or let Claude do it with gh.

## Next task
1. Owner: try `cargo run --release -p ptk-gui` (Projects, Import Game, Marketplace), check the macOS menu bar, create the ptk-marketplace repo.
2. Phase 3: first exporter (Godot: .pck + export template), plugin runtime (separate process, restricted cwd, declared folders only), recipe format, marketplace with reviewed flag, AI-assisted advanced options. Phase 4: Windows/Linux.
3. Not tested: Linux/Windows builds (CI adds `libgtk-3-dev` etc. for rfd/egui but Actions is blocked on the account), app bundling (`.app`, icon), real games (only fixtures and a sparse fake).

## Working agreement (after every change)
1. Run the tests (Rust and Python); for CLI output changes, take screenshots and show them.
2. Commit with a clear message and push to `main`.
3. Update "Git / release state" and "Next task" here.
4. Report in Hebrew: what changed, what was verified, what the owner must do, what is untested.
