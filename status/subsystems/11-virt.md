← [Status index](../README.md) · [Do this first](../next.md)

# 11. Virtualization & Containers

**Grade: Stub (32%)** · `kvm.rs`, `vmx.rs`, `container.rs`, `namespaces.rs`

**Do not expand this until Ring 3 and namespaces work for ordinary processes.**

- [x] VMX constants; `vmxon`/`vmlaunch`/`vmresume` `asm` in `vmx.rs`
- [ ] `kvm::start_vm` sets `Running` and **does not** `vmlaunch`
- [ ] EPT, virtio device emulation for guests
- [ ] `container.rs` — OCI structs; start is comments (`fork`, `unshare`, `pivot_root`)
- [x] Namespace maps inherited on fork/clone; `CLONE_NEWUTS` isolates hostname (L2); `CLONE_NEWPID` isolates `getpid` (M1); `CLONE_NEWNS` isolates bind mounts (N1); `CLONE_NEWNET` isolates interfaces (O1); `CLONE_NEWUSER` maps the child to uid 0 (P1); `CLONE_NEWIPC` isolates SysV shm keys (Q1); `CLONE_NEWCGROUP` isolates cgroup paths (R1); `CLONE_NEWTIME` isolates CLOCK_MONOTONIC offsets (S1); `setns` joins an existing UTS ns (T1); `chroot` jails path lookup (U1); `pivot_root` keeps the old root at `put_old` (V1); OverlayFS merge+whiteout is VFS-live and mount-ns isolated (W1)
- [x] OverlayFS on VFS path lookup / list / write (W1); FUSE still unused
- [x] cgroup v2 **memory** enforcement (`GATE_CGROUP2 memory`). CPU quota and IO byte cap deny (`GATE_CGROUP cpu`, `GATE_CGROUP io`).

Note: `vmm.rs` is **process page tables**, not a hypervisor.

---
