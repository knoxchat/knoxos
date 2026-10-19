← [Status index](../README.md) · [Do this first](../next.md)

# 11. Virtualization & Containers

**Grade: Wired (58%)** · `kvm.rs`, `vmx.rs`, `container.rs`, `namespaces.rs`

**Do not expand this until Ring 3 and namespaces work for ordinary processes.**

- [x] VMX constants; `vmxon`/`vmlaunch`/`vmresume` `asm` in `vmx.rs`
- [x] `kvm::start_vm` software EPT fetch of HLT `0xF4` (`GATE_KVM vmlaunch`). Nested VMXON is not used on stock QEMU.
- [x] EPT 4-level walk + virtio-mmio magic (`GATE_KVM ept`)
- [x] `container.rs` — `runc start` unshare + `pivot_root` (`GATE_OCI start`)
- [x] Namespace maps inherited on fork/clone; `CLONE_NEWUTS` isolates hostname (L2); `CLONE_NEWPID` isolates `getpid` (M1); `CLONE_NEWNS` isolates bind mounts (N1); `CLONE_NEWNET` isolates interfaces (O1); `CLONE_NEWUSER` maps the child to uid 0 (P1); `CLONE_NEWIPC` isolates SysV shm keys (Q1); `CLONE_NEWCGROUP` isolates cgroup paths (R1); `CLONE_NEWTIME` isolates CLOCK_MONOTONIC offsets (S1); `setns` joins an existing UTS ns (T1); `chroot` jails path lookup (U1); `pivot_root` keeps the old root at `put_old` (V1); OverlayFS merge+whiteout is VFS-live and mount-ns isolated (W1)
- [x] OverlayFS on VFS path lookup / list / write (W1); FUSE LOOKUP+READ (`GATE_FUSE lookup`); FUSE WRITE (`GATE_FUSE write`); FUSE UNLINK (`GATE_FUSE unlink`); FUSE MKDIR (`GATE_FUSE mkdir`); FUSE RMDIR (`GATE_FUSE rmdir`); FUSE RENAME (`GATE_FUSE rename`); FUSE SYMLINK (`GATE_FUSE symlink`); FUSE LINK (`GATE_FUSE link`); FUSE CREATE (`GATE_FUSE create`); FUSE GETATTR (`GATE_FUSE getattr`); FUSE SETATTR (`GATE_FUSE setattr`); FUSE READDIR (`GATE_FUSE readdir`)
- [x] cgroup v2 **memory** enforcement (`GATE_CGROUP2 memory`). CPU quota and IO byte cap deny (`GATE_CGROUP cpu`, `GATE_CGROUP io`). Frozen cgroup denies CPU charge (`GATE_CGROUP freeze`).

Note: `vmm.rs` is **process page tables**, not a hypervisor.

---
