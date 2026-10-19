# Still open

← [Status index](README.md) · [Do this first](next.md)

Every remaining `- [ ]` in the subsystem files, plus unfinished Gate G. This is an inventory, not a sequence — start with [next.md](next.md).

## Gate G (quality bar)

| ID | Task | Done when |
|----|------|-----------|
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | **Done** — default `gui`+`net`+`fs-ext4`; `stub-drivers` opt-in (`GATE_G3 features live`) |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | **Done** — unused clock/time numbers ENOSYS (`GATE_G4 enosys`); `madvise`/`mlock*`/`readahead`/`fadvise64`/`set_robust_list` store real state (`GATE_G4 audit live`) |
| G5 | `unsafe` SAFETY comments + size budget | **Done** — 16 MB size gate; every `unsafe {` has a `// SAFETY:` comment; crate-level `undocumented_unsafe_blocks = deny` (`GATE_G5 safety comments`) |

## 1. Kernel Core

Details: [subsystems/01-kernel-core.md](subsystems/01-kernel-core.md)

- [x] **x86_64 / aarch64 / riscv64** — x86_64 desktop boots; aarch64/riscv64 QEMU serial `_start` (`GATE_ARCH aarch64 boot` / `GATE_ARCH riscv64 boot`). Full runtime on those arches is later.
- [x] **ACPI AML / DSDT** — DSDT mapped; `Name(_S5)` SLP_TYP extracted (`GATE_AML dsdt`). `acpi::shutdown` writes PM1a_CNT (`GATE_ACPI s5`).
- [x] **UEFI runtime** — `GetTime` from CMOS RTC; `SetVariable`/`GetVariable` round-trip (`GATE_UEFI time`)
- [x] **Kernel modules** — ELF ET_REL `.ko` load + reloc + `init_module` (`GATE_KO load`)
- [x] **IOMMU** — root table from buddy; `enable_translation` called (`GATE_IOMMU translation`)
- [x] **Live patch** — dedicated 4K trampoline, W^X (never W+X), activeness wait (`GATE_LPATCH wx`)
- [x] **Nested IRQ / TPR** — ISRs raise LAPIC TPR; nested enter writes TPR (`GATE_NIRQ tpr`)

## 2. Memory Management

Details: [subsystems/02-memory.md](subsystems/02-memory.md)

- [x] **NUMA** — `allocate_node` returns a buddy frame charged to the node (`GATE_NUMA frame`)
- [x] **KSM** — hashes via `phys_to_virt`; identical pages collapse and the victim is freed (`GATE_KSM merge`)
- [x] **THP** — buddy order-9 2 MiB allocation (`GATE_THP hugepage`)
- [x] **KASLR** for loadable kernel mappings (`GATE_KASLR slide`)
- [x] **Memory cgroups** enforcement — mmap/brk charge; hard `memory.max` is ENOMEM (`GATE_CGROUP memory`, `GATE_CGROUP2 memory`)

## 3. Process & Scheduling

Details: [subsystems/03-process.md](subsystems/03-process.md)

- [x] **sched_ext / eBPF** — verifier + interpreter `r0=42`; sched_ext pick (`GATE_EBPF exec`)

## 4. Filesystem & Storage

Details: [subsystems/04-filesystem.md](subsystems/04-filesystem.md)

