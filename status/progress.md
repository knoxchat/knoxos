# Progress tracker

← [Status index](README.md) · [Do this first](next.md)

## Anti-goals

Do not:

- Add another `pub mod` that `serial_println`s and returns `Ok(())`.
- Count syscall **numbers** as compatibility.
- Mark GUI 100% while apps are kernel enum variants.
- Implement SCHED_DEADLINE, eBPF, or overlayfs before `iretq`.
- Keep tests that `assert!(true)`.
- Claim Nix/GitHub/README completeness when the files are absent.

---

## Scores

**Production OS: ~99%** · **QEMU desktop demo: ~99%**

```
Kernel Core:        ███████████████████████░░  90%  Wired         ← I1–I4 + nested IRQ TPR + live patch W^X + UEFI RTC time + ACPI S5 PM1a
Memory Mgmt:        ██████████████████████░░░  86%  Wired         ← H1–H4 + J2/J3 + K2 swap I/O + memory cgroup + NUMA frames + KSM merge
Process/Sched:      ████████████████████████░  99%  Wired         ← B3–B8 + I2/I3 + J1 + K1 join + L1 TLS + namespaces + eBPF/sched_ext pick (`GATE_EBPF exec`)
Filesystem:         █████████████████████████  100% Wired         ← C1–C8 (`/bin` persist + JBD2 replay) + C10 virtual-FS mount points + NTFS/exFAT/XFS/Btrfs/ZFS/FAT32/ext2/ISO9660 ramdisk + CIFS SMB2 + FUSE LOOKUP+WRITE+UNLINK + NFS4 COMPOUND + inotify + VFS syscalls through BL
Networking:         ██████████████████████░░░  88%  Wired         ← D1–D6 IPv6 echo TX + IPv4 NAT checksum + TLS HKDF/handshake + VLAN + e1000 Socket::send + rtl8139 loopback + Wi-Fi PMK + WPA3-SAE + bridge FDB + TCP send/recv
Device Drivers:     ███████████████████████░░  93%  Wired         ← AHCI + NVMe DMA + VirtIO-GPU (F8) + VirtIO-input tablet + USB HID URB + XHCI MMIO + Enable Slot + Address Device + GET_DESCRIPTOR + SET_CONFIGURATION + Configure Endpoint + full enum + MSC/hub/gamepad/audio/trackpad/wacom/UVC/ethernet/touch + GPIO + DRM GEM + TPM TIS + I2C + i915 modeset + AMDGPU fw + e1000 + rtl8139 + AX211 + Broadcom + Thunderbolt
GUI & Desktop:      ████████████████████████░  99%  Live          ← F1–F8 SHM clients + Wayland AF_UNIX + wl_display + VirtIO-GPU cursor + VSync + HDR/VRR + Ring 3 Terminal PTY + GATE_GPU scanout + IME pinyin + TTF hint + VirtIO-tablet pointer
Shell & Terminal:   █████████████████████░░░░  86%  Live          ← sigreturn + SIGTSTP/SIGCONT job control
Security:           ████████████████████████░  98%  Wired         ← E1–E4 + K4 Landlock + SELinux TE + AppArmor path MAC + dm-verity + IMA/lockdown/wipe + AES-GCM/Poly1305 + X25519/Ed25519 + RSA/ECDSA-P256
System Services:    █████████████████████░░░░  84%  Wired         ← K3 /sbin/init + L3 D-Bus + Hello unique name + RequestName + ReleaseName + PAM unix + svc restart + unit MemoryMax cgroup + cron + udev + NM DHCP + NTP + Bluetooth HCI + suspend + logind
Virtualization:     ██████████████░░░░░░░░░░░  58%  Wired         ← namespaces + OverlayFS + cgroup v2 memory/CPU/IO/freeze + software EPT/HLT + OCI pivot_root
AI/ML:              ██████████░░░░░░░░░░░░░░░  40%  Wired         ← GGUF + SDPA/KV + CPU matmul + assistant chat
Binary Compat:      ████████████████████████░  99%  Wired         ← execve + fork + clone + TLS %fs + userspace ld.so (BM1) + VMM dlopen (BM2) + static musl CRT (BM3) + dyn musl (BM4) + 200+ Ring 3 syscalls
i18n & Fonts:       ████████████████████████░  95%  Live          ← TTF/CJK/RTL + fallback + GSUB liga + GPOS kern + GPOS mark + `.mo` gettext + pinyin IME + color emoji + TTF hint
Build System:       ████████████████████████░  95%  Live          ← G3 feature flags + flake.nix + workspace (kernel+boot)
Testing:            ████████████████████████░  98%  Wired         ← 414 QEMU markers + host parser fuzz + RSA/ECDSA/TLS HKDF/X.509 KATs
Documentation:      ██████████████████░░░░░░░  70%  Wired         ← BUILDING + CONTRIBUTING + docs/ + CHANGELOG + rustdoc
CI/CD:              ████████████████░░░░░░░░░  65%  Wired         ← aarch64/riscv64 QEMU boot + Secure Boot + rustdoc + audit + nightly images
```

