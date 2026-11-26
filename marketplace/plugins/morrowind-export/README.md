# Morrowind export (OpenMW)

Runs The Elder Scrolls III: Morrowind (Data Files with Morrowind.esm, Tribunal and Bloodmoon) on macOS and Linux with OpenMW.

- Runtime: [OpenMW](https://github.com/OpenMW/openmw) 0.51.0, GPL-3.0-or-later.
- Pinned official downloads (SHA-256 checked): linux-x64, macos-arm64, macos-x64.
- Any other target: export with `--set runtime=PATH` (the app, the folder, or an archive of it). The path is remembered.
- OpenMW for Windows is an installer, so Windows needs --set runtime=PATH to an installed OpenMW folder. If OpenMW reports missing fallback values, run the OpenMW Launcher once to import Morrowind.ini.

Every build carries `LICENSE-openmw.txt` with the runtime's licence and source address. The game's files
come from your own copy and are only copied into the build.

Not tested with the real runtime or a real game yet: Porting Toolkit's tests use fake runtimes.
