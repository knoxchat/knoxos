# Gates

← [Status index](../README.md) · [Do this first](../next.md)

Stop adding Phase 34 modules. **Wire, delete, or feature-gate.** Sequence is dependency order.

Gates **A–F, H–I** made KnoxOS an OS. Gates **J–BL** proved syscalls and namespaces on the same QEMU boot path. **BM1** is a userspace `ld.so`. They are **done**. Remaining work is [next.md](../next.md), not another lettered ENOSYS gate.

| File | Gates | Status |
|------|-------|--------|
| [foundational.md](foundational.md) | A–F, H, I, BM1 | **Done** — boot, Ring 3, persist, packets, enforcement, isolated clients, memory leftovers, SMP, `ld.so` |
| [syscalls.md](syscalls.md) | J–BL | **Done** — clone, namespaces, VFS, sockets, honesty ENOSYS |
| [quality.md](quality.md) | G + explicitly later | **Partial** — G1/G2 done; G3–G5 open; KVM/AI/Wi-Fi stay later |

Live-path evidence for A–I, BM1, and C7 is summarized in [live.md](../live.md). J–BL tables are the archive of those proofs. Integration: **268/268**.
