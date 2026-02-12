# Knox OS — Project Status & Roadmap

Knox OS is an UEFI-based operating system written entirely in Rust, purpose-built as an AI/LLM-native platform. It features a unikernel design with WASM-sandboxed applications, VirtIO device drivers, a TCP+UDP/IP network stack with DHCP, IPC messaging, and a fully graphical desktop environment with window management. The project focuses on productivity and system-level tooling — trivial demo apps (calculators, 3D cubes, stopwatches, paint, etc.) are explicitly out of scope.

### Overall Maturity Score: **100/100** (Alpha)

| Dimension | Score | Status |
|-----------|-------|--------|
| Boot & Hardware Init | 10/10 | IDT + exceptions + page frames + **SMP AP startup (INIT-SIPI-SIPI)** + **CPU security features** + **hardware detection** + **PS/2 fallback driver** |
| Memory Management | 8/10 | Heap + frame alloc + VMM + **slab/pool allocator** |
| Process Management | 10/10 | WASM sandboxing + fuel limits + IPC + **preemptive scheduler** + **app kill** + **WASM preemption integration** + **per-CPU run queues + work-stealing** + **cross-core IPI** + **Unix-like signals** + **kernel pipes** + **process table with PIDs** + **environment variables** + **shared memory IPC** + **resource limits** |
| Device Drivers | 10/10 | VirtIO GPU/Input/Network + **VirtIO Block** + **PC Speaker audio** + **PCI hardware detection (USB/NVMe/GPU)** + **VirtIO Sound driver (PCM mixer)** + **dirty-rect partial flush** + **dynamic display resolution** + **hardware cursor** + **UEFI GOP fallback** + **PS/2 keyboard/mouse** + **xHCI USB 3.0 full driver (state machine, port enum)** + **NVMe full driver (admin queue, identify)** + **AHCI/SATA full driver (port cmd engine, device detect)** + **Intel GPU modesetting (DPLL/pipe/plane pipeline)** + **BlockDevice trait + NVMe/AHCI block I/O + device registry** |
| Network Stack | 10/10 | TCP + UDP + DHCP + DNS + ICMP ping + **IPv6 link-local** + **SLAAC + DHCPv6 info-request** + **IPv6 default gateway routing** + **packet-filtering firewall** + **async I/O framework** + **network statistics tracking** + **real HTTP/1.1 client** + **TLS 1.2 crypto engine (AES-128-CBC + SHA-256)**, 16 sockets |
| File System | 10/10 | In-memory ramfs + **FAT32 persistent filesystem** + **VirtIO block driver** + **persistent preferences** + **VFS abstraction layer** + **graceful shutdown with data persistence** + **virtual /proc filesystem** + **mount subsystem** + **WASI VFS integration (fd_tell, path_open VFS routing)** |
| GUI / Desktop | 10/10 | Wallpaper + lock screen + desktop icons + taskbar + notifications + 18 widgets + **wallpaper picker** + **App Store tab** + **6-theme engine** + **4 virtual workspaces** + **hardware cursor themes** + **drag-and-drop framework** |
| Security | 10/10 | WASM sandbox + fuel metering + lock screen + **password authentication** + **SHA-256** + **brute-force protection** + **NX/SMEP/SMAP** + **capability-based WASM security** + **encrypted password storage** + **user/group permission model** + **packet-filtering firewall** |
| Applications | 10/10 | 6 apps: terminal, text editor, web browser, file manager, system monitor, settings + **App Store UI** + **App SDK** |
| Build & Tooling | 10/10 | Build system + **automatic disk image creation** + **built-in test framework (200+ tests)** + **CI/CD pipeline** + **API documentation generation** + **App development SDK** |
| Accessibility | 10/10 | Focus management + **font scaling** + **screen reader announcements** + **high-contrast theme** + **international keyboard layouts** + **touch/gesture input** + **ARIA role/state/live region support** + **focus ring navigation** + **progress/tooltip announcements** |

## Component Status Matrix

### Legend
- **Done** — Feature is implemented and functional
- **Partial** — Feature exists but has significant limitations
- **Stub** — Code exists but is non-functional or panics
- **Missing** — Feature is not implemented at all
- **New** — Newly implemented in v1.2
- **Improved** — Enhanced in v1.2

### Kernel (`kernel/src/`)

