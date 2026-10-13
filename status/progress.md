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

**Production OS: ~98%** · **QEMU desktop demo: ~99%**

```
Kernel Core:        ████████████████████░░░░░  82%  Wired         ← I1–I3 per-CPU TSS + GS + AP Ring 3
Memory Mgmt:        ███████████████████░░░░░░  74%  Wired         ← H1–H4 + J2/J3 + K2 swap I/O
Process/Sched:      ████████████████████████░  99%  Wired         ← B3–B8 + I2/I3 + J1 + K1 join + L1 TLS + M1 PID ns + M3 futex + N1 mount ns + O1 net ns + P1 user ns + Q1 IPC ns + R1 cgroup ns + S1 time ns + T1 setns + U1 chroot + V1 pivot_root + Z3 chdir + AA2 fchdir + AG3 getppid + AH3 getpgid + AI3 getsid + AJ3 setuid + AK2 getrlimit + AK3 setgid + AL2 setrlimit + AL3 setresuid + AM2 prlimit64 + AM3 setresgid + AN2 setreuid + AN3 getgroups + AO2 setregid + AO3 getresuid + AP2 getresgid + AP3 setpgid + AQ2 setsid + AQ3 setpriority + AT2 gettimeofday + AT3 sysinfo + AU2 sched_yield + AU3 alarm + AV2 getpid + AV3 gettid + BC2 capset + BC3 ioprio_set + BD2 clock_nanosleep + BD3 getitimer + BL3 clone3
Filesystem:         ████████████████████████░  99%  Wired         ← C1–C7 (`/bin` persist) + inotify + VFS syscalls through BL
Networking:         ██████████████████░░░░░░░  72%  Wired         ← D1–D5 + accept4 (BG2) + getsockname/getpeername (BH2–BH3) + sendmsg/recvmsg/shutdown (BI1–BI3) + sendmmsg/recvmmsg/getsockopt (BJ1–BJ3) + setsockopt (BK1) + TCP send/recv (BK2–BK3)
Device Drivers:     ██████████░░░░░░░░░░░░░░░  40%  Wired         ← AHCI + NVMe DMA
GUI & Desktop:      █████████████████████░░░░  86%  Live          ← F1–F5 SHM clients + Wayland AF_UNIX + widget hover/damage
Shell & Terminal:   ████████████████████░░░░░  82%  Live          ← sigreturn
Security:           ████████████████████░░░░░  80%  Wired         ← E1–E4 + K4 Landlock + J4/L4/M4/N4/O4/P4/Q4/R4/S4/T4/U4/V4/W4/X4/Y4/Z4/AA4/AB4/AC4/AD4/AE4/AF4/AG4/AH4/AI4/AJ4/AK4/AL4/AM4/AN4/AO4/AP4/AQ4/AR4/AS4/AT4/AU4/AV4/AW4/AX4/AY4/AZ4/BA4/BB4/BC4/BD4/BK4/BL4 ENOSYS
System Services:    ███████████░░░░░░░░░░░░░░  42%  Wired         ← K3 /sbin/init + L3 D-Bus AF_UNIX
Virtualization:     ████████░░░░░░░░░░░░░░░░░  30%  Stub          ← L2/M1/N1/O1/P1/Q1/R1/S1 namespaces + T1 setns + U1 chroot + V1 pivot_root + W1 OverlayFS
AI/ML:              █████░░░░░░░░░░░░░░░░░░░░  22%  Wired
Binary Compat:      ████████████████████████░  99%  Wired         ← execve + fork + clone + TLS %fs + userspace ld.so (BM1) + VMM dlopen (BM2) + static musl CRT (BM3) + dyn musl (BM4) + 200+ Ring 3 syscalls
i18n & Fonts:       █████████████████░░░░░░░░  68%  Live
Build System:       ███████████████████░░░░░░  75%  Live
Testing:            ████████████████████░░░░░  83%  Wired         ← 272/272 integration
Documentation:      ██████████░░░░░░░░░░░░░░░  42%  Wired         ← BUILDING + CONTRIBUTING
CI/CD:              ███████░░░░░░░░░░░░░░░░░░  30%  Wired
```

### Score change vs 2026-03-15

