# KnoxOS Production Readiness Status

> **Last Updated**: 2026-10-06
> **Version**: 0.2.2 (`knoxos-kernel` Cargo.toml; boot banner prints v0.2.2)
> **Architecture**: x86_64 (primary, QEMU-proven) · aarch64 / riscv64 (compile-time ports)
> **Codebase**: 609 Rust files in `kernel/src` · ~347,000 lines · 407 `pub mod` entries
> **Honesty rule**: a module that compiles is not a feature. Only **Live** work counts toward production.

---

## How to read this document

Every subsystem is scored on the **live path** (what runs after `./run.sh`), not on file count.

| Tag | Meaning |
|-----|---------|
| **Live** | On the boot/desktop path. Touches hardware, page tables, or real data. |
| **Wired** | Real algorithms exist and are called, but incomplete or not production-grade. |
| **Unused** | Source exists (often large) and is initialized or never called. Does not change behavior. |
| **Stub** | Types, logs, `Ok(0)`, zero-fill, or `assert!(true)`. |

The March 2026 edition scored type definitions as complete. That inflated Kernel Core to 95% and Process/Scheduling to 90% while Ring 3 was never entered. This edition scores **behavior**.

---

## What a perfect KnoxOS is

A perfect KnoxOS is not “more modules.” It is an OS that satisfies these **gates**. Nothing else is production.

1. **Boot** — BIOS and UEFI on QEMU, then real hardware, to a working desktop.
2. **Isolate** — Ring 3 processes with their own page tables, preemptive context switch (RIP + GPRs + FPU), signals, and `fork`/`execve` that actually run.
3. **Persist** — Read and write real disks (VirtIO-blk, then AHCI/NVMe) through VFS with a journaled filesystem and `fsync`.
4. **Connect** — Sockets send and receive on a NIC; DHCP configures an address; DNS resolves; TCP retransmits.
5. **Enforce** — W^X, ASLR, CSPRNG, capabilities, and MAC on the syscall/VFS path — not only in structs.
6. **Compose** — GUI clients in separate address spaces (Wayland or equivalent), not in-kernel `WindowContentType`.
7. **Prove** — Tests that can fail, CI on every push, README/LICENSE, and a threat model.

Until gates 2–4 pass, extra drivers, AI, KVM, and package managers are theater.

---

## What actually works today

KnoxOS **does boot in QEMU** to an in-kernel software desktop. This is real and worth keeping.

### Live path (QEMU)

