# RPG Maker 2000/2003 export (EasyRPG Player)

Runs RPG Maker 2000 and 2003 games (RPG_RT.ldb, RPG_RT.lmt) on macOS, Windows and Linux with EasyRPG Player.

- Runtime: [EasyRPG Player](https://github.com/EasyRPG/Player) 0.8.1.1, GPL-3.0-or-later.
- Pinned official downloads (SHA-256 checked): linux-x64, macos, windows-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Games that use the RPG Maker 2000/2003 RTP need it installed where EasyRPG looks for it (see easyrpg.org). The pins were hashed by Porting Toolkit (easyrpg.org publishes no checksums).

Every build carries `LICENSE-easyrpg.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
