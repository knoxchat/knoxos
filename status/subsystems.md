# Subsystem status

← [Status index](README.md) · [Do this first](next.md)

## Code distribution

| Area | Files | Lines | Role |
|------|-------|-------|------|
| Kernel top-level modules | 404 | ~247,000 | Core + many Unused/Stub subsystems |
| GUI & desktop (`gui/`) | 161 | ~91,700 | Live software compositor + in-process apps |
| Shell & builtins | 17 | ~13,200 | Live in-kernel shell |
| Syscall interface | 14 | ~7,200 | Dispatch table; many no-ops |
| Terminal emulator | 10 | ~5,100 | Live in-kernel terminal |
| Boot crate | — | ~90 | Image builder |
| **Total `kernel/src`** | **609** | **~346,900** | |

~400 `pub mod` entries does **not** mean 400 working subsystems. Prefer deleting or gating Unused modules over adding Phase 34+.

---

## Summary

Percentages are **production usefulness**, not lines of code.

| # | Subsystem | Grade | Live % | Priority | One-line truth |
|---|-----------|-------|--------|----------|----------------|
| 1 | [Kernel Core](subsystems/01-kernel-core.md) | Wired | 82% | High | Interrupts and timers; per-CPU TSS + GS; AP Ring 3 (I3). Further APs still share BSP CFS. |
| 2 | [Memory](subsystems/02-memory.md) | Wired | 74% | **Critical** | Demand paging, CoW, file mmap, buddy, LRU, swap I/O, OOM, stack guards. |
| 3 | [Process](subsystems/03-process.md) | Wired | 99% | **Critical** | Scheduled Ring 3 (B2–B8); clone/clone3; namespaces. SMP load balance still BSP-shaped. |
| 4 | [Filesystem](subsystems/04-filesystem.md) | Wired | 99% | **Critical** | VirtIO-blk persist C1–C7 (`/bin` included); AHCI/NVMe DMA; VFS syscalls through BL. Virtual FS still rebuilt. |
| 5 | [Networking](subsystems/05-networking.md) | Wired | 72% | **Critical** | Loopback + VirtIO-net DHCP/DNS/TCP CUBIC (D1–D5). IPv6/TLS/Wi-Fi later. |
| 6 | [Drivers](subsystems/06-drivers.md) | Wired | 40% | **Critical** | PCI, PS/2, UART, VirtIO-blk, AHCI/NVMe DMA live; USB/GPU mostly stub. |
| 7 | [GUI](subsystems/07-gui.md) | Live | 84% | Medium | Software compositor + Ring 3 SHM clients (F1–F4). Most apps still in-kernel. |
| 8 | [Shell](subsystems/08-shell.md) | Live | 82% | Medium | Real parser/PTY; Ring 3 `/bin/sh`; desktop terminal still in-kernel. |
| 9 | [Security](subsystems/09-security.md) | Wired | 80% | **Critical** | W^X, ChaCha20, seccomp, caps (E1–E4); Landlock. Unimplemented syscalls return ENOSYS. |
| 10 | [Services](subsystems/10-services.md) | Wired | 42% | High | Ring 3 `/sbin/init` (K3); AF_UNIX D-Bus. No crash restart. |
| 11 | [Virt](subsystems/11-virt.md) | Stub | 30% | Low | Namespaces live; KVM `vmlaunch` unused. **Later.** |
| 12 | [AI/ML](subsystems/12-ai.md) | Wired | 22% | Low | GGUF parse + naive CPU. **Later.** |
| 13 | [Binary compat](subsystems/13-binary-compat.md) | Wired | 99% | **Do first** | Static ELF + 200+ Ring 3 syscalls + userspace `ld.so` (BM1). Next: real `dlopen`. |
| 14 | [i18n](subsystems/14-i18n.md) | Live | 68% | Low | TTF, CJK, RTL on the compositor; locale loading partial. |
| 15 | [Build](subsystems/15-build.md) | Live | 75% | Medium | Make/QEMU work; `flake.nix` missing. |
| 16 | [Testing](subsystems/16-testing.md) | Wired | 82% | **Critical** | `268/268` QEMU markers; `assert!(true)` gone. |
| 17 | [Docs](subsystems/17-docs.md) | Wired | 42% | **Critical** | README + LICENSE + BUILDING + CONTRIBUTING + `status/`. Architecture guides missing. |
| 18 | [CI/CD](subsystems/18-ci.md) | Wired | 30% | High | GitHub Actions fmt/clippy/size/QEMU boot; no signed releases. |

**QEMU desktop demo: ~99%.** **Production OS: ~98%.** Gates A–F, H–I, J–BL, BM1, and C7 are on the live boot path. Full gate tables: [gates/](gates/README.md). Integration: `./tests/run_integration.sh` **268/268** on QEMU (2026-10-13). What to do next: [next.md](next.md).

---