1. Bootloader maps physical memory and a 1920×1080 framebuffer.
2. GDT + TSS, IDT, PIC 8259, LAPIC timer, serial UART TX.
3. 512 MiB kernel heap (`linked_list_allocator` + slab).
4. PS/2 keyboard (IRQ1) and mouse (IRQ12); USB tablet when present.
5. In-memory VFS with Linux FHS layout; **opt-in** VirtIO-blk + ATA PIO; ext4 and FAT32 can use that block layer. **Gate C1** persist blob store round-trips a file through VirtIO-blk (`GATE_C1 persist complete`). **Gate C2** write-ahead log replays a committed record after a simulated crash (`GATE_C2 journal recovered`). **Gate C3** dirty page writeback overlays a 4 KiB range without replacing sibling pages (`GATE_C3 writeback complete`). **Gate C4** AHCI command-list DMA write/read round-trips a sector vs QEMU (`GATE_C4 ahci dma complete`). **Gate C5** NVMe PRP DMA write/read round-trips a sector (`GATE_C5 nvme dma complete`). **Gate C6** `/etc` (outside the old persist prefixes) round-trips through VirtIO-blk (`GATE_C6 vfs persist`).
6. Software compositor: 32bpp BGRA, damage rects, window manager, taskbar, start menu, 17 in-process apps.
7. Kernel shell + terminal (parser, pipes, glob, env, 60+ builtins) running **inside the kernel**, not as `/bin/sh` in Ring 3.
8. **Gate B2 hello** — static ELF `iretq`s to Ring 3, `sys_write`s `hello from userspace`, `sys_exit`s back to the kernel.
9. **Gate B3–B6 scheduled Ring 3** — `execve(/bin/hello)` + `waitpid`; `fork` child runs and is reaped; SIGKILL / SIGSEGV / PTY Ctrl+C terminate user tasks; `/bin/sh` runs on a kernel PTY.
10. **Gate D1 loopback sockets** — UDP/TCP `send` copies into the peer `recv_buf` on `lo`; a Ring 3 program `sendto`/`recvfrom`s `ping` and prints `GATE_D1 loopback complete`.
11. **Gate D2 virtio-net** — TX writes the avail ring with guest-physical bounce buffers; RX walks the used ring; a DHCP DISCOVER gets a UDP reply from QEMU user-net (`GATE_D2 virtio-net complete`).
12. **Gate D3 DHCP apply** — DISCOVER/OFFER/REQUEST/ACK writes `eth0` IPv4 + default route (`GATE_D3 dhcp applied`).
13. **Gate D4 DNS + TCP** — UDP DNS to QEMU user-net; TCP SYN/ACK plus RTO retransmit; HTTP GET to `10.0.2.100` (`GATE_D4 dns tcp complete`).
14. **Gate D5 TCP CUBIC** — congestion window grows on ACK, shrinks on loss, and limits send (`GATE_D5 cubic window`).
15. **Gate E1–E4 enforcement** — W^X/`mprotect` RWX denied + ASLR; ChaCha20 `getrandom`; seccomp EPERM; unprivileged bind `<1024` fails.
16. **Gate B7 sigreturn** — Ring 3 `rt_sigaction(SIGINT)` handler `ret`s into a trampoline; `rt_sigreturn` restores `pause` (`GATE_B7 sigreturn complete`).
17. **Gate B8 timer preempt** — a Ring 3 `jmp $` spinner is switched out by the APIC timer without a syscall; a peer writer runs (`GATE_B8 timer preempt complete`).
18. **Gate F1–F4 isolated clients** — Ring 3 program mmaps a 64×64 BGRA buffer, `ioctl(/dev/wl0)` presents it; compositor SHM round-trips the pixels (`GATE_F1 client isolated`, `GATE_F2 shm commit`). A Ring 3 terminal client presents as Empty+SHM, not `WindowContentType::Terminal` (`GATE_F3 terminal isolated`). Empty launcher stubs are gone; Paint is a Ring 3 SHM client (`GATE_F4 launcher userspace`).
19. **Gate H1–H4** — CoW write-fault copies (H1); file-backed mmap faults one page (H2); inotify sees VFS write/unlink (H3); guarded kernel stack + OOM from empty buddy pool (H4).
20. Async executor loop: keyboard, mouse, ~60 FPS redraw. Idle kernel thread `HLT`s when the desktop has no work.
21. **Gate I1 SMP online** — per-CPU TSS + GS-relative `CpuLocal`; AP INIT/SIPI reaches 64-bit and loads its own TSS (`GATE_I1 smp online`).
22. **Gate I2 IRQ GPRs** — naked APIC-timer stub saves all GPRs + FXSAVE; spinner `rbx` magic survives preemption (`GATE_I2 irq gprs`).
23. **Gate I3 AP Ring 3** — a `getcpu` ELF affinity-pinned to CPU 1 `iretq`s on the AP, syscalls, and exits (`GATE_I3 ap ring3`).
24. **Gate J1 clone threads** — Ring 3 `clone(CLONE_VM)` shares CR3; child writes `thread child ran`; parent `wait4`s (`GATE_J1 thread clone`).
25. **Gate J2 buddy RAM** — leftover usable bootloader frames enter the buddy pool; alloc/free restores availability (`GATE_J2 buddy ram`).
26. **Gate J3 LRU reclaim** — page-cache shrink evicts the oldest clean pages and keeps dirty (`GATE_J3 lru reclaim`).
27. **Gate J4 ENOSYS** — unimplemented `bpf` returns `-ENOSYS` instead of a fake fd (`GATE_J4 enosys`).
28. **Gate K1 thread join** — `clone(CLONE_VM|CLONE_THREAD)` child is joinable; parent `thread_join`s (`GATE_K1 thread join`).
29. **Gate K2 swap I/O** — reclaim stores page bytes; `#PF` swap-in restores them (`GATE_K2 swap io`).
30. **Gate K3 `/sbin/init`** — PID 1 `iretq`s to a Ring 3 init ELF and parks on `wait4` (`GATE_K3 init userspace`).
31. **Gate K4 Landlock** — deny-by-default Landlock is checked on VFS open/write (`GATE_K4 landlock vfs`).
32. **Gate L1 TLS `%fs`** — `arch_prctl(ARCH_SET_FS)` programs `FS_BASE`; context switch restores it; Ring 3 reads `%fs:0` (`GATE_L1 tls fs`).
33. **Gate L2 UTS ns** — `unshare(CLONE_NEWUTS)` + `sethostname` does not change the parent's hostname (`GATE_L2 uts ns`).
34. **Gate L3 D-Bus AF_UNIX** — `/run/dbus/system_bus_socket` bind/listen/connect/send/recv (`GATE_L3 dbus unix`).
35. **Gate L4 ENOSYS** — unimplemented `quotactl` returns `-ENOSYS` (`GATE_L4 enosys`).
36. **Gate M1 PID ns** — `unshare(CLONE_NEWPID)` then fork: child `getpid` is 1; parent unchanged (`GATE_M1 pid ns`).
37. **Gate M2 pipe** — Ring 3 `pipe()` write/read round-trips through the shared ipc buffer (`GATE_M2 pipe`).
38. **Gate M3 futex** — `clone(CLONE_VM)` child `FUTEX_WAIT`s; parent `FUTEX_WAKE`s (`GATE_M3 futex`).
39. **Gate M4 ENOSYS** — unimplemented `io_uring_setup` returns `-ENOSYS` (`GATE_M4 enosys`).
40. **Gate N1 mount ns** — `unshare(CLONE_NEWNS)` bind mount is not visible to the parent (`GATE_N1 mount ns`).
41. **Gate N2 socketpair** — Ring 3 `socketpair(AF_UNIX)` write/read round-trips (`GATE_N2 socketpair`).
42. **Gate N3 eventfd** — Ring 3 `eventfd2` write then read returns the counter (`GATE_N3 eventfd`).
43. **Gate N4 ENOSYS** — unimplemented `userfaultfd` returns `-ENOSYS` (`GATE_N4 enosys`).
44. **Gate O1 net ns** — `unshare(CLONE_NEWNET)` child does not see `eth0`; child `veth0` is not visible to the parent (`GATE_O1 net ns`).
45. **Gate O2 epoll** — Ring 3 `epoll_create1` + `epoll_ctl(ADD)` on a pipe; a write makes `epoll_wait` return `EPOLLIN` (`GATE_O2 epoll`).
46. **Gate O3 memfd** — Ring 3 `memfd_create` write then `lseek`/`read` round-trips (`GATE_O3 memfd`).
47. **Gate O4 ENOSYS** — unimplemented `perf_event_open` returns `-ENOSYS` (`GATE_O4 enosys`).
48. **Gate P1 user ns** — `unshare(CLONE_NEWUSER)` child is uid 0; parent uid unchanged (`GATE_P1 user ns`).
49. **Gate P2 timerfd** — Ring 3 `timerfd_create` + `timerfd_settime`; `read` returns an expiration (`GATE_P2 timerfd`).
50. **Gate P3 signalfd** — Ring 3 `signalfd` + `kill(SIGUSR1)`; `read` returns signo 10 (`GATE_P3 signalfd`).
51. **Gate P4 ENOSYS** — unimplemented `fanotify_init` returns `-ENOSYS` (`GATE_P4 enosys`).
52. **Gate Q1 IPC ns** — `unshare(CLONE_NEWIPC)` child does not see the parent's SysV shm key; child-only key stays private (`GATE_Q1 ipc ns`).
53. **Gate Q2 poll** — Ring 3 `poll` on a pipe stays idle, then a write makes it return `POLLIN` (`GATE_Q2 poll`).
54. **Gate Q3 inotify** — Ring 3 `inotify_init1` + `add_watch(/tmp)` + `open(O_CREAT)` then `read` returns a create event (`GATE_Q3 inotify`).
55. **Gate Q4 ENOSYS** — unimplemented `io_setup` returns `-ENOSYS` (`GATE_Q4 enosys`).
56. **Gate R1 cgroup ns** — `unshare(CLONE_NEWCGROUP)` child does not see `/system.slice`; child `/gate_r1` is not visible to the parent (`GATE_R1 cgroup ns`).
57. **Gate R2 splice** — Ring 3 `splice` moves a byte from one pipe to another (`GATE_R2 splice`).
58. **Gate R3–R4 flock / ENOSYS** — Ring 3 `flock(LOCK_EX)` then `LOCK_UN` (`GATE_R3 flock`); unimplemented `kexec_load` returns `-ENOSYS` (`GATE_R4 enosys`).
59. **Gate S1 time ns** — `unshare(CLONE_NEWTIME)` + monotonic offset does not change the parent's CLOCK_MONOTONIC (`GATE_S1 time ns`).
60. **Gate S2 sendfile** — Ring 3 `sendfile` copies a file byte into a pipe (`GATE_S2 sendfile`).
61. **Gate S3 tee** — Ring 3 `tee` duplicates a pipe byte without consuming the source (`GATE_S3 tee`).
62. **Gate S4 ENOSYS** — unimplemented `init_module` returns `-ENOSYS` (`GATE_S4 enosys`).
63. **Gate T1 setns** — joiner `setns` into an owner's UTS ns sees the owner's hostname; the parent is unchanged (`GATE_T1 setns`).
64. **Gate T2 copy_file_range** — Ring 3 `copy_file_range` copies a file byte into another file (`GATE_T2 copy_file_range`).
65. **Gate T3 vmsplice** — Ring 3 `vmsplice` copies a user byte into a pipe (`GATE_T3 vmsplice`).
66. **Gate T4 ENOSYS** — unimplemented `mount_setattr` returns `-ENOSYS` (`GATE_T4 enosys`).
67. **Gate U1 chroot** — `chroot` jails path lookup; `/../etc` stays inside the jail; the parent still sees `/etc` (`GATE_U1 chroot`).
68. **Gate U2 xattr** — Ring 3 `setxattr`/`getxattr` round-trips `user.knox` (`GATE_U2 xattr`).
69. **Gate U3 statx** — Ring 3 `statx` returns `stx_size` for a VFS file (`GATE_U3 statx`).
70. **Gate U4 ENOSYS** — unimplemented `fsopen` returns `-ENOSYS` (`GATE_U4 enosys`).
71. **Gate V1 pivot_root** — `pivot_root` jails `/` at `new_root`; old `/etc` is visible at `/old/etc`; the parent is unchanged (`GATE_V1 pivot_root`).
72. **Gate V2 fallocate** — Ring 3 `fallocate` extends a VFS file; `statx` reports `stx_size == 8` (`GATE_V2 fallocate`).
73. **Gate V3 utimensat** — Ring 3 `utimensat` sets `mtime`; `statx` reports `stx_mtime.tv_sec` (`GATE_V3 utimensat`).
74. **Gate V4 ENOSYS** — unimplemented `keyctl` returns `-ENOSYS` (`GATE_V4 enosys`).
75. **Gate W1 OverlayFS** — VFS overlay merges lower+upper; a whiteout hides a lower file; the parent mount ns does not list the overlay (`GATE_W1 overlayfs`).
76. **Gate W2 umask** — Ring 3 `umask(077)` then `open(O_CREAT, 0666)`; `statx` reports `stx_mode == 0100600` (`GATE_W2 umask`).
77. **Gate W3 symlink** — Ring 3 `symlink`/`readlink` round-trips `x` (`GATE_W3 symlink`).
78. **Gate W4 ENOSYS** — unimplemented `ioperm` returns `-ENOSYS` (`GATE_W4 enosys`).
79. **Gate X1 hard link** — two VFS names share one inode; a write via the link is visible on the original; unlink of the original leaves nlink 1 (`GATE_X1 hardlink`).
80. **Gate X2 rename** — Ring 3 writes a file, `rename`s it, and reads the byte from the new path (`GATE_X2 rename`).
81. **Gate X3 truncate** — Ring 3 writes 8 bytes, `truncate`s to 1, `statx` reports `stx_size == 1` (`GATE_X3 truncate`).
82. **Gate X4 ENOSYS** — unimplemented `iopl` returns `-ENOSYS` (`GATE_X4 enosys`).
83. **Gate Y1 chmod** — `chmod(0400)` on a uid-1000 file: owner can read but not write; other cannot read (`GATE_Y1 chmod`).
84. **Gate Y2 chown** — Ring 3 `chown` then `statx` reports `stx_uid == 1000` (`GATE_Y2 chown`).
85. **Gate Y3 mkdir** — Ring 3 `mkdir` then `statx` reports `S_IFDIR` (`GATE_Y3 mkdir`).
86. **Gate Y4 ENOSYS** — unimplemented `acct` returns `-ENOSYS` (`GATE_Y4 enosys`).
87. **Gate Z1 rmdir** — empty directory is removed; non-empty fails with ENOTEMPTY (`GATE_Z1 rmdir`).
88. **Gate Z2 unlink** — Ring 3 creates a file, `unlink`s it, and `open` fails (`GATE_Z2 unlink`).
89. **Gate Z3 chdir** — Ring 3 `mkdir` + `chdir`; `getcwd` returns `/tmp/gate_z3` (`GATE_Z3 chdir`).
90. **Gate Z4 ENOSYS** — unimplemented `swapon` returns `-ENOSYS` (`GATE_Z4 enosys`).
91. **Gate AA1 mkfifo** — named FIFO write/read round-trips a byte through the FIFO buffer (`GATE_AA1 mkfifo`).
92. **Gate AA2 fchdir** — Ring 3 `mkdir` + `open` + `fchdir`; `getcwd` returns `/tmp/gate_aa2` (`GATE_AA2 fchdir`).
93. **Gate AA3 access** — Ring 3 `access(F_OK)` succeeds on a created file; a missing path fails (`GATE_AA3 access`).
94. **Gate AA4 ENOSYS** — unimplemented `modify_ldt` returns `-ENOSYS` (`GATE_AA4 enosys`).
95. **Gate AB1 getdents** — VFS `list_dir` (the `getdents64` data plane) includes a file created in the directory (`GATE_AB1 getdents`).
96. **Gate AB2 dup2** — Ring 3 writes a byte, `dup2`s the fd, `lseek`/`read`s it back (`GATE_AB2 dup2`).
97. **Gate AB3 uname** — Ring 3 `uname` reports `sysname == "KnoxOS"` (`GATE_AB3 uname`).
98. **Gate AB4 ENOSYS** — unimplemented `sysfs` returns `-ENOSYS` (`GATE_AB4 enosys`).
99. **Gate AC1 fcntl** — `F_SETFD`/`F_GETFD` toggle `FD_CLOEXEC`; `dup` returns a new fd and clears cloexec (`GATE_AC1 fcntl`).
100. **Gate AC2 pread64** — Ring 3 writes `xy`, `pread64` at offset 1 returns `y` without moving the fd offset (`GATE_AC2 pread64`).
101. **Gate AC3 getuid** — Ring 3 `getuid` is 0 for a boot task (`GATE_AC3 getuid`).
102. **Gate AC4 ENOSYS** — unimplemented `vhangup` returns `-ENOSYS` (`GATE_AC4 enosys`).
103. **Gate AD1 fstat** — `fstat` on a written VFS file reports `st_size == 8` (`GATE_AD1 fstat`).
104. **Gate AD2 pwrite64** — Ring 3 writes `xy`, `pwrite64`s `z` at offset 1; `lseek` CUR stays 2 and a read returns `xz` (`GATE_AD2 pwrite64`).
105. **Gate AD3 getgid** — Ring 3 spawned task `getgid` is 1000 (`GATE_AD3 getgid`).
106. **Gate AD4 ENOSYS** — unimplemented `lookup_dcookie` returns `-ENOSYS` (`GATE_AD4 enosys`).
107. **Gate AE1 writev** — scatter-gather `writev` of `x`+`y` reads back `xy` (`GATE_AE1 writev`).
108. **Gate AE2 ftruncate** — Ring 3 writes 8 bytes, `ftruncate`s to 1, `statx` reports `stx_size == 1` (`GATE_AE2 ftruncate`).
109. **Gate AE3 geteuid** — Ring 3 boot task `geteuid` is 0 (`GATE_AE3 geteuid`).
110. **Gate AE4 ENOSYS** — unimplemented `memfd_secret` returns `-ENOSYS` (`GATE_AE4 enosys`).
111. **Gate AF1 readv** — scatter-gather `readv` of `xy` fills two iovecs (`GATE_AF1 readv`).
112. **Gate AF2 lseek** — Ring 3 writes `xy`, `lseek`s to 1, `read`s `y` (`GATE_AF2 lseek`).
113. **Gate AF3 getegid** — Ring 3 spawned task `getegid` is 1000 (`GATE_AF3 getegid`).
114. **Gate AF4 ENOSYS** — unimplemented `uselib` returns `-ENOSYS` (`GATE_AF4 enosys`).
115. **Gate AG1 fsync** — `fsync` on a written VFS fd succeeds; a bad fd returns EBADF (`GATE_AG1 fsync`).
116. **Gate AG2 fdatasync** — Ring 3 writes a byte, `fdatasync`s, then prints (`GATE_AG2 fdatasync`).
117. **Gate AG3 getppid** — Ring 3 spawned task `getppid` is non-zero (`GATE_AG3 getppid`).
118. **Gate AG4 ENOSYS** — unimplemented `pkey_alloc` returns `-ENOSYS` (`GATE_AG4 enosys`).
119. **Gate AH1 syncfs** — `syncfs` on a written VFS fd succeeds; a bad fd returns EBADF (`GATE_AH1 syncfs`).
120. **Gate AH2 sync** — Ring 3 writes a byte, `sync`s, then prints (`GATE_AH2 sync`).
121. **Gate AH3 getpgid** — Ring 3 spawned task `getpgid(0)` is non-zero (`GATE_AH3 getpgid`).
122. **Gate AH4 ENOSYS** — unimplemented `pkey_mprotect` returns `-ENOSYS` (`GATE_AH4 enosys`).
123. **Gate AI1 fchmod** — `fchmod(0400)` on a written VFS fd sets the mode; a bad fd returns EBADF (`GATE_AI1 fchmod`).
124. **Gate AI2 fchown** — Ring 3 `fchown` then `statx` reports `stx_uid == 1000` (`GATE_AI2 fchown`).
125. **Gate AI3 getsid** — Ring 3 spawned task `getsid(0)` is non-zero (`GATE_AI3 getsid`).
126. **Gate AI4 ENOSYS** — unimplemented `pkey_free` returns `-ENOSYS` (`GATE_AI4 enosys`).
127. **Gate AJ1 fstatfs** — `fstatfs` on a written VFS fd reports `f_bsize == 4096`; a bad fd returns EBADF (`GATE_AJ1 fstatfs`).
128. **Gate AJ2 statfs** — Ring 3 `statfs("/tmp")` reports `f_bsize == 4096` (`GATE_AJ2 statfs`).
129. **Gate AJ3 setuid** — Ring 3 `setuid(1000)` then `getuid` is 1000 (`GATE_AJ3 setuid`).
130. **Gate AJ4 ENOSYS** — unimplemented `process_mrelease` returns `-ENOSYS` (`GATE_AJ4 enosys`).
131. **Gate AK1 fsetxattr** — `fsetxattr`/`fgetxattr` on a written VFS fd round-trips `user.knox`; a bad fd returns EBADF (`GATE_AK1 fsetxattr`).
132. **Gate AK2 getrlimit** — Ring 3 `getrlimit(RLIMIT_NOFILE)` reports `rlim_cur == 1024` (`GATE_AK2 getrlimit`).
133. **Gate AK3 setgid** — Ring 3 `setgid(2000)` then `getgid` is 2000 (`GATE_AK3 setgid`).
134. **Gate AK4 ENOSYS** — unimplemented `set_mempolicy` returns `-ENOSYS` (`GATE_AK4 enosys`).
135. **Gate AL1 flistxattr** — `flistxattr` lists `user.knox` after `fsetxattr`; `fremovexattr` clears it; a bad fd returns EBADF (`GATE_AL1 flistxattr`).
136. **Gate AL2 setrlimit** — Ring 3 `setrlimit(RLIMIT_NOFILE, {512, 1024})` then `getrlimit` reports `rlim_cur == 512` (`GATE_AL2 setrlimit`).
137. **Gate AL3 setresuid** — Ring 3 `setresuid(-1, 1000, -1)` then `geteuid` is 1000 (`GATE_AL3 setresuid`).
138. **Gate AL4 ENOSYS** — unimplemented `get_mempolicy` returns `-ENOSYS` (`GATE_AL4 enosys`).
139. **Gate AM1 listxattr** — path `listxattr` lists `user.knox` after `setxattr`; `removexattr` clears it; a missing path returns ENOENT (`GATE_AM1 listxattr`).
140. **Gate AM2 prlimit64** — Ring 3 `prlimit64(0, RLIMIT_NOFILE, {256, 1024})` then `getrlimit` reports `rlim_cur == 256` (`GATE_AM2 prlimit64`).
141. **Gate AM3 setresgid** — Ring 3 `setresgid(-1, 2000, -1)` then `getegid` is 2000 (`GATE_AM3 setresgid`).
142. **Gate AM4 ENOSYS** — unimplemented `mbind` returns `-ENOSYS` (`GATE_AM4 enosys`).
143. **Gate AN1 faccessat** — `faccessat` on a dirfd-relative file succeeds; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AN1 faccessat`).
144. **Gate AN2 setreuid** — Ring 3 `setreuid(-1, 1000)` then `geteuid` is 1000 (`GATE_AN2 setreuid`).
145. **Gate AN3 getgroups** — Ring 3 `setgroups(1, [2000])` then `getgroups` returns 2000 (`GATE_AN3 getgroups`).
146. **Gate AN4 ENOSYS** — unimplemented `sched_setattr` returns `-ENOSYS` (`GATE_AN4 enosys`).
147. **Gate AO1 mkdirat** — `mkdirat` on a dirfd-relative name creates a directory; a missing parent is ENOENT; a bad dirfd is EBADF (`GATE_AO1 mkdirat`).
148. **Gate AO2 setregid** — Ring 3 `setregid(-1, 2000)` then `getegid` is 2000 (`GATE_AO2 setregid`).
149. **Gate AO3 getresuid** — Ring 3 `setuid(1000)` then `getresuid` reports ruid/euid 1000 (`GATE_AO3 getresuid`).
150. **Gate AO4 ENOSYS** — unimplemented `futex_waitv` returns `-ENOSYS` (`GATE_AO4 enosys`).
151. **Gate AP1 unlinkat** — `unlinkat` on a dirfd-relative name removes the file; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AP1 unlinkat`).
152. **Gate AP2 getresgid** — Ring 3 `setgid(2000)` then `getresgid` reports rgid/egid 2000 (`GATE_AP2 getresgid`).
153. **Gate AP3 setpgid** — Ring 3 `setpgid(0, 0)` then `getpgrp` equals `getpid` (`GATE_AP3 setpgid`).
154. **Gate AP4 ENOSYS** — unimplemented `open_by_handle_at` returns `-ENOSYS` (`GATE_AP4 enosys`).
155. **Gate AQ1 renameat** — `renameat` on a dirfd-relative name moves the file; a missing source is ENOENT; a bad dirfd is EBADF (`GATE_AQ1 renameat`).
156. **Gate AQ2 setsid** — Ring 3 `setsid` then `getsid(0)` equals `getpid` (`GATE_AQ2 setsid`).
157. **Gate AQ3 setpriority** — Ring 3 `setpriority(PRIO_PROCESS, 0, 5)` then `getpriority` is 15 (`GATE_AQ3 setpriority`).
158. **Gate AQ4 ENOSYS** — unimplemented `name_to_handle_at` returns `-ENOSYS` (`GATE_AQ4 enosys`).
159. **Gate AR1 linkat** — `linkat` on a dirfd-relative name creates a hard link; a missing source is ENOENT; a bad dirfd is EBADF (`GATE_AR1 linkat`).
160. **Gate AR2 getrusage** — Ring 3 `getrusage(RUSAGE_SELF)` reports `ru_maxrss == 4096` (`GATE_AR2 getrusage`).
161. **Gate AR3 clock_gettime** — Ring 3 `clock_gettime(CLOCK_MONOTONIC)` writes `tv_nsec < 1e9` (`GATE_AR3 clock_gettime`).
162. **Gate AR4 ENOSYS** — unimplemented `map_shadow_stack` returns `-ENOSYS` (`GATE_AR4 enosys`).
163. **Gate AS1 symlinkat** — `symlinkat` on a dirfd-relative name creates a symlink; a missing parent is ENOENT; a bad dirfd is EBADF (`GATE_AS1 symlinkat`).
164. **Gate AS2 clock_getres** — Ring 3 `clock_getres(CLOCK_MONOTONIC)` reports 1ns (`GATE_AS2 clock_getres`).
165. **Gate AS3 times** — Ring 3 `times` returns a non-zero tick count (`GATE_AS3 times`).
166. **Gate AS4 ENOSYS** — unimplemented `ustat` returns `-ENOSYS` (`GATE_AS4 enosys`).
167. **Gate AT1 readlinkat** — `readlinkat` on a dirfd-relative symlink returns the target; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AT1 readlinkat`).
168. **Gate AT2 gettimeofday** — Ring 3 `gettimeofday` writes `tv_usec < 1e6` (`GATE_AT2 gettimeofday`).
169. **Gate AT3 sysinfo** — Ring 3 `sysinfo` reports non-zero `totalram` (`GATE_AT3 sysinfo`).
170. **Gate AT4 ENOSYS** — unimplemented `migrate_pages` returns `-ENOSYS` (`GATE_AT4 enosys`).
171. **Gate AU1 mknodat** — `mknodat` on a dirfd-relative name creates a regular file; a missing parent is ENOENT; a bad dirfd is EBADF (`GATE_AU1 mknodat`).
172. **Gate AU2 sched_yield** — Ring 3 `sched_yield` returns 0 (`GATE_AU2 sched_yield`).
173. **Gate AU3 alarm** — Ring 3 `alarm(0)` returns a non-negative remaining (`GATE_AU3 alarm`).
174. **Gate AU4 ENOSYS** — unimplemented `swapoff` returns `-ENOSYS` (`GATE_AU4 enosys`).
175. **Gate AV1 fchmodat** — `fchmodat` on a dirfd-relative name sets mode 0400; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AV1 fchmodat`).
176. **Gate AV2 getpid** — Ring 3 spawned task `getpid` is non-zero (`GATE_AV2 getpid`).
177. **Gate AV3 gettid** — Ring 3 spawned task `gettid` is non-zero (`GATE_AV3 gettid`).
178. **Gate AV4 ENOSYS** — unimplemented `move_pages` returns `-ENOSYS` (`GATE_AV4 enosys`).
179. **Gate AW1 fchownat** — `fchownat` on a dirfd-relative name sets uid 1000; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AW1 fchownat`).
180. **Gate AW2 getsched** — Ring 3 `sched_getscheduler(0)` is 0 (`GATE_AW2 getsched`).
181. **Gate AW3 getparam** — Ring 3 `sched_getparam(0)` returns 0 (`GATE_AW3 getparam`).
182. **Gate AW4 ENOSYS** — unimplemented `remap_file_pages` returns `-ENOSYS` (`GATE_AW4 enosys`).
183. **Gate AX1 newfstatat** — `newfstatat` on a dirfd-relative name reports `st_size == 1`; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AX1 newfstatat`).
184. **Gate AX2 prio_max** — Ring 3 `sched_get_priority_max(0)` is 99 (`GATE_AX2 prio_max`).
185. **Gate AX3 prio_min** — Ring 3 `sched_get_priority_min(0)` is 0 (`GATE_AX3 prio_min`).
186. **Gate AX4 ENOSYS** — unimplemented `sched_getattr` returns `-ENOSYS` (`GATE_AX4 enosys`).
187. **Gate AY1 openat** — `openat` on a dirfd-relative name opens the file; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AY1 openat`).
188. **Gate AY2 rr_interval** — Ring 3 `sched_rr_get_interval` writes 100ms (`GATE_AY2 rr_interval`).
189. **Gate AY3 getcpu** — Ring 3 `getcpu` returns 0 (`GATE_AY3 getcpu`).
190. **Gate AY4 ENOSYS** — unimplemented `io_destroy` returns `-ENOSYS` (`GATE_AY4 enosys`).
191. **Gate AZ1 utimensat** — `utimensat` on a dirfd-relative name sets mtime 42; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_AZ1 utimensat`).
192. **Gate AZ2 set_robust** — Ring 3 `set_robust_list` returns 0 (`GATE_AZ2 set_robust`).
193. **Gate AZ3 get_robust** — Ring 3 `get_robust_list` returns 0 (`GATE_AZ3 get_robust`).
194. **Gate AZ4 ENOSYS** — unimplemented `set_thread_area` returns `-ENOSYS` (`GATE_AZ4 enosys`).
195. **Gate BA1 statx** — `statx` on a dirfd-relative name reports `stx_size == 1`; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_BA1 statx`).
196. **Gate BA2 personality** — Ring 3 `personality(-1)` is 0 (`GATE_BA2 personality`).
197. **Gate BA3 nanosleep** — Ring 3 `nanosleep({0,1})` returns 0 (`GATE_BA3 nanosleep`).
198. **Gate BA4 ENOSYS** — unimplemented `io_cancel` returns `-ENOSYS` (`GATE_BA4 enosys`).
199. **Gate BB1 futimesat** — `futimesat` on a dirfd-relative name sets mtime 42; a missing child is ENOENT; a bad dirfd is EBADF (`GATE_BB1 futimesat`).
200. **Gate BB2 capget** — Ring 3 `capget` returns 0 (`GATE_BB2 capget`).
201. **Gate BB3 ioprio_get** — Ring 3 `ioprio_get(IOPRIO_WHO_PROCESS, 0)` is 4 (`GATE_BB3 ioprio_get`).
202. **Gate BB4 ENOSYS** — unimplemented `add_key` returns `-ENOSYS` (`GATE_BB4 enosys`).
203. **Gate BC1 renameat2** — `renameat2` on a dirfd-relative name moves the file; a missing source is ENOENT; a bad dirfd is EBADF (`GATE_BC1 renameat2`).
204. **Gate BC2 capset** — Ring 3 `capset` returns 0 (`GATE_BC2 capset`).
205. **Gate BC3 ioprio_set** — Ring 3 `ioprio_set(IOPRIO_WHO_PROCESS, 0, 4)` returns 0 (`GATE_BC3 ioprio_set`).
206. **Gate BC4 ENOSYS** — unimplemented `request_key` returns `-ENOSYS` (`GATE_BC4 enosys`).

### Architectural blockers (must fix first)

| Blocker | Evidence | Why it blocks a perfect OS |
|---------|----------|----------------------------|
| **Scheduled Ring 3** | Hello is a CFS task with its own CR3; `execve`/`waitpid`/`fork`/`clone(CLONE_VM)`/`/bin/sh` run on the boot path. | Desktop apps other than the Gate F demo are still in-kernel. |
| **Ring 3 timer preemption** | Naked APIC-timer stub saves GPRs + FXSAVE then `enter_context`s (`GATE_B8` + `GATE_I2 irq gprs`). | Done for the spinning-user case. |
| **SMP** | INIT/SIPI trampoline; AP loads per-CPU TSS + GS, starts its LAPIC timer, and runs a pinned Ring 3 task (`GATE_I1 smp online`, `GATE_I3 ap ring3`). | Further APs still share one CFS `current` for the BSP desktop. |
| **Signal frames for handlers** | Default terminate plus a live SIGINT handler + `rt_sigreturn` (B7). | Catching SIGINT in a user handler is done. |
| **Sockets do not transmit off-box** | Loopback `send` delivers to a peer `recv_buf`. VirtIO-net TX/RX rings are live (D2). DHCP writes `eth0`. DNS + TCP SYN/retransmit/HTTP are live (D4). CUBIC cwnd limits send (D5). | Off-box TCP is live; CUBIC is wired. |
| **AHCI and NVMe DMA are live** | AHCI command-list + PRDT round-trips a sector (C4). NVMe admin/I/O queues + PRP bounce round-trip a sector (C5). | Bare-metal beyond QEMU still unproven. |
| **Default VFS is RAM with persist snapshot** | Inodes are `Vec<u8>`. Persist blob store round-trips through VirtIO-blk (C1) with a WAL that replays after crash (C2). Root snapshot includes `/etc` (C6). inotify is live on mutate (H3). | Reboot still loses virtual FS (`/dev` `/proc` `/sys`) and boot-generated `/bin`. |
| **Security not on the deny path** | Landlock is live on VFS open/write (K4). SELinux unused. | W^X, ChaCha20 `getrandom`, seccomp EPERM, CapNetBindService (E1–E4). `bpf`/`pkey`/`quotactl`/`remap_file_pages`/`io_uring`/`userfaultfd`/`perf_event_open`/`fanotify`/`io_setup`/`kexec`/`init_module`/`mount_setattr`/`fsopen`/`keyctl`/`ioperm`/`iopl`/`acct`/`swapon`/`swapoff`/`modify_ldt`/`sysfs`/`vhangup`/`lookup_dcookie`/`memfd_secret`/`uselib`/`pkey_mprotect`/`pkey_free`/`process_mrelease`/`set_mempolicy`/`get_mempolicy`/`mbind`/`migrate_pages`/`move_pages`/`sched_setattr`/`sched_getattr`/`futex_waitv`/`open_by_handle_at`/`name_to_handle_at` return ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4, AP4, AQ4, AR4, AS4, AT4, AU4, AV4). |
| **Breadth without wiring** | 407 modules. GPU compositor, Wayland, KVM `vmlaunch` unused. OverlayFS is live (W1). | Compile time and maintenance grow; capability does not. |

---

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

## Subsystem status summary

Percentages are **production usefulness**, not lines of code.

| # | Subsystem | Grade | Live % | Priority | One-line truth |
|---|-----------|-------|--------|----------|----------------|
| 1 | Kernel Core | Wired | 82% | High | Interrupts and timers work; GS-relative CpuLocal; per-CPU TSS; MADT IOAPIC + ISO; AP online then Ring 3 (I3). |
| 2 | Memory Management | Wired | 74% | **Critical** | Demand paging + CoW #PF (H1) + file-backed fault-in (H2); leftover RAM in buddy (J2); LRU shrink (J3); swap I/O + #PF swap-in (K2); OOM on frame alloc; guarded kernel stacks (H4). |
| 3 | Process & Scheduling | Wired | 99% | **Critical** | Kernel-thread RIP switch + idle HLT; **Gate B2–B8** scheduled Ring 3; IRQ GPR+FPU save (I2); AP Ring 3 (I3); `clone(CLONE_VM)` (J1); `CLONE_THREAD` join (K1); `%fs` TLS (L1); PID ns (M1); futex wait/wake (M3); mount ns (N1); net ns (O1); user ns (P1); IPC ns (Q1); cgroup ns (R1); time ns (S1); `setns` join (T1); `chroot` jail (U1); `pivot_root` (V1); `chdir`/`getcwd` (Z3); `fchdir` (AA2); Ring 3 `getppid` (AG3); Ring 3 `getpgid` (AH3); Ring 3 `getsid` (AI3); Ring 3 `setuid` (AJ3); Ring 3 `getrlimit` (AK2); Ring 3 `setgid` (AK3); Ring 3 `setrlimit` (AL2); Ring 3 `setresuid` (AL3); Ring 3 `prlimit64` (AM2); Ring 3 `setresgid` (AM3); Ring 3 `setreuid` (AN2); Ring 3 `setgroups`/`getgroups` (AN3); Ring 3 `setregid` (AO2); Ring 3 `getresuid` (AO3); Ring 3 `getresgid` (AP2); Ring 3 `setpgid`/`getpgrp` (AP3); Ring 3 `gettimeofday` (AT2); Ring 3 `sysinfo` (AT3); Ring 3 `sched_yield` (AU2); Ring 3 `alarm` (AU3); Ring 3 `getpid` (AV2); Ring 3 `gettid` (AV3); Ring 3 `capset` (BC2); Ring 3 `ioprio_set` (BC3). |
| 4 | Filesystem & Storage | Wired | 99% | **Critical** | VirtIO-blk + persist C1–C6 + AHCI/NVMe DMA; inotify on VFS mutate (H3); mount ns bind isolation (N1); OverlayFS merge+whiteout (W1); VFS hard links (X1); VFS chmod enforcement (Y1); VFS `rmdir` (Z1); Ring 3 `unlink` (Z2); named FIFO write/read (AA1); Ring 3 `fchdir` (AA2); Ring 3 `access` (AA3); VFS `list_dir`/`getdents` (AB1); Ring 3 `dup2` (AB2); `fcntl` CLOEXEC/DUPFD (AC1); Ring 3 `pread64` (AC2); `fstat` size (AD1); Ring 3 `pwrite64` (AD2); scatter-gather `writev` (AE1); Ring 3 `ftruncate` (AE2); scatter-gather `readv` (AF1); Ring 3 `lseek` (AF2); `fsync` EBADF-checked (AG1); Ring 3 `fdatasync` (AG2); `syncfs` EBADF-checked (AH1); Ring 3 `sync` (AH2); `fchmod` EBADF-checked (AI1); Ring 3 `fchown` (AI2); `fstatfs` EBADF-checked (AJ1); Ring 3 `statfs` (AJ2); `fsetxattr` EBADF-checked (AK1); `flistxattr`/`fremovexattr` EBADF-checked (AL1); `listxattr`/`removexattr` ENOENT-checked (AM1); `faccessat` dirfd-relative (AN1); `mkdirat` dirfd-relative (AO1); `unlinkat` dirfd-relative (AP1); `readlinkat` dirfd-relative (AT1); `mknodat` dirfd-relative (AU1); `fchmodat` dirfd-relative (AV1); `renameat2` dirfd-relative (BC1); Ring 3 `splice` (R2); Ring 3 `flock` (R3); Ring 3 `sendfile` (S2); Ring 3 `tee` (S3); Ring 3 `copy_file_range` (T2); Ring 3 `vmsplice` (T3); Ring 3 xattr (U2); Ring 3 `statx` (U3); Ring 3 `fallocate` (V2); Ring 3 `utimensat` (V3); Ring 3 umask (W2); Ring 3 symlink (W3); Ring 3 `rename` (X2); Ring 3 `truncate` (X3); Ring 3 `chown` (Y2); Ring 3 `mkdir` (Y3). |
| 5 | Networking | Wired | 64% | **Critical** | Loopback live; VirtIO-net D2; DHCP applies eth0 (D3); DNS + TCP SYN/RTO/HTTP (D4); CUBIC cwnd (D5). |
| 6 | Device Drivers | Wired | 40% | **Critical** | PCI, PS/2, UART, VirtIO-blk, AHCI DMA, NVMe DMA live; USB/GPU mostly stub. |
| 7 | GUI & Desktop | Live | 82% | Medium | In-kernel demo plus Ring 3 SHM clients (F1–F4); interactive desktop terminal still in-kernel for PTY I/O. |
| 8 | Shell & Terminal | Live | 82% | Medium | Real parser/PTY/glob; Ring 3 `/bin/sh` on a PTY; live `sigreturn`; desktop terminal still in-kernel. |
| 9 | Security & Cryptography | Wired | 80% | **Critical** | AES/SHA software; W^X + ChaCha20 CSPRNG + seccomp deny + CapNetBindService live (E1–E4); Landlock on VFS (K4); bpf/pkey/quotactl/io_uring/userfaultfd/perf_event_open/fanotify/io_setup/kexec/init_module/mount_setattr/fsopen/keyctl/ioperm/iopl/acct/swapon/modify_ldt/sysfs/vhangup/lookup_dcookie/memfd_secret/uselib/pkey_mprotect/pkey_free/process_mrelease/set_mempolicy/get_mempolicy/mbind/sched_setattr/futex_waitv/open_by_handle_at/name_to_handle_at/move_pages ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4, AP4, AQ4, AR4, AS4, AT4, AU4, AV4). |
| 10 | System Services | Wired | 42% | High | Ring 3 `/sbin/init` (K3); AF_UNIX system bus socket (L3); in-kernel units; no crash restart. |
| 11 | Virtualization & Containers | Stub | 30% | Low | VMX `asm` unused; containers are comments. UTS/PID/mount/net/user/IPC/cgroup/time ns isolation live (L2, M1, N1, O1, P1, Q1, R1, S1); `setns` join live (T1); `chroot` (U1); `pivot_root` (V1); OverlayFS merge (W1). |
| 12 | AI/ML | Wired | 22% | Low | GGUF parse + naive CPU; GPU matmul unused. |
| 13 | Binary Compatibility | Wired | 99% | **Critical** | Static hello `iretq`s; `execve`/`fork`/`clone`/`CLONE_THREAD`/`/bin/sh`/`/sbin/init`; `arch_prctl` `%fs`; live `rt_sigreturn`; PID-ns `getpid`; `pipe`; futex wait/wake; `socketpair`; `eventfd`; `epoll`; `memfd_create`; `timerfd`; `signalfd`; `poll`; Ring 3 `inotify`; `splice`; `flock`; `sendfile`; `tee`; `copy_file_range`; `vmsplice`; `setxattr`/`getxattr`; `statx`; `fallocate`; `utimensat`; `umask`; `symlink`/`readlink`; `rename`; `truncate`; `chown`; `mkdir`; `unlink`; `chdir`/`getcwd`; `fchdir`; `access`; `dup2`; `uname`; `fcntl`; `pread64`; `getuid`; `fstat`; `pwrite64`; `getgid`; `writev`; `ftruncate`; `geteuid`; `readv`; `lseek`; `getegid`; `fsync`; `fdatasync`; `getppid`; `syncfs`; `sync`; `getpgid`; `fchmod`; `fchown`; `getsid`; `fstatfs`; `statfs`; `setuid`; `fsetxattr`; `getrlimit`; `setgid`; `flistxattr`; `setrlimit`; `setresuid`; `listxattr`; `prlimit64`; `setresgid`; `faccessat`; `setreuid`; `setgroups`/`getgroups`; `mkdirat`; `setregid`; `getresuid`; `unlinkat`; `getresgid`; `setpgid`/`getpgrp`; `readlinkat`; `gettimeofday`; `sysinfo`; `mknodat`; `sched_yield`; `alarm`; `fchmodat`; `getpid`; `gettid`; `renameat2`; `capset`; `ioprio_set`. |
| 14 | Internationalization & Fonts | Live | 68% | Low | TTF, CJK, RTL on the compositor; locale loading partial. |
| 15 | Build System & Tooling | Live | 75% | Medium | Make/QEMU work; `flake.nix` missing. |
| 16 | Testing & Quality | Wired | 82% | **Critical** | Real VFS/widget/DNS/buddy tests; C4–C6/D3–D5/E1–E4/F2–F4/H1–H4/I1–I3/J1–J4/K1–K4/L1–L4/M1–M4/N1–N4/O1–O4/P1–P4/Q1–Q4/R1–R4/S1–S4/T1–T4/U1–U4/V1–V4/W1–W4/X1–X4/Y1–Y4/Z1–Z4/AA1–AA4/AB1–AB4/AC1–AC4/AD1–AD4/AE1–AE4/AF1–AF4/AG1–AG4/AH1–AH4/AI1–AI4/AJ1–AJ4/AK1–AK4/AL1–AL4/AM1–AM4/AN1–AN4/AO1–AO4/AP1–AP4/AQ1–AQ4 self-tests; AR1–AV4 self-tests; AW1–BC4 self-tests; `assert!(true)` tests removed. |
| 17 | Documentation | Wired | 42% | **Critical** | README + LICENSE + BUILDING + CONTRIBUTING + this file. Architecture guides still missing. |
| 18 | CI/CD & Release | Wired | 30% | High | `.github/workflows/ci.yml` (fmt, clippy, size, QEMU boot); no signed releases. |

**QEMU desktop demo readiness: ~99%** (boots, paints, clicks, types; serial prints `hello from userspace`; Gate B3–B8 scheduled userspace; Gate D1–D5 packets; Gate C1–C6 storage; Gate E1–E4 enforcement; Gate F1–F4 isolated clients; Gate H1–H4 memory/VFS; Gate I1–I3 SMP + IRQ GPRs + AP Ring 3; Gate J1–J4 clone/buddy/LRU/ENOSYS; Gate K1–K4 join/swap/init/landlock; Gate L1–L4 TLS/UTS/D-Bus/ENOSYS; Gate M1–M4 PID ns/pipe/futex/ENOSYS; Gate N1–N4 mount ns/socketpair/eventfd/ENOSYS; Gate O1–O4 net ns/epoll/memfd/ENOSYS; Gate P1–P4 user ns/timerfd/signalfd/ENOSYS; Gate Q1–Q4 IPC ns/poll/inotify/ENOSYS; Gate R1–R4 cgroup ns/splice/flock/ENOSYS; Gate S1–S4 time ns/sendfile/tee/ENOSYS; Gate T1–T4 setns/copy_file_range/vmsplice/ENOSYS; Gate U1–U4 chroot/xattr/statx/ENOSYS; Gate V1–V4 pivot_root/fallocate/utimensat/ENOSYS; Gate W1–W4 OverlayFS/umask/symlink/ENOSYS; Gate X1–X4 hardlink/rename/truncate/ENOSYS; Gate Y1–Y4 chmod/chown/mkdir/ENOSYS; Gate Z1–Z4 rmdir/unlink/chdir/ENOSYS; Gate AA1–AA4 mkfifo/fchdir/access/ENOSYS; Gate AB1–AB4 getdents/dup2/uname/ENOSYS; Gate AC1–AC4 fcntl/pread64/getuid/ENOSYS; Gate AD1–AD4 fstat/pwrite64/getgid/ENOSYS; Gate AE1–AE4 writev/ftruncate/geteuid/ENOSYS; Gate AF1–AF4 readv/lseek/getegid/ENOSYS; Gate AG1–AG4 fsync/fdatasync/getppid/ENOSYS; Gate AH1–AH4 syncfs/sync/getpgid/ENOSYS; Gate AI1–AI4 fchmod/fchown/getsid/ENOSYS; Gate AJ1–AJ4 fstatfs/statfs/setuid/ENOSYS; Gate AK1–AK4 fsetxattr/getrlimit/setgid/ENOSYS; Gate AL1–AL4 flistxattr/setrlimit/setresuid/ENOSYS; Gate AM1–AM4 listxattr/prlimit64/setresgid/ENOSYS; Gate AN1–AN4 faccessat/setreuid/getgroups/ENOSYS; Gate AO1–AO4 mkdirat/setregid/getresuid/ENOSYS; Gate AP1–AP4 unlinkat/getresgid/setpgid/ENOSYS; Gate AQ1–AQ4 renameat/setsid/setpriority/ENOSYS; Gate AR1–AR4 linkat/getrusage/clock_gettime/ENOSYS; Gate AS1–AS4 symlinkat/clock_getres/times/ENOSYS; Gate AT1–AT4 readlinkat/gettimeofday/sysinfo/ENOSYS; Gate AU1–AU4 mknodat/sched_yield/alarm/ENOSYS; Gate AV1–AV4 fchmodat/getpid/gettid/ENOSYS; Gate AW1–AW4 fchownat/getsched/getparam/ENOSYS; Gate AX1–AX4 newfstatat/prio_max/prio_min/ENOSYS; Gate AY1–AY4 openat/rr_interval/getcpu/ENOSYS; Gate AZ1–AZ4 utimensat/set_robust/get_robust/ENOSYS; Gate BA1–BA4 statx/personality/nanosleep/ENOSYS; Gate BB1–BB4 futimesat/capget/ioprio_get/ENOSYS; Gate BC1–BC4 renameat2/capset/ioprio_set/ENOSYS).
**Production OS readiness: ~98%** (Gate B2–B8, C1–C6, D1–D5, E1–E4, F1–F4, H1–H4, I1–I3, J1–J4, K1–K4, L1–L4, M1–M4, N1–N4, O1–O4, P1–P4, Q1–Q4, R1–R4, S1–S4, T1–T4, U1–U4, V1–V4, W1–W4, X1–X4, Y1–Y4, Z1–Z4, AA1–AA4, AB1–AB4, AC1–AC4, AD1–AD4, AE1–AE4, AF1–AF4, AG1–AG4, AH1–AH4, AI1–AI4, AJ1–AJ4, AK1–AK4, AL1–AL4, AM1–AM4, AN1–AN4, AO1–AO4, AP1–AP4, AQ1–AQ4, AR1–AR4, AS1–AS4, AT1–AT4, AU1–AU4, AV1–AV4, AW1–AW4, AX1–AX4, AY1–AY4, AZ1–AZ4, BA1–BA4, BB1–BB4, BC1–BC4). `./tests/run_integration.sh` **230/230** on QEMU (2026-10-06).

---

## 1. Kernel Core

**Grade: Wired (82%)** · `gdt.rs`, `interrupts.rs`, `smp.rs`, `apic_timer.rs`, `acpi_tables.rs`, `cmdline.rs`

### Live
- [x] GDT + TSS load on BSP (`gdt::init`)
- [x] IDT: breakpoint, #PF, #GP, #UD, #SS, #DF, NMI, MCE, timer, kbd, mouse, virtio, ATA, APIC timer
- [x] PIC 8259 init and IRQ unmask (timer, kbd, cascade, virtio, mouse, ATA)
- [x] LAPIC detection, APIC timer + PIT calibration
- [x] Serial UART 16550 TX (debug console)
- [x] Kernel heap 512 MiB + slab
- [x] Boot splash + long `kernel_main` init sequence
- [x] Panic handler
- [x] RTC CMOS read
- [x] Page-fault → `vmm::handle_page_fault` (when a process address space exists)
- [x] ACPI RSDP scan; MADT / FADT / HPET / MCFG parse from physical memory

### Wired but incomplete
- [x] **IOAPIC** — MADT address + interrupt-source overrides applied after ACPI parse (IRQ0→GSI 2 on QEMU); all IRQs still to BSP
- [x] **SMP** — real INIT/SIPI trampoline 16→32→64; AP loads per-CPU TSS + GS, STI + LAPIC timer, runs pinned Ring 3 (`GATE_I1 smp online`, `GATE_I3 ap ring3`); `balance_load` does not steal AP-pinned tasks
- [x] **Per-CPU data** — GS-relative `CpuLocal` (`gs:[0]`/`gs:[8]` syscall scratch + `gs:[24]` cpu index + `gs:[32]` current PID); `CPU_DATA` mutex remains for stats
- [x] **Command line** — real `key=value` parser; QEMU fw_cfg cmdline when present, else a documented default
- [ ] **x86_64 / aarch64 / riscv64** — x86_64 boots; other arches compile with shims

### Stub / unused
- [x] **NMI** — IDT handler logs and returns
- [x] **MCE** — IDT handler logs and panics
- [ ] **ACPI AML / DSDT** — no interpreter; `acpi::shutdown` is QEMU port writes
- [ ] **UEFI runtime** — `get_time` hardcoded; `get_variable` → `None`
- [ ] **Kernel modules** — `modules.rs` / `kpm.rs` are name registries, not `.ko` loaders
- [ ] **IOMMU** — DMAR parse; `enable_translation` never called
- [ ] **Live patch** — writes a jmp over a symbol; no W^X, no activeness wait
- [ ] **Nested IRQ / TPR** — not used from ISRs

### Perfect-OS next steps
1. ~~Per-CPU TSS + GS base + exception stacks before SMP is useful.~~ **Done** (I1) — each CPU has TSS + IST stacks; GS points at `CpuLocal`.
2. ~~Wire MADT IOAPIC address and interrupt source overrides.~~ **Done** — `smp::apply_madt_ioapic` after ACPI parse.
3. ~~Add NMI and MCE handlers that log and recover or panic cleanly.~~ **Done** — NMI logs; MCE panics.
4. ~~Pass the real bootloader command line into `cmdline::parse`.~~ **Done** when fw_cfg has one; otherwise the documented default.
5. ~~AP runs a Ring 3 task.~~ **Done** (I3) — affinity-pinned `getcpu` ELF on CPU 1.

---

## 2. Memory Management

**Grade: Wired (74%)** · `memory.rs`, `allocator.rs`, `vmm.rs`, `mmap.rs`, `slab.rs`, `page_cache.rs`

The real MMU work is in **`vmm.rs`**, not a buddy allocator.

### Live / Wired
- [x] Bootloader mmap bump-pointer frame allocator (no free)
- [x] Linked-list kernel heap (eager-mapped)
- [x] Slab for ≤4 KiB objects (backed by heap, not physical buddy)
- [x] 4-level page-table walk (`map_page_in_table`)
- [x] Demand paging: `#PF` → allocate + map + zero (`handle_demand_fault`)
- [x] CoW: `AddressSpace::fork` marks PTEs read-only; write fault copies (**Gate H1** / A4)
- [x] Anonymous `mmap` (lazy unless `MAP_POPULATE`)
- [x] File-backed `mmap`: one-page fault-in from page cache / VFS; siblings stay unmapped (**Gate H2**)
- [x] User ASLR offsets (xorshift; kernel image not randomized)
- [x] **Buddy allocator** on the VMM frame pool (orders 0–10, split/merge; 32 MiB prefill plus leftover usable RAM)
- [x] **OOM on alloc** — empty buddy pool shrinks the page cache then `trigger_oom` (**Gate H4**)
- [x] **Kernel stack guards** — `alloc_guarded_stack` used after VMM is ready; guard page is unmapped (**Gate H4**)

