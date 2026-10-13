# What actually works today

← [Status index](README.md) · [Do this first](next.md)

KnoxOS **does boot in QEMU** to an in-kernel software desktop. This is real and worth keeping.

### Live path (QEMU)

1. Bootloader maps physical memory and a 1920×1080 framebuffer.
2. GDT + TSS, IDT, PIC 8259, LAPIC timer, serial UART TX.
3. 512 MiB kernel heap (`linked_list_allocator` + slab).
4. PS/2 keyboard (IRQ1) and mouse (IRQ12); USB tablet when present.
5. In-memory VFS with Linux FHS layout; **opt-in** VirtIO-blk + ATA PIO; ext4 and FAT32 can use that block layer. **Gate C1** persist blob store round-trips a file through VirtIO-blk (`GATE_C1 persist complete`). **Gate C2** write-ahead log replays a committed record after a simulated crash (`GATE_C2 journal recovered`). **Gate C3** dirty page writeback overlays a 4 KiB range without replacing sibling pages (`GATE_C3 writeback complete`). **Gate C4** AHCI command-list DMA write/read round-trips a sector vs QEMU (`GATE_C4 ahci dma complete`). **Gate C5** NVMe PRP DMA write/read round-trips a sector (`GATE_C5 nvme dma complete`). **Gate C6** `/etc` (outside the old persist prefixes) round-trips through VirtIO-blk (`GATE_C6 vfs persist`).
6. Software compositor: 32bpp BGRA, damage rects, window manager, taskbar, start menu, 17 in-process apps.
7. Kernel shell + terminal (parser, pipes, glob, env, 60+ builtins) running **inside the kernel**, not as `/bin/sh` in Ring 3.
8. **Gate B2 hello** — static ELF `iretq`s to Ring 3, `sys_write`s `hello from userspace`, `sys_exit`s back to the kernel.
9. **Gate B3–B6 scheduled Ring 3** — `execve(/bin/hello)` + `waitpid`; `fork` child runs and is reaped; SIGKILL / SIGSEGV / PTY Ctrl+C terminate user tasks; `/bin/sh` runs on a kernel PTY.
10. **Gate D1 loopback sockets** — UDP/TCP `send` copies into the peer `recv_buf` on `lo`; a Ring 3 program `sendto`/`recvfrom`s `ping` and prints `GATE_D1 loopback complete`.
11. **Gate D2 virtio-net** — TX writes the avail ring with guest-physical bounce buffers; RX walks the used ring; a DHCP DISCOVER gets a UDP reply from QEMU user-net (`GATE_D2 virtio-net complete`).
12. **Gate D3 DHCP apply** — DISCOVER/OFFER/REQUEST/ACK writes `eth0` IPv4 + default route (`GATE_D3 dhcp applied`).
13. **Gate D4 DNS + TCP** — UDP DNS to QEMU user-net; TCP SYN/ACK plus RTO retransmit; HTTP GET to `10.0.2.100` (`GATE_D4 dns tcp complete`).
14. **Gate D5 TCP CUBIC** — congestion window grows on ACK, shrinks on loss, and limits send (`GATE_D5 cubic window`).
15. **Gate E1–E4 enforcement** — W^X/`mprotect` RWX denied + ASLR; ChaCha20 `getrandom`; seccomp EPERM; unprivileged bind `<1024` fails.
16. **Gate B7 sigreturn** — Ring 3 `rt_sigaction(SIGINT)` handler `ret`s into a trampoline; `rt_sigreturn` restores `pause` (`GATE_B7 sigreturn complete`).
17. **Gate B8 timer preempt** — a Ring 3 `jmp $` spinner is switched out by the APIC timer without a syscall; a peer writer runs (`GATE_B8 timer preempt complete`).
18. **Gate F1–F4 isolated clients** — Ring 3 program mmaps a 64×64 BGRA buffer, `ioctl(/dev/wl0)` presents it; compositor SHM round-trips the pixels (`GATE_F1 client isolated`, `GATE_F2 shm commit`). A Ring 3 terminal client presents as Empty+SHM, not `WindowContentType::Terminal` (`GATE_F3 terminal isolated`). Empty launcher stubs are gone; Paint is a Ring 3 SHM client (`GATE_F4 launcher userspace`).
19. **Gate H1–H4** — CoW write-fault copies (H1); file-backed mmap faults one page (H2); inotify sees VFS write/unlink (H3); guarded kernel stack + OOM from empty buddy pool (H4).
20. Async executor loop: keyboard, mouse, ~60 FPS redraw. Idle kernel thread `HLT`s when the desktop has no work.
21. **Gate I1 SMP online** — per-CPU TSS + GS-relative `CpuLocal`; AP INIT/SIPI reaches 64-bit and loads its own TSS (`GATE_I1 smp online`).
22. **Gate I2 IRQ GPRs** — naked APIC-timer stub saves all GPRs + FXSAVE; spinner `rbx` magic survives preemption (`GATE_I2 irq gprs`).
23. **Gate I3 AP Ring 3** — a `getcpu` ELF affinity-pinned to CPU 1 `iretq`s on the AP, syscalls, and exits (`GATE_I3 ap ring3`).

Syscall gates **J–BL** (clone, namespaces, VFS, sockets, ENOSYS honesty) are proven on the same boot path. They are archived under [gates/syscalls.md](gates/syscalls.md), not repeated here.

Integration: `./tests/run_integration.sh` **266/266** on QEMU (2026-10-13).
