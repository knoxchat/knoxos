# Changelog

## Unreleased

- **Gate D6** — IPv6 echo actually TXes: loopback `::1` injects ICMPv6 Echo Request, reply is sent (`GATE_D6 ipv6 echo`). VirtIO-net RX `0x86DD` feeds `process_ipv6_packet`.
- **Service restart** — failed units with `OnFailure`/`Always` are brought back (`GATE_SVC restart`).
- **udev hotplug** — PCI scan creates `/dev` nodes and drains kobject ADD uevents (`GATE_UDEV hotplug`).
- **Quota enforcement** — VFS `write_file_checked` returns `EDQUOT` when the hard block limit is exceeded (`GATE_QUOTA enforce`).
- **Cron timer** — `@reboot` jobs run at init (`GATE_CRON reboot`); the PIT/APIC tick sets a due flag so `tick` is not desktop-loop-only.
- **VSync pacing** — `gpu_compositor` is on the live path; software VBlank flips a page (`GATE_VSYNC paced`).
- **Ring 3 PTY terminal** — a userspace client `openpty`s and presents SHM Empty, not `WindowContentType::Terminal` (`GATE_TERM pty userspace`). Interactive start-menu Terminal still uses the in-kernel PTY for live I/O.
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
