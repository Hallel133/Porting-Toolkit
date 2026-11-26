# Quake export (vkQuake)

Runs Quake (its id1/ folder and mission packs) on Windows and Linux with vkQuake, and on macOS with a Quake port app you choose.

- Runtime: [vkQuake](https://github.com/Novum/vkQuake) 1.37.0, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): linux-x64, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- vkQuake publishes no macOS build. On a Mac, install QuakeSpasm (https://quakespasm.sourceforge.net) in Applications or pass --set runtime=PATH to its app.

Every build carries `LICENSE-vkquake.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
