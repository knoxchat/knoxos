← [Status index](../README.md) · [Do this first](../next.md)

# 7. GUI & Desktop Environment

**Grade: Live (99%)** · `gui/` 162 files, ~91,700 lines

The compositor is the most complete **product** in the tree. It is not a Unix display server.

### Live
- [x] 32bpp BGRA software FB, double-buffer, ≤16 damage rects, `present_rect`
- [x] Window manager: z-order, focus, min/max, snap, workspaces, chrome, shadows, animations
- [x] Desktop, wallpaper, icons, context menu, pill taskbar, tray, start menu, Alt-Tab, hot corners
- [x] TTF raster, glyph cache, subpixel/grayscale, CJK, RTL runs, KnoxUI widgets, immediate-mode UI
- [x] Mouse (click/drag/resize), keyboard focus, shortcuts, theme (dark/light), notifications
- [x] Lock/login screens, screenshot, in-process DnD, blur, night light, on-screen keyboard
- [x] Clipboard used by terminal/explorer/input (`clipboard.rs`)
- [x] **In-kernel apps remaining**: none on the start menu. Terminal is Ring 3 `openpty`+SHM (`GATE_TERM pty interactive`). Calculator (F6) and Files (F7) are Ring 3 SHM; Browser/Editor/Settings/Task Manager/AI plus Image Viewer/Logs/Calendar/Disks/Bluetooth/Updater/Software Center/Setup Wizard/Archive launch userspace SHM.
- [x] **Widget hover/press** — calculator keys and settings tabs highlight under the pointer; retained widgets (`Button`/`Checkbox`/`Toggle`/`TextInput`) have `update_pointer`
- [x] **Frame input latch** — `gui::input::frame_input()` feeds KnoxUI / `Ui::button_at` with press/release edges
- [x] **Window-local damage** — content clicks, chrome hover, and clock ticks composite the window/taskbar instead of 1920×1080
- [x] **Faster software blit** — opaque `fill_rect` stamps packed BGRA; `fill_rounded_rect_aa` fills the body as rects and AA-only the corners

### Unused / stub
- [x] **Wayland protocol** — `/dev/wl0` ioctl (F1–F2), AF_UNIX `/run/wayland-0` (F5), and `wl_display` object opcodes (`GATE_WL display protocol`)
- [x] **GPU compositor** — `gpu_compositor` init + VSync; desktop present flushes through VirtIO-GPU (`GATE_GPU desktop scanout`). VGA blit remains for `-vga std`. VirtIO-GPU 2D scanout of a 64×64 host resource is live (F8).
- [x] **Client isolation (demo)** — Gate F Ring 3 clients paint SHM (F1–F7); Calculator and Files are Empty+SHM (`GATE_F6 calculator userspace`, `GATE_F7 files userspace`). Browser/Editor/Settings/Task Manager/AI plus Calendar/Logs/Disks launch userspace SHM. Start-menu Terminal is Empty+SHM after Ring 3 `openpty` (`GATE_TERM pty interactive`).
- [x] Hardware cursor — VirtIO-GPU cursorq; desktop skips software blit (`GATE_F8 hw cursor`)
- [x] VSync software VBlank on the live present path (`GATE_VSYNC paced`); desktop present also flushes VirtIO-GPU (`GATE_GPU desktop scanout`)
- [x] Start-menu **Paint / Video Player / Webamp / Doom / ClassiCube / Quake III** removed; Paint is a Ring 3 SHM client (F4)
- [x] IME — pinyin `"ni"` → `你` (`GATE_IME compose`)
- [x] TrueType bytecode hinting — SVTCA/PUSHB/MDAP round-to-grid (`GATE_TTF hint`)
- [x] HDR, VRR — PQ EOTF + Adaptive-Sync range (`GATE_HDR pq`, `GATE_VRR range`). Full IME framework later.

### Perfect-OS next steps
1. Freeze new in-kernel apps. Calculator (F6), Files (F7), and Terminal (`GATE_TERM`) are Ring 3. Every new app should be a userspace binary.
2. ~~Wayland over Unix sockets~~ F5 live. ~~`wl_display` object protocol~~ `GATE_WL` live. ~~VirtIO-GPU scanout + hardware cursor~~ F8 live. ~~Desktop GPU scanout~~ `GATE_GPU` live.

---
