# Do this first

← [Status index](README.md)

Gates **A–F, H–I**, syscall gates **J–BL**, **BM1–BM4** (userspace `ld.so` + musl static/dynamic), **F5** (Wayland AF_UNIX), **C7** (`/bin` persist), **G3** (feature flags), **F6** (Ring 3 calculator), and **I4** (SMP balance onto an idle AP) are done on QEMU. Adding another `GATE_xx` ENOSYS is **not** the critical path.

Work top to bottom. Skip anything marked later.

---

## 1. Quality bar (Gate G)

The linker path is done (BM1–BM4: `ld.so`, `dlopen`, static + dynamic musl). These shrink the lie in the tree.

| ID | Do | Done when | Details |
|----|----|-----------|---------|
| G3 | Feature flags `gui`, `net`, `fs-ext4`, `stub-drivers` | **Done** — default is `gui`+`net`; `stub-drivers` and `fs-ext4` stay off (`GATE_G3 features live`) | [gates/quality.md](gates/quality.md) |
| G4 | Finish the syscall audit | **Partial** — `adjtimex`/`settimeofday`/`clock_settime`/`mq_notify` return ENOSYS (`GATE_G4 enosys`); `sched_setaffinity`/`sched_getaffinity` are real. Documented no-ops remain (`madvise`, `mlock*`, `readahead`, `fadvise64`, `set_robust_list`) | same |
| G5 | `unsafe` SAFETY comments + size budget | **Partial** — 16 MB size gate holds; SAFETY comments on boot PIC/memory + SMP idle + affinity. Crate-level `undocumented_unsafe_blocks = warn` is **not** enabled: `scripts/pre-commit.sh` runs clippy `-D warnings` and would fail ~1100 existing `unsafe` blocks. Deny-all still open | same |

Convert a silent `Ok(0)` when you touch that syscall. Do not add a new lettered gate just to ENOSYS one more unused number.

---

## 2. Live-path gaps that still block a perfect OS

These are the unfinished rows in [blockers.md](blockers.md). Do them with Gate G, not instead of it.

1. **Persist `/bin`** — **Done** (C7). Virtual FS (`/dev` `/proc` `/sys` `/run`) is still rebuilt at boot. [04-filesystem.md](subsystems/04-filesystem.md)
2. **Move desktop apps to Ring 3** — Calculator is a Ring 3 SHM client (F6). Calendar/Image Viewer/Logs/Disks/Bluetooth/Updater/Software Center/Setup Wizard/Archive route to userspace SHM, not new `WindowContentType`. Interactive terminal, Files, Browser, Editor, Settings, Task Manager, and AI Assistant remain in-kernel. [07-gui.md](subsystems/07-gui.md)
3. **SMP load balance** — **Done** (I4): per-CPU CFS `current`; APs do not steal the BSP desktop; an unpinned Ring 3 task runs on an idle AP (`GATE_I4 smp balance`). [01-kernel-core.md](subsystems/01-kernel-core.md)
4. **Wayland over a Unix socket** — **Done** (F5): compositor `bind`+`listen`s `/run/wayland-0`; a Ring 3 client `connect`s and `sendto`s a SHM vaddr (`GATE_F5 wayland unix`). Not a `wl_display` protocol server. [07-gui.md](subsystems/07-gui.md)

---

## 3. After the linker (still production)

A dynamically linked musl binary runs (BM4). These are still open:

- Hardware cursor + VirtIO-GPU scanout (desktop still software-blits).
- ext4 as default root (persist WAL is live; JBD2 is not).
- USB HID URB completion (PS/2 is the live input path).
- Architecture guides + `cargo doc` ([17-docs.md](subsystems/17-docs.md)).
- Signed ISO, cross-arch QEMU in CI. `flake.nix` is present (devShell + QEMU/Make; kernel still rustup nightly) ([15-build.md](subsystems/15-build.md), [18-ci.md](subsystems/18-ci.md)).

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
