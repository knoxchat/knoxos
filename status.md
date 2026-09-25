# KnoxOS Production Readiness Status

> **Last Updated**: 2026-09-25
> **Version**: 0.2.2 (`knoxos-kernel` Cargo.toml; boot banner prints v0.2.2)
> **Architecture**: x86_64 (primary, QEMU-proven) · aarch64 / riscv64 (compile-time ports)
> **Codebase**: 609 Rust files in `kernel/src` · ~347,000 lines · 407 `pub mod` entries
> **Honesty rule**: a module that compiles is not a feature. Only **Live** work counts toward production.

---

## How to read this document

Every subsystem is scored on the **live path** (what runs after `./run.sh`), not on file count.

| Tag | Meaning |
|-----|---------|
| **Live** | On the boot/desktop path. Touches hardware, page tables, or real data. |
| **Wired** | Real algorithms exist and are called, but incomplete or not production-grade. |
| **Unused** | Source exists (often large) and is initialized or never called. Does not change behavior. |
| **Stub** | Types, logs, `Ok(0)`, zero-fill, or `assert!(true)`. |

The March 2026 edition scored type definitions as complete. That inflated Kernel Core to 95% and Process/Scheduling to 90% while Ring 3 was never entered. This edition scores **behavior**.

---

## What a perfect KnoxOS is

A perfect KnoxOS is not “more modules.” It is an OS that satisfies these **gates**. Nothing else is production.

1. **Boot** — BIOS and UEFI on QEMU, then real hardware, to a working desktop.
2. **Isolate** — Ring 3 processes with their own page tables, preemptive context switch (RIP + GPRs + FPU), signals, and `fork`/`execve` that actually run.
3. **Persist** — Read and write real disks (VirtIO-blk, then AHCI/NVMe) through VFS with a journaled filesystem and `fsync`.
4. **Connect** — Sockets send and receive on a NIC; DHCP configures an address; DNS resolves; TCP retransmits.
5. **Enforce** — W^X, ASLR, CSPRNG, capabilities, and MAC on the syscall/VFS path — not only in structs.
6. **Compose** — GUI clients in separate address spaces (Wayland or equivalent), not in-kernel `WindowContentType`.
7. **Prove** — Tests that can fail, CI on every push, README/LICENSE, and a threat model.

Until gates 2–4 pass, extra drivers, AI, KVM, and package managers are theater.

---

## What actually works today

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

### Architectural blockers (must fix first)

| Blocker | Evidence | Why it blocks a perfect OS |
|---------|----------|----------------------------|
| **Scheduled Ring 3** | Hello is a CFS task with its own CR3; `execve`/`waitpid`/`fork`/`/bin/sh` run on the boot path. | Desktop apps other than the Gate F demo are still in-kernel. |
| **Ring 3 timer preemption** | Naked APIC-timer stub saves GPRs + FXSAVE then `enter_context`s (`GATE_B8` + `GATE_I2 irq gprs`). | Done for the spinning-user case. |
| **SMP** | INIT/SIPI trampoline; AP loads per-CPU TSS + GS, starts its LAPIC timer, and runs a pinned Ring 3 task (`GATE_I1 smp online`, `GATE_I3 ap ring3`). | Further APs still share one CFS `current` for the BSP desktop. |
| **Signal frames for handlers** | Default terminate plus a live SIGINT handler + `rt_sigreturn` (B7). | Catching SIGINT in a user handler is done. |
| **Sockets do not transmit off-box** | Loopback `send` delivers to a peer `recv_buf`. VirtIO-net TX/RX rings are live (D2). DHCP writes `eth0`. DNS + TCP SYN/retransmit/HTTP are live (D4). CUBIC cwnd limits send (D5). | Off-box TCP is live; CUBIC is wired. |
| **AHCI and NVMe DMA are live** | AHCI command-list + PRDT round-trips a sector (C4). NVMe admin/I/O queues + PRP bounce round-trip a sector (C5). | Bare-metal beyond QEMU still unproven. |
| **Default VFS is RAM with persist snapshot** | Inodes are `Vec<u8>`. Persist blob store round-trips through VirtIO-blk (C1) with a WAL that replays after crash (C2). Root snapshot includes `/etc` (C6). inotify is live on mutate (H3). | Reboot still loses virtual FS (`/dev` `/proc` `/sys`) and boot-generated `/bin`. |
| **Security not on the deny path** | SELinux unused by VFS. Many syscalls `Ok(0)`. | W^X, ChaCha20 `getrandom`, seccomp EPERM, and CapNetBindService are live (E1–E4). |
| **Breadth without wiring** | 407 modules. GPU compositor, Wayland, KVM `vmlaunch`, overlayfs never on the live path. | Compile time and maintenance grow; capability does not. |

---

## Code distribution

| Area | Files | Lines | Role |
|------|-------|-------|------|
| Kernel top-level modules | 404 | ~247,000 | Core + many Unused/Stub subsystems |
| GUI & desktop (`gui/`) | 161 | ~91,700 | Live software compositor + in-process apps |
| Shell & builtins | 17 | ~13,200 | Live in-kernel shell |
| Syscall interface | 14 | ~7,200 | Dispatch table; many no-ops |
| Terminal emulator | 10 | ~5,100 | Live in-kernel terminal |
| Boot crate | — | ~90 | Image builder |
| **Total `kernel/src`** | **609** | **~346,900** | |

~400 `pub mod` entries does **not** mean 400 working subsystems. Prefer deleting or gating Unused modules over adding Phase 34+.

---

## Subsystem status summary

Percentages are **production usefulness**, not lines of code.

