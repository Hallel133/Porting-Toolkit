# PS5 title import

Reads a PS5 title from **your own dump** (`eboot.bin`, `sce_sys/param.json`, `sce_module/`)
into the project's `recipes/ps5.json`: title ID, name, version, bundled modules, and whether
each executable is a decrypted ELF (what [AnyPS5](https://github.com/boykopovar/AnyPS5) needs) or
an encrypted SELF container.

Only file names, `param.json` and the first four bytes of each executable are read. Porting
Toolkit does not decrypt anything, and nothing from the title goes into the project.

Export with **ps5-export**.