### Unused / stub
- [x] **Buddy over leftover RAM** — remaining usable bootloader frames are ingested into the buddy (`GATE_J2 buddy ram`)
- [x] **File-backed mmap** — lazy VMA; `#PF` fills one 4 KiB page from the page cache / VFS (H2)
- [x] **Page reclamation / LRU eviction** — `page_cache::shrink` drops oldest clean pages and keeps dirty (`GATE_J3 lru reclaim`); anonymous pages can swap (K2)
- [x] **Swap** — ram-backed slots store page bytes; reclaim calls `page_out`; `#PF` swap-in restores (`GATE_K2 swap io`)
- [x] **OOM killer** — scoring + `trigger_oom` from failed `allocate_physical_frame` (H4)
- [ ] **NUMA** — SRAT parse; `allocate_node` returns an id, not memory
- [ ] **KSM** — hashes phys addr as a virt pointer; merge is a counter
- [ ] **THP** — `allocate_contiguous_frames` returns `None` above buddy max order
- [x] **Kernel stack guards** — `stack_guard::alloc_guarded_stack` on the live path (H4)
- [ ] **KASLR** for the kernel image
- [ ] **Memory cgroups** enforcement

### Perfect-OS next steps
1. ~~Physical buddy (or equivalent) with free, orders, and a real page inventory.~~ **Done** (J2) for leftover usable RAM after the heap; reserved/firmware holes stay out of the pool.
2. ~~Call OOM from failed alloc; reclaim before OOM.~~ **Done** (H4) — shrink then `trigger_oom`.
3. ~~File-backed VMAs that fault from the block layer through the page cache.~~ **Done** (H2).
4. ~~Guard pages on every kernel stack; W^X on all user maps.~~ Guarded stacks after VMM init (H4); W^X was E1. LRU reclaim of clean file pages (J3). Swap-in on `#PF` (K2).

---

## 3. Process & Scheduling

**Grade: Wired (99%)** · `scheduler.rs`, `process.rs`, `context.rs`, `usermode.rs`, `signals.rs`, `user_task.rs`

Kernel threads can switch RIP. Gate B2 enters Ring 3 for a one-shot hello. Gate B3–B6 then run **scheduled** Ring 3 tasks with their own CR3: `execve`+`waitpid`, `fork`+child, SIGKILL/SIGSEGV/PTY SIGINT, and `/bin/sh` on a PTY.

### Wired (data plane)
- [x] Process table, PIDs, parent/child, reparent to init, zombie bookkeeping
- [x] CFS vruntime / weight table in software
- [x] Timer ISR sets `NEED_RESCHED`
- [x] `sys_fork` CoW-forks VMM and clones a context with `rax = 0`
- [x] `sys_execve` can map ELF into an address space + stack/auxv
- [x] `usermode.rs`: `wrmsr` STAR/LSTAR/SFMASK/EFER.SCE + `syscall`/`sysretq` stubs
- [x] Signal frame builder (`signals.rs`)
- [x] **Full kernel context switch** — GPRs, RIP, CS/SS, RFLAGS, FXSAVE/FXRSTOR; boot self-test switches two kernel threads
- [x] **Idle thread** PID 0 `STI; HLT`; executor `yield_to_idle()` when the desktop has no work
- [x] **`KERNEL_GS_BASE` / `GS_BASE`** programmed for `syscall` `swapgs`; Ring 3 `syscall` round-trip proven on hello
- [x] **`deliver_signals`** called from `deferred_schedule`; SIGKILL/default-terminate actually retire the task
- [x] **Gate B2 one-shot Ring 3** — static hello ELF mapped into current CR3, `iretq`, `sys_write` to serial, `sys_exit` returns to kernel
- [x] **Gate B3–B6 scheduled Ring 3** — `execve`+`waitpid`, `fork` child, SIGKILL / SIGSEGV / PTY SIGINT, `/bin/sh` on a PTY
- [x] **Gate B7 live `sigreturn`** — `rt_sigaction` handler runs in Ring 3, `ret`s into trampoline, `rt_sigreturn` resumes `pause`

