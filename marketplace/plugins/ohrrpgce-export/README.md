# OHRRPGCE export

Runs OHRRPGCE games (.rpg files) with the OHRRPGCE player you provide.

- Runtime: [OHRRPGCE](https://rpg.hamsterrepublic.com/ohrrpgce/Source) any, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): none.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- OHRRPGCE's downloads have no version in their names and no checksums, so nothing is pinned: pass --set runtime=PATH to the unzipped player.

Every build carries `LICENSE-ohrrpgce.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
