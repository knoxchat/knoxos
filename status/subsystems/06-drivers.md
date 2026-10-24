← [Status index](../README.md) · [Do this first](../next.md)

# 6. Device Drivers

**Grade: Wired (93%)** · `pci.rs`, `usb.rs`, `ahci.rs`, `nvme.rs`, `virtio_gpu.rs`, `i915.rs`, …

### Live
- [x] PCI config `0xCF8`/`0xCFC`, BDF scan, BAR decode
- [x] PS/2 keyboard + mouse (interrupt-driven)
- [x] UART TX
- [x] VirtIO-blk
- [x] AHCI command-list DMA (C4)
- [x] NVMe admin/I/O queue DMA (C5)
- [x] VGA/BGA framebuffer from bootloader
- [x] VirtIO-GPU controlq 2D scanout + cursorq (`GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor`)
- [x] VirtIO-input modern tablet/keyboard (`GATE_VIRTIO input tablet`)
- [x] RTC, HPET registers (timer path uses APIC + PIT)
- [x] XHCI BAR0 mapped uncached + buddy DMA rings (`GATE_USB xhci mmio`); Enable Slot Command Completion cc=1 (`GATE_USB xhci slot`); Address Device Command Completion cc=1 (`GATE_USB xhci addr`); GET_DESCRIPTOR Transfer Event (`GATE_USB xhci desc`); SET_CONFIGURATION Transfer Event (`GATE_USB xhci config`); Configure Endpoint Command Completion cc=1 (`GATE_USB xhci ep`); GET_DESCRIPTOR(Configuration) plus full enum with no timeout fallback (`GATE_USB xhci enum`); GET_STATUS Transfer Event (`GATE_USB xhci status`); SET_INTERFACE Transfer Event (`GATE_USB xhci iface`); GET_CONFIGURATION Transfer Event (`GATE_USB xhci getcfg`); GET_INTERFACE Transfer Event (`GATE_USB xhci getiface`); CLEAR_FEATURE Transfer Event (`GATE_USB xhci clear`); bulk OUT Normal TRB Transfer Event (`GATE_USB xhci bulk`); SET_FEATURE Transfer Event (`GATE_USB xhci set`); bulk IN Normal TRB Transfer Event (`GATE_USB xhci bulkin`); SET_DESCRIPTOR Transfer Event (`GATE_USB xhci stdesc`); isochronous IN Isoch TRB Transfer Event (`GATE_USB xhci isoch`); GET_DESCRIPTOR(String) Transfer Event (`GATE_USB xhci string`); interrupt IN Normal TRB Transfer Event (`GATE_USB xhci intr`); No-Op Command Completion cc=1 (`GATE_USB xhci noop`); Disable Slot Command Completion cc=1 (`GATE_USB xhci disable`); Evaluate Context Command Completion cc=1 (`GATE_USB xhci eval`); Stop Endpoint bulk OUT (DCI 2) Command Completion cc=1 (`GATE_USB xhci stop`); Reset Endpoint Halted bulk IN (DCI 5) Command Completion cc=1 (`GATE_USB xhci resetep`); Set TR Dequeue Pointer Stopped extra pipe Command Completion cc=1 (`GATE_USB xhci settr`); Reset Device already-addressed slot Command Completion cc=1 (`GATE_USB xhci resetdev`); Transfer No-Op bulk OUT (DCI 2) Transfer Event (`GATE_USB xhci trnoop`); GET_DESCRIPTOR(Device Qualifier) Transfer Event (`GATE_USB xhci qual`); GET_DESCRIPTOR(BOS) Transfer Event (`GATE_USB xhci bos`); GET_DESCRIPTOR(Other Speed Configuration) Transfer Event (`GATE_USB xhci oscfg`); SYNCH_FRAME Transfer Event (`GATE_USB xhci synch`); SET_SEL Transfer Event (`GATE_USB xhci setsel`); SET_ISOCH_DELAY Transfer Event (`GATE_USB xhci isochdelay`); GET_DESCRIPTOR(HID) Transfer Event (`GATE_USB xhci hiddesc`); GET_DESCRIPTOR(HID Report) Transfer Event (`GATE_USB xhci hidreport`); GET_REPORT Transfer Event (`GATE_USB xhci getreport`); SET_REPORT Transfer Event (`GATE_USB xhci setreport`); Transfer Events drain into HID

### Partial MMIO (not a full driver)
- [x] i915 modeset — pipe A + plane + GTT[0] in shadow MMIO (`GATE_I915 modeset`). AMDGPU firmware (`GATE_AMDGPU fw`).
- [x] HDA — CORB/RIRB + PCM BDL from buddy frames (`GATE_HDA dma`)
- [x] TPM TIS FIFO — locality 0 REQUEST_USE → VALID|ACTIVE (`GATE_TPM tis`). Software SHA-256 PCR extend (`GATE_TPM pcr`). Hardware 0xFED4_0000 still unmapped on stock QEMU.

