# TIC-80 export

Runs TIC-80 fantasy-console carts (.tic) on macOS, Windows and Linux with TIC-80.

- Runtime: [TIC-80](https://github.com/nesbox/TIC-80) 1.4.1, MIT.
- Pinned official downloads (SHA-256 checked): linux-x64, macos-arm64, macos-x64, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- The cart starts directly (--skip leaves out TIC-80's intro).

Every build carries `LICENSE-tic80.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
