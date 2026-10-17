← [Status index](../README.md) · [Do this first](../next.md)

# 6. Device Drivers

**Grade: Wired (58%)** · `pci.rs`, `usb.rs`, `ahci.rs`, `nvme.rs`, `virtio_gpu.rs`, `i915.rs`, …

### Live
- [x] PCI config `0xCF8`/`0xCFC`, BDF scan, BAR decode
- [x] PS/2 keyboard + mouse (interrupt-driven)
- [x] UART TX
- [x] VirtIO-blk
- [x] AHCI command-list DMA (C4)
- [x] NVMe admin/I/O queue DMA (C5)
- [x] VGA/BGA framebuffer from bootloader
- [x] VirtIO-GPU controlq 2D scanout + cursorq (`GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor`)
- [x] RTC, HPET registers (timer path uses APIC + PIT)
- [x] XHCI BAR0 mapped uncached + buddy DMA rings (`GATE_USB xhci mmio`); Transfer Events drain into HID

### Partial MMIO (not a full driver)
- [ ] i915 / AMDGPU — register read/write; no modeset/firmware/GTT
- [x] HDA — CORB/RIRB + PCM BDL from buddy frames (`GATE_HDA dma`)
- [ ] TPM TIS FIFO at `0xFED4_0000` — incomplete locality handshake

### Stub
- [x] VirtIO-GPU — controlq + cursorq vs QEMU `-device virtio-gpu-pci` (F8). 3D/Virgl not negotiated.
- [ ] DRM — in-memory CRTC/GEM
- [x] USB HID URB completion — submit + complete + boot-protocol parse → evdev (`GATE_USB hid urb`). Transfer Events feed the same handler and the GUI mouse/key path. Hub / MSC / audio still stub.
- [ ] e1000/rtl8139 — see networking; not used by `Socket::send`
- [ ] Touchscreen, trackpad, gamepad, GPIO, I2C, Thunderbolt, Wacom, UVC
- [ ] Broadcom Wi-Fi, AX211, USB Ethernet

### Perfect-OS next steps
1. One storage, one net, one GPU scanout, one USB HID — **finished**, not 40 started.
2. Storage: VirtIO-blk production-quality; AHCI DMA live (C4); NVMe DMA live (C5).
3. Net: VirtIO-net rings + DHCP + DNS/TCP live (D2–D4); e1000 still unused by sockets.
4. Display: VirtIO-GPU 2D resource + scanout live (F8); VGA software FB remains the compositor present path.
5. Input: USB HID URB completion loop is live (`GATE_USB hid urb`). XHCI BAR0 is mapped (`GATE_USB xhci mmio`). Desktop cursor is proven without PS/2 (`GATE_USB no ps2 cursor`); i8042 aux is not enabled.

---
