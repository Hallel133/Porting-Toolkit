# ScummVM export

Builds SCUMM (LucasArts), Sierra AGI, Adventure Game Studio, Wintermute and
Freescape games for macOS, Windows and Linux with [ScummVM](https://www.scummvm.org),
from your own copy of the game's data files.

- **macOS:** uses `/Applications/ScummVM.app` if you have it, otherwise downloads
  the official ScummVM 2026.4.0 disk image once (SHA-256 checked). The build is
  `<Game>.app`, signed ad hoc.
- **Windows:** downloads the official 64-bit zip once. The build is a folder with
  `<Game>.cmd`, plus a zip.
- **Linux:** ScummVM publishes no portable Linux build. Export with
  `--set runtime=PATH` (an AppImage, or a folder with a `scummvm` built to run
  anywhere).

ScummVM is GPL-3.0-or-later; every build carries `LICENSE-scummvm.txt` with its
source address. The game's files stay yours: they are copied into the build,
never shared.
