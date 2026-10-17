← [Status index](../README.md) · [Do this first](../next.md)

# 15. Build System & Tooling

**Grade: Live (95%)** · `Makefile`, `kernel/Makefile`, `run.sh`

### Live
- [x] `./run.sh` / `make kernel` / BIOS & UEFI images
- [x] QEMU: 2G RAM, 2 CPUs, VirtIO disk, serial, cocoa/gtk display
- [x] Release: LTO, `opt-level = "z"`, 16 MB size gate
- [x] Local CI: `ci-fmt`, `ci-clippy`, `ci-size`, `ci-qemu`
- [x] Cross stubs for aarch64/riscv64
- [x] **`.github/workflows/ci.yml`** — docs, fmt, clippy+size, QEMU BIOS boot

### Missing (old status was wrong)
- [x] **`flake.nix`** — present (devShell + QEMU/Make; kernel still rustup nightly)
- [x] Cargo workspace unifying `kernel` + `boot` + `tools` — `default-members` are host tools; kernel is `./scripts/kernel-cargo.sh` (`x86_64-unknown-none` + `-Zbuild-std`)
- [x] Feature flags so Unused modules do not compile into the demo kernel (`gui`/`net` default)
- [x] ISO + SHA256SUMS + optional GPG (`scripts/make-iso.sh`, `scripts/release-checksums.sh`). PK/KEK/db + OVMF Secure Boot QEMU boot (`scripts/qemu-secure-boot.sh`).

---
