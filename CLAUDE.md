# CLAUDE.md: Porting Toolkit handoff notes

Read this first in every new session. The owner (Hallel133) writes in Hebrew: **reply in Hebrew**; keep code, commits, CLI/UI text and docs in English (README also has a Hebrew section).

## What this repo is
A community tool to manage, convert, export and import game projects between platforms. macOS first. Not part of Grand-V (it lives next to it, in `~/porting-toolkit`).

- Rust workspace: `crates/ptk-core` (manifest, targets, `.ptk` pack/unpack, verify, hashing, engine RPC client, plus shared operations `scan`/`project`/`doctor`/`plugins`/`banner`), `crates/ptk-cli` (binary `ptk`, clap), `crates/ptk-gui` (binary `ptk-gui`, egui 0.33 desktop app).
- Python engine: `engine/ptk_engine` (uv + pytest, standard library only, Python 3.11+). Detection, plugin validation and the plugin runtime (`runtime.py` → `host.py`, API in `api.py`, format readers in `formats/`); Rust runs it as a subprocess over JSON-RPC on stdin/stdout.
- Built-in plugins: `engine/plugins/{godot,html5}-{import,export}` (macOS + web targets).
- Docs: `README.md`, `ARCHITECTURE.md` (protocol, layout), `docs/project-format.md`, `docs/plugins.md`, `docs/legal.md`.
- Fixtures: `fixtures/games/*` are fake game folders of **empty files** with the right names.

