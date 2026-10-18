# Drivers (live vs stub)

QEMU is the hardware. Tick a driver only when the virtqueue, DMA, or IRQ path round-trips on that machine.

## Live on the boot path

| Device | Evidence |
|--------|----------|
| PCI config `0xCF8`/`0xCFC` | BAR decode, bus scan |
| UART | Serial markers |
| PS/2 kbd/mouse | IRQ1 / IRQ12 aux fallback (`gui::input::init_mouse`) |
| VirtIO-input | modern `virtio-tablet-pci` eventq; `GATE_VIRTIO input tablet` |
| VirtIO-blk | Guest-physical virtqueue; persist C1–C7 |
| VirtIO-net | TX/RX rings; DHCP/DNS/TCP (D2–D5) |
| VirtIO-GPU | controlq + cursorq; `GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor` |
| AHCI | Command-list + PRDT DMA (`GATE_C4 ahci dma complete`) |
| NVMe | Admin + I/O queues, PRP bounce (`GATE_C5 nvme dma complete`) |
| VGA/BGA FB | Bootloader framebuffer; software present |

QEMU flags used by `./run.sh` and `tests/run_integration.sh`: `-drive if=virtio`, `-device ahci`, `-device nvme`, `-device virtio-net-pci,disable-modern=on`, `-device virtio-gpu-pci`, `-device virtio-tablet-pci`, `-device virtio-keyboard-pci`, `-device qemu-xhci`, `-device intel-hda`, `-vga std`.

## Wired, not the live input/display path

- USB XHCI: BAR0 mapped uncached (`GATE_USB xhci mmio`). HID URB submit + complete + boot-protocol parse is a software KAT (`GATE_USB hid urb`). QEMU `usb-tablet` reports are 5-byte digitizers (`set_absolute_mouse`). Enumeration still has timeout fallbacks; `./run.sh` pointer is VirtIO-tablet first.
- USB class KATs: MSC, hub, gamepad, audio, trackpad, Wacom, UVC, CDC ECM, HID digitizer (`GATE_USB audio` / `trackpad` / `wacom` / `uvc` / `ethernet` / `touch`).
- GPIO pin0→pin1 loopback (`GATE_GPIO xfer`).
- i915 modeset in shadow MMIO (`GATE_I915 modeset`). AMDGPU firmware + CRTC0 (`GATE_AMDGPU fw`).
- e1000 `Socket::send` loopback (`GATE_E1000 send`); rtl8139 TX→RX status-header loopback (`GATE_RTL8139 send`).
- AX211 iwlwifi TLV firmware + Beacon inject (`GATE_AX211 fw`).
- Broadcom `BRCMFW` TLV + scan SSID (`GATE_BRCM fw`).
- Thunderbolt route hop + PCIe tunnel (`GATE_TB hop`).
- GPU matmul CPU fallback KAT (`GATE_GPU matmul`).

## Stub (do not count)

TPM measured boot, nested VT-x `VMXON` on real hardware. Full inventory: [status/subsystems/06-drivers.md](../status/subsystems/06-drivers.md).

## Next (from [status/next.md](../status/next.md))

XHCI BAR0 is mapped (`GATE_USB xhci mmio`). Keep PS/2 as fallback until a boot with PS/2 removed still moves the desktop cursor from Transfer Events.
