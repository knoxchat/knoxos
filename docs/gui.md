# GUI and desktop

The compositor is an in-kernel software desktop, not a Unix display server. Product completeness here is not Unix completeness.

## What is live

- 32bpp BGRA back-buffer, damage rects, `present_rect` to the bootloader framebuffer.
- Window manager, taskbar, start menu, TTF, pointer (VirtIO-tablet absolute, USB HID digitizer, PS/2 aux fallback) + keyboard (IRQ1 and virtio-keyboard).
- Ring 3 SHM clients via `/dev/wl0` ioctl (F1–F2), AF_UNIX `/run/wayland-0` (F5), and `wl_display` object opcodes (`GATE_WL display protocol`).
- Calculator (F6) and Files (F7) are userspace SHM, not `WindowContentType`. Settings, Task Manager, Editor, Browser, and AI spawn the same class. Start-menu Terminal is Ring 3 `openpty` (`GATE_TERM pty interactive`).
- VirtIO-GPU 2D resource scanout + cursorq KAT (F8). Desktop `present_rect` also flushes through that scanout (`GATE_GPU desktop scanout`). The cocoa window is `-vga std`; the pointer is the software arrow (host cursor off). The 64×64 cursor plane is not that window.
- GSUB Latin `fi` ligature (`GATE_GSUB liga`) and GPOS A+V kerning (`GATE_GPOS kern`).

## What is not

- Full IME framework. GPOS mark-to-base is live (`GATE_GPOS mark`). SCM_RIGHTS / memfd `wl_shm.create_pool` (GATE_WL uses a KnoxOS vaddr create_pool).

## Client rule

Freeze new in-kernel apps. New apps should be Ring 3 binaries that present SHM. Terminal owns a real PTY master fd in Ring 3.

## Related

- [architecture.md](architecture.md) — boot and scanout
- [status/subsystems/07-gui.md](../status/subsystems/07-gui.md)