| # | Subsystem | Grade | Live % | Priority | One-line truth |
|---|-----------|-------|--------|----------|----------------|
| 1 | Kernel Core | Wired | 82% | High | Interrupts and timers work; GS-relative CpuLocal; per-CPU TSS; MADT IOAPIC + ISO; AP online then Ring 3 (I3). |
| 2 | Memory Management | Wired | 58% | **Critical** | Demand paging + CoW #PF (H1) + file-backed fault-in (H2); OOM on frame alloc; guarded kernel stacks (H4). |
| 3 | Process & Scheduling | Wired | 78% | **Critical** | Kernel-thread RIP switch + idle HLT; **Gate B2–B8** scheduled Ring 3; IRQ GPR+FPU save (I2); AP Ring 3 (I3). |
| 4 | Filesystem & Storage | Wired | 72% | **Critical** | VirtIO-blk + persist C1–C6 + AHCI/NVMe DMA; inotify on VFS mutate (H3). |
| 5 | Networking | Wired | 64% | **Critical** | Loopback live; VirtIO-net D2; DHCP applies eth0 (D3); DNS + TCP SYN/RTO/HTTP (D4); CUBIC cwnd (D5). |
| 6 | Device Drivers | Wired | 40% | **Critical** | PCI, PS/2, UART, VirtIO-blk, AHCI DMA, NVMe DMA live; USB/GPU mostly stub. |
| 7 | GUI & Desktop | Live | 82% | Medium | In-kernel demo plus Ring 3 SHM clients (F1–F4); interactive desktop terminal still in-kernel for PTY I/O. |
| 8 | Shell & Terminal | Live | 82% | Medium | Real parser/PTY/glob; Ring 3 `/bin/sh` on a PTY; live `sigreturn`; desktop terminal still in-kernel. |
| 9 | Security & Cryptography | Wired | 42% | **Critical** | AES/SHA software; W^X + ChaCha20 CSPRNG + seccomp deny + CapNetBindService live (E1–E4). |
| 10 | System Services | Wired | 28% | High | In-kernel units and in-memory D-Bus; no real supervision. |
| 11 | Virtualization & Containers | Stub | 12% | Low | VMX `asm` unused; containers are comments. |
| 12 | AI/ML | Wired | 22% | Low | GGUF parse + naive CPU; GPU matmul unused. |
| 13 | Binary Compatibility | Wired | 45% | **Critical** | Static hello `iretq`s; `execve`/`fork`/`/bin/sh`; live `rt_sigaction` + `rt_sigreturn`. |
| 14 | Internationalization & Fonts | Live | 68% | Low | TTF, CJK, RTL on the compositor; locale loading partial. |
| 15 | Build System & Tooling | Live | 75% | Medium | Make/QEMU work; `flake.nix` missing. |
| 16 | Testing & Quality | Wired | 48% | **Critical** | Real VFS/widget/DNS/buddy tests; C4–C6/D3–D5/E1–E4/F2–F4/H1–H4/I1–I3 self-tests; `assert!(true)` tests removed. |
| 17 | Documentation | Wired | 42% | **Critical** | README + LICENSE + BUILDING + CONTRIBUTING + this file. Architecture guides still missing. |
| 18 | CI/CD & Release | Wired | 30% | High | `.github/workflows/ci.yml` (fmt, clippy, size, QEMU boot); no signed releases. |

**QEMU desktop demo readiness: ~88%** (boots, paints, clicks, types; serial prints `hello from userspace`; Gate B3–B8 scheduled userspace; Gate D1–D5 packets; Gate C1–C6 storage; Gate E1–E4 enforcement; Gate F1–F4 isolated clients; Gate H1–H4 memory/VFS; Gate I1–I3 SMP + IRQ GPRs + AP Ring 3).
**Production OS readiness: ~72%** (Gate B2–B8, C1–C6, D1–D5, E1–E4, F1–F4, H1–H4, I1–I3). `./tests/run_integration.sh` **46/46** on QEMU (2026-09-25).

---

## 1. Kernel Core

**Grade: Wired (82%)** · `gdt.rs`, `interrupts.rs`, `smp.rs`, `apic_timer.rs`, `acpi_tables.rs`, `cmdline.rs`

### Live
- [x] GDT + TSS load on BSP (`gdt::init`)
- [x] IDT: breakpoint, #PF, #GP, #UD, #SS, #DF, NMI, MCE, timer, kbd, mouse, virtio, ATA, APIC timer
- [x] PIC 8259 init and IRQ unmask (timer, kbd, cascade, virtio, mouse, ATA)
- [x] LAPIC detection, APIC timer + PIT calibration
- [x] Serial UART 16550 TX (debug console)
- [x] Kernel heap 512 MiB + slab
- [x] Boot splash + long `kernel_main` init sequence
- [x] Panic handler
- [x] RTC CMOS read
- [x] Page-fault → `vmm::handle_page_fault` (when a process address space exists)
- [x] ACPI RSDP scan; MADT / FADT / HPET / MCFG parse from physical memory

### Wired but incomplete
- [x] **IOAPIC** — MADT address + interrupt-source overrides applied after ACPI parse (IRQ0→GSI 2 on QEMU); all IRQs still to BSP
- [x] **SMP** — real INIT/SIPI trampoline 16→32→64; AP loads per-CPU TSS + GS, STI + LAPIC timer, runs pinned Ring 3 (`GATE_I1 smp online`, `GATE_I3 ap ring3`); `balance_load` does not steal AP-pinned tasks
- [x] **Per-CPU data** — GS-relative `CpuLocal` (`gs:[0]`/`gs:[8]` syscall scratch + `gs:[24]` cpu index + `gs:[32]` current PID); `CPU_DATA` mutex remains for stats
- [x] **Command line** — real `key=value` parser; QEMU fw_cfg cmdline when present, else a documented default
- [ ] **x86_64 / aarch64 / riscv64** — x86_64 boots; other arches compile with shims

### Stub / unused
- [x] **NMI** — IDT handler logs and returns
- [x] **MCE** — IDT handler logs and panics
- [ ] **ACPI AML / DSDT** — no interpreter; `acpi::shutdown` is QEMU port writes
- [ ] **UEFI runtime** — `get_time` hardcoded; `get_variable` → `None`
- [ ] **Kernel modules** — `modules.rs` / `kpm.rs` are name registries, not `.ko` loaders
- [ ] **IOMMU** — DMAR parse; `enable_translation` never called
- [ ] **Live patch** — writes a jmp over a symbol; no W^X, no activeness wait
- [ ] **Nested IRQ / TPR** — not used from ISRs

### Perfect-OS next steps
1. ~~Per-CPU TSS + GS base + exception stacks before SMP is useful.~~ **Done** (I1) — each CPU has TSS + IST stacks; GS points at `CpuLocal`.
2. ~~Wire MADT IOAPIC address and interrupt source overrides.~~ **Done** — `smp::apply_madt_ioapic` after ACPI parse.
3. ~~Add NMI and MCE handlers that log and recover or panic cleanly.~~ **Done** — NMI logs; MCE panics.
4. ~~Pass the real bootloader command line into `cmdline::parse`.~~ **Done** when fw_cfg has one; otherwise the documented default.
5. ~~AP runs a Ring 3 task.~~ **Done** (I3) — affinity-pinned `getcpu` ELF on CPU 1.

---

## 2. Memory Management

**Grade: Wired (58%)** · `memory.rs`, `allocator.rs`, `vmm.rs`, `mmap.rs`, `slab.rs`, `page_cache.rs`

The real MMU work is in **`vmm.rs`**, not a buddy allocator.

