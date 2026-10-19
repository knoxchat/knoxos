# Do this first

← [Status index](README.md)

Gates **A–F, H–I**, syscall gates **J–BL**, **BM1–BM4** (userspace `ld.so` + musl static/dynamic), **F5** (Wayland AF_UNIX), **C7–C8** (`/bin` persist + JBD2), **G3** (feature flags), **G4** (syscall audit), **F6–F8** (calculator + Files userspace + VirtIO-GPU scanout/cursor), **I4** (SMP balance onto an idle AP), USB HID URB completion, **D6** (IPv6 echo TX), udev `/dev` hotplug, service restart, quota `EDQUOT`, cron `@reboot` + timer, VSync pacing, Ring 3 `openpty`+SHM (`GATE_TERM`), memory cgroups, job control, IMA, lockdown, wipe-on-free, UEFI time, and NetworkManager DHCP are done on QEMU. Adding another `GATE_xx` ENOSYS is **not** the critical path.

Work top to bottom. Skip anything marked later.

---

## 1. Quality bar (Gate G)

The linker path is done (BM1–BM4: `ld.so`, `dlopen`, static + dynamic musl). These shrink the lie in the tree.

| ID | Do | Done when | Details |
|----|----|-----------|---------|
| G3 | Feature flags `gui`, `net`, `fs-ext4`, `stub-drivers` | **Done** — default is `gui`+`net`+`fs-ext4`; `stub-drivers` stays off (`GATE_G3 features live`) | [gates/quality.md](gates/quality.md) |
| G4 | Finish the syscall audit | **Done** — clock/time unused numbers are ENOSYS (`GATE_G4 enosys`); `madvise`/`mlock*`/`readahead`/`fadvise64`/`set_robust_list` store real state (`GATE_G4 audit live`) | same |
| G5 | `unsafe` SAFETY comments + size budget | **Done** — 16 MB size gate holds; every `unsafe {` has a `// SAFETY:` comment; `undocumented_unsafe_blocks = deny` in `kernel/Cargo.toml` (`GATE_G5 safety comments`) | same |

Convert a silent `Ok(0)` when you touch that syscall. Do not add a new lettered gate just to ENOSYS one more unused number.

The leftover inventory that was still wireable on QEMU is also landed: memory cgroups (`GATE_CGROUP` / `GATE_CGROUP2`), job-control stop/continue (`GATE_JOB stop`), IMA on exec (`GATE_IMA exec`), lockdown + wipe-on-free (`GATE_LOCKDOWN module`, `GATE_WIPE free`), UEFI time/vars (`GATE_UEFI time`), NetworkManager DHCP apply (`GATE_NM dhcp`). Integration **426/426**.

---

## 2. Live-path gaps that still block a perfect OS

These are the unfinished rows in [blockers.md](blockers.md). Do them with Gate G, not instead of it.

1. **Persist `/bin`** — **Done** (C7). **Virtual FS mount points** — **Done** (C10): `/dev` `/proc` `/sys` `/run` directories round-trip; generated files under them stay virtual. [04-filesystem.md](subsystems/04-filesystem.md)
2. **Move desktop apps to Ring 3** — Calculator (F6) and Files (F7) are Ring 3 SHM. Settings/Task Manager/Editor/Browser/AI plus Calendar/Image Viewer/Logs/Disks/Bluetooth/Updater/Software Center/Setup Wizard/Archive also spawn userspace SHM, not new `WindowContentType`. Start-menu Terminal is Ring 3 `openpty`+SHM (`GATE_TERM pty interactive`). [07-gui.md](subsystems/07-gui.md)
3. **SMP load balance** — **Done** (I4): per-CPU CFS `current`; APs do not steal the BSP desktop; an unpinned Ring 3 task runs on an idle AP (`GATE_I4 smp balance`). [01-kernel-core.md](subsystems/01-kernel-core.md)
4. **Wayland over a Unix socket** — **Done** (F5 + WL): compositor `bind`+`listen`s `/run/wayland-0`; a Ring 3 client `connect`s and either `sendto`s a SHM vaddr (`GATE_F5 wayland unix`) or speaks `wl_display` object opcodes (`GATE_WL display protocol`). [07-gui.md](subsystems/07-gui.md)

---

## 3. After the linker (still production)

A dynamically linked musl binary runs (BM4). VirtIO-GPU scanout + hardware cursor are live (F8). JBD2 replay (C8) and the USB HID URB completion loop are live. IPv6 echo TX (D6), udev `/dev`, service restart, quota `EDQUOT`, cron `@reboot`+timer, and VSync pacing are live. The production leftover list from this section is landed:

