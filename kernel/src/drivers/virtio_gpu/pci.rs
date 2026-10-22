/// PCI discovery and VirtIO capability-region MMIO for virtio-gpu.
use core::sync::atomic::{AtomicBool, Ordering};

use crate::pci;
use crate::serial_println;

/// Set when the VirtIO GPU is a VGA-class device (`virtio-vga` / `-vga virtio`).
/// That head is the cocoa/GTK window. `virtio-gpu-pci` plus `-vga std` is not.
static PRIMARY_DISPLAY: AtomicBool = AtomicBool::new(false);

/// True when the live VirtIO GPU is the primary (VGA-class) display.
pub fn is_primary_display() -> bool {
    PRIMARY_DISPLAY.load(Ordering::Relaxed)
}

const VIRTIO_PCI_VENDOR: u16 = 0x1AF4;
const VIRTIO_GPU_DEVICE_MODERN: u16 = 0x1050;
const VIRTIO_GPU_DEVICE_LEGACY: u16 = 0x1010;
const VIRTIO_ID_GPU: u16 = 16;

const VIRTIO_PCI_CAP_COMMON: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY: u8 = 2;
const VIRTIO_PCI_CAP_ISR: u8 = 3;
const PCI_CAP_ID_VNDR: u8 = 0x09;

pub(super) const CFG_DEVICE_FEATURE_SELECT: u64 = 0;
pub(super) const CFG_DEVICE_FEATURE: u64 = 4;
pub(super) const CFG_GUEST_FEATURE_SELECT: u64 = 8;
pub(super) const CFG_GUEST_FEATURE: u64 = 12;
pub(super) const CFG_DEVICE_STATUS: u64 = 20;
pub(super) const CFG_QUEUE_SELECT: u64 = 22;
pub(super) const CFG_QUEUE_SIZE: u64 = 24;
pub(super) const CFG_QUEUE_MSIX: u64 = 26;
pub(super) const CFG_QUEUE_ENABLE: u64 = 28;
pub(super) const CFG_QUEUE_NOTIFY_OFF: u64 = 30;
pub(super) const CFG_QUEUE_DESC: u64 = 32;
pub(super) const CFG_QUEUE_DRIVER: u64 = 40;
pub(super) const CFG_QUEUE_DEVICE: u64 = 48;

pub(super) fn mmio_r8(addr: u64) -> u8 {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::read_volatile(addr as *const u8) }
}
pub(super) fn mmio_r16(addr: u64) -> u16 {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::read_volatile(addr as *const u16) }
}
pub(super) fn mmio_r32(addr: u64) -> u32 {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}
pub(super) fn mmio_w8(addr: u64, v: u8) {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::write_volatile(addr as *mut u8, v) }
}
pub(super) fn mmio_w16(addr: u64, v: u16) {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::write_volatile(addr as *mut u16, v) }
}
pub(super) fn mmio_w32(addr: u64, v: u32) {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::write_volatile(addr as *mut u32, v) }
}
pub(super) fn mmio_w64(addr: u64, v: u64) {
    mmio_w32(addr, v as u32);
    mmio_w32(addr + 4, (v >> 32) as u32);
}

fn pci_bar_phys(bus: u8, dev: u8, func: u8, bar: u8) -> Option<u64> {
    let off = 0x10 + bar * 4;
    let lo = pci::pci_config_read32(bus, dev, func, off);
    if lo == 0 {
        return None;
    }
    if lo & 1 != 0 {
        return None;
    }
    let mut phys = (lo & 0xFFFF_FFF0) as u64;
    if (lo >> 1) & 3 == 2 {
        let hi = pci::pci_config_read32(bus, dev, func, off + 4);
        phys |= (hi as u64) << 32;
    }
    if phys == 0 { None } else { Some(phys) }
}

fn map_cap_region(bus: u8, dev: u8, func: u8, bar: u8, offset: u32, length: u32) -> Option<u64> {
    let phys = pci_bar_phys(bus, dev, func, bar)?;
    let bytes = (length as usize).max(0x1000);
    crate::vmm::map_mmio(phys + offset as u64, bytes)
}

pub(super) struct VirtioCaps {
    pub(super) common: u64,
    pub(super) notify: u64,
    pub(super) notify_mult: u32,
    pub(super) isr: u64,
}

