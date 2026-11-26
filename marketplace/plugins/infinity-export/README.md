# Infinity Engine export (GemRB)

Runs Baldur's Gate, Baldur's Gate II, Icewind Dale and Planescape: Torment (chitin.key and their .bif data) on macOS, Windows and Linux with GemRB.

- Runtime: [GemRB](https://github.com/gemrb/gemrb) 0.9.5, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): linux-x64, macos-arm64, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- GemRB is given the game folder and detects which game it is. The Intel Mac build is a tar.bz2 without a published checksum: use --set runtime=PATH. Pins were hashed by Porting Toolkit (SourceForge publishes no SHA-256).

Every build carries `LICENSE-gemrb.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