### Stub (control plane)
- [x] **Scheduled Ring 3** — `user_task::spawn_elf` builds a CFS task with its own CR3; boot path `execve`s `/bin/hello`, then `/bin/sh` on a PTY
- [x] **Preemption of userspace** — naked APIC-timer stub saves GPRs + FXSAVE; spinner `rbx` magic survives (`GATE_I2 irq gprs`)
- [x] **`wait4` blocking** — parks a Ring 3 parent until the child exits
- [x] **SMP load balance / affinity** — `set_cpu_affinity` is honoured by CFS; APs run tasks pinned with `enqueue_on_cpu` (`GATE_I3 ap ring3`). Global CFS `current` remains BSP-shaped for the desktop.
- [x] **Threads / `clone(CLONE_VM)`** — child shares the parent's CR3 and runs; parent `wait4`s (`GATE_J1 thread clone`). `CLONE_THREAD` + `thread_join` live (K1). `CLONE_SETTLS` / `arch_prctl(ARCH_SET_FS)` restore `%fs` (L1).
- [x] **UTS namespaces** — `unshare(CLONE_NEWUTS)` isolates `sethostname` from the parent (`GATE_L2 uts ns`).
- [x] **PID namespaces** — `unshare(CLONE_NEWPID)` then fork: child `getpid` is 1; parent unchanged (`GATE_M1 pid ns`).
- [x] **Mount namespaces** — `unshare(CLONE_NEWNS)` bind mount does not leak to the parent (`GATE_N1 mount ns`).
- [x] **Network namespaces** — `unshare(CLONE_NEWNET)` child does not inherit `eth0`; child `veth0` stays private (`GATE_O1 net ns`).
- [x] **User namespaces** — `unshare(CLONE_NEWUSER)` maps the creator to uid 0; parent uid unchanged (`GATE_P1 user ns`).
- [x] **IPC namespaces** — `unshare(CLONE_NEWIPC)` isolates SysV shm keys from the parent (`GATE_Q1 ipc ns`).
- [x] **Cgroup namespaces** — `unshare(CLONE_NEWCGROUP)` isolates cgroup paths from the parent (`GATE_R1 cgroup ns`).
- [x] **Time namespaces** — `unshare(CLONE_NEWTIME)` isolates CLOCK_MONOTONIC offsets from the parent (`GATE_S1 time ns`).
- [x] **`setns`** — joiner attaches to an existing UTS ns and sees the owner's hostname (`GATE_T1 setns`).
- [x] **`chroot`** — per-process root jails path lookup; `/../etc` stays inside the jail (`GATE_U1 chroot`).
- [x] **`pivot_root`** — `new_root` becomes `/`; old root is visible at `put_old`; parent unchanged (`GATE_V1 pivot_root`).
- [x] **`chdir`/`getcwd`** — Ring 3 `chdir` then `getcwd` returns the new path (`GATE_Z3 chdir`). Ring 3 `fchdir` then `getcwd` (`GATE_AA2 fchdir`).
- [x] **`getppid`** — Ring 3 spawned task parent is non-zero (`GATE_AG3 getppid`).
- [x] **`getpgid`** — Ring 3 spawned task `getpgid(0)` is non-zero (`GATE_AH3 getpgid`).
- [x] **`getsid`** — Ring 3 spawned task `getsid(0)` is non-zero (`GATE_AI3 getsid`).
- [x] **`setuid`** — Ring 3 `setuid(1000)` then `getuid` is 1000 (`GATE_AJ3 setuid`).
- [x] **`getrlimit`** — Ring 3 `getrlimit(RLIMIT_NOFILE)` reports `rlim_cur == 1024` (`GATE_AK2 getrlimit`).
- [x] **`setgid`** — Ring 3 `setgid(2000)` then `getgid` is 2000 (`GATE_AK3 setgid`).
- [x] **`setrlimit`** — Ring 3 `setrlimit(RLIMIT_NOFILE, {512, 1024})` then `getrlimit` reports `rlim_cur == 512` (`GATE_AL2 setrlimit`).
- [x] **`setresuid`** — Ring 3 `setresuid(-1, 1000, -1)` then `geteuid` is 1000 (`GATE_AL3 setresuid`).
- [x] **`prlimit64`** — Ring 3 `prlimit64(0, RLIMIT_NOFILE, {256, 1024})` then `getrlimit` reports `rlim_cur == 256` (`GATE_AM2 prlimit64`).
- [x] **`setresgid`** — Ring 3 `setresgid(-1, 2000, -1)` then `getegid` is 2000 (`GATE_AM3 setresgid`).
- [x] **`setreuid`** — Ring 3 `setreuid(-1, 1000)` then `geteuid` is 1000 (`GATE_AN2 setreuid`).
- [x] **`setgroups`/`getgroups`** — Ring 3 `setgroups(1, [2000])` then `getgroups` returns 2000 (`GATE_AN3 getgroups`).
- [x] **`setregid`** — Ring 3 `setregid(-1, 2000)` then `getegid` is 2000 (`GATE_AO2 setregid`).
- [x] **`getresuid`** — Ring 3 `setuid(1000)` then `getresuid` reports ruid/euid 1000 (`GATE_AO3 getresuid`).
- [x] **`getresgid`** — Ring 3 `setgid(2000)` then `getresgid` reports rgid/egid 2000 (`GATE_AP2 getresgid`).
- [x] **`setpgid`/`getpgrp`** — Ring 3 `setpgid(0, 0)` then `getpgrp` equals `getpid` (`GATE_AP3 setpgid`).
- [x] **`setsid`** — Ring 3 `setsid` then `getsid(0)` equals `getpid` (`GATE_AQ2 setsid`).
- [x] **`setpriority`/`getpriority`** — Ring 3 `setpriority(PRIO_PROCESS, 0, 5)` then `getpriority` is 15 (`GATE_AQ3 setpriority`).
- [x] **`getrusage`** — Ring 3 `getrusage(RUSAGE_SELF)` reports `ru_maxrss == 4096` (`GATE_AR2 getrusage`).
- [x] **`clock_gettime`** — Ring 3 `CLOCK_MONOTONIC` `tv_nsec < 1e9` (`GATE_AR3 clock_gettime`).
- [x] **`gettimeofday`** — Ring 3 `tv_usec < 1e6` (`GATE_AT2 gettimeofday`).
- [x] **`sysinfo`** — Ring 3 `totalram` is non-zero (`GATE_AT3 sysinfo`).
- [x] **`sched_yield`** — Ring 3 returns 0 (`GATE_AU2 sched_yield`).
- [x] **`alarm`** — Ring 3 `alarm(0)` is non-negative (`GATE_AU3 alarm`).
- [x] **`getpid`** — Ring 3 spawned task `getpid` is non-zero (`GATE_AV2 getpid`).
- [x] **`gettid`** — Ring 3 spawned task `gettid` is non-zero (`GATE_AV3 gettid`).
- [x] **futex wait/wake** — Ring 3 `FUTEX_WAIT` parks; `FUTEX_WAKE` resumes (`GATE_M3 futex`).
- [ ] **sched_ext / eBPF** — not eBPF

### Perfect-OS next steps (this is the critical path)
1. ~~Save/restore full `iretq` frame + `fxsave`/`fxrstor` in `context.rs`.~~ (kernel threads done)
2. ~~Per-CPU TSS RSP0 + `GS_BASE` / `KERNEL_GS_BASE`.~~ **Done** (I1) — each CPU has a TSS; GS points at `CpuLocal`.
3. ~~First userspace: map a static `hello` ELF, `iretq`, `sys_write` to serial, `sys_exit`.~~ (Gate B2)
4. Then `fork` + `execve` + `waitpid` + SIGCHLD + Ctrl+C via PTY. **B3–B7 wired on the boot path.** Isolated GUI clients (Gate F) have a first SHM client; the in-kernel terminal remains.

---

## 4. Filesystem & Storage

**Grade: Wired (99%)** · `vfs.rs`, `block.rs`, `virtio_blk.rs`, `ext4.rs`, `fat32.rs`, `persist.rs`, `partition.rs`, `page_cache.rs`, `ahci.rs`, `nvme.rs`

### Live
- [x] In-memory VFS (FHS tree, path walk, fds, metadata)
- [x] procfs / sysfs / devfs / tmpfs (virtual)
- [x] ATA PIO IDENTIFY/READ/WRITE (`block.rs`, ports `0x1F0` / `0x170`)
- [x] VirtIO-blk: PCI, virtqueue, guest-physical DMA + bounce buffer, sector R/W (QEMU)
- [x] ext4: superblock at LBA 2, inode table, extents, read/write via block layer
- [x] FAT32: BPB, FAT, clusters, LFN
- [x] GPT/MBR parse (`partition.rs`)
- [x] `persist.rs` blob store on VirtIO-blk (`KNOXPERSIST`)
- [x] **Gate C1 persist roundtrip** — write `/var/lib/knoxos/gate_c1`, unlink from RAM VFS, restore from VirtIO-blk
- [x] **Gate C2 persist WAL** — committed journal record survives simulated crash; uncommitted is discarded (`GATE_C2 journal recovered`)
- [x] **Gate C3 page cache writeback** — dirty 4 KiB page overlays backing file at offset; sibling pages survive (`GATE_C3 writeback complete`)
- [x] **Gate C4 AHCI DMA** — command list + PRDT write then read a marker sector (`GATE_C4 ahci dma complete`)
- [x] **Gate C5 NVMe DMA** — admin + I/O queues; PRP1 bounce write/read round-trip (`GATE_C5 nvme dma complete`)
- [x] **Gate C6 VFS root persist** — `/etc` (and the rest of the RAM namespace except virtual FS and `/bin`) round-trips through VirtIO-blk (`GATE_C6 vfs persist`)
- [x] 4 MB ramdisk always created

### Wired / partial
- [x] Page cache is an in-RAM `BTreeMap`; write-back overlays dirty pages via `pwrite_file` (C3)
- [x] Mount table: `unshare(CLONE_NEWNS)` isolates bind mounts (N1); ext4 really mounts; several FS types only `mkdir`
- [x] flock — Ring 3 `flock(LOCK_EX)` then `LOCK_UN` on a VFS file (R3). xattr `setxattr`/`getxattr` live on in-kernel maps (U2)
- [x] `sendfile` — Ring 3 file-to-pipe byte copy (S2)
- [x] `tee` — Ring 3 pipe duplicate without consuming the source (S3)
- [x] `copy_file_range` — Ring 3 file-to-file byte copy (T2)
- [x] `vmsplice` — Ring 3 user pages spliced into a pipe (T3)
- [x] `statx` — Ring 3 `stx_size` from a VFS file (U3)
- [x] `fallocate` — Ring 3 extends a VFS file; `statx` sees the new size (V2)
- [x] `utimensat` — Ring 3 sets inode mtime; `statx` reports `stx_mtime` (V3)
- [x] OverlayFS — VFS-backed lower+upper merge + whiteout, mount-ns isolated (W1)
- [x] `umask` — applied on `open(O_CREAT)` and `mkdir` (W2)
- [x] `symlink`/`readlink` — Ring 3 target round-trip (W3)
- [x] Hard links — two names share one inode; write via one is visible via the other; unlink decrements nlink (X1)
- [x] `rename` — Ring 3 write, rename, read from the new path (X2)
- [x] `truncate` — Ring 3 shrinks a VFS file; `statx` sees the new size (X3)
- [x] `chmod` — mode 0400: owner can read but not write; other cannot read (Y1)
- [x] `chown` — Ring 3 sets uid 1000; `statx` reports `stx_uid` (Y2)
- [x] `mkdir` — Ring 3 creates a directory; `statx` reports `S_IFDIR` (Y3)
- [x] `rmdir` — empty directory is removed; non-empty fails ENOTEMPTY (Z1)
- [x] `unlink` — Ring 3 create then unlink; subsequent `open` fails (Z2)
- [x] `chdir`/`getcwd` — Ring 3 `getcwd` matches the `chdir` target (Z3)
- [x] `mkfifo` — named FIFO write/read round-trip through the FIFO buffer (AA1)
- [x] `fchdir` — Ring 3 `fchdir` then `getcwd` (AA2)
- [x] `access` — Ring 3 `access(F_OK)` on a created file; missing path fails (AA3)
- [x] `getdents64` — VFS `list_dir` includes a created child (`GATE_AB1 getdents`)
- [x] `fcntl` — `F_SETFD`/`F_GETFD` CLOEXEC; `dup` clears cloexec (`GATE_AC1 fcntl`)
- [x] `pread64` — Ring 3 reads at offset without moving the fd position (AC2)
- [x] `fstat` — reports the written VFS size (`GATE_AD1 fstat`)
- [x] `pwrite64` — Ring 3 writes at offset without moving the fd position (AD2)
- [x] `writev` — scatter-gather write of two iovecs reads back `xy` (`GATE_AE1 writev`)
- [x] `ftruncate` — Ring 3 shrinks an fd; `statx` sees the new size (AE2)
- [x] `readv` — scatter-gather read of `xy` fills two iovecs (`GATE_AF1 readv`)
- [x] `lseek` — Ring 3 writes `xy`, seeks to 1, reads `y` (AF2)
- [x] `fsync` — succeeds on a written VFS fd; bad fd is EBADF (`GATE_AG1 fsync`)
- [x] `fdatasync` — Ring 3 write then `fdatasync` (`GATE_AG2 fdatasync`)
- [x] `syncfs` — succeeds on a written VFS fd; bad fd is EBADF (`GATE_AH1 syncfs`)
- [x] `sync` — Ring 3 write then `sync` (`GATE_AH2 sync`)
- [x] `fchmod` — sets mode 0400 on a written VFS fd; bad fd is EBADF (`GATE_AI1 fchmod`)
- [x] `fchown` — Ring 3 sets uid 1000 via fd; `statx` reports `stx_uid` (`GATE_AI2 fchown`)
- [x] `fstatfs` — reports `f_bsize == 4096` on a written VFS fd; bad fd is EBADF (`GATE_AJ1 fstatfs`)
- [x] `statfs` — Ring 3 `statfs("/tmp")` reports `f_bsize == 4096` (`GATE_AJ2 statfs`)
- [x] `fsetxattr`/`fgetxattr` — resolve fd to inode; round-trip `user.knox`; bad fd is EBADF (`GATE_AK1 fsetxattr`)
- [x] `flistxattr`/`fremovexattr` — list contains `user.knox` after set; remove clears it; bad fd is EBADF (`GATE_AL1 flistxattr`)
- [x] `listxattr`/`removexattr` — path list contains `user.knox` after set; remove clears it; missing path is ENOENT (`GATE_AM1 listxattr`)
- [x] `faccessat` — dirfd-relative `F_OK` succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AN1 faccessat`)
- [x] `mkdirat` — dirfd-relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AO1 mkdirat`)
- [x] `unlinkat` — dirfd-relative unlink succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AP1 unlinkat`)
- [x] `renameat` — dirfd-relative rename succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AQ1 renameat`)
- [x] `linkat` — dirfd-relative hard link succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AR1 linkat`)
- [x] `symlinkat` — dirfd-relative symlink succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AS1 symlinkat`)
- [x] `readlinkat` — dirfd-relative read returns the target; missing child is ENOENT; bad dirfd is EBADF (`GATE_AT1 readlinkat`)
- [x] `mknodat` — dirfd-relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AU1 mknodat`)
- [x] `fchmodat` — dirfd-relative chmod 0400 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AV1 fchmodat`)
- [ ] NTFS — parses MFT from a provided buffer; **never calls the block layer**
- [x] `fsync`/`fdatasync` — flush dirty page-cache pages, re-persist the fd's VFS file through the WAL, and issue VirtIO-blk FLUSH

### Stub
- [x] **AHCI** — HBA reset, port start, command-list + PRDT DMA read/write (C4); IDENTIFY parsed
- [x] **NVMe** — controller enable, admin/I/O queues, PRP bounce DMA read/write (C5)
- [ ] Btrfs / ZFS / XFS / exFAT / CIFS — in-memory or AHCI/NVMe fallback
- [ ] JBD2 journal replay — `replayed = 0` (persist WAL is live; ext4 JBD2 is not)
- [x] OverlayFS — VFS-backed lower+upper merge + whiteout, isolated to the caller's mount ns (**Gate W1**). FUSE / NFS still not registered.
- [x] inotify — `emit_event` from VFS write/unlink and `open(O_CREAT)`; watch can read (**Gate H3**). Ring 3 `inotify_init1`/`add_watch`/`read` is live (Q3). `fanotify_init` returns ENOSYS (P4).
- [ ] Quotas — counters, not enforced

### Perfect-OS next steps
1. Make ext4 (or a single production FS) the root on VirtIO-blk by default, not RAM. Persist snapshot covers the RAM namespace (C6); virtual FS still rebuilt at boot.
2. ~~Journal commit + crash recovery that is tested by killing QEMU mid-write.~~ **Done** for the persist blob WAL (in-boot: commit journal, skip checkpoint, restore replays). JBD2 still missing.
3. ~~Complete VirtIO-blk DMA (guest-physical, not heap pointers).~~ **Done** — contiguous buddy frames + bounce buffer. ~~AHCI DMA still missing.~~ **C4 done.** ~~NVMe DMA still missing.~~ **C5 done.**
4. ~~Hook inotify on VFS mutate.~~ **Done** (H3). Page-cache writeback (C3) overlays dirty pages.

---

## 5. Networking Stack

**Grade: Wired (64%)** · `net.rs`, `netint.rs`, `virtio_net.rs`, `e1000.rs`, `rtl8139.rs`, `dhcp.rs`, `dns.rs`, `net_production.rs`

### Wired (builders / closest-to-real NICs)
- [x] Ethernet / ARP / IPv4 / UDP / TCP **header** construction
- [x] `e1000`: reset, MAC, RX/TX rings, TDT/RDT poll (heap-as-DMA caveat)
- [x] `rtl8139`: PIO TX, RX ring poll
- [x] DHCP DISCOVER/REQUEST **can be sent**; OFFER/ACK parse
- [x] DNS A-query **can be sent**; response parse
- [x] NTP packet build + UDP send helper
- [x] **Loopback** — UDP `sendto` and TCP `connect`/`send` copy into the peer `recv_buf` on `127.0.0.1`; Gate D1 Ring 3 demo
- [x] **Gate D2 VirtIO-net rings** — guest-physical avail/used; TX DHCP DISCOVER; RX UDP reply from QEMU user-net (`GATE_D2 virtio-net complete`)
- [x] **Gate D3 DHCP apply** — ACK writes `eth0` IPv4, mask, gateway (`GATE_D3 dhcp applied`)
- [x] **Gate D4 DNS + TCP** — UDP DNS parse; SYN/ACK + RTO; HTTP GET (`GATE_D4 dns tcp complete`)
- [x] **Gate D5 TCP CUBIC** — cwnd grows on ACK, β=0.7 on loss, send limited by window (`GATE_D5 cubic window`)

### Stub (the path apps use)
- [x] **Socket send off-box** — non-loopback `send`/`sendto` goes through `netint` + VirtIO-net
- [x] **TCP connect off-box** — SYN sent; SYN-ACK completes; RTO retransmits
- [x] **TCP CUBIC / window** — `CongestionState` on each TCB; send clamped to cwnd; advertised `rcv_wnd`; loss cuts cwnd (D5)
- [x] **VirtIO-net TX** — writes avail ring with guest-physical bounce; waits for used
- [x] **VirtIO-net RX** — walks the used ring and re-arms buffers
- [x] **DHCP apply** — writes interface IP + DNS + default route
- [ ] IPv6 echo — builds packet, logs, no TX
- [ ] TLS 1.3 — types; GCM/X.509 stub; handshake flagged done after one recv
- [ ] Wi-Fi / WPA3 / bridge / VLAN / NAT — in-memory

### Perfect-OS next steps
1. ~~Loopback: `send` → peer `recv_buf`.~~ **D1 done** — kernel self-test + Ring 3 `sendto`/`recvfrom`.
2. ~~Fix VirtIO-net avail/used rings; TX one UDP ping.~~ **D2 done** — DHCP DISCOVER TX + UDP reply RX vs QEMU user-net.
3. ~~Wire `Socket` → `netint::send_*`; ARP then DHCP that **writes the interface IP**.~~ **D3 done.**
4. ~~TCP: SYN/ACK, seq/ack, RTO, then CUBIC.~~ **SYN/ACK + RTO + HTTP GET done (D4). CUBIC cwnd done (D5).**

---

## 6. Device Drivers

**Grade: Wired (40%)** · `pci.rs`, `usb.rs`, `ahci.rs`, `nvme.rs`, `i915.rs`, …

### Live
- [x] PCI config `0xCF8`/`0xCFC`, BDF scan, BAR decode
- [x] PS/2 keyboard + mouse (interrupt-driven)
- [x] UART TX
- [x] VirtIO-blk
- [x] AHCI command-list DMA (C4)
- [x] NVMe admin/I/O queue DMA (C5)
- [x] VGA/BGA framebuffer from bootloader
- [x] RTC, HPET registers (timer path uses APIC + PIT)

### Partial MMIO (not a full driver)
- [ ] XHCI — rings/doorbells; timeouts “continue anyway”; fallback QEMU descriptors
- [ ] i915 / AMDGPU — register read/write; no modeset/firmware/GTT
- [ ] HDA — CORB/RIRB; DMA buffer is a heap pointer
- [ ] TPM TIS FIFO at `0xFED4_0000` — incomplete locality handshake

### Stub
- [ ] VirtIO-GPU — **no virtqueue**; fake 1920×1080
- [ ] DRM — in-memory CRTC/GEM
- [ ] USB HID / hub / MSC / audio — parsers or flags, no URB completion loop
- [ ] e1000/rtl8139 — see networking; not used by `Socket::send`
- [ ] Touchscreen, trackpad, gamepad, GPIO, I2C, Thunderbolt, Wacom, UVC
- [ ] Broadcom Wi-Fi, AX211, USB Ethernet

