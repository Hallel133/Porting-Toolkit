# M.U.G.E.N export (Ikemen GO)

Runs M.U.G.E.N content (chars/, stages/, data/) on macOS, Windows and Linux with Ikemen GO.

- Runtime: [Ikemen GO](https://github.com/ikemen-engine/Ikemen-GO) 1.0.0, MIT.
- Pinned official downloads (SHA-256 checked): linux-x64, macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Ikemen GO keeps its own data/, external/ and font/ next to the program; the M.U.G.E.N content is merged into that folder (the game's files win where both have one).

Every build carries `LICENSE-ikemen-go.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
