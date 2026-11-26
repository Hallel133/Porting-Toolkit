# Scratch export (TurboWarp)

Runs Scratch projects (.sb3, .sb2) on macOS, Windows and Linux with TurboWarp Desktop.

- Runtime: [TurboWarp Desktop](https://github.com/TurboWarp/desktop) 1.17.1, GPL-3.0-only.
- Pinned official downloads (SHA-256 checked): linux-x64, macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- TurboWarp opens the project in its editor window; press the green flag to play. For a stand-alone web or app build of a project, the TurboWarp Packager (packager.turbowarp.org) is the better tool.

Every build carries `LICENSE-turbowarp.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
