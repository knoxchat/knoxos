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
- [x] **ACPI AML / DSDT** — DSDT mapped; `Name(_S5)` SLP_TYP extracted (`GATE_AML dsdt`). `acpi::shutdown` still has QEMU port fallbacks
- [x] **UEFI runtime** — `GetTime` from CMOS RTC; `SetVariable`/`GetVariable` round-trip (`GATE_UEFI time`)
- [x] **Kernel modules** — ELF ET_REL `.ko` load + reloc + `init_module` (`GATE_KO load`)
- [x] **IOMMU** — root table from buddy; `enable_translation` called (`GATE_IOMMU translation`)
- [ ] **Live patch** — writes a jmp over a symbol; no W^X, no activeness wait
- [ ] **Nested IRQ / TPR** — not used from ISRs

## 2. Memory Management

Details: [subsystems/02-memory.md](subsystems/02-memory.md)

- [ ] **NUMA** — SRAT parse; `allocate_node` returns an id, not memory
- [ ] **KSM** — hashes phys addr as a virt pointer; merge is a counter
- [x] **THP** — buddy order-9 2 MiB allocation (`GATE_THP hugepage`)
- [x] **KASLR** for loadable kernel mappings (`GATE_KASLR slide`)
- [x] **Memory cgroups** enforcement — mmap/brk charge; hard `memory.max` is ENOMEM (`GATE_CGROUP memory`, `GATE_CGROUP2 memory`)

## 3. Process & Scheduling

Details: [subsystems/03-process.md](subsystems/03-process.md)

- [ ] **sched_ext / eBPF** — not eBPF

## 4. Filesystem & Storage

Details: [subsystems/04-filesystem.md](subsystems/04-filesystem.md)

- [x] NTFS — boot + MFT `$DATA` through the ramdisk block layer (`GATE_NTFS block`)
- [ ] Btrfs / ZFS / XFS / exFAT / CIFS — in-memory or AHCI/NVMe fallback
- [x] JBD2 journal replay — descriptor + commit applied; torn descriptor discarded (`GATE_C8 jbd2 replay`)
- [x] Quotas — `write_file_checked` returns `EDQUOT` (`GATE_QUOTA enforce`)

## 5. Networking Stack

Details: [subsystems/05-networking.md](subsystems/05-networking.md)

- [x] IPv6 echo — loopback `::1` Echo Request produces a TX reply (`GATE_D6 ipv6 echo`)
- [ ] TLS 1.3 — types; GCM/X.509 stub; handshake flagged done after one recv
- [ ] Wi-Fi / WPA3 / bridge / VLAN / NAT — in-memory

## 6. Device Drivers

Details: [subsystems/06-drivers.md](subsystems/06-drivers.md)

- [x] XHCI BAR0 mapped uncached + buddy DMA rings (`GATE_USB xhci mmio`). Transfer Events drain into HID. Enumeration still has timeout fallbacks.
- [ ] i915 / AMDGPU — register read/write; no modeset/firmware/GTT
- [x] HDA — CORB/RIRB + PCM BDL from buddy frames (`GATE_HDA dma`)
- [ ] TPM TIS FIFO at `0xFED4_0000` — incomplete locality handshake
- [x] VirtIO-GPU — controlq + cursorq vs QEMU `-device virtio-gpu-pci` (`GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor`)
- [ ] DRM — in-memory CRTC/GEM
- [x] USB HID URB completion — submit + complete + boot-protocol parse (`GATE_USB hid urb`). Transfer Events feed the same handler and the GUI mouse/key path. Hub / MSC / audio still stub.
- [ ] e1000/rtl8139 — see networking; not used by `Socket::send`
- [ ] Touchscreen, trackpad, gamepad, GPIO, I2C, Thunderbolt, Wacom, UVC
- [ ] Broadcom Wi-Fi, AX211, USB Ethernet

## 7. GUI & Desktop Environment

Details: [subsystems/07-gui.md](subsystems/07-gui.md)

