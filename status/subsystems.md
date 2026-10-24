# Subsystem status

← [Status index](README.md) · [Do this first](next.md)

## Code distribution

| Area | Files | Lines | Role |
|------|-------|-------|------|
| Crate-root leftovers | 28 | ~15,600 | `lib`/`main`, boot splash, vdso, sysctl, clipboard |
| Hardware (`drivers/`) | 132 | ~49,100 | Live PCI/VirtIO/USB + stub devices |
| Filesystems (`fs/`) | 68 | ~29,400 | VFS, ext4, persist, disk/net FS |
| Memory (`mm/`) | 26 | ~10,300 | VMM, mmap, heap |
| Arch (`arch/`) | 34 | ~13,100 | GDT, IDT, SMP, serial |
| Sched (`sched/`) | 42 | ~18,500 | Process, ELF, cgroups |
| IPC (`ipc/`) | 23 | ~8,600 | Pipes, SHM, futex, PTY |
| Security (`security/`) | 16 | ~9,300 | seccomp, LSM, landlock |
| Net (`net/`) | 46 | ~14,300 | TCP/IP + folded leftovers |
| Crypto (`crypto/`) | 23 | ~4,800 | Primitives + TLS |
| Compat (`compat/`) | 234 | ~94,700 | Kernel-hosted userland fakes |
| GUI & desktop (`gui/`) | 221 | ~87,700 | Live software compositor + in-process apps |
| Syscall interface | 31 | ~10,200 | Dispatch table; many no-ops |
| Terminal emulator | 10 | ~5,100 | Live in-kernel terminal |
| Boot crate | — | ~90 | Image builder |
| **Total `kernel/src`** | **1035** | **~388,000** | |

~400 `pub mod` entries does **not** mean 400 working subsystems. Prefer deleting or gating Unused modules over adding Phase 34+.

---

## Summary

Percentages are **production usefulness**, not lines of code.