### Stub
- [x] VirtIO-GPU — controlq + cursorq vs QEMU `-vga virtio` (F8 + modern stack). 3D/Virgl not negotiated.
- [x] DRM — GEM from buddy frames + CRTC set (`GATE_DRM gem`). HDR PQ + VRR (`GATE_HDR pq`, `GATE_VRR range`)
- [x] USB HID URB completion — submit + complete + boot-protocol parse → evdev (`GATE_USB hid urb`). Transfer Events feed the same handler and the GUI mouse/key path.
- [x] USB MSC — SCSI READ(10) vs ramdisk LUN (`GATE_USB msc`). USB Audio SET_CUR + PCM (`GATE_USB audio`).
- [x] USB hub — descriptor type 0x29 + GetPortStatus (`GATE_USB hub`)
- [x] USB HID gamepad — 8-byte report parse (`GATE_USB gamepad`)
- [x] I2C SMBus EEPROM write/read (`GATE_I2C xfer`)
- [x] e1000 — `Socket::send` programs the TX ring; loopback RX (`GATE_E1000 send`). rtl8139 TX→RX (`GATE_RTL8139 send`).
- [x] Touchscreen HID digitizer (`GATE_USB touch`); trackpad two-finger (`GATE_USB trackpad`); GPIO loopback (`GATE_GPIO xfer`); Wacom pen (`GATE_USB wacom`); UVC YUY2 (`GATE_USB uvc`)
- [x] USB Ethernet CDC ECM loopback (`GATE_USB ethernet`). AX211 iwlwifi TLV (`GATE_AX211 fw`). Broadcom `BRCMFW` (`GATE_BRCM fw`). Thunderbolt hop (`GATE_TB hop`).

### Perfect-OS next steps
1. One storage, one net, one GPU scanout, one USB HID — **finished**, not 40 started.
2. Storage: VirtIO-blk production-quality; AHCI DMA live (C4); NVMe DMA live (C5).
3. Net: VirtIO-net rings + DHCP + DNS/TCP live (D2–D4); e1000 `Socket::send` loopback live (`GATE_E1000 send`).
4. Display: VirtIO-GPU 2D resource + scanout is the primary present path on `-vga virtio` (F8 + `GATE_GPU modern stack`). VGA blit remains only for a secondary `-vga std` head.
5. Input: USB HID URB completion loop is live (`GATE_USB hid urb`). XHCI BAR0 is mapped (`GATE_USB xhci mmio`). Enable Slot Command Completion is live (`GATE_USB xhci slot`). Address Device Command Completion is live (`GATE_USB xhci addr`). GET_DESCRIPTOR Transfer Event is live (`GATE_USB xhci desc`). SET_CONFIGURATION Transfer Event is live (`GATE_USB xhci config`). Configure Endpoint Command Completion is live (`GATE_USB xhci ep`). Full enumeration including GET_DESCRIPTOR(Configuration) requires live events (`GATE_USB xhci enum`). GET_STATUS Transfer Event is live (`GATE_USB xhci status`). SET_INTERFACE Transfer Event is live (`GATE_USB xhci iface`). GET_CONFIGURATION Transfer Event is live (`GATE_USB xhci getcfg`). GET_INTERFACE Transfer Event is live (`GATE_USB xhci getiface`). CLEAR_FEATURE Transfer Event is live (`GATE_USB xhci clear`). bulk OUT Normal TRB Transfer Event is live (`GATE_USB xhci bulk`). SET_FEATURE Transfer Event is live (`GATE_USB xhci set`). bulk IN Normal TRB Transfer Event is live (`GATE_USB xhci bulkin`). SET_DESCRIPTOR Transfer Event is live (`GATE_USB xhci stdesc`). isochronous IN Isoch TRB Transfer Event is live (`GATE_USB xhci isoch`). GET_DESCRIPTOR(String) Transfer Event is live (`GATE_USB xhci string`). interrupt IN Normal TRB Transfer Event is live (`GATE_USB xhci intr`). No-Op Command Completion is live (`GATE_USB xhci noop`). Disable Slot Command Completion is live (`GATE_USB xhci disable`). Evaluate Context Command Completion is live (`GATE_USB xhci eval`). Stop Endpoint bulk OUT (DCI 2) Command Completion is live (`GATE_USB xhci stop`). Reset Endpoint Halted bulk IN (DCI 5) Command Completion is live (`GATE_USB xhci resetep`). Set TR Dequeue Pointer Stopped extra pipe Command Completion is live (`GATE_USB xhci settr`). Reset Device already-addressed slot Command Completion is live (`GATE_USB xhci resetdev`). Transfer No-Op bulk OUT (DCI 2) Transfer Event is live (`GATE_USB xhci trnoop`). GET_DESCRIPTOR(Device Qualifier) Transfer Event is live (`GATE_USB xhci qual`). GET_DESCRIPTOR(BOS) Transfer Event is live (`GATE_USB xhci bos`). GET_DESCRIPTOR(Other Speed Configuration) Transfer Event is live (`GATE_USB xhci oscfg`). SYNCH_FRAME Transfer Event is live (`GATE_USB xhci synch`). SET_SEL Transfer Event is live (`GATE_USB xhci setsel`). SET_ISOCH_DELAY Transfer Event is live (`GATE_USB xhci isochdelay`). GET_DESCRIPTOR(HID) Transfer Event is live (`GATE_USB xhci hiddesc`). GET_DESCRIPTOR(HID Report) Transfer Event is live (`GATE_USB xhci hidreport`). GET_REPORT Transfer Event is live (`GATE_USB xhci getreport`). SET_REPORT Transfer Event is live (`GATE_USB xhci setreport`). Desktop cursor is VirtIO-tablet + USB digitizer + PS/2 aux.

---
