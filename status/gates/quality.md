# Gate G — quality bar, and what is later

← [Status index](../README.md) · [Do this first](../next.md) · [Gates](README.md)

### Gate G — Quality bar (parallel from day one)

| ID | Task | Done when |
|----|------|-----------|
| G1 | `README.md` + `LICENSE` | **Done** |
| G2 | GitHub Actions: fmt, clippy, size, QEMU boot | **Workflow present** (`.github/workflows/ci.yml`) |
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | **Done** — default `gui`+`net`; `stub-drivers`/`fs-ext4` off (`GATE_G3 features live`) |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | **Partial** — prior lettered ENOSYS plus `adjtimex`/`settimeofday`/`clock_settime`/`mq_notify` (`GATE_G4 enosys`); `sched_setaffinity`/`sched_getaffinity` are real. Remaining documented no-ops: `madvise`, `mlock*`, `readahead`, `fadvise64`, `set_robust_list` |
| G5 | `unsafe` SAFETY comments + size budget | **Partial** — 16 MB size gate holds; SAFETY on boot PIC/memory + SMP idle. Crate-level `undocumented_unsafe_blocks = warn` is off because `scripts/pre-commit.sh` uses clippy `-D warnings`. Deny-all still open |

### Explicitly later (after Gates A–E)

KVM, containers/k8s, Btrfs/ZFS, Wi-Fi, TLS 1.3, GPU compute, AI inference, package formats, Chromium/Vivaldi, aarch64/riscv64 **runtime**, Thunderbolt, TPM measured boot.

Shipping these before Gate B **increases** the distance to a perfect OS.

---
