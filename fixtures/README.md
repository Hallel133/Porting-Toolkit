# Test fixtures

`games/` holds fake game folders used by the engine-detection tests, the CLI
tests and demos (`ptk scan fixtures/games/unity-windows`).

Every file is **empty** and only has the right name, except
`macos-app/Demo.app/Contents/MacOS/Demo`, which holds the four-byte Mach-O
magic number. No game content, ever: see `docs/legal.md`.
