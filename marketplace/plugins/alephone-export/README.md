# Marathon export (Aleph One)

Runs Marathon, Marathon 2, Marathon Infinity and other Aleph One scenarios (Shapes, Sounds, Map files) on macOS and Windows with Aleph One.

- Runtime: [Aleph One](https://github.com/Aleph-One-Marathon/alephone) 20260930, GPL-3.0-or-later.
- Pinned official downloads (SHA-256 checked): macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Aleph One is given the scenario folder. Linux has no official build (Flatpak only): --set runtime=PATH.

Every build carries `LICENSE-alephone.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
