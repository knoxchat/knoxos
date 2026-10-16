# Architecture (live path)

KnoxOS is a `no_std` kernel. A module that compiles is not a feature. This page describes **what runs after `./run.sh`**, not the unused tree. Remaining work: [status/next.md](../status/next.md).

## Boot

```
bootloader (BIOS or UEFI)
    → identity-maps physical RAM + 1920×1080 framebuffer
kernel_main
    → GDT + per-CPU TSS, IDT, PIC 8259, LAPIC timer, serial UART
    → 512 MiB heap; buddy over leftover frames
    → VirtIO-blk persist, AHCI/NVMe DMA, VirtIO-net, VirtIO-GPU
    → VFS (RAM namespace + persist snapshot), scheduler, syscall MSRs
    → SMP INIT/SIPI (`-smp 2`): AP loads its own TSS + GS
    → Gate B2: `iretq` hello ELF → `sys_write` serial → `sys_exit`
    → Gates B3–B8 / F / H / I / J–BL / BM: scheduled Ring 3 demos
    → software compositor (VGA FB) + VirtIO-GPU 2D scanout + cursor plane
async executor (~60 FPS)
    → PS/2 + XHCI HID (mapped MMIO), damage-rect present, idle `HLT`
```

x86_64 on QEMU is the proven runtime. aarch64 and riscv64 compile with shims; they do not boot a desktop.

## Rings

| Ring | What runs there |
|------|-----------------|
| 0 | Kernel, software compositor, interactive PTY terminal, drivers, syscall path |
| 3 | Static/dynamic musl ELFs, `ld.so`, SHM desktop clients (calculator, Files, …) |

User tasks have their own CR3. Preemption saves GPRs + FXSAVE. `fork` is CoW; `execve` maps a new image.

## Persistence

The default root is still an in-memory VFS. A VirtIO-blk blob store (`KNOXPERSIST`) plus a WAL round-trips `/etc` and `/bin` across reboot (Gates C1–C2, C6–C7). Virtual FS (`/dev` `/proc` `/sys` `/run`) is rebuilt at boot. JBD2 replay walks descriptor + commit blocks on a ramdisk (`GATE_C8 jbd2 replay`); ext4 mount uses the same walker when `fs-ext4` is on. The boot root is not yet an ext4 volume.

## Display

1. Bootloader VGA/BGA framebuffer — software compositor back-buffer, `present_rect` to HW.
2. VirtIO-GPU controlq — `RESOURCE_CREATE_2D` + `ATTACH_BACKING` + `SET_SCANOUT` + `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH` (`GATE_F8 virtio-gpu scanout`).
3. VirtIO-GPU cursorq — `UPDATE_CURSOR` / `MOVE_CURSOR`. When that completes, the desktop skips the software cursor blit (`GATE_F8 hw cursor`).

## Isolation vs compose

Wayland today is `/dev/wl0` ioctl plus an AF_UNIX socket at `/run/wayland-0` that carries a SHM vaddr. It is not a `wl_display` object-protocol server. Clients are separate address spaces; the compositor still lives in the kernel.

## Further reading

- [syscalls.md](syscalls.md) — implemented vs ENOSYS
- [drivers.md](drivers.md) — live vs stub devices
- [gui.md](gui.md) — compositor and Ring 3 clients
- [status/live.md](../status/live.md) — serial-marker evidence