pub(super) fn parse_virtio_caps(bus: u8, dev: u8, func: u8) -> Option<VirtioCaps> {
    let status = pci::pci_config_read16(bus, dev, func, 0x06);
    if status & 0x10 == 0 {
        return None;
    }
    let mut cap = pci::pci_config_read8(bus, dev, func, 0x34);
    let mut common = 0u64;
    let mut notify = 0u64;
    let mut notify_mult = 0u32;
    let mut isr = 0u64;
    let mut hops = 0u8;
    while cap != 0 && cap != 0xFF && hops < 48 {
        hops += 1;
        let cap_id = pci::pci_config_read8(bus, dev, func, cap);
        if cap_id == PCI_CAP_ID_VNDR {
            let cfg_type = pci::pci_config_read8(bus, dev, func, cap.wrapping_add(3));
            let bar = pci::pci_config_read8(bus, dev, func, cap.wrapping_add(4));
            let offset = pci::pci_config_read32(bus, dev, func, cap.wrapping_add(8));
            let length = pci::pci_config_read32(bus, dev, func, cap.wrapping_add(12));
            if let Some(virt) = map_cap_region(bus, dev, func, bar, offset, length) {
                match cfg_type {
                    VIRTIO_PCI_CAP_COMMON => common = virt,
                    VIRTIO_PCI_CAP_NOTIFY => {
                        notify = virt;
                        notify_mult = pci::pci_config_read32(bus, dev, func, cap.wrapping_add(16));
                    }
                    VIRTIO_PCI_CAP_ISR => isr = virt,
                    _ => {}
                }
            }
        }
        cap = pci::pci_config_read8(bus, dev, func, cap.wrapping_add(1));
    }
    if common == 0 || notify == 0 {
        None
    } else {
        Some(VirtioCaps {
            common,
            notify,
            notify_mult,
            isr,
        })
    }
}

pub(super) fn find_virtio_gpu() -> Option<(u8, u8, u8, u32)> {
    for bus in 0u8..=0 {
        for device in 0u8..32 {
            for function in 0u8..8 {
                let vendor = pci::pci_config_read16(bus, device, function, 0x00);
                if vendor == 0xFFFF {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                if vendor != VIRTIO_PCI_VENDOR {
                    if function == 0 {
                        let ht = pci::pci_config_read8(bus, device, 0, 0x0E);
                        if ht & 0x80 == 0 {
                            break;
                        }
                    }
                    continue;
                }
                let device_id = pci::pci_config_read16(bus, device, function, 0x02);
                let subsystem = pci::pci_config_read16(bus, device, function, 0x2E);
                let is_gpu = device_id == VIRTIO_GPU_DEVICE_MODERN
                    || device_id == VIRTIO_GPU_DEVICE_LEGACY
                    || ((0x1000..=0x103F).contains(&device_id) && subsystem == VIRTIO_ID_GPU);
                if is_gpu {
                    let bar0 = pci::pci_config_read32(bus, device, function, 0x10);
                    let class = pci::pci_config_read8(bus, device, function, 0x0B);
                    let subclass = pci::pci_config_read8(bus, device, function, 0x0A);
                    // VGA-class virtio-vga (`-vga virtio`) is the window the
                    // host shows. Display-other virtio-gpu-pci is a second head.
                    let primary = class == 0x03 && subclass == 0x00;
                    PRIMARY_DISPLAY.store(primary, Ordering::SeqCst);
                    serial_println!(
                        "[VIRTIO-GPU] PCI {:02x}:{:02x}.{} id={:#06x} class={:02x}:{:02x} {} bar0={:#x}",
                        bus,
                        device,
                        function,
                        device_id,
                        class,
                        subclass,
                        if primary { "primary VGA" } else { "secondary" },
                        bar0
                    );
                    return Some((bus, device, function, bar0));
                }
                if function == 0 {
                    let ht = pci::pci_config_read8(bus, device, 0, 0x0E);
                    if ht & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    None
}

pub(super) fn enable_pci_bus_master(bus: u8, dev: u8, func: u8) {
    let cmd = pci::pci_config_read16(bus, dev, func, 0x04);
    pci::pci_config_write16(bus, dev, func, 0x04, cmd | 0x07);
}
