/// Virtqueues and live VirtIO GPU transport (modern PCI and legacy I/O).
use core::sync::atomic::Ordering;
use spin::Mutex;

use crate::serial_println;

use super::pci::{
    CFG_DEVICE_FEATURE, CFG_DEVICE_FEATURE_SELECT, CFG_DEVICE_STATUS, CFG_GUEST_FEATURE,
    CFG_GUEST_FEATURE_SELECT, CFG_QUEUE_DESC, CFG_QUEUE_DEVICE, CFG_QUEUE_DRIVER, CFG_QUEUE_ENABLE,
    CFG_QUEUE_MSIX, CFG_QUEUE_NOTIFY_OFF, CFG_QUEUE_SELECT, CFG_QUEUE_SIZE, mmio_r8, mmio_r16,
    mmio_r32, mmio_w8, mmio_w16, mmio_w32, mmio_w64, parse_virtio_caps,
};
use super::protocol::{
    BOUNCE_CMD, BOUNCE_RESP, BOUNCE_RESP_LEN, CURSOR_BYTES, QUEUE_CAP, SCANOUT_BYTES,
};

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_FAILED: u8 = 128;

const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

const VIRTIO_F_VERSION_1: u32 = 1;

const LEGACY_DEVICE_FEATURES: u16 = 0;
const LEGACY_GUEST_FEATURES: u16 = 4;
const LEGACY_QUEUE_ADDRESS: u16 = 8;
const LEGACY_QUEUE_SIZE: u16 = 12;
const LEGACY_QUEUE_SELECT: u16 = 14;
const LEGACY_QUEUE_NOTIFY: u16 = 16;
const LEGACY_DEVICE_STATUS: u16 = 18;
const LEGACY_ISR_STATUS: u16 = 19;

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

pub(super) struct Vq {
    pub(super) index: u16,
    pub(super) size: u16,
    vq_phys: u64,
    avail_off: usize,
    used_off: usize,
    avail_idx: u16,
    used_idx: u16,
    notify_off: u16,
}

pub(super) struct LiveGpu {
    common: u64,
    notify: u64,
    notify_mult: u32,
    isr: u64,
    io_base: u16,
    pub(super) modern: bool,
    pub(super) ctrl: Vq,
    pub(super) cursor: Vq,
    bounce_phys: u64,
    pub(super) scanout_phys: u64,
    pub(super) cursor_phys: u64,
    fence: u64,
    pub(super) ready: bool,
}

pub(super) static LIVE_GPU: Mutex<Option<LiveGpu>> = Mutex::new(None);

impl Vq {
    fn empty(index: u16) -> Self {
        Self {
            index,
            size: 0,
            vq_phys: 0,
            avail_off: 0,
            used_off: 0,
            avail_idx: 0,
            used_idx: 0,
            notify_off: 0,
        }
    }

    fn vq_virt(&self) -> u64 {
        crate::vmm::phys_to_virt(self.vq_phys)
    }

    fn desc_ptr(&self, i: u16) -> *mut VirtqDesc {
        (self.vq_virt() as *mut VirtqDesc).wrapping_add(i as usize)
    }

    fn avail_flags_ptr(&self) -> *mut u16 {
        (self.vq_virt() + self.avail_off as u64) as *mut u16
    }

    fn avail_idx_ptr(&self) -> *mut u16 {
        // SAFETY: avail ring is a mapped virtqueue page.
        unsafe { self.avail_flags_ptr().add(1) }
    }

    fn avail_ring_entry(&self, i: u16) -> *mut u16 {
        // SAFETY: slot is within the allocated avail ring.
        unsafe { self.avail_flags_ptr().add(2 + i as usize) }
    }

    fn used_idx_ptr(&self) -> *const u16 {
        // SAFETY: used ring is a mapped virtqueue page.
        unsafe { ((self.vq_virt() + self.used_off as u64) as *const u16).add(1) }
    }

    fn used_elem(&self, i: u16) -> *const VirtqUsedElem {
        // SAFETY: used ring elements follow the used header.
        unsafe {
            let base = ((self.vq_virt() + self.used_off as u64) as *const u16).add(2)
                as *const VirtqUsedElem;
            base.add(i as usize)
        }
    }
}

