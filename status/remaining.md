# Still open

← [Status index](README.md) · [Do this first](next.md)

Every remaining `- [ ]` in the subsystem files, plus unfinished Gate G. This is an inventory, not a sequence — start with [next.md](next.md).

## Gate G (quality bar)

| ID | Task | Done when |
|----|------|-----------|
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | Default kernel compiles **Live** code only |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | **Partial** — `bpf`/`pkey`/`process_mrelease`/`quotactl`/`remap_file_pages`/`io_uring_*`/`userfaultfd`/`perf_event_open`/`fanotify`/`io_setup`/`io_destroy`/`kexec`/`init_module`/`mount_setattr`/`fsopen`/`keyctl`/`ioperm`/`iopl`/`acct`/`swapon`/`swapoff`/`modify_ldt`/`sysfs`/`vhangup`/`lookup_dcookie`/`memfd_secret`/`uselib`/`pkey_mprotect`/`process_mrelease`/`set_mempolicy`/`get_mempolicy`/`mbind`/`migrate_pages`/`move_pages`/`sched_setattr`/`sched_getattr`/`futex_waitv`/`open_by_handle_at`/`name_to_handle_at`/`map_shadow_stack`/`ustat`/`set_thread_area`/`add_key`/`request_key`/`io_submit`/`io_getevents`/`io_pgetevents`/`fsconfig`/`fsmount`/`open_tree`/`move_mount`/`fspick`/`delete_module`/KVM vCPU regs return ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4, AP4, AQ4, AR4, AS4, AT4, AU4, AV4, AW4, AX4, AY4, AZ4, BA4, BB4, BC4, BD4, BE4, BF4, BG4, BH4, BI4, BJ4, BK4, BL4); remaining silent `Ok(0)` still exist |
| G5 | `unsafe` SAFETY comments + size budget | Clippy gate |

## 1. Kernel Core

Details: [subsystems/01-kernel-core.md](subsystems/01-kernel-core.md)

- [ ] **x86_64 / aarch64 / riscv64** — x86_64 boots; other arches compile with shims
- [ ] **ACPI AML / DSDT** — no interpreter; `acpi::shutdown` is QEMU port writes
- [ ] **UEFI runtime** — `get_time` hardcoded; `get_variable` → `None`
- [ ] **Kernel modules** — `modules.rs` / `kpm.rs` are name registries, not `.ko` loaders
- [ ] **IOMMU** — DMAR parse; `enable_translation` never called
- [ ] **Live patch** — writes a jmp over a symbol; no W^X, no activeness wait
- [ ] **Nested IRQ / TPR** — not used from ISRs

## 2. Memory Management

Details: [subsystems/02-memory.md](subsystems/02-memory.md)

- [ ] **NUMA** — SRAT parse; `allocate_node` returns an id, not memory
- [ ] **KSM** — hashes phys addr as a virt pointer; merge is a counter
- [ ] **THP** — `allocate_contiguous_frames` returns `None` above buddy max order
- [ ] **KASLR** for the kernel image
- [ ] **Memory cgroups** enforcement

## 3. Process & Scheduling

Details: [subsystems/03-process.md](subsystems/03-process.md)

- [ ] **sched_ext / eBPF** — not eBPF

## 4. Filesystem & Storage

Details: [subsystems/04-filesystem.md](subsystems/04-filesystem.md)

- [ ] NTFS — parses MFT from a provided buffer; **never calls the block layer**
- [ ] Btrfs / ZFS / XFS / exFAT / CIFS — in-memory or AHCI/NVMe fallback
- [ ] JBD2 journal replay — `replayed = 0` (persist WAL is live; ext4 JBD2 is not)
- [ ] Quotas — counters, not enforced

## 5. Networking Stack

Details: [subsystems/05-networking.md](subsystems/05-networking.md)

- [ ] IPv6 echo — builds packet, logs, no TX
- [ ] TLS 1.3 — types; GCM/X.509 stub; handshake flagged done after one recv
- [ ] Wi-Fi / WPA3 / bridge / VLAN / NAT — in-memory

## 6. Device Drivers

Details: [subsystems/06-drivers.md](subsystems/06-drivers.md)

- [ ] XHCI — rings/doorbells; timeouts “continue anyway”; fallback QEMU descriptors
- [ ] i915 / AMDGPU — register read/write; no modeset/firmware/GTT
- [ ] HDA — CORB/RIRB; DMA buffer is a heap pointer
- [ ] TPM TIS FIFO at `0xFED4_0000` — incomplete locality handshake
- [ ] VirtIO-GPU — **no virtqueue**; fake 1920×1080
- [ ] DRM — in-memory CRTC/GEM
- [ ] USB HID / hub / MSC / audio — parsers or flags, no URB completion loop
- [ ] e1000/rtl8139 — see networking; not used by `Socket::send`
- [ ] Touchscreen, trackpad, gamepad, GPIO, I2C, Thunderbolt, Wacom, UVC
- [ ] Broadcom Wi-Fi, AX211, USB Ethernet

