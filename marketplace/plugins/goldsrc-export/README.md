# Half-Life export (Xash3D FWGS)

Runs Half-Life (its valve/ folder) with Xash3D FWGS, an open GoldSrc-compatible engine you provide.

- Runtime: [Xash3D FWGS](https://github.com/FWGS/xash3d-fwgs) continuous, GPL-3.0-or-later.
- Pinned official downloads (SHA-256 checked): none.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- Xash3D FWGS publishes only rolling 'continuous' builds, which cannot be pinned: download one for the target and pass --set runtime=PATH. Half-Life's own game libraries (valve/dlls) are Windows-only: other platforms need hlsdk-portable builds of them (https://github.com/FWGS/hlsdk-portable) in valve/.

Every build carries `LICENSE-xash3d.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
