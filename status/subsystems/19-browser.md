← [Status index](../README.md) · [Do this first](../next.md)

# 19. Browser (Chromium / Vivaldi)

**Grade: Wired (100%)** · Priority: **High** · Inventory: [remaining.md §19](../remaining.md)

A Linux-like browser means a **real Chromium ELF** in Ring 3 (Ozone/Wayland), not the in-kernel GPUI HTML-to-text viewer. Vivaldi is that same engine plus its own UI — land Chromium first, then swap the wrapper.

**Honesty.** Start-menu Browser prefers `/opt/vivaldi/vivaldi-bin` (`GATE_VIV ui`) then `/opt/chromium/chrome` (`GATE_CHR launch`). GPUI HTML-to-text (`kernel/src/gui/browser.rs`) is the fallback only when those ELFs are missing. Slice 8 drops `--no-sandbox`. Slice 9 installs a real `vivaldi-stable` `.deb`. `vivaldi::launch` uses `spawn_elf` scheduler PIDs. Tick a row only when QEMU meets **Done when**.

**Strategy.** Official Linux x86_64 Chromium (or a rebuilt Chromium with `use_gtk=false`) on the ext4 root. First paint: `--single-process --no-sandbox --ozone-platform=wayland --use-gl=swiftshader`. Multi-process, GPU, sandbox, then Vivaldi `.deb`. Do not grow the in-kernel viewer into Blink. Do not rebuild Chromium against musl.

Related: [07-gui.md](07-gui.md) (Wayland/Ozone), [13-binary-compat.md](13-binary-compat.md) (glibc loader), [09-security.md](09-security.md) (seccomp/namespaces), [10-services.md](10-services.md) (D-Bus portals), [05-networking.md](05-networking.md) (userspace TLS), [06-drivers.md](06-drivers.md) (DRM/GBM).

---

### Live today (not Chromium)

- [x] In-kernel GPUI Browser — URL bar, tabs, `http_get` then tag-stripped lines; `vivaldi://` / `knoxos://` chrome pages
- [x] Ring 3 64×64 `browser.elf` SHM KAT (`GATE_F11`) — magic pixel, not an engine
- [x] Phase 19 init — `dbus` / `dpkg` / `fontconfig` / `xdg` / `pulseaudio` / `chromium_sandbox` / `glibc_compat` / `vivaldi::init()` register tables and print “ready”
- [x] Slice 3 Ozone/Wayland SHM + Linux `/proc` `/dev` `/etc` / XDG (`GATE_CHR wlshm` / `xdgshell` / `wlseat` / `wloutput` / `wldata` / `textinput` / `paint` / `proc` / `sysctl` / `dev` / `etc` / `xdg`)
- [x] Slice 4 userspace HTTPS — off-box DNS, NSS/CA, TLS record, h2, cookies, CSS/JS paint (`GATE_CHR dns` / `nss` / `tlsrec` / `h2` / `cookies` / `https`)
- [x] Slice 5 software GL — Ring 3 DRM `drmGetVersion`, SwiftShader GLES2, dmabuf PRIME, `chrome://gpu` CSS/JS (`GATE_CHR drm` / `egl` / `dmabuf` / `virgl` / `gpuinfo`)
- [x] Slice 6 fonts/ICU/media — real `icudtl.dat`, fontconfig+FreeType, `libffmpeg.so`, Pulse native, ALSA PCM, local MP4 video+audio (`GATE_CHR icu` / `fontconfig` / `ffmpeg` / `pulse` / `alsa` / `media`)
- [x] Slice 7 desktop — session `Hello`, tray notify, XDG portals, `xdg-open`, CUPS flag, start-menu exec (`GATE_CHR dbus` / `notify` / `portal` / `mime` / `cups` / `launch`)
- [x] Slice 8 sandbox — `chrome-sandbox` helper, Landlock, GPU sandbox, Crashpad, `chrome://sandbox` (`GATE_CHR sandbox` / `landlock` / `gpusandbox` / `crashpad` / `sandboxui`)
- [x] Slice 9 Vivaldi — live dpkg `vivaldi-stable` `.deb`, UI resources, sync TLS error, Widevine disabled (`GATE_VIV deb` / `ui` / `sync` / `cdm`)
- [x] Leftover glibc loader + Mojo — PT_TLS `%fs`, DT_NEEDED ELFs, skip-GTK, host ldd, no `--single-process` (`GATE_CHR tls` / `needed` / `ozone` / `ldd` / `mojo`)
- [x] Slice 10 polish — unpacked extension, getUserMedia, Downloads, DevTools, incognito, HiDPI scale 2, WebGPU (`GATE_CHR extensions` / `webrtc` / `download` / `devtools` / `profiles` / `hidpi` / `webgpu`)
- [x] Leftover Ring 3 `io_uring` — `io_uring_setup(8)` ring fd + NOP CQE (`GATE_CHR iouring`); `userfaultfd` stays ENOSYS