- **Interactive start-menu Terminal in Ring 3** — **Done**: start menu spawns a Ring 3 `openpty` client; master fd + slave I/O are live (`GATE_TERM pty interactive`). Keyboard on Empty+SHM Terminal writes the PTY, not `TerminalApp`.
- **`wl_display` object protocol** — **Done**: `get_registry` / `bind` / `create_pool(vaddr)` / `attach` / `commit` on `/run/wayland-0` (`GATE_WL display protocol`). F5 8-byte vaddr remains for the old client.
- **GPU compositor for the desktop** — **Done**: desktop `present_rect` flushes the software FB through the live VirtIO-GPU scanout (`GATE_GPU desktop scanout`). VGA blit remains for `-vga std`.
- Host fuzz of ELF/ext4/packet parsers — **Done** (`cargo test -p knoxos-host-tests` mutates malformed inputs; parsers must not panic).
- Cargo workspace that includes `kernel` + `boot` — **Done**: root workspace members; `default-members` stay host tools; kernel builds via `./scripts/kernel-cargo.sh`.
- Memory cgroup hard limit on mmap/brk (`GATE_CGROUP memory`, `GATE_CGROUP2 memory`).
- SIGTSTP / `waitpid(WUNTRACED)` / SIGCONT against a process (`GATE_JOB stop`).
- IMA SHA-256 measure on `execve` / spawn (`GATE_IMA exec`).
- Lockdown denies unsigned `insert_module` (`GATE_LOCKDOWN module`); buddy free zeros the frame (`GATE_WIPE free`).
- UEFI `GetTime` from RTC + variable round-trip (`GATE_UEFI time`).
- NetworkManager `activate` copies live `eth0` (`GATE_NM dhcp`).

---

## 4. Still production (not the later list)

