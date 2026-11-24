# bcdec

`bcdec.h` decompresses BC1–BC7 texture blocks. It is the decoder Godot itself
ships, and Porting Toolkit uses it to turn a Godot game's desktop textures
(S3TC/BPTC) back into pixels before re-encoding them as ASTC for Android.

- Source: https://github.com/iOrange/bcdec, file `bcdec.h` at commit
  `80859ed3b7afb1c527a2a99d70c61457bea72d0c` (v0.985), unchanged.
- Licence: MIT or the Unlicense, at your choice (see the end of `bcdec.h`).
- Compiled on the user's computer, once, into `ptk-bcdec` (see
  `engine/ptk_engine/texconv/ptk_bcdec.c`).
