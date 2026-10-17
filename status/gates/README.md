# Gates

← [Status index](../README.md) · [Do this first](../next.md)

Stop adding Phase 34 modules. **Wire, delete, or feature-gate.** Sequence is dependency order.

Gates **A–F, H–I** made KnoxOS an OS. Gates **J–BL** proved syscalls and namespaces on the same QEMU boot path. **BM1** is a userspace `ld.so`. **BM2** is runtime `dlopen` via VMM `mmap`. **BM3** is static musl CRT. **BM4** is a dynamically linked musl binary. They are **done**. Remaining work is [next.md](../next.md), not another lettered ENOSYS gate.

| File | Gates | Status |
|------|-------|--------|
| [foundational.md](foundational.md) | A–F, H, I, BM1–BM4 | **Done** — boot, Ring 3, persist, packets, enforcement, isolated clients (F5 AF_UNIX), VirtIO-GPU scanout/cursor (F8), memory leftovers, SMP, `ld.so`, `dlopen`, static + dynamic musl CRT |
| [syscalls.md](syscalls.md) | J–BL | **Done** — clone, namespaces, VFS, sockets, honesty ENOSYS |
| [quality.md](quality.md) | G + explicitly later | **Done** — G1–G5; KVM/AI/Wi-Fi stay later |

Live-path evidence for A–I, BM1–BM4, F5–F8, C7–C9, G3–G5, I4, D6, USB HID URB, XHCI MMIO, no-PS/2 cursor, Secure Boot, cross-arch serial boot, udev/quota/cron/vsync/TERM/cgroup/job/IMA/lockdown/UEFI/wipe/NM/NTFS/heredoc/VT100/HDA/PA/IOMMU/THP/KASLR/AML/KO plus NUMA/KSM/nested-IRQ/livepatch/exFAT/CPU-IO/suspend/logind/AES-GCM/Poly1305/font-fallback/XFS/X25519/Ed25519/locale/IME/MSC/NAT/DRM/GSUB is summarized in [live.md](../live.md). J–BL tables are the archive of those proofs. Integration: **336/336**.
