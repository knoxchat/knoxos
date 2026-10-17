# Changelog

## Unreleased

## 0.2.4

- **TLS 1.3 HKDF** — RFC 5869 SHA-256 extract/expand plus RFC 8446 Expand-Label (`GATE_TLS13 hkdf`).
- **HDR / VRR** — ST 2084 PQ EOTF known-answer and Adaptive-Sync range retargets VBlank (`GATE_HDR pq`, `GATE_VRR range`).
- **e1000** — `Socket::send` programs the TX ring; software loopback delivers RX (`GATE_E1000 send`).
- **eBPF / sched_ext** — verifier + interpreter `r0=42`; sched_ext pick runs the same program (`GATE_EBPF exec`).
- **Wi-Fi** — 802.11 Beacon SSID parse + WPA2-PSK PBKDF2-SHA1 PMK (`GATE_WIFI pmk`).
- **i915 modeset** — pipe A + plane + GTT[0] in shadow MMIO (`GATE_I915 modeset`).
- **Btrfs** — superblock + inode + extent `$DATA` through the ramdisk (`GATE_BTRFS block`).
- **VLAN** — 802.1Q insert/strip round-trip (`GATE_VLAN tag`).
- **RSA / ECDSA-P256** — 512-bit PKCS#1 v1.5 SHA-256 verify and RFC 6979 P-256 (`GATE_RSA kat`, `GATE_ECDSA kat`).
- **Color emoji** — U+1F600 is a multi-color bitmap, not a solid placeholder (`GATE_EMOJI color`).
- **USB hub / gamepad** — hub descriptor + GetPortStatus (`GATE_USB hub`); HID gamepad report parse (`GATE_USB gamepad`).
- **TPM TIS** — locality 0 REQUEST_USE → VALID|ACTIVE (`GATE_TPM tis`). Hardware MMIO skipped without a mapped TIS.
- **TrueType hinting** — SVTCA/PUSHB/MDAP round-to-grid (`GATE_TTF hint`).
- **I2C** — SMBus EEPROM write/read on a software slave (`GATE_I2C xfer`).
- **XFS** — superblock + inode + extent `$DATA` through the ramdisk (`GATE_XFS block`).
- **X25519 / Ed25519** — RFC 7748 Alice public and RFC 8032 TEST 1 (`GATE_X25519 kat`, `GATE_ED25519 kat`).
- **Locale** — GNU `.mo` load + `set_locale` + gettext (`GATE_LOCALE switch`).
- **IME** — pinyin `"ni"` → `你` (`GATE_IME compose`).
- **USB MSC** — SCSI READ(10) vs ramdisk LUN (`GATE_USB msc`).
- **NAT** — IPv4 SNAT/DNAT rewrite with header checksum (`GATE_NAT masquerade`).
- **DRM** — GEM from buddy frames + CRTC set (`GATE_DRM gem`).
- **GSUB** — Latin `fi` ligature via `shape_text` (`GATE_GSUB liga`).
- **NUMA** — `allocate_node` returns a buddy frame charged to the node (`GATE_NUMA frame`).
- **KSM** — hashes through `phys_to_virt`; identical pages collapse and the victim is freed (`GATE_KSM merge`).
- **Nested IRQ / TPR** — ISRs raise LAPIC TPR; nested enter is proven (`GATE_NIRQ tpr`).
- **Live patch** — dedicated 4K trampoline, never W+X, activeness wait (`GATE_LPATCH wx`).
- **exFAT** — VBR + FAT + directory `$DATA` through the ramdisk (`GATE_EXFAT block`).
- **cgroup CPU/IO** — `cpu.max` quota and `io.max` byte cap deny (`GATE_CGROUP cpu`, `GATE_CGROUP io`).
- **Suspend / logind** — S3 save/restore hooks (`GATE_SUSPEND resume`); session create/list/terminate (`GATE_LOGIND session`).
- **AES-GCM / Poly1305** — NIST AES-128-GCM and RFC 8439 Poly1305 known-answer (`GATE_AESGCM kat`, `GATE_POLY1305 kat`).
- **Font fallback** — compositor chain used for CJK/emoji (`GATE_FONT fallback`).
- **NTFS** — boot + MFT `$DATA` read through the block layer (`GATE_NTFS block`).
- **Shell** — here-docs feed stdin; `name() { }` runs; `~/.profile` is sourced (`GATE_HEREDOC expand`, `GATE_PROFILE sourced`).
- **VT100** — CUP + xterm-256 SGR + ED on the live emulator (`GATE_VT100 xterm`).
- **HDA + PulseAudio** — buddy DMA buffers (not heap pointers); two streams mix into the HDA ring (`GATE_HDA dma`, `GATE_PA mix`).
- **IOMMU** — root table from the buddy; `enable_translation` is called (`GATE_IOMMU translation`).
- **THP / KASLR** — 2 MiB hugepage from buddy order 9; kernel module slide is non-zero (`GATE_THP hugepage`, `GATE_KASLR slide`).
- **ACPI AML** — DSDT scan for `Name(_S5_)` / `Name(\_S5)` SLP_TYP (`GATE_AML dsdt`). FADT uses the 32-bit DSDT pointer so a short table cannot produce a non-canonical VA (#GP).
- **Kernel `.ko`** — ELF ET_REL load, relocate, call `init_module` (`GATE_KO load`).
- **Memory cgroups** — mmap/brk charge against v1 + v2 hard limits; over-limit is ENOMEM (`GATE_CGROUP memory`, `GATE_CGROUP2 memory`).
- **Job control** — SIGTSTP stops a process; `waitpid(WUNTRACED)` reports it; SIGCONT resumes (`GATE_JOB stop`).
- **IMA** — SHA-256 measure on `execve` / Ring 3 spawn (`GATE_IMA exec`).
- **Lockdown** — Integrity blocks unsigned `insert_module` (`GATE_LOCKDOWN module`).
- **Secure wipe** — buddy free zeros the physical frame (`GATE_WIPE free`).
- **UEFI runtime** — `GetTime` from CMOS RTC; `SetVariable`/`GetVariable` round-trip (`GATE_UEFI time`).
- **NetworkManager** — `activate` copies live `eth0` DHCP (`GATE_NM dhcp`).
- **Gate D6** — IPv6 echo actually TXes: loopback `::1` injects ICMPv6 Echo Request, reply is sent (`GATE_D6 ipv6 echo`). VirtIO-net RX `0x86DD` feeds `process_ipv6_packet`.
- **Service restart** — failed units with `OnFailure`/`Always` are brought back (`GATE_SVC restart`).
- **udev hotplug** — PCI scan creates `/dev` nodes and drains kobject ADD uevents (`GATE_UDEV hotplug`).
- **Quota enforcement** — VFS `write_file_checked` returns `EDQUOT` when the hard block limit is exceeded (`GATE_QUOTA enforce`).
- **Cron timer** — `@reboot` jobs run at init (`GATE_CRON reboot`); the PIT/APIC tick sets a due flag so `tick` is not desktop-loop-only.
- **VSync pacing** — `gpu_compositor` is on the live path; software VBlank flips a page (`GATE_VSYNC paced`).
- **Ring 3 PTY terminal** — start-menu Terminal is a userspace client: `openpty` installs a master fd, slave I/O round-trips (`GATE_TERM pty interactive`), SHM Empty not `WindowContentType::Terminal`. Keyboard on that window writes the PTY.
- **`wl_display` object protocol** — Ring 3 client `get_registry` / `bind` / `create_pool(vaddr)` / `attach` / `commit` on `/run/wayland-0` (`GATE_WL display protocol`). F5 8-byte vaddr sendto remains.
- **GPU desktop scanout** — `present_rect` copies the software FB into the live VirtIO-GPU resource and flushes (`GATE_GPU desktop scanout`).
- Host fuzz of ELF / ext4 / Ethernet-IPv4-IPv6 parsers (`cargo test -p knoxos-host-tests`).
- Cargo workspace includes `kernel` + `boot` + tools; `default-members` stay host-only; kernel builds via `./scripts/kernel-cargo.sh`.
- Host-side parser tests (`cargo test -p knoxos-host-tests`) for ELF magic, ICMPv6 checksum, cron fields, ext4 super magic, quota KiB rounding.
- **Gate G5** — `// SAFETY:` on every `unsafe {`; `clippy::undocumented_unsafe_blocks = deny` (`GATE_G5 safety comments`).
- **Gate C9** — ext4 is the default root on a second virtio-blk (`mkfs.ext4`); `/dev` `/proc` `/sys` `/run` `/tmp` stay virtual (`GATE_C9 ext4 root`). `fs-ext4` is a default Cargo feature.
- **USB HID cursor without PS/2** — i8042 aux is not enabled; a boot-protocol mouse URB moves the GUI cursor (`GATE_USB no ps2 cursor`).
- **Secure Boot QEMU** — `scripts/qemu-secure-boot.sh` enrolls PK/KEK/db, signs `BOOTX64.EFI`, boots OVMF with `smm=on` (`GATE_SB ovmf boot`).
- **Cross-arch QEMU boot** — aarch64/riscv64 `_start` prints `GATE_ARCH aarch64 boot` / `GATE_ARCH riscv64 boot` and idles.
- **XHCI MMIO** — BAR0 mapped uncached via `vmm::map_mmio`; command/event/transfer rings are buddy DMA frames (`GATE_USB xhci mmio`). Transfer Events drain into the HID URB handler and the GUI mouse/key path.
- **Secure Boot keys** — `scripts/secure-boot-enroll.sh` writes PK/KEK/db. OVMF PK enrollment runs when `virt-fw-vars` is installed.
- **Gate C8** — JBD2 journal replay: committed descriptor+data+commit applied; torn descriptor discarded (`GATE_C8 jbd2 replay`). ext4 mount uses the same walker when `fs-ext4` is on.
- **USB HID URB** — submit + complete + boot-protocol parse delivers evdev KEY_A (`GATE_USB hid urb`).
- **Gate F8** — VirtIO-GPU controlq 2D scanout (`RESOURCE_CREATE_2D` / `ATTACH_BACKING` / `SET_SCANOUT` / `TRANSFER_TO_HOST_2D` / `RESOURCE_FLUSH`) and cursorq `UPDATE_CURSOR` on QEMU `-device virtio-gpu-pci`. Serial: `GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor`. Desktop skips the software cursor blit when the host cursor plane is live.
- Architecture / syscall / driver / GUI notes under `docs/`.
- CI: `cargo check` for aarch64 and riscv64 (compile only); rustdoc artifact; `cargo-audit` + SPDX; nightly images; ISO + SHA256SUMS (`scripts/make-iso.sh`).

## 0.2.3

Gates A–F, H–I, J–BL, BM1–BM4 (`ld.so` + musl), F5–F7, C7, G3–G4, I4 on the QEMU boot path. Persist snapshot covers `/etc` and `/bin`. Default root is still a RAM VFS.
