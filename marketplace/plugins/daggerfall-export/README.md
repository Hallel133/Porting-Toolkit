# Daggerfall export (Daggerfall Unity)

Runs The Elder Scrolls II: Daggerfall (its ARENA2 folder) on macOS, Windows and Linux with Daggerfall Unity.

- Runtime: [Daggerfall Unity](https://github.com/Interkarma/daggerfall-unity) 1.1.1, MIT.
- Pinned official downloads (SHA-256 checked): linux-x64, macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Daggerfall Unity asks for the Daggerfall folder on its first start: choose the game folder inside the build (it holds ARENA2).

Every build carries `LICENSE-daggerfall-unity.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