- [x] NTFS — boot + MFT `$DATA` through the ramdisk block layer (`GATE_NTFS block`)
- [x] exFAT — VBR + FAT + directory `$DATA` through the ramdisk block layer (`GATE_EXFAT block`). CIFS SMB2 WRITE/READ (`GATE_CIFS smb2`). ZFS uberblock + Fletcher-4 (`GATE_ZFS block`).
- [x] Btrfs — superblock + inode + extent `$DATA` through the ramdisk (`GATE_BTRFS block`)
- [x] XFS — superblock + inode + extent `$DATA` through the ramdisk (`GATE_XFS block`)
- [x] JBD2 journal replay — descriptor + commit applied; torn descriptor discarded (`GATE_C8 jbd2 replay`). Virtual-FS mount-point directories persist (`GATE_C10 virtfs persist`) including `/tmp`. FAT32 / ext2 / ISO9660 through the ramdisk (`GATE_FAT32 block`, `GATE_EXT2 block`, `GATE_ISO9660 block`).
- [x] FUSE — LOOKUP + READ through the in-kernel loopback daemon (`GATE_FUSE lookup`). WRITE stores bytes a later READ returns (`GATE_FUSE write`). UNLINK drops the name (`GATE_FUSE unlink`). NFS4 COMPOUND PUTROOTFH+LOOKUP+READ (`GATE_NFS compound`). CPIO newc (`GATE_CPIO newc`). dpkg ar+tar (`GATE_DPKG deb`).
- [x] Quotas — `write_file_checked` returns `EDQUOT` (`GATE_QUOTA enforce`). fanotify MODIFY/CREATE on VFS write (`GATE_FANOTIFY modify`); Ring 3 `fanotify_init` stays ENOSYS. io_uring NOP+WRITE (`GATE_IOURING sqe`); Ring 3 `io_uring_setup` stays ENOSYS.

## 5. Networking Stack

Details: [subsystems/05-networking.md](subsystems/05-networking.md)

- [x] IPv6 echo — loopback `::1` Echo Request produces a TX reply (`GATE_D6 ipv6 echo`)
- [x] TLS 1.3 HKDF — RFC 5869 extract/expand + Expand-Label (`GATE_TLS13 hkdf`). ClientHello+ServerHello matching traffic secret (`GATE_TLS13 hs`). DER + RSA PKCS#1 over TBS (`GATE_TLS13 x509`).
- [x] NAT — IPv4 SNAT/DNAT rewrite with header checksum (`GATE_NAT masquerade`)
- [x] VLAN — 802.1Q insert/strip (`GATE_VLAN tag`). Wi-Fi Beacon + WPA2 PMK (`GATE_WIFI pmk`). WPA3-SAE matching PMK (`GATE_WPA3 sae`); bridge FDB (`GATE_BRIDGE fdb`).

## 6. Device Drivers

Details: [subsystems/06-drivers.md](subsystems/06-drivers.md)

- [x] XHCI BAR0 mapped uncached + buddy DMA rings (`GATE_USB xhci mmio`). Enable Slot Command Completion cc=1 (`GATE_USB xhci slot`). Address Device Command Completion cc=1 (`GATE_USB xhci addr`). GET_DESCRIPTOR Transfer Event (`GATE_USB xhci desc`). SET_CONFIGURATION Transfer Event (`GATE_USB xhci config`). Configure Endpoint Command Completion cc=1 (`GATE_USB xhci ep`). GET_DESCRIPTOR(Configuration) Transfer Event plus full enum with no timeout fallback (`GATE_USB xhci enum`). Transfer Events drain into HID.
- [x] i915 modeset — pipe A + plane + GTT[0] in shadow MMIO (`GATE_I915 modeset`). AMDGPU firmware (`GATE_AMDGPU fw`).
- [x] HDA — CORB/RIRB + PCM BDL from buddy frames (`GATE_HDA dma`)
- [x] TPM TIS FIFO — locality 0 REQUEST_USE grants VALID|ACTIVE (`GATE_TPM tis`). Software SHA-256 PCR extend (`GATE_TPM pcr`). Hardware 0xFED4_0000 still unmapped on stock QEMU.
- [x] VirtIO-GPU — controlq + cursorq vs QEMU `-device virtio-gpu-pci` (`GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor`)
- [x] DRM — GEM from buddy frames + CRTC set (`GATE_DRM gem`). HDR PQ + VRR Adaptive-Sync (`GATE_HDR pq`, `GATE_VRR range`).
- [x] USB HID URB completion — submit + complete + boot-protocol parse (`GATE_USB hid urb`). Transfer Events feed the same handler and the GUI mouse/key path.
- [x] USB MSC — SCSI READ(10) vs ramdisk LUN (`GATE_USB msc`). USB Audio SET_CUR + PCM loopback (`GATE_USB audio`).
- [x] USB hub — descriptor type 0x29 + GetPortStatus (`GATE_USB hub`)
- [x] USB HID gamepad — 8-byte report parse (`GATE_USB gamepad`)
- [x] I2C SMBus EEPROM write/read (`GATE_I2C xfer`)
- [x] e1000 — `Socket::send` programs the TX ring; loopback RX (`GATE_E1000 send`). rtl8139 TX→RX (`GATE_RTL8139 send`).
- [x] Touchscreen HID digitizer (`GATE_USB touch`); trackpad two-finger scroll (`GATE_USB trackpad`); GPIO pin loopback (`GATE_GPIO xfer`); Wacom pen (`GATE_USB wacom`); UVC YUY2 (`GATE_USB uvc`)
- [x] USB Ethernet CDC ECM frame loopback (`GATE_USB ethernet`). AX211 iwlwifi TLV (`GATE_AX211 fw`). Broadcom `BRCMFW` (`GATE_BRCM fw`). Thunderbolt route hop (`GATE_TB hop`).

