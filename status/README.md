# KnoxOS status

> **Last updated**: 2026-10-18
> **Version**: 0.2.3 (`knoxos-kernel` Cargo.toml; boot banner prints v0.2.3)
> **Architecture**: x86_64 (primary, QEMU-proven) · aarch64 / riscv64 (compile-time ports)
> **Codebase**: 609 Rust files in `kernel/src` · ~347,000 lines · 407 `pub mod` entries
> **Honesty rule**: a module that compiles is not a feature. Only **Live** work counts toward production.

**Start here:** [next.md](next.md) — what to do first, in order.

This directory is the source of truth for KnoxOS readiness. The old single-file `status.md` at the repo root is an index that points here.

---

## How to read these files

Every subsystem is scored on the **live path** (what runs after `./run.sh`), not on file count.

| Tag | Meaning |
|-----|---------|
| **Live** | On the boot/desktop path. Touches hardware, page tables, or real data. |
| **Wired** | Real algorithms exist and are called, but incomplete or not production-grade. |
| **Unused** | Source exists (often large) and is initialized or never called. Does not change behavior. |
| **Stub** | Types, logs, `Ok(0)`, zero-fill, or `assert!(true)`. |

The March 2026 edition scored type definitions as complete. That inflated Kernel Core to 95% and Process/Scheduling to 90% while Ring 3 was never entered. This edition scores **behavior**.

Tick a checkbox only when the **Done when** criterion is met on QEMU (or real hardware). Unused source does not count.

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

Until gates 2–4 pass, extra drivers, AI, KVM, and package managers are theater. Gates A–F, H–I, syscall gates J–BL, BM1–BM4 (`ld.so` + musl), F5 (Wayland AF_UNIX), C7–C8 (`/bin` persist + JBD2), G3–G4 (feature flags + syscall audit), F6–F8 (calculator + Files userspace + VirtIO-GPU scanout/cursor), I4 (SMP balance), USB HID URB completion, and XHCI mapped MMIO are **done** on QEMU. The remaining work is in [next.md](next.md).

Stop adding Phase 34 modules. **Wire, delete, or feature-gate.**

---

## File map

Read top to bottom if you are choosing work. Skip the archives unless you need evidence.

| File | What it is | When to open it |
|------|------------|-----------------|
| **[next.md](next.md)** | Ordered remaining work | Always first |
| [remaining.md](remaining.md) | Every open `- [ ]` | Inventory, not sequence |
| [blockers.md](blockers.md) | Architectural gaps that still matter | After next.md |
| [live.md](live.md) | What the QEMU boot path actually does | “Does X work today?” |
| [subsystems.md](subsystems.md) | Grades and one-line truth for 18 areas | Scores |
| [subsystems/](subsystems.md) | Live / Wired / Stub checklists | Implementing in one area |
| [gates/](gates/README.md) | Gate A–BL tables (almost all **Done**) | Proving a gate; not the backlog |
| [progress.md](progress.md) | Score bars, anti-goals, vs March 2026 | Reporting |

### Subsystems

| # | File | Grade | Priority |
|---|------|-------|----------|
| 1 | [01-kernel-core.md](subsystems/01-kernel-core.md) | Wired 82% | High |
| 2 | [02-memory.md](subsystems/02-memory.md) | Wired 74% | Critical |
| 3 | [03-process.md](subsystems/03-process.md) | Wired 99% | Critical |
| 4 | [04-filesystem.md](subsystems/04-filesystem.md) | Wired 99% | Critical |
| 5 | [05-networking.md](subsystems/05-networking.md) | Wired 72% | Critical |
| 6 | [06-drivers.md](subsystems/06-drivers.md) | Wired 58% | Critical |
| 7 | [07-gui.md](subsystems/07-gui.md) | Live 95% | Medium |
| 8 | [08-shell.md](subsystems/08-shell.md) | Live 82% | Medium |
| 9 | [09-security.md](subsystems/09-security.md) | Wired 80% | Critical |
| 10 | [10-services.md](subsystems/10-services.md) | Wired 42% | High |
| 11 | [11-virt.md](subsystems/11-virt.md) | Stub 30% | Low — later |
| 12 | [12-ai.md](subsystems/12-ai.md) | Wired 22% | Low — later |
| 13 | [13-binary-compat.md](subsystems/13-binary-compat.md) | Wired 99% | High |
| 14 | [14-i18n.md](subsystems/14-i18n.md) | Live 68% | Low |
| 15 | [15-build.md](subsystems/15-build.md) | Live 75% | Medium |
| 16 | [16-testing.md](subsystems/16-testing.md) | Wired 86% | Critical |
| 17 | [17-docs.md](subsystems/17-docs.md) | Wired 70% | Critical |
| 18 | [18-ci.md](subsystems/18-ci.md) | Wired 55% | High |
