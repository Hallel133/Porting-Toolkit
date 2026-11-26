# Wolfenstein 3D export (ECWolf)

Runs Wolfenstein 3D and Spear of Destiny (VSWAP.*, GAMEMAPS.*) on macOS and Windows with ECWolf.

- Runtime: [ECWolf](https://bitbucket.org/ecwolf/ecwolf) 1.4.2, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- ECWolf reads the game's files from the folder it starts in (the launcher starts it in the game folder). Linux needs --set runtime=PATH. The pins were hashed by Porting Toolkit (no published checksums).

Every build carries `LICENSE-ecwolf.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