| # | Subsystem | Grade | Live % | Priority | One-line truth |
|---|-----------|-------|--------|----------|----------------|
| 1 | [Kernel Core](subsystems/01-kernel-core.md) | Wired | 90% | High | Interrupts and timers; per-CPU TSS + GS; AP Ring 3 (I3); nested IRQ TPR; live patch W^X; UEFI RTC time. |
| 2 | [Memory](subsystems/02-memory.md) | Wired | 86% | **Critical** | Demand paging, CoW, file mmap, buddy, LRU, swap I/O, OOM, stack guards, memory cgroup, NUMA frames, KSM merge. |
| 3 | [Process](subsystems/03-process.md) | Wired | 99% | **Critical** | Scheduled Ring 3 (B2–B8); clone/clone3; namespaces; eBPF/sched_ext pick (`GATE_EBPF`). |
| 4 | [Filesystem](subsystems/04-filesystem.md) | Wired | 100% | **Critical** | VirtIO-blk persist C1–C7 (`/bin` included) + C10 virtual-FS mount points; AHCI/NVMe DMA; NTFS/exFAT/XFS/Btrfs/ZFS/FAT32/ext2/ISO9660 ramdisk; FAT32/ISO9660/NTFS/exFAT/Btrfs/XFS/ZFS mount; CIFS SMB2; FUSE LOOKUP+WRITE+UNLINK+MKDIR+RMDIR+RENAME+SYMLINK+LINK+CREATE+GETATTR+SETATTR+READDIR+MKNOD+STATFS+SETXATTR+GETXATTR+LISTXATTR+REMOVEXATTR+ACCESS+FSYNC+OPEN+RELEASE+FLUSH+FSYNCDIR+OPENDIR+RELEASEDIR+GETLK+SETLK+SETLKW+BMAP+DESTROY+IOCTL+POLL+FALLOCATE+READDIRPLUS+RENAME2+LSEEK+COPY_FILE_RANGE+SYNCFS+STATX; NFS4 COMPOUND+WRITE+REMOVE+RENAME+CREATE+LINK+READLINK+SETATTR+READDIR+ACCESS+COMMIT+GETFH+PUTFH+LOOKUPP+RESTOREFH+PUTPUBFH+VERIFY+NVERIFY+SETCLIENTID_CONFIRM+CLOSE+RENEW+LOCK+LOCKU+LOCKT+RELEASE_LOCKOWNER+OPEN_CONFIRM+OPEN_DOWNGRADE+SECINFO+OPENATTR+DELEGPURGE+DELEGRETURN+EXCHANGE_ID+CREATE_SESSION+DESTROY_SESSION+DESTROY_CLIENTID. |
| 5 | [Networking](subsystems/05-networking.md) | Wired | 88% | **Critical** | Loopback + VirtIO-net DHCP/DNS/TCP CUBIC (D1–D5) + IPv6 echo TX (D6) + NAT + TLS HKDF/handshake + VLAN + e1000 `Socket::send` + rtl8139 loopback + Wi-Fi PMK + WPA3-SAE + bridge FDB. |
| 6 | [Drivers](subsystems/06-drivers.md) | Wired | 93% | **Critical** | PCI, PS/2, UART, VirtIO-blk, VirtIO-GPU scanout+cursor (F8), AHCI/NVMe DMA, USB HID URB + XHCI MMIO + Enable Slot cc=1 + Address Device cc=1 + GET_DESCRIPTOR + SET_CONFIGURATION + Configure Endpoint + full enum (`GATE_USB xhci enum`) + GET_STATUS (`GATE_USB xhci status`) + SET_INTERFACE (`GATE_USB xhci iface`) + GET_CONFIGURATION (`GATE_USB xhci getcfg`) + GET_INTERFACE (`GATE_USB xhci getiface`) + CLEAR_FEATURE (`GATE_USB xhci clear`) + bulk (`GATE_USB xhci bulk`) + SET_FEATURE (`GATE_USB xhci set`) + bulk IN (`GATE_USB xhci bulkin`) + SET_DESCRIPTOR (`GATE_USB xhci stdesc`) + isoch IN (`GATE_USB xhci isoch`) + GET_DESCRIPTOR(String) (`GATE_USB xhci string`) + interrupt IN (`GATE_USB xhci intr`) + No-Op Command (`GATE_USB xhci noop`) + Disable Slot (`GATE_USB xhci disable`) + Evaluate Context (`GATE_USB xhci eval`) + Stop Endpoint (`GATE_USB xhci stop`) + Reset Endpoint (`GATE_USB xhci resetep`) + Set TR Dequeue (`GATE_USB xhci settr`) + Reset Device (`GATE_USB xhci resetdev`) + Transfer No-Op (`GATE_USB xhci trnoop`) + Device Qualifier (`GATE_USB xhci qual`) + BOS (`GATE_USB xhci bos`) + Other Speed Configuration (`GATE_USB xhci oscfg`) + SYNCH_FRAME (`GATE_USB xhci synch`) + SET_SEL (`GATE_USB xhci setsel`) + SET_ISOCH_DELAY (`GATE_USB xhci isochdelay`) + MSC/hub/gamepad/audio/trackpad/wacom/UVC/ethernet/touch + GPIO + DRM GEM + TPM TIS + I2C + i915 modeset + AMDGPU fw + e1000/rtl8139 send + AX211 + Broadcom fw + Thunderbolt hop. |
| 7 | [GUI](subsystems/07-gui.md) | Live | 99% | Medium | Software compositor + Ring 3 SHM (F1–F8) + VSync + VirtIO-GPU cursor + TTF hinting. Start-menu Terminal is Ring 3 PTY. |
| 8 | [Shell](subsystems/08-shell.md) | Live | 86% | Medium | Real parser/PTY; Ring 3 `/bin/sh`; SIGTSTP/SIGCONT job control. |
| 9 | [Security](subsystems/09-security.md) | Wired | 98% | **Critical** | W^X, ChaCha20, AES-GCM, Poly1305, X25519, Ed25519, RSA, ECDSA-P256, seccomp, caps (E1–E4); Landlock; SELinux TE; AppArmor path MAC; dm-verity; IMA; lockdown; wipe-on-free. |
| 10 | [Services](subsystems/10-services.md) | Wired | 85% | High | Ring 3 `/sbin/init` (K3); AF_UNIX D-Bus + Hello unique name + RequestName + ReleaseName + ListNames + GetConnectionUnixProcessID + GetNameOwner + NameHasOwner + GetConnectionUnixUser + AddMatch + RemoveMatch + GetId + StartServiceByName + GetConnectionCredentials + ListActivatableNames + ListQueuedOwners + GetConnectionSELinuxSecurityContext + GetConnectionAppArmorSecurityContext + GetAdtAuditSessionData + UpdateActivationEnvironment + ReloadConfig + BecomeMonitor + Peer Ping + Peer GetMachineId + Introspect + Properties.Get + GetAll + Set + GetStats + GetAllMatchRules + GetConnectionStats + GetManagedObjects + NameOwnerChanged + NameLost + NameAcquired + GetConnectionLinuxSecurityLabel + GetConfigFile + ActivatableServicesChanged; PAM unix SHA-512; crash restart; unit MemoryMax cgroup; cron timer; udev `/dev`; NM DHCP; NTP apply; Bluetooth HCI Reset; suspend; logind. |
| 11 | [Virt](subsystems/11-virt.md) | Wired | 58% | Low | Namespaces live; cgroup v2 memory/CPU/IO/freeze; software EPT+HLT (`GATE_KVM`); OCI unshare+pivot_root (`GATE_OCI`). |
| 12 | [AI/ML](subsystems/12-ai.md) | Wired | 40% | Low | GGUF parse + SDPA/KV (`GATE_AI attn`) + CPU matmul (`GATE_GPU matmul`) + assistant chat (`GATE_AI chat`). |
| 13 | [Binary compat](subsystems/13-binary-compat.md) | Wired | 99% | High | Static ELF + 200+ Ring 3 syscalls + `ld.so` (BM1) + VMM `dlopen` (BM2) + static musl CRT (BM3) + dynamically linked musl (BM4). Next: Ring 3 desktop apps. |
| 14 | [i18n](subsystems/14-i18n.md) | Live | 95% | Low | TTF, CJK, RTL, fallback chain, GSUB Latin liga, GPOS kern, GPOS mark, color emoji, TTF hinting, `.mo` gettext, pinyin IME. |
| 15 | [Build](subsystems/15-build.md) | Live | 80% | Medium | Make/QEMU work; feature flags G3; `flake.nix` present. |
| 16 | [Testing](subsystems/16-testing.md) | Wired | 98% | **Critical** | 539 QEMU markers + host parser tests; `assert!(true)` gone. |
| 17 | [Docs](subsystems/17-docs.md) | Wired | 70% | **Critical** | README + LICENSE + BUILDING + CONTRIBUTING + `status/` + `docs/` + CHANGELOG + rustdoc artifact. |
| 18 | [CI/CD](subsystems/18-ci.md) | Wired | 55% | High | GitHub Actions fmt/clippy/size/QEMU boot + aarch64/riscv64 compile + rustdoc + audit + nightly images. |

