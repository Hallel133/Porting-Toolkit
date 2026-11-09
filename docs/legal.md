# A tool, not a distribution channel

Porting Toolkit helps people move games they own between platforms. It must
never become a way to share the games themselves. This is a design rule, not
a policy footnote, and the code enforces what it can.

## The model: bring your own game

Like [N64Recomp](https://github.com/N64Recomp/N64Recomp) and similar
projects, the community shares **how** to port a game, and every player
supplies **their own copy**:

- A community `.ptk` project contains recipes, settings, community-written
  patches and code, plugin versions, and **SHA-256 hashes** of the original
  files.
- It **never** contains original game files, extracted assets or decompiled
  game code.
- `ptk verify` checks that a player's own copy matches the hashes, so a
  project is only applied to the exact version it was made for.

## What the code enforces

- `ptk pack` only packs `ptk.json`, `README.md`, `LICENSE*`, `plugins.lock`,
  `recipes/`, `patches/` and `extra/`. Nothing else in the folder.
- It refuses third-party content: files the manifest marks in `third_party`,
  files whose hash equals a recorded game file (a copied or renamed original),
  and engine archives (`.rpf`, `.pak`, `.vpk`, `.pck`, `.assets`, `data.win`,
  …).
- The only exception is a game whose own license allows redistribution,
  declared in `game.content_license`: an open-source game (MIT, GPL, CC0, …)
  or the author's own game (`LicenseRef-Own-Work`). "Proprietary" or no
  license means never.
- `ptk scan` detects engines from file names and executable headers only.
  It does not decrypt, unpack or decode protected archives.

## What contributors must do

- Only add patches and code you wrote, or that you may share under the
  project's license.
- Do not commit or pack game files, extracted assets, decompiled or
  disassembled game code, keys, or links to pirated copies.
- Respect the law where you live. Rules about reverse engineering for
  interoperability, format shifting and circumvention differ between
  countries. Porting Toolkit does not give legal advice and does not
  make anything legal that is not.

## Reporting

If you find a `.ptk` file or plugin that distributes game content, open an
issue on the repository so it can be removed from community listings.

The project's own code is MIT licensed (see `LICENSE`). Trademarks and game
names belong to their owners; fixtures in this repository are empty files
with generic names.
