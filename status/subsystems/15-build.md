← [Status index](../README.md) · [Do this first](../next.md)

# 15. Build System & Tooling

**Grade: Live (72%)** · `Makefile`, `kernel/Makefile`, `run.sh`

### Live
- [x] `./run.sh` / `make kernel` / BIOS & UEFI images
- [x] QEMU: 2G RAM, 2 CPUs, VirtIO disk, serial, cocoa/gtk display
- [x] Release: LTO, `opt-level = "z"`, 16 MB size gate
- [x] Local CI: `ci-fmt`, `ci-clippy`, `ci-size`, `ci-qemu`
- [x] Cross stubs for aarch64/riscv64
- [x] **`.github/workflows/ci.yml`** — docs, fmt, clippy+size, QEMU BIOS boot

### Missing (old status was wrong)
- [ ] **`flake.nix`** — `make nix-build` would fail; file absent
- [ ] Cargo workspace unifying `kernel` + `boot` + `tools`
- [ ] Feature flags so Unused modules do not compile into the demo kernel
- [ ] Signed ISO / Secure Boot artifacts

---