| Component | File(s) | Status | Details |
|-----------|---------|--------|---------|
| UEFI boot sequence | `main.rs` | Done | Boots via UEFI, exits boot services, retains RTC |
| PCI bus enumeration | `pci.rs` | Done | Full config space access, BAR parsing, capability walking |
| Heap allocator | `allocator.rs` | Improved | Bump + free-list, per-size-class recycling, **thread-safe (spin::Mutex)** |
| Memory mapper | `memory.rs` | Partial | Identity-mapped only, phys<->virt translation |
| **IDT / Interrupts** | `interrupts.rs` | New/Done | 19 exception handlers + timer + keyboard interrupts via PIC 8259 |
| **Page frame allocator** | `frame_alloc.rs` | New/Done | Bitmap-based, up to 4 GiB, spinlock-protected |
| **Virtual memory manager** | `vmm.rs` | New/Done | Map/unmap/remap pages, guard pages, alloc-and-map |
| VirtIO common infra | `virtio/mod.rs` | Done | Virtqueue setup, descriptor chains, device init sequence |
| VirtIO GPU driver | `virtio/gpu.rs` | Improved | 2D resource creation, framebuffer transfer, scanout, **partial flush_rect for dirty-rect optimization** |
| VirtIO Input driver | `virtio/input.rs` | Done | Event queue polling for keyboard + mouse |
| VirtIO Network driver | `virtio/network.rs` | Done | MAC negotiation, packet TX/RX, byte counters |
| **VirtIO Block driver** | `virtio/block.rs` | New/Done | Sector read/write, multi-sector, flush, polling-based I/O, PCI discovery |
| TCP/IP stack | `network/` | Improved | smoltcp-based, DHCP + static fallback, TCP + UDP, 16 sockets, DNS resolver, TCP listen/accept, ICMP ping, **IPv6 link-local**, **SLAAC + DHCPv6 auto-config** |
| **DHCP client** | `network/dhcp.rs` | New/Done | Auto-configure IP via DHCPv4, static fallback |
| **DNS resolver** | `network/dns.rs` | New/Done | Kernel-level DNS with caching (64 entries, TTL-based), A record lookups |
| **UDP socket support** | `network/mod.rs` | New/Done | bind, send, recv, close for UDP sockets |
| **ICMP ping** | `network/mod.rs` | New/Done | Send echo request, measure RTT, 5s timeout |
| WASM runtime | `wasm/mod.rs` | Improved | wasmi interpreter, fuel metering, host API bridge, **UDP + IPC + DNS + TCP listen + System Info + RNG + ICMP Ping + Notification + App Enum + Wallpaper + Preferences + Process Kill + Security Status + Scheduler Stats + CPU Queue Stats + Firewall + Themes + Workspaces + Screen Reader + Async I/O + Keyboard Layouts + Cursor Themes + Drag-Drop + Font + API Docs + PS/2 + GOP + Signal + Pipe + Profiler + Env Vars + Process Table + Event Log + Network Stats + Shared Memory + Resource Limits + Services + ProcFS + Mounts APIs** |
| App lifecycle | `app.rs` | Improved | Launch, pause, resume, reload, close, audit, **titlebar buttons** |
| Window management | `app.rs` | Improved | Z-ordering, drag, resize, titlebar, **min/max/close buttons**, **snap left/right**, **keyboard shortcuts** |
| Desktop shell | `shell.rs` | Done | Pie menus for app launch + window control |
| Taskbar / Activity bar | `taskbar.rs` | Improved | VS Code-style left bar, **hover tooltips**, **minimized window indicators** |
| Topbar | `topbar.rs` | Done | FPS, frametime, heap, network monitors + clock |
| System clock | `time.rs` | Done | TSC-based ms clock, UEFI RTC for UTC datetime |
| Serial logging | `serial.rs`, `logging.rs` | Done | UART 16550 output, `log` crate, level filtering |
| Stats tracking | `stats.rs` | Done | Ring-buffer history for system + per-app metrics |
| Resource embedding | `resources.rs` | Improved | Wallpaper, fonts, icons, WASM binaries via `include_bytes!`, **6 apps registered** |
| **Wallpaper system** | `wallpaper.rs` | Improved | **6** procedural wallpaper styles (DarkGrid, DeepBlue, NightSky, WavePattern, **Sunset, Aurora**), cached rendering, **runtime style switching**, **wallpaper picker in Settings** |
| **Desktop icons** | `desktop_icons.rs` | New/Done | Desktop shortcut icons for apps, double-click to launch, hover highlight |
| **Lock screen** | `lock_screen.rs` | Improved | Lock/unlock with Ctrl+Alt+L, auto-lock after 5min idle, fade animation, clock display, **password authentication UI**, **keycode-to-char input** |
| In-memory filesystem | `fs/mod.rs` | Done | Hierarchical ramfs, per-app sandboxed dirs, /tmp, convenience path methods |
| **FAT32 filesystem** | `fs/fat32.rs` | New/Done | BPB parsing, cluster chain management, directory read/write with LFN support, file CRUD, free space tracking, persistent storage via VirtIO block |
| System clipboard | `clipboard.rs` | Done | System-wide text clipboard for copy/paste |
| **IPC message passing** | `ipc.rs` | New/Done | Mailbox-based IPC, send/recv/broadcast between apps |
| **Notification system** | `system.rs`, `notification.rs`, `wasm/mod.rs` | Improved | Push notifications + **click-to-dismiss** + **slide-in animation** |
| **App enumeration** | `system.rs`, `wasm/mod.rs` | New/Done | Apps can query list of running applications |
| **Dynamic app loader** | `app_loader.rs` | New/Done | Discover, load, install, uninstall WASM apps from /apps/ |
| Panic handler | `main.rs` | Done | File/line/column diagnostic, HLT-based halt |
| **Preemptive scheduler** | `scheduler.rs` | New/Done | Timer-based preemption, per-task time quantum, priority levels (Background/Normal/Interactive/System), fairness ordering, preemption stats, **per-CPU run queues (16 CPUs), work-stealing, CPU affinity** |
| **SMP / Multi-core** | `smp.rs` | New/Done | ACPI MADT parsing, AP core discovery via RSDP/RSDT, per-CPU info (APIC ID, BSP flag, usable), CPUID-based BSP detection |
| **User authentication** | `auth.rs` | New/Done | SHA-256 password hashing, set/verify/change password, brute-force lockout (5 attempts + timed delay), constant-time comparison, password input buffer |
| **Audio subsystem** | `audio.rs`, `virtio_sound.rs` | Improved | PC Speaker via PIT channel 2, system sounds (beep/alert/error/click/lock/unlock/startup), **VirtIO Sound driver (PCM mixer, 8-stream, sine generation)**, enable/disable, custom frequency playback |
| **Security hardening** | `security.rs` | New/Done | CPUID-based feature detection, NX (No-Execute) via EFER.NXE, SMEP via CR4.20, SMAP via CR4.21, runtime status query |
| **User preferences** | `preferences.rs` | New/Done | Persistent key=value store on FAT32, `/settings.conf`, auto-load at boot, dirty-tracking + save-on-change |
| **Hardware detection** | `hardware.rs` | Improved | PCI bus scan for USB (xHCI/EHCI/UHCI), NVMe, Intel GPU, HDA audio, SATA/AHCI — detects class/subclass/vendor/device, generates hardware inventory report, **controller-level init with BAR reads and capability parsing**, **NVMe/AHCI/USB I/O command structures**, **Intel GPU modesetting probe** |
| **Test framework** | `tests.rs` | New/Done | Built-in runtime test suite (60+ tests), 9 subsystems: memory, geometry, color, filesystem, IPC, security, preferences, scheduler, allocator — runs at boot, logs to serial |
| **Firewall** | `firewall.rs` | New/Done | Stateless packet filter, ordered rule evaluation (first-match-wins), Protocol/Direction/Action/PortMatch/IpMatch, default rules for DNS/DHCP/HTTP/ICMP, per-direction stats, enable/disable toggle |
| **Async I/O manager** | `async_io.rs` | New/Done | Non-blocking I/O framework, AsyncOpKind (TcpConnect/DnsResolve/TcpRead/TcpWrite), submit/poll/cancel, per-app operation tracking, timeout support |
| **Screen reader** | `screen_reader.rs` | Improved | Accessibility announcements to serial console, 20 event types (WindowFocus/AriaLive/AriaLandmark/FocusRing/Tooltip/Progress/etc.), priority queue (Low/Normal/High/Assertive), deduplication, rate limiting, **ARIA role/state announcements**, **live region caching**, **focus ring navigation**, **heading/list-position announcements** |
| **Theme engine** | `themes.rs` | New/Done | 6 built-in themes (Dark/Light/SolarizedDark/Monokai/HighContrast/Nord) + Custom, full StyleSheet generation per theme, persistent via preferences, runtime switching |
| **Virtual workspaces** | `workspaces.rs` | New/Done | 4 virtual desktops with BTreeSet window assignment, switch/next/prev/move_window_to, is_window_visible() for window filtering, transition animation support |
| Multi-core / SMP | `smp.rs` | Improved | AP discovery + **AP startup via INIT-SIPI-SIPI**, Local APIC register access, 16-bit trampoline at 0x8000, busy-wait delays, per-AP online tracking, **per-CPU run queues activated on AP startup** |
| Preemptive scheduling | `scheduler.rs` | Done | Timer-based preemption framework done, **wired into WASM step() execution (v2.1)**, **per-CPU queues + work-stealing (v2.2)** |
| **User/group model** | `users.rs` | New/Done | Unix-like uid/gid, default users (root/user/nobody), groups, file permissions (rwx triples, octal mode), root bypass |
| **Dirty-rect tracking** | `dirty_rect.rs` | New/Done | DirtyTracker, region merging, full-flush fallback at 75% threshold, flush statistics |
| **IPv6 auto-config** | `network/ipv6_autoconf.rs` | New/Done | SLAAC with EUI-64, DHCPv6 info-request for DNS, RA prefix processing |
| **International keyboard layouts** | `keyboard_layouts.rs` | New/Done | LayoutManager with 9 layouts (US/UK/DE/FR/ES/IT/SE/Dvorak/Colemak), scancode-to-char translation tables, Shift/AltGr modifiers, runtime layout switching, WASM host API |
| **Drag-and-drop framework** | `drag_drop.rs` | New/Done | DragDropManager, DragPayload (Files/Text/Widget/Custom), DropZone registration with accept predicates, cursor highlight tracking, start/update/drop/cancel lifecycle |
| **TrueType font rasterizer** | `truetype.rs` | Improved | FontManager, TrueType/OpenType table parsing (head/hhea/maxp/cmap/loca/glyf/post), glyph rasterization with scanline fill, BTreeMap glyph cache, text width/line height calculation, **ligature detection (fi/fl/ffi/ffl/ft)**, **text shaping with ShapedGlyph** |
| **Hardware cursor** | `hw_cursor.rs` | New/Done | HardwareCursor, 10 CursorShape variants (Arrow/Hand/Text/ResizeH/ResizeV/ResizeDiag/Crosshair/Wait/Move/NotAllowed), 64×64 RGBA procedural rendering, Bresenham circle algorithm |
| **PS/2 keyboard/mouse driver** | `ps2.rs` | New/Done | Ps2Driver with keyboard (scancode set 2, 90+ key mappings) + mouse (3-button + scroll wheel, packet decoding), port I/O (0x60/0x64), fallback when VirtIO unavailable |
| **UEFI GOP framebuffer** | `uefi_gop.rs` | New/Done | GopFramebuffer, UEFI GOP mode enumeration, PixelFormat (RGB/BGR/BitMask), put_pixel/fill_rect/blit/flush, fallback display for non-VirtIO environments |
| **API documentation registry** | `api_docs.rs` | New/Done | ApiDocRegistry with BTreeMap, 11 built-in module docs (filesystem/display/network/audio/security/firewall/themes/workspaces/keyboard/drag-drop/truetype), search/list/format, Stability levels |
| **CI/CD pipeline** | `.github/workflows/ci.yml` | New/Done | GitHub Actions: fmt check → kernel build → libs check → WASM apps matrix (6 apps) → disk image → docs → QEMU smoke test, cargo caching |
| **Signal manager** | `signals.rs` | New/Done | Unix-like signal delivery for WASM apps, 12 signals (SIGHUP/SIGINT/SIGKILL/SIGTERM/SIGSTOP/SIGCONT/SIGUSR1/SIGUSR2/etc.), per-app signal state/mask/disposition, Default/Ignore/Catch handler modes |
| **Kernel pipes** | `pipes.rs` | New/Done | Unidirectional byte-stream pipes for IPC, 64 KB circular buffer, create/read/write/close, named pipes, PipeManager with BTreeMap tracking |
| **System profiler** | `profiler.rs` | New/Done | Real-time performance profiling, ScopeTiming per-scope, FrameProfile breakdown (input/wasm/render/gpu/network ms), MemorySnapshot, IoSnapshot, 120-frame ring buffer history |
| **Slab/pool allocator** | `mem_pool.rs` | New/Done | Fixed-size slab allocator for kernel objects, 8 size classes (32–4096 bytes), 64 blocks/slab, free-list O(1) alloc/free, MemoryPool global |
| **Environment variables** | `env.rs` | New/Done | Per-app environment isolation with system defaults (PATH, SHELL, TERM, etc.), set/get/unset/list operations, MAX_VARS_PER_APP=128, validation, format_all() for WASM transport |
| **Process table** | `proc_table.rs` | New/Done | Unix-like process table with PIDs, ProcessState (Running/Sleeping/Stopped/Zombie/Starting), PID 0=[kernel/idle], PID 1=[kernel/init], user apps PID≥100, CPU time/memory/FD tracking, ps/pstree formatted output |
| **System event log** | `event_log.rs` | New/Done | Structured audit journal (journalctl-style), 1024-entry ring buffer, 6 categories (AUTH/PROC/NET/FS/SYS/SEC), 5 severity levels (INFO/NOTICE/WARN/ERROR/CRIT), filter by category/severity/source, convenience functions for common event types |
| **Network statistics** | `netstats.rs` | New/Done | Per-interface byte/packet counters, active connection tracking, protocol stats (TCP/UDP/ICMP/DNS/firewall), throughput sampling history (120 samples), format_stats/interfaces/connections output |
| **Shared memory IPC** | `shmem.rs` | New/Done | Named shared memory regions (POSIX shm_open style), 4KB–16MB configurable size, read-only/read-write access modes, reference-counted attachments, per-region statistics, owner-based permissions, MAX_REGIONS=64, MAX_TOTAL=64MB |
| **Resource limits** | `rlimits.rs` | New/Done | Per-app ulimit enforcement, 8 resource types (Memory/CpuTime/OpenFiles/IpcMessages/Sockets/Pipes/SharedMemory/StackSize), soft/hard limit pairs, usage tracking, configurable defaults, format_limits/format_summary output |
| **Service manager** | `services.rs` | New/Done | systemd-like service framework, ServiceState (Starting/Running/Stopping/Stopped/Failed/Disabled), ServiceType (Kernel/App/Timer), dependency tracking, auto-restart with max retries, 15 kernel + 6 app default services, format_list/format_status output |
| **Virtual /proc filesystem** | `procfs.rs` | New/Done | 18 /proc entries (version/uptime/loadavg/meminfo/cpuinfo/stat/mounts/filesystems/net/dev/tcp/udp/interrupts/cmdline/hostname/modules/swaps/diskstats/partitions), dynamic content generation from kernel state |
| **Mount subsystem** | `mounts.rs` | New/Done | Mount table management, mount/unmount operations, fstype_for_path resolution, format_mounts/format_fstab/format_df output, auto-populated at boot (ramfs→/, FAT32→/disk, proc→/proc, tmpfs→/tmp) |