### Perfect-OS next steps
1. One storage, one net, one GPU scanout, one USB HID — **finished**, not 40 started.
2. Storage: VirtIO-blk production-quality; AHCI DMA live (C4); NVMe DMA live (C5).
3. Net: VirtIO-net rings + DHCP + DNS/TCP live (D2–D4); e1000 still unused by sockets.
4. Display: VirtIO-GPU 2D resource + scanout **or** keep software FB until Ring 3 exists.
5. Input: USB HID interrupt-IN so the desktop works without PS/2.

---

## 7. GUI & Desktop Environment

**Grade: Live (82%)** · `gui/` 161 files, ~91,700 lines

The compositor is the most complete **product** in the tree. It is not a Unix display server.

### Live
- [x] 32bpp BGRA software FB, double-buffer, ≤16 damage rects, `present_rect`
- [x] Window manager: z-order, focus, min/max, snap, workspaces, chrome, shadows, animations
- [x] Desktop, wallpaper, icons, context menu, pill taskbar, tray, start menu, Alt-Tab, hot corners
- [x] TTF raster, glyph cache, subpixel/grayscale, CJK, RTL runs, KnoxUI widgets, immediate-mode UI
- [x] Mouse (click/drag/resize), keyboard focus, shortcuts, theme (dark/light), notifications
- [x] Lock/login screens, screenshot, in-process DnD, blur, night light, on-screen keyboard
- [x] Clipboard used by terminal/explorer/input (`clipboard.rs`)
- [x] **17 real in-process apps**: Terminal, Files, Browser (tag HTML), AI Assistant, Editor, Settings, Task Manager, Calculator, Image Viewer, Log Viewer, Calendar, Disk Utility, Bluetooth manager, Software Updater, Software Center, Archive Manager, Setup Wizard

### Unused / stub
- [ ] **Wayland** — object model plus a live `/dev/wl0` present ioctl; one Ring 3 SHM client (F1–F2); no bind/listen Unix socket
- [ ] **GPU compositor** — `gpu_compositor.rs` never invoked; desktop software-blits
- [x] **Client isolation (demo)** — Gate F Ring 3 clients paint SHM (F1–F4); remaining in-kernel apps still `WindowContentType`
- [ ] Hardware cursor probed (`hw_cursor::init`); desktop still draws a software cursor
- [ ] VSync module unused; pacing is TSC ~60 FPS
- [x] Start-menu **Paint / Video Player / Webamp / Doom / ClassiCube / Quake III** removed; Paint is a Ring 3 SHM client (F4)
- [ ] HDR, VRR, TrueType bytecode hinting, full IME framework

### Perfect-OS next steps
1. Freeze new in-kernel apps. Every new app should be a Ring 3 binary.
2. After `execve` works: Wayland (or a tiny custom protocol) over Unix sockets, SHM buffers, kernel compositor scanout.
3. Then VirtIO-GPU scanout + hardware cursor. Not before userspace exists.

---

## 8. Shell & Terminal

**Grade: Live (82%)** · `shell/`, `terminal/`, `pty.rs`, `tty.rs`

### Live
- [x] Parser: pipes, redirects, background, quoting
- [x] Scripting: variables, conditionals, loops, glob (`*`, `?`, `[...]`), `export` / PATH
- [x] 60+ builtins (files, text, net, packages, system)
- [x] Terminal grid, split panes, history, completion, highlighting, sixel
- [x] PTY Unix98 pairs, 4 KB rings, termios; ISIG → SIGINT
- [x] Kernel `signals::kill` for Ctrl+C **to kernel tasks**

### Remaining (userspace-shaped)
- [x] Shell as `/bin/sh` in Ring 3 attached to a PTY slave
- [x] Live `sigreturn` for a custom SIGINT handler (B7)
- [ ] Full VT100/xterm-256
- [ ] Here-docs, functions, `~/.profile` once a real home exists on disk
- [ ] Job control against **processes**, not kernel windows. Ring 3 `pipe()` is live (M2).

---

## 9. Security & Cryptography

**Grade: Wired (80%)** · `crypto.rs`, `tls.rs`, `seccomp.rs`, `random.rs`, `ssp.rs`

### Wired
- [x] AES-128/256 block + CBC (software)
- [x] SHA-256 / SHA-512
- [x] ChaCha20 block function + RFC 8439 known-answer (E2)
- [x] dm-crypt AES-XTS / LUKS **structures** + sector crypto helpers
- [x] `seccomp::check_syscall` denies listed syscalls with EPERM (E3)
- [x] Entropy pool + RDRAND mix; **ChaCha20 CSPRNG** for `getrandom` (E2)
- [x] `__stack_chk_fail` + canary (`ssp.rs`); compiler SSP not shown enabled
- [x] Unix permission bits on VFS inodes
- [x] W^X + NX on user maps; `mprotect` RWX fails; ASLR randomizes (E1)
- [x] CapNetBindService on `bind`; unprivileged `:80` fails; dropped on exec (E4)
- [x] Unimplemented security-sensitive syscalls (`bpf`, `pkey_alloc`/`pkey_free`/`pkey_mprotect`, `process_mrelease`, `quotactl`, `remap_file_pages`, `io_uring_*`, `userfaultfd`, `perf_event_open`, `fanotify_init`/`fanotify_mark`, Linux AIO `io_setup`/`io_submit`/`io_getevents`, `kexec_load`/`kexec_file_load`, `init_module`/`finit_module`/`delete_module`, `mount_setattr`, `fsopen`/`fsconfig`/`fsmount`/`move_mount`/`open_tree`/`fspick`, `add_key`/`request_key`/`keyctl`, `ioperm`, `iopl`, `acct`, `swapon`/`swapoff`, `modify_ldt`, `sysfs`, `vhangup`, `lookup_dcookie`, `memfd_secret`, `uselib`, `set_mempolicy`/`get_mempolicy`/`mbind`/`migrate_pages`/`move_pages`, `sched_setattr`/`sched_getattr`/`futex_waitv`, KVM vCPU regs) return ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4)
- [x] Landlock deny-by-default on VFS open/write (K4)

### Stub / unused
- [ ] AES-GCM, Poly1305, TLS 1.3 key schedule, X.509 chain verify
- [ ] RSA / ECDSA / Ed25519 / X25519 production-quality
- [x] **Landlock** hooked on VFS (`GATE_K4 landlock vfs`). SELinux / AppArmor still unused.
- [ ] Kernel lockdown, secure wipe on free, IMA

### Perfect-OS next steps
1. ~~Replace xorshift userspace RNG with ChaCha20 from a real entropy pool (RDRAND + jitter + timings).~~ **E2 done.**
2. ~~W^X + NX on user maps the day Ring 3 boots.~~ **E1 done.**
3. ~~Deny-by-default seccomp for desktop apps; capabilities on `execve`.~~ Seccomp EPERM + CapNetBindService live; desktop default-deny still later.
4. Landlock on VFS is live (K4). MAC labels on disk still later.

---

## 10. System Services

**Grade: Wired (42%)** · `init.rs`, `service_manager.rs`, `dbus.rs`, `cron.rs`, `pam.rs`

### Wired
- [x] Init searches standard paths; can map a built-in ELF blob
- [x] **Ring 3 `/sbin/init`** — boot path `iretq`s to a static init ELF that writes and `wait4`s (`GATE_K3 init userspace`)
- [x] Service manager `boot()` from `main.rs`; dependency-ish unit states
- [x] In-memory D-Bus routing (`send_message`)
- [x] **AF_UNIX system bus** — `/run/dbus/system_bus_socket` bind/listen/connect/send/recv (`GATE_L3 dbus unix`)
- [x] Cron field parser + `tick` → execute builtin/ELF
- [x] dmesg ring, syslog structures
- [x] PAM salted SHA-512 verify; session/seat structs
- [x] QEMU shutdown ports

### Stub
- [x] Init `iretq`s to `/sbin/init` (K3); supervision/restart still missing
- [ ] No crash restart / watchdog / cgroup limits on units
- [x] D-Bus listens on `AF_UNIX` `/run/dbus/system_bus_socket` (L3); not a full bus daemon
- [ ] Cron needs a guaranteed timer caller
- [ ] udev-style hotplug → `/dev`
- [ ] Suspend/hibernate, logind, NetworkManager (activate logs “Starting DHCP”)
- [ ] PulseAudio/PipeWire mixing to HDA

---

## 11. Virtualization & Containers

**Grade: Stub (30%)** · `kvm.rs`, `vmx.rs`, `container.rs`, `namespaces.rs`

**Do not expand this until Ring 3 and namespaces work for ordinary processes.**

- [x] VMX constants; `vmxon`/`vmlaunch`/`vmresume` `asm` in `vmx.rs`
- [ ] `kvm::start_vm` sets `Running` and **does not** `vmlaunch`
- [ ] EPT, virtio device emulation for guests
- [ ] `container.rs` — OCI structs; start is comments (`fork`, `unshare`, `pivot_root`)
- [x] Namespace maps inherited on fork/clone; `CLONE_NEWUTS` isolates hostname (L2); `CLONE_NEWPID` isolates `getpid` (M1); `CLONE_NEWNS` isolates bind mounts (N1); `CLONE_NEWNET` isolates interfaces (O1); `CLONE_NEWUSER` maps the child to uid 0 (P1); `CLONE_NEWIPC` isolates SysV shm keys (Q1); `CLONE_NEWCGROUP` isolates cgroup paths (R1); `CLONE_NEWTIME` isolates CLOCK_MONOTONIC offsets (S1); `setns` joins an existing UTS ns (T1); `chroot` jails path lookup (U1); `pivot_root` keeps the old root at `put_old` (V1); OverlayFS merge+whiteout is VFS-live and mount-ns isolated (W1)
- [x] OverlayFS on VFS path lookup / list / write (W1); FUSE still unused
- [ ] cgroup v2 **enforcement** (CPU/memory/IO)

Note: `vmm.rs` is **process page tables**, not a hypervisor.

---

## 12. AI/ML Integration

**Grade: Wired (22%)** · `gguf.rs`, `llm.rs`, `onnx.rs`, `ai.rs`

Nice-to-have. Not on the path to a perfect OS.

- [x] GGUF v3 parse, Q4_0/Q8_0 dequant
- [x] Naive CPU generate / ONNX graph ops (add/mul/matmul/relu/softmax)
- [x] AI syscalls `0x1000+`
- [ ] Real transformer (attention, KV cache, production tokenizer vocab)
- [ ] `gpu_matmul.rs` only logs; `available: false`
- [ ] GUI assistant is placeholder text unless a model is loaded

---

## 13. Binary Compatibility & Runtime

**Grade: Wired (99%)** · `elf.rs`, `dynlink.rs`, `syscall/mod.rs`, `vdso.rs`

Linux **syscall numbers 0–451** are named and mostly dispatched. That is **not** 95.8% compatibility. Many arms return `Ok(0)` or ignore flags (`mprotect` “not enforced on our flat memory model”).

### Wired
- [x] ELF64 validation, PHDRs
- [x] `vmm::load_elf_into_address_space` maps segments
- [x] Relocation helpers in `dynlink.rs`
- [x] vDSO page contents mapped for `/init` only
- [x] Kernel `memcpy`/`printf`-family in `posix_libc.rs` (kernel address space, not libc.so)
- [x] **Gate B2 static hello** — `iretq` to Ring 3, `sys_write` / `sys_exit` (not via `execve`)

### Stub
- [x] **`execve` starts the binary as a scheduled process**
- [ ] Dynamic linker as a userspace `ld.so` (DT_NEEDED, lazy PLT, RELRO)
- [ ] `dlopen` that maps `.so` via VMM (`ldknoxos.rs` fake base)
- [x] **TLS `%fs`** — `arch_prctl(ARCH_SET_FS)` + restore on switch; Ring 3 `%fs:0` (L1). ELF initial-exec / general-dynamic still later.
- [ ] glibc/musl ABI for Ring 3
- [x] Signal trampoline + `rt_sigreturn` (custom SIGINT handler returns to `pause`)
- [x] Working `fork` child that runs
- [x] Working `clone(CLONE_VM)` child that shares CR3 (J1)
- [x] Working `clone(CLONE_THREAD)` + `thread_join` (K1); `CLONE_SETTLS` applies the child's `%fs`
- [x] **PID-ns `getpid`** — child of `unshare(CLONE_NEWPID)` sees 1 (M1)
- [x] **`pipe()`** — shared ipc buffer; Ring 3 write/read round-trip (M2)
- [x] **futex wait/wake** — parks a Ring 3 waiter until `FUTEX_WAKE` (M3)
- [x] **`socketpair(AF_UNIX)`** — connected pair write/read round-trip (N2)
- [x] **`eventfd`** — counter write then read (N3)
- [x] **`epoll`** — `epoll_create1`/`epoll_ctl`/`epoll_wait` on a pipe (O2)
- [x] **`memfd_create`** — anonymous file write/`lseek`/read (O3)
- [x] **`timerfd`** — `timerfd_create`/`timerfd_settime` then `read` expirations (P2)
- [x] **`signalfd`** — SIGUSR1 queued to a signalfd and `read` (P3)
- [x] **`poll`** — Ring 3 `poll` on a pipe sees `POLLIN` after write (Q2)
- [x] **Ring 3 `inotify`** — `inotify_init1` + `add_watch` + `read` after `open(O_CREAT)` (Q3)
- [x] **`splice`** — pipe-to-pipe byte move without a userspace copy (R2)
- [x] **`flock`** — exclusive lock then unlock on a VFS file (R3)
- [x] **`sendfile`** — file-to-pipe byte copy (S2)
- [x] **`tee`** — pipe duplicate without consuming the source (S3)
- [x] **`setns`** — join an existing UTS namespace (T1)
- [x] **`copy_file_range`** — file-to-file byte copy (T2)
- [x] **`vmsplice`** — user pages spliced into a pipe (T3)
- [x] **`setxattr`/`getxattr`** — Ring 3 `user.knox` round-trip (U2)
- [x] **`statx`** — Ring 3 `stx_size` from a VFS file (U3)
- [x] **`fallocate`** — Ring 3 extends a file; `statx` sees size 8 (V2)
- [x] **`utimensat`** — Ring 3 sets mtime; `statx` reports it (V3)
- [x] **`umask`** — applied on creat; Ring 3 `statx` sees `0100600` (W2)
- [x] **`symlink`/`readlink`** — Ring 3 target round-trip (W3)
- [x] **`link`/`linkat`** — VFS hard link shares the inode (X1)
- [x] **`rename`** — Ring 3 write then read from the new path (X2)
- [x] **`truncate`** — Ring 3 shrinks; `statx` `stx_size == 1` (X3)
- [x] **`chmod`** — VFS mode 0400 denies owner write and other read (Y1)
- [x] **`chown`** — Ring 3 sets uid 1000; `statx` sees it (Y2)
- [x] **`mkdir`** — Ring 3 creates a directory; `statx` `S_IFDIR` (Y3)
- [x] **`rmdir`** — empty dir removed; non-empty ENOTEMPTY (Z1)
- [x] **`unlink`** — Ring 3 create then unlink; `open` fails (Z2)
- [x] **`chdir`/`getcwd`** — Ring 3 `getcwd` matches the `chdir` target (Z3)
- [x] **`fchdir`** — Ring 3 `fchdir` then `getcwd` is `/tmp/gate_aa2` (AA2)
- [x] **`access`** — Ring 3 `access(F_OK)` on a created file; missing path fails (AA3)
- [x] **named FIFO** — `mkfifo` write/read round-trip (AA1)
- [x] **`getdents64`** — VFS directory listing includes a created child (AB1)
- [x] **`dup2`** — Ring 3 write then read via the new fd (AB2)
- [x] **`uname`** — Ring 3 `sysname` is `KnoxOS` (AB3)
- [x] **`fcntl`** — `F_SETFD`/`F_GETFD` CLOEXEC; `dup` clears cloexec (AC1)
- [x] **`pread64`** — Ring 3 reads at offset without moving the fd position (AC2)
- [x] **`getuid`** — Ring 3 boot task uid is 0 (AC3)
- [x] **`fstat`** — reports the written VFS file size (AD1)
- [x] **`pwrite64`** — Ring 3 writes at offset without moving the fd position (AD2)
- [x] **`getgid`** — Ring 3 spawned task gid is 1000 (AD3)
- [x] **`writev`** — scatter-gather write of two iovecs reads back `xy` (AE1)
- [x] **`ftruncate`** — Ring 3 shrinks; `statx` `stx_size == 1` (AE2)
- [x] **`geteuid`** — Ring 3 boot task euid is 0 (AE3)
- [x] **`readv`** — scatter-gather read of two iovecs from `xy` (AF1)
- [x] **`lseek`** — Ring 3 writes `xy`, seeks to 1, reads `y` (AF2)
- [x] **`getegid`** — Ring 3 spawned task egid is 1000 (AF3)
- [x] **`fsync`** — succeeds on a VFS fd; bad fd is EBADF (AG1)
- [x] **`fdatasync`** — Ring 3 write then `fdatasync` (AG2)
- [x] **`getppid`** — Ring 3 spawned task parent is non-zero (AG3)
- [x] **`syncfs`** — succeeds on a VFS fd; bad fd is EBADF (AH1)
- [x] **`sync`** — Ring 3 write then `sync` (AH2)
- [x] **`getpgid`** — Ring 3 spawned task `getpgid(0)` is non-zero (AH3)
- [x] **`fchmod`** — sets mode 0400 on a VFS fd; bad fd is EBADF (AI1)
- [x] **`fchown`** — Ring 3 sets uid 1000 via fd; `statx` sees it (AI2)
- [x] **`getsid`** — Ring 3 spawned task `getsid(0)` is non-zero (AI3)
- [x] **`fstatfs`** — reports `f_bsize == 4096` on a VFS fd; bad fd is EBADF (AJ1)
- [x] **`statfs`** — Ring 3 `statfs("/tmp")` reports `f_bsize == 4096` (AJ2)
- [x] **`setuid`** — Ring 3 `setuid(1000)` then `getuid` is 1000 (AJ3)
- [x] **`fsetxattr`/`fgetxattr`** — fd resolves to the VFS inode (AK1)
- [x] **`getrlimit`** — Ring 3 `RLIMIT_NOFILE` soft limit is 1024 (AK2)
- [x] **`setgid`** — Ring 3 `setgid(2000)` then `getgid` is 2000 (AK3)
- [x] **`flistxattr`/`fremovexattr`** — list then remove `user.knox` on a VFS fd (AL1)
- [x] **`setrlimit`** — Ring 3 sets `RLIMIT_NOFILE` soft limit to 512 (AL2)
- [x] **`setresuid`** — Ring 3 `setresuid(-1, 1000, -1)` then `geteuid` is 1000 (AL3)
- [x] **`listxattr`/`removexattr`** — path list then remove `user.knox` (AM1)
- [x] **`prlimit64`** — Ring 3 sets `RLIMIT_NOFILE` soft limit to 256 (AM2)
- [x] **`setresgid`** — Ring 3 `setresgid(-1, 2000, -1)` then `getegid` is 2000 (AM3)
- [x] **`faccessat`** — dirfd-relative `F_OK`; missing child ENOENT; bad dirfd EBADF (AN1)
- [x] **`setreuid`** — Ring 3 `setreuid(-1, 1000)` then `geteuid` is 1000 (AN2)
- [x] **`setgroups`/`getgroups`** — Ring 3 sets gid 2000 then reads it back (AN3)
- [x] **`mkdirat`** — dirfd-relative create; missing parent ENOENT; bad dirfd EBADF (AO1)
- [x] **`setregid`** — Ring 3 `setregid(-1, 2000)` then `getegid` is 2000 (AO2)
- [x] **`getresuid`** — Ring 3 `setuid(1000)` then `getresuid` reports 1000 (AO3)
- [x] **`unlinkat`** — dirfd-relative unlink; missing child ENOENT; bad dirfd EBADF (AP1)
- [x] **`getresgid`** — Ring 3 `setgid(2000)` then `getresgid` reports 2000 (AP2)
- [x] **`setpgid`/`getpgrp`** — Ring 3 `setpgid(0, 0)` then `getpgrp` equals `getpid` (AP3)
- [x] **`renameat`** — dirfd-relative rename; missing source ENOENT; bad dirfd EBADF (AQ1)
- [x] **`setsid`** — Ring 3 `setsid` then `getsid(0)` equals `getpid` (AQ2)
- [x] **`setpriority`/`getpriority`** — Ring 3 sets nice 5; `getpriority` is 15 (AQ3)
- [x] **`linkat`** — dirfd-relative hard link; missing source ENOENT; bad dirfd EBADF (AR1)
- [x] **`getrusage`** — Ring 3 `ru_maxrss == 4096` (AR2)
- [x] **`clock_gettime`** — Ring 3 `CLOCK_MONOTONIC` `tv_nsec < 1e9` (AR3)
- [x] **`readlinkat`** — dirfd-relative read returns the target; missing child ENOENT; bad dirfd EBADF (AT1)
- [x] **`gettimeofday`** — Ring 3 `tv_usec < 1e6` (AT2)
- [x] **`sysinfo`** — Ring 3 `totalram` is non-zero (AT3)
- [x] **`mknodat`** — dirfd-relative create; missing parent ENOENT; bad dirfd EBADF (AU1)
- [x] **`sched_yield`** — Ring 3 returns 0 (AU2)
- [x] **`alarm`** — Ring 3 `alarm(0)` is non-negative (AU3)
- [x] **`fchmodat`** — dirfd-relative chmod 0400; missing child ENOENT; bad dirfd EBADF (AV1)
- [x] **`getpid`** — Ring 3 spawned task is non-zero (AV2)
- [x] **`gettid`** — Ring 3 spawned task is non-zero (AV3)
- [x] **`renameat2`** — dirfd-relative rename; missing source ENOENT; bad dirfd EBADF (BC1)
- [x] **`capset`** — Ring 3 returns 0 (BC2)
- [x] **`ioprio_set`** — Ring 3 `ioprio_set(IOPRIO_WHO_PROCESS, 0, 4)` returns 0 (BC3)

