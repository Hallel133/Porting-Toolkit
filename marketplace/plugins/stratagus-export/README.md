# Stratagus export

Runs Stratagus games (Wargus, Stargus and others: a folder with scripts/stratagus.lua) with a Stratagus build you provide.

- Runtime: [Stratagus](https://github.com/Wargus/stratagus) 3.3.2, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): none.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Stratagus's last release (3.3.2, 2022) ships only installers without checksums, so nothing is pinned: pass --set runtime=PATH. Wargus needs data extracted from the original Warcraft II with its own tool first.

Every build carries `LICENSE-stratagus.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