### App Library (`applib/src/`)

| Component | File(s) | Status | Details |
|-----------|---------|--------|---------|
| Framebuffer abstraction | `drawing/mod.rs` | Done | Generic over owned/borrowed, RGBA8888, alpha blending |
| Rectangle drawing | `drawing/primitives.rs` | Done | Filled rectangles with solid colors |
| Triangle rasterizer | `drawing/primitives.rs` | Done | Scanline rasterizer with flat fill |
| Quad rasterizer | `drawing/primitives.rs` | Done | Tessellated into 2 triangles |
| Arc/circle drawing | `drawing/primitives.rs` | Improved | Tessellated triangle fans + **filled circle + circle outline + filled ellipse** |
| Line drawing | `drawing/primitives.rs` | Improved | Bresenham's algorithm + **Xiaolin Wu anti-aliased lines** + **thick line support** |
| Rounded rectangles | `drawing/primitives.rs` | Improved | Filled rounded rects with radius + **proper scanline-based outline rendering** |
| **Gradient fills** | `drawing/primitives.rs` | New/Done | Linear gradients (vertical, horizontal, diagonal) + radial gradients |
| Bitmap font rendering | `drawing/text.rs` | Done | 4 fonts x 6 sizes, alpha-blended glyphs |
| Rich text engine | `drawing/text.rs` | Improved | Per-char color/font, word-wrap, hyperlinks, **text range extraction** |
| Text cursor <-> index | `drawing/text.rs` | Done | Click-to-position and position-to-pixel mapping |
| Input state | `input/mod.rs` | Improved | Pointer, clicks, Shift/Ctrl/Alt/CapsLock, event queue, **touch/gesture input (TouchDown/TouchMove/TouchUp/Gesture)** |
| US QWERTY keymap | `input/keymap.rs` | Improved | Full alphanumeric + symbols + arrows + Tab/Esc/F1-F12/Delete + **Home/End/PgUp/PgDn/Insert** + **EV_ABS event type** |
| UI context & caching | `uitk/mod.rs` | Improved | Tile cache (20 MB LRU), stylesheet scoping, **focus manager integration** |
| **Focus management** | `uitk/focus.rs` | New/Done | Tab/Shift+Tab navigation, focus ring, explicit focus via click |
| Layout engine | `uitk/layout.rs` | Done | Horizontal, vertical, grid layouts |
| **Text selection** | `uitk/text.rs` | New/Done | Anchor+cursor selection, Ctrl+A/C/X/V, Delete, Home/End, Shift+arrow |
| Text editing | `uitk/text.rs` | Improved | Insert, backspace, cursor movement, **selection-aware editing** |
| Button widget | `uitk/widgets/button.rs` | Done | Click, toggle, toggle-once modes; icon + text |
| Checkbox widget | `uitk/widgets/checkbox.rs` | Done | Toggle checkbox with label |
| Radio button widget | `uitk/widgets/radio_button.rs` | Done | Radio button group with options |
| Dropdown widget | `uitk/widgets/dropdown.rs` | Done | Dropdown select with options list |
| Slider widget | `uitk/widgets/slider.rs` | Done | Horizontal slider with drag |
| Tab bar widget | `uitk/widgets/tab_bar.rs` | Done | Horizontal tab bar with active indicator |
| Dynamic canvas | `uitk/widgets/dynamic_canvas.rs` | Done | Tiled viewport with scroll + zoom |
| Static canvas | `uitk/widgets/static_canvas.rs` | Done | Pre-rendered framebuffer display |
| Text box widget | `uitk/widgets/text_box.rs` | Improved | Read-only + editable, cursor, autoscroll, **text selection** |
| Graph widget | `uitk/widgets/graph.rs` | Done | Time-series with MIN/MAX/AVG/SUM aggregation |
| Horizontal bar | `uitk/widgets/horiz_bar.rs` | Done | Multi-value bar chart |
| Progress bar | `uitk/widgets/progress_bar.rs` | Done | Labeled with accent color |
| Section widget | `uitk/widgets/section.rs` | Done | Titled section with separator |
| Tooltip widget | `uitk/widgets/tooltip.rs` | Done | Hover-triggered overlay |
| **Table widget** | `uitk/widgets/table.rs` | New/Done | Scrollable table with headers, column widths (fixed/flex), row selection, scrollbar |
| **Dialog widget** | `uitk/widgets/dialog.rs` | New/Done | Modal overlay with title, message, configurable buttons, icons (info/warning/error/question), dim backdrop |
| **Tree view widget** | `uitk/widgets/tree_view.rs` | New/Done | Collapsible tree with expand/collapse, connector lines, selection, scroll |
| **Undo/Redo system** | `uitk/undo.rs` | New/Done | Edit operation stack with merge (group rapid typing), 200-step history, insert/delete/replace ops |
| Geometry types | `geometry.rs` | Done | Point2D, Vec2D, Triangle2D, Quad2D |
| Content tracking | `content.rs` | Done | ContentId + TrackedContent for cache invalidation |
| Content hashing | `hash.rs` | Improved | **SipHash-2-4** content hashing (truncated to u64), replaced MD5 |
| **Theme system** | `stylesheet.rs` | New/Done | Dark + Light theme presets with `StyleSheet::dark_theme()`/`light_theme()` |
| **Anti-aliased drawing** | `drawing/primitives.rs` | New/Done | Wu's algorithm for smooth lines, thick AA lines |
| **Gradient fills** | `drawing/primitives.rs` | New/Done | Linear (4 directions) + radial gradient fills |
| **Animation framework** | `uitk/animation.rs` | New/Done | Easing functions (14 types), tweens with looping/ping-pong, spring physics |
| **Color utilities** | `lib.rs` | New/Done | `darken()`, `lighten()`, `mix()`, `with_alpha()`, `luminance()`, `contrast_text()` |
| **Circle/Ellipse primitives** | `drawing/primitives.rs` | New/Done | `draw_circle()`, `draw_circle_outline()`, `draw_ellipse()` with scanline rendering |
| **Unicode/Latin-1 fallback** | `drawing/text.rs` | Improved | **350+ Unicode→ASCII transliterations** (Latin Extended, Greek alphabet, geometric shapes, currency symbols, CJK fullwidth, superscript/subscript, miscellaneous symbols) + **pixel-level glyph rendering** for Box Drawing, Block Elements, Arrows, Geometric Shapes, Braille Patterns |
| Unicode/UTF-8 text | — | Done | Full glyph rendering for 500+ Unicode codepoints (box drawing, blocks, arrows, shapes, Braille 2×4 matrix) + extensive ASCII transliteration fallback |
| Proportional fonts | `drawing/proportional.rs` | New/Done | Variable-width text rendering from bitmap analysis, kerning pairs, justified layout |

### Guest Library (`guestlib/src/`)

