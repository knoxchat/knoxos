# Gate G — quality bar, and what is later

← [Status index](../README.md) · [Do this first](../next.md) · [Gates](README.md)

### Gate G — Quality bar (parallel from day one)

| ID | Task | Done when |
|----|------|-----------|
| G1 | `README.md` + `LICENSE` | **Done** |
| G2 | GitHub Actions: fmt, clippy, size, QEMU boot | **Workflow present** (`.github/workflows/ci.yml`) |
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | Default kernel compiles **Live** code only |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | **Partial** — `bpf`/`pkey`/`process_mrelease`/`quotactl`/`remap_file_pages`/`io_uring_*`/`userfaultfd`/`perf_event_open`/`fanotify`/`io_setup`/`io_destroy`/`kexec`/`init_module`/`mount_setattr`/`fsopen`/`keyctl`/`ioperm`/`iopl`/`acct`/`swapon`/`swapoff`/`modify_ldt`/`sysfs`/`vhangup`/`lookup_dcookie`/`memfd_secret`/`uselib`/`pkey_mprotect`/`process_mrelease`/`set_mempolicy`/`get_mempolicy`/`mbind`/`migrate_pages`/`move_pages`/`sched_setattr`/`sched_getattr`/`futex_waitv`/`open_by_handle_at`/`name_to_handle_at`/`map_shadow_stack`/`ustat`/`set_thread_area`/`add_key`/`request_key`/`io_submit`/`io_getevents`/`io_pgetevents`/`fsconfig`/`fsmount`/`open_tree`/`move_mount`/`fspick`/`delete_module`/KVM vCPU regs return ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4, AP4, AQ4, AR4, AS4, AT4, AU4, AV4, AW4, AX4, AY4, AZ4, BA4, BB4, BC4, BD4, BE4, BF4, BG4, BH4, BI4, BJ4, BK4, BL4); remaining silent `Ok(0)` still exist |
| G5 | `unsafe` SAFETY comments + size budget | Clippy gate |

### Explicitly later (after Gates A–E)

KVM, containers/k8s, Btrfs/ZFS, Wi-Fi, TLS 1.3, GPU compute, AI inference, package formats, Chromium/Vivaldi, aarch64/riscv64 **runtime**, Thunderbolt, TPM measured boot.

Shipping these before Gate B **increases** the distance to a perfect OS.

---
