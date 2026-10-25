# Syscall behavior

Linux syscall **numbers** are not compatibility. A handler that returns `Ok(0)` is a no-op, not an implementation. Gate G4 is the audit: unused clock/time numbers return `-ENOSYS` from Ring 3 (`GATE_G4 enosys`); `madvise` / `mlock*` / `readahead` / `fadvise64` / `set_robust_list` store real state (`GATE_G4 audit live`).

## How to read a number

| Kind | Meaning | Example |
|------|---------|---------|
| Live | Changes kernel state or copies real bytes | `write`, `mmap`, `clone`, `sendto` |
| Wired | Partial: real path, incomplete POSIX | `madvise` records advice; no reclaim yet |
| ENOSYS | Honest reject (`-ENOSYS`) | `bpf`, `kexec`, `adjtimex` |
| No-op | Silent success — **do not add these** | Convert to ENOSYS or implement when you touch it |

Numbers, `SyscallError`, `stat`/`statx` layouts, and `sockaddr_*` live in [`abi/`](../abi/) (`knox-abi`). The dispatch table and handlers are `kernel/src/syscall/`. VFS inodes stay in `kernel/src/fs/`. Letter gates **J–BL** in [status/gates/syscalls.md](../status/gates/syscalls.md) are QEMU proofs of individual calls (clone, namespaces, VFS, sockets). Adding another `GATE_xx` only to ENOSYS one unused number is not the critical path.

## Mount: number → behaviour

The mount syscalls used to be the last silent `Ok(0)`s in the tree. They now drive
one real table (`kernel/src/fs/mount.rs`), and the mount-namespace table
(`kernel/src/sched/namespaces.rs`) mirrors the entries that belong to a namespace.

| Number | Name | Behaviour today |
|--------|------|-----------------|
| 165 | `mount` | Parses source/target/fstype/data, rejects an already-mounted target with `EBUSY`, creates the mount point, runs the driver (`ext4`/`ext2`/`vfat`/`iso9660`/`ntfs`/`exfat`/`btrfs`/`xfs`/`zfs`), and records the entry in the global table **and** the caller's mount namespace |
| 166 | `umount2` | Removes the entry from both tables. Not a mount point → `EINVAL`; a nested mount still attached → `EBUSY` unless `MNT_FORCE` |
| 0x3032 | `umount` | Same as `umount2(target, 0)` |
| 429 | `move_mount` | Retargets the entry, keeping its `mount_id`, source, fs type, and driver index; nested mounts move with it. Root or self-nesting → `EINVAL`, occupied target → `EBUSY` |

`GATE_UMOUNT live` is the QEMU self-test for that matrix; `tests/run_integration.sh`
checks the marker. `do_move_mount` / `umount2` are also unit-tested through
`test_umount_live_path`.

## Userspace ABI

- Syscall entry: `syscall`/`sysretq` with TSS + `KERNEL_GS_BASE`.
- Static ELF: musl `crt1` `_start` → `__libc_start_main` → `main` (BM3).
- Dynamic ELF: `PT_INTERP` `/lib/ld-knoxos.so`, `DT_NEEDED`, lazy PLT, GNU_RELRO (BM1, BM4).
- `dlopen`: `open` + file-backed `mmap` via the VMM (BM2).

452 numbers in a table still does not mean 452 behaviors. Prefer converting a silent `Ok(0)` over adding a new lettered gate.
