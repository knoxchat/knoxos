# KnoxOS Production Readiness Status

> **Last Updated**: 2026-10-16
> **Version**: 0.2.3
> **Honesty rule**: a module that compiles is not a feature. Only **Live** work counts toward production.

The full status tree lives in [`status/`](status/README.md). This file is the stable root path CI and old links still check.

**Start here: [status/next.md](status/next.md)** — what to do first.

| File | Purpose |
|------|---------|
| [status/next.md](status/next.md) | Ordered remaining work (G5 deny + ext4 root + no-PS/2 cursor + Secure Boot + cross-arch QEMU boot landed) |
| [status/README.md](status/README.md) | Honesty tags, what a perfect OS is, file map |
| [status/remaining.md](status/remaining.md) | Every open checkbox (inventory, not sequence) |
| [status/blockers.md](status/blockers.md) | Architectural gaps |
| [status/live.md](status/live.md) | What the QEMU boot path actually does |
| [status/subsystems.md](status/subsystems.md) | 18-area grades |
| [status/gates/](status/gates/README.md) | Gates A–BL + BM1–BM4 + F5–F8 + C7–C9 + G3–G5 + I4 + USB HID URB + XHCI MMIO + no-PS/2 cursor + Secure Boot + cross-arch boot |
| [status/progress.md](status/progress.md) | Score bars and anti-goals |

Integration: `./tests/run_integration.sh` **286/286** on QEMU (2026-10-16).
