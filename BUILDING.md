# Building KnoxOS

KnoxOS is a `no_std` kernel for `x86_64-unknown-none`. The live demo path is QEMU.

## Requirements

- Nightly Rust with `rust-src`, `rustfmt`, `clippy`, and `llvm-tools-preview` (see `kernel/rust-toolchain.toml`; `rustup` will install them)
- QEMU (`qemu-system-x86_64`)
- For UEFI: OVMF firmware (Homebrew `qemu` ships it; on Debian/Ubuntu `ovmf`)

```bash
# macOS
brew install qemu

# Debian / Ubuntu
sudo apt install qemu-system-x86 ovmf
```

## Run

```bash
./run.sh              # BIOS, cocoa/gtk display, 2 GiB RAM, 2 CPUs
./run.sh --uefi       # UEFI (needs OVMF)
./run.sh --release    # LTO + size-optimized kernel
```

`./run.sh` builds the kernel, writes a BIOS (and optionally UEFI) disk image, and starts QEMU with VirtIO-blk, AHCI, NVMe, and virtio-net.

| Command | Purpose |
|---------|---------|
| `make kernel` | Kernel ELF only (`kernel/target/x86_64-unknown-none/release/knoxos-kernel`) |
| `make ci-all` | fmt, clippy, 16 MiB size gate |
| `./tests/run_integration.sh` | QEMU boot; waits for serial markers (Gates B–I) |
| `make status` | Check README, LICENSE, and images exist |

## Layout

- `kernel/` — kernel crate (`src/main.rs` + `src/lib.rs`)
- `boot/` — disk-image builder (`bootloader` crate)
- `status.md` — production-readiness score (source of truth)
- `tests/` — QEMU integration harness

Do not add a new `pub mod` unless it is on the live boot path. See [CONTRIBUTING.md](CONTRIBUTING.md) and [status.md](status.md).
