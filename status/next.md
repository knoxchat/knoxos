# Do this first

← [Status index](README.md)

Gates **A–F, H–I**, syscall gates **J–BL**, **BM1** (userspace `ld.so`), **BM2** (real `dlopen`), **BM3** (static musl CRT), and **C7** (`/bin` persist) are done on QEMU (`./tests/run_integration.sh` **270/270**). Adding another `GATE_xx` ENOSYS is **not** the critical path.

Work top to bottom. Skip anything marked later.

---

## 1. Dynamic linker — the critical path

Static Ring 3 works (`execve`, `fork`, `clone`/`clone3`, `/bin/sh`, `/sbin/init`). Nothing that needs `libc.so` does.

| Do | Done when | Details |
|----|-----------|---------|
| Userspace `ld.so` | **Done** — Ring 3 ELF with `DT_NEEDED` runs via `/lib/ld-knoxos.so` (lazy PLT, GNU_RELRO) (`GATE_BM1 dynlink complete`) | [13-binary-compat.md](subsystems/13-binary-compat.md) |
| Real `dlopen` | **Done** — Ring 3 `open` + VMM `mmap` of `libgate_bm.so`, dynsym `gate_hello`, call returns `0x42` (`GATE_BM2 dlopen complete`) | same |
| musl or glibc ABI | **Partial** — static musl CRT (`_start` → `__libc_start_main` → `main`) is live (`GATE_BM3 musl abi`). A dynamically linked musl binary is still open. | same |

`ld.so` (BM1), runtime `dlopen` (BM2), and static musl CRT (BM3) are live. Remaining desktop apps still stay in-kernel `WindowContentType` until a dynamically linked musl/glibc binary runs; “binary compat 99%” is still mostly syscall coverage.

---

## 2. Quality bar (Gate G, parallel)

Do these beside the linker. They shrink the lie in the tree.

| ID | Do | Done when | Details |
|----|----|-----------|---------|
| G3 | Feature flags `gui`, `net`, `fs-ext4`, `stub-drivers` | Default kernel compiles **Live** code only | [gates/quality.md](gates/quality.md) |
| G4 | Finish the syscall audit | Remaining silent `Ok(0)` become ENOSYS or real | same |
| G5 | `unsafe` SAFETY comments + size budget | Clippy gate holds | same |

Convert a silent `Ok(0)` when you touch that syscall. Do not add a new lettered gate just to ENOSYS one more unused number.

---

## 3. Live-path gaps that still block a perfect OS

These are the unfinished rows in [blockers.md](blockers.md). Do them after (or with) the linker, not instead of it.

1. **Persist `/bin`** — **Done** (C7). Virtual FS (`/dev` `/proc` `/sys` `/run`) is still rebuilt at boot. [04-filesystem.md](subsystems/04-filesystem.md)
2. **Move desktop apps to Ring 3** — freeze new in-kernel apps; each new app is a userspace binary. Interactive terminal is still in-kernel for PTY I/O. [07-gui.md](subsystems/07-gui.md)
3. **SMP load balance** — AP can run a pinned Ring 3 task (I3). Further APs still share one CFS `current` for the BSP desktop. [01-kernel-core.md](subsystems/01-kernel-core.md)
4. **Wayland over a Unix socket** — `/dev/wl0` present ioctl + one SHM client (F1–F2) is not a display server. [07-gui.md](subsystems/07-gui.md)

---

## 4. After the linker (still production)

Only when a dynamically linked binary can run:

- Hardware cursor + VirtIO-GPU scanout (desktop still software-blits).
- ext4 as default root (persist WAL is live; JBD2 is not).
- USB HID URB completion (PS/2 is the live input path).
- Architecture guides + `cargo doc` ([17-docs.md](subsystems/17-docs.md)).
- `flake.nix`, signed ISO, cross-arch QEMU in CI ([15-build.md](subsystems/15-build.md), [18-ci.md](subsystems/18-ci.md)).

---

## Do not start

These increase the distance to a perfect OS. Full list: [gates/quality.md](gates/quality.md).

KVM / `vmlaunch`, containers/k8s, Btrfs/ZFS, Wi-Fi, TLS 1.3, GPU compute, AI inference, package formats, Chromium/Vivaldi, aarch64/riscv64 **runtime**, Thunderbolt, TPM measured boot, sched_ext/eBPF, another unused `pub mod`.

Open checkboxes for those live in [remaining.md](remaining.md) so they are not forgotten — they are not next.

---

## How to land work

1. Wire an existing stub into the live path, **or** delete / feature-gate Unused modules, **or** add a test that can fail.
2. New live-path behavior needs a serial marker (`GATE_…`) and a `check_marker` line in `tests/run_integration.sh`.
3. Tick the subsystem checkbox (and a gate row, if this is a named gate) only when QEMU (or hardware) meets **Done when**.
