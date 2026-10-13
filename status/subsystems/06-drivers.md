← [Status index](../README.md) · [Do this first](../next.md)

# 6. Device Drivers

**Grade: Wired (40%)** · `pci.rs`, `usb.rs`, `ahci.rs`, `nvme.rs`, `i915.rs`, …

### Live
- [x] PCI config `0xCF8`/`0xCFC`, BDF scan, BAR decode
- [x] PS/2 keyboard + mouse (interrupt-driven)
- [x] UART TX
- [x] VirtIO-blk
- [x] AHCI command-list DMA (C4)
- [x] NVMe admin/I/O queue DMA (C5)
- [x] VGA/BGA framebuffer from bootloader
- [x] RTC, HPET registers (timer path uses APIC + PIT)

### Partial MMIO (not a full driver)
- [ ] XHCI — rings/doorbells; timeouts “continue anyway”; fallback QEMU descriptors
- [ ] i915 / AMDGPU — register read/write; no modeset/firmware/GTT
- [ ] HDA — CORB/RIRB; DMA buffer is a heap pointer
- [ ] TPM TIS FIFO at `0xFED4_0000` — incomplete locality handshake

### Stub
- [ ] VirtIO-GPU — **no virtqueue**; fake 1920×1080
- [ ] DRM — in-memory CRTC/GEM
- [ ] USB HID / hub / MSC / audio — parsers or flags, no URB completion loop
- [ ] e1000/rtl8139 — see networking; not used by `Socket::send`
- [ ] Touchscreen, trackpad, gamepad, GPIO, I2C, Thunderbolt, Wacom, UVC
- [ ] Broadcom Wi-Fi, AX211, USB Ethernet

### Perfect-OS next steps
1. One storage, one net, one GPU scanout, one USB HID — **finished**, not 40 started.
2. Storage: VirtIO-blk production-quality; AHCI DMA live (C4); NVMe DMA live (C5).
3. Net: VirtIO-net rings + DHCP + DNS/TCP live (D2–D4); e1000 still unused by sockets.
4. Display: VirtIO-GPU 2D resource + scanout **or** keep software FB until Ring 3 exists.
5. Input: USB HID interrupt-IN so the desktop works without PS/2.

---