### Live / Wired
- [x] Bootloader mmap bump-pointer frame allocator (no free)
- [x] Linked-list kernel heap (eager-mapped)
- [x] Slab for ≤4 KiB objects (backed by heap, not physical buddy)
- [x] 4-level page-table walk (`map_page_in_table`)
- [x] Demand paging: `#PF` → allocate + map + zero (`handle_demand_fault`)
- [x] CoW: `AddressSpace::fork` marks PTEs read-only; write fault copies (**Gate H1** / A4)
- [x] Anonymous `mmap` (lazy unless `MAP_POPULATE`)
- [x] File-backed `mmap`: one-page fault-in from page cache / VFS; siblings stay unmapped (**Gate H2**)
- [x] User ASLR offsets (xorshift; kernel image not randomized)
- [x] **Buddy allocator** on the VMM frame pool (orders 0–10, split/merge; 32 MiB prefill)
- [x] **OOM on alloc** — empty buddy pool shrinks the page cache then `trigger_oom` (**Gate H4**)
- [x] **Kernel stack guards** — `alloc_guarded_stack` used after VMM is ready; guard page is unmapped (**Gate H4**)

### Unused / stub
- [ ] **Buddy over all RAM** — bootloader bump-pointer still has no free; only the VMM pool is buddy-backed
- [x] **File-backed mmap** — lazy VMA; `#PF` fills one 4 KiB page from the page cache / VFS (H2)
- [ ] **Page reclamation / LRU eviction** — `page_cache::shrink` is called before OOM; no anonymous LRU
- [ ] **Swap** — `page_out` ignores phys addr; never called from reclaim
- [x] **OOM killer** — scoring + `trigger_oom` from failed `allocate_physical_frame` (H4)
- [ ] **NUMA** — SRAT parse; `allocate_node` returns an id, not memory
- [ ] **KSM** — hashes phys addr as a virt pointer; merge is a counter
- [ ] **THP** — `allocate_contiguous_frames` returns `None` above buddy max order
- [x] **Kernel stack guards** — `stack_guard::alloc_guarded_stack` on the live path (H4)
- [ ] **KASLR** for the kernel image
- [ ] **Memory cgroups** enforcement

### Perfect-OS next steps
1. Physical buddy (or equivalent) with free, orders, and a real page inventory.
2. ~~Call OOM from failed alloc; reclaim before OOM.~~ **Done** (H4) — shrink then `trigger_oom`.
3. ~~File-backed VMAs that fault from the block layer through the page cache.~~ **Done** (H2).
4. ~~Guard pages on every kernel stack; W^X on all user maps.~~ Guarded stacks after VMM init (H4); W^X was E1.

---

## 3. Process & Scheduling

**Grade: Wired (78%)** · `scheduler.rs`, `process.rs`, `context.rs`, `usermode.rs`, `signals.rs`, `user_task.rs`

Kernel threads can switch RIP. Gate B2 enters Ring 3 for a one-shot hello. Gate B3–B6 then run **scheduled** Ring 3 tasks with their own CR3: `execve`+`waitpid`, `fork`+child, SIGKILL/SIGSEGV/PTY SIGINT, and `/bin/sh` on a PTY.

### Wired (data plane)
- [x] Process table, PIDs, parent/child, reparent to init, zombie bookkeeping
- [x] CFS vruntime / weight table in software
- [x] Timer ISR sets `NEED_RESCHED`
- [x] `sys_fork` CoW-forks VMM and clones a context with `rax = 0`
- [x] `sys_execve` can map ELF into an address space + stack/auxv
- [x] `usermode.rs`: `wrmsr` STAR/LSTAR/SFMASK/EFER.SCE + `syscall`/`sysretq` stubs
- [x] Signal frame builder (`signals.rs`)
- [x] **Full kernel context switch** — GPRs, RIP, CS/SS, RFLAGS, FXSAVE/FXRSTOR; boot self-test switches two kernel threads
- [x] **Idle thread** PID 0 `STI; HLT`; executor `yield_to_idle()` when the desktop has no work
- [x] **`KERNEL_GS_BASE` / `GS_BASE`** programmed for `syscall` `swapgs`; Ring 3 `syscall` round-trip proven on hello
- [x] **`deliver_signals`** called from `deferred_schedule`; SIGKILL/default-terminate actually retire the task
- [x] **Gate B2 one-shot Ring 3** — static hello ELF mapped into current CR3, `iretq`, `sys_write` to serial, `sys_exit` returns to kernel
- [x] **Gate B3–B6 scheduled Ring 3** — `execve`+`waitpid`, `fork` child, SIGKILL / SIGSEGV / PTY SIGINT, `/bin/sh` on a PTY
- [x] **Gate B7 live `sigreturn`** — `rt_sigaction` handler runs in Ring 3, `ret`s into trampoline, `rt_sigreturn` resumes `pause`

### Stub (control plane)
- [x] **Scheduled Ring 3** — `user_task::spawn_elf` builds a CFS task with its own CR3; boot path `execve`s `/bin/hello`, then `/bin/sh` on a PTY
- [x] **Preemption of userspace** — naked APIC-timer stub saves GPRs + FXSAVE; spinner `rbx` magic survives (`GATE_I2 irq gprs`)
- [x] **`wait4` blocking** — parks a Ring 3 parent until the child exits
- [x] **SMP load balance / affinity** — `set_cpu_affinity` is honoured by CFS; APs run tasks pinned with `enqueue_on_cpu` (`GATE_I3 ap ring3`). Global CFS `current` remains BSP-shaped for the desktop.
- [ ] **sched_ext / eBPF** — not eBPF
- [ ] **Threads** — metadata only; `thread_join` → EAGAIN

### Perfect-OS next steps (this is the critical path)
1. ~~Save/restore full `iretq` frame + `fxsave`/`fxrstor` in `context.rs`.~~ (kernel threads done)
2. ~~Per-CPU TSS RSP0 + `GS_BASE` / `KERNEL_GS_BASE`.~~ **Done** (I1) — each CPU has a TSS; GS points at `CpuLocal`.
3. ~~First userspace: map a static `hello` ELF, `iretq`, `sys_write` to serial, `sys_exit`.~~ (Gate B2)
4. Then `fork` + `execve` + `waitpid` + SIGCHLD + Ctrl+C via PTY. **B3–B7 wired on the boot path.** Isolated GUI clients (Gate F) have a first SHM client; the in-kernel terminal remains.

---

## 4. Filesystem & Storage

**Grade: Wired (72%)** · `vfs.rs`, `block.rs`, `virtio_blk.rs`, `ext4.rs`, `fat32.rs`, `persist.rs`, `partition.rs`, `page_cache.rs`, `ahci.rs`, `nvme.rs`

