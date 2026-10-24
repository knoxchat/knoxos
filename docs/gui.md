# GUI and desktop

The compositor is an in-kernel software desktop, not a Unix display server. Product completeness here is not Unix completeness.

## What is live

- 32bpp BGRA back-buffer, damage rects, `present_rect` to the bootloader framebuffer.
- Window manager, taskbar, start menu, TTF, pointer (VirtIO-tablet absolute, USB HID digitizer, PS/2 aux fallback) + keyboard (IRQ1 and virtio-keyboard).
- Ring 3 SHM clients via `/dev/wl0` ioctl (F1–F2), AF_UNIX `/run/wayland-0` (F5), and `wl_display` object opcodes (`GATE_WL display protocol`).
- Calculator (F6) and Files (F7) remain 64×64 Gate F KATs. D15 presents Knox GPUI at **window size** via `/run/wayland-0` (`GATE_GPUI ring3 calc`). Keys re-raster that SHM (`GATE_GPUI ring3 calc key`). A musl CRT client calls `KnoxUiRaster` (`GATE_GPUI musl raster`). A Ring 3 process owns `/run/wayland-0` (`GATE_WL server userspace`); the compositor stays in-kernel. Desktop Calculator prefers the Ring 3 client; in-kernel GPUI calc is the fallback. C ABI: `knox_ui_raster_button` / `knox_ui_raster_calculator` in `userspace/libs/knox-ui` (no userspace Rust GUI runtime yet).
- Settings, Task Manager, Editor, Browser, and AI are in-kernel Knox GPUI views. Browser is HTML-to-text (`http_get` + tag strip), not Chromium/Blink. Start-menu Terminal is Ring 3 `openpty` (`GATE_TERM pty interactive`) with GPUI chrome.
- VirtIO-GPU 2D resource scanout follows the software FB size (`GATE_GPUI gpu scanout`) plus the F8 KAT (`GATE_F8 virtio-gpu scanout`). Desktop `present_rect` flushes the damaged rect through that resource (`GATE_GPU desktop scanout`). `./run.sh` uses `-vga virtio` so that scanout **is** the cocoa/GTK window (`GATE_GPU modern stack`). Cursorq `UPDATE_CURSOR` is proven (`GATE_GPU pointer plane`) but QEMU cocoa does not composite that overlay, so the visible pointer is the software arrow in the scanout plus the host cursor (`show-cursor=on`). One framebuffer; `monitor.rs` types do not fake per-head DPI.
- GSUB Latin `fi` ligature (`GATE_GSUB liga`) and GPOS A+V kerning (`GATE_GPOS kern`).

## What is not

- Full IME framework. GPOS mark-to-base is live (`GATE_GPOS mark`). Linux `wl_shm.create_pool` via `SCM_RIGHTS`+memfd is live (`GATE_CHR wlshm`); the Knox vaddr opcode remains for F5/WL.
- Start-menu Browser prefers `/opt/vivaldi/vivaldi-bin` (`GATE_VIV ui`) then `/opt/chromium/chrome` (`GATE_CHR launch`). GPUI HTML-to-text is the fallback if those ELFs are missing. Ozone/Wayland xdg-shell SHM paints Chromium- or Vivaldi-titled windows (`GATE_CHR paint`, `GATE_VIV ui`). Userspace HTTPS fetches `https://example.com` with CSS/JS (`GATE_CHR https`). `chrome://gpu` paints CSS/JS via SwiftShader + dmabuf (`GATE_CHR gpuinfo`). A local MP4 decodes with ICU/FreeType/ffmpeg and Pulse+ALSA (`GATE_CHR media`). Sandbox is on (`GATE_CHR sandboxui`): no `--no-sandbox`. Mojo runs without `--single-process` (`GATE_CHR mojo`). Ring 3 `io_uring_setup(8)` returns a ring fd (`GATE_CHR iouring`). Ring 3 `userfaultfd` stays ENOSYS. Backlog: [status/subsystems/19-browser.md](../status/subsystems/19-browser.md).

## Client rule

Freeze new in-kernel apps. New apps should be Ring 3 binaries that present SHM. Terminal owns a real PTY master fd in Ring 3.

## Related

- [architecture.md](architecture.md) — boot and scanout
- [status/subsystems/07-gui.md](../status/subsystems/07-gui.md)
- [status/subsystems/19-browser.md](../status/subsystems/19-browser.md) — Chromium / Vivaldi program
