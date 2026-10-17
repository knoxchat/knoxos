# GUI and desktop

The compositor is an in-kernel software desktop, not a Unix display server. Product completeness here is not Unix completeness.

## What is live

- 32bpp BGRA back-buffer, damage rects, `present_rect` to the bootloader framebuffer.
- Window manager, taskbar, start menu, TTF, PS/2 pointer + keyboard.
- Ring 3 SHM clients via `/dev/wl0` ioctl (F1–F2) and AF_UNIX `/run/wayland-0` (F5).
- Calculator (F6) and Files (F7) are userspace SHM, not `WindowContentType`. Settings, Task Manager, Editor, Browser, and AI spawn the same class.
- VirtIO-GPU 2D resource scanout + hardware cursor plane (F8). When cursorq completes, the desktop does not software-blit the pointer.

## What is not

- `wl_display` object protocol (bind/listen + SHM vaddr is not Wayland-the-protocol).
- GPU compositor (`gpu_compositor`) — init + software VBlank (`GATE_VSYNC paced`); desktop still software-blits to VGA, then F8 also scanouts a 64×64 host resource as the virtqueue proof.
- Interactive start-menu terminal — still `WindowContentType::Terminal` for live PTY I/O. A Ring 3 client `openpty`s and presents SHM (`GATE_TERM pty userspace`).
- HDR, VRR, TrueType bytecode hinting, full IME.

## Client rule

Freeze new in-kernel apps. New apps should be Ring 3 binaries that present SHM. Keep the PTY terminal in-kernel until a userspace terminal can own the PTY.

## Related

- [architecture.md](architecture.md) — boot and scanout
- [status/subsystems/07-gui.md](../status/subsystems/07-gui.md)
