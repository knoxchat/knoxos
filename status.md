# KnoxOS Production Readiness Status

> **Last Updated**: 2026-10-24
> **Version**: 0.2.6
> **Honesty rule**: a module that compiles is not a feature. Only **Live** work counts toward production.

The full status tree lives in [`status/`](status/README.md). This file is the stable root path CI and old links still check.

**Start here: [status/next.md](status/next.md)** — what to do first.

| File | Purpose |
|------|---------|
| [status/next.md](status/next.md) | Ordered remaining work — leftover FUSE/D-Bus/XHCI/NFS and Chromium slices 1–10 landed (Vivaldi `.deb`, Mojo, ELF TLS / DT_NEEDED, polish, Ring 3 `io_uring`); k8s stays later |
| [status/README.md](status/README.md) | Honesty tags, what a perfect OS is, file map |
| [status/remaining.md](status/remaining.md) | Every open checkbox (inventory, not sequence) |
| [status/blockers.md](status/blockers.md) | Architectural gaps |
| [status/live.md](status/live.md) | What the QEMU boot path actually does |
| [status/subsystems.md](status/subsystems.md) | 19-area grades (incl. [19-browser.md](status/subsystems/19-browser.md)) |
| [status/gates/](status/gates/README.md) | Gates A–BL + BM1–BM4 + F5–F8 + C7–C10 + G3–G5 + I4 + D6 + USB HID URB + XHCI MMIO + no-PS/2 cursor + Secure Boot + cross-arch boot + udev/quota/cron/vsync/TERM + cgroup/job/IMA/lockdown/UEFI/wipe/NM + TLS13/HDR/VRR/e1000/eBPF/Wi-Fi/i915/Btrfs/VLAN + RequestName/FUSE WRITE/PAM/cgroup freeze + XHCI enum + ACPI S5/FAT32/ext2/ISO9660/Configure Endpoint/ReleaseName/FUSE UNLINK + FUSE MKDIR/ListNames/GET_STATUS/FAT32 mount/NFS WRITE + FUSE RMDIR/RENAME/GetConnectionUnixProcessID/SET_INTERFACE/ISO9660 mount/NTFS mount/NFS REMOVE + FUSE SYMLINK/LINK/GetNameOwner/GET_CONFIGURATION/exFAT mount/Btrfs mount/NFS RENAME + FUSE CREATE/GETATTR/NameHasOwner/GET_INTERFACE/XFS mount/ZFS mount/NFS CREATE + FUSE SETATTR/READDIR/GetConnectionUnixUser/AddMatch/CLEAR_FEATURE/bulk/NFS LINK/READLINK + FUSE MKNOD/STATFS/RemoveMatch/GetId/SET_FEATURE/bulk IN/NFS SETATTR/READDIR + FUSE SETXATTR/GETXATTR/StartServiceByName/GetConnectionCredentials/SET_DESCRIPTOR/isoch IN/NFS ACCESS/COMMIT + FUSE LISTXATTR/REMOVEXATTR/ListActivatableNames/ListQueuedOwners/GET_DESCRIPTOR(String)/interrupt IN/NFS GETFH/PUTFH + FUSE ACCESS/FSYNC/GetConnectionSELinuxSecurityContext/GetConnectionAppArmorSecurityContext/No-Op Command/Disable Slot/NFS LOOKUPP/RESTOREFH + FUSE OPEN/RELEASE/GetAdtAuditSessionData/UpdateActivationEnvironment/Evaluate Context/Stop Endpoint/NFS PUTPUBFH/VERIFY + FUSE FLUSH/FSYNCDIR/ReloadConfig/BecomeMonitor/Reset Endpoint/Set TR Dequeue/NFS NVERIFY/SETCLIENTID_CONFIRM + FUSE OPENDIR/RELEASEDIR/Peer Ping/GetMachineId/Reset Device/Transfer No-Op/NFS CLOSE/RENEW + FUSE GETLK/SETLK/Introspect/Properties.Get/Device Qualifier/BOS/NFS LOCK/LOCKU + FUSE SETLKW/BMAP/Properties.GetAll/Set/Other Speed Configuration/SYNCH_FRAME/NFS LOCKT/RELEASE_LOCKOWNER + FUSE DESTROY/IOCTL/GetStats/GetAllMatchRules/SET_SEL/SET_ISOCH_DELAY/NFS OPEN_CONFIRM/OPEN_DOWNGRADE + FUSE POLL/FALLOCATE/GetConnectionStats/GetManagedObjects/HID/HID Report/NFS SECINFO/OPENATTR + FUSE READDIRPLUS/RENAME2/NameOwnerChanged/NameLost/GET_REPORT/SET_REPORT/NFS DELEGPURGE/DELEGRETURN + FUSE LSEEK/COPY_FILE_RANGE/NameAcquired/GetConnectionLinuxSecurityLabel/GET_IDLE/SET_IDLE/NFS EXCHANGE_ID/CREATE_SESSION + FUSE SYNCFS/STATX/GetConfigFile/ActivatableServicesChanged/GET_PROTOCOL/SET_PROTOCOL/NFS DESTROY_SESSION/DESTROY_CLIENTID + FUSE INIT/FORGET/PropertiesChanged/InterfacesAdded/GET_STATUS(INTERFACE)/GET_STATUS(ENDPOINT)/NFS SEQUENCE/RECLAIM_COMPLETE |
| [status/progress.md](status/progress.md) | Score bars and anti-goals |

Integration: `./tests/run_integration.sh` **661/661** on QEMU (2026-10-24).
