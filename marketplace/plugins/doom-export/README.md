# Doom engine export (UZDoom)

Runs Doom, Doom II, Heretic, Hexen, Strife and Freedoom (their IWADs) on macOS, Windows and Linux with UZDoom, the maintained continuation of GZDoom.

- Runtime: [UZDoom](https://github.com/UZDoom/UZDoom) 5.0.3, GPL-3.0-or-later.
- Pinned official downloads (SHA-256 checked): linux-x64, macos-arm64, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- UZDoom publishes no Intel Mac build; an Intel Mac build can use --set runtime=PATH (a GZDoom app, for example).

Every build carries `LICENSE-uzdoom.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
