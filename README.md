# KnoxOS

An operating system written in Rust. It boots in QEMU to an in-kernel software desktop.

This is **not** a production OS yet. The kernel compiles a large tree of modules; only the **live path** (what runs after `./run.sh`) counts. See [status.md](status.md) for an honest subsystem-by-subsystem score.

| What works today | What does not |
|------------------|---------------|
| BIOS/UEFI boot in QEMU to a 1920×1080 desktop | Isolated Ring 3 processes (`iretq` / `execve` as a running program) |
| PS/2 keyboard and mouse, in-kernel compositor | Preemptive RIP switch on the timer path for user tasks |
| In-memory VFS; opt-in VirtIO-blk + ext4/FAT32 | Default disk persistence; AHCI/NVMe DMA |
| Kernel shell, PTY, 60+ builtins | `/bin/sh` in userspace |
| Serial debug console | Sockets that put packets on the wire |

**Production OS readiness: ~32%.** **QEMU desktop demo: ~70%.**

## Quick start

```bash
# macOS
brew install qemu

# Linux
sudo apt install qemu-system-x86 ovmf   # or your distro equivalent

# Nightly Rust with rust-src (see kernel/rust-toolchain.toml)
./run.sh
```

`./run.sh` builds the kernel, creates BIOS (and optionally UEFI) disk images, and launches QEMU with 2 GiB RAM, 2 CPUs, a VirtIO disk, and serial on stdio.

| Command | Purpose |
|---------|---------|
| `./run.sh` | Build and run (BIOS, cocoa/gtk display) |
| `./run.sh --uefi` | UEFI boot (needs OVMF) |
| `make kernel` | Kernel ELF only |
| `make ci-all` | Local fmt / clippy / size gates |
| `make status` | Check that README, LICENSE, and images exist |

## Architecture (live path)

```
bootloader (BIOS/UEFI)
    → maps physical memory + framebuffer
kernel_main
    → GDT/TSS, IDT, PIC, LAPIC timer, heap
    → VFS, scheduler tables, syscall MSRs
    → software compositor + in-process apps
async executor (~60 FPS)
    → keyboard, mouse, redraw
    → idle thread HLT when the desktop has no work
```

Desktop apps are kernel functions dispatched by `WindowContentType`, not separate address spaces. The next real milestone is Gate B in `status.md`: map a static ELF, `iretq` to Ring 3, `sys_write` to serial, `sys_exit`.

## Layout

- `kernel/` — `no_std` kernel (`x86_64-unknown-none`; aarch64/riscv64 compile with shims)
- `boot/` — disk-image builder (`bootloader` crate, BIOS + UEFI)
- `status.md` — production-readiness score (source of truth)
- `tests/run_integration.sh` — QEMU serial-marker boot check

## License

[MIT](LICENSE)
