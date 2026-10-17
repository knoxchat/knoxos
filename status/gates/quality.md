# Gate G — quality bar, and what is later

← [Status index](../README.md) · [Do this first](../next.md) · [Gates](README.md)

### Gate G — Quality bar (parallel from day one)

| ID | Task | Done when |
|----|------|-----------|
| G1 | `README.md` + `LICENSE` | **Done** |
| G2 | GitHub Actions: fmt, clippy, size, QEMU boot | **Workflow present** (`.github/workflows/ci.yml`) |
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | **Done** — default `gui`+`net`+`fs-ext4`; `stub-drivers` off (`GATE_G3 features live`) |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | **Done** — unused clock/time numbers ENOSYS (`GATE_G4 enosys`); `madvise`/`mlock*`/`readahead`/`fadvise64`/`set_robust_list` store real state (`GATE_G4 audit live`) |
| G5 | `unsafe` SAFETY comments + size budget | **Done** — 16 MB size gate holds; every `unsafe {` has a `// SAFETY:` comment; `clippy::undocumented_unsafe_blocks = deny` (`GATE_G5 safety comments`) |

### Explicitly later (after Gates A–E)

KVM, containers/k8s, ZFS, WPA3/bridge, TLS 1.3 handshake/X.509 chain, GPU compute, AI inference, package formats, Chromium/Vivaldi, aarch64/riscv64 **runtime**, Thunderbolt, TPM measured boot.

Shipping these before Gate B **increases** the distance to a perfect OS.

---