### Live
- [x] In-memory VFS (FHS tree, path walk, fds, metadata)
- [x] procfs / sysfs / devfs / tmpfs (virtual)
- [x] ATA PIO IDENTIFY/READ/WRITE (`block.rs`, ports `0x1F0` / `0x170`)
- [x] VirtIO-blk: PCI, virtqueue, guest-physical DMA + bounce buffer, sector R/W (QEMU)
- [x] ext4: superblock at LBA 2, inode table, extents, read/write via block layer
- [x] FAT32: BPB, FAT, clusters, LFN
- [x] GPT/MBR parse (`partition.rs`)
- [x] `persist.rs` blob store on VirtIO-blk (`KNOXPERSIST`)
- [x] **Gate C1 persist roundtrip** — write `/var/lib/knoxos/gate_c1`, unlink from RAM VFS, restore from VirtIO-blk
- [x] **Gate C2 persist WAL** — committed journal record survives simulated crash; uncommitted is discarded (`GATE_C2 journal recovered`)
- [x] **Gate C3 page cache writeback** — dirty 4 KiB page overlays backing file at offset; sibling pages survive (`GATE_C3 writeback complete`)
- [x] **Gate C4 AHCI DMA** — command list + PRDT write then read a marker sector (`GATE_C4 ahci dma complete`)
- [x] **Gate C5 NVMe DMA** — admin + I/O queues; PRP1 bounce write/read round-trip (`GATE_C5 nvme dma complete`)
- [x] **Gate C6 VFS root persist** — `/etc` (and the rest of the RAM namespace except virtual FS and `/bin`) round-trips through VirtIO-blk (`GATE_C6 vfs persist`)
- [x] 4 MB ramdisk always created

### Wired / partial
- [x] Page cache is an in-RAM `BTreeMap`; write-back overlays dirty pages via `pwrite_file` (C3)
- [ ] Mount table: ext4 really mounts; several FS types only `mkdir`
- [ ] xattr / flock — in-kernel maps, not on-disk
- [ ] NTFS — parses MFT from a provided buffer; **never calls the block layer**
- [x] `fsync`/`fdatasync` — flush dirty page-cache pages, re-persist the fd's VFS file through the WAL, and issue VirtIO-blk FLUSH

### Stub
- [x] **AHCI** — HBA reset, port start, command-list + PRDT DMA read/write (C4); IDENTIFY parsed
- [x] **NVMe** — controller enable, admin/I/O queues, PRP bounce DMA read/write (C5)
- [ ] Btrfs / ZFS / XFS / exFAT / CIFS — in-memory or AHCI/NVMe fallback
- [ ] JBD2 journal replay — `replayed = 0` (persist WAL is live; ext4 JBD2 is not)
- [ ] OverlayFS / FUSE / NFS — not registered with VFS I/O
- [x] inotify — `emit_event` from VFS write/unlink; watch can read (**Gate H3**). fanotify still unused.
- [ ] Quotas — counters, not enforced

### Perfect-OS next steps
1. Make ext4 (or a single production FS) the root on VirtIO-blk by default, not RAM. Persist snapshot covers the RAM namespace (C6); virtual FS still rebuilt at boot.
2. ~~Journal commit + crash recovery that is tested by killing QEMU mid-write.~~ **Done** for the persist blob WAL (in-boot: commit journal, skip checkpoint, restore replays). JBD2 still missing.
3. ~~Complete VirtIO-blk DMA (guest-physical, not heap pointers).~~ **Done** — contiguous buddy frames + bounce buffer. ~~AHCI DMA still missing.~~ **C4 done.** ~~NVMe DMA still missing.~~ **C5 done.**
4. ~~Hook inotify on VFS mutate.~~ **Done** (H3). Page-cache writeback (C3) overlays dirty pages.

---

## 5. Networking Stack

**Grade: Wired (64%)** · `net.rs`, `netint.rs`, `virtio_net.rs`, `e1000.rs`, `rtl8139.rs`, `dhcp.rs`, `dns.rs`, `net_production.rs`

### Wired (builders / closest-to-real NICs)
- [x] Ethernet / ARP / IPv4 / UDP / TCP **header** construction
- [x] `e1000`: reset, MAC, RX/TX rings, TDT/RDT poll (heap-as-DMA caveat)
- [x] `rtl8139`: PIO TX, RX ring poll
- [x] DHCP DISCOVER/REQUEST **can be sent**; OFFER/ACK parse
- [x] DNS A-query **can be sent**; response parse
- [x] NTP packet build + UDP send helper
- [x] **Loopback** — UDP `sendto` and TCP `connect`/`send` copy into the peer `recv_buf` on `127.0.0.1`; Gate D1 Ring 3 demo
- [x] **Gate D2 VirtIO-net rings** — guest-physical avail/used; TX DHCP DISCOVER; RX UDP reply from QEMU user-net (`GATE_D2 virtio-net complete`)
- [x] **Gate D3 DHCP apply** — ACK writes `eth0` IPv4, mask, gateway (`GATE_D3 dhcp applied`)
- [x] **Gate D4 DNS + TCP** — UDP DNS parse; SYN/ACK + RTO; HTTP GET (`GATE_D4 dns tcp complete`)
- [x] **Gate D5 TCP CUBIC** — cwnd grows on ACK, β=0.7 on loss, send limited by window (`GATE_D5 cubic window`)

### Stub (the path apps use)
- [x] **Socket send off-box** — non-loopback `send`/`sendto` goes through `netint` + VirtIO-net
- [x] **TCP connect off-box** — SYN sent; SYN-ACK completes; RTO retransmits
- [x] **TCP CUBIC / window** — `CongestionState` on each TCB; send clamped to cwnd; advertised `rcv_wnd`; loss cuts cwnd (D5)
- [x] **VirtIO-net TX** — writes avail ring with guest-physical bounce; waits for used
- [x] **VirtIO-net RX** — walks the used ring and re-arms buffers
- [x] **DHCP apply** — writes interface IP + DNS + default route
- [ ] IPv6 echo — builds packet, logs, no TX
- [ ] TLS 1.3 — types; GCM/X.509 stub; handshake flagged done after one recv
- [ ] Wi-Fi / WPA3 / bridge / VLAN / NAT — in-memory

### Perfect-OS next steps
1. ~~Loopback: `send` → peer `recv_buf`.~~ **D1 done** — kernel self-test + Ring 3 `sendto`/`recvfrom`.
2. ~~Fix VirtIO-net avail/used rings; TX one UDP ping.~~ **D2 done** — DHCP DISCOVER TX + UDP reply RX vs QEMU user-net.
3. ~~Wire `Socket` → `netint::send_*`; ARP then DHCP that **writes the interface IP**.~~ **D3 done.**
4. ~~TCP: SYN/ACK, seq/ack, RTO, then CUBIC.~~ **SYN/ACK + RTO + HTTP GET done (D4). CUBIC cwnd done (D5).**

---

## 6. Device Drivers

**Grade: Wired (40%)** · `pci.rs`, `usb.rs`, `ahci.rs`, `nvme.rs`, `i915.rs`, …

### Live
- [x] PCI config `0xCF8`/`0xCFC`, BDF scan, BAR decode
- [x] PS/2 keyboard + mouse (interrupt-driven)
- [x] UART TX
- [x] VirtIO-blk
- [x] AHCI command-list DMA (C4)
- [x] NVMe admin/I/O queue DMA (C5)
- [x] VGA/BGA framebuffer from bootloader
- [x] RTC, HPET registers (timer path uses APIC + PIT)