## 7. GUI & Desktop Environment

Details: [subsystems/07-gui.md](subsystems/07-gui.md)

- [x] **Wayland protocol** — `/dev/wl0` ioctl (F1–F2) plus AF_UNIX `/run/wayland-0` (F5) plus `wl_display` object opcodes (`GATE_WL display protocol`)
- [x] **GPU compositor** — `gpu_compositor` init + VSync; desktop `present_rect` flushes through VirtIO-GPU (`GATE_GPU desktop scanout`). VGA blit remains for `-vga std`.
- [x] Hardware cursor — VirtIO-GPU cursorq `UPDATE_CURSOR` / `MOVE_CURSOR` KAT (`GATE_F8 hw cursor`). Software arrow still draws on `-vga std`.
- [x] VSync — `gpu_compositor` software VBlank flip on the live path (`GATE_VSYNC paced`); desktop still software-blits to VGA
- [x] IME — pinyin `"ni"` → `你` (`GATE_IME compose`). Full IME framework later.
- [x] TrueType bytecode hinting — SVTCA/PUSHB/MDAP round-to-grid (`GATE_TTF hint`). Title `'i'` stem (`GATE_FONT title`).
- [x] HDR, VRR — PQ EOTF + Adaptive-Sync range (`GATE_HDR pq`, `GATE_VRR range`)
- [x] Ring 3 `openpty` + SHM terminal client (`GATE_TERM pty userspace`). Start-menu Terminal is Ring 3; master fd + slave I/O (`GATE_TERM pty interactive`). First SHM frame paints glyphs (`GATE_TERM shm paint`).

## 8. Shell & Terminal

Details: [subsystems/08-shell.md](subsystems/08-shell.md)

- [x] Full VT100/xterm-256 — CUP + SGR 256-color + ED (`GATE_VT100 xterm`)
- [x] Here-docs, functions, `~/.profile` (`GATE_HEREDOC expand`, `GATE_PROFILE sourced`)
- [x] Job control against **processes** — SIGTSTP stops; `waitpid(WUNTRACED)` reports; SIGCONT resumes (`GATE_JOB stop`). Ring 3 `pipe()` is live (M2).

## 9. Security & Cryptography

Details: [subsystems/09-security.md](subsystems/09-security.md)