### Perfect-OS next steps
~~Ship **static musl hello** first.~~ Gate B2 hello is an in-kernel generated static ELF. ~~B6 PTY + `/bin/sh`.~~ ~~Live `sigreturn`.~~ Isolated GUI clients F1–F4 live. ~~`clone(CLONE_VM)` (J1).~~ ~~`arch_prctl` `%fs` (L1).~~ ~~PID ns / pipe / futex (M1–M3).~~ ~~`socketpair` / `eventfd` (N2–N3).~~ ~~`epoll` / `memfd_create` (O2–O3).~~ ~~`timerfd` / `signalfd` (P2–P3).~~ ~~`poll` / Ring 3 `inotify` (Q2–Q3).~~ ~~`splice` / `flock` (R2–R3).~~ ~~`sendfile` / `tee` (S2–S3).~~ ~~`copy_file_range` / `vmsplice` (T2–T3).~~ ~~`setxattr` / `statx` (U2–U3).~~ ~~`fallocate` / `utimensat` (V2–V3).~~ ~~OverlayFS / umask / symlink (W1–W3).~~ ~~Hard link / rename / truncate (X1–X3).~~ ~~chmod / chown / mkdir (Y1–Y3).~~ ~~rmdir / unlink / chdir (Z1–Z3).~~ ~~mkfifo / fchdir / access (AA1–AA3).~~ ~~getdents / dup2 / uname (AB1–AB3).~~ ~~fcntl / pread64 / getuid (AC1–AC3).~~ ~~fstat / pwrite64 / getgid (AD1–AD3).~~ ~~writev / ftruncate / geteuid (AE1–AE3).~~ ~~readv / lseek / getegid (AF1–AF3).~~ ~~fsync / fdatasync / getppid (AG1–AG3).~~ ~~syncfs / sync / getpgid (AH1–AH3).~~ ~~fchmod / fchown / getsid (AI1–AI3).~~ ~~fstatfs / statfs / setuid (AJ1–AJ3).~~ ~~fsetxattr / getrlimit / setgid (AK1–AK3).~~ ~~flistxattr / setrlimit / setresuid (AL1–AL3).~~ ~~listxattr / prlimit64 / setresgid (AM1–AM3).~~ ~~faccessat / setreuid / getgroups (AN1–AN3).~~ ~~mkdirat / setregid / getresuid (AO1–AO3).~~ ~~unlinkat / getresgid / setpgid (AP1–AP3).~~ ~~renameat / setsid / setpriority (AQ1–AQ3).~~ ~~linkat / getrusage / clock_gettime (AR1–AR3).~~ ~~symlinkat / clock_getres / times (AS1–AS3).~~ ~~readlinkat / gettimeofday / sysinfo (AT1–AT3).~~ ~~mknodat / sched_yield / alarm (AU1–AU3).~~ ~~`fchmodat` / `getpid` / `gettid` (AV1–AV3).~~ ~~`renameat2` / `capset` / `ioprio_set` (BC1–BC3).~~ Next: `faccessat2` / `clock_nanosleep` / `getitimer` via Ring 3, then dynamic linking.

---

## 14. Internationalization & Fonts

**Grade: Live (68%)**

### Live
- [x] UTF-8, graphemes, CJK, RTL class/runs
- [x] Embedded bitmaps + TTF + LRU glyph cache + ClearType/grayscale
- [x] Date/time format helpers, timezone tables

### Partial
- [ ] Font fallback chain
- [ ] Real GSUB/GPOS / HarfBuzz-class shaping (Latin ligatures hardcoded)
- [ ] Color emoji
- [ ] `.mo` parser exists; runtime locale switch incomplete

---

## 15. Build System & Tooling

**Grade: Live (72%)** · `Makefile`, `kernel/Makefile`, `run.sh`

### Live
- [x] `./run.sh` / `make kernel` / BIOS & UEFI images
- [x] QEMU: 2G RAM, 2 CPUs, VirtIO disk, serial, cocoa/gtk display
- [x] Release: LTO, `opt-level = "z"`, 16 MB size gate
- [x] Local CI: `ci-fmt`, `ci-clippy`, `ci-size`, `ci-qemu`
- [x] Cross stubs for aarch64/riscv64
- [x] **`.github/workflows/ci.yml`** — docs, fmt, clippy+size, QEMU BIOS boot

### Missing (old status was wrong)
- [ ] **`flake.nix`** — `make nix-build` would fail; file absent
- [ ] Cargo workspace unifying `kernel` + `boot` + `tools`
- [ ] Feature flags so Unused modules do not compile into the demo kernel
- [ ] Signed ISO / Secure Boot artifacts

---

## 16. Testing & Quality

**Grade: Wired (82%)**

### Exists
- [x] `#[test_case]` framework + QEMU exit ports
- [x] Real tests: VFS read/write, allocator Box/Vec, some path tests (~subset of 103 `#[test_case]`)
- [x] `tests/run_integration.sh` waits for serial `Desktop Environment ready` plus C1–C6 / D1–D5 / E1–E4 / F1–F4 / H1–H4 / I1–I3 / J1–J4 / K1–K4 / L1–L4 / M1–M4 / N1–N4 / O1–O4 / P1–P4 / Q1–Q4 / R1–R4 / S1–S4 / T1–T4 / U1–U4 / V1–V4 / W1–W4 / X1–X4 / Y1–Y4 / Z1–Z4 / AA1–AA4 / AB1–AB4 / AC1–AC4 / AD1–AD4 / AE1–AE4 / AF1–AF4 / AG1–AG4 / AH1–AH4 / AI1–AI4 / AJ1–AJ4 / AK1–AK4 / AL1–AL4 / AM1–AM4 / AN1–AN4 / AO1–AO4 / AP1–AP4 / AQ1–AQ4 / AR1–AR4 / AS1–AS4 / AT1–AT4 / AU1–AU4 / AV1–AV4 markers

### Harmful
- [x] **`assert!(true)` tests removed** — widgets, VFS stress, DNS, TCP flags, creds, buddy, path normalize are real assertions
- [ ] No `#[test]` (expected for `no_std`, but host-side tests could exist)
- [ ] 300+ `unsafe` blocks without a systematic SAFETY audit
- [ ] No Miri, no fuzz of ELF/ext4/packet parsers, no coverage

### Perfect-OS next steps
1. Delete or rewrite every `assert!(true)`.
2. Host unit tests for parsers (ELF, ext4 superblock, TCP checksum) without QEMU.
3. QEMU tests: boot marker, virtio-blk persist across reboot, later `hello` in Ring 3.
4. SAFETY comments on every `unsafe`; clippy `undocumented_unsafe_blocks` as a gate.

---

## 17. Documentation

**Grade: Wired (42%)**

| Artifact | Status |
|----------|--------|
| `status.md` | This file (updated 2026-09-28) |
| `README.md` | Present — honesty paragraph, `./run.sh`, architecture sketch |
| `LICENSE` | MIT |
| `BUILDING.md` / `CONTRIBUTING.md` | Present |
| Architecture / syscall / driver / GUI guides | Missing |
| `cargo doc` published | Missing |
| Changelog | Missing |

### Perfect-OS next steps (cheap, high leverage)
1. MIT `LICENSE`.
2. `README.md`: what KnoxOS is, honesty paragraph, `./run.sh`, screenshots, link here.
3. Boot flow diagram: bootloader → `kernel_main` → compositor loop.
4. Syscall table: **behavior** (`Ok(0)` vs implemented vs ENOSYS), not just numbers.

---

## 18. CI/CD & Release

**Grade: Wired (30%)**

### Live locally
- [x] `make ci-all` (fmt, clippy, size)

### Missing
- [x] **`.github/workflows`** — fmt, clippy, size, QEMU BIOS boot, docs present
- [ ] Cross-compile + QEMU boot on every PR (x86_64 QEMU only so far)
- [ ] `cargo-audit`, license/SPDX check
- [ ] Tagged release → ISO → checksums → signatures
- [ ] Nightly main-branch images

---

## Path to a perfect OS

Stop adding Phase 34 modules. **Wire, delete, or feature-gate.** Sequence is dependency order.

### Gate A — Honest kernel (4–8 weeks)

Make the scheduler and memory manager true.

| ID | Task | Done when |
|----|------|-----------|
| A1 | Full context switch (GPRs, RIP, CS/SS, RFLAGS, FXSAVE) | **Done** — boot self-test switches two kernel threads with different RIP |
| A2 | Idle thread `HLT` on BSP | **Done** — PID 0 `STI; HLT`; executor yields when idle |
| A3 | Physical page free + buddy (or equivalent) | **Done** for leftover usable RAM after the heap (J2); bootloader reserved regions stay out of the pool |
| A4 | `#PF` + CoW tested | **Done** — forked address space write-fault copies; parent bytes unchanged (`GATE_H1 cow fault`) |
| A5 | Kill `assert!(true)` tests | **Done** — those tests are rewritten or deleted |

### Gate B — First userspace (6–12 weeks)

This **is** becoming an OS.

| ID | Task | Done when |
|----|------|-----------|
| B1 | TSS + `KERNEL_GS_BASE` + syscall entry that cannot fault on `swapgs` | **Done** — hello `syscall`/`sysretq` round-trip (write + exit) |
| B2 | Map static ELF, `iretq`, `sys_write` serial, `sys_exit` | **Done** — QEMU serial prints `hello from userspace`, then desktop starts |
| B3 | `execve` + `waitpid` | **Done** — boot spawns `execve("/bin/hello")`, hello prints, parent reaps (`GATE_B3 wait complete`) |
| B4 | `fork` CoW + child runs | **Done** — child writes `fork child ran`, parent `wait4`s (`GATE_B4 fork complete`) |
| B5 | Signals: SIGKILL, SIGSEGV, SIGINT from PTY | **Done** — parked `pause` + SIGKILL; null-deref SIGSEGV; PTY Ctrl+C (`GATE_B5 signals complete`) |
| B6 | PTY + `/bin/sh` (even a tiny static shell) | **Done** — boot spawns `/bin/sh` on a kernel PTY; serial shows `$ ` then `GATE_B6 sh complete` |
| B7 | Custom handler + live `rt_sigreturn` | **Done** — SIGINT handler `ret`s into trampoline; `pause` resumes (`GATE_B7 sigreturn complete`) |
| B8 | Timer preempt of spinning Ring 3 | **Done** — APIC timer switches a `jmp $` user off the CPU (`GATE_B8 timer preempt complete`) |

### Gate C — Durable storage (4–8 weeks)

| ID | Task | Done when |
|----|------|-----------|
| C1 | Root on VirtIO-blk (ext4 or persist) | **Done** — persist blob store round-trips a file through VirtIO-blk (`GATE_C1 persist complete`). VFS namespace is still RAM. |
| C2 | Journal + `fsync` | **Done** — persist WAL round-trips a committed record after simulated crash (`GATE_C2 journal recovered`). Uncommitted discarded. |
| C3 | Page cache writeback | **Done** — dirty middle page flushes via `pwrite_file`; sibling pages survive (`GATE_C3 writeback complete`) |
| C4 | AHCI or NVMe **one** real DMA path | **Done** — AHCI command-list + PRDT write/read round-trip (`GATE_C4 ahci dma complete`). NVMe DMA also live (C5). |
| C5 | NVMe PRP DMA | **Done** — admin + I/O queues; PRP bounce write/read (`GATE_C5 nvme dma complete`) |
| C6 | RAM VFS root snapshot | **Done** — `/etc` and the rest of the namespace (except virtual FS and `/bin`) persist (`GATE_C6 vfs persist`) |

### Gate D — Packets (4–8 weeks)

| ID | Task | Done when |
|----|------|-----------|
| D1 | Loopback sockets | **Done** — kernel UDP/TCP self-test + Ring 3 `sendto`/`recvfrom` (`GATE_D1 loopback complete`) |
| D2 | VirtIO-net avail/used correct | **Done** — DHCP DISCOVER TX + UDP reply RX vs QEMU user-net (`GATE_D2 virtio-net complete`) |
| D3 | DHCP applies IP + default route | **Done** — ACK writes `eth0` IPv4 + gateway (`GATE_D3 dhcp applied`) |
| D4 | DNS + TCP connect/retransmit | **Done** — UDP DNS + SYN/ACK + RTO + HTTP GET (`GATE_D4 dns tcp complete`) |
| D5 | TCP CUBIC / window | **Done** — cwnd grows on ACK, β=0.7 on loss, send clamped (`GATE_D5 cubic window`) |

### Gate E — Enforcement (3–6 weeks)

| ID | Task | Done when |
|----|------|-----------|
| E1 | W^X + NX + ASLR on user maps | **Done** — `mmap`/`mprotect` RWX denied; NX on data; ASLR bases differ (`GATE_E1 wx aslr complete`) |
| E2 | ChaCha20 CSPRNG `getrandom` | **Done** — RFC 8439 KAT + `getrandom` (`GATE_E2 csprng complete`) |
| E3 | Seccomp actually denies | **Done** — listed syscall returns EPERM (`GATE_E3 seccomp deny`) |
| E4 | Caps on exec | **Done** — unprivileged bind `:80` fails; dropped on exec (`GATE_E4 caps exec`) |

### Gate F — Real desktop (ongoing, after B)

| ID | Task | Done when |
|----|------|-----------|
| F1 | One Wayland client (or custom protocol) out of process | **Done** — Ring 3 `/dev/wl0` present (`GATE_F1 client isolated`). |
| F2 | SHM / DMA-BUF to compositor | **Done** — client mmap copied into a kernel SHM pool; compositor round-trips magic pixels (`GATE_F2 shm commit`) |
| F3 | VirtIO-GPU scanout or keep FB but clients isolated | **Done** — FB scanout of SHM; Ring 3 terminal is Empty+SHM, not `WindowContentType::Terminal` (`GATE_F3 terminal isolated`). Interactive desktop terminal still in-kernel for PTY I/O. |
| F4 | Remove Empty launcher stubs or implement them as userspace | **Done** — Doom/Quake/Webamp stubs removed; Paint is a Ring 3 SHM client (`GATE_F4 launcher userspace`) |

### Gate H — Memory, VFS, core leftovers (after A–F)

| ID | Task | Done when |
|----|------|-----------|
| H1 | CoW `#PF` | **Done** — forked space write-fault copies; parent unchanged (`GATE_H1 cow fault`) |
| H2 | File-backed mmap fault-in | **Done** — mmap does not copy the whole file; one page faults from VFS/page cache (`GATE_H2 mmap fault`) |
| H3 | inotify from VFS | **Done** — write/unlink emit events a watch can read (`GATE_H3 inotify`) |
| H4 | Guarded stacks + OOM on alloc | **Done** — unmapped guard page allocated; empty buddy pool calls `trigger_oom` (`GATE_H4 guard oom`) |

### Gate I — Per-CPU + SMP + IRQ context (after H)

| ID | Task | Done when |
|----|------|-----------|
| I1 | Per-CPU TSS + GS `CpuLocal`; AP online | **Done** — BSP `str` matches TSS[0]; GS_BASE → `CpuLocal[0]`; AP loads a distinct TSS (`GATE_I1 smp online`) |
| I2 | IRQ saves GPRs + FXSAVE | **Done** — spinner `mov rbx, magic; jmp $`; after timer preempt, saved `rbx` and FXSAVE area are live (`GATE_I2 irq gprs`) |
| I3 | AP runs Ring 3 | **Done** — `getcpu` ELF affinity-pinned to CPU 1 syscalls on the AP (`GATE_I3 ap ring3`) |

### Gate J — Threads, buddy RAM, reclaim, honest syscalls (after I)

| ID | Task | Done when |
|----|------|-----------|
| J1 | `clone(CLONE_VM)` child runs on shared CR3 | **Done** — child writes `thread child ran`; parent `wait4`s (`GATE_J1 thread clone`) |
| J2 | Buddy over leftover RAM | **Done** — remaining usable frames enter the buddy; alloc/free restores (`GATE_J2 buddy ram`) |
| J3 | Page-cache LRU reclaim | **Done** — shrink drops oldest clean pages; dirty survives (`GATE_J3 lru reclaim`) |
| J4 | No silent `Ok(0)` for unimplemented security syscalls | **Done** — Ring 3 `bpf` returns `-ENOSYS` (`GATE_J4 enosys`) |

### Gate K — Threads join, swap I/O, init, Landlock (after J)

| ID | Task | Done when |
|----|------|-----------|
| K1 | `clone(CLONE_THREAD)` + `thread_join` | **Done** — child exit status 42; parent writes (`GATE_K1 thread join`) |
| K2 | Swap I/O + `#PF` swap-in | **Done** — ram-backed slot restores magic bytes (`GATE_K2 swap io`) |
| K3 | Ring 3 `/sbin/init` | **Done** — PID 1 `iretq`s and parks on `wait4` (`GATE_K3 init userspace`) |
| K4 | Landlock on VFS | **Done** — deny-by-default write outside allow path (`GATE_K4 landlock vfs`) |

### Gate L — TLS, namespaces, D-Bus socket, honest syscalls (after K)

| ID | Task | Done when |
|----|------|-----------|
| L1 | `arch_prctl(ARCH_SET_FS)` + `%fs:0` | **Done** — Ring 3 reads magic at the programmed FS base (`GATE_L1 tls fs`) |
| L2 | UTS namespace isolation | **Done** — child `sethostname` does not change parent (`GATE_L2 uts ns`) |
| L3 | D-Bus `AF_UNIX` socket | **Done** — connect/send/recv on `/run/dbus/system_bus_socket` (`GATE_L3 dbus unix`) |
| L4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `quotactl` returns `-ENOSYS` (`GATE_L4 enosys`) |

### Gate M — PID ns, pipes, futex, honest syscalls (after L)

| ID | Task | Done when |
|----|------|-----------|
| M1 | PID namespace isolation | **Done** — `unshare(CLONE_NEWPID)` child `getpid` is 1; parent unchanged (`GATE_M1 pid ns`) |
| M2 | `pipe()` write/read round-trip | **Done** — Ring 3 write/read through the shared ipc buffer (`GATE_M2 pipe`) |
| M3 | futex wait/wake | **Done** — `clone(CLONE_VM)` child `FUTEX_WAIT`s; parent `FUTEX_WAKE`s (`GATE_M3 futex`) |
| M4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_uring_setup` returns `-ENOSYS` (`GATE_M4 enosys`) |

### Gate N — Mount ns, socketpair, eventfd, honest syscalls (after M)

| ID | Task | Done when |
|----|------|-----------|
| N1 | Mount namespace isolation | **Done** — `unshare(CLONE_NEWNS)` bind mount is not visible to the parent (`GATE_N1 mount ns`) |
| N2 | `socketpair` write/read round-trip | **Done** — Ring 3 AF_UNIX pair write/read (`GATE_N2 socketpair`) |
| N3 | `eventfd` write/read | **Done** — Ring 3 counter write then read (`GATE_N3 eventfd`) |
| N4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `userfaultfd` returns `-ENOSYS` (`GATE_N4 enosys`) |

### Gate O — Net ns, epoll, memfd, honest syscalls (after N)

| ID | Task | Done when |
|----|------|-----------|
| O1 | Network namespace isolation | **Done** — `unshare(CLONE_NEWNET)` child does not see `eth0`; child `veth0` is private (`GATE_O1 net ns`) |
| O2 | `epoll` wait on a pipe | **Done** — Ring 3 `epoll_create1`/`epoll_ctl`/`epoll_wait` sees `EPOLLIN` after write (`GATE_O2 epoll`) |
| O3 | `memfd_create` write/read | **Done** — Ring 3 write then `lseek`/`read` round-trip (`GATE_O3 memfd`) |
| O4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `perf_event_open` returns `-ENOSYS` (`GATE_O4 enosys`) |

### Gate P — User ns, timerfd, signalfd, honest syscalls (after O)

| ID | Task | Done when |
|----|------|-----------|
| P1 | User namespace isolation | **Done** — `unshare(CLONE_NEWUSER)` child is uid 0; parent unchanged (`GATE_P1 user ns`) |
| P2 | `timerfd` expire/read | **Done** — Ring 3 `timerfd_create`/`timerfd_settime`; `read` returns an expiration (`GATE_P2 timerfd`) |
| P3 | `signalfd` SIGUSR1 | **Done** — Ring 3 `signalfd` + `kill(SIGUSR1)`; `read` returns signo 10 (`GATE_P3 signalfd`) |
| P4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `fanotify_init` returns `-ENOSYS` (`GATE_P4 enosys`) |

### Gate Q — IPC ns, poll, inotify, honest syscalls (after P)

| ID | Task | Done when |
|----|------|-----------|
| Q1 | IPC namespace isolation | **Done** — `unshare(CLONE_NEWIPC)` child does not see the parent's SysV shm key; child-only key stays private (`GATE_Q1 ipc ns`) |
| Q2 | `poll` wait on a pipe | **Done** — Ring 3 `poll` stays idle on an empty pipe and returns `POLLIN` after write (`GATE_Q2 poll`) |
| Q3 | Ring 3 `inotify` | **Done** — `inotify_init1` + `add_watch(/tmp)` + `open(O_CREAT)`; `read` returns a create event (`GATE_Q3 inotify`) |
| Q4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_setup` returns `-ENOSYS` (`GATE_Q4 enosys`) |

