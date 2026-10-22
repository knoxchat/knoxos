# shm-clients

Other `init/programs/display.rs` clients (one blob each). Source: `src.py`.

| Blob | Marker |
|------|--------|
| `display.elf` | `GATE_F1 client isolated` |
| `terminal.elf` | `GATE_F3 terminal isolated` |
| `settings.elf` | `GATE_F8 settings userspace` |
| `taskmgr.elf` | `GATE_F9 taskmgr userspace` |
| `editor.elf` | `GATE_F10 editor userspace` |
| `browser.elf` | `GATE_F11 browser userspace` |
| `ai.elf` | `GATE_F12 ai userspace` |
| `wayland-unix.elf` | `GATE_F5 wayland unix` |
| `wayland-protocol.elf` | `GATE_WL display protocol` |
| `gpui-button.elf` | `GATE_GPUI ring3 button` |
| `gpui-calc.elf` | `GATE_GPUI ring3 calc` |
| `wayland-server.elf` | `GATE_WL server userspace` |
| `dbus-daemon.elf` | `GATE_DBUS daemon userspace` |

Calculator, Files, Paint, and Terminal have their own `userspace/apps/` directories.
