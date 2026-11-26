# Build engine export (Raze)

Runs Duke Nukem 3D, Blood, Shadow Warrior, Redneck Rampage and other Build engine games on macOS and Windows with Raze.

- Runtime: [Raze](https://github.com/ZDoom/Raze) 1.11.0, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Raze looks for the game data in the folder it starts in (the launcher starts it in the game folder). Linux needs --set runtime=PATH. The pins were hashed by Porting Toolkit (the 1.11.0 release has no published digests).

Every build carries `LICENSE-raze.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
