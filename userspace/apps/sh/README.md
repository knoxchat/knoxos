# sh

Ring 3 `/bin/sh` payload. Source: `src.py`. Blob: `sh.elf`.

Reads a line, then `ShellExecute` (`0x3053`). Kernel `compat/shell` is the implementation behind that syscall (`GATE_SH ring3 parser`).
