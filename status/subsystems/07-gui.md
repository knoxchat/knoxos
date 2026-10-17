← [Status index](../README.md) · [Do this first](../next.md)

# 7. GUI & Desktop Environment

**Grade: Live (95%)** · `gui/` 162 files, ~91,700 lines

The compositor is the most complete **product** in the tree. It is not a Unix display server.

### Live
- [x] 32bpp BGRA software FB, double-buffer, ≤16 damage rects, `present_rect`
- [x] Window manager: z-order, focus, min/max, snap, workspaces, chrome, shadows, animations
- [x] Desktop, wallpaper, icons, context menu, pill taskbar, tray, start menu, Alt-Tab, hot corners
- [x] TTF raster, glyph cache, subpixel/grayscale, CJK, RTL runs, KnoxUI widgets, immediate-mode UI
- [x] Mouse (click/drag/resize), keyboard focus, shortcuts, theme (dark/light), notifications
- [x] Lock/login screens, screenshot, in-process DnD, blur, night light, on-screen keyboard
- [x] Clipboard used by terminal/explorer/input (`clipboard.rs`)
- [x] **In-kernel apps remaining**: Interactive start-menu Terminal (PTY I/O). Ring 3 `openpty`+SHM is proven (`GATE_TERM pty userspace`). Calculator (F6) and Files (F7) are Ring 3 SHM; Browser/Editor/Settings/Task Manager/AI plus Image Viewer/Logs/Calendar/Disks/Bluetooth/Updater/Software Center/Setup Wizard/Archive launch userspace SHM.
- [x] **Widget hover/press** — calculator keys and settings tabs highlight under the pointer; retained widgets (`Button`/`Checkbox`/`Toggle`/`TextInput`) have `update_pointer`
- [x] **Frame input latch** — `gui::input::frame_input()` feeds KnoxUI / `Ui::button_at` with press/release edges
- [x] **Window-local damage** — content clicks, chrome hover, and clock ticks composite the window/taskbar instead of 1920×1080
- [x] **Faster software blit** — opaque `fill_rect` stamps packed BGRA; `fill_rounded_rect_aa` fills the body as rects and AA-only the corners

### Unused / stub
- [ ] **Wayland protocol** — `/dev/wl0` ioctl (F1–F2) plus AF_UNIX `/run/wayland-0` present (F5); not a `wl_display` object-protocol server
- [ ] **GPU compositor** — `gpu_compositor.rs` never invoked; desktop software-blits to VGA. VirtIO-GPU 2D scanout of a 64×64 host resource is live (F8).
- [x] **Client isolation (demo)** — Gate F Ring 3 clients paint SHM (F1–F7); Calculator and Files are Empty+SHM (`GATE_F6 calculator userspace`, `GATE_F7 files userspace`). Browser/Editor/Settings/Task Manager/AI plus Calendar/Logs/Disks launch userspace SHM. `GATE_TERM` is Empty+SHM after Ring 3 `openpty`. Interactive start-menu Terminal remains `WindowContentType::Terminal` for live PTY I/O.
- [x] Hardware cursor — VirtIO-GPU cursorq; desktop skips software blit (`GATE_F8 hw cursor`)
- [x] VSync software VBlank on the live present path (`GATE_VSYNC paced`); TSC min-frame still used for desktop blit
- [x] Start-menu **Paint / Video Player / Webamp / Doom / ClassiCube / Quake III** removed; Paint is a Ring 3 SHM client (F4)
- [ ] HDR, VRR, TrueType bytecode hinting, full IME framework

### Perfect-OS next steps
1. Freeze new in-kernel apps. Calculator (F6) and Files (F7) are Ring 3. Every new app should be a userspace binary. Interactive terminal stays in-kernel for PTY I/O.
2. ~~Wayland over Unix sockets~~ F5 `connect`/`sendto` present is live. ~~VirtIO-GPU scanout + hardware cursor~~ F8 live. Next: `wl_display` object protocol.

---
