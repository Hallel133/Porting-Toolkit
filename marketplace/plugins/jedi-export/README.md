# Dark Forces export (The Force Engine)

Runs Star Wars: Dark Forces (DARK.GOB and its other .GOB files) on Windows with The Force Engine.

- Runtime: [The Force Engine](https://github.com/luciusDXL/TheForceEngine) 1.22.420, GPL-2.0-only.
- Pinned official downloads (SHA-256 checked): windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- The Force Engine has no macOS release. On first start it asks where Dark Forces is: choose the game folder inside the build. Linux needs --set runtime=PATH (a build from source).

Every build carries `LICENSE-theforceengine.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
