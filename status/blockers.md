# Architectural blockers

← [Status index](README.md) · [Do this first](next.md)

### Architectural blockers (must fix first)

| Blocker | Evidence | Why it blocks a perfect OS |
|---------|----------|----------------------------|
| **Scheduled Ring 3** | Hello is a CFS task with its own CR3; `execve`/`waitpid`/`fork`/`clone(CLONE_VM)`/`/bin/sh` run on the boot path. Calculator (F6) and Files (F7) are Ring 3 SHM. | Interactive terminal still in-kernel for PTY I/O. |
| **Ring 3 timer preemption** | Naked APIC-timer stub saves GPRs + FXSAVE then `enter_context`s (`GATE_B8` + `GATE_I2 irq gprs`). | Done for the spinning-user case. |
| **SMP** | INIT/SIPI trampoline; AP loads per-CPU TSS + GS, starts its LAPIC timer, runs pinned (I3) and unpinned (I4) Ring 3; per-CPU CFS `current`. | Further APs beyond the QEMU `-smp 2` pair unproven. |
| **Signal frames for handlers** | Default terminate plus a live SIGINT handler + `rt_sigreturn` (B7). | Catching SIGINT in a user handler is done. |
| **Sockets do not transmit off-box** | Loopback `send` delivers to a peer `recv_buf`. VirtIO-net TX/RX rings are live (D2). DHCP writes `eth0`. DNS + TCP SYN/retransmit/HTTP are live (D4). CUBIC cwnd limits send (D5). | Off-box TCP is live; CUBIC is wired. |
| **AHCI and NVMe DMA are live** | AHCI command-list + PRDT round-trips a sector (C4). NVMe admin/I/O queues + PRP bounce round-trip a sector (C5). | Bare-metal beyond QEMU still unproven. |
| **Default VFS is RAM with persist snapshot + ext4 root** | Inodes are `Vec<u8>` for virtual FS. Persist blob store round-trips through vda (C1–C7, C10). A second virtio-blk `mkfs.ext4` volume is mounted at `/` (`GATE_C9 ext4 root`). | Mount-point directories persist (C10); generated files under `/dev` `/proc` `/sys` `/run` `/tmp` stay virtual. |
| **Security not on the deny path** | Landlock is live on VFS open/write (K4). SELinux unused. | W^X, ChaCha20 `getrandom`, seccomp EPERM, CapNetBindService (E1–E4). `bpf`/`pkey`/`quotactl`/`remap_file_pages`/`io_uring`/`userfaultfd`/`perf_event_open`/`fanotify`/`io_setup`/`kexec`/`init_module`/`mount_setattr`/`fsopen`/`keyctl`/`ioperm`/`iopl`/`acct`/`swapon`/`swapoff`/`modify_ldt`/`sysfs`/`vhangup`/`lookup_dcookie`/`memfd_secret`/`uselib`/`pkey_mprotect`/`pkey_free`/`process_mrelease`/`set_mempolicy`/`get_mempolicy`/`mbind`/`migrate_pages`/`move_pages`/`sched_setattr`/`sched_getattr`/`futex_waitv`/`open_by_handle_at`/`name_to_handle_at` return ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4, AP4, AQ4, AR4, AS4, AT4, AU4, AV4). |
| **Breadth without wiring** | 609 modules. GPU compositor, Wayland, KVM `vmlaunch` unused. OverlayFS is live (W1). PAM unix + D-Bus RequestName/ReleaseName + FUSE WRITE/UNLINK + cgroup freeze + ACPI S5 + FAT32/ext2/ISO9660 ramdisk are live. | Compile time and maintenance grow; capability does not. |

---
