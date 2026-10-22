# libc

Ring 3 musl-style libc: static CRT (BM3), `/lib/libc.so` exporting `__libc_start_main`, and the dynamically linked musl binary (BM4). Instruction bytes: `elf.rs`. The kernel embeds the images from `kernel/src/init/dynlink.rs`.

QEMU markers: `GATE_BM3 musl abi`, `GATE_BM4 musl dynlink`, `GATE_GPUI musl raster` (`/bin/gate_musl_ui`).

`kernel/src/compat/musl.rs` is kernel-side CRT/target glue, not this libc.
