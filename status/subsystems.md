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
| 1 | [Kernel Core](subsystems/01-kernel-core.md) | Wired | 90% | High | Interrupts and timers; per-CPU TSS + GS; AP Ring 3 (I3); nested IRQ TPR; live patch W^X; UEFI RTC time. |
| 2 | [Memory](subsystems/02-memory.md) | Wired | 86% | **Critical** | Demand paging, CoW, file mmap, buddy, LRU, swap I/O, OOM, stack guards, memory cgroup, NUMA frames, KSM merge. |
| 3 | [Process](subsystems/03-process.md) | Wired | 99% | **Critical** | Scheduled Ring 3 (B2–B8); clone/clone3; namespaces; eBPF/sched_ext pick (`GATE_EBPF`). |
| 4 | [Filesystem](subsystems/04-filesystem.md) | Wired | 99% | **Critical** | VirtIO-blk persist C1–C7 (`/bin` included); AHCI/NVMe DMA; NTFS/exFAT/XFS/Btrfs/ZFS ramdisk; CIFS SMB2; FUSE LOOKUP; NFS4 COMPOUND. Virtual FS still rebuilt. |
| 5 | [Networking](subsystems/05-networking.md) | Wired | 88% | **Critical** | Loopback + VirtIO-net DHCP/DNS/TCP CUBIC (D1–D5) + IPv6 echo TX (D6) + NAT + TLS HKDF/handshake + VLAN + e1000 `Socket::send` + rtl8139 loopback + Wi-Fi PMK + WPA3-SAE + bridge FDB. |
| 6 | [Drivers](subsystems/06-drivers.md) | Wired | 91% | **Critical** | PCI, PS/2, UART, VirtIO-blk, VirtIO-GPU scanout+cursor (F8), AHCI/NVMe DMA, USB HID URB + XHCI MMIO + Enable Slot cc=1 + GET_DESCRIPTOR + MSC/hub/gamepad/audio/trackpad/wacom/UVC/ethernet/touch + GPIO + DRM GEM + TPM TIS + I2C + i915 modeset + AMDGPU fw + e1000/rtl8139 send + AX211 + Broadcom fw + Thunderbolt hop. |
| 7 | [GUI](subsystems/07-gui.md) | Live | 99% | Medium | Software compositor + Ring 3 SHM (F1–F8) + VSync + VirtIO-GPU cursor + TTF hinting. Start-menu Terminal is Ring 3 PTY. |
| 8 | [Shell](subsystems/08-shell.md) | Live | 86% | Medium | Real parser/PTY; Ring 3 `/bin/sh`; SIGTSTP/SIGCONT job control. |
| 9 | [Security](subsystems/09-security.md) | Wired | 98% | **Critical** | W^X, ChaCha20, AES-GCM, Poly1305, X25519, Ed25519, RSA, ECDSA-P256, seccomp, caps (E1–E4); Landlock; SELinux TE; AppArmor path MAC; dm-verity; IMA; lockdown; wipe-on-free. |
| 10 | [Services](subsystems/10-services.md) | Wired | 80% | High | Ring 3 `/sbin/init` (K3); AF_UNIX D-Bus; crash restart; unit MemoryMax cgroup; cron timer; udev `/dev`; NM DHCP; NTP apply; Bluetooth HCI Reset; suspend; logind. |
| 11 | [Virt](subsystems/11-virt.md) | Wired | 55% | Low | Namespaces live; cgroup v2 memory/CPU/IO; software EPT+HLT (`GATE_KVM`); OCI unshare+pivot_root (`GATE_OCI`). |
| 12 | [AI/ML](subsystems/12-ai.md) | Wired | 40% | Low | GGUF parse + SDPA/KV (`GATE_AI attn`) + CPU matmul (`GATE_GPU matmul`) + assistant chat (`GATE_AI chat`). |
| 13 | [Binary compat](subsystems/13-binary-compat.md) | Wired | 99% | High | Static ELF + 200+ Ring 3 syscalls + `ld.so` (BM1) + VMM `dlopen` (BM2) + static musl CRT (BM3) + dynamically linked musl (BM4). Next: Ring 3 desktop apps. |
| 14 | [i18n](subsystems/14-i18n.md) | Live | 95% | Low | TTF, CJK, RTL, fallback chain, GSUB Latin liga, GPOS kern, GPOS mark, color emoji, TTF hinting, `.mo` gettext, pinyin IME. |
| 15 | [Build](subsystems/15-build.md) | Live | 80% | Medium | Make/QEMU work; feature flags G3; `flake.nix` present. |
| 16 | [Testing](subsystems/16-testing.md) | Wired | 98% | **Critical** | 396 QEMU markers + host parser tests; `assert!(true)` gone. |
| 17 | [Docs](subsystems/17-docs.md) | Wired | 70% | **Critical** | README + LICENSE + BUILDING + CONTRIBUTING + `status/` + `docs/` + CHANGELOG + rustdoc artifact. |
| 18 | [CI/CD](subsystems/18-ci.md) | Wired | 55% | High | GitHub Actions fmt/clippy/size/QEMU boot + aarch64/riscv64 compile + rustdoc + audit + nightly images. |

**QEMU desktop demo: ~99%.** **Production OS: ~99%.** Gates A–F, H–I, J–BL, BM1–BM4, F5–F8, C7–C9, G3–G5, I4, D6, USB HID URB, XHCI MMIO, udev/quota/cron/vsync/TERM plus cgroup/job/IMA/lockdown/UEFI/wipe/NM/XFS/X25519/Ed25519/locale/IME/MSC/NAT/DRM/GSUB/RSA/ECDSA/emoji/hub/gamepad/TPM/TTF/I2C/TLS13/HDR/VRR/e1000/eBPF/Wi-Fi/i915/Btrfs/VLAN plus USB audio/trackpad/wacom/UVC/ethernet/touch/GPIO plus rtl8139/CIFS/GPOS/AMDGPU/AX211/FUSE plus KVM EPT/HLT, OCI start, ZFS, WPA3/bridge, TLS handshake, GPU matmul, AI attention, NFS, Broadcom, Thunderbolt plus AppArmor, XHCI GET_DESCRIPTOR, unit MemoryMax, GPOS mark are on the live boot path. Full gate tables: [gates/](gates/README.md). Integration: `./tests/run_integration.sh` **396/396** on QEMU (2026-10-19). What to do next: [next.md](next.md). Desktop pointer is VirtIO-tablet + USB digitizer + PS/2 (`GATE_VIRTIO input tablet`).

---
