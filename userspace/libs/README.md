# userspace/libs

**System libs** linked into Ring 3 processes: `knox-gpui`, `knox-ui`, `libc`, and `ld.so`.

Today the in-kernel compositor may still link `knox-gpui` / `knox-ui`. The source home is still this directory; that does not make the compositor Ring 3.

**Does not belong here**

- Kernel-hosted libc / loader theater (`musl.rs`, `posix_libc/`, `ldknoxos.rs`, …). Keep them in `kernel/src/compat/`. The Ring 3 `ld.so` / musl ELF sources live in `ld.so/` and `libc/` here.
- Syscall handlers (`kernel/src/syscall/`) and the ABI crate (`abi/`).

Ring 3 linkage is proven by QEMU markers (`GATE_BM3` / `GATE_BM4` for musl), not by crate layout.