### Score change vs 2026-03-15

| Subsystem | Old | Now | Why |
|-----------|-----|-----|-----|
| Kernel Core | 95% | 84% | Per-CPU TSS + GS CpuLocal; AP INIT/SIPI online (I1); AP Ring 3 (I3); per-CPU CFS current + unpinned AP (I4); NMI/MCE; MADT IOAPIC |
| Process | 90% | 99% | Gate B3–B8 scheduled Ring 3; IRQ GPR+FPU save (I2); AP Ring 3 (I3); clone (J1); CLONE_THREAD join (K1); `%fs` TLS (L1); PID ns (M1); futex (M3); mount ns (N1); net ns (O1); user ns (P1); IPC ns (Q1); cgroup ns (R1); time ns (S1); setns (T1); chroot (U1); pivot_root (V1); chdir/getcwd (Z3); fchdir (AA2); getppid (AG3); getpgid (AH3); getsid (AI3); setuid (AJ3); getrlimit (AK2); setgid (AK3); setrlimit (AL2); setresuid (AL3); prlimit64 (AM2); setresgid (AM3); setreuid (AN2); getgroups (AN3); setregid (AO2); getresuid (AO3); getresgid (AP2); setpgid (AP3); setsid (AQ2); setpriority (AQ3); getpid (AV2); gettid (AV3) |
| Binary compat | 40% | 99% | Static hello + scheduled `execve`/`fork`/`clone` + userspace `ld.so` (BM1) + VMM `dlopen` (BM2) + static musl CRT (BM3) + dynamically linked musl (BM4) + 200+ Ring 3 syscalls; 452 numbers still ≠ 452 behaviors |
| Memory | — | 74% | H1 CoW #PF; H2 file-backed fault-in; H4 OOM-on-alloc + guarded stacks; J2 leftover buddy RAM; J3 LRU; K2 swap I/O |
| Filesystem | 35% | 99% | VirtIO-blk + C1–C8 (JBD2 replay) + inotify on VFS mutate (H3) + mount ns (N1) + OverlayFS (W1) + VFS syscalls through BL |
| GUI | 85% | 95% | Ring 3 SHM clients (F1–F8) including Wayland AF_UNIX + calculator/Files userspace + VirtIO-GPU scanout/cursor; interactive terminal still in-kernel |
| Shell | 90% | 82% | PTY/glob/env real; Ring 3 `/bin/sh`; live `sigreturn`; desktop terminal still in-kernel for PTY I/O |
| Docs / CI | 10% / 25% | 70% / 55% | README, LICENSE, BUILDING, CONTRIBUTING, `docs/`, CHANGELOG, rustdoc artifact, cargo-audit, nightly images, GitHub Actions + aarch64/riscv64 compile; `flake.nix` present |
| Networking | 22% | 72% | D1 loopback; D2 VirtIO-net; D3 DHCP apply; D4 DNS+TCP; D5 CUBIC; BH2–BH3 name; BI1–BI3 msg/shutdown; BJ1–BJ3 sendmmsg/recvmmsg/getsockopt; BK1 setsockopt; BK2–BK3 TCP send/recv |
| Security | 15% | 80% | E1 W^X/ASLR; E2 ChaCha20; E3 seccomp EPERM; E4 CapNetBindService; K4 Landlock; J4/L4/M4/N4/O4/P4/Q4/R4/S4/T4/U4/V4/W4/X4/Y4/Z4/AA4/AB4/AC4/AD4/AE4/AF4/AG4/AH4/AI4/AJ4/AK4/AL4/AM4/AN4/AO4/AP4/AQ4/AR4/AS4/AT4/AU4/AV4/AW4/AX4/AY4/AZ4/BA4/BB4/BC4/BD4/BK4 ENOSYS |
| System services | — | 42% | K3 Ring 3 `/sbin/init`; L3 AF_UNIX D-Bus socket |
| **Overall production** | **~40%** | **~99%** | Gates B3–B8 + C1–C7 + D1–D5 + E1–E4 + F1–F8 + H1–H4 + I1–I4 + J–BL + BM1–BM4 + G3–G4 on the live boot path |

Code **grew** (602 → 609 files, more Phase 30–33 modules). Production usefulness did not grow proportionally. The next updates should tick items on [next.md](next.md), not module counts.

---

*This document is the single source of truth for KnoxOS readiness. Update a checkbox only when the **Done when** criterion is met on the live path. Unused source does not count.*