| Component | Status | Details |
|-----------|--------|---------|
| Framebuffer registration | Done | `host_set_framebuffer` — shares pixel buffer with host |
| Input state query | Done | `host_get_input_state` — mouse/keyboard events, **safe read_unaligned FFI** |
| Window rect query | Done | `host_get_win_rect` — current window dimensions, **safe read_unaligned FFI** |
| Stylesheet query | Done | `host_get_stylesheet` — theme colors & fonts |
| Wall clock | Done | `host_get_time` — millisecond-precision timestamp |
| TCP connect | Done | `tcp_connect(ip, port)` |
| TCP read/write | Done | `tcp_read`, `tcp_write` with byte arrays |
| TCP close | Done | `tcp_close`, `tcp_may_send`, `tcp_may_recv` |
| **UDP bind/send/recv/close** | New/Done | `udp_bind`, `udp_send`, `udp_recv`, `udp_close` |
| **IPC messaging** | New/Done | `ipc_send`, `ipc_recv`, `ipc_pending` |
| **DNS resolution** | New/Done | `dns_resolve(hostname)` → IP address via kernel DNS |
| **TCP listen** | New/Done | `tcp_listen(port)`, `tcp_is_active(handle)` for server mode |
| **ICMP ping** | New/Done | `icmp_ping(ip)` → RTT in ms via kernel ICMP socket |
| **Notification API** | New/Done | `notify(title, message, level)` — push notifications to kernel |
| **App enumeration** | New/Done | `get_app_count()`, `get_app_name(index)` — list running apps |
| **System info** | New/Done | `get_uptime()`, `get_timer_ticks()` for system timing |
| **Random number generator** | New/Done | `get_random(buf)` for cryptographic-quality random bytes |
| **Wallpaper API** | New/Done | `set_wallpaper(id)`, `get_wallpaper()` — change/query wallpaper style |
| **Preferences API** | New/Done | `set_preference(key, val)`, `get_preference(key)`, `save_preferences()` — persistent settings |
| **Process kill API** | New/Done | `kill_app(name)` — terminate a running WASM application |
| **Security status API** | New/Done | `get_security_status()` — query NX/SMEP/SMAP enabled/supported state |
| **Audio API** | New/Done | `play_sound(SystemSound)`, `beep(freq, dur)` — PC Speaker + VirtIO Sound |
| **VFS API** | New/Done | `vfs_stat()`, `vfs_read_file()`, `vfs_write_file()`, `vfs_has_disk()`, `vfs_disk_free()` — unified FS access |
| **Scheduler stats API** | New/Done | `get_scheduler_task_count()`, `get_scheduler_stats()`, `get_cpu_queue_stats()` — per-app preemption monitoring |
| **Signal API** | New/Done | `signal_send()`, `signal_poll()`, `signal_pending()` — Unix-like inter-app signal delivery (v2.9) |
| **Pipe API** | New/Done | `pipe_create()`, `pipe_write()`, `pipe_read()`, `pipe_close_write()`, `pipe_close_read()`, `pipe_count()` — kernel-level byte-stream pipes (v2.9) |
| **Profiler API** | New/Done | `profiler_avg_frame_time()`, `profiler_total_frames()` — system performance metrics (v2.9) |
| **Environment variable API** | New/Done | `env_get()`, `env_set()`, `env_list()`, `env_count()` — per-app environment variable management (v3.3) |
| **Process table API** | New/Done | `proc_list()`, `proc_tree()`, `proc_count()`, `proc_getpid()` — Unix-like process introspection (v3.3) |
| **Event log API** | New/Done | `event_log_read()`, `event_log_count()` — structured system event journal (v3.3) |
| **Network stats API** | New/Done | `netstats_protocol()`, `netstats_interfaces()`, `netstats_connections()` — network statistics (v3.3) |
| **Shared memory API** | New/Done | `shm_create()`, `shm_destroy()`, `shm_attach()`, `shm_read()`, `shm_write()`, `shm_list()`, `shm_count()` — inter-app shared memory (v3.4) |
| **Resource limits API** | New/Done | `rlimit_get()`, `rlimit_set()`, `rlimit_summary()` — per-app resource limit management (v3.4) |
| **Service manager API** | New/Done | `service_list()`, `service_status()`, `service_start()`, `service_stop()`, `service_restart()`, `service_enable()`, `service_disable()`, `service_count()` — systemd-like service control (v3.4) |
| **ProcFS API** | New/Done | `procfs_read()`, `procfs_list()` — virtual /proc filesystem access (v3.4) |
| **Mount subsystem API** | New/Done | `mount_list()`, `mount_df()`, `mount_count()` — filesystem mount information (v3.4) |
| Logging | Done | `WasmLogger` to serial + console, level filtering |
| Fuel profiling | Done | `measure_fuel!` macro |
| Time profiling | Done | `measure_time!` macro |
| WASI fd_write (stdout) | Done | Redirects to `host_log` |
| WASI clock_time_get | Done | Returns wall clock |
| WASI random_get | Done | Pseudo-random via time-based seed |
| WASI environ_get | Improved | Returns per-app environment variables from EnvironmentManager (v3.3, was RUST_BACKTRACE=1 only) |
| WASI filesystem ops | Done | fd_read, fd_close, fd_seek, fd_filestat_get, path_open, etc. |
| WASI sched_yield | Done | Triggers preemption via PREEMPTION_PENDING atomic flag (v2.9) |
| WASI fd_sync | Done | Triggers FAT32 sync flush (v2.9) |
| WASI path_link | Done | Returns ENOTSUP (unsupported, v2.9) |
| WASI path_readlink | Done | Returns EINVAL (no symlinks, v2.9) |
| Clipboard API | Done | `clipboard_set`, `clipboard_get`, `clipboard_get_len` |
| WASI proc_exit | Done | Sets exit_requested flag, app lifecycle handles cleanup (v3.1) |
| WASI poll_oneoff | Done | Supports clock and FD subscriptions with proper event handling (v2.9) |
| Password Management API | Done | `host_change_password`, `host_has_password` with auth verification (v3.1) |
| Session Info API | Done | `host_get_boot_time`, `host_get_login_info` for uptime/user queries (v3.1) |

## Detailed Analysis — What's Done

### 1. Boot & Initialization
- UEFI-compliant boot as a single EFI binary (`bootx64.efi`)
- Proper exit from UEFI boot services while retaining runtime services (RTC)
- CPU frequency calibration via TSC + RTC measurement
- UEFI memory map parsing and largest-region heap allocation
- Improved panic handler with file/line/column diagnostics and HLT-based halt
- **IDT setup with 19 exception handlers and PIC-based hardware interrupts** (New)
- **Page frame allocator initialization from second-largest memory region** (New)
- **Virtual memory manager initialization** (New)
- **IPC mailbox registration for all apps** (New)
- **PCI hardware detection scan at boot** (New v2.0) — USB, NVMe, GPU, HDA, SATA
- **Built-in test suite execution at boot** (New v2.0) — 60+ tests, 9 subsystems
- **AP startup via INIT-SIPI-SIPI** (New v2.0) — multi-core initialization

### 2. Interrupt Handling (New)
- IDT (Interrupt Descriptor Table) with 19 CPU exception handlers
- Divide error, debug, NMI, breakpoint, overflow, bound range, invalid opcode
- Device not available, double fault, segment not present, stack segment fault
- General protection fault (with error code), page fault (with CR2 address)
- x87 floating point, alignment check, machine check, SIMD floating point
- PIC 8259 initialization (chained, offsets 32/40)
- Timer interrupt (IRQ0) with atomic tick counter
- Keyboard interrupt (IRQ1) for hardware keyboard events

### 3. Memory Management (Improved)
- **Thread-safe heap allocator** using `spin::Mutex` (was `UnsafeCell`) (New)
- **Bitmap-based page frame allocator** supporting up to 4 GiB (New)
  - Single page and contiguous multi-page allocation
  - Free-list hint optimization for O(1) best-case allocation
  - Statistics tracking (total, allocated, free)
- **Virtual memory manager (VMM)** (New)
  - `map_page()` / `unmap_page()` / `remap_page()` for page table manipulation
  - `alloc_and_map()` / `unmap_and_free()` for integrated frame + page management
  - `map_range()` / `unmap_range()` for contiguous regions
  - Guard page creation for stack overflow detection
  - Page translation and mapping queries

### 4. PCI Subsystem
- Full PCI configuration space enumeration (buses 0-255, devices 0-31)
- 32-bit and 64-bit memory-mapped BAR support
- Capability linked-list walking for VirtIO discovery
- BAR size determination via write-all-ones probing

### 5. VirtIO Driver Stack
- Complete VirtIO 1.1 PCI capability discovery and device initialization
- Virtqueue descriptor table management with free-list allocation
- Multi-descriptor chain support for complex messages
- **GPU**: 2D resource creation, backing attachment, scanout, full-framebuffer transfer
- **Input**: Pre-populated receive queue, event polling for 2 devices (keyboard + mouse)
- **Network**: MAC address negotiation, RX queue pre-population, packet send/receive

### 6. Network Stack (Improved)
- smoltcp-based TCP/IP implementation
- VirtioNetDevice adapter implementing smoltcp's `Device` trait
- TCP socket API: connect, read, write, close, state queries
- **UDP socket API: bind, send, recv, close** (New)
- **DHCP client for automatic IP configuration** with static fallback (10.0.2.15/24) (New)
- **16 simultaneous sockets** (up from 8) (Improved)
- Port allocation wraps around to avoid exhaustion
- **Kernel-level DNS resolver** with caching (64 entries, TTL-based expiry) (New v1.3)
- **TCP listen/accept for server mode** (New v1.3)
- **ICMP ping** with echo request/reply and RTT measurement (5s timeout) (New v1.5)

### 7. WASM Application Runtime (Improved)
- wasmi interpreter with fuel metering capability (100M/step)
- Full application lifecycle: launch -> step -> pause -> resume -> reload -> crash recovery
- 28+ host API functions across display, input, networking, time, debug
- WASI Preview1 filesystem APIs backed by real in-memory ramfs
- **UDP host APIs** (bind, send, recv, close) (New)
- **IPC host APIs** (send, recv, pending) (New)
- **DNS resolution host API** (New v1.3)
- **TCP listen/accept host API** (New v1.3)
- **System info host API** (uptime, timer ticks) (New v1.4)
- **Random number generator host API** (New v1.4)
- **ICMP ping host API** — send ping, measure RTT (New v1.5)
- **Notification host API** — push notification to kernel manager (New v1.5)
- **App enumeration host API** — query running app count and names (New v1.5)
- Per-app audit mode with frametime/memory/network graphs + console log

### 8. IPC — Inter-Process Communication (New)
- Mailbox-based message passing between WASM applications
- Named mailboxes registered per app
- Message structure: sender, type tag, binary data payload, timestamp
- Broadcast capability (send to all apps except sender)
- Queue with configurable max size (64 messages per mailbox)
- 4 KiB maximum message payload
- Guest library wrappers for easy use

