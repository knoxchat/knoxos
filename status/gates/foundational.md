# Gates A–I — become an OS

← [Status index](../README.md) · [Do this first](../next.md) · [Gates](README.md)

### Gate A — Honest kernel (4–8 weeks)

Make the scheduler and memory manager true.

| ID | Task | Done when |
|----|------|-----------|
| A1 | Full context switch (GPRs, RIP, CS/SS, RFLAGS, FXSAVE) | **Done** — boot self-test switches two kernel threads with different RIP |
| A2 | Idle thread `HLT` on BSP | **Done** — PID 0 `STI; HLT`; executor yields when idle |
| A3 | Physical page free + buddy (or equivalent) | **Done** for leftover usable RAM after the heap (J2); bootloader reserved regions stay out of the pool |
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
| C6 | RAM VFS root snapshot | **Done** — `/etc` and the rest of the namespace (except virtual FS) persist (`GATE_C6 vfs persist`) |
| C7 | Persist `/bin` | **Done** — `/bin/gate_c7` round-trips through VirtIO-blk (`GATE_C7 bin persist`). |
| C8 | JBD2 journal replay | **Done** — descriptor + data + commit on the ramdisk; committed block applied, torn descriptor discarded (`GATE_C8 jbd2 replay`). ext4 mount uses the same walker when `fs-ext4` is on. |
| C10 | Virtual FS mount points | **Done** — `/dev` `/proc` `/sys` `/run` directories round-trip through VirtIO-blk; generated files under them stay virtual (`GATE_C10 virtfs persist`) |

### Gate D — Packets (4–8 weeks)

| ID | Task | Done when |
|----|------|-----------|
| D1 | Loopback sockets | **Done** — kernel UDP/TCP self-test + Ring 3 `sendto`/`recvfrom` (`GATE_D1 loopback complete`) |
| D2 | VirtIO-net avail/used correct | **Done** — DHCP DISCOVER TX + UDP reply RX vs QEMU user-net (`GATE_D2 virtio-net complete`) |
| D3 | DHCP applies IP + default route | **Done** — ACK writes `eth0` IPv4 + gateway (`GATE_D3 dhcp applied`) |
| D4 | DNS + TCP connect/retransmit | **Done** — UDP DNS + SYN/ACK + RTO + HTTP GET (`GATE_D4 dns tcp complete`) |
| D5 | TCP CUBIC / window | **Done** — cwnd grows on ACK, β=0.7 on loss, send clamped (`GATE_D5 cubic window`) |
| D6 | IPv6 echo TX | **Done** — loopback `::1` Echo Request produces a sent reply (`GATE_D6 ipv6 echo`) |

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
| F3 | VirtIO-GPU scanout or keep FB but clients isolated | **Done** — FB scanout of SHM; Ring 3 terminal is Empty+SHM, not `WindowContentType::Terminal` (`GATE_F3 terminal isolated`). Start-menu Terminal is Ring 3 `openpty` (`GATE_TERM pty interactive`). |
| F4 | Remove Empty launcher stubs or implement them as userspace | **Done** — Doom/Quake/Webamp stubs removed; Paint is a Ring 3 SHM client (`GATE_F4 launcher userspace`) |
| F5 | Wayland over a Unix socket | **Done** — compositor `listen`s `/run/wayland-0`; Ring 3 `connect`+`sendto` presents SHM (`GATE_F5 wayland unix`). Object protocol: `GATE_WL display protocol`. |
| F6 | Calculator userspace | **Done** — Ring 3 SHM, not `WindowContentType::Calculator` (`GATE_F6 calculator userspace`) |
| F7 | Files userspace | **Done** — Ring 3 SHM, not `WindowContentType::FileExplorer` (`GATE_F7 files userspace`) |
| F8 | VirtIO-GPU scanout + hardware cursor | **Done** — controlq 2D resource + SET_SCANOUT + FLUSH (`GATE_F8 virtio-gpu scanout`); cursorq UPDATE_CURSOR (`GATE_F8 hw cursor`). Desktop `present_rect` also flushes through that scanout (`GATE_GPU desktop scanout`). |

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

### Gate BM — Userspace dynamic linker (after B)

| ID | Task | Done when |
|----|------|-----------|
| BM1 | Userspace `ld.so` + `DT_NEEDED` | **Done** — `/lib/ld-knoxos.so` maps `libgate_bm.so`, lazy PLT resolves `gate_hello`, GNU_RELRO (`GATE_BM1 dynlink complete`) |
| BM2 | Real `dlopen` via VMM | **Done** — Ring 3 `open` + file-backed `mmap` of `libgate_bm.so`, dynsym `gate_hello` call returns `0x42` (`GATE_BM2 dlopen complete`) |
| BM3 | Static musl CRT ABI | **Done** — `_start` → `__libc_start_main` → `main` (not the hello fixture) (`GATE_BM3 musl abi`) |
| BM4 | Dynamically linked musl | **Done** — PT_INTERP `ld-knoxos.so`, DT_NEEDED `/lib/libc.so`, crt1 `_start` → `__libc_start_main@plt` → `main` (`GATE_BM4 musl dynlink`) |
