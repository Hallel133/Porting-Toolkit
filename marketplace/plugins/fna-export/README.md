# XNA export (FNA)

Runs Microsoft XNA 4 games (an `.exe` with `Content/*.xnb`) on macOS and Linux with
[FNA](https://fna-xna.github.io), the accurate reimplementation of XNA used by most commercial
XNA ports.

- Needs an **FNA kit** built once: see [KIT.md](KIT.md). Export with `--set runtime=PATH` to
  it (remembered). Nothing is downloaded.
- The game runs unchanged: FNA's own `Microsoft.Xna.Framework*.dll` replace XNA's, and
  MonoKickstart (renamed `<Game>.bin.osx` / `<Game>.bin.x86_64`) starts the game's `.exe`.
- When the folder has several `.exe` files, pick the game with `--set exe=NAME.exe`.
- Warnings, not errors: `.wmv` videos (convert to `.ogv`), Steamworks (needs its Mac/Linux
  library), Apple Silicon (MonoKickstart is usually Intel-only, so Rosetta 2 is used).
- Windows: XNA games already run there (with the XNA redistributable), so there is no
  Windows target.

Not tested with a real kit or game yet: Porting Toolkit's tests use a fake kit.
