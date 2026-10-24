← [Status index](../README.md) · [Do this first](../next.md)

# 7. GUI & Desktop Environment

**Grade: Live** · Knox GPUI is the live toolkit (`./run.sh`). Compositor stays in-kernel.

The session chrome, catalog gallery, and in-process apps are Knox GPUI views (`userspace/libs/knox-gpui` + `userspace/libs/knox-ui`) rastered in software and blitted by the compositor. This is not a 99% hand-drawn painter and not an unused widget stack beside the desktop.

### Live

- [x] 32bpp BGRA software FB, double-buffer, ≤16 damage rects, `present_rect`
- [x] Window manager: z-order, focus, min/max, snap, workspaces, CSD TitleBar for GPUI views, SSD for Ring 3 SHM
- [x] **Panel / dock, launcher, OSD, notify, tray, Alt-Tab, desktop icons, hot corners** as Knox GPUI (`GATE_GPUI panel` / `launcher` / `osd` / `notify` / `tray` / `alt-tab` / `desktop` / `hot corner`)
- [x] TTF via `font_engine` into the Scene atlas. Title `'i'` keeps a stem (`GATE_FONT title`)
- [x] Mouse (VirtIO-tablet absolute + USB digitizer + PS/2 aux), Tab focus rings, `cmd-k` keymap (`command_palette`)
- [x] Login / lock as GPUI Dialogs (`GATE_GPUI login`); auto-login default true so QEMU GATEs still reach the desktop
- [x] Calculator, Settings, Files, Task Manager, Editor, Browser, AI, Terminal chrome, utilities, installer/picker as GPUI views. Browser is HTML-to-text (`http_get` + tag strip), not Blink. Chromium: [19-browser.md](19-browser.md).
- [x] Kit catalog gallery (`GATE_GPUI kit gallery`)
- [x] Scene dirty regions (typing damages the focused field, not 1920×1080)
- [x] Opaque fill SIMD on x86_64 (`GATE_GPUI simd fill`)
- [x] VirtIO-GPU scanout at FB size (`GATE_GPUI gpu scanout`). `-vga virtio` primary + hardware cursor plane (`GATE_GPU modern stack`, `GATE_GPU pointer plane`)
- [x] Night light and zoom lens are compositor post-process on the composed FB (including GPUI blits)
- [x] High-contrast + reduced-motion tokens from Settings; panel clock and launcher labels follow `locale.rs`
- [x] Keyboard cheat-sheet is a GPUI Dialog (`?` / Ctrl+/); lists D6.3 `cmd-k` / `command_palette`
- [x] Ring 3 can present Knox GPUI rasters via `/run/wayland-0` at **window size** (`GATE_GPUI ring3 calc`). C ABI: `knox_ui_raster_button` / `knox_ui_raster_calculator`. Desktop Calculator prefers that client; in-kernel GPUI calc is the fallback
- [x] Interactive Ring 3 calc re-rasters SHM on key (`GATE_GPUI ring3 calc key`)
- [x] musl CRT client calls `KnoxUiRaster` and presents GPUM (`GATE_GPUI musl raster`)
- [x] Ring 3 Wayland server owns `/run/wayland-0` (`GATE_WL server userspace`); compositor stays in-kernel
- [x] Wayland SHM (`/dev/wl0`, `/run/wayland-0`, `wl_display` opcodes)
- [x] Dead stacks removed: `knoxui/`, unused `widgets/`, `gui/wayland_server.rs`, Zed / Longbridge reference trees

### Unused / later

- [x] Hardware cursor plane — cursorq KAT on `-vga virtio`; cocoa shows the software arrow in the GPU scanout
- [x] Multi-monitor is still one FB (`monitor.rs` types; no fake per-head DPI)
- Ring 3 Rust GUI runtime (D15.2 documents the C ABI until musl Rust exists)
- Chromium Ozone/Wayland xdg-shell + `wl_shm` + dmabuf are **done** (`GATE_CHR paint`, `GATE_CHR dmabuf`). Start-menu launch is **done** (`GATE_CHR launch`). Sandbox is **done** (`GATE_CHR sandboxui`). Vivaldi `.deb` is **done** (`GATE_VIV deb` / `ui`).

### Perfect-OS next steps

1. A musl Rust GUI runtime (today `KnoxUiRaster` is the C-ABI trampoline; no musl Rust target yet).
2. **Chromium first paint** — xdg-shell + `SCM_RIGHTS` memfd pool so Ozone/Wayland can commit a buffer ([19-browser.md](19-browser.md) phase 3).

---
