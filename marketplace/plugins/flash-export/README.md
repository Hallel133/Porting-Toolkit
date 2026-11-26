# Flash export (Ruffle)

Runs Flash games (.swf), including Flixel games, on macOS, Windows and Linux with the Ruffle desktop player.

- Runtime: [Ruffle](https://github.com/ruffle-rs/ruffle) nightly-2026-11-26, MIT OR Apache-2.0.
- Pinned official downloads (SHA-256 checked): linux-x64, macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Ruffle publishes only nightly builds; this one is pinned. Games drawn with Stage3D (Starling, Flare3D, Away3D) do not work in Ruffle yet. If a game has several .swf files, the first one found is started.

Every build carries `LICENSE-ruffle.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