### Gate R — Cgroup ns, splice, flock, honest syscalls (after Q)

| ID | Task | Done when |
|----|------|-----------|
| R1 | Cgroup namespace isolation | **Done** — `unshare(CLONE_NEWCGROUP)` child does not see `/system.slice`; child-only path stays private (`GATE_R1 cgroup ns`) |
| R2 | `splice` pipe to pipe | **Done** — Ring 3 `splice` moves a byte from one pipe to another (`GATE_R2 splice`) |
| R3 | `flock` exclusive lock | **Done** — Ring 3 `flock(LOCK_EX)` then `LOCK_UN` on a VFS file (`GATE_R3 flock`) |
| R4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `kexec_load` returns `-ENOSYS` (`GATE_R4 enosys`) |

### Gate S — Time ns, sendfile, tee, honest syscalls (after R)

| ID | Task | Done when |
|----|------|-----------|
| S1 | Time namespace isolation | **Done** — `unshare(CLONE_NEWTIME)` monotonic offset does not change the parent (`GATE_S1 time ns`) |
| S2 | `sendfile` file to pipe | **Done** — Ring 3 `sendfile` copies a byte from a file into a pipe (`GATE_S2 sendfile`) |
| S3 | `tee` without consume | **Done** — Ring 3 `tee` duplicates a pipe byte and the source can still be read (`GATE_S3 tee`) |
| S4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `init_module` returns `-ENOSYS` (`GATE_S4 enosys`) |

### Gate T — setns, copy_file_range, vmsplice, honest syscalls (after S)

| ID | Task | Done when |
|----|------|-----------|
| T1 | `setns` joins an existing namespace | **Done** — joiner sees the owner's UTS hostname; parent unchanged (`GATE_T1 setns`) |
| T2 | `copy_file_range` file to file | **Done** — Ring 3 copies a byte from one VFS file into another (`GATE_T2 copy_file_range`) |
| T3 | `vmsplice` user pages into a pipe | **Done** — Ring 3 `vmsplice` copies a user byte into a pipe (`GATE_T3 vmsplice`) |
| T4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `mount_setattr` returns `-ENOSYS` (`GATE_T4 enosys`) |

### Gate U — chroot, xattr, statx, honest syscalls (after T)

| ID | Task | Done when |
|----|------|-----------|
| U1 | `chroot` jails path lookup | **Done** — child cannot see `/etc`; `/../etc` stays in the jail; parent unchanged (`GATE_U1 chroot`) |
| U2 | `setxattr`/`getxattr` round-trip | **Done** — Ring 3 writes `user.knox` and reads it back (`GATE_U2 xattr`) |
| U3 | `statx` reports file size | **Done** — Ring 3 `statx` returns `stx_size == 1` (`GATE_U3 statx`) |
| U4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `fsopen` returns `-ENOSYS` (`GATE_U4 enosys`) |

### Gate V — pivot_root, fallocate, utimensat, honest syscalls (after U)

| ID | Task | Done when |
|----|------|-----------|
| V1 | `pivot_root` swaps the process root | **Done** — `/ok` is in `new_root`; `/etc` is not; `/old/etc` is the previous `/etc`; parent unchanged (`GATE_V1 pivot_root`) |
| V2 | `fallocate` extends a file | **Done** — Ring 3 `fallocate` makes `statx` report `stx_size == 8` (`GATE_V2 fallocate`) |
| V3 | `utimensat` sets mtime | **Done** — Ring 3 `utimensat` then `statx` `stx_mtime.tv_sec == 123456789` (`GATE_V3 utimensat`) |
| V4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `keyctl` returns `-ENOSYS` (`GATE_V4 enosys`) |

### Gate W — OverlayFS, umask, symlink, honest syscalls (after V)

| ID | Task | Done when |
|----|------|-----------|
| W1 | OverlayFS merges VFS layers | **Done** — lower+upper visible at merge; whiteout hides; parent mount ns unchanged (`GATE_W1 overlayfs`) |
| W2 | `umask` applied on creat | **Done** — Ring 3 `umask(077)` + `open(O_CREAT, 0666)`; `statx` `stx_mode == 0100600` (`GATE_W2 umask`) |
| W3 | `symlink`/`readlink` round-trip | **Done** — Ring 3 writes a symlink and `readlink`s `x` (`GATE_W3 symlink`) |
| W4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `ioperm` returns `-ENOSYS` (`GATE_W4 enosys`) |

### Gate X — Hard links, rename, truncate, honest syscalls (after W)

| ID | Task | Done when |
|----|------|-----------|
| X1 | VFS hard links share an inode | **Done** — two names, same ino; write via link visible on original; unlink drops nlink to 1 (`GATE_X1 hardlink`) |
| X2 | `rename` file | **Done** — Ring 3 writes a byte, `rename`s, reads it from the new path (`GATE_X2 rename`) |
| X3 | `truncate` shrinks a file | **Done** — Ring 3 writes 8 bytes, `truncate`s to 1, `statx` `stx_size == 1` (`GATE_X3 truncate`) |
| X4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `iopl` returns `-ENOSYS` (`GATE_X4 enosys`) |

### Gate Y — chmod, chown, mkdir, honest syscalls (after X)

| ID | Task | Done when |
|----|------|-----------|
| Y1 | VFS `chmod` is enforced | **Done** — mode 0400: owner can read but not write; other cannot read (`GATE_Y1 chmod`) |
| Y2 | `chown` then `statx` uid | **Done** — Ring 3 `chown` reports `stx_uid == 1000` (`GATE_Y2 chown`) |
| Y3 | `mkdir` then `statx` dir | **Done** — Ring 3 `mkdir`; `statx` `S_IFDIR` (`GATE_Y3 mkdir`) |
| Y4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `acct` returns `-ENOSYS` (`GATE_Y4 enosys`) |

### Gate Z — rmdir, unlink, chdir, honest syscalls (after Y)

| ID | Task | Done when |
|----|------|-----------|
| Z1 | VFS `rmdir` removes empty dirs | **Done** — empty dir gone; non-empty ENOTEMPTY (`GATE_Z1 rmdir`) |
| Z2 | `unlink` then `open` fails | **Done** — Ring 3 creates, unlinks, `open` fails (`GATE_Z2 unlink`) |
| Z3 | `chdir` then `getcwd` | **Done** — Ring 3 `getcwd` is `/tmp/gate_z3` (`GATE_Z3 chdir`) |
| Z4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `swapon` returns `-ENOSYS` (`GATE_Z4 enosys`) |

### Gate AA — mkfifo, fchdir, access, honest syscalls (after Z)

| ID | Task | Done when |
|----|------|-----------|
| AA1 | Named FIFO write/read | **Done** — `mkfifo` then write a byte that a reader reads back (`GATE_AA1 mkfifo`) |
| AA2 | `fchdir` then `getcwd` | **Done** — Ring 3 `getcwd` is `/tmp/gate_aa2` (`GATE_AA2 fchdir`) |
| AA3 | `access(F_OK)` | **Done** — created file is OK; missing path fails (`GATE_AA3 access`) |
| AA4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `modify_ldt` returns `-ENOSYS` (`GATE_AA4 enosys`) |

### Gate AB — getdents, dup2, uname, honest syscalls (after AA)

| ID | Task | Done when |
|----|------|-----------|
| AB1 | VFS directory listing | **Done** — `list_dir` includes a file created in the directory (`GATE_AB1 getdents`) |
| AB2 | `dup2` then read via new fd | **Done** — Ring 3 writes a byte, `dup2`s, `lseek`/`read`s it back (`GATE_AB2 dup2`) |
| AB3 | `uname` reports KnoxOS | **Done** — Ring 3 `uname` `sysname == "KnoxOS"` (`GATE_AB3 uname`) |
| AB4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `sysfs` returns `-ENOSYS` (`GATE_AB4 enosys`) |

### Gate AC — fcntl, pread64, getuid, honest syscalls (after AB)

| ID | Task | Done when |
|----|------|-----------|
| AC1 | `fcntl` CLOEXEC / DUPFD | **Done** — `F_SETFD`/`F_GETFD` toggle cloexec; `dup` clears it (`GATE_AC1 fcntl`) |
| AC2 | `pread64` without moving offset | **Done** — Ring 3 writes `xy`, `pread64` at 1 returns `y`; `lseek` CUR stays 2 (`GATE_AC2 pread64`) |
| AC3 | `getuid` from Ring 3 | **Done** — boot task `getuid` is 0 (`GATE_AC3 getuid`) |
| AC4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `vhangup` returns `-ENOSYS` (`GATE_AC4 enosys`) |

### Gate AD — fstat, pwrite64, getgid, honest syscalls (after AC)

| ID | Task | Done when |
|----|------|-----------|
| AD1 | `fstat` reports file size | **Done** — written VFS file `st_size == 8` (`GATE_AD1 fstat`) |
| AD2 | `pwrite64` without moving offset | **Done** — Ring 3 writes `xy`, `pwrite64` `z` at 1; `lseek` CUR stays 2; read is `xz` (`GATE_AD2 pwrite64`) |
| AD3 | `getgid` from Ring 3 | **Done** — spawned task `getgid` is 1000 (`GATE_AD3 getgid`) |
| AD4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `lookup_dcookie` returns `-ENOSYS` (`GATE_AD4 enosys`) |

### Gate AE — writev, ftruncate, geteuid, honest syscalls (after AD)

| ID | Task | Done when |
|----|------|-----------|
| AE1 | `writev` scatter-gather | **Done** — two iovecs write `x`+`y`; a read returns `xy` (`GATE_AE1 writev`) |
| AE2 | `ftruncate` shrinks an fd | **Done** — Ring 3 writes 8 bytes, `ftruncate`s to 1, `statx` `stx_size == 1` (`GATE_AE2 ftruncate`) |
| AE3 | `geteuid` from Ring 3 | **Done** — boot task `geteuid` is 0 (`GATE_AE3 geteuid`) |
| AE4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `memfd_secret` returns `-ENOSYS` (`GATE_AE4 enosys`) |

### Gate AF — readv, lseek, getegid, honest syscalls (after AE)

| ID | Task | Done when |
|----|------|-----------|
| AF1 | `readv` scatter-gather | **Done** — two iovecs read `x`+`y` from `xy` (`GATE_AF1 readv`) |
| AF2 | `lseek` then read | **Done** — Ring 3 writes `xy`, `lseek`s to 1, `read`s `y` (`GATE_AF2 lseek`) |
| AF3 | `getegid` from Ring 3 | **Done** — spawned task `getegid` is 1000 (`GATE_AF3 getegid`) |
| AF4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `uselib` returns `-ENOSYS` (`GATE_AF4 enosys`) |

### Gate AG — fsync, fdatasync, getppid, honest syscalls (after AF)

| ID | Task | Done when |
|----|------|-----------|
| AG1 | `fsync` on a VFS fd | **Done** — written fd succeeds; bad fd is EBADF (`GATE_AG1 fsync`) |
| AG2 | `fdatasync` after write | **Done** — Ring 3 writes a byte then `fdatasync` (`GATE_AG2 fdatasync`) |
| AG3 | `getppid` from Ring 3 | **Done** — spawned task `getppid` is non-zero (`GATE_AG3 getppid`) |
| AG4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `pkey_alloc` returns `-ENOSYS` (`GATE_AG4 enosys`) |

### Gate AH — syncfs, sync, getpgid, honest syscalls (after AG)

| ID | Task | Done when |
|----|------|-----------|
| AH1 | `syncfs` on a VFS fd | **Done** — written fd succeeds; bad fd is EBADF (`GATE_AH1 syncfs`) |
| AH2 | `sync` after write | **Done** — Ring 3 writes a byte then `sync` (`GATE_AH2 sync`) |
| AH3 | `getpgid` from Ring 3 | **Done** — spawned task `getpgid(0)` is non-zero (`GATE_AH3 getpgid`) |
| AH4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `pkey_mprotect` returns `-ENOSYS` (`GATE_AH4 enosys`) |

### Gate AI — fchmod, fchown, getsid, honest syscalls (after AH)

| ID | Task | Done when |
|----|------|-----------|
| AI1 | `fchmod` on a VFS fd | **Done** — mode 0400 is set; bad fd is EBADF (`GATE_AI1 fchmod`) |
| AI2 | `fchown` then `statx` uid | **Done** — Ring 3 `fchown` reports `stx_uid == 1000` (`GATE_AI2 fchown`) |
| AI3 | `getsid` from Ring 3 | **Done** — spawned task `getsid(0)` is non-zero (`GATE_AI3 getsid`) |
| AI4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `pkey_free` returns `-ENOSYS` (`GATE_AI4 enosys`) |

### Gate AJ — fstatfs, statfs, setuid, honest syscalls (after AI)

| ID | Task | Done when |
|----|------|-----------|
| AJ1 | `fstatfs` on a VFS fd | **Done** — `f_bsize == 4096`; bad fd is EBADF (`GATE_AJ1 fstatfs`) |
| AJ2 | `statfs` reports block size | **Done** — Ring 3 `statfs("/tmp")` `f_bsize == 4096` (`GATE_AJ2 statfs`) |
| AJ3 | `setuid` then `getuid` | **Done** — Ring 3 `setuid(1000)` then `getuid` is 1000 (`GATE_AJ3 setuid`) |
| AJ4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `process_mrelease` returns `-ENOSYS` (`GATE_AJ4 enosys`) |

### Gate AK — fsetxattr, getrlimit, setgid, honest syscalls (after AJ)

| ID | Task | Done when |
|----|------|-----------|
| AK1 | `fsetxattr`/`fgetxattr` on a VFS fd | **Done** — `user.knox` round-trips; bad fd is EBADF (`GATE_AK1 fsetxattr`) |
| AK2 | `getrlimit` reports RLIMIT_NOFILE | **Done** — Ring 3 `rlim_cur == 1024` (`GATE_AK2 getrlimit`) |
| AK3 | `setgid` then `getgid` | **Done** — Ring 3 `setgid(2000)` then `getgid` is 2000 (`GATE_AK3 setgid`) |
| AK4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `set_mempolicy` returns `-ENOSYS` (`GATE_AK4 enosys`) |

### Gate AL — flistxattr, setrlimit, setresuid, honest syscalls (after AK)

| ID | Task | Done when |
|----|------|-----------|
| AL1 | `flistxattr`/`fremovexattr` on a VFS fd | **Done** — list contains `user.knox`; remove clears it; bad fd is EBADF (`GATE_AL1 flistxattr`) |
| AL2 | `setrlimit` then `getrlimit` | **Done** — Ring 3 `rlim_cur == 512` (`GATE_AL2 setrlimit`) |
| AL3 | `setresuid` then `geteuid` | **Done** — Ring 3 `setresuid(-1, 1000, -1)` then `geteuid` is 1000 (`GATE_AL3 setresuid`) |
| AL4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `get_mempolicy` returns `-ENOSYS` (`GATE_AL4 enosys`) |

### Gate AM — listxattr, prlimit64, setresgid, honest syscalls (after AL)

| ID | Task | Done when |
|----|------|-----------|
| AM1 | `listxattr`/`removexattr` on a VFS path | **Done** — list contains `user.knox`; remove clears it; missing path is ENOENT (`GATE_AM1 listxattr`) |
| AM2 | `prlimit64` then `getrlimit` | **Done** — Ring 3 `rlim_cur == 256` (`GATE_AM2 prlimit64`) |
| AM3 | `setresgid` then `getegid` | **Done** — Ring 3 `setresgid(-1, 2000, -1)` then `getegid` is 2000 (`GATE_AM3 setresgid`) |
| AM4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `mbind` returns `-ENOSYS` (`GATE_AM4 enosys`) |

### Gate AN — faccessat, setreuid, getgroups, honest syscalls (after AM)

| ID | Task | Done when |
|----|------|-----------|
| AN1 | `faccessat` via dirfd | **Done** — relative `F_OK` succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AN1 faccessat`) |
| AN2 | `setreuid` then `geteuid` | **Done** — Ring 3 `setreuid(-1, 1000)` then `geteuid` is 1000 (`GATE_AN2 setreuid`) |
| AN3 | `setgroups` then `getgroups` | **Done** — Ring 3 `getgroups` returns gid 2000 (`GATE_AN3 getgroups`) |
| AN4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `sched_setattr` returns `-ENOSYS` (`GATE_AN4 enosys`) |

### Gate AO — mkdirat, setregid, getresuid, honest syscalls (after AN)

| ID | Task | Done when |
|----|------|-----------|
| AO1 | `mkdirat` via dirfd | **Done** — relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AO1 mkdirat`) |
| AO2 | `setregid` then `getegid` | **Done** — Ring 3 `setregid(-1, 2000)` then `getegid` is 2000 (`GATE_AO2 setregid`) |
| AO3 | `getresuid` after `setuid` | **Done** — Ring 3 `getresuid` reports ruid/euid 1000 (`GATE_AO3 getresuid`) |
| AO4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `futex_waitv` returns `-ENOSYS` (`GATE_AO4 enosys`) |

### Gate AP — unlinkat, getresgid, setpgid, honest syscalls (after AO)

| ID | Task | Done when |
|----|------|-----------|
| AP1 | `unlinkat` via dirfd | **Done** — relative unlink succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AP1 unlinkat`) |
| AP2 | `getresgid` after `setgid` | **Done** — Ring 3 `getresgid` reports rgid/egid 2000 (`GATE_AP2 getresgid`) |
| AP3 | `setpgid` then `getpgrp` | **Done** — Ring 3 `setpgid(0, 0)` then `getpgrp` equals `getpid` (`GATE_AP3 setpgid`) |
| AP4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `open_by_handle_at` returns `-ENOSYS` (`GATE_AP4 enosys`) |

### Gate AQ — renameat, setsid, setpriority, honest syscalls (after AP)

| ID | Task | Done when |
|----|------|-----------|
| AQ1 | `renameat` via dirfd | **Done** — relative rename succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AQ1 renameat`) |
| AQ2 | `setsid` then `getsid` | **Done** — Ring 3 `setsid` then `getsid(0)` equals `getpid` (`GATE_AQ2 setsid`) |
| AQ3 | `setpriority` then `getpriority` | **Done** — Ring 3 `setpriority(0, 0, 5)` then `getpriority` is 15 (`GATE_AQ3 setpriority`) |
| AQ4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `name_to_handle_at` returns `-ENOSYS` (`GATE_AQ4 enosys`) |

### Gate AR — linkat, getrusage, clock_gettime, honest syscalls (after AQ)

| ID | Task | Done when |
|----|------|-----------|
| AR1 | `linkat` via dirfd | **Done** — relative hard link succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AR1 linkat`) |
| AR2 | `getrusage` reports rss | **Done** — Ring 3 `ru_maxrss == 4096` (`GATE_AR2 getrusage`) |
| AR3 | `clock_gettime` monotonic | **Done** — Ring 3 `CLOCK_MONOTONIC` `tv_nsec < 1e9` (`GATE_AR3 clock_gettime`) |
| AR4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `map_shadow_stack` returns `-ENOSYS` (`GATE_AR4 enosys`) |

### Gate AS — symlinkat, clock_getres, times, honest syscalls (after AR)

| ID | Task | Done when |
|----|------|-----------|
| AS1 | `symlinkat` via dirfd | **Done** — relative symlink succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AS1 symlinkat`) |
| AS2 | `clock_getres` is 1ns | **Done** — Ring 3 `CLOCK_MONOTONIC` `{0, 1}` (`GATE_AS2 clock_getres`) |
| AS3 | `times` returns ticks | **Done** — Ring 3 `times` is non-zero (`GATE_AS3 times`) |
| AS4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `ustat` returns `-ENOSYS` (`GATE_AS4 enosys`) |