fn alloc_vq(index: u16, size: u16) -> Option<Vq> {
    if size == 0 || !size.is_power_of_two() {
        return None;
    }
    let qsz = size as usize;
    let avail_off = 16 * qsz;
    let avail_bytes = 6 + 2 * qsz;
    let used_off = (avail_off + avail_bytes + 4095) & !4095;
    let used_bytes = 6 + 8 * qsz;
    let vq_pages = (used_off + used_bytes).div_ceil(4096);
    let vq_phys = crate::vmm::allocate_contiguous_frames(vq_pages)?;
    let vq_virt = crate::vmm::phys_to_virt(vq_phys);
    // SAFETY: freshly allocated DMA pages owned by this driver.
    unsafe {
        core::ptr::write_bytes(vq_virt as *mut u8, 0, vq_pages * 4096);
    }
    Some(Vq {
        index,
        size,
        vq_phys,
        avail_off,
        used_off,
        avail_idx: 0,
        used_idx: 0,
        notify_off: 0,
    })
}

impl LiveGpu {
    pub(super) fn bounce_virt(&self) -> u64 {
        crate::vmm::phys_to_virt(self.bounce_phys)
    }

    fn set_status(&self, status: u8) {
        if self.modern {
            mmio_w8(self.common + CFG_DEVICE_STATUS, status);
        } else {
            // SAFETY: legacy VirtIO status port from BAR0.
            unsafe {
                let mut p: crate::arch_compat::instructions::port::Port<u8> =
                    crate::arch_compat::instructions::port::Port::new(
                        self.io_base + LEGACY_DEVICE_STATUS,
                    );
                p.write(status);
            }
        }
    }

    fn notify(&self, vq: &Vq) {
        if self.modern {
            let off = vq.notify_off as u64 * self.notify_mult as u64;
            mmio_w16(self.notify + off, vq.index);
        } else {
            // SAFETY: legacy VirtIO notify port from BAR0.
            unsafe {
                let mut p: crate::arch_compat::instructions::port::Port<u16> =
                    crate::arch_compat::instructions::port::Port::new(
                        self.io_base + LEGACY_QUEUE_NOTIFY,
                    );
                p.write(vq.index);
            }
        }
    }

    fn ack_isr(&self) {
        if self.modern {
            if self.isr != 0 {
                let _ = mmio_r8(self.isr);
            }
        } else {
            // SAFETY: legacy ISR read acknowledges the interrupt.
            unsafe {
                let mut p: crate::arch_compat::instructions::port::Port<u8> =
                    crate::arch_compat::instructions::port::Port::new(
                        self.io_base + LEGACY_ISR_STATUS,
                    );
                let _ = p.read();
            }
        }
    }