## 7. GUI & Desktop Environment

Details: [subsystems/07-gui.md](subsystems/07-gui.md)

- [ ] **Wayland** — object model plus a live `/dev/wl0` present ioctl; one Ring 3 SHM client (F1–F2); no bind/listen Unix socket
- [ ] **GPU compositor** — `gpu_compositor.rs` never invoked; desktop software-blits
- [ ] Hardware cursor probed (`hw_cursor::init`); desktop still draws a software cursor
- [ ] VSync module unused; pacing is TSC ~60 FPS
- [ ] HDR, VRR, TrueType bytecode hinting, full IME framework

## 8. Shell & Terminal

Details: [subsystems/08-shell.md](subsystems/08-shell.md)

- [ ] Full VT100/xterm-256
- [ ] Here-docs, functions, `~/.profile` once a real home exists on disk
- [ ] Job control against **processes**, not kernel windows. Ring 3 `pipe()` is live (M2).

## 9. Security & Cryptography

Details: [subsystems/09-security.md](subsystems/09-security.md)

- [ ] AES-GCM, Poly1305, TLS 1.3 key schedule, X.509 chain verify
- [ ] RSA / ECDSA / Ed25519 / X25519 production-quality
- [ ] Kernel lockdown, secure wipe on free, IMA

## 10. System Services

Details: [subsystems/10-services.md](subsystems/10-services.md)

- [ ] No crash restart / watchdog / cgroup limits on units
- [ ] Cron needs a guaranteed timer caller
- [ ] udev-style hotplug → `/dev`
- [ ] Suspend/hibernate, logind, NetworkManager (activate logs “Starting DHCP”)
- [ ] PulseAudio/PipeWire mixing to HDA

## 11. Virtualization & Containers

Details: [subsystems/11-virt.md](subsystems/11-virt.md)

- [ ] `kvm::start_vm` sets `Running` and **does not** `vmlaunch`
- [ ] EPT, virtio device emulation for guests
- [ ] `container.rs` — OCI structs; start is comments (`fork`, `unshare`, `pivot_root`)
- [ ] cgroup v2 **enforcement** (CPU/memory/IO)

## 12. AI/ML Integration

Details: [subsystems/12-ai.md](subsystems/12-ai.md)

- [ ] Real transformer (attention, KV cache, production tokenizer vocab)
- [ ] `gpu_matmul.rs` only logs; `available: false`
- [ ] GUI assistant is placeholder text unless a model is loaded

## 13. Binary Compatibility & Runtime

Details: [subsystems/13-binary-compat.md](subsystems/13-binary-compat.md)

- [ ] Dynamic linker as a userspace `ld.so` (DT_NEEDED, lazy PLT, RELRO)
- [ ] `dlopen` that maps `.so` via VMM (`ldknoxos.rs` fake base)
- [ ] glibc/musl ABI for Ring 3

## 14. Internationalization & Fonts

Details: [subsystems/14-i18n.md](subsystems/14-i18n.md)

- [ ] Font fallback chain
- [ ] Real GSUB/GPOS / HarfBuzz-class shaping (Latin ligatures hardcoded)
- [ ] Color emoji
- [ ] `.mo` parser exists; runtime locale switch incomplete

## 15. Build System & Tooling

Details: [subsystems/15-build.md](subsystems/15-build.md)

- [ ] **`flake.nix`** — `make nix-build` would fail; file absent
- [ ] Cargo workspace unifying `kernel` + `boot` + `tools`
- [ ] Feature flags so Unused modules do not compile into the demo kernel
- [ ] Signed ISO / Secure Boot artifacts

## 16. Testing & Quality

Details: [subsystems/16-testing.md](subsystems/16-testing.md)

- [ ] No `#[test]` (expected for `no_std`, but host-side tests could exist)
- [ ] 300+ `unsafe` blocks without a systematic SAFETY audit
- [ ] No Miri, no fuzz of ELF/ext4/packet parsers, no coverage

## 17. Documentation

Details: [subsystems/17-docs.md](subsystems/17-docs.md)

- [ ] Architecture / syscall / driver / GUI guides
- [ ] `cargo doc` published
- [ ] Changelog

## 18. CI/CD & Release

Details: [subsystems/18-ci.md](subsystems/18-ci.md)

- [ ] Cross-compile + QEMU boot on every PR (x86_64 QEMU only so far)
- [ ] `cargo-audit`, license/SPDX check
- [ ] Tagged release → ISO → checksums → signatures
- [ ] Nightly main-branch images
