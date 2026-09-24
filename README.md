# KnoxOS

An operating system written in Rust. It boots in QEMU to an in-kernel software desktop plus scheduled Ring 3 tasks.

This is **not** a production OS yet. The kernel compiles a large tree of modules; only the **live path** (what runs after `./run.sh`) counts. See [status.md](status.md) for an honest subsystem-by-subsystem score.

| What works today | What does not |
|------------------|---------------|
| BIOS/UEFI boot in QEMU to a 1920×1080 desktop | Default root is still a RAM VFS (persist snapshot on VirtIO-blk) |
| PS/2 keyboard and mouse; Ring 3 SHM clients (Gate F) | Most desktop apps still in-kernel `WindowContentType` |
| VirtIO-blk persist + AHCI/NVMe DMA (C1–C6) | ext4 JBD2; OverlayFS |
| Loopback + VirtIO-net DHCP/DNS/TCP CUBIC (D1–D5) | IPv6, TLS 1.3, e1000 as the socket NIC |
| Scheduled Ring 3: `execve`/`fork`/`wait`, signals, timer preempt + GPR/FPU save | Dynamic linker / glibc; USB HID |
| W^X, ChaCha20 `getrandom`, seccomp, capabilities (E1–E4) | SELinux on the VFS path |
| Per-CPU TSS + GS; AP online on QEMU `-smp 2` (I1) | SMP load balance of Ring 3 (APs still HLT) |

**Production OS readiness: ~70%.** **QEMU desktop demo: ~87%.**

## Quick start

See [BUILDING.md](BUILDING.md) for dependencies and make targets.

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
| `./tests/run_integration.sh` | QEMU serial-marker boot check |
| `make status` | Check that README, LICENSE, and images exist |

## Architecture (live path)

```
bootloader (BIOS/UEFI)
    → maps physical memory + framebuffer
kernel_main
    → GDT + per-CPU TSS, IDT, PIC, LAPIC timer, heap
    → VFS, scheduler tables, syscall MSRs, GS CpuLocal
    → SMP INIT/SIPI (AP loads its own TSS, then HLT)
    → Gate B2: iretq hello ELF → sys_write serial → sys_exit
    → Gate B3–B8 / I2: scheduled Ring 3, timer preempt with GPR+FPU save
    → software compositor + in-process apps + Ring 3 SHM clients
async executor (~60 FPS)
    → keyboard, mouse, redraw
    → idle thread HLT when the desktop has no work
```

See [status.md](status.md) for the gate list. [CONTRIBUTING.md](CONTRIBUTING.md) explains what “live path” means.

## Layout

- `kernel/` — `no_std` kernel (`x86_64-unknown-none`; aarch64/riscv64 compile with shims)
- `boot/` — disk-image builder (`bootloader` crate, BIOS + UEFI)
- `status.md` — production-readiness score (source of truth)
- `tests/run_integration.sh` — QEMU serial-marker boot check

## License

[MIT](LICENSE)
