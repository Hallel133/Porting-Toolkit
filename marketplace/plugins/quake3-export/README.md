# Quake III Arena export (ioquake3)

Runs Quake III Arena (its baseq3/ folder) on macOS, Windows and Linux with ioquake3.

- Runtime: [ioquake3](https://github.com/ioquake/ioq3) 1.36+, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): none.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- ioquake3's downloads (files.ioquake3.org) have no version in their names and no checksums, so nothing is pinned: install ioquake3, or pass --set runtime=PATH to the unzipped build.

Every build carries `LICENSE-ioquake3.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