### Partial MMIO (not a full driver)
- [ ] XHCI — rings/doorbells; timeouts “continue anyway”; fallback QEMU descriptors
- [ ] i915 / AMDGPU — register read/write; no modeset/firmware/GTT
- [ ] HDA — CORB/RIRB; DMA buffer is a heap pointer
- [ ] TPM TIS FIFO at `0xFED4_0000` — incomplete locality handshake

### Stub
- [ ] VirtIO-GPU — **no virtqueue**; fake 1920×1080
- [ ] DRM — in-memory CRTC/GEM
- [ ] USB HID / hub / MSC / audio — parsers or flags, no URB completion loop
- [ ] e1000/rtl8139 — see networking; not used by `Socket::send`
- [ ] Touchscreen, trackpad, gamepad, GPIO, I2C, Thunderbolt, Wacom, UVC
- [ ] Broadcom Wi-Fi, AX211, USB Ethernet

### Perfect-OS next steps
1. One storage, one net, one GPU scanout, one USB HID — **finished**, not 40 started.
2. Storage: VirtIO-blk production-quality; AHCI DMA live (C4); NVMe DMA live (C5).
3. Net: VirtIO-net rings + DHCP + DNS/TCP live (D2–D4); e1000 still unused by sockets.
4. Display: VirtIO-GPU 2D resource + scanout **or** keep software FB until Ring 3 exists.
5. Input: USB HID interrupt-IN so the desktop works without PS/2.

---

## 7. GUI & Desktop Environment

**Grade: Live (82%)** · `gui/` 161 files, ~91,700 lines

The compositor is the most complete **product** in the tree. It is not a Unix display server.

### Live
- [x] 32bpp BGRA software FB, double-buffer, ≤16 damage rects, `present_rect`
- [x] Window manager: z-order, focus, min/max, snap, workspaces, chrome, shadows, animations
- [x] Desktop, wallpaper, icons, context menu, pill taskbar, tray, start menu, Alt-Tab, hot corners
- [x] TTF raster, glyph cache, subpixel/grayscale, CJK, RTL runs, KnoxUI widgets, immediate-mode UI
- [x] Mouse (click/drag/resize), keyboard focus, shortcuts, theme (dark/light), notifications
- [x] Lock/login screens, screenshot, in-process DnD, blur, night light, on-screen keyboard
- [x] Clipboard used by terminal/explorer/input (`clipboard.rs`)
- [x] **17 real in-process apps**: Terminal, Files, Browser (tag HTML), AI Assistant, Editor, Settings, Task Manager, Calculator, Image Viewer, Log Viewer, Calendar, Disk Utility, Bluetooth manager, Software Updater, Software Center, Archive Manager, Setup Wizard

### Unused / stub
- [ ] **Wayland** — object model plus a live `/dev/wl0` present ioctl; one Ring 3 SHM client (F1–F2); no bind/listen Unix socket
- [ ] **GPU compositor** — `gpu_compositor.rs` never invoked; desktop software-blits
- [x] **Client isolation (demo)** — Gate F Ring 3 clients paint SHM (F1–F4); remaining in-kernel apps still `WindowContentType`
- [ ] Hardware cursor probed (`hw_cursor::init`); desktop still draws a software cursor
- [ ] VSync module unused; pacing is TSC ~60 FPS
- [x] Start-menu **Paint / Video Player / Webamp / Doom / ClassiCube / Quake III** removed; Paint is a Ring 3 SHM client (F4)
- [ ] HDR, VRR, TrueType bytecode hinting, full IME framework

### Perfect-OS next steps
1. Freeze new in-kernel apps. Every new app should be a Ring 3 binary.
2. After `execve` works: Wayland (or a tiny custom protocol) over Unix sockets, SHM buffers, kernel compositor scanout.
3. Then VirtIO-GPU scanout + hardware cursor. Not before userspace exists.

---

## 8. Shell & Terminal

**Grade: Live (82%)** · `shell/`, `terminal/`, `pty.rs`, `tty.rs`

### Live
- [x] Parser: pipes, redirects, background, quoting
- [x] Scripting: variables, conditionals, loops, glob (`*`, `?`, `[...]`), `export` / PATH
- [x] 60+ builtins (files, text, net, packages, system)
- [x] Terminal grid, split panes, history, completion, highlighting, sixel
- [x] PTY Unix98 pairs, 4 KB rings, termios; ISIG → SIGINT
- [x] Kernel `signals::kill` for Ctrl+C **to kernel tasks**

### Remaining (userspace-shaped)
- [x] Shell as `/bin/sh` in Ring 3 attached to a PTY slave
- [x] Live `sigreturn` for a custom SIGINT handler (B7)
- [ ] Full VT100/xterm-256
- [ ] Here-docs, functions, `~/.profile` once a real home exists on disk
- [ ] Job control against **processes**, not kernel windows

---

## 9. Security & Cryptography

**Grade: Wired (42%)** · `crypto.rs`, `tls.rs`, `seccomp.rs`, `random.rs`, `ssp.rs`

### Wired
- [x] AES-128/256 block + CBC (software)
- [x] SHA-256 / SHA-512
- [x] ChaCha20 block function + RFC 8439 known-answer (E2)
- [x] dm-crypt AES-XTS / LUKS **structures** + sector crypto helpers
- [x] `seccomp::check_syscall` denies listed syscalls with EPERM (E3)
- [x] Entropy pool + RDRAND mix; **ChaCha20 CSPRNG** for `getrandom` (E2)
- [x] `__stack_chk_fail` + canary (`ssp.rs`); compiler SSP not shown enabled
- [x] Unix permission bits on VFS inodes
- [x] W^X + NX on user maps; `mprotect` RWX fails; ASLR randomizes (E1)
- [x] CapNetBindService on `bind`; unprivileged `:80` fails; dropped on exec (E4)

### Stub / unused
- [ ] AES-GCM, Poly1305, TLS 1.3 key schedule, X.509 chain verify
- [ ] RSA / ECDSA / Ed25519 / X25519 production-quality
- [ ] SELinux / AppArmor / Landlock — not hooked in VFS/net
- [ ] Kernel lockdown, secure wipe on free, IMA

### Perfect-OS next steps
1. ~~Replace xorshift userspace RNG with ChaCha20 from a real entropy pool (RDRAND + jitter + timings).~~ **E2 done.**
2. ~~W^X + NX on user maps the day Ring 3 boots.~~ **E1 done.**
3. ~~Deny-by-default seccomp for desktop apps; capabilities on `execve`.~~ Seccomp EPERM + CapNetBindService live; desktop default-deny still later.
4. MAC only after VFS disk is real (labels must persist).

---

## 10. System Services