    fn kick_and_wait(&mut self, ctrl: bool) -> bool {
        let vq = if ctrl {
            &mut self.ctrl
        } else {
            &mut self.cursor
        };
        let avail_slot = vq.avail_idx % vq.size;
        // SAFETY: descriptor 0 is the chain head we just filled.
        unsafe {
            core::ptr::write_volatile(vq.avail_ring_entry(avail_slot), 0u16);
        }
        core::sync::atomic::fence(Ordering::Release);
        vq.avail_idx = vq.avail_idx.wrapping_add(1);
        // SAFETY: avail idx is the virtqueue producer index.
        unsafe {
            core::ptr::write_volatile(vq.avail_idx_ptr(), vq.avail_idx);
        }
        let index = vq.index;
        let notify_off = vq.notify_off;
        if self.modern {
            let off = notify_off as u64 * self.notify_mult as u64;
            mmio_w16(self.notify + off, index);
        } else {
            // SAFETY: legacy VirtIO notify port from BAR0.
            unsafe {
                let mut p: crate::arch_compat::instructions::port::Port<u16> =
                    crate::arch_compat::instructions::port::Port::new(
                        self.io_base + LEGACY_QUEUE_NOTIFY,
                    );
                p.write(index);
            }
        }

        for iter in 0..500_000u32 {
            if iter % 64 == 0 {
                self.ack_isr();
            }
            let vq = if ctrl {
                &mut self.ctrl
            } else {
                &mut self.cursor
            };
            // SAFETY: used idx is written by the device into guest memory.
            let device_used = unsafe { core::ptr::read_volatile(vq.used_idx_ptr()) };
            if device_used != vq.used_idx {
                while vq.used_idx != device_used {
                    let slot = vq.used_idx % vq.size;
                    // SAFETY: the address is device MMIO or a DMA/on-disk record we own; volatile access prevents the compiler from eliding it.
                    let _ = unsafe { core::ptr::read_volatile(vq.used_elem(slot)) };
                    vq.used_idx = vq.used_idx.wrapping_add(1);
                }
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    pub(super) fn submit_ctrl(&mut self, cmd: &[u8]) -> Option<u32> {
        if cmd.is_empty() || cmd.len() > BOUNCE_RESP {
            return None;
        }
        let bounce = self.bounce_virt();
        // SAFETY: bounce is a dedicated DMA page owned by this device.
        unsafe {
            core::ptr::copy_nonoverlapping(cmd.as_ptr(), bounce as *mut u8, cmd.len());
            core::ptr::write_bytes(
                (bounce as *mut u8).add(BOUNCE_RESP),
                0,
                BOUNCE_RESP_LEN as usize,
            );
            core::ptr::write_volatile(
                self.ctrl.desc_ptr(0),
                VirtqDesc {
                    addr: self.bounce_phys + BOUNCE_CMD as u64,
                    len: cmd.len() as u32,
                    flags: VRING_DESC_F_NEXT,
                    next: 1,
                },
            );
            core::ptr::write_volatile(
                self.ctrl.desc_ptr(1),
                VirtqDesc {
                    addr: self.bounce_phys + BOUNCE_RESP as u64,
                    len: BOUNCE_RESP_LEN,
                    flags: VRING_DESC_F_WRITE,
                    next: 0,
                },
            );
        }
        core::sync::atomic::fence(Ordering::SeqCst);
        if !self.kick_and_wait(true) {
            serial_println!("[VIRTIO-GPU] control command timed out");
            return None;
        }
        let mut typ_bytes = [0u8; 4];
        // SAFETY: device wrote the response header into the bounce page.
        unsafe {
            core::ptr::copy_nonoverlapping(
                (self.bounce_virt() as *const u8).add(BOUNCE_RESP),
                typ_bytes.as_mut_ptr(),
                4,
            );
        }
        Some(u32::from_le_bytes(typ_bytes))
    }

    pub(super) fn submit_cursor_cmd(&mut self, cmd: &[u8]) -> bool {
        if cmd.is_empty() || cmd.len() > 256 {
            return false;
        }
        let bounce = self.bounce_virt();
        // SAFETY: bounce is a dedicated DMA page owned by this device.
        unsafe {
            core::ptr::copy_nonoverlapping(cmd.as_ptr(), bounce as *mut u8, cmd.len());
            core::ptr::write_volatile(
                self.cursor.desc_ptr(0),
                VirtqDesc {
                    addr: self.bounce_phys,
                    len: cmd.len() as u32,
                    flags: 0,
                    next: 0,
                },
            );
        }
        core::sync::atomic::fence(Ordering::SeqCst);
        self.kick_and_wait(false)
    }

    pub(super) fn next_fence(&mut self) -> u64 {
        self.fence = self.fence.wrapping_add(1);
        if self.fence == 0 {
            self.fence = 1;
        }
        self.fence
    }
}

fn setup_modern_queue(common: u64, index: u16) -> Option<Vq> {
    mmio_w16(common + CFG_QUEUE_SELECT, index);
    let qs = mmio_r16(common + CFG_QUEUE_SIZE);
    if qs == 0 {
        serial_println!("[VIRTIO-GPU] queue {} size 0", index);
        return None;
    }
    let size = QUEUE_CAP.min(qs);
    let size = if size.is_power_of_two() {
        size
    } else {
        size.next_power_of_two() >> 1
    };
    if size == 0 {
        return None;
    }
    mmio_w16(common + CFG_QUEUE_SIZE, size);
    let mut vq = alloc_vq(index, size)?;
    mmio_w16(common + CFG_QUEUE_MSIX, 0xFFFF);
    mmio_w64(common + CFG_QUEUE_DESC, vq.vq_phys);
    mmio_w64(common + CFG_QUEUE_DRIVER, vq.vq_phys + vq.avail_off as u64);
    mmio_w64(common + CFG_QUEUE_DEVICE, vq.vq_phys + vq.used_off as u64);
    vq.notify_off = mmio_r16(common + CFG_QUEUE_NOTIFY_OFF);
    mmio_w16(common + CFG_QUEUE_ENABLE, 1);
    serial_println!(
        "[VIRTIO-GPU] queue {} size={} notify_off={}",
        index,
        size,
        vq.notify_off
    );
    Some(vq)
}

pub(super) fn init_modern(bus: u8, dev: u8, func: u8) -> Option<LiveGpu> {
    let caps = parse_virtio_caps(bus, dev, func)?;
    mmio_w8(caps.common + CFG_DEVICE_STATUS, 0);
    mmio_w8(caps.common + CFG_DEVICE_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);
    mmio_w8(
        caps.common + CFG_DEVICE_STATUS,
        VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
    );

    mmio_w32(caps.common + CFG_DEVICE_FEATURE_SELECT, 0);
    let feat0 = mmio_r32(caps.common + CFG_DEVICE_FEATURE);
    mmio_w32(caps.common + CFG_DEVICE_FEATURE_SELECT, 1);
    let feat1 = mmio_r32(caps.common + CFG_DEVICE_FEATURE);
    serial_println!("[VIRTIO-GPU] features lo={:#x} hi={:#x}", feat0, feat1);

    mmio_w32(caps.common + CFG_GUEST_FEATURE_SELECT, 0);
    mmio_w32(caps.common + CFG_GUEST_FEATURE, 0);
    mmio_w32(caps.common + CFG_GUEST_FEATURE_SELECT, 1);
    mmio_w32(
        caps.common + CFG_GUEST_FEATURE,
        if feat1 & VIRTIO_F_VERSION_1 != 0 {
            VIRTIO_F_VERSION_1
        } else {
            0
        },
    );
    mmio_w8(
        caps.common + CFG_DEVICE_STATUS,
        VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
    );
    let st = mmio_r8(caps.common + CFG_DEVICE_STATUS);
    if st & VIRTIO_STATUS_FEATURES_OK == 0 {
        serial_println!("[VIRTIO-GPU] FEATURES_OK rejected ({:#x})", st);
        mmio_w8(caps.common + CFG_DEVICE_STATUS, VIRTIO_STATUS_FAILED);
        return None;
    }

    let ctrl = setup_modern_queue(caps.common, 0)?;
    let cursor = setup_modern_queue(caps.common, 1)?;
    let bounce_phys = crate::vmm::allocate_contiguous_frames(1)?;
    let scan_pages = SCANOUT_BYTES.div_ceil(4096).max(1);
    let cur_pages = CURSOR_BYTES.div_ceil(4096).max(1);
    let scanout_phys = crate::vmm::allocate_contiguous_frames(scan_pages)?;
    let cursor_phys = crate::vmm::allocate_contiguous_frames(cur_pages)?;
    // SAFETY: DMA pages are exclusive to this driver.
    unsafe {
        core::ptr::write_bytes(crate::vmm::phys_to_virt(bounce_phys) as *mut u8, 0, 4096);
        core::ptr::write_bytes(
            crate::vmm::phys_to_virt(scanout_phys) as *mut u8,
            0,
            scan_pages * 4096,
        );
        core::ptr::write_bytes(
            crate::vmm::phys_to_virt(cursor_phys) as *mut u8,
            0,
            cur_pages * 4096,
        );
    }

    mmio_w8(
        caps.common + CFG_DEVICE_STATUS,
        VIRTIO_STATUS_ACKNOWLEDGE
            | VIRTIO_STATUS_DRIVER
            | VIRTIO_STATUS_FEATURES_OK
            | VIRTIO_STATUS_DRIVER_OK,
    );

    Some(LiveGpu {
        common: caps.common,
        notify: caps.notify,
        notify_mult: caps.notify_mult,
        isr: caps.isr,
        io_base: 0,
        modern: true,
        ctrl,
        cursor,
        bounce_phys,
        scanout_phys,
        cursor_phys,
        fence: 0,
        ready: true,
    })
}

pub(super) fn init_legacy(io_base: u16) -> Option<LiveGpu> {
    // SAFETY: `io_base` is BAR0 of a legacy VirtIO GPU.
    unsafe {
        let mut status: crate::arch_compat::instructions::port::Port<u8> =
            crate::arch_compat::instructions::port::Port::new(io_base + LEGACY_DEVICE_STATUS);
        status.write(0);
        status.write(VIRTIO_STATUS_ACKNOWLEDGE);
        status.write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);
        let mut feat: crate::arch_compat::instructions::port::Port<u32> =
            crate::arch_compat::instructions::port::Port::new(io_base + LEGACY_DEVICE_FEATURES);
        let device_features = feat.read();
        let mut guest: crate::arch_compat::instructions::port::Port<u32> =
            crate::arch_compat::instructions::port::Port::new(io_base + LEGACY_GUEST_FEATURES);
        guest.write(0);
        serial_println!("[VIRTIO-GPU] legacy features={:#x}", device_features);

        let mut qsel: crate::arch_compat::instructions::port::Port<u16> =
            crate::arch_compat::instructions::port::Port::new(io_base + LEGACY_QUEUE_SELECT);
        let mut qsz: crate::arch_compat::instructions::port::Port<u16> =
            crate::arch_compat::instructions::port::Port::new(io_base + LEGACY_QUEUE_SIZE);
        let mut qaddr: crate::arch_compat::instructions::port::Port<u32> =
            crate::arch_compat::instructions::port::Port::new(io_base + LEGACY_QUEUE_ADDRESS);

        qsel.write(0);
        let ctrl_qs = qsz.read();
        if ctrl_qs == 0 {
            status.write(VIRTIO_STATUS_FAILED);
            return None;
        }
        let mut ctrl = alloc_vq(0, ctrl_qs)?;
        qaddr.write((ctrl.vq_phys / 4096) as u32);

        qsel.write(1);
        let cur_qs = qsz.read();
        if cur_qs == 0 {
            status.write(VIRTIO_STATUS_FAILED);
            return None;
        }
        let mut cursor = alloc_vq(1, cur_qs)?;
        qaddr.write((cursor.vq_phys / 4096) as u32);
        ctrl.notify_off = 0;
        cursor.notify_off = 0;

        let bounce_phys = crate::vmm::allocate_contiguous_frames(1)?;
        let scan_pages = SCANOUT_BYTES.div_ceil(4096).max(1);
        let cur_pages = CURSOR_BYTES.div_ceil(4096).max(1);
        let scanout_phys = crate::vmm::allocate_contiguous_frames(scan_pages)?;
        let cursor_phys = crate::vmm::allocate_contiguous_frames(cur_pages)?;
        core::ptr::write_bytes(crate::vmm::phys_to_virt(bounce_phys) as *mut u8, 0, 4096);
        core::ptr::write_bytes(
            crate::vmm::phys_to_virt(scanout_phys) as *mut u8,
            0,
            scan_pages * 4096,
        );
        core::ptr::write_bytes(
            crate::vmm::phys_to_virt(cursor_phys) as *mut u8,
            0,
            cur_pages * 4096,
        );

        status.write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);
        Some(LiveGpu {
            common: 0,
            notify: 0,
            notify_mult: 0,
            isr: 0,
            io_base,
            modern: false,
            ctrl,
            cursor,
            bounce_phys,
            scanout_phys,
            cursor_phys,
            fence: 0,
            ready: true,
        })
    }
}