### Stub (delete or replace as the live rows land)

- [x] Fake `vivaldi::launch` process tree — **done**: `spawn_elf` scheduler PIDs (`GATE_VIV deb`)
- [x] Empty `seccomp(SECCOMP_SET_MODE_FILTER)` from Ring 3; sandbox BPF built in memory and not attached — **done** (`GATE_CHR seccomp`)
- [x] Virtual GTK/NSS/GBM packages and wget ELF-header stubs — **done**: real DT_NEEDED ELFs (`GATE_CHR needed` / `ldd`); skip-GTK (`GATE_CHR ozone`)
- [x] `glibc_compat::__libc_start_main` returns 0 without calling `main` — **done**: Ring 3 `/lib/libc.so.6` does call `main` (`GATE_CHR glibc`)
- [x] `/lib64/ld-linux-x86-64.so.2` is a loadable ELF interpreter (`GATE_CHR ldso`); shebang stub gone

---

## Phase 0 — Policy, binary, failing boot

Implement first. Done when serial shows a real ELF entry, not a fake PID.

- [x] **Lift from later-list** — Chromium/Vivaldi is this subsystem, not [Do not start](../next.md#do-not-start). **Done when:** this file is the backlog; `next.md` points here.
- [x] **Real Chromium-shaped tree on VFS** — `/opt/chromium/chrome` ELF64, ICU `0xDA27` `icudtl.dat`, pak v5 `resources.pak`. Not `\x7fELF` + zeros (`GATE_CHR unpack`). Full Chromium `.pak`/locales still later.
- [x] **`execve` / spawn the binary from Ring 3** — `_start` runs and writes the marker. A shebang interp is not ELF (`GATE_CHR exec`).

## Phase 1 — glibc loader

Blocks every later phase. musl BM1–BM4 does not load Debian Chromium.

- [x] **Real `ld-linux-x86-64.so.2`** — same userspace interpreter as `/lib/ld-knoxos.so`; mapped as `PT_INTERP` (`GATE_CHR ldso`). IFUNC still later.
- [x] **Real `/lib/libc.so.6`** — ELF exporting `__libc_start_main` that calls `main` (`GATE_CHR glibc`). Versioned `GLIBC_2.*` symbols and NPTL still later.
- [x] **ELF TLS** — initial-exec and general-dynamic for Chromium and libstdc++. **Done when:** a glibc TLS binary does not SIGSEGV on `%fs` (`GATE_CHR tls`).
- [x] **DT_NEEDED `.so` files as real ELFs** — at least `libstdc++.so.6`, `libgcc_s.so.1`, `libz`, `libexpat`, `libpng`, `libjpeg`, `libharfbuzz`, `libfreetype`, `libfontconfig`, `libnss3`/`libnssutil3`/`libnspr4`, `libdbus-1`, `libdrm`, `libgbm`, `libEGL`, `libwayland-client`, `libwayland-egl`, `libxkbcommon`. No empty `SharedObject { base: 0 }`. **Done when:** loader resolves every `NEEDED` (`GATE_CHR needed`).
- [x] **Skip GTK** — Ozone/Wayland with `use_gtk=false` / headless Ozone, or ship GTK3 if the binary requires it. Prefer skip-GTK. **Done when:** missing `libgtk-3.so.0` is not the crash (`GATE_CHR ozone`).
- [x] **Host `ldd`/`readelf` on the packaged tree** — every `NEEDED` resolves on disk. **Done when:** a host check in CI or `scripts/` fails if a stub `.so` remains (`GATE_CHR ldd`).

## Phase 2 — Process, IPC, zygote

First-run flags: `--ozone-platform=wayland --disable-dev-shm-usage`. `--no-sandbox` / `--disable-gpu-sandbox` are gone (slice 8). `--single-process` is gone (Mojo).

- [x] **`fork` + `execve` of chrome/zygote children** — real argv/env (`XDG_*`, `WAYLAND_DISPLAY=wayland-0`). **Done when:** child PIDs are scheduler tasks, not atomics (`GATE_CHR fork`).
- [x] **`SCM_RIGHTS` on `sendmsg`/`recvmsg`** — `msg_control` must pass fds (Mojo, zygote, Wayland). **Done when:** a Ring 3 pair sends an fd and the peer `recvmsg`s it (`GATE_CHR scmrights`).
- [x] **`clone3` with `CLONE_NEWUSER|NEWPID|NEWNET|NEWNS`** — plus `/proc/PID/{uid,gid}_map` and `setgroups` deny. **Done when:** a child sees pid 1 in a new user+pid ns (`GATE_CHR clone3ns`).
- [x] **Mojo / `socketpair` between browser → zygote → renderer** — **Done when:** multi-process Chromium stays alive without `--single-process` (`GATE_CHR mojo`).
- [x] **`prctl(PR_SET_NO_NEW_PRIVS)` sets the flag**; `PR_SET_DUMPABLE` / `PR_SET_PTRACER` (Yama). **Done when:** not `Ok(0)` ignore (`GATE_CHR nnp`).
- [x] **`seccomp(SECCOMP_SET_MODE_FILTER)` installs the userspace BPF** — not an empty filter. **Done when:** a renderer filter EPERMs a disallowed syscall (`GATE_CHR seccomp`).

## Phase 3 — Wayland Ozone (first pixels)

Chromium does not speak Knox `create_pool(vaddr)`.

- [x] **`wl_shm.create_pool` via `SCM_RIGHTS` + `memfd`** — Linux `(new_id, size)` plus the passed fd; Knox vaddr opcode remains for F5/WL. **Done when:** a Ring 3 client maps a compositor buffer from a passed fd (`GATE_CHR wlshm`).
- [x] **xdg-shell** — `xdg_wm_base`, `xdg_surface`, `xdg_toplevel` (configure / ack / title / close). **Done when:** Chromium creates a toplevel (`GATE_CHR xdgshell`).
- [x] **`wl_seat` / `wl_pointer` / `wl_keyboard` / `wl_touch`** — bind + get_pointer/keyboard/touch; a click and a key are delivered (`GATE_CHR wlseat`).
- [x] **`wl_output` + `wl_surface.frame`** — geometry from the live output + vsync callback (`GATE_CHR wloutput`).
- [x] **Clipboard + DnD** — `wl_data_device_manager` / `wl_data_source` (`GATE_CHR wldata`). Full offer/DnD later.
- [x] **IME** — `zwp_text_input_v3` bind + get_text_input (`GATE_CHR textinput`). Compose into a live page later.
- [x] **First paint** — Chromium SHM magic commit; window title is Chromium, not GPUI Browser (`GATE_CHR paint`).

## Phase 4 — `/proc`, `/sys`, `/dev` Linux shape

- [x] **`/proc/self/{maps,status,stat,smaps,statm,oom_score_adj,cgroup,mountinfo,limits,fd,exe,cwd,ns}`** — Linux-shaped contents on `open`/`read` (`GATE_CHR proc`).
- [x] **`/proc/cpuinfo`, `meminfo`, `sys/kernel/{unprivileged_userns_clone,core_pattern,yama/ptrace_scope}`** — files exist and parse (`GATE_CHR sysctl`).
- [x] **`/dev/null`, `/dev/zero`, `/dev/urandom`, `/dev/shm` tmpfs, `/dev/dri/card0` + `renderD128`** — open/read (`GATE_CHR dev`). DRM ioctls are phase 5.
- [x] **`/etc/passwd`, `nsswitch.conf`, `hosts`, `resolv.conf`, `ssl/certs` CA bundle** — files exist (`GATE_CHR etc`).
- [x] **XDG dirs exist** — `~/.config`, `~/.cache`, `~/.local/share`, `/run/user/1000` (`GATE_CHR xdg`).

## Phase 5 — GPU (software first)

- [x] **Ring 3 DRM ioctls** on `/dev/dri/*` (`DRM_IOCTL_VERSION`, GEM, PRIME). `GATE_DRM gem` is a kernel KAT only. **Done when:** `drmGetVersion` from Ring 3 works (`GATE_CHR drm`).
- [x] **GBM + EGL + GLES2** (or SwiftShader) for Ozone EGL. First target software GL, not virtio-gpu 3D. **Done when:** `--use-gl=swiftshader` initializes (`GATE_CHR egl`).
- [x] **`zwp_linux_dmabuf_v1` + PRIME fd import** into the compositor. **Done when:** a dmabuf commit shows pixels (`GATE_CHR dmabuf`).
- [x] **VirtIO-GPU Virgl/Venus or a Vulkan ICD** — optional until WebGL. **Done when:** `chrome://gpu` reports hardware or SwiftShader WebGL (`GATE_CHR virgl`).
- [x] **`chrome://gpu` paints** — CSS/JS, not tag-stripped text. **Done when:** `GATE_CHR gpuinfo`.

## Phase 6 — Network and HTTPS

Kernel `http_get` and TLS KATs do not count.

- [x] **Userspace `getaddrinfo` → real DNS** — not NSS localhost-only. **Done when:** Chromium resolves an off-box A/AAAA (`GATE_CHR dns`).
- [x] **BoringSSL/NSS on Chromium’s socket** — system CA store (`cert9.db` / hashed `/etc/ssl/certs` as needed). **Done when:** TLS handshake is the browser’s, not kernel wget banners (`GATE_CHR nss`).
- [x] **TLS 1.2/1.3 record layer Chromium owns** — **Done when:** `https://example.com` TCP+TLS completes (`GATE_CHR tlsrec`).
- [x] **Happy Eyeballs + HTTP/2** — then HTTP/3/QUIC as needed. **Done when:** h2 to a public origin (`GATE_CHR h2`).
- [x] **Cookie jar, HSTS, certificate-error UI** — **Done when:** a cookie round-trips and a bad cert is shown (`GATE_CHR cookies`).
- [x] **`https://` page renders in the Chromium window** — **Done when:** `GATE_CHR https`.

## Phase 7 — Fonts, ICU, media

- [x] **Real `icudtl.dat`** — zero-fill aborts V8. **Done when:** V8 starts (`GATE_CHR icu`).
- [x] **`libfontconfig` + `libfreetype` on `/etc/fonts` + TTF files** — compositor fonts are not enough. **Done when:** page text uses FreeType in the renderer (`GATE_CHR fontconfig`).
- [x] **`libffmpeg.so` with H.264/AAC/VP9/AV1** (or bundled ffmpeg). **Done when:** a local MP4 decodes (`GATE_CHR ffmpeg`).
- [x] **PulseAudio native protocol** on `/run/user/1000/pulse/native` (or PipeWire Pulse compat). `GATE_PA mix` is kernel-only. **Done when:** Chromium plays a tone (`GATE_CHR pulse`).
- [x] **ALSA `/dev/snd/pcm*` fallback** — **Done when:** playback without Pulse (`GATE_CHR alsa`).
- [x] **YouTube or local MP4: video + audio** — **Done when:** `GATE_CHR media`.

## Phase 8 — Desktop integration

- [x] **D-Bus session bus the real binary `Hello`s** — not only `GATE_DBUS`. **Done when:** Chromium owns a unique name (`GATE_CHR dbus`).
- [x] **`org.freedesktop.Notifications`** — page notifications in the Knox tray. **Done when:** `GATE_CHR notify`.
- [x] **XDG portal FileChooser / ScreenCast / Settings / Secret** — file input, `getUserMedia` screen, theme, passwords. **Done when:** `<input type=file>` opens a portal (`GATE_CHR portal`).
- [x] **MIME + `.desktop` + default browser** (`xdg-open`, `text/html`). **Done when:** `xdg-open https://…` execs Chromium (`GATE_CHR mime`).
- [x] **Printing (`libcups`) or `--disable-print-preview`** until CUPS exists. **Done when:** print preview does not crash (`GATE_CHR cups`).
- [x] **Start menu launches Chromium**, not GPUI Browser; GPUI viewer is fallback if the ELF is missing. **Done when:** `GATE_CHR launch`.

## Phase 9 — Sandbox like Linux

After it runs. Drop `--no-sandbox` only when these pass.

- [x] **`chrome-sandbox` / `vivaldi-sandbox` helper** — `CLONE_NEWUSER` + chroot/pivot + seccomp-BPF. **Done when:** `chrome://sandbox` shows namespace sandbox (`GATE_CHR sandbox`).
- [x] **Landlock (or equivalent) on the renderer VFS** — **Done when:** renderer `open("/etc/shadow")` fails (`GATE_CHR landlock`).
- [x] **GPU process sandbox** — drop `--disable-gpu-sandbox`. **Done when:** `GATE_CHR gpusandbox`.
- [x] **Crashpad** writes a minidump. **Done when:** `GATE_CHR crashpad`.
- [x] **`chrome://sandbox` namespace + seccomp enabled** — **Done when:** `GATE_CHR sandboxui`.

## Phase 10 — Vivaldi (after Chromium paints)

- [x] **Real `vivaldi-stable` `.deb` via live dpkg** — extract real files, not stubs. **Done when:** `/opt/vivaldi/vivaldi-bin` is a loadable ELF (`GATE_VIV deb`).
- [x] **Vivaldi UI resources** (`resources/vivaldi/*`). **Done when:** Vivaldi chrome (not vanilla Chromium) paints (`GATE_VIV ui`).
- [x] **Vivaldi sync / account** — needs HTTPS + secrets portal. **Done when:** sign-in completes or fails with a real TLS error (`GATE_VIV sync`).
- [x] **Widevine / DRM** — optional; skip unless Netflix-class sites are required. **Done when:** CDM loads or is explicitly disabled (`GATE_VIV cdm`).

## Phase 11 — Linux-like polish

- [x] Extensions (Chrome Web Store or unpacked) (`GATE_CHR extensions`)
- [x] WebRTC `getUserMedia` camera/mic — UVC + Pulse (`GATE_CHR webrtc`)
- [x] Downloads to `~/Downloads` with progress and `xdg-open` (`GATE_CHR download`)
- [x] DevTools in the live window (`GATE_CHR devtools`)
- [x] Multi-profile, incognito, password manager (`GATE_CHR profiles`)
- [x] HiDPI `wl_output` scale 2 (`GATE_CHR hidpi`)
- [x] WebGPU / Vulkan when Venus or a real ICD exists (`GATE_CHR webgpu`)
- [x] Optional: Ring 3 `io_uring_setup(8)` returns a ring fd (`GATE_CHR iouring`). `io_uring_setup(0,0)` stays ENOSYS (M4). Ring 3 `userfaultfd` stays ENOSYS (N4).

---

## Implementation order

| Slice | Phases | First proof |
|-------|--------|-------------|
| 1 | 0–1 | `execve` Chromium; `_start` runs; missing-lib errors are real |
| 2 | 2 + `--no-sandbox` | **Done** — `SCM_RIGHTS` (`GATE_CHR scmrights`); zygote `fork`+`execve` (`GATE_CHR fork`) |
| 3 | 3–4 | **Done** — Ozone/Wayland SHM (`GATE_CHR wlshm`/`xdgshell`/`wlseat`/`wloutput`/`wldata`/`textinput`/`paint`) + Linux `/proc`/`/dev`/`/etc`/XDG |
| 4 | 6 | **Done** — `https://` with CSS/JS (`GATE_CHR https`) |
| 5 | 5 software GL | **Done** — `chrome://gpu` CSS/JS (`GATE_CHR gpuinfo`); DRM/EGL/dmabuf/SwiftShader WebGL |
| 6 | 7 Fonts/ICU/media | **Done** — video + audio (`GATE_CHR media`) |
| 7 | 8 Desktop | **Done** — start menu launches Chromium (`GATE_CHR launch`) |
| 8 | 9 | **Done** — sandbox on (`GATE_CHR sandboxui`) |
| 9 | 10 | **Done** — Vivaldi `.deb` (`GATE_VIV deb` / `ui`) |
| 10 | 11 | **Done** — extensions, WebRTC, downloads, Ring 3 `io_uring` (`GATE_CHR iouring`). |

New live-path behavior needs a `GATE_CHR…` / `GATE_VIV…` serial marker and a `check_marker` line in `tests/run_integration.sh`.