**Grade: Wired (28%)** · `init.rs`, `service_manager.rs`, `dbus.rs`, `cron.rs`, `pam.rs`

### Wired
- [x] Init searches standard paths; can map a built-in ELF blob
- [x] Service manager `boot()` from `main.rs`; dependency-ish unit states
- [x] In-memory D-Bus routing (`send_message`)
- [x] Cron field parser + `tick` → execute builtin/ELF
- [x] dmesg ring, syslog structures
- [x] PAM salted SHA-512 verify; session/seat structs
- [x] QEMU shutdown ports

### Stub
- [ ] Init never `iretq`s to `/sbin/init`
- [ ] No crash restart / watchdog / cgroup limits on units
- [ ] D-Bus is not `AF_UNIX` `/run/dbus/system_bus_socket`
- [ ] Cron needs a guaranteed timer caller
- [ ] udev-style hotplug → `/dev`
- [ ] Suspend/hibernate, logind, NetworkManager (activate logs “Starting DHCP”)
- [ ] PulseAudio/PipeWire mixing to HDA

---

## 11. Virtualization & Containers

**Grade: Stub (12%)** · `kvm.rs`, `vmx.rs`, `container.rs`, `namespaces.rs`

**Do not expand this until Ring 3 and namespaces work for ordinary processes.**

- [x] VMX constants; `vmxon`/`vmlaunch`/`vmresume` `asm` in `vmx.rs`
- [ ] `kvm::start_vm` sets `Running` and **does not** `vmlaunch`
- [ ] EPT, virtio device emulation for guests
- [ ] `container.rs` — OCI structs; start is comments (`fork`, `unshare`, `pivot_root`)
- [ ] Namespace maps unused by clone/exec
- [ ] OverlayFS not in VFS
- [ ] cgroup v2 **enforcement** (CPU/memory/IO)

Note: `vmm.rs` is **process page tables**, not a hypervisor.

---

## 12. AI/ML Integration

**Grade: Wired (22%)** · `gguf.rs`, `llm.rs`, `onnx.rs`, `ai.rs`

Nice-to-have. Not on the path to a perfect OS.

- [x] GGUF v3 parse, Q4_0/Q8_0 dequant
- [x] Naive CPU generate / ONNX graph ops (add/mul/matmul/relu/softmax)
- [x] AI syscalls `0x1000+`
- [ ] Real transformer (attention, KV cache, production tokenizer vocab)
- [ ] `gpu_matmul.rs` only logs; `available: false`
- [ ] GUI assistant is placeholder text unless a model is loaded

---

## 13. Binary Compatibility & Runtime

**Grade: Wired (45%)** · `elf.rs`, `dynlink.rs`, `syscall/mod.rs`, `vdso.rs`

Linux **syscall numbers 0–451** are named and mostly dispatched. That is **not** 95.8% compatibility. Many arms return `Ok(0)` or ignore flags (`mprotect` “not enforced on our flat memory model”).

### Wired
- [x] ELF64 validation, PHDRs
- [x] `vmm::load_elf_into_address_space` maps segments
- [x] Relocation helpers in `dynlink.rs`
- [x] vDSO page contents mapped for `/init` only
- [x] Kernel `memcpy`/`printf`-family in `posix_libc.rs` (kernel address space, not libc.so)
- [x] **Gate B2 static hello** — `iretq` to Ring 3, `sys_write` / `sys_exit` (not via `execve`)

### Stub
- [x] **`execve` starts the binary as a scheduled process**
- [ ] Dynamic linker as a userspace `ld.so` (DT_NEEDED, lazy PLT, RELRO)
- [ ] `dlopen` that maps `.so` via VMM (`ldknoxos.rs` fake base)
- [ ] TLS (initial-exec / general-dynamic)
- [ ] glibc/musl ABI for Ring 3
- [x] Signal trampoline + `rt_sigreturn` (custom SIGINT handler returns to `pause`)
- [x] Working `fork` child that runs

### Perfect-OS next steps
~~Ship **static musl hello** first.~~ Gate B2 hello is an in-kernel generated static ELF. ~~B6 PTY + `/bin/sh`.~~ ~~Live `sigreturn`.~~ Isolated GUI clients F1–F4 live. Next: dynamic linking.

---

## 14. Internationalization & Fonts

**Grade: Live (68%)**

### Live
- [x] UTF-8, graphemes, CJK, RTL class/runs
- [x] Embedded bitmaps + TTF + LRU glyph cache + ClearType/grayscale
- [x] Date/time format helpers, timezone tables

### Partial
- [ ] Font fallback chain
- [ ] Real GSUB/GPOS / HarfBuzz-class shaping (Latin ligatures hardcoded)
- [ ] Color emoji
- [ ] `.mo` parser exists; runtime locale switch incomplete

---

## 15. Build System & Tooling

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

## 16. Testing & Quality

**Grade: Wired (46%)**

### Exists
- [x] `#[test_case]` framework + QEMU exit ports
- [x] Real tests: VFS read/write, allocator Box/Vec, some path tests (~subset of 103 `#[test_case]`)
- [x] `tests/run_integration.sh` waits for serial `Desktop Environment ready` plus C1–C6 / D1–D5 / E1–E4 / F1–F4 / H1–H4 / I1–I3 markers

### Harmful
- [x] **`assert!(true)` tests removed** — widgets, VFS stress, DNS, TCP flags, creds, buddy, path normalize are real assertions
- [ ] No `#[test]` (expected for `no_std`, but host-side tests could exist)
- [ ] 300+ `unsafe` blocks without a systematic SAFETY audit
- [ ] No Miri, no fuzz of ELF/ext4/packet parsers, no coverage

### Perfect-OS next steps
1. Delete or rewrite every `assert!(true)`.
2. Host unit tests for parsers (ELF, ext4 superblock, TCP checksum) without QEMU.
3. QEMU tests: boot marker, virtio-blk persist across reboot, later `hello` in Ring 3.
4. SAFETY comments on every `unsafe`; clippy `undocumented_unsafe_blocks` as a gate.

---

## 17. Documentation

**Grade: Wired (42%)**

| Artifact | Status |
|----------|--------|
| `status.md` | This file (updated 2026-09-25) |
| `README.md` | Present — honesty paragraph, `./run.sh`, architecture sketch |
| `LICENSE` | MIT |
| `BUILDING.md` / `CONTRIBUTING.md` | Present |
| Architecture / syscall / driver / GUI guides | Missing |
| `cargo doc` published | Missing |
| Changelog | Missing |

### Perfect-OS next steps (cheap, high leverage)
1. MIT `LICENSE`.
2. `README.md`: what KnoxOS is, honesty paragraph, `./run.sh`, screenshots, link here.
3. Boot flow diagram: bootloader → `kernel_main` → compositor loop.
4. Syscall table: **behavior** (`Ok(0)` vs implemented vs ENOSYS), not just numbers.