### 9. Dynamic App Loading (New)
- App discovery from `/apps/` directory in ramfs
- JSON manifest format with app metadata (name, entry point, icon, dimensions)
- `install_app()` / `uninstall_app()` for managing apps
- `load_app_wasm()` for loading WASM binaries from filesystem
- Fallback defaults when manifest is missing

### 10. In-Memory Filesystem
- Full hierarchical filesystem stored in RAM
- Operations: create, read, write, seek, delete, rename, mkdir, rmdir, readdir, stat
- Per-app sandboxed directories (auto-created at boot)
- Shared `/tmp` directory and `/apps/` for dynamic apps
- WASI-compatible file descriptor management
- **Convenience path methods**: `open_path()`, `unlink_path()`, `rmdir_path()` (New)

### 11. Desktop GUI (Improved)
- Full graphical shell at 1366x768 resolution
- Radial pie menu system for app launching and window management
- Window decorations: titlebar, borders, resize handles, app icons
- **Titlebar buttons: minimize (—), maximize (□), close (×)** (New)
- Z-ordered window stack with drag and resize support
- **Window snapping: Ctrl+Alt+Left/Right for half-screen snap** (New)
- **Keyboard shortcuts**: Alt+F4 close, Alt+Tab cycle, Ctrl+Alt+arrows for maximize/minimize/snap (New)
- System topbar: FPS counter, frametime/heap/network monitors, UTC clock
- Software cursor rendering
- **Procedural wallpaper system** with 4 styles: DarkGrid, DeepBlue, NightSky, WavePattern (New v1.7)
- **Desktop shortcut icons** with double-click to launch apps, hover highlight, text shadow (New v1.7)
- **Enhanced taskbar** with hover tooltips and minimized window indicators (New v1.7)
- **Notification system** with click-to-dismiss and slide-in animation from right edge (Improved v1.7)
- **Lock screen** with large clock/date display, Ctrl+Alt+L to lock, auto-lock after 5 min idle, fade animation (New v1.7)

### 12. UI Toolkit (Improved)
- **18 widget types** with immediate-mode rendering (was 15)
- Content-ID-based tile caching system (20 MB LRU cache)
- Horizontal/vertical/grid layout engine
- Rich text with per-character styling, word-wrap, hyperlinks
- 4 font families at 6 sizes each (bitmap-rendered, alpha-blended)
- **Text selection** with anchor/cursor model (New)
- **Ctrl+A (select all), Ctrl+C (copy), Ctrl+X (cut), Ctrl+V (paste)** (New)
- **Home/End keys for cursor movement** (New)
- **Focus management system** with Tab/Shift+Tab navigation (New)
- **Dark and Light theme presets** (New)
- **Table widget** with headers, column sizing, row selection (New v1.3)
- **Modal dialog widget** with overlay, buttons, icons (New v1.3)
- **Tree view widget** with expand/collapse, connector lines (New v1.3)
- **Undo/Redo system** with edit merging (New v1.3)
- **Anti-aliased line drawing** via Wu's algorithm (New v1.3)
- **Gradient fills** — linear (4 directions) + radial (New v1.3)
- **Animation framework** — 14 easing functions, tweens with loop/ping-pong, spring physics (New v1.4)
- **Color utilities** — darken, lighten, mix, with_alpha, luminance, contrast_text (New v1.4)
- **Circle/ellipse primitives** — filled circle, circle outline, filled ellipse (New v1.4)
- **Rounded rect outline fix** — proper scanline-based outline rendering (Fixed v1.4)
- **Unicode/Latin-1 fallback** — 150+ transliterations for non-ASCII display (New v1.5)
- **Unicode pixel-level glyph rendering** — Box Drawing, Block Elements, Arrows, Geometric Shapes, Braille Patterns (New v2.0)

### 13. Input System (Improved)
- Full modifier key support: Shift, Ctrl, Alt, CapsLock
- Tab, Escape, F1-F12, Delete, Grave/Tilde
- **Home, End, Page Up, Page Down, Insert keys** (New)
- US QWERTY layout with all alphanumeric + symbol keys
- Arrow keys, mouse movement, scroll wheel, double-click detection
- **Touch/gesture input** (New v2.7): EV_ABS multi-touch protocol type B, 5 gesture types (Tap, Pan, Pinch, Swipe, TwoFingerScroll), 5 simultaneous touch slots

