# Ultima VII export (Exult)

Runs Ultima VII: The Black Gate and Serpent Isle (their STATIC/ data) on macOS with Exult.

- Runtime: [Exult](https://github.com/exult/exult) 1.12.1, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): macos.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Exult for Windows is an installer: Windows and Linux need --set runtime=PATH. The build writes an exult.cfg pointing at the game folder. The macOS pin was hashed by Porting Toolkit.

Every build carries `LICENSE-exult.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
