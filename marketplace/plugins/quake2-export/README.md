# Quake II export (Yamagi Quake II)

Runs Quake II (its baseq2/ folder) on Windows with Yamagi Quake II, and on macOS and Linux with a Yamagi build you choose.

- Runtime: [Yamagi Quake II](https://github.com/yquake2/yquake2) 8.70a, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Yamagi publishes Windows builds only (no checksums: the pin was hashed by Porting Toolkit). macOS and Linux need --set runtime=PATH to a build of yquake2.

Every build carries `LICENSE-yquake2.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