- [x] AES-GCM, Poly1305 — NIST AES-128-GCM + RFC 8439 Poly1305 (`GATE_AESGCM kat`, `GATE_POLY1305 kat`). TLS 1.3 HKDF (`GATE_TLS13 hkdf`) plus ClientHello/ServerHello (`GATE_TLS13 hs`). DER + RSA PKCS#1 over TBS (`GATE_TLS13 x509`).
- [x] X25519 / Ed25519 — RFC 7748 Alice public + RFC 8032 TEST 1 (`GATE_X25519 kat`, `GATE_ED25519 kat`)
- [x] RSA / ECDSA-P256 — PKCS#1 v1.5 SHA-256 + RFC 6979 P-256 (`GATE_RSA kat`, `GATE_ECDSA kat`)
- [x] Kernel lockdown, secure wipe on free, IMA — unsigned `insert_module` denied (`GATE_LOCKDOWN module`); buddy free zeros the frame (`GATE_WIPE free`); exec measures SHA-256 (`GATE_IMA exec`)
- [x] SELinux — Enforcing TE on VFS writes (`GATE_SELINUX deny`). Default stays Permissive. AppArmor path MAC on VFS writes (`GATE_APPARMOR deny`).
- [x] dm-verity — SHA-256 Merkle leaf equals root; tamper rejected (`GATE_VERITY root`)

## 10. System Services

Details: [subsystems/10-services.md](subsystems/10-services.md)

- [x] Crash restart of failed units (`GATE_SVC restart`). Unit `MemoryMax=` cgroup (`GATE_SVC cgroup`). D-Bus `Hello` unique name (`GATE_DBUS method`). `RequestName` PRIMARY_OWNER (`GATE_DBUS name`). `ReleaseName` drops ownership (`GATE_DBUS release`). PAM unix SHA-512 (`GATE_PAM unix`).
- [x] NTP — fixture Server-mode packet applies a 1000 ms offset (`GATE_NTP apply`)
- [x] Bluetooth HCI — virtual adapter Reset Command Complete (`GATE_BT hci`)
- [x] Cron `@reboot` plus PIT/APIC due-flag (`GATE_CRON reboot`)
- [x] udev-style hotplug → `/dev` from PCI scan (`GATE_UDEV hotplug`)
- [x] NetworkManager activate copies live `eth0` DHCP (`GATE_NM dhcp`)
- [x] Suspend/hibernate, logind — S3 save/restore hooks (`GATE_SUSPEND resume`); session create/list/terminate (`GATE_LOGIND session`)
- [x] PulseAudio mixing to HDA (`GATE_PA mix`)

## 11. Virtualization & Containers

Details: [subsystems/11-virt.md](subsystems/11-virt.md)

- [x] `kvm::start_vm` software EPT fetch of HLT `0xF4` (`GATE_KVM vmlaunch`). Nested VMXON is not used on stock QEMU.
- [x] EPT 4-level walk + virtio-mmio magic `0x74726976` (`GATE_KVM ept`)
- [x] `container.rs` — `runc start` unshare + `pivot_root` (`GATE_OCI start`)
- [x] cgroup v2 **memory** enforcement (`GATE_CGROUP2 memory`). CPU quota and IO byte cap deny (`GATE_CGROUP cpu`, `GATE_CGROUP io`). Frozen cgroup denies CPU charge (`GATE_CGROUP freeze`).

## 12. AI/ML Integration

Details: [subsystems/12-ai.md](subsystems/12-ai.md)

- [x] Scaled-dot-product attention + KV cache append (`GATE_AI attn`). BPE encode/decode of `"the"` (`GATE_AI vocab`). Production tokenizer vocab later.
- [x] `gpu_matmul.rs` CPU KAT with `available: true` (`GATE_GPU matmul`)
- [x] GUI assistant summarize uses attention, not placeholder text (`GATE_AI chat`)

## 13. Binary Compatibility & Runtime

Details: [subsystems/13-binary-compat.md](subsystems/13-binary-compat.md)