Skip [Do not start](#do-not-start). Highest-leverage open rows in [remaining.md](remaining.md) from this section are **done**, including this batch:

- rtl8139 TX→RX (`GATE_RTL8139 send`); CIFS SMB2 (`GATE_CIFS smb2`); GPOS kern (`GATE_GPOS kern`); AMDGPU firmware (`GATE_AMDGPU fw`); AX211 TLV (`GATE_AX211 fw`); FUSE LOOKUP+READ (`GATE_FUSE lookup`)
- KVM software EPT+HLT (`GATE_KVM ept`, `GATE_KVM vmlaunch`); OCI unshare+pivot_root (`GATE_OCI start`); ZFS uberblock (`GATE_ZFS block`); WPA3-SAE (`GATE_WPA3 sae`); bridge FDB (`GATE_BRIDGE fdb`); TLS 1.3 handshake (`GATE_TLS13 hs`); GPU matmul (`GATE_GPU matmul`); SDPA+KV (`GATE_AI attn`); assistant chat (`GATE_AI chat`); NFS4 COMPOUND (`GATE_NFS compound`); Broadcom `BRCMFW` (`GATE_BRCM fw`); Thunderbolt hop (`GATE_TB hop`)
- X.509 RSA over TBS (`GATE_TLS13 x509`); dpkg ar+tar (`GATE_DPKG deb`); TPM PCR extend (`GATE_TPM pcr`); BPE `"the"` (`GATE_AI vocab`); CPIO newc (`GATE_CPIO newc`); title `'i'` (`GATE_FONT title`); Terminal SHM glyphs (`GATE_TERM shm paint`)
- SELinux VFS deny (`GATE_SELINUX deny`); XHCI Enable Slot cc=1 (`GATE_USB xhci slot`); NTP offset (`GATE_NTP apply`); Bluetooth HCI Reset (`GATE_BT hci`); dm-verity Merkle (`GATE_VERITY root`)
- AppArmor VFS deny (`GATE_APPARMOR deny`); XHCI GET_DESCRIPTOR Transfer Event (`GATE_USB xhci desc`); unit MemoryMax cgroup (`GATE_SVC cgroup`); GPOS mark-to-base (`GATE_GPOS mark`)
- io_uring NOP+WRITE SQE (`GATE_IOURING sqe`); fanotify VFS MODIFY (`GATE_FANOTIFY modify`); D-Bus Hello unique name (`GATE_DBUS method`); XHCI SET_CONFIGURATION (`GATE_USB xhci config`)
- USB Audio / trackpad / Wacom / UVC / Ethernet / touch / GPIO (`GATE_USB audio`, `GATE_USB trackpad`, `GATE_USB wacom`, `GATE_USB uvc`, `GATE_USB ethernet`, `GATE_USB touch`, `GATE_GPIO xfer`)
- XHCI Address Device cc=1 (`GATE_USB xhci addr`); D-Bus RequestName (`GATE_DBUS name`); FUSE WRITE (`GATE_FUSE write`); PAM unix (`GATE_PAM unix`); cgroup freeze (`GATE_CGROUP freeze`)
- XHCI full enum with no timeout fallback (`GATE_USB xhci enum`); virtual-FS mount-point persist (`GATE_C10 virtfs persist`); ACPI S5 PM1a (`GATE_ACPI s5`); FAT32/ext2/ISO9660 ramdisk (`GATE_FAT32 block`, `GATE_EXT2 block`, `GATE_ISO9660 block`); Configure Endpoint (`GATE_USB xhci ep`); D-Bus ReleaseName (`GATE_DBUS release`); FUSE UNLINK (`GATE_FUSE unlink`)
- FUSE MKDIR (`GATE_FUSE mkdir`); D-Bus ListNames (`GATE_DBUS list`); XHCI GET_STATUS (`GATE_USB xhci status`); FAT32 vfat mount (`GATE_FAT32 mount`); NFS WRITE (`GATE_NFS write`)
- FUSE RMDIR (`GATE_FUSE rmdir`); FUSE RENAME (`GATE_FUSE rename`); D-Bus GetConnectionUnixProcessID (`GATE_DBUS pid`); XHCI SET_INTERFACE (`GATE_USB xhci iface`); ISO9660/NTFS mount (`GATE_ISO9660 mount`, `GATE_NTFS mount`); NFS REMOVE (`GATE_NFS remove`)
- TLS 1.3 HKDF (`GATE_TLS13 hkdf`); HDR/VRR (`GATE_HDR pq`, `GATE_VRR range`); e1000 `Socket::send` (`GATE_E1000 send`); eBPF/sched_ext (`GATE_EBPF exec`); Wi-Fi Beacon+PMK (`GATE_WIFI pmk`); i915 modeset (`GATE_I915 modeset`); Btrfs ramdisk (`GATE_BTRFS block`); VLAN 802.1Q (`GATE_VLAN tag`)

Earlier in this section:

- XFS through the block layer (`GATE_XFS block`); X25519/Ed25519 RFC KATs (`GATE_X25519 kat`, `GATE_ED25519 kat`); locale `.mo` (`GATE_LOCALE switch`); pinyin IME (`GATE_IME compose`); USB MSC READ(10) (`GATE_USB msc`); IPv4 NAT checksum (`GATE_NAT masquerade`); DRM GEM+CRTC (`GATE_DRM gem`); GSUB Latin liga (`GATE_GSUB liga`)
- NTFS through the block layer (`GATE_NTFS block`); here-docs / `~/.profile` (`GATE_HEREDOC expand`, `GATE_PROFILE sourced`); VT100 CUP+SGR256 (`GATE_VT100 xterm`)
- HDA DMA + PulseAudio mix (`GATE_HDA dma`, `GATE_PA mix`); IOMMU `enable_translation` (`GATE_IOMMU translation`)
- THP / KASLR (`GATE_THP hugepage`, `GATE_KASLR slide`); ACPI AML `_S5` (`GATE_AML dsdt`); kernel `.ko` load (`GATE_KO load`)
- NUMA frame alloc (`GATE_NUMA frame`); KSM merge (`GATE_KSM merge`); nested IRQ TPR (`GATE_NIRQ tpr`); live patch W^X (`GATE_LPATCH wx`)
- exFAT through the block layer (`GATE_EXFAT block`); CPU/IO cgroups (`GATE_CGROUP cpu`, `GATE_CGROUP io`)
- Suspend/logind (`GATE_SUSPEND resume`, `GATE_LOGIND session`); AES-GCM/Poly1305 (`GATE_AESGCM kat`, `GATE_POLY1305 kat`); font fallback (`GATE_FONT fallback`)

---

## Do not start

These increase the distance to a perfect OS. Full list: [gates/quality.md](gates/quality.md).

k8s orchestration, Chromium/Vivaldi, aarch64/riscv64 **runtime**, nested VT-x `VMXON`/`VMLAUNCH` on real hardware, RPM/APK, production tokenizer vocab, another unused `pub mod`.

Open checkboxes for those live in [remaining.md](remaining.md) so they are not forgotten — they are not next.

---

## How to land work

1. Wire an existing stub into the live path, **or** delete / feature-gate Unused modules, **or** add a test that can fail.
2. New live-path behavior needs a serial marker (`GATE_…`) and a `check_marker` line in `tests/run_integration.sh`.
3. Tick the subsystem checkbox (and a gate row, if this is a named gate) only when QEMU (or hardware) meets **Done when**.
