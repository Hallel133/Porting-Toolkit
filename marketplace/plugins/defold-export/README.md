# Defold export

Runs Defold games (`game.projectc`, `game.arcd`, `game.arci`, `game.dmanifest`) on macOS,
Windows and Linux with the **Defold engine of the exact version the game was built with**.

- The version is read from the game's own engine (`<Game>.exe`, the Linux binary or the Mac
  app's binary): every Defold engine embeds its version and sha1. Only the bytes are read; the
  game's engine is never run.
- The matching engine for the target is downloaded once from
  `d.defold.com/archive/stable/<sha1>/engine/<platform>/dmengine`. Defold publishes no SHA-256
  for these files, so the download relies on HTTPS; the export notes show its SHA-256.
- `--set defold_sha1=<sha1>` when the game's engine cannot be read; `--set runtime=PATH` to use
  a `dmengine` you already have.
- Games with native extensions (Steam, ads, platform SDKs) built into their engine lose those
  features: the downloaded engine is the plain one.

Not tested with a real Defold game yet: Porting Toolkit's tests use a fake engine.