## Non-negotiable rules
- A tool, not a distribution channel. `.ptk` files contain recipes, settings, community patches and SHA-256 hashes, **never** game files or decompiled code. `ptk pack` refuses third-party files (manifest `third_party` globs, copies of recorded game files by hash, engine archives) unless `game.content_license` is redistributable (open license or `LicenseRef-Own-Work`). Never weaken this.
- `ptk scan` uses names and 4-byte executable headers only; never decode archives.
- NoVA (owner's RISC-V SoC) is out of scope until the owner's NoVA SDK exists. It plugs in via `[[target]]` TOML in `<data>/targets/` with `export_backend` = an export plugin id. Keep that path working.
- `LICENSE` is MIT (owner's choice, 2026-11-09).

## Git / release state (2026-11-24)
- Remote `origin` = https://github.com/Hallel133/Porting-Toolkit (public; the owner created it with a one-line README). Work on `main`.
- Phase 1 (commit 4c21302) was pushed to `origin/main` on 2026-11-09; pushing works despite the account flag.
- The owner said their GitHub account is "flagged" (Grand-V notes: GitHub Actions is disabled for the Hallel133 account, `HTTP 422`). So `.github/workflows/ci.yml` (macos-14 + ubuntu: fmt, clippy -D warnings, cargo test, uv run pytest) has probably never run. Run the same commands locally.

## Commands
```bash
export PATH="$HOME/.cargo/bin:$PATH"          # cargo is not on the default PATH on the owner's Mac
cargo test --workspace                        # 78 Rust tests, starts the real Python engine
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
(cd engine && uv run pytest)                  # 163 Python tests (+1 skipped; compiles the Swift wrapper on macOS; builds a real APK when the Android SDK is installed)
(cd engine && PTK_TOOL_TESTS=1 uv run pytest -k real)       # also runs real FFmpeg encodes
(cd engine && PTK_GUI_TESTS=1 uv run pytest -k macos_app)   # also opens the wrapper app and checks the page loaded
cargo run -q -p ptk-cli -- scan fixtures/games/unity-windows
python3 tools/gen_banner.py                   # regenerate banner art (then update ptk-core/src/banner.rs + ptk-cli/tests/snapshots/banner.txt)
cargo run -p ptk-gui                          # the desktop app
cargo run -p ptk-gui -- --view scan --project P --game G --run --screenshot out.png   # screenshot after the job, then exits
# add --scroll-bottom to capture the end of a long page (the window cannot be taller than the screen)
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
- The marketplace repo content is ready locally in `~/ptk-marketplace` (index.json with no entries, README with the publish flow, PR template, MIT license; one commit on `main`). Creating the public GitHub repo from Claude's session was blocked by the permission system, so **the owner must run** `cd ~/ptk-marketplace && gh repo create Hallel133/ptk-marketplace --public --source . --remote origin --push`. Until then Browse shows "could not be loaded" (404).

## Done (2026-11-10, phase 3 start: export/import plugins for macOS and web)
Owner asked: "add the export and import plugins for Mac and Web".
- Plugin runtime: `plugin.find` / `plugin.run` RPC; each plugin runs in `python -m ptk_engine.host` (own process, temp cwd, stdout kept clean). `ctx` helpers enforce declared write folders, network hosts (https only, system curl) and `subprocess`. `ctx.clean_dir` only wipes folders with a `.ptk-output` marker.
- Godot: `.pck` reader for formats 1/2/3 (Godot 3, 4.0–4.3, 4.4+), embedded pck extraction (v1 absolute offsets rewritten). Export: macOS `.app` (template, Info.plist, lipo thin, ad-hoc codesign), web (nothreads template by default, `threads=true` adds the service worker). Templates: `--set templates=`, then the Godot editor folder, then cache, then HTTP-range download of only the needed `.tpz` members from GitHub (4.7.2 tpz is 1.28 GB; macos.zip 123 MB, web 10 MB). Refuses encrypted pcks and C#.
- HTML5: RPG Maker MV/MZ, NW.js (folder or package.nw), Electron (app.asar reader). New engine signature `html5`. Export: web folder; macOS `.app` with `wrapper.swift` (WKWebView + `app://` scheme handler with ranges, menus, full screen, icon via sips/iconutil, ad-hoc sign), compiled once per arch into `<data>/cache/html5-export/wrapper/`.
- Core `convert.rs` (find/run plugins, `plugins.lock`, `.ptk-local.json` remembers the game per machine), `serve.rs` (127.0.0.1 static server, COOP/COEP, ranges). Web target is now Active. Titles drop platform suffixes (`Game-windows.zip` → `Game`).
- CLI: `ptk export [-p P] [-t ID]… [-o DIR] [--set k=v] [--plugin ID] [--open]`, `ptk serve DIR [--open]`; `ptk import` runs the import plugin.
- GUI: Export page has Export per target / Export all, output folder (shown with `~`), plugin settings, a Builds card (Open, Play in browser via the built-in server, Show in Finder); status bar shows plugin progress. Import Game runs the import plugin. `--view export --run` exports (screenshot `docs/images/gui-export.png`).
- Verified on the owner's Mac: the real Godot 4.7.2 web template booted a test pck in the browser pane (label "Ported with Porting Toolkit (Web)"); the macOS Godot app was built, thinned, signed and passes `codesign --verify --strict`, but **was not launched** (running the downloaded Godot binary was blocked for the session). The HTML5 wrapper app was launched in self-test mode and loaded its page (`PTK_WRAPPER_LOADED loaded:Quest`).

## Done (2026-11-11, owner feedback on the first exports)
Owner exported Buckshot Roulette (Godot 4.6.3, a macOS `.app` with GodotSteam) to Mac and web: Mac worked except a gray screen; web had effects stuck on screen. Asked: advanced options on the game's page (and app-level options such as default algorithms), an icon and poster per game, and game ports in the marketplace (paid games must not become free; no payments or identity checks for now).
- Cause of the gray screen: the export did not copy `Contents/Frameworks` (libgodotsteam), so the `GlobalSteam` autoload failed. Fixed: Godot export copies the source app's Frameworks (or matching libraries in the game folder) and warns when a macOS library is missing; keeps the source app's icon. Re-exported the owner's game into the scratchpad: Frameworks present, original icon, `codesign --verify --strict` passes. **Not launched by Claude** (owner must check the gray screen is gone).
- Web: the game uses Forward+ (renderer unset); browsers only have Compatibility. Now a clear warning, plus a GodotSteam "no web version" warning; the loading screen uses the game's boot splash + bg color and the page gets the game icon.
- `formats/godot.py` reads `project.binary` (ECFG) and `.gdextension` library lists; godot-import records title/renderer/icon/autoloads and caches the game icon (per machine, `.ptk-local.json` `game_icon`); `convert::import` replaces a file-derived title with the real one.
- GUI: the Options tab is gone (⌘1–⌘5 now). The Game page (formerly Overview) has Game, Targets, Advanced options (compact editor with format/algorithm per category), folded Project details, and an Icon & poster card. Settings → Defaults (first tab) edits `<data>/settings.json` (`ptk_core::settings::AppSettings`: default targets, default options, projects folder); `InitOptions::with_defaults` applies it in `ptk init`, `ptk import` and the app.
- Art: `ptk_core::art` (`art/icon.*`, `art/poster.*`, ≤ 8 MB, in `ptk.json` `game.icon/poster`), `art/` is now a pack dir, `ptk art` CLI, exports use it (`ctx.art`). Project rows show the icon.
- Marketplace: kind `project` is labelled "Game port"; entries gain `game`, `poster`, `store`. Browse shows ports first as poster cards (posters cached in `<data>/cache/posters`), "Get port" unpacks into the projects folder, opens it and asks for the user's own copy (Scan). `ptk publish` of a `.ptk` writes the poster next to it and placeholders for `poster`/`store`. `~/ptk-marketplace` README updated (local commit c19825d; still no GitHub repo).

## Done (2026-11-12, plan "algorithms, new platforms, more importers", phase 1)
Owner asked to plan and design new compression algorithms for new platforms and to widen import. Owner chose: targets Windows x64 + Linux x64 and Android APK; importers Ren'Py, LÖVE, RPG Maker XP/VX/Ace (mkxp-z) and more HTML5 engines; tools "installed first, then a pinned download checked by SHA-256". The full plan (5 phases) is in `~/.claude/plans/groovy-popping-summit.md`; phase 1 is done:
- `Target.media` (`MediaCaps {textures, audio, video}`, preference order; `[target.media]` in target TOML files). `manifest.validate`: an unknown `format` is an error; a GPU texture format (astc/bc7/etc2, `manifest::GPU_TEXTURES`) none of the targets list is a warning.
- `engine/ptk_engine/tools.py`: registry (ffmpeg, ffprobe, cwebp, oxipng, pngquant, oggenc, afconvert), lookup `--set tool.<id>=PATH` → PATH → /opt/homebrew/bin, /usr/local/bin → `<data>/cache/tools/` → pinned download (only oxipng is pinned, from GitHub's release digests; hosts allowlisted in tools.py). RPCs `tools.status`, `tools.install`; `ptk tools [list|install ID]`; doctor lists the tools (GUI Settings → Environment has Install buttons).
- `engine/ptk_engine/media/`: encoders (png: oxipng / pngquant / built-in re-deflate that always works; jpeg, vorbis (ffmpeg libvorbis or oggenc), opus, aac (ffmpeg or afconvert), mp3, h264/hevc/vp9/av1), profiles per engine (loose-file engines keep names and formats and are never resized; MZ and LÖVE keep `.ogg` Vorbis; Ren'Py follows the target; Godot is "kept"), checks (≥3% smaller, same size/alpha, same duration/channels/LOOPSTART tags), content cache `<data>/cache/media/` with `.skip` sentinels, `plan` mode (largest 20 files per category, extrapolated).
- `ctx.process_assets(root, profile)`, `ctx.assets_mode`, `ctx.tool`, `ctx.zip_dir`. html5-export applies the options to its copy (web and macOS, before signing); godot-export reports "kept". `RunResult.assets` (`AssetReport`), `ExportOptions.assets` (`AssetsMode`). CLI `ptk export --assets apply|plan|off`, `ptk assets`. GUI: Estimate savings button, assets table in the Builds card, "Automatic: …" lines and greyed GPU formats in the options editor, preset "Smaller videos" replaces "Drop videos".
- **Homebrew's FFmpeg 8 on the owner's Mac has no libvorbis**: Vorbis needs `brew install vorbis-tools` (oggenc), otherwise `.ogg` files are kept with a note. This matters for RPG Maker MZ.
- Verified: synthetic MZ project via the CLI (title.png 592 KB → 125 KB; game folder unchanged), real FFmpeg Opus encode keeps LOOPSTART, GUI screenshots (scratchpad only: they show the scratchpad path, so docs images were not replaced).

## Done (2026-11-24, plan phase 2: Windows x64 and Linux x64)
- `windows-x64` and `linux-x64` are Active targets. Both built-in exporters build them (plugin versions 1.1.0).
- godot-export: release templates (verified names in the real 4.4.1 and 3.6 .tpz: `windows_release_x86_64.exe`, `linux_release.x86_64`; Godot 3 `windows_64_release.exe`, `linux_x11_64_release`) renamed to `<Title>.exe` / `<Title>.x86_64` with `<Title>.pck` beside it; `godot.extension_files(pck, platform, arch)` picks release libraries for the platform/arch plus `[dependencies]`; they and `steam_appid.txt` are copied from the game folder, missing ones warn. Linux gets `icon.png`. Windows `.exe` keeps Godot's icon (no PE resource editing yet).
- html5-export: NW.js 0.119.2 normal build (`NWJS` pins from dl.nwjs.io SHASUMS256.txt; `--set nwjs=PATH` for a local runtime), `nw.exe`/`nw` renamed to the title, game in `package.nw/` with `package.json` main = the game page, codec warning for `.mp4`/`.m4a` without `.webm`/`.ogg` twins. Electron is not used (RPG Maker needs nw.gui).
- Both write `<out>/<Title>/` + `<out>/<Title>-<target>.zip` (`ctx.zip_dir` streams files, 0755 for executables; `--set zip=false`).
- html5-import now returns the title (RPG Maker: `data/System.json` `gameTitle`).
- Verified: real NW.js downloads for win-x64 and linux-x64 matched their SHA-256 and built folders/zips for a synthetic MZ project (oxipng was auto-installed from its pin on the way). Not run on Windows or Linux (no machines here); no real Godot template was downloaded for Windows/Linux in this round.

## Done (2026-11-24, plan phase 3: importers)
- 3a: html5 detection knows Construct 2/3, GDevelop, Phaser, Cocos, Defold and Twine (scan reads top-level `.html` for `<tw-storydata`, signature rule kind `mark`); `Html5Game.framework`. RPG Maker XP/VX/VX Ace moved to the new engine id `rgss` (old `rpgmaker` = MV/MZ only). New engine `love`. Routes for rgss, love, html5. Fixtures: love-windows, construct3, gdevelop, twine.
- 3b: `formats/love.py` + love-import/love-export. LÖVE 11.5 pinned (SHA-256 computed from the release assets; GitHub publishes no digest for them). Verified with real downloads: signed Mac app, fused exe, AppImage + script.
- 3c: `formats/renpy.py` (layout, version, platforms, options.rpy title, safe RPA listing) + renpy-import/renpy-export following `launcher/game/distribute.rpy` of 8.5. SDK zip downloaded only when the build lacks the target's files, checked against renpy.org `checksums.txt` SHA-256, only lib/ + launchers unpacked, zip deleted. Verified: real 8.3.7 SDK, signed Mac app from a synthetic PC build.
- 3d: `formats/rgss.py` + rgss-import/rgss-export. mkxp-z has no release builds (0 GitHub releases; CI artifacts need sign-in): `--set mkxp=PATH`, remembered in the plugin cache (`mkxp-<os>.txt`); macOS game in `Contents/Game` (from mkxp-z's `filesystemImplApple.mm`), Linux next to the executable; `mkxp.json`; GPL note. Windows = original game minus saves. Not run with a real mkxp-z build.
- Manifest `game.variant` (from scan) and `game.engine_version` (import result `engine_version`); `GameInfo::engine_label()` shown on the Game page.

## Done (2026-11-24, plan phase 4: Android, experimental)
- Target `android` (Experimental, media astc/etc2, opus/vorbis, h264/vp9). Owner's Mac has the Android SDK from Homebrew (`/opt/homebrew/share/android-commandlinetools`: build-tools 34/35, platforms;android-35) and JDK 26; `ptk tools`/doctor list them (`android.status()`).
- `formats/axml.py` (binary manifest + arsc string pools; verified on the real Godot 4.4.1 template: parses and round-trips byte-identically) and `android.py` (SDK/JDK discovery, aligned APK writer that streams from the pck, debug keystore via keytool, apksigner sign+verify, `patch_manifest`, `settings_for_export`, `sign_for_export`).
- godot-export Android (Godot 4 only) per Godot 4.4's `export_plugin.cpp` (non-Gradle path). S3TC/BPTC-only textures get a `path=` fallback in `.import` (Godot 4.2+ decodes BC at load; 4.0/4.1 refused).
- html5-export Android: WebView player in `engine/plugins/html5-export/android/` built once with aapt2/javac/d8.
- Verified on this Mac: synthetic MZ game → APK, and synthetic Godot 4.4.1 game with the real template → APK; both pass `zipalign -c -P 16 4`, `apksigner verify` (v2+v3) and `aapt2 dump badging` (package, label, orientation, ABIs). **Not installed on a phone or emulator** (no emulator images here).

## Done (2026-11-25, plan phase 5: Godot textures to ASTC for Android)
- `formats/ctex.py` reads/writes Godot 4 `.ctex` (GST2) with Godot's mip size rules (`level_sizes`; checked against `Image::_get_dst_image_size`). `ptk_engine/texconv`: `ptk-bcdec` (C driver around the vendored `engine/third_party/bcdec/bcdec.h`, commit 80859ed3, MIT/Unlicense; compiled once with `cc`) decodes DXT1/3/5 and BC7 levels; astcenc (`-cl`, same LDR profile as Godot's astcenc module) encodes them. New pinned tool `astcenc` 5.7.1 (macOS universal zip, GitHub digest).
- godot-export Android: `android_textures=astc` (default) adds `.astc.ctex` files and `path.astc=` ahead of the `path=` fallback; `decode` keeps only the fallback; `astc_block`, `astc_quality`.
- Verified: decoder compiled and tested on real BC1 blocks; conversion and the remap order tested with a stand-in astcenc script (real astcenc not run here: Claude does not execute downloaded binaries). **Not checked on a phone.**

## Next task
1. Owner: re-export Buckshot Roulette for macOS and check the gray screen is gone and Steam features work; check the web build's loading screen. Try the Game page, Settings → Defaults, Icon & poster.
2. Owner: try a real Godot game and a real RPG Maker MV/MZ game (`ptk import <build>` then `ptk export`, or the app's Export tab); open the Godot `.app` on the Mac (not done by Claude). Check the macOS menu bar. Create the ptk-marketplace repo (see above).
3. Owner: run a Windows/Linux export of a real game on a Windows PC / Linux machine (or Steam Deck); try a real Ren'Py, LÖVE and RPG Maker XP/VX/Ace game (for the last one, download mkxp-z once and pass --set mkxp=PATH). Install an Android build on a phone (`adb install -r`) and report what happens. The plan's 5 phases are done; next ideas: a real-device test round, Godot 3 Android, ETC2 for phones without ASTC, love.js/Ren'Py web. Owner should try `ptk assets` / Estimate savings on a real RPG Maker game and check the music still loops after `brew install vorbis-tools`.
4. Not tested: real games (only synthetic pcks and fake HTML5 games), Godot 3 templates, threaded web builds in a browser, Electron games, Intel Macs, running the Windows/Linux exports (no machines here), running ptk itself on Windows/Linux (CI blocked on the account).

## Working agreement (after every change)
1. Run the tests (Rust and Python); for CLI output changes, take screenshots and show them.
2. Commit with a clear message and push to `main`.
3. Update "Git / release state" and "Next task" here.
4. Report in Hebrew: what changed, what was verified, what the owner must do, what is untested.