**QEMU desktop demo: ~99%.** **Production OS: ~99%.** Gates A–F, H–I, J–BL, BM1–BM4, F5–F8, C7–C10, G3–G5, I4, D6, USB HID URB, XHCI MMIO, udev/quota/cron/vsync/TERM plus cgroup/job/IMA/lockdown/UEFI/wipe/NM/XFS/X25519/Ed25519/locale/IME/MSC/NAT/DRM/GSUB/RSA/ECDSA/emoji/hub/gamepad/TPM/TTF/I2C/TLS13/HDR/VRR/e1000/eBPF/Wi-Fi/i915/Btrfs/VLAN plus USB audio/trackpad/wacom/UVC/ethernet/touch/GPIO plus rtl8139/CIFS/GPOS/AMDGPU/AX211/FUSE plus KVM EPT/HLT, OCI start, ZFS, WPA3/bridge, TLS handshake, GPU matmul, AI attention, NFS, Broadcom, Thunderbolt plus AppArmor, XHCI GET_DESCRIPTOR, unit MemoryMax, GPOS mark plus io_uring SQE, fanotify VFS, D-Bus Hello, XHCI SET_CONFIGURATION plus Address Device, RequestName, FUSE WRITE, PAM unix, cgroup freeze, XHCI enum, virtual-FS persist, ACPI S5, FAT32/ext2/ISO9660 ramdisk, Configure Endpoint, ReleaseName, FUSE UNLINK, FUSE MKDIR, ListNames, GET_STATUS, FAT32 mount, NFS WRITE, FUSE RMDIR/RENAME, GetConnectionUnixProcessID, SET_INTERFACE, ISO9660/NTFS mount, NFS REMOVE, FUSE SYMLINK/LINK, GetNameOwner, GET_CONFIGURATION, exFAT/Btrfs mount, NFS RENAME, FUSE CREATE/GETATTR, NameHasOwner, GET_INTERFACE, XFS/ZFS mount, NFS CREATE, FUSE SETATTR/READDIR, GetConnectionUnixUser, AddMatch, CLEAR_FEATURE, bulk, NFS LINK/READLINK, FUSE MKNOD/STATFS, RemoveMatch, GetId, SET_FEATURE, bulk IN, NFS SETATTR/READDIR, FUSE SETXATTR/GETXATTR, StartServiceByName, GetConnectionCredentials, SET_DESCRIPTOR, isoch IN, NFS ACCESS/COMMIT, FUSE LISTXATTR/REMOVEXATTR, ListActivatableNames, ListQueuedOwners, GET_DESCRIPTOR(String), interrupt IN, NFS GETFH/PUTFH/LOOKUPP/RESTOREFH/PUTPUBFH/VERIFY plus FUSE ACCESS/FSYNC/OPEN/RELEASE, D-Bus SELinux/AppArmor/ADT/env, XHCI No-Op/Disable Slot/Evaluate Context/Stop Endpoint/Reset Endpoint/Set TR Dequeue/Reset Device/Transfer No-Op plus FUSE FLUSH/FSYNCDIR/OPENDIR/RELEASEDIR, D-Bus ReloadConfig/BecomeMonitor/Peer Ping/GetMachineId, NFS NVERIFY/SETCLIENTID_CONFIRM/CLOSE/RENEW plus FUSE GETLK/SETLK, D-Bus Introspect/Properties.Get, XHCI Device Qualifier/BOS, NFS LOCK/LOCKU plus FUSE SETLKW/BMAP, D-Bus Properties.GetAll/Set, XHCI Other Speed Configuration/SYNCH_FRAME, NFS LOCKT/RELEASE_LOCKOWNER plus FUSE DESTROY/IOCTL, D-Bus GetStats/GetAllMatchRules, XHCI SET_SEL/SET_ISOCH_DELAY, NFS OPEN_CONFIRM/OPEN_DOWNGRADE plus FUSE POLL/FALLOCATE, D-Bus GetConnectionStats/GetManagedObjects, XHCI HID/HID Report, NFS SECINFO/OPENATTR plus FUSE READDIRPLUS/RENAME2, D-Bus NameOwnerChanged/NameLost, XHCI GET_REPORT/SET_REPORT, NFS DELEGPURGE/DELEGRETURN plus FUSE LSEEK/COPY_FILE_RANGE, D-Bus NameAcquired/GetConnectionLinuxSecurityLabel, XHCI GET_IDLE/SET_IDLE, NFS EXCHANGE_ID/CREATE_SESSION plus FUSE SYNCFS/STATX, D-Bus GetConfigFile/ActivatableServicesChanged, XHCI GET_PROTOCOL/SET_PROTOCOL, NFS DESTROY_SESSION/DESTROY_CLIENTID are on the live boot path. Full gate tables: [gates/](gates/README.md). Integration: `./tests/run_integration.sh` **587/587** on QEMU (2026-10-24). What to do next: [next.md](next.md). Desktop pointer is VirtIO-tablet + USB digitizer + PS/2 (`GATE_VIRTIO input tablet`).

---
