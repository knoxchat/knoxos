# Syscall behavior

Linux syscall **numbers** are not compatibility. A handler that returns `Ok(0)` is a no-op, not an implementation. Gate G4 is the audit: unused clock/time numbers return `-ENOSYS` from Ring 3 (`GATE_G4 enosys`); `madvise` / `mlock*` / `readahead` / `fadvise64` / `set_robust_list` store real state (`GATE_G4 audit live`).

## How to read a number

| Kind | Meaning | Example |
|------|---------|---------|
| Live | Changes kernel state or copies real bytes | `write`, `mmap`, `clone`, `sendto` |
| Wired | Partial: real path, incomplete POSIX | `madvise` records advice; no reclaim yet |
| ENOSYS | Honest reject (`-ENOSYS`) | `bpf`, `io_uring`, `kexec`, `adjtimex` |
| No-op | Silent success — **do not add these** | Convert to ENOSYS or implement when you touch it |

The dispatch table is `kernel/src/syscall/`. Letter gates **J–BL** in [status/gates/syscalls.md](../status/gates/syscalls.md) are QEMU proofs of individual calls (clone, namespaces, VFS, sockets). Adding another `GATE_xx` only to ENOSYS one unused number is not the critical path.

## Userspace ABI

- Syscall entry: `syscall`/`sysretq` with TSS + `KERNEL_GS_BASE`.
- Static ELF: musl `crt1` `_start` → `__libc_start_main` → `main` (BM3).
- Dynamic ELF: `PT_INTERP` `/lib/ld-knoxos.so`, `DT_NEEDED`, lazy PLT, GNU_RELRO (BM1, BM4).
- `dlopen`: `open` + file-backed `mmap` via the VMM (BM2).

452 numbers in a table still does not mean 452 behaviors. Prefer converting a silent `Ok(0)` over adding a new lettered gate.