---

## 18. CI/CD & Release

**Grade: Wired (30%)**

### Live locally
- [x] `make ci-all` (fmt, clippy, size)

### Missing
- [x] **`.github/workflows`** — fmt, clippy, size, QEMU BIOS boot, docs present
- [ ] Cross-compile + QEMU boot on every PR (x86_64 QEMU only so far)
- [ ] `cargo-audit`, license/SPDX check
- [ ] Tagged release → ISO → checksums → signatures
- [ ] Nightly main-branch images

---

## Path to a perfect OS

Stop adding Phase 34 modules. **Wire, delete, or feature-gate.** Sequence is dependency order.

### Gate A — Honest kernel (4–8 weeks)

Make the scheduler and memory manager true.

| ID | Task | Done when |
|----|------|-----------|
| A1 | Full context switch (GPRs, RIP, CS/SS, RFLAGS, FXSAVE) | **Done** — boot self-test switches two kernel threads with different RIP |
| A2 | Idle thread `HLT` on BSP | **Done** — PID 0 `STI; HLT`; executor yields when idle |
| A3 | Physical page free + buddy (or equivalent) | **Done** for the 32 MiB VMM pool (buddy orders 0–10); bootloader bump-pointer still has no free |
| A4 | `#PF` + CoW tested | **Done** — forked address space write-fault copies; parent bytes unchanged (`GATE_H1 cow fault`) |
| A5 | Kill `assert!(true)` tests | **Done** — those tests are rewritten or deleted |

### Gate B — First userspace (6–12 weeks)

This **is** becoming an OS.

| ID | Task | Done when |
|----|------|-----------|
| B1 | TSS + `KERNEL_GS_BASE` + syscall entry that cannot fault on `swapgs` | **Done** — hello `syscall`/`sysretq` round-trip (write + exit) |
| B2 | Map static ELF, `iretq`, `sys_write` serial, `sys_exit` | **Done** — QEMU serial prints `hello from userspace`, then desktop starts |
| B3 | `execve` + `waitpid` | **Done** — boot spawns `execve("/bin/hello")`, hello prints, parent reaps (`GATE_B3 wait complete`) |
| B4 | `fork` CoW + child runs | **Done** — child writes `fork child ran`, parent `wait4`s (`GATE_B4 fork complete`) |
| B5 | Signals: SIGKILL, SIGSEGV, SIGINT from PTY | **Done** — parked `pause` + SIGKILL; null-deref SIGSEGV; PTY Ctrl+C (`GATE_B5 signals complete`) |
| B6 | PTY + `/bin/sh` (even a tiny static shell) | **Done** — boot spawns `/bin/sh` on a kernel PTY; serial shows `$ ` then `GATE_B6 sh complete` |
| B7 | Custom handler + live `rt_sigreturn` | **Done** — SIGINT handler `ret`s into trampoline; `pause` resumes (`GATE_B7 sigreturn complete`) |
| B8 | Timer preempt of spinning Ring 3 | **Done** — APIC timer switches a `jmp $` user off the CPU (`GATE_B8 timer preempt complete`) |

### Gate C — Durable storage (4–8 weeks)

| ID | Task | Done when |
|----|------|-----------|
| C1 | Root on VirtIO-blk (ext4 or persist) | **Done** — persist blob store round-trips a file through VirtIO-blk (`GATE_C1 persist complete`). VFS namespace is still RAM. |
| C2 | Journal + `fsync` | **Done** — persist WAL round-trips a committed record after simulated crash (`GATE_C2 journal recovered`). Uncommitted discarded. |
| C3 | Page cache writeback | **Done** — dirty middle page flushes via `pwrite_file`; sibling pages survive (`GATE_C3 writeback complete`) |
| C4 | AHCI or NVMe **one** real DMA path | **Done** — AHCI command-list + PRDT write/read round-trip (`GATE_C4 ahci dma complete`). NVMe DMA also live (C5). |
| C5 | NVMe PRP DMA | **Done** — admin + I/O queues; PRP bounce write/read (`GATE_C5 nvme dma complete`) |
| C6 | RAM VFS root snapshot | **Done** — `/etc` and the rest of the namespace (except virtual FS and `/bin`) persist (`GATE_C6 vfs persist`) |

### Gate D — Packets (4–8 weeks)

| ID | Task | Done when |
|----|------|-----------|
| D1 | Loopback sockets | **Done** — kernel UDP/TCP self-test + Ring 3 `sendto`/`recvfrom` (`GATE_D1 loopback complete`) |
| D2 | VirtIO-net avail/used correct | **Done** — DHCP DISCOVER TX + UDP reply RX vs QEMU user-net (`GATE_D2 virtio-net complete`) |
| D3 | DHCP applies IP + default route | **Done** — ACK writes `eth0` IPv4 + gateway (`GATE_D3 dhcp applied`) |
| D4 | DNS + TCP connect/retransmit | **Done** — UDP DNS + SYN/ACK + RTO + HTTP GET (`GATE_D4 dns tcp complete`) |
| D5 | TCP CUBIC / window | **Done** — cwnd grows on ACK, β=0.7 on loss, send clamped (`GATE_D5 cubic window`) |

### Gate E — Enforcement (3–6 weeks)

| ID | Task | Done when |
|----|------|-----------|
| E1 | W^X + NX + ASLR on user maps | **Done** — `mmap`/`mprotect` RWX denied; NX on data; ASLR bases differ (`GATE_E1 wx aslr complete`) |
| E2 | ChaCha20 CSPRNG `getrandom` | **Done** — RFC 8439 KAT + `getrandom` (`GATE_E2 csprng complete`) |
| E3 | Seccomp actually denies | **Done** — listed syscall returns EPERM (`GATE_E3 seccomp deny`) |
| E4 | Caps on exec | **Done** — unprivileged bind `:80` fails; dropped on exec (`GATE_E4 caps exec`) |

### Gate F — Real desktop (ongoing, after B)

| ID | Task | Done when |
|----|------|-----------|
| F1 | One Wayland client (or custom protocol) out of process | **Done** — Ring 3 `/dev/wl0` present (`GATE_F1 client isolated`). |
| F2 | SHM / DMA-BUF to compositor | **Done** — client mmap copied into a kernel SHM pool; compositor round-trips magic pixels (`GATE_F2 shm commit`) |
| F3 | VirtIO-GPU scanout or keep FB but clients isolated | **Done** — FB scanout of SHM; Ring 3 terminal is Empty+SHM, not `WindowContentType::Terminal` (`GATE_F3 terminal isolated`). Interactive desktop terminal still in-kernel for PTY I/O. |
| F4 | Remove Empty launcher stubs or implement them as userspace | **Done** — Doom/Quake/Webamp stubs removed; Paint is a Ring 3 SHM client (`GATE_F4 launcher userspace`) |

