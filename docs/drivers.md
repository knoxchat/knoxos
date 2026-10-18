# Drivers (live vs stub)

QEMU is the hardware. Tick a driver only when the virtqueue, DMA, or IRQ path round-trips on that machine.

## Live on the boot path

| Device | Evidence |
|--------|----------|
| PCI config `0xCF8`/`0xCFC` | BAR decode, bus scan |
| UART | Serial markers |
| PS/2 kbd/mouse | IRQ1 / IRQ12; desktop input |
| VirtIO-blk | Guest-physical virtqueue; persist C1–C7 |
| VirtIO-net | TX/RX rings; DHCP/DNS/TCP (D2–D5) |
| VirtIO-GPU | controlq + cursorq; `GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor` |
| AHCI | Command-list + PRDT DMA (`GATE_C4 ahci dma complete`) |
| NVMe | Admin + I/O queues, PRP bounce (`GATE_C5 nvme dma complete`) |
| VGA/BGA FB | Bootloader framebuffer; software present |

QEMU flags used by `./run.sh` and `tests/run_integration.sh`: `-drive if=virtio`, `-device ahci`, `-device nvme`, `-device virtio-net-pci,disable-modern=on`, `-device virtio-gpu-pci`, `-device qemu-xhci`, `-device intel-hda`, `-vga std`.

## Wired, not the live input/display path

- USB XHCI: BAR0 mapped uncached (`GATE_USB xhci mmio`). HID URB submit + complete + boot-protocol parse is live (`GATE_USB hid urb`). Transfer Events drain into that handler and the GUI mouse/key path. PS/2 remains the fallback. Enumeration still has timeout fallbacks.
- USB class KATs: MSC, hub, gamepad, audio, trackpad, Wacom, UVC, CDC ECM, HID digitizer (`GATE_USB audio` / `trackpad` / `wacom` / `uvc` / `ethernet` / `touch`).
- GPIO pin0→pin1 loopback (`GATE_GPIO xfer`).
- i915 / AMDGPU: register r/w; no modeset.
- e1000: probed; sockets use VirtIO-net.

## Stub (do not count)

ZFS, WPA3, TPM measured boot, Thunderbolt, Broadcom Wi-Fi, AX211, GPU compute. Full inventory: [status/subsystems/06-drivers.md](../status/subsystems/06-drivers.md).

## Next (from [status/next.md](../status/next.md))

XHCI BAR0 is mapped (`GATE_USB xhci mmio`). Keep PS/2 as fallback until a boot with PS/2 removed still moves the desktop cursor from Transfer Events.
