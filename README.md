# KnoxOS

An operating system written in Rust. It boots in QEMU to an in-kernel software desktop plus scheduled Ring 3 tasks.

This is **not** a production OS yet. The kernel compiles a large tree of modules; only the **live path** (what runs after `./run.sh`) counts. See [status/next.md](status/next.md) for what to do first, and [status/README.md](status/README.md) for the subsystem scores.

| What works today | What does not |
|------------------|---------------|
| BIOS/UEFI boot in QEMU to a 1920×1080 desktop | Default root is still a RAM VFS (persist snapshot on VirtIO-blk) |
| PS/2 keyboard and mouse; Ring 3 SHM clients (Gate F) | Most desktop apps still in-kernel `WindowContentType` |
| VirtIO-blk persist + AHCI/NVMe DMA (C1–C8) + JBD2 replay | ext4 as default root (RAM VFS still boots) |
| Loopback + VirtIO-net DHCP/DNS/TCP CUBIC (D1–D5) | IPv6, TLS 1.3, e1000 as the socket NIC |
| Scheduled Ring 3: `execve`/`fork`/`wait`, signals, timer preempt + GPR/FPU save; VirtIO-GPU primary scanout + hardware pointer (`-vga virtio`); USB HID URB + XHCI MMIO | Desktop without PS/2 unproven (PS/2 remains fallback); ext4 is not default root |
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

See [status/next.md](status/next.md) for remaining work, [docs/architecture.md](docs/architecture.md) for the boot/scanout sketch, and [status/gates/](status/gates/README.md) for the completed gate list. [CONTRIBUTING.md](CONTRIBUTING.md) explains what “live path” means.

## Layout

- `userspace/` — Ring 3 program sources (`apps/`) and system libs (`libs/knox-gpui`, `libs/knox-ui`, `libs/ld.so`, `libs/libc`)
- `abi/` — syscall contract (`SyscallNumber`, `SyscallError`, `stat`/`sockaddr`); handlers stay in `kernel/src/syscall/`
- `kernel/` — `no_std` kernel (`x86_64-unknown-none`; aarch64/riscv64 compile with shims)
- `kernel/src/drivers/` — hardware (MMIO / IRQ / DMA)
- `boot/` — disk-image builder (`bootloader` crate, BIOS + UEFI)
- `status/` — production-readiness score (source of truth); start at [status/next.md](status/next.md)
- `docs/` — architecture, syscalls, drivers, GUI (live path)
- `tests/run_integration.sh` — QEMU serial-marker boot check

## Acknowledgments

The desktop toolkit was designed with thanks to two Apache-2.0 projects. They are **design references only**: not Cargo dependencies, not workspace members, and not vendored in this tree. Knox GPUI / Knox UI are new `no_std` MIT code.

- **[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)** by [Zed Industries](https://zed.dev) — Entity, Element, Scene, and keymap. The `gpui` crate is [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0). The Zed **application** (editor, workspace, command palette, and related crates) is [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html); KnoxOS follows UX patterns from it and does not copy that source.
- **[GPUI Kit](https://github.com/longbridge/gpui-kit)** by [Longbridge](https://github.com/longbridge) — the control catalog, token roles, and “native, quiet, precise” visual language reimplemented in `userspace/libs/knox-ui`. Software and code examples are [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0). Kit documentation prose and original illustrations are also [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); this repo does not copy those.
- **[Lucide](https://lucide.dev)** — stroke path data for `knox_ui::Icon` ([ISC](https://github.com/lucide-icons/lucide/blob/main/LICENSE)).

## License

KnoxOS is [MIT](LICENSE). The credits above do not change that: we did not vendor GPUI, GPUI Kit, or the GPL Zed app.
