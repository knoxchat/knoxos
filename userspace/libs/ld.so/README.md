# ld.so

Ring 3 ELF interpreter (`/lib/ld-knoxos.so`). Instruction bytes: `elf.rs`. The kernel embeds the image from `kernel/src/init/dynlink.rs` (`include!`) and installs it on the VFS. `execve` still loads `PT_INTERP`.

QEMU marker: `GATE_BM1 dynlink complete`.

`kernel/src/compat/ldknoxos.rs` is kernel-side linker theater (search paths / cache). It is not this interpreter.
