# Doom 3 export (dhewm3)

Runs Doom 3 and Resurrection of Evil (base/*.pk4) on Windows and Linux with dhewm3, and on macOS with a dhewm3 app you choose.

- Runtime: [dhewm3](https://github.com/dhewm/dhewm3) 1.5.5, GPL-3.0-or-later.
- Pinned official downloads (SHA-256 checked): linux-x64, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- dhewm3 1.5.5 has no macOS build (only its release candidates had one): pass --set runtime=PATH to a dhewm3 app. The Windows build is 32-bit and runs on 64-bit Windows.

Every build carries `LICENSE-dhewm3.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
