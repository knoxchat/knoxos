← [Status index](../README.md) · [Do this first](../next.md)

# 6. Device Drivers

**Grade: Wired (74%)** · `pci.rs`, `usb.rs`, `ahci.rs`, `nvme.rs`, `virtio_gpu.rs`, `i915.rs`, …

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
- [x] i915 modeset — pipe A + plane + GTT[0] in shadow MMIO (`GATE_I915 modeset`). AMDGPU firmware later.
- [x] HDA — CORB/RIRB + PCM BDL from buddy frames (`GATE_HDA dma`)
- [x] TPM TIS FIFO — locality 0 REQUEST_USE → VALID|ACTIVE (`GATE_TPM tis`). Hardware 0xFED4_0000 still unmapped on stock QEMU.

### Stub
- [x] VirtIO-GPU — controlq + cursorq vs QEMU `-device virtio-gpu-pci` (F8). 3D/Virgl not negotiated.
- [x] DRM — GEM from buddy frames + CRTC set (`GATE_DRM gem`). HDR PQ + VRR (`GATE_HDR pq`, `GATE_VRR range`)
- [x] USB HID URB completion — submit + complete + boot-protocol parse → evdev (`GATE_USB hid urb`). Transfer Events feed the same handler and the GUI mouse/key path.
- [x] USB MSC — SCSI READ(10) vs ramdisk LUN (`GATE_USB msc`). Audio still stub.
- [x] USB hub — descriptor type 0x29 + GetPortStatus (`GATE_USB hub`)
- [x] USB HID gamepad — 8-byte report parse (`GATE_USB gamepad`)
- [x] I2C SMBus EEPROM write/read (`GATE_I2C xfer`)
- [x] e1000 — `Socket::send` programs the TX ring; loopback RX (`GATE_E1000 send`). rtl8139 later.
- [ ] Touchscreen, trackpad, GPIO, Thunderbolt, Wacom, UVC
- [ ] Broadcom Wi-Fi, AX211, USB Ethernet

### Perfect-OS next steps
1. One storage, one net, one GPU scanout, one USB HID — **finished**, not 40 started.
2. Storage: VirtIO-blk production-quality; AHCI DMA live (C4); NVMe DMA live (C5).
3. Net: VirtIO-net rings + DHCP + DNS/TCP live (D2–D4); e1000 `Socket::send` loopback live (`GATE_E1000 send`).
4. Display: VirtIO-GPU 2D resource + scanout live (F8); VGA software FB remains the compositor present path.
5. Input: USB HID URB completion loop is live (`GATE_USB hid urb`). XHCI BAR0 is mapped (`GATE_USB xhci mmio`). Desktop cursor is proven without PS/2 (`GATE_USB no ps2 cursor`); i8042 aux is not enabled.

---