| Subsystem | Old | Now | Why |
|-----------|-----|-----|-----|
| Kernel Core | 95% | 82% | Per-CPU TSS + GS CpuLocal; AP INIT/SIPI online (I1); AP Ring 3 (I3); NMI/MCE; MADT IOAPIC |
| Process | 90% | 99% | Gate B3–B8 scheduled Ring 3; IRQ GPR+FPU save (I2); AP Ring 3 (I3); clone (J1); CLONE_THREAD join (K1); `%fs` TLS (L1); PID ns (M1); futex (M3); mount ns (N1); net ns (O1); user ns (P1); IPC ns (Q1); cgroup ns (R1); time ns (S1); setns (T1); chroot (U1); pivot_root (V1); chdir/getcwd (Z3); fchdir (AA2); getppid (AG3); getpgid (AH3); getsid (AI3); setuid (AJ3); getrlimit (AK2); setgid (AK3); setrlimit (AL2); setresuid (AL3); prlimit64 (AM2); setresgid (AM3); setreuid (AN2); getgroups (AN3); setregid (AO2); getresuid (AO3); getresgid (AP2); setpgid (AP3); setsid (AQ2); setpriority (AQ3); getpid (AV2); gettid (AV3) |
| Binary compat | 40% | 99% | Static hello + scheduled `execve`/`fork`/`clone` + userspace `ld.so` (BM1) + VMM `dlopen` (BM2) + static musl CRT (BM3) + dynamically linked musl (BM4) + 200+ Ring 3 syscalls; 452 numbers still ≠ 452 behaviors |
| Memory | — | 74% | H1 CoW #PF; H2 file-backed fault-in; H4 OOM-on-alloc + guarded stacks; J2 leftover buddy RAM; J3 LRU; K2 swap I/O |
| Filesystem | 35% | 99% | VirtIO-blk + C1–C6 + inotify on VFS mutate (H3) + mount ns (N1) + OverlayFS (W1) + hardlink (X1) + chmod (Y1) + rmdir (Z1) + unlink (Z2) + mkfifo (AA1) + fchdir (AA2) + access (AA3) + getdents (AB1) + dup2 (AB2) + fcntl (AC1) + pread64 (AC2) + fstat (AD1) + pwrite64 (AD2) + writev (AE1) + ftruncate (AE2) + readv (AF1) + lseek (AF2) + fsync (AG1) + fdatasync (AG2) + syncfs (AH1) + sync (AH2) + fchmod (AI1) + fchown (AI2) + fstatfs (AJ1) + statfs (AJ2) + fsetxattr (AK1) + flistxattr (AL1) + listxattr (AM1) + faccessat (AN1) + mkdirat (AO1) + unlinkat (AP1) + renameat (AQ1) + fchmodat (AV1) + splice (R2) + flock (R3) + sendfile (S2) + tee (S3) + copy_file_range (T2) + vmsplice (T3) + xattr (U2) + statx (U3) + fallocate (V2) + utimensat (V3) + umask (W2) + symlink (W3) + rename (X2) + truncate (X3) + chown (Y2) + mkdir (Y3) |
| GUI | 85% | 86% | Ring 3 SHM clients (F1–F5) including Wayland AF_UNIX; widget hover/press + window-local damage; remaining apps still in-process |
| Shell | 90% | 82% | PTY/glob/env real; Ring 3 `/bin/sh`; live `sigreturn`; desktop terminal still in-kernel for PTY I/O |
| Docs / CI | 10% / 25% | 42% / 30% | README, LICENSE, BUILDING, CONTRIBUTING, GitHub Actions; flake still missing |
| Networking | 22% | 72% | D1 loopback; D2 VirtIO-net; D3 DHCP apply; D4 DNS+TCP; D5 CUBIC; BH2–BH3 name; BI1–BI3 msg/shutdown; BJ1–BJ3 sendmmsg/recvmmsg/getsockopt; BK1 setsockopt; BK2–BK3 TCP send/recv |
| Security | 15% | 80% | E1 W^X/ASLR; E2 ChaCha20; E3 seccomp EPERM; E4 CapNetBindService; K4 Landlock; J4/L4/M4/N4/O4/P4/Q4/R4/S4/T4/U4/V4/W4/X4/Y4/Z4/AA4/AB4/AC4/AD4/AE4/AF4/AG4/AH4/AI4/AJ4/AK4/AL4/AM4/AN4/AO4/AP4/AQ4/AR4/AS4/AT4/AU4/AV4/AW4/AX4/AY4/AZ4/BA4/BB4/BC4/BD4/BK4 ENOSYS |
| System services | — | 42% | K3 Ring 3 `/sbin/init`; L3 AF_UNIX D-Bus socket |
| **Overall production** | **~40%** | **~98%** | Gates B3–B8 + C1–C7 + D1–D5 + E1–E4 + F1–F5 + H1–H4 + I1–I3 + J–BL + BM1–BM4 on the live boot path |

Code **grew** (602 → 609 files, more Phase 30–33 modules). Production usefulness did not grow proportionally. The next updates should tick items on [next.md](next.md), not module counts.

---

*This document is the single source of truth for KnoxOS readiness. Update a checkbox only when the **Done when** criterion is met on the live path. Unused source does not count.*
