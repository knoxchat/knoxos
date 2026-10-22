# Gates

← [Status index](../README.md) · [Do this first](../next.md)

Stop adding Phase 34 modules. **Wire, delete, or feature-gate.** Sequence is dependency order.

Gates **A–F, H–I** made KnoxOS an OS. Gates **J–BL** proved syscalls and namespaces on the same QEMU boot path. **BM1** is a userspace `ld.so`. **BM2** is runtime `dlopen` via VMM `mmap`. **BM3** is static musl CRT. **BM4** is a dynamically linked musl binary. They are **done**. Remaining work is [next.md](../next.md), not another lettered ENOSYS gate.

| File | Gates | Status |
|------|-------|--------|
| [foundational.md](foundational.md) | A–F, H, I, BM1–BM4 | **Done** — boot, Ring 3, persist, packets, enforcement, isolated clients (F5 AF_UNIX), VirtIO-GPU scanout/cursor (F8), memory leftovers, SMP, `ld.so`, `dlopen`, static + dynamic musl CRT |
| [syscalls.md](syscalls.md) | J–BL | **Done** — clone, namespaces, VFS, sockets, honesty ENOSYS |
| [quality.md](quality.md) | G + explicitly later | **Done** — G1–G5; KVM/AI/Wi-Fi stay later |

Live-path evidence for A–I, BM1–BM4, F5–F8, C7–C10, G3–G5, I4, D6, USB HID URB, XHCI MMIO, no-PS/2 cursor, Secure Boot, cross-arch serial boot, udev/quota/cron/vsync/TERM/cgroup/job/IMA/lockdown/UEFI/wipe/NM/NTFS/heredoc/VT100/HDA/PA/IOMMU/THP/KASLR/AML/KO plus NUMA/KSM/nested-IRQ/livepatch/exFAT/CPU-IO/suspend/logind/AES-GCM/Poly1305/font-fallback/XFS/X25519/Ed25519/locale/IME/MSC/NAT/DRM/GSUB/RSA/ECDSA/emoji/hub/gamepad/TPM/TTF/I2C/TLS13/HDR/VRR/e1000/eBPF/Wi-Fi/i915/Btrfs/VLAN plus USB audio/trackpad/wacom/UVC/ethernet/touch/GPIO plus rtl8139/CIFS/GPOS/AMDGPU/AX211/FUSE plus Address Device, RequestName, FUSE WRITE, PAM unix, cgroup freeze, XHCI enum, virtual-FS persist, ACPI S5, FAT32/ext2/ISO9660 ramdisk, Configure Endpoint, ReleaseName, FUSE UNLINK, FUSE MKDIR, ListNames, GET_STATUS, FAT32 mount, NFS WRITE, FUSE RMDIR/RENAME, GetConnectionUnixProcessID, SET_INTERFACE, ISO9660/NTFS mount, NFS REMOVE, FUSE SYMLINK/LINK, GetNameOwner, GET_CONFIGURATION, exFAT/Btrfs mount, NFS RENAME, FUSE CREATE/GETATTR, NameHasOwner, GET_INTERFACE, XFS/ZFS mount, NFS CREATE, FUSE SETATTR/READDIR, GetConnectionUnixUser, AddMatch, CLEAR_FEATURE, bulk, NFS LINK/READLINK, FUSE MKNOD/STATFS, RemoveMatch, GetId, SET_FEATURE, bulk IN, NFS SETATTR/READDIR, FUSE SETXATTR/GETXATTR, StartServiceByName, GetConnectionCredentials, SET_DESCRIPTOR, isoch IN, NFS ACCESS/COMMIT, FUSE LISTXATTR/REMOVEXATTR, ListActivatableNames, ListQueuedOwners, GET_DESCRIPTOR(String), interrupt IN, NFS GETFH/PUTFH/LOOKUPP/RESTOREFH plus FUSE ACCESS/FSYNC/OPEN/RELEASE, D-Bus SELinux/AppArmor/ADT/env, XHCI No-Op/Disable Slot/Evaluate Context/Stop Endpoint, NFS LOOKUPP/RESTOREFH/PUTPUBFH/VERIFY/NVERIFY/SETCLIENTID_CONFIRM plus FUSE FLUSH/FSYNCDIR, D-Bus ReloadConfig/BecomeMonitor, XHCI Reset Endpoint/Set TR Dequeue is summarized in [live.md](../live.md). J–BL tables are the archive of those proofs. Integration: **521/521**.