- [x] **Wayland protocol** — `/dev/wl0` ioctl (F1–F2) plus AF_UNIX `/run/wayland-0` (F5) plus `wl_display` object opcodes (`GATE_WL display protocol`)
- [x] **GPU compositor** — `gpu_compositor` init + VSync; desktop `present_rect` flushes through VirtIO-GPU (`GATE_GPU desktop scanout`). VGA blit remains for `-vga std`.
- [x] Hardware cursor — VirtIO-GPU cursorq `UPDATE_CURSOR` / `MOVE_CURSOR`; desktop skips software blit (`GATE_F8 hw cursor`)
- [x] VSync — `gpu_compositor` software VBlank flip on the live path (`GATE_VSYNC paced`); desktop still software-blits to VGA
- [ ] HDR, VRR, TrueType bytecode hinting, full IME framework
- [x] Ring 3 `openpty` + SHM terminal client (`GATE_TERM pty userspace`). Start-menu Terminal is Ring 3; master fd + slave I/O (`GATE_TERM pty interactive`).

## 8. Shell & Terminal

Details: [subsystems/08-shell.md](subsystems/08-shell.md)

- [x] Full VT100/xterm-256 — CUP + SGR 256-color + ED (`GATE_VT100 xterm`)
- [x] Here-docs, functions, `~/.profile` (`GATE_HEREDOC expand`, `GATE_PROFILE sourced`)
- [x] Job control against **processes** — SIGTSTP stops; `waitpid(WUNTRACED)` reports; SIGCONT resumes (`GATE_JOB stop`). Ring 3 `pipe()` is live (M2).

## 9. Security & Cryptography

Details: [subsystems/09-security.md](subsystems/09-security.md)

- [ ] AES-GCM, Poly1305, TLS 1.3 key schedule, X.509 chain verify
- [ ] RSA / ECDSA / Ed25519 / X25519 production-quality
- [x] Kernel lockdown, secure wipe on free, IMA — unsigned `insert_module` denied (`GATE_LOCKDOWN module`); buddy free zeros the frame (`GATE_WIPE free`); exec measures SHA-256 (`GATE_IMA exec`)

## 10. System Services

Details: [subsystems/10-services.md](subsystems/10-services.md)

- [x] Crash restart of failed units (`GATE_SVC restart`). No cgroup limits on units.
- [x] Cron `@reboot` plus PIT/APIC due-flag (`GATE_CRON reboot`)
- [x] udev-style hotplug → `/dev` from PCI scan (`GATE_UDEV hotplug`)
- [x] NetworkManager activate copies live `eth0` DHCP (`GATE_NM dhcp`)
- [ ] Suspend/hibernate, logind
- [x] PulseAudio mixing to HDA (`GATE_PA mix`)

## 11. Virtualization & Containers

Details: [subsystems/11-virt.md](subsystems/11-virt.md)

- [ ] `kvm::start_vm` sets `Running` and **does not** `vmlaunch`
- [ ] EPT, virtio device emulation for guests
- [ ] `container.rs` — OCI structs; start is comments (`fork`, `unshare`, `pivot_root`)
- [x] cgroup v2 **memory** enforcement (`GATE_CGROUP2 memory`). CPU/IO still counters.

## 12. AI/ML Integration

Details: [subsystems/12-ai.md](subsystems/12-ai.md)

- [ ] Real transformer (attention, KV cache, production tokenizer vocab)
- [ ] `gpu_matmul.rs` only logs; `available: false`
- [ ] GUI assistant is placeholder text unless a model is loaded

## 13. Binary Compatibility & Runtime

Details: [subsystems/13-binary-compat.md](subsystems/13-binary-compat.md)

- [x] Dynamic linker as a userspace `ld.so` (DT_NEEDED, lazy PLT, RELRO) — **Gate BM1**
- [x] `dlopen` that maps `.so` via VMM (`open` + file-backed `mmap`, not `ldknoxos.rs` fake base) — **Gate BM2**
- [x] Static musl CRT ABI (`_start` → `__libc_start_main` → `main`) — **Gate BM3**
- [x] Dynamically linked musl binary for Ring 3 (`PT_INTERP` + `DT_NEEDED libc.so` + `__libc_start_main@plt`) — **Gate BM4**

## 14. Internationalization & Fonts

Details: [subsystems/14-i18n.md](subsystems/14-i18n.md)

- [ ] Font fallback chain
- [ ] Real GSUB/GPOS / HarfBuzz-class shaping (Latin ligatures hardcoded)
- [ ] Color emoji
- [ ] `.mo` parser exists; runtime locale switch incomplete

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
- [x] Host parser tests (ELF / ICMPv6 / cron / ext4 magic / quota KiB)
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
