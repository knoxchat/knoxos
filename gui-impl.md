# GUI live-path restore

The compositor is up after `./run.sh` (clock, memory widget, dock, start-menu Terminal). Clicks, drags, and keyboard do nothing. This file is the analysis plus the todo list that was implemented against that failure.

## What is actually live

- Redraw loop runs: RTC clock, memory 66%, load, process count, ~14 min uptime.
- Software / VirtIO-GPU scanout is presenting the 32bpp desktop.
- `./run.sh` attaches **only** `qemu-xhci` + `usb-kbd` + `usb-tablet`. Cocoa/GTK uses `show-cursor=off` (absolute host pointer, no grab).
- `GATE_USB hid urb` / `GATE_USB no ps2 cursor` are **software URB KATs**, not proof that QEMU Transfer Events complete.

## Root cause (why the desktop is dead)

QEMU cocoa/GTK generates **absolute** pointer events. Those go to an absolute HID tablet, **not** to the i8042 aux port.

| Path | What `./run.sh` presents | What the kernel did | Result |
|------|--------------------------|---------------------|--------|
| PS/2 mouse (IRQ12) | Default i8042 still exists | `gui::input::init_mouse()` never called (USB-only experiment) | Aux port never enabled; even relative leftovers never arrive |
| USB tablet | `-device usb-tablet,bus=xhci.0` | XHCI BAR mapped; enumeration still has timeout fallbacks; HID treats QEMU `0627:0001` as **boot relative mouse** | Reports rarely complete; when they do, 16-bit XY is parsed as `dx/dy` so the cursor is not under the host pointer |
| VirtIO tablet | **not attached** | `virtio_input` requires PCI subsystem `18` **and** an I/O BAR; modern `0x1052` is skipped (`memory BAR — skipping`) | No absolute virtqueue |
| USB / VirtIO keyboard | `usb-kbd` on XHCI; default PS/2 kbd | IRQ1 still registered; USB kbd also needs XHCI URBs | Keys may work on PS/2; they do not if cocoa feeds USB only |
| `virtio_tablet.rs` | fw_cfg `"usb-tablet"` string | Parallel `ABS_X/Y` **not** wired through `set_absolute_mouse` | Digitizer parse did not click windows |
| Ring 3 Terminal | `spawn_pty_terminal_app` | Empty+SHM; no in-kernel fallback if ELF spawn fails | Blank “Term nal” window if the client never paints |

Screenshot evidence matches this: guest compositor is healthy, host cursor is hidden, guest cursor is frozen (or an invisible VirtIO-GPU cursor plane), clicks land nowhere useful.

## Todos

### P0 — pointer and keyboard on `./run.sh`

- [x] **P0.1** Restore PS/2 aux as fallback (`gui::input::init_mouse()`). Keep `GATE_USB no ps2 cursor` as a USB KAT; do not skip i8042 on the live desktop.
- [x] **P0.2** Attach `-device virtio-tablet-pci` and `-device virtio-keyboard-pci` in `./run.sh`, UEFI/debug QEMU, Makefile, CI, and `tests/run_integration.sh`.
- [x] **P0.3** Bind VirtIO input **modern** PCI (`0x1040+18 = 0x1052`): capability walk, DMA virtqueue from buddy frames, eventq poll → `set_absolute_mouse`. Stop skipping MMIO BARs.
- [x] **P0.4** USB HID: QEMU tablet is a 5-byte digitizer `[buttons, x_lo, x_hi, y_lo, y_hi]` (0..32767). Parse absolute and call `set_absolute_mouse`. Keep 3-byte boot-mouse for the USB cursor KAT.
- [x] **P0.5** `virtio_tablet::handle_digitizer_report` must update the compositor mouse, not only the parallel tablet atomics.
- [x] **P0.6** VirtIO `EV_KEY` (non-button) → `add_scancode` so keyboard works when XHCI kbd URBs do not complete.
- [x] **P0.7** `GATE_VIRTIO input tablet` — ABS+SYN KAT moves the GUI cursor (host-tests parse the 8-byte virtio-input event).

### P1 — windows that already opened

- [x] **P1.1** Start-menu Terminal: if Ring 3 `openpty` spawn fails, fall through to the in-kernel emulator (`WindowContentType::Terminal`) so the user is not stuck with an Empty SHM hole.
- [x] **P1.2** Honesty: live pointer is VirtIO-tablet absolute + USB digitizer + PS/2 fallback. Docs/status no longer claim USB-only or “PS/2 is the desktop mouse” without the aux init.

### P2 — follow-ups (not blockers for click)

- [x] **P2.1** Title-bar glyph gap (“Term nal”) — thin `i` clipping / icon overlap. Font/layout, not input. (`GATE_FONT title`)
- [x] **P2.2** If VirtIO-GPU cursorq is live but the 64×64 plane is empty, force software cursor so the pointer is visible on a dark wallpaper.
- [x] **P2.3** Enable Slot Command Completion cc=1 plus scratchpad buffers (`GATE_USB xhci slot`). Interrupt-IN URB submit exists. GET_DESCRIPTOR Transfer Event is live (`GATE_USB xhci desc`). Enumeration still has a timeout fallback.
- [x] **P2.4** Ring 3 PTY client must paint SHM on first frame (brown blob in the Terminal client area is uncommitted SHM). (`GATE_TERM shm paint`)

## Done when

1. `./run.sh` QEMU cocoa: moving the host mouse moves the guest pointer 1:1; click opens start menu / icons.
2. Typing reaches the focused window (PS/2 and/or virtio-keyboard).
3. `./tests/run_integration.sh` includes `GATE_VIRTIO input tablet`.
4. `scripts/pre-commit.sh` passes; kernel < 16 MiB.
5. Honesty tags: USB HID URB KAT ≠ live QEMU tablet; VirtIO-tablet is the `./run.sh` pointer.