### Gate H — Memory, VFS, core leftovers (after A–F)

| ID | Task | Done when |
|----|------|-----------|
| H1 | CoW `#PF` | **Done** — forked space write-fault copies; parent unchanged (`GATE_H1 cow fault`) |
| H2 | File-backed mmap fault-in | **Done** — mmap does not copy the whole file; one page faults from VFS/page cache (`GATE_H2 mmap fault`) |
| H3 | inotify from VFS | **Done** — write/unlink emit events a watch can read (`GATE_H3 inotify`) |
| H4 | Guarded stacks + OOM on alloc | **Done** — unmapped guard page allocated; empty buddy pool calls `trigger_oom` (`GATE_H4 guard oom`) |

### Gate I — Per-CPU + SMP + IRQ context (after H)

| ID | Task | Done when |
|----|------|-----------|
| I1 | Per-CPU TSS + GS `CpuLocal`; AP online | **Done** — BSP `str` matches TSS[0]; GS_BASE → `CpuLocal[0]`; AP loads a distinct TSS (`GATE_I1 smp online`) |
| I2 | IRQ saves GPRs + FXSAVE | **Done** — spinner `mov rbx, magic; jmp $`; after timer preempt, saved `rbx` and FXSAVE area are live (`GATE_I2 irq gprs`) |
| I3 | AP runs Ring 3 | **Done** — `getcpu` ELF affinity-pinned to CPU 1 syscalls on the AP (`GATE_I3 ap ring3`) |

### Gate G — Quality bar (parallel from day one)

| ID | Task | Done when |
|----|------|-----------|
| G1 | `README.md` + `LICENSE` | **Done** |
| G2 | GitHub Actions: fmt, clippy, size, QEMU boot | **Workflow present** (`.github/workflows/ci.yml`) |
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | Default kernel compiles **Live** code only |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | No silent `Ok(0)` for security-sensitive calls |
| G5 | `unsafe` SAFETY comments + size budget | Clippy gate |

### Explicitly later (after Gates A–E)

KVM, containers/k8s, Btrfs/ZFS, Wi-Fi, TLS 1.3, GPU compute, AI inference, package formats, Chromium/Vivaldi, aarch64/riscv64 **runtime**, Thunderbolt, TPM measured boot.

Shipping these before Gate B **increases** the distance to a perfect OS.

---

## Anti-goals

Do not:

- Add another `pub mod` that `serial_println`s and returns `Ok(())`.
- Count syscall **numbers** as compatibility.
- Mark GUI 100% while apps are kernel enum variants.
- Implement SCHED_DEADLINE, eBPF, or overlayfs before `iretq`.
- Keep tests that `assert!(true)`.
- Claim Nix/GitHub/README completeness when the files are absent.

---

## Progress tracker

**Production OS: ~72%** · **QEMU desktop demo: ~88%**

```
Kernel Core:        ████████████████████░░░░░  82%  Wired         ← I1–I3 per-CPU TSS + GS + AP Ring 3
Memory Mgmt:        ██████████████░░░░░░░░░░░  58%  Wired         ← H1–H4 CoW/mmap/OOM/guards
Process/Sched:      ███████████████████░░░░░░  78%  Wired         ← B3–B8 + I2 IRQ GPRs + I3 AP Ring 3
Filesystem:         ██████████████████░░░░░░░  72%  Wired         ← C1–C6 + inotify H3
Networking:         ████████████████░░░░░░░░░  64%  Wired         ← D1–D5
Device Drivers:     ██████████░░░░░░░░░░░░░░░  40%  Wired         ← AHCI + NVMe DMA
GUI & Desktop:      ████████████████████░░░░░  82%  Live          ← F1–F4 SHM clients
Shell & Terminal:   ████████████████████░░░░░  82%  Live          ← sigreturn
Security:           ██████████░░░░░░░░░░░░░░░  42%  Wired         ← E1–E4
System Services:    ███████░░░░░░░░░░░░░░░░░░  28%  Wired
Virtualization:     ███░░░░░░░░░░░░░░░░░░░░░░  12%  Stub
AI/ML:              █████░░░░░░░░░░░░░░░░░░░░  22%  Wired
Binary Compat:      ███████████░░░░░░░░░░░░░░  45%  Wired        ← execve + fork + sigreturn
i18n & Fonts:       █████████████████░░░░░░░░  68%  Live
Build System:       ███████████████████░░░░░░  75%  Live
Testing:            ████████████░░░░░░░░░░░░░  48%  Wired         ← 46/46 integration
Documentation:      ██████████░░░░░░░░░░░░░░░  42%  Wired         ← BUILDING + CONTRIBUTING
CI/CD:              ███████░░░░░░░░░░░░░░░░░░  30%  Wired
```

```

### Score change vs 2026-03-15

| Subsystem | Old | Now | Why |
|-----------|-----|-----|-----|
| Kernel Core | 95% | 82% | Per-CPU TSS + GS CpuLocal; AP INIT/SIPI online (I1); AP Ring 3 (I3); NMI/MCE; MADT IOAPIC |
| Process | 90% | 78% | Gate B3–B8 scheduled Ring 3; IRQ GPR+FPU save (I2); AP Ring 3 (I3) |
| Binary compat | 40% | 45% | Static hello + scheduled `execve`/`fork` + `/bin/sh` + `rt_sigreturn`; 452 numbers still ≠ 452 behaviors |
| Memory | — | 58% | H1 CoW #PF; H2 file-backed fault-in; H4 OOM-on-alloc + guarded stacks |
| Filesystem | 35% | 72% | VirtIO-blk + C1–C6 + inotify on VFS mutate (H3) |
| GUI | 85% | 82% | Ring 3 SHM clients (F1–F4); remaining apps still in-process |
| Shell | 90% | 82% | PTY/glob/env real; Ring 3 `/bin/sh`; live `sigreturn`; desktop terminal still in-kernel for PTY I/O |
| Docs / CI | 10% / 25% | 42% / 30% | README, LICENSE, BUILDING, CONTRIBUTING, GitHub Actions; flake still missing |
| Networking | 22% | 64% | D1 loopback; D2 VirtIO-net; D3 DHCP apply; D4 DNS+TCP; D5 CUBIC |
| Security | 15% | 42% | E1 W^X/ASLR; E2 ChaCha20; E3 seccomp EPERM; E4 CapNetBindService |
| **Overall production** | **~40%** | **~72%** | Gates B3–B8 + C1–C6 + D1–D5 + E1–E4 + F1–F4 + H1–H4 + I1–I3 on the live boot path |

Code **grew** (602 → 609 files, more Phase 30–33 modules). Production usefulness did not grow proportionally. The next updates to this file should tick **Gate** IDs, not module counts.

---

*This document is the single source of truth for KnoxOS readiness. Update a checkbox only when the **Done when** criterion is met on the live path. Unused source does not count.*
