# Gold Box export (DOSBox Staging)

Runs SSI Gold Box games (Pool of Radiance and others: .DAX data) on macOS, Windows and Linux with DOSBox Staging.

- Runtime: [DOSBox Staging](https://github.com/dosbox-staging/dosbox-staging) 0.83.1, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): linux-x64, macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- DOSBox Staging starts START.EXE or GAME.EXE when the game has one; otherwise it opens a DOS prompt in the game folder.

Every build carries `LICENSE-dosbox-staging.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
