# Do this first

← [Status index](README.md)

Gates **A–F, H–I**, syscall gates **J–BL**, **BM1–BM4** (userspace `ld.so` + musl static/dynamic), **F5** (Wayland AF_UNIX), **C7–C8** (`/bin` persist + JBD2), **G3** (feature flags), **G4** (syscall audit), **F6–F8** (calculator + Files userspace + VirtIO-GPU scanout/cursor), **I4** (SMP balance onto an idle AP), and USB HID URB completion are done on QEMU. Adding another `GATE_xx` ENOSYS is **not** the critical path.

Work top to bottom. Skip anything marked later.

---

## 1. Quality bar (Gate G)

The linker path is done (BM1–BM4: `ld.so`, `dlopen`, static + dynamic musl). These shrink the lie in the tree.

| ID | Do | Done when | Details |
|----|----|-----------|---------|
| G3 | Feature flags `gui`, `net`, `fs-ext4`, `stub-drivers` | **Done** — default is `gui`+`net`+`fs-ext4`; `stub-drivers` stays off (`GATE_G3 features live`) | [gates/quality.md](gates/quality.md) |
| G4 | Finish the syscall audit | **Done** — clock/time unused numbers are ENOSYS (`GATE_G4 enosys`); `madvise`/`mlock*`/`readahead`/`fadvise64`/`set_robust_list` store real state (`GATE_G4 audit live`) | same |
| G5 | `unsafe` SAFETY comments + size budget | **Done** — 16 MB size gate holds; every `unsafe {` has a `// SAFETY:` comment; `undocumented_unsafe_blocks = deny` in `kernel/Cargo.toml` (`GATE_G5 safety comments`) | same |

Convert a silent `Ok(0)` when you touch that syscall. Do not add a new lettered gate just to ENOSYS one more unused number.

---

## 2. Live-path gaps that still block a perfect OS

These are the unfinished rows in [blockers.md](blockers.md). Do them with Gate G, not instead of it.

1. **Persist `/bin`** — **Done** (C7). Virtual FS (`/dev` `/proc` `/sys` `/run`) is still rebuilt at boot. [04-filesystem.md](subsystems/04-filesystem.md)
2. **Move desktop apps to Ring 3** — Calculator (F6) and Files (F7) are Ring 3 SHM. Settings/Task Manager/Editor/Browser/AI plus Calendar/Image Viewer/Logs/Disks/Bluetooth/Updater/Software Center/Setup Wizard/Archive also spawn userspace SHM, not new `WindowContentType`. Interactive terminal stays in-kernel for PTY I/O. [07-gui.md](subsystems/07-gui.md)
3. **SMP load balance** — **Done** (I4): per-CPU CFS `current`; APs do not steal the BSP desktop; an unpinned Ring 3 task runs on an idle AP (`GATE_I4 smp balance`). [01-kernel-core.md](subsystems/01-kernel-core.md)
4. **Wayland over a Unix socket** — **Done** (F5): compositor `bind`+`listen`s `/run/wayland-0`; a Ring 3 client `connect`s and `sendto`s a SHM vaddr (`GATE_F5 wayland unix`). Not a `wl_display` protocol server. [07-gui.md](subsystems/07-gui.md)

---

## 3. After the linker (still production)

A dynamically linked musl binary runs (BM4). VirtIO-GPU scanout + hardware cursor are live (F8). JBD2 replay (C8) and the USB HID URB completion loop are live. These are still open:

- ext4 as **default root** — **Done** (`GATE_C9 ext4 root`). `fs-ext4` is a default feature. A second virtio-blk (`mkfs.ext4`) is mounted at `/`; `/dev` `/proc` `/sys` `/run` `/tmp` stay virtual. Persist blob stays on vda (C1–C7).
- Desktop input without PS/2 — **Done** (`GATE_USB no ps2 cursor`). i8042 aux is not enabled; a boot-protocol mouse URB moves `MOUSE.x/y`.
- Secure Boot enrollment — **Done** when host tools exist (`scripts/qemu-secure-boot.sh` enrolls PK/KEK/db, signs `BOOTX64.EFI`, boots OVMF `smm=on`; `GATE_SB ovmf boot`).
- Cross-arch QEMU **boot** — **Done** for the serial marker (`GATE_ARCH aarch64 boot` / `GATE_ARCH riscv64 boot`). Full desktop runtime on those arches is still later.

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