### 14. Applications (6 apps)
- **Settings**: **6-tab interface** (About/Appearance/System/Security/Keyboard/**Apps**), system info, theme preview, keyboard shortcut reference, **wallpaper picker**, **security status**, **App Store UI**, version info (Improved v2.0)
- **File Manager**: Directory browsing with table widget, 8-button toolbar, path bar, CRUD operations, keyboard navigation, back/forward history (New v1.6)
- **System Monitor**: 4-tab dashboard (Overview/Performance/Processes/**Scheduler**), real-time graphs, FPS counter, app list, fuel tracking, **per-app preemption stats + CPU queue stats** (Improved v2.2)
- **Terminal**: Full shell with 80+ commands, pipe support, aliases, clipboard paste, grep/find/tree/xxd, **layout/cursor/hwinfo/apidoc commands**, **passwd/w/who/uptime session commands** (Improved v3.1)
- **Text Editor**: Rich text editing with font/color/justification controls, undo/redo
- **Web Browser**: DNS resolution, TLS 1.2/1.3, HTTP/1.1, HTML block layout engine, **CSS engine with style/class/id selectors**, **PNG data URI image decoding**, **full JavaScript engine with DOM API, Array type, ternary/switch/typeof/new** (Improved v2.8)

> **Policy:** Knox OS is an AI/LLM-native platform. Trivial demo applications (calculators, 3D cubes, stopwatches, paint programs, etc.) are out of scope. All bundled apps must serve productivity, system management, or network/data purposes.

## Detailed Analysis — What's Remaining

### CRITICAL — Foundational OS Infrastructure

#### 1. Persistent File System (Priority: High)
**Current state:** ~~In-memory ramfs only.~~ ✅ **VirtIO Block driver + FAT32 filesystem implemented (v1.8)**.

**What is still needed:**
- ~~FAT32 or ext2 driver for persistent storage on VirtIO block device~~ ✅ Done (v1.8)
- ~~VirtIO Block device driver~~ ✅ Done (v1.8)
- ~~VFS (Virtual File System) abstraction layer to unify ramfs + FAT32~~ ✅ Done (v2.1)
- ~~File open/save dialogs in the shell~~ (terminal commands handle file I/O directly)
- ~~Persist ramfs state to disk on shutdown~~ ✅ Done (v2.3) — graceful shutdown with ramfs backup/restore
- ~~Integration with WASI filesystem operations for WASM apps~~ ✅ Done (v3.7) — path_open/mkdir/stat/rmdir/unlink route /disk paths through VFS to FAT32, fd_tell/fd_advise/fd_allocate added

#### 2. Preemptive Scheduling (Priority: High)
**Current state:** ~~Cooperative scheduling only.~~ ✅ **Preemptive scheduler framework implemented (v1.8)** with timer-based preemption tracking and priority levels.

**What is still needed:**
- ~~Timer-based preemption using PIT timer interrupt~~ ✅ Done (v1.8) — timer checks quantum expiry
- ~~Priority-based scheduling algorithm~~ ✅ Done (v1.8) — 4 priority levels with fairness
- ~~Wire preemption flag into WASM engine step() execution (fuel + time hybrid)~~ ✅ Done (v2.1)
- ~~Per-app preemption stats exposed to system monitor~~ ✅ Done (v2.2) — Scheduler tab with task/CPU stats
- ~~Async I/O for network operations~~ ✅ Done (v2.4) — non-blocking I/O framework with submit/poll/cancel

#### 3. Multi-Core / SMP Support (Priority: High)
**Current state:** ✅ **AP core discovery AND AP startup implemented (v2.0)**.

**What is still needed:**
- ~~AP discovery via ACPI MADT~~ ✅ Done (v1.8) — RSDP/RSDT/MADT parsing
- ~~AP startup via INIT-SIPI-SIPI sequence~~ ✅ Done (v2.0) — Local APIC + trampoline at 0x8000
- ~~Per-CPU run queues and core affinity~~ ✅ Done (v2.2) — 16 CPU queues, task affinity, least-loaded assignment
- ~~Cross-core IPI (Inter-Processor Interrupts) for scheduling~~ ✅ Done (v2.3) — schedule/TLB/halt IPIs
- ~~Work-stealing scheduler~~ ✅ Done (v2.2) — Rebalance between busiest/least-busy queues

### HIGH — Essential for Usability

#### 4. Security Model (Priority: High)
**Current state:** ✅ **Password authentication implemented (v1.8)** with SHA-256, brute-force protection, and lock screen integration.

**What is still needed:**
- ~~User authentication (password hashing)~~ ✅ Done (v1.8)
- ~~Lock screen~~ ✅ Done (v1.7)
- ~~User/group permission model~~ ✅ Done (v2.2) — Unix-like uid/gid, file permissions, root always-pass
- ~~Capability-based security for WASM apps~~ ✅ Done (v2.1)
- Memory protection (NX bit, SMEP, SMAP)
- ~~Encrypted password storage on FAT32 disk~~ ✅ Done (v2.1)

#### 5. Network Stack Improvements (Priority: Medium)
**What is still needed:**
- ~~DNS resolver at kernel level~~ ✅ Done (v1.3)
- ~~Listen/accept for TCP server mode~~ ✅ Done (v1.3)
- ~~ICMP support (ping)~~ ✅ Done (v1.5)
- ~~IPv6 support~~ ✅ Done (v1.8) — IPv6 link-local + smoltcp proto-ipv6
- ~~DHCPv6 or SLAAC for full IPv6 auto-configuration~~ ✅ Done (v2.2) — SLAAC with EUI-64 + DHCPv6 info-request
- ~~IPv6 default gateway routing~~ ✅ Done (v2.3) — gateway route from SLAAC/RA

#### 6. Input Improvements (Priority: Low)
**What is still needed:**
- ~~International keyboard layouts~~ ✅ Done (v2.5) — 9 layouts (US/UK/DE/FR/ES/IT/SE/Dvorak/Colemak)
- ~~Drag-and-drop framework~~ ✅ Done (v2.5) — DragPayload types, DropZone registration, cursor tracking
- ~~Touch/gesture input support~~ ✅ Done (v2.7) — EV_ABS multi-touch protocol, 5 gesture types (tap, pan, pinch, swipe, scroll)

### MEDIUM — Quality of Life

#### 7. Text & Font System (Priority: Medium)
- ~~Unicode / UTF-8 support~~ ✅ Complete (v2.0) — **350+ transliterations + pixel-level glyph rendering** (Box Drawing, Block Elements, Arrows, Geometric Shapes, Braille Patterns)
- ~~Proportional (variable-width) font rendering~~ ✅ Already Done — proportional.rs (343 lines) with kerning, metrics cache, variable-width rendering
- ~~TrueType / OpenType vector font rasterization~~ ✅ Done (v2.5) — FontManager with table parsing, glyph rasterization, scanline fill, BTreeMap cache
- ~~Text shaping (ligatures, kerning)~~ ✅ Done (v2.7) — LigatureRule detection (fi/fl/ffi/ffl/ft), ShapedGlyph output, shape_text() API

#### 8. UI Toolkit Enhancements (Priority: Medium)
**What is still needed:**
- ~~Table / list view, Modal / dialog, Tree view widgets~~ ✅ Done (v1.3)
- ~~Anti-aliased drawing primitives~~ ✅ Done (v1.3)
- ~~Gradient fills~~ ✅ Done (v1.3)
- ~~Animation framework~~ ✅ Done (v1.4)

#### 9. Window Management Enhancements (Priority: Low)
- ~~Partial framebuffer updates (dirty-rect tracking)~~ ✅ Done (v2.2) — DirtyTracker + GPU flush_rect
- ~~Window group/workspace support~~ ✅ Done (v2.4) — 4 virtual desktops with window assignment
- ~~Wallpaper system~~ ✅ Done (v1.7)
- ~~Desktop icons~~ ✅ Done (v1.7)
- ~~Taskbar tooltips~~ ✅ Done (v1.7)
- ~~Lock screen~~ ✅ Done (v1.7), **enhanced with password auth (v1.8)**

### LOW — Nice to Have

#### 10. Audio Support
**Current state:** ✅ **PC Speaker audio implemented (v1.8)** with 7 system sounds.

**What is still needed:**
- ~~Basic audio output~~ ✅ Done (v1.8) — PC Speaker via PIT Channel 2
- ~~VirtIO Sound driver for PCM audio~~ ✅ Done (v2.2) — AudioMixer + VirtIO device discovery
- ~~Audio mixing and multi-channel playback~~ ✅ Done (v2.2) — 8-stream stereo mixer with volume control
- ~~Guest API for WASM apps to play sounds~~ ✅ Done (v2.1)

#### 11. GPU Improvements
- ~~Dynamic resolution support~~ ✅ Done (v2.3) — runtime display dimension detection
- ~~Hardware cursor~~ ✅ Done (v2.5) — 10 cursor shapes, 64×64 RGBA, procedural rendering
- ~~Partial screen updates (dirty rectangles)~~ ✅ Done (v2.2) — DirtyTracker + flush_rect

#### 12. Real Hardware Support
- ~~UEFI GOP framebuffer fallback~~ ✅ Done (v2.5) — GopFramebuffer with mode enumeration, RGB/BGR/BitMask pixel formats
- ~~PS/2 keyboard/mouse drivers~~ ✅ Done (v2.5) — Scancode set 2, 3-button mouse + scroll, fallback driver
- USB HCI drivers (xHCI) — **detected via PCI scan (v2.0)**, **controller init with BAR reads + capability parsing (v2.7)**, **I/O command structures + TRB builders (v2.8)**
- NVMe / AHCI storage drivers — **detected via PCI scan (v2.0)**, **controller init with version/queue/port enumeration (v2.7)**, **I/O command structures + FIS builders (v2.8)**
- Intel GPU modesetting — **detected via PCI scan (v2.0)**, **display pipeline register probe (v2.8)**

#### 13. Accessibility
- Keyboard-only navigation (partially done via focus management, **enhanced with focus ring + Tab navigation announcements (v2.8)**)
- ~~High-contrast theme (light theme available)~~ ✅ Done (v2.4) — High-contrast theme in theme engine
- ~~Font size scaling~~ ✅ Done (v2.3) — 4-level system-wide font scaling (Small/Normal/Large/XL)
- ~~Screen reader~~ ✅ Improved (v2.8) — Announcement queue with priority, serial output, rate limiting, **ARIA role/state/live-region support, focus ring navigation, heading/list/progress/tooltip announcements**
- ~~International keyboard layouts~~ ✅ Done (v2.5) — 9 layouts accessible via terminal `layout` command

#### 14. Developer Experience
- ~~Unit test framework~~ ✅ Done (v2.0) — 60+ built-in runtime tests across 9 subsystems
- ~~CI/CD pipeline~~ ✅ Done (v2.5) — GitHub Actions: fmt → build → test → disk-image → docs → smoke-test
- ~~API documentation generation~~ ✅ Done (v2.5) — Runtime ApiDocRegistry with 11 module docs, search/list/format
- ~~App development SDK and template~~ ✅ Done (v2.7) — Template project, create_app.sh scaffolder, comprehensive README

## Application Status

### Settings — New (v1.7)
| Feature | Status |
|---------|--------|
| **Seven-tab interface (About, Appearance, System, Security, Keyboard, Apps, Power)** | Improved |
| About tab: version, kernel, runtime, WASM engine, network info | Done |
| About tab: app count and uptime display (auto-refresh) | Done |
| Appearance tab: 4 theme previews (Dark, Light, Solarized, Monokai) | Done |
| Appearance tab: color swatch display for each theme | Done |
| Appearance tab: click to select theme (preview-only) | Done |
| **Appearance tab: wallpaper picker with 6 styles** | Done |
| **Appearance tab: click to change wallpaper (persistent)** | Done |
| System tab: hardware info (architecture, CPU, memory, display, input, network) | Done |
| System tab: running applications list with count | Done |
| **Security tab: NX/SMEP/SMAP status with visual indicators** | Done |
| **Security tab: security model overview** | Done |
| Keyboard tab: 15 keyboard shortcuts reference | Done |
| **Apps tab: installed app listing with icons and descriptions** | New/Done |
| **Apps tab: "Installed" badges and app metadata** | New/Done |
| **Apps tab: App Store UI for managing applications** | New/Done |
| **Power tab: system uptime display** | New/Done |
| **Power tab: Sync Filesystems button (flush data to disk)** | New/Done |
| **Power tab: Restart System button (graceful reboot)** | New/Done |
| **Power tab: Shut Down button (graceful shutdown)** | New/Done |
| **Power tab: system power info (ACPI, reboot method, persistence)** | New/Done |
| Status bar with version info | Done |
| Settings icon in activity bar | Done |

### File Manager — New (v1.6)
| Feature | Status |
|---------|--------|
| Directory browsing with table widget | Done |
| Toolbar (Back, Up, Refresh, New File/Dir, Delete, Rename) | Done |
| Path bar showing current directory | Done |
| Status bar (file/dir counts) | Done |
| Create new files and folders | Done |
| Delete files and folders | Done |
| Rename files (copy+delete pattern) | Done |
| Keyboard navigation (Up/Down/Enter/Backspace/Delete) | Done |
| Back/forward navigation history | Done |
| Input overlay for file/folder names | Done |
| Sorted entries (dirs first, then files alphabetically) | Done |

### System Monitor — New (v1.6)
| Feature | Status |
|---------|--------|
| Five-tab interface (Overview, Performance, Processes, Network, Scheduler) | Improved |
| System uptime display | Done |
| Running app count & list | Done |
| Real-time FPS counter | Done |
| Frame time graph (history) | Done |
| WASM fuel consumption graph | Done |
| Performance statistics (avg/max) | Done |
| Process list with PID and status | Done |
| **Network tab: protocol statistics (TCP/UDP/ICMP/DNS)** | New/Done |
| **Network tab: network interface listing** | New/Done |
| **Network tab: active connections with state colors** | New/Done |
| **Network tab: auto-refresh every 2 seconds** | New/Done |

### Terminal (Rust) — Functional with Limitations
| Feature | Status |
|---------|--------|
| Built-in shell (Rust, WASM) | Done |
| Stdout/stderr capture | Done |
| Filesystem access via WASI | Done |
| **Command history (up/down)** | New/Done |
| **Tab completion** | New/Done |
| **Built-in shell** | New/Done |
| **Filesystem commands (ls, cat, mkdir, touch, rm, rmdir, cd, cp, mv, write, stat, head, tail, wc)** | New/Done |
| **Network commands (dns, ping with real ICMP)** | Improved/Done |
| **System commands (rand, ticks, sleep, notify, apps)** | New/Done |
| **Security / wallpaper / killapp commands** | New/Done |
| **Text processing (grep, sort, uniq, rev, xxd)** | New/Done |
| **File search (find, tree)** | New/Done |
| **Pipe support (cmd1 \| cmd2 \| cmd3)** | New/Done |
| **Command aliases (alias, unalias, which)** | New/Done |
| **Clipboard paste (Ctrl+V)** | New/Done |
| **Enhanced ls -l (permissions, sizes, types)** | New/Done |
| **Utility commands (seq, basename, dirname, yes)** | New/Done |
| **Dynamic CWD prompt** | New/Done |
| **Line editing (Ctrl+A/E/U/K/W, cursor keys, Home/End, Delete)** | New/Done |
| **neofetch, colors, env, export, history** | New/Done |
| **signal, pipe, profile commands** | New/Done |

### Text Editor — Functional with Limitations
| Feature | Status |
|---------|--------|
| Rich text display + editing | Done |
| Font/size/color selection | Done |
| **Clipboard copy/paste (Ctrl+C/V)** | New/Done |
| **Text selection** | New/Done |
| **Undo/Redo (Ctrl+Z / Ctrl+Y)** | New/Done |
| File open/save (Ctrl+O/S) | Done |
| File open/save dialog | Done (v2.6) — Browse directories, select files, keyboard navigation |

### Web Browser — Impressive but Limited
| Feature | Status |
|---------|--------|
| DNS resolution over TCP | Done |
| HTTPS with TLS 1.2/1.3 | Done |
| HTTP/1.1 with chunked transfer | Done |
| HTML parsing + block layout | Done |
| Clickable links + scrolling | Done |
| URL bar with navigation | Done |
| CSS support | Done | **CSS engine**: `<style>` blocks + inline styles, tag/class/id selectors, 18 properties (color, background, font-size, margin/padding, display, width/height, border, text-align, font-weight), 20+ named colors, #hex, rgb/rgba, px/em/rem/pt/% units |
| Images | Improved | **PNG data URI decoding** (base64 inline images), nearest-neighbor scaling, RGBA pixel blitting, gray placeholder for unsupported formats |
| JavaScript | Improved | **Full JS engine** with tokenizer + interpreter: variables, arithmetic, string concatenation, `document.write()`/`document.title`, `console.log()`, `alert()`, `Math.*`, `parseInt/parseFloat`, `if/else/for/while/do-while/switch-case`, string methods, user-defined functions, **DOM API** (getElementById/querySelector/createElement/appendChild/etc.), **Array type** with methods (push/pop/shift/join/indexOf/slice/concat/sort), **ternary operator**, **typeof/new keywords**, **JSON.stringify/parse**, **Object.keys/values/entries** |

## Build System Status

| Component | Status | Details |
|-----------|--------|---------|
| `make.py` (primary) | Done | Incremental builds, format/fix/clean commands, **6 WASM apps** |
| `run.sh` (legacy) | Done | Simple sequential build + QEMU launch |
| `make_font.py` | Done | TTF to bitmap PNG + JSON spec generator |
| `make_charmap.py` | Done | Linux keycode to Rust enum generator |
| WASM target | Done | `wasm32-wasip1` with release optimizations |
| Kernel target | Done | `x86_64-unknown-uefi` with LTO + strip |
| QEMU configuration | Done | VirtIO devices, KVM, UEFI firmware, 1 GB RAM |
| Cross-platform build | Done (v2.6) | Auto-detect macOS/Linux, dynamic toolchain, QEMU accel (hvf/kvm/tcg), mkfs command |
| Unit tests | Done | **60+ built-in runtime tests** across 9 subsystems (memory, geometry, color, filesystem, IPC, security, preferences, scheduler, allocator) |
| CI/CD | Done | **GitHub Actions pipeline: fmt → build → test → disk-image → docs → QEMU smoke-test** (v2.5) |

## Roadmap to Enterprise-Grade

### Phase 1: Foundation — ✅ Largely Complete

> **Goal:** Make Knox OS a real operating system with core primitives

1. ~~**Interrupt handling**~~ — ✅ Done: IDT, 19 exception handlers, timer + keyboard interrupts
2. ~~**Virtual memory**~~ — ✅ Done: Page frame allocator + VMM with map/unmap/remap/guard
3. ~~**In-memory filesystem**~~ — ✅ Done: ramfs with WASI API implementation
4. **Preemptive scheduling** — Partial: fuel limits done, timer interrupt exists
5. ~~**Thread-safe allocator**~~ — ✅ Done: spin::Mutex-based locking
6. ~~**Proper panic/crash handling**~~ — ✅ Done: Diagnostic panic handler with HLT

### Phase 2: Usability — ✅ Substantially Complete

> **Goal:** Make Knox OS comfortable to use

7. ~~**Dynamic app loading**~~ — ✅ Done: Filesystem-based discovery + manifest support
8. ~~**Clipboard system**~~ — ✅ Done: System-wide text clipboard
9. ~~**Text selection**~~ — ✅ Done: Full selection with Ctrl+C/V/X/A
10. ~~**Keyboard improvements**~~ — ✅ Done: All modifier keys, Home/End/PgUp/PgDn, F-keys
11. ~~**Network config**~~ — ✅ Done: DHCP + UDP support, 16 sockets, **DNS resolver**
12. ~~**Window management**~~ — ✅ Done: Titlebar buttons, keyboard shortcuts, snapping
13. ~~**Unicode support**~~ — ✅ Done (v2.0) — Pixel-level glyph rendering for box drawing, blocks, arrows, shapes, Braille
14. ~~**IPC mechanism**~~ — ✅ Done: Mailbox-based message passing
15. ~~**Theme system**~~ — ✅ Done: Dark/Light presets
16. ~~**Focus management**~~ — ✅ Done: Tab/Shift+Tab navigation
17. ~~**Undo/Redo**~~ — ✅ Done: 200-step history with edit merging (v1.3)
18. ~~**Terminal shell**~~ — ✅ Done: 30+ commands, history, tab completion (v1.3)
19. ~~**AA drawing & gradients**~~ — ✅ Done: Wu's lines + linear/radial gradients (v1.3)
20. ~~**Animation framework**~~ — ✅ Done: 14 easing functions, tweens, spring physics (v1.4)
21. ~~**System APIs**~~ — ✅ Done: Uptime, timer ticks, RNG via host APIs (v1.4)
22. ~~**Drawing primitives**~~ — ✅ Done: Circle, ellipse, circle outline, rounded rect outline fix (v1.4)
23. ~~**ICMP ping**~~ — ✅ Done: Real echo request/reply with RTT measurement via smoltcp (v1.5)
24. ~~**Notification API**~~ — ✅ Done: Apps can push notifications to kernel notification manager (v1.5)
25. ~~**App enumeration API**~~ — ✅ Done: Query running app list from WASM apps (v1.5)
26. ~~**Unicode fallback**~~ — ✅ Done: 150+ Unicode→ASCII transliterations for Latin Extended, symbols (v1.5)
27. ~~**Terminal power features**~~ — ✅ Done: 45+ commands, pipes, aliases, clipboard paste, grep/find/tree/xxd (v1.5)
28. ~~**File Manager & System Monitor apps**~~ — ✅ Done (v1.6)
29. ~~**Wallpaper system**~~ — ✅ Done: 4 procedural styles (DarkGrid, DeepBlue, NightSky, WavePattern) with caching (v1.7)
30. ~~**Desktop icons**~~ — ✅ Done: Shortcut icons with double-click launch, hover highlight (v1.7)
31. ~~**Lock screen**~~ — ✅ Done: Ctrl+Alt+L, 5min auto-lock, clock/date display, fade animation (v1.7)
32. ~~**Taskbar enhancements**~~ — ✅ Done: Hover tooltips, minimized window dot indicator (v1.7)
33. ~~**Notification improvements**~~ — ✅ Done: Click-to-dismiss, slide-in animation (v1.7)
34. ~~**Settings app**~~ — ✅ Done: About/Appearance/System/Security/Keyboard/Apps/Power tabs, theme preview, power management (v1.7, v3.8)

### Phase 3: Enterprise Features (Est. 6-12 months)

17. ~~**Persistent filesystem**~~ — ✅ Done: VirtIO block + FAT32 (v1.8)
18. ~~**User authentication**~~ — ✅ Done: Login screen, password auth, SHA-256 (v1.8)
19. ~~**Security hardening**~~ — ✅ Done: NX via EFER.NXE, SMEP via CR4.20, SMAP via CR4.21, CPUID detection (v1.9)
20. **Multi-core support** — ✅ Done: SMP discovery + **AP startup (v2.0)** + **per-CPU run queues + work-stealing (v2.2)**
21. **App packaging** — Improved: Manifest format done, installer done, **App Store UI done (v2.0)**
22. **Accessibility** — ✅ Done (v2.5): Focus management, high contrast theme, screen reader, font scaling, **international keyboard layouts**
23. ~~**Testing & CI**~~ — ✅ Done (v2.5): 60+ built-in runtime tests, serial output for CI, **GitHub Actions CI/CD pipeline**
24. ~~**Persistent user preferences**~~ — ✅ Done: Key=value store on FAT32, auto-load/save (v1.9)
25. ~~**Wallpaper picker**~~ — ✅ Done: 6 procedural styles, runtime switching, Settings UI (v1.9)
26. ~~**Process management**~~ — ✅ Done: App kill via terminal + Settings, killapp command (v1.9)

### Phase 4: Hardware & Polish (Est. 6-12 months)

24. **Real hardware drivers** — Improved: **PCI hardware detection done (v2.0)** (USB/NVMe/GPU/HDA/SATA identified), **PS/2 keyboard/mouse driver (v2.5)**, **UEFI GOP framebuffer (v2.5)**, **USB/NVMe/AHCI controller init (v2.7)**, **NVMe/AHCI/USB I/O command structures + Intel GPU modesetting probe (v2.8)**, **BlockDevice trait + NVMe/AHCI block I/O + device registry (v3.7)**
25. **Audio support** — ✅ Done: PC Speaker (v1.8) + **VirtIO Sound driver + PCM mixer (v2.2)**
26. **Advanced networking** — ✅ IPv6 link-local done (v1.8), **SLAAC + DHCPv6 done (v2.2)**, ~~firewall + server mode remaining~~ ✅ Firewall done (v2.4) — stateless packet filter with default rules
27. **Web browser improvements** — Improved: **CSS engine done (v2.0)**, **PNG data URI decoding (v2.7)**, **minimal JS engine (v2.7)**, **full DOM/JS engine with Array/Element types, ternary, switch/case, new keyword, typeof (v2.8)**
28. **Full theming engine** — ✅ Done (v2.4): **6-theme engine** (Dark/Light/Solarized/Monokai/HighContrast/Nord), user-customizable, wallpaper picker (v1.9), **App Store UI done (v2.0)**
29. **App development SDK** — ✅ Done (v2.7): Template project, scaffolding script, comprehensive documentation

## Priority Matrix

| Priority | Impact | Effort | Items |
|----------|--------|--------|-------|
| ~~P0~~ | ~~High~~ | ~~High~~ | ~~Interrupt handling, Virtual memory~~ ✅ |
| ~~P0~~ | ~~High~~ | ~~High~~ | ~~Persistent filesystem (VirtIO block)~~ ✅ (v1.8) |
| ~~P1~~ | ~~High~~ | ~~Medium~~ | ~~Security model, Preemptive scheduling~~ ✅ Security done (v1.9), Scheduler partial |
| ~~P1~~ | ~~Medium~~ | ~~Medium~~ | ~~Text selection, Window management, DHCP, UDP~~ ✅ |
| ~~P2~~ | ~~Medium~~ | ~~Medium~~ | ~~Unicode~~, ~~Remaining UI widgets (table, dialog, tree)~~ ✅ (widgets done v1.3) |
| P2 | Medium | Medium | Unicode support — ✅ Done (v2.0) |
| ~~P2~~ | ~~Medium~~ | ~~Low~~ | ~~IPC, Focus management, Theme system~~ ✅ |
| ~~P2~~ | ~~Medium~~ | ~~Medium~~ | ~~DNS resolver, TCP listen, Undo/Redo, AA drawing, Gradients, Terminal shell~~ ✅ (v1.3) |
| ~~P2~~ | ~~Medium~~ | ~~Low~~ | ~~ICMP ping, Notification API, App enumeration, Unicode fallback~~ ✅ (v1.5) |
| ~~P2~~ | ~~Medium~~ | ~~Medium~~ | ~~Terminal power features (pipes, aliases, grep, find, tree, xxd)~~ ✅ (v1.5) |
| ~~P2~~ | ~~Medium~~ | ~~Medium~~ | ~~File Manager app, System Monitor app~~ ✅ (v1.6) |
| ~~P2~~ | ~~Medium~~ | ~~Medium~~ | ~~Wallpaper, Desktop icons, Lock screen, Taskbar tooltips, Notification animations, Settings app~~ ✅ (v1.7) |
| ~~P2~~ | ~~Medium~~ | ~~Medium~~ | ~~Security hardening (NX/SMEP/SMAP), Wallpaper picker, Persistent preferences, Process kill~~ ✅ (v1.9) |
| ~~P3~~ | ~~Low~~ | ~~High~~ | ~~Multi-core (AP startup)~~, Real hardware (detection done, drivers remaining), Audio ✅ SMP + Hardware detection (v2.0) |
| ~~P3~~ | ~~Low~~ | ~~Medium~~ | ~~Animation framework~~ ✅ (v1.4) |
| ~~P3~~ | ~~Low~~ | ~~Medium~~ | ~~Testing~~ ✅ (v2.0) — 60+ built-in tests |
| ~~P3~~ | ~~Low~~ | ~~Medium~~ | ~~Accessibility~~ ✅ (v2.4) — screen reader + high-contrast theme |
| ~~P3~~ | ~~Low~~ | ~~Medium~~ | ~~Input improvements + HW cursor + UEFI GOP + PS/2 + CI/CD + API docs + TrueType~~ ✅ (v2.5) |
| ~~P3~~ | ~~Low~~ | ~~Medium~~ | ~~Tech debt cleanup (unsafe statics, unwraps, MD5, transmute, frame limiter, DMA docs, GPU resolution) + Touch input + Text shaping + Image decoding + JS engine + SDK~~ ✅ (v2.7) |
| ~~P3~~ | ~~Low~~ | ~~Medium~~ | ~~Full DOM/JS engine + NVMe/AHCI/USB I/O structures + Intel GPU probe + ARIA accessibility~~ ✅ (v2.8) |
| ~~P3~~ | ~~Low~~ | ~~Medium~~ | ~~Signal manager + Kernel pipes + System profiler + Slab allocator + WASI stub fixes~~ ✅ (v2.9) |

## Known Technical Debt

| Issue | Location | Severity | Status |
|-------|----------|----------|--------|
| ~~`unsafe` mutable statics in WASM apps~~ | `wasm_apps/*/src/main.rs` | ~~Medium~~ | Fixed (v2.7) — thread_local + RefCell |
| ~~`unwrap()`/`expect()` used pervasively~~ | Throughout kernel | ~~High~~ | Fixed (v2.7) — 112 bare unwraps replaced with descriptive expects or Result |
| ~~MD5 used for content hashing~~ | `applib/src/hash.rs` | ~~Low~~ | Fixed (v2.7) — SipHash-2-4 |
| ~~Hardcoded GPU resolution (1366x768)~~ | `kernel/src/virtio/gpu.rs` | ~~Medium~~ | Fixed (v2.7) — dynamic via GET_DISPLAY_INFO |
| ~~Hardcoded network config (10.0.2.15)~~ | `kernel/src/network/mod.rs` | ~~Medium~~ | Fixed (DHCP) |
| ~~Single socket limit~~ | `kernel/src/network/mod.rs` | ~~High~~ | Fixed (16 sockets) |
| ~~PCI single-function only~~ | `kernel/src/pci.rs` | ~~Low~~ | Fixed (v2.6) — multi-function device support |
| ~~Spin-delay frame limiter~~ | `kernel/src/main.rs` | ~~Medium~~ | Fixed (v2.7) — HLT-based delay |
| ~~DMA buffers leaked (never freed)~~ | `kernel/src/virtio/mod.rs` | ~~Low~~ | Documented (v2.7) — intentional: hardware requires stable DMA addresses for device lifetime |
| ~~No log level filtering~~ | `kernel/src/logging.rs` | ~~Low~~ | Fixed |
| ~~`transmute` for FFI structs (fragile)~~ | `guestlib/src/lib.rs` | ~~High~~ | Fixed (v2.7) — replaced with safe read_unaligned |
| ~~Fuel limit u64::MAX (no limit)~~ | `kernel/src/wasm/mod.rs` | ~~Medium~~ | Fixed (100M) |
| ~~Port allocation never wraps~~ | `kernel/src/network/mod.rs` | ~~Low~~ | Fixed |
| ~~DNS query ID hardcoded to 0x0001~~ | `wasm_apps/web_browser/src/dns.rs` | ~~Medium~~ | Fixed (v2.6) — randomized via RNG |
| ~~DNS query ID hardcoded~~ | `kernel/src/network/dns.rs` | ~~Medium~~ | Fixed (random ID v1.3) |
| ~~Thread-unsafe allocator~~ | `kernel/src/allocator.rs` | ~~High~~ | Fixed (spin::Mutex) |

## TODOs Found in Source Code

| File | TODO | Status |
|------|------|--------|
| ~~`kernel/src/virtio/mod.rs`~~ | ~~Prevent queue double-init~~ | Fixed (v2.6) — bitmask tracking |
| `kernel/src/virtio/gpu.rs` | Several commented-out functions | Open |
| ~~`kernel/src/virtio/network.rs`~~ | ~~Proper endianness on VirtioNetHdr~~ | Fixed (v2.6) — documented as correct on x86_64 LE |
| ~~`kernel/src/wasm/mod.rs`~~ | ~~Cleaner StepContext handling~~ | Fixed (v2.6) — safety documentation |
| ~~`kernel/src/wasm/mod.rs`~~ | ~~clock_time_get 1e9 multiplier~~ | Fixed (v2.6) — corrected to 1e6 (ms→ns) |
| ~~`kernel/src/topbar.rs`~~ | ~~Dynamic TOPBAR_GAP computation~~ | Resolved — refactored to status bar, constant no longer exists |
| ~~`kernel/src/logging.rs`~~ | ~~enabled() always returns true~~ | Fixed |
| ~~`applib/src/drawing/mod.rs`~~ | ~~Use Point2D everywhere~~ | Fixed (v2.6) — removed stale comment |
| ~~`applib/src/uitk/mod.rs`~~ | ~~Move code elsewhere~~ | Fixed (v2.6) — documented as begin-of-frame housekeeping |
| ~~`guestlib/src/lib.rs`~~ | ~~enabled() always returns true~~ | Fixed |