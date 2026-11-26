# NScripter export (ONScripter)

Runs NScripter visual novels (nscript.dat, .nsa archives) with an ONScripter build you provide.

- Runtime: [ONScripter](https://onscripter.osdn.jp/onscripter.html) any, GPL-2.0-or-later.
- Pinned official downloads (SHA-256 checked): none.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- ONScripter's home (OSDN) no longer serves pinned releases, so nothing is pinned: pass --set runtime=PATH (ONScripter, or a fork such as ONScripter-EN or ONScripter-RU).

Every build carries `LICENSE-onscripter.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
