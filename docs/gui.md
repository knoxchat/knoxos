# GUI and desktop

The compositor is an in-kernel software desktop, not a Unix display server. Product completeness here is not Unix completeness.

## What is live

- 32bpp BGRA back-buffer, damage rects, `present_rect` to the bootloader framebuffer.
- Window manager, taskbar, start menu, TTF, pointer (VirtIO-tablet absolute, USB HID digitizer, PS/2 aux fallback) + keyboard (IRQ1 and virtio-keyboard).
- Ring 3 SHM clients via `/dev/wl0` ioctl (F1–F2), AF_UNIX `/run/wayland-0` (F5), and `wl_display` object opcodes (`GATE_WL display protocol`).
- Calculator (F6) and Files (F7) remain 64×64 Gate F KATs. D15 presents Knox GPUI at **window size** via `/run/wayland-0` (`GATE_GPUI ring3 calc`). Keys re-raster that SHM (`GATE_GPUI ring3 calc key`). A musl CRT client calls `KnoxUiRaster` (`GATE_GPUI musl raster`). A Ring 3 process owns `/run/wayland-0` (`GATE_WL server userspace`); the compositor stays in-kernel. Desktop Calculator prefers the Ring 3 client; in-kernel GPUI calc is the fallback. C ABI: `knox_ui_raster_button` / `knox_ui_raster_calculator` in `userspace/libs/knox-ui` (no userspace Rust GUI runtime yet).
- Settings, Task Manager, Editor, Browser, and AI are in-kernel Knox GPUI views. Start-menu Terminal is Ring 3 `openpty` (`GATE_TERM pty interactive`) with GPUI chrome.
- VirtIO-GPU 2D resource scanout follows the software FB size (`GATE_GPUI gpu scanout`) plus the F8 KAT (`GATE_F8 virtio-gpu scanout`). Desktop `present_rect` flushes the damaged rect through that resource (`GATE_GPU desktop scanout`). The cocoa/GTK window is still `-vga std`; the visible pointer is the software arrow (host cursor off). VirtIO-GPU cursorq is a 64×64 overlay that is **not** that window, so it stays disabled for pointing. One framebuffer; `monitor.rs` types do not fake per-head DPI.
- GSUB Latin `fi` ligature (`GATE_GSUB liga`) and GPOS A+V kerning (`GATE_GPOS kern`).

## What is not

- Full IME framework. GPOS mark-to-base is live (`GATE_GPOS mark`). SCM_RIGHTS / memfd `wl_shm.create_pool` (GATE_WL uses a KnoxOS vaddr create_pool).

## Client rule

Freeze new in-kernel apps. New apps should be Ring 3 binaries that present SHM. Terminal owns a real PTY master fd in Ring 3.

## Related

- [architecture.md](architecture.md) — boot and scanout
- [status/subsystems/07-gui.md](../status/subsystems/07-gui.md)
