# PS5 title export (AnyPS5)

Converts a PS5 title from **your own decrypted dump** into a native **Linux** or **Windows**
program (or an x86-64 Mac one) with [AnyPS5](https://github.com/boykopovar/AnyPS5)'s relinker
and its PS5 system libraries. No emulator: the title's code runs natively.

- **Input:** the title's `app0` folder with a **decrypted** `eboot.bin` (ELF), `sce_sys/` and
  `sce_module/`. Encrypted (SELF) files are refused: AnyPS5 does not decrypt, and neither does
  Porting Toolkit.
- **Relinker:** the official Linux or Windows build of AnyPS5 v0.1.5 is downloaded once (SHA-256
  checked) and run on this computer at export time; its output is cached. AnyPS5 publishes no Mac
  build: on a Mac, build the relinker-only version (AnyPS5's `docs/dev/BUILD.md`) and pass
  `--set relinker=PATH`.
- **System libraries:** `prx-linux` / `prx-windows` v0.1.5 (about 120 MB each, SHA-256 checked).
  The Linux ones need glibc 2.43 (Ubuntu 26.04, Fedora 44). A Mac target needs libraries you
  built: `--set libs=PATH`.
- **Output:** `<Title>/<Title>.elf|.exe`, `libs/`, `app0/` (AnyPS5's runtime layout) and
  `LICENSE-anyps5.txt`. `--set zip=true` also zips it (off by default: titles are large).
- **Requirements and risks:** a CPU with AVX2, FMA, F16C, BMI1/2, LZCNT and MOVBE; the title
  runs as an ordinary program with your account's permissions (no console sandbox).

Experimental, and not tested with a real title or the real relinker yet: Porting Toolkit's tests
use a fake relinker. AnyPS5 is not affiliated with Sony Interactive Entertainment.
