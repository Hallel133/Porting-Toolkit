# FreeSpace 2 export (FreeSpace Open)

Runs FreeSpace 2 (its .vp archives) on macOS, Windows and Linux with FreeSpace Open.

- Runtime: [FreeSpace Open](https://github.com/scp-fs2open/fs2open.github.com) 26.2.0, Volition source code licence (non-commercial).
- Pinned official downloads (SHA-256 checked): linux-x64, macos-arm64, macos-x64, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- FreeSpace Open reads the game's .vp files from the folder it starts in (the launcher starts it in the game folder). The SSE2 Windows build is used because it runs on every 64-bit PC. FreeSpace Open's licence forbids selling it.

Every build carries `LICENSE-fs2open.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