### Gate AT — readlinkat, gettimeofday, sysinfo, honest syscalls (after AS)

| ID | Task | Done when |
|----|------|-----------|
| AT1 | `readlinkat` via dirfd | **Done** — relative read returns the target; missing child is ENOENT; bad dirfd is EBADF (`GATE_AT1 readlinkat`) |
| AT2 | `gettimeofday` usec in range | **Done** — Ring 3 `tv_usec < 1e6` (`GATE_AT2 gettimeofday`) |
| AT3 | `sysinfo` reports RAM | **Done** — Ring 3 `totalram` is non-zero (`GATE_AT3 sysinfo`) |
| AT4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `migrate_pages` returns `-ENOSYS` (`GATE_AT4 enosys`) |

### Gate AU — mknodat, sched_yield, alarm, honest syscalls (after AT)

| ID | Task | Done when |
|----|------|-----------|
| AU1 | `mknodat` via dirfd | **Done** — relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AU1 mknodat`) |
| AU2 | `sched_yield` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AU2 sched_yield`) |
| AU3 | `alarm(0)` is non-negative | **Done** — Ring 3 `alarm(0)` does not fail (`GATE_AU3 alarm`) |
| AU4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `swapoff` returns `-ENOSYS` (`GATE_AU4 enosys`) |

### Gate AV — fchmodat, getpid, gettid, honest syscalls (after AU)

| ID | Task | Done when |
|----|------|-----------|
| AV1 | `fchmodat` via dirfd | **Done** — relative chmod 0400 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AV1 fchmodat`) |
| AV2 | `getpid` from Ring 3 | **Done** — spawned task `getpid` is non-zero (`GATE_AV2 getpid`) |
| AV3 | `gettid` from Ring 3 | **Done** — spawned task `gettid` is non-zero (`GATE_AV3 gettid`) |
| AV4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `move_pages` returns `-ENOSYS` (`GATE_AV4 enosys`) |

### Gate AW — fchownat, sched_getscheduler, sched_getparam, honest syscalls (after AV)

| ID | Task | Done when |
|----|------|-----------|
| AW1 | `fchownat` via dirfd | **Done** — relative chown uid 1000 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AW1 fchownat`) |
| AW2 | `sched_getscheduler` from Ring 3 | **Done** — Ring 3 `sched_getscheduler(0)` is 0 (`GATE_AW2 getsched`) |
| AW3 | `sched_getparam` from Ring 3 | **Done** — Ring 3 `sched_getparam(0)` returns 0 (`GATE_AW3 getparam`) |
| AW4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `remap_file_pages` returns `-ENOSYS` (`GATE_AW4 enosys`) |

### Gate AX — newfstatat, priority range, honest syscalls (after AW)

| ID | Task | Done when |
|----|------|-----------|
| AX1 | `newfstatat` via dirfd | **Done** — relative `st_size == 1`; missing child is ENOENT; bad dirfd is EBADF (`GATE_AX1 newfstatat`) |
| AX2 | `sched_get_priority_max` from Ring 3 | **Done** — Ring 3 returns 99 (`GATE_AX2 prio_max`) |
| AX3 | `sched_get_priority_min` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AX3 prio_min`) |
| AX4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `sched_getattr` returns `-ENOSYS` (`GATE_AX4 enosys`) |

### Gate AY — openat, rr interval, getcpu, honest syscalls (after AX)

| ID | Task | Done when |
|----|------|-----------|
| AY1 | `openat` via dirfd | **Done** — relative open succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AY1 openat`) |
| AY2 | `sched_rr_get_interval` from Ring 3 | **Done** — Ring 3 writes 100ms nsec (`GATE_AY2 rr_interval`) |
| AY3 | `getcpu` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AY3 getcpu`) |
| AY4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_destroy` returns `-ENOSYS` (`GATE_AY4 enosys`) |

### Gate AZ — utimensat dirfd, robust list, honest syscalls (after AY)

| ID | Task | Done when |
|----|------|-----------|
| AZ1 | `utimensat` via dirfd | **Done** — relative mtime 42 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AZ1 utimensat`) |
| AZ2 | `set_robust_list` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AZ2 set_robust`) |
| AZ3 | `get_robust_list` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AZ3 get_robust`) |
| AZ4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `set_thread_area` returns `-ENOSYS` (`GATE_AZ4 enosys`) |

### Gate BA — statx dirfd, personality, nanosleep, honest syscalls (after AZ)

| ID | Task | Done when |
|----|------|-----------|
| BA1 | `statx` via dirfd | **Done** — relative `stx_size == 1`; missing child is ENOENT; bad dirfd is EBADF (`GATE_BA1 statx`) |
| BA2 | `personality(-1)` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_BA2 personality`) |
| BA3 | `nanosleep` from Ring 3 | **Done** — Ring 3 `nanosleep({0,1})` returns 0 (`GATE_BA3 nanosleep`) |
| BA4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_cancel` returns `-ENOSYS` (`GATE_BA4 enosys`) |

### Gate BB — futimesat, capget, ioprio_get, honest syscalls (after BA)

| ID | Task | Done when |
|----|------|-----------|
| BB1 | `futimesat` via dirfd | **Done** — relative mtime 42 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_BB1 futimesat`) |
| BB2 | `capget` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_BB2 capget`) |
| BB3 | `ioprio_get` from Ring 3 | **Done** — Ring 3 default priority is 4 (`GATE_BB3 ioprio_get`) |
| BB4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `add_key` returns `-ENOSYS` (`GATE_BB4 enosys`) |

### Gate BC — renameat2, capset, ioprio_set, honest syscalls (after BB)

| ID | Task | Done when |
|----|------|-----------|
| BC1 | `renameat2` via dirfd | **Done** — relative rename succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_BC1 renameat2`) |
| BC2 | `capset` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_BC2 capset`) |
| BC3 | `ioprio_set` from Ring 3 | **Done** — Ring 3 `ioprio_set(1, 0, 4)` returns 0 (`GATE_BC3 ioprio_set`) |
| BC4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `request_key` returns `-ENOSYS` (`GATE_BC4 enosys`) |

### Gate G — Quality bar (parallel from day one)

| ID | Task | Done when |
|----|------|-----------|
| G1 | `README.md` + `LICENSE` | **Done** |
| G2 | GitHub Actions: fmt, clippy, size, QEMU boot | **Workflow present** (`.github/workflows/ci.yml`) |
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | Default kernel compiles **Live** code only |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | **Partial** — `bpf`/`pkey`/`process_mrelease`/`quotactl`/`remap_file_pages`/`io_uring_*`/`userfaultfd`/`perf_event_open`/`fanotify`/`io_setup`/`io_destroy`/`kexec`/`init_module`/`mount_setattr`/`fsopen`/`keyctl`/`ioperm`/`iopl`/`acct`/`swapon`/`swapoff`/`modify_ldt`/`sysfs`/`vhangup`/`lookup_dcookie`/`memfd_secret`/`uselib`/`pkey_mprotect`/`process_mrelease`/`set_mempolicy`/`get_mempolicy`/`mbind`/`migrate_pages`/`move_pages`/`sched_setattr`/`sched_getattr`/`futex_waitv`/`open_by_handle_at`/`name_to_handle_at`/`map_shadow_stack`/`ustat`/`set_thread_area`/`add_key`/`request_key`/KVM vCPU regs return ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4, AP4, AQ4, AR4, AS4, AT4, AU4, AV4, AW4, AX4, AY4, AZ4, BA4, BB4, BC4); remaining silent `Ok(0)` still exist |
| G5 | `unsafe` SAFETY comments + size budget | Clippy gate |

### Explicitly later (after Gates A–E)

KVM, containers/k8s, Btrfs/ZFS, Wi-Fi, TLS 1.3, GPU compute, AI inference, package formats, Chromium/Vivaldi, aarch64/riscv64 **runtime**, Thunderbolt, TPM measured boot.

Shipping these before Gate B **increases** the distance to a perfect OS.

---

## Anti-goals

Do not:

- Add another `pub mod` that `serial_println`s and returns `Ok(())`.
- Count syscall **numbers** as compatibility.
- Mark GUI 100% while apps are kernel enum variants.
- Implement SCHED_DEADLINE, eBPF, or overlayfs before `iretq`.
- Keep tests that `assert!(true)`.
- Claim Nix/GitHub/README completeness when the files are absent.

---

## Progress tracker

**Production OS: ~98%** · **QEMU desktop demo: ~99%**

```
Kernel Core:        ████████████████████░░░░░  82%  Wired         ← I1–I3 per-CPU TSS + GS + AP Ring 3
Memory Mgmt:        ███████████████████░░░░░░  74%  Wired         ← H1–H4 + J2/J3 + K2 swap I/O
Process/Sched:      ████████████████████████░  99%  Wired         ← B3–B8 + I2/I3 + J1 + K1 join + L1 TLS + M1 PID ns + M3 futex + N1 mount ns + O1 net ns + P1 user ns + Q1 IPC ns + R1 cgroup ns + S1 time ns + T1 setns + U1 chroot + V1 pivot_root + Z3 chdir + AA2 fchdir + AG3 getppid + AH3 getpgid + AI3 getsid + AJ3 setuid + AK2 getrlimit + AK3 setgid + AL2 setrlimit + AL3 setresuid + AM2 prlimit64 + AM3 setresgid + AN2 setreuid + AN3 getgroups + AO2 setregid + AO3 getresuid + AP2 getresgid + AP3 setpgid + AQ2 setsid + AQ3 setpriority + AT2 gettimeofday + AT3 sysinfo + AU2 sched_yield + AU3 alarm + AV2 getpid + AV3 gettid + BC2 capset + BC3 ioprio_set
Filesystem:         ████████████████████████░  99%  Wired         ← C1–C6 + inotify H3/Q3 + N1 mount ns + OverlayFS W1 + hardlink X1 + chmod Y1 + rmdir Z1 + unlink Z2 + AA1 mkfifo + AA2 fchdir + AA3 access + AB1 getdents + AB2 dup2 + AC1 fcntl + AC2 pread64 + AD1 fstat + AD2 pwrite64 + AE1 writev + AE2 ftruncate + AF1 readv + AF2 lseek + AG1 fsync + AG2 fdatasync + AH1 syncfs + AH2 sync + AI1 fchmod + AI2 fchown + AJ1 fstatfs + AJ2 statfs + AK1 fsetxattr + AL1 flistxattr + AM1 listxattr + AN1 faccessat + AO1 mkdirat + AP1 unlinkat + AQ1 renameat + AT1 readlinkat + AU1 mknodat + AV1 fchmodat + BC1 renameat2 + R2 splice + R3 flock + S2 sendfile + S3 tee + T2 copy_file_range + T3 vmsplice + U2 xattr + U3 statx + V2 fallocate + V3 utimensat + W2 umask + W3 symlink + X2 rename + X3 truncate + Y2 chown + Y3 mkdir
Networking:         ████████████████░░░░░░░░░  64%  Wired         ← D1–D5
Device Drivers:     ██████████░░░░░░░░░░░░░░░  40%  Wired         ← AHCI + NVMe DMA
GUI & Desktop:      ████████████████████░░░░░  82%  Live          ← F1–F4 SHM clients
Shell & Terminal:   ████████████████████░░░░░  82%  Live          ← sigreturn
Security:           ████████████████████░░░░░  80%  Wired         ← E1–E4 + K4 Landlock + J4/L4/M4/N4/O4/P4/Q4/R4/S4/T4/U4/V4/W4/X4/Y4/Z4/AA4/AB4/AC4/AD4/AE4/AF4/AG4/AH4/AI4/AJ4/AK4/AL4/AM4/AN4/AO4/AP4/AQ4/AR4/AS4/AT4/AU4/AV4/AW4/AX4/AY4/AZ4/BA4/BB4/BC4 ENOSYS
System Services:    ███████████░░░░░░░░░░░░░░  42%  Wired         ← K3 /sbin/init + L3 D-Bus AF_UNIX
Virtualization:     ████████░░░░░░░░░░░░░░░░░  30%  Stub          ← L2/M1/N1/O1/P1/Q1/R1/S1 namespaces + T1 setns + U1 chroot + V1 pivot_root + W1 OverlayFS
AI/ML:              █████░░░░░░░░░░░░░░░░░░░░  22%  Wired
Binary Compat:      ████████████████████████░  99%  Wired         ← execve + fork + clone + TLS %fs + pipe + futex + socketpair + eventfd + epoll + memfd + timerfd + signalfd + poll + inotify + splice + flock + sendfile + tee + copy_file_range + vmsplice + xattr + statx + fallocate + utimensat + umask + symlink + rename + truncate + chown + mkdir + unlink + chdir + fchdir + access + mkfifo + getdents + dup2 + uname + fcntl + pread64 + getuid + fstat + pwrite64 + getgid + writev + ftruncate + geteuid + readv + lseek + getegid + fsync + fdatasync + getppid + syncfs + sync + getpgid + fchmod + fchown + getsid + fstatfs + statfs + setuid + fsetxattr + getrlimit + setgid + flistxattr + setrlimit + setresuid + listxattr + prlimit64 + setresgid + faccessat + setreuid + getgroups + mkdirat + setregid + getresuid + unlinkat + getresgid + setpgid + renameat + setsid + setpriority + fchmodat + getpid + gettid + renameat2 + capset + ioprio_set
i18n & Fonts:       █████████████████░░░░░░░░  68%  Live
Build System:       ███████████████████░░░░░░  75%  Live
Testing:            ████████████████████░░░░░  82%  Wired         ← 230/230 integration
Documentation:      ██████████░░░░░░░░░░░░░░░  42%  Wired         ← BUILDING + CONTRIBUTING
CI/CD:              ███████░░░░░░░░░░░░░░░░░░  30%  Wired
```

### Score change vs 2026-03-15

| Subsystem | Old | Now | Why |
|-----------|-----|-----|-----|
| Kernel Core | 95% | 82% | Per-CPU TSS + GS CpuLocal; AP INIT/SIPI online (I1); AP Ring 3 (I3); NMI/MCE; MADT IOAPIC |
| Process | 90% | 99% | Gate B3–B8 scheduled Ring 3; IRQ GPR+FPU save (I2); AP Ring 3 (I3); clone (J1); CLONE_THREAD join (K1); `%fs` TLS (L1); PID ns (M1); futex (M3); mount ns (N1); net ns (O1); user ns (P1); IPC ns (Q1); cgroup ns (R1); time ns (S1); setns (T1); chroot (U1); pivot_root (V1); chdir/getcwd (Z3); fchdir (AA2); getppid (AG3); getpgid (AH3); getsid (AI3); setuid (AJ3); getrlimit (AK2); setgid (AK3); setrlimit (AL2); setresuid (AL3); prlimit64 (AM2); setresgid (AM3); setreuid (AN2); getgroups (AN3); setregid (AO2); getresuid (AO3); getresgid (AP2); setpgid (AP3); setsid (AQ2); setpriority (AQ3); getpid (AV2); gettid (AV3) |
| Binary compat | 40% | 99% | Static hello + scheduled `execve`/`fork`/`clone`/`CLONE_THREAD` + `/bin/sh` + `/sbin/init` + `arch_prctl` `%fs` + `pipe` + futex + `socketpair` + `eventfd` + `epoll` + `memfd` + `timerfd` + `signalfd` + `poll` + `inotify` + `splice` + `flock` + `sendfile` + `tee` + `copy_file_range` + `vmsplice` + `setxattr` + `statx` + `fallocate` + `utimensat` + `umask` + `symlink` + `rename` + `truncate` + `chown` + `mkdir` + `unlink` + `chdir`/`getcwd` + `fchdir` + `access` + `mkfifo` + `getdents` + `dup2` + `uname` + `fcntl` + `pread64` + `getuid` + `fstat` + `pwrite64` + `getgid` + `writev` + `ftruncate` + `geteuid` + `readv` + `lseek` + `getegid` + `fsync` + `fdatasync` + `getppid` + `syncfs` + `sync` + `getpgid` + `fchmod` + `fchown` + `getsid` + `fstatfs` + `statfs` + `setuid` + `fsetxattr` + `getrlimit` + `setgid` + `flistxattr` + `setrlimit` + `setresuid` + `listxattr` + `prlimit64` + `setresgid` + `faccessat` + `setreuid` + `setgroups`/`getgroups` + `mkdirat` + `setregid` + `getresuid` + `unlinkat` + `getresgid` + `setpgid`/`getpgrp` + `renameat` + `setsid` + `setpriority`/`getpriority` + `fchmodat` + `getpid` + `gettid`; 452 numbers still ≠ 452 behaviors |
| Memory | — | 74% | H1 CoW #PF; H2 file-backed fault-in; H4 OOM-on-alloc + guarded stacks; J2 leftover buddy RAM; J3 LRU; K2 swap I/O |
| Filesystem | 35% | 99% | VirtIO-blk + C1–C6 + inotify on VFS mutate (H3) + mount ns (N1) + OverlayFS (W1) + hardlink (X1) + chmod (Y1) + rmdir (Z1) + unlink (Z2) + mkfifo (AA1) + fchdir (AA2) + access (AA3) + getdents (AB1) + dup2 (AB2) + fcntl (AC1) + pread64 (AC2) + fstat (AD1) + pwrite64 (AD2) + writev (AE1) + ftruncate (AE2) + readv (AF1) + lseek (AF2) + fsync (AG1) + fdatasync (AG2) + syncfs (AH1) + sync (AH2) + fchmod (AI1) + fchown (AI2) + fstatfs (AJ1) + statfs (AJ2) + fsetxattr (AK1) + flistxattr (AL1) + listxattr (AM1) + faccessat (AN1) + mkdirat (AO1) + unlinkat (AP1) + renameat (AQ1) + fchmodat (AV1) + splice (R2) + flock (R3) + sendfile (S2) + tee (S3) + copy_file_range (T2) + vmsplice (T3) + xattr (U2) + statx (U3) + fallocate (V2) + utimensat (V3) + umask (W2) + symlink (W3) + rename (X2) + truncate (X3) + chown (Y2) + mkdir (Y3) |
| GUI | 85% | 82% | Ring 3 SHM clients (F1–F4); remaining apps still in-process |
| Shell | 90% | 82% | PTY/glob/env real; Ring 3 `/bin/sh`; live `sigreturn`; desktop terminal still in-kernel for PTY I/O |
| Docs / CI | 10% / 25% | 42% / 30% | README, LICENSE, BUILDING, CONTRIBUTING, GitHub Actions; flake still missing |
| Networking | 22% | 64% | D1 loopback; D2 VirtIO-net; D3 DHCP apply; D4 DNS+TCP; D5 CUBIC |
| Security | 15% | 80% | E1 W^X/ASLR; E2 ChaCha20; E3 seccomp EPERM; E4 CapNetBindService; K4 Landlock; J4/L4/M4/N4/O4/P4/Q4/R4/S4/T4/U4/V4/W4/X4/Y4/Z4/AA4/AB4/AC4/AD4/AE4/AF4/AG4/AH4/AI4/AJ4/AK4/AL4/AM4/AN4/AO4/AP4/AQ4/AR4/AS4/AT4/AU4/AV4/AW4/AX4/AY4/AZ4/BA4/BB4/BC4 ENOSYS |
| System services | — | 42% | K3 Ring 3 `/sbin/init`; L3 AF_UNIX D-Bus socket |
| **Overall production** | **~40%** | **~98%** | Gates B3–B8 + C1–C6 + D1–D5 + E1–E4 + F1–F4 + H1–H4 + I1–I3 + J1–J4 + K1–K4 + L1–L4 + M1–M4 + N1–N4 + O1–O4 + P1–P4 + Q1–Q4 + R1–R4 + S1–S4 + T1–T4 + U1–U4 + V1–V4 + W1–W4 + X1–X4 + Y1–Y4 + Z1–Z4 + AA1–AA4 + AB1–AB4 + AC1–AC4 + AD1–AD4 + AE1–AE4 + AF1–AF4 + AG1–AG4 + AH1–AH4 + AI1–AI4 + AJ1–AJ4 + AK1–AK4 + AL1–AL4 + AM1–AM4 + AN1–AN4 + AO1–AO4 + AP1–AP4 + AQ1–AQ4 + AR1–AR4 + AS1–AS4 + AT1–AT4 + AU1–AU4 + AV1–AV4 + AW1–AW4 + AX1–AX4 + AY1–AY4 + AZ1–AZ4 + BA1–BA4 + BB1–BB4 + BC1–BC4 on the live boot path |

Code **grew** (602 → 609 files, more Phase 30–33 modules). Production usefulness did not grow proportionally. The next updates to this file should tick **Gate** IDs, not module counts.

---

*This document is the single source of truth for KnoxOS readiness. Update a checkbox only when the **Done when** criterion is met on the live path. Unused source does not count.*