- [x] Dynamic linker as a userspace `ld.so` (DT_NEEDED, lazy PLT, RELRO) — **Gate BM1**
- [x] `dlopen` that maps `.so` via VMM (`open` + file-backed `mmap`, not `ldknoxos.rs` fake base) — **Gate BM2**
- [x] Static musl CRT ABI (`_start` → `__libc_start_main` → `main`) — **Gate BM3**
- [x] Dynamically linked musl binary for Ring 3 (`PT_INTERP` + `DT_NEEDED libc.so` + `__libc_start_main@plt`) — **Gate BM4**

## 14. Internationalization & Fonts

Details: [subsystems/14-i18n.md](subsystems/14-i18n.md)

- [x] Font fallback chain — compositor chain used for CJK/emoji (`GATE_FONT fallback`)
- [x] GSUB Latin ligatures — `shape_text("fi")` → U+FB01 (`GATE_GSUB liga`). GPOS A+V kern (`GATE_GPOS kern`). GPOS mark-to-base `e`+U+0301 (`GATE_GPOS mark`).
- [x] Color emoji — U+1F600 multi-color bitmap (`GATE_EMOJI color`)
- [x] Locale `.mo` — GNU magic load + `set_locale` + `gettext("File")=="Archivo"` (`GATE_LOCALE switch`)

## 15. Build System & Tooling

Details: [subsystems/15-build.md](subsystems/15-build.md)

- [x] **`flake.nix`** — present (devShell + QEMU/Make; kernel still rustup nightly)
- [x] Cargo workspace unifying `kernel` + `boot` + `tools` (`default-members` are host tools; kernel via `./scripts/kernel-cargo.sh`)
- [x] Feature flags so Unused modules do not compile into the demo kernel (`gui`/`net` default; `stub-drivers`/`fs-ext4` opt-in)
- [x] ISO + SHA256SUMS + optional GPG (`scripts/make-iso.sh`, `scripts/release-checksums.sh`). PK/KEK/db + OVMF enroll via `scripts/secure-boot-enroll.sh`. QEMU Secure Boot boot: `scripts/qemu-secure-boot.sh` (`GATE_SB ovmf boot`).

## 16. Testing & Quality

Details: [subsystems/16-testing.md](subsystems/16-testing.md)

- [x] Host-side `#[test]` in `tools/host-tests` (`cargo test -p knoxos-host-tests`)
- [x] Clippy `undocumented_unsafe_blocks` is **deny** (`kernel/Cargo.toml`); every `unsafe {` has a `// SAFETY:` comment (`GATE_G5 safety comments`).
- [x] Host parser tests (ELF / ICMPv6 / cron / ext4 magic / quota KiB / Btrfs magic / VLAN TPID / Beacon SSID)
- [x] Host fuzz of ELF / ext4 / packet parsers (`cargo test -p knoxos-host-tests`; malformed inputs must not panic). No Miri, no coverage.

## 17. Documentation

Details: [subsystems/17-docs.md](subsystems/17-docs.md)

- [x] Architecture / syscall / driver / GUI guides (`docs/`)
- [x] `cargo doc` published (CI rustdoc artifact)
- [x] Changelog (`CHANGELOG.md`)

## 18. CI/CD & Release

Details: [subsystems/18-ci.md](subsystems/18-ci.md)

- [x] Cross-compile aarch64/riscv64 on every PR (compile only; QEMU boot of those arches is later)
- [x] Cross-arch QEMU **boot** (`scripts/qemu-cross-boot.sh`: `GATE_ARCH aarch64 boot` / `GATE_ARCH riscv64 boot`)
- [x] `cargo-audit`, license/SPDX check (CI + `scripts/license-check.sh`)
- [x] Tagged release → ISO → checksums → signatures (`scripts/make-iso.sh` + `release-checksums.sh`; GPG optional). PK/KEK/db + OVMF Secure Boot QEMU boot (`scripts/qemu-secure-boot.sh`).
- [x] Nightly main-branch images (`.github/workflows/nightly.yml`)
