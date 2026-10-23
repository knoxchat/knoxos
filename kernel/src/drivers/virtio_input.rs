#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// VirtIO Input Device Driver
///
/// Binds `virtio-tablet-pci` / `virtio-keyboard-pci` / `virtio-mouse-pci`.
/// Events arrive on eventq (queue 0) as `{type, code, value}` matching Linux
/// `input_event`. Absolute SYN frames call `set_absolute_mouse`; relative
/// frames are encoded as PS/2 packets; non-button `EV_KEY` becomes scancodes.
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering, fence};
use spin::Mutex;

use crate::pci;
use crate::serial_println;
use crate::virtio_net::{
    VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER, VIRTIO_STATUS_DRIVER_OK, scan_pci_bus,
};

// ── Linux input event types ─────────────────────────────────────────
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_WHEEL_CODE: u16 = 0x08;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_TOOL_PEN: u16 = 0x140;
const BTN_TOOL_FINGER: u16 = 0x145;
const BTN_TOUCH: u16 = 0x14a;

// ── VirtIO PCI ──────────────────────────────────────────────────────
const VIRTIO_PCI_QUEUE_ADDRESS: u16 = 8;
const VIRTIO_PCI_QUEUE_SIZE: u16 = 12;
const VIRTIO_PCI_QUEUE_SELECT: u16 = 14;
const VIRTIO_PCI_QUEUE_NOTIFY: u16 = 16;
const VIRTIO_PCI_DEVICE_STATUS: u16 = 18;

const VIRTIO_PCI_VENDOR: u16 = 0x1AF4;
const VIRTIO_INPUT_DEVICE_MODERN: u16 = 0x1052; // 0x1040 + 18
const VIRTIO_INPUT_DEVICE_LEGACY: u16 = 0x1012;
const VIRTIO_ID_INPUT: u16 = 18;
const VRING_DESC_F_WRITE: u16 = 2;
const EVENT_QUEUE_SIZE: u16 = 64;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_FAILED: u8 = 128;
const VIRTIO_F_VERSION_1: u32 = 1;

const VIRTIO_PCI_CAP_COMMON: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY: u8 = 2;
const VIRTIO_PCI_CAP_ISR: u8 = 3;
const PCI_CAP_ID_VNDR: u8 = 0x09;

const CFG_DEVICE_FEATURE_SELECT: u64 = 0;
const CFG_DEVICE_FEATURE: u64 = 4;
const CFG_GUEST_FEATURE_SELECT: u64 = 8;
const CFG_GUEST_FEATURE: u64 = 12;
const CFG_DEVICE_STATUS: u64 = 20;
const CFG_QUEUE_SELECT: u64 = 22;
const CFG_QUEUE_SIZE: u64 = 24;
const CFG_QUEUE_MSIX: u64 = 26;
const CFG_QUEUE_ENABLE: u64 = 28;
const CFG_QUEUE_NOTIFY_OFF: u64 = 30;
const CFG_QUEUE_DESC: u64 = 32;
const CFG_QUEUE_DRIVER: u64 = 40;
const CFG_QUEUE_DEVICE: u64 = 48;

pub const GATE_VIRTIO_INPUT_MARKER: &str = "GATE_VIRTIO input tablet";

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
struct VirtioInputEvent {
    event_type: u16,
    code: u16,
    value: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

struct VirtioInputDev {
    io_base: u16,
    modern: bool,
    common: u64,
    notify: u64,
    notify_mult: u32,
    notify_off: u16,
    isr: u64,
    queue_size: u16,
    vq_phys: u64,
    desc_base: *mut VirtqDesc,
    avail_base: *mut u8,
    used_base: *mut u8,
    event_phys: u64,
    avail_idx: u16,
    used_idx: u16,
    ready: bool,
}

// Safety: VirtioInputDev is only accessed behind INPUT_DEVICES Mutex
unsafe impl Send for VirtioInputDev {}

fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

fn mmio_r8(addr: u64) -> u8 {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::read_volatile(addr as *const u8) }
}
fn mmio_r16(addr: u64) -> u16 {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::read_volatile(addr as *const u16) }
}
fn mmio_r32(addr: u64) -> u32 {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}
fn mmio_w8(addr: u64, v: u8) {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::write_volatile(addr as *mut u8, v) }
}
fn mmio_w16(addr: u64, v: u16) {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::write_volatile(addr as *mut u16, v) }
}
fn mmio_w32(addr: u64, v: u32) {
    // SAFETY: `addr` is a mapped VirtIO MMIO register.
    unsafe { core::ptr::write_volatile(addr as *mut u32, v) }
}
fn mmio_w64(addr: u64, v: u64) {
    mmio_w32(addr, v as u32);
    mmio_w32(addr + 4, (v >> 32) as u32);
}

fn pci_bar_phys(bus: u8, dev: u8, func: u8, bar: u8) -> Option<u64> {
    let off = 0x10 + bar * 4;
    let lo = pci::pci_config_read32(bus, dev, func, off);
    if lo == 0 || lo & 1 != 0 {
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

struct VirtioCaps {
    common: u64,
    notify: u64,
    notify_mult: u32,
    isr: u64,
}

fn parse_virtio_caps(bus: u8, dev: u8, func: u8) -> Option<VirtioCaps> {
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

fn is_virtio_input(device_id: u16, subsystem: u16) -> bool {
    device_id == VIRTIO_INPUT_DEVICE_MODERN
        || device_id == VIRTIO_INPUT_DEVICE_LEGACY
        || ((0x1000..=0x103F).contains(&device_id) && subsystem == VIRTIO_ID_INPUT)
        || subsystem == VIRTIO_ID_INPUT
}

fn enable_pci_bus_master(bus: u8, dev: u8, func: u8) {
    let cmd = pci::pci_config_read16(bus, dev, func, 0x04);
    pci::pci_config_write16(bus, dev, func, 0x04, cmd | 0x07);
}

fn power_of_two_queue(qs: u16) -> u16 {
    let size = EVENT_QUEUE_SIZE.min(qs);
    if size == 0 {
        return 0;
    }
    if size.is_power_of_two() {
        size
    } else {
        let next = size.next_power_of_two();
        if next > size { next >> 1 } else { next }
    }
}

impl VirtioInputDev {
    fn empty() -> Self {
        Self {
            io_base: 0,
            modern: false,
            common: 0,
            notify: 0,
            notify_mult: 1,
            notify_off: 0,
            isr: 0,
            queue_size: 0,
            vq_phys: 0,
            desc_base: core::ptr::null_mut(),
            avail_base: core::ptr::null_mut(),
            used_base: core::ptr::null_mut(),
            event_phys: 0,
            avail_idx: 0,
            used_idx: 0,
            ready: false,
        }
    }

    fn avail_idx_ptr(&self) -> *mut u16 {
        // SAFETY: avail ring is a mapped virtqueue page we own.
        unsafe { (self.avail_base as *mut u16).add(1) }
    }
    fn avail_ring_entry(&self, i: u16) -> *mut u16 {
        // SAFETY: slot is within the allocated avail ring.
        unsafe { (self.avail_base as *mut u16).add(2 + i as usize) }
    }
    fn used_idx_ptr(&self) -> *const u16 {
        // SAFETY: used ring is a mapped virtqueue page we own.
        unsafe { (self.used_base as *const u16).add(1) }
    }
    fn used_elem(&self, i: u16) -> VirtqUsedElem {
        // SAFETY: used-ring elements follow the used header.
        unsafe {
            let base = (self.used_base as *const u16).add(2) as *const VirtqUsedElem;
            core::ptr::read_volatile(base.add(i as usize))
        }
    }

    fn ack_isr(&self) {
        if self.modern {
            if self.isr != 0 {
                let _ = mmio_r8(self.isr);
            }
        } else if self.io_base != 0 {
            // SAFETY: legacy ISR read acknowledges the interrupt.
            unsafe {
                let mut isr: Port<u8> = Port::new(self.io_base + 19);
                let _ = isr.read();
            }
        }
    }

    fn read_event(&self, desc_id: usize) -> Option<VirtioInputEvent> {
        if self.event_phys == 0 || desc_id >= self.queue_size as usize {
            return None;
        }
        let virt = crate::vmm::phys_to_virt(self.event_phys);
        // SAFETY: event_phys is a DMA page we allocated; desc_id is in range.
        Some(unsafe { core::ptr::read_volatile((virt as *const VirtioInputEvent).add(desc_id)) })
    }

    fn kick(&self) {
        if self.modern {
            let off = self.notify_off as u64 * self.notify_mult as u64;
            mmio_w16(self.notify + off, 0);
        } else if self.io_base != 0 {
            // SAFETY: legacy VirtIO notify port from BAR0.
            unsafe {
                let mut notify: Port<u16> = Port::new(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY);
                notify.write(0);
            }
        }
    }

    fn setup_rings(&mut self, qsz: u16) -> bool {
        if qsz == 0 || !qsz.is_power_of_two() || !crate::vmm::ready() {
            return false;
        }
        let qsz_us = qsz as usize;
        let desc_size = qsz_us * 16;
        let avail_size = 6 + 2 * qsz_us;
        let used_offset = align_up(desc_size + avail_size, 4096);
        let used_size = 6 + 8 * qsz_us;
        let total = used_offset + used_size;
        let pages = total.div_ceil(4096);
        let Some(vq_phys) = crate::vmm::allocate_contiguous_frames(pages) else {
            serial_println!("[VIRTIO-INPUT] virtqueue alloc failed");
            return false;
        };
        let vq_virt = crate::vmm::phys_to_virt(vq_phys);
        // SAFETY: freshly allocated DMA pages owned by this driver.
        unsafe {
            core::ptr::write_bytes(vq_virt as *mut u8, 0, pages * 4096);
        }

        let evt_bytes = qsz_us * core::mem::size_of::<VirtioInputEvent>();
        let evt_pages = evt_bytes.div_ceil(4096).max(1);
        let Some(event_phys) = crate::vmm::allocate_contiguous_frames(evt_pages) else {
            serial_println!("[VIRTIO-INPUT] event buffer alloc failed");
            return false;
        };
        // SAFETY: event DMA pages are exclusive to this device.
        unsafe {
            core::ptr::write_bytes(
                crate::vmm::phys_to_virt(event_phys) as *mut u8,
                0,
                evt_pages * 4096,
            );
        }

        self.vq_phys = vq_phys;
        self.desc_base = vq_virt as *mut VirtqDesc;
        self.avail_base = (vq_virt as usize + desc_size) as *mut u8;
        self.used_base = (vq_virt as usize + used_offset) as *mut u8;
        self.event_phys = event_phys;
        self.queue_size = qsz;

        for i in 0..qsz_us {
            // SAFETY: descriptor i is inside the allocated table.
            unsafe {
                let desc = &mut *self.desc_base.add(i);
                desc.addr = event_phys + (i * core::mem::size_of::<VirtioInputEvent>()) as u64;
                desc.len = core::mem::size_of::<VirtioInputEvent>() as u32;
                desc.flags = VRING_DESC_F_WRITE;
                desc.next = 0;
                core::ptr::write_volatile(self.avail_ring_entry(i as u16), i as u16);
            }
        }
        self.avail_idx = qsz;
        // SAFETY: avail flags/idx sit at the start of the avail ring.
        unsafe {
            core::ptr::write_volatile(self.avail_base as *mut u16, 0u16);
            fence(Ordering::Release);
            core::ptr::write_volatile(self.avail_idx_ptr(), self.avail_idx);
        }
        true
    }

    fn init_legacy(&mut self, io_base: u16) -> bool {
        self.io_base = io_base;
        self.modern = false;
        // SAFETY: I/O port access is exclusive to this driver path; BAR0 is the legacy VirtIO map.
        unsafe {
            let mut status_port: Port<u8> = Port::new(io_base + VIRTIO_PCI_DEVICE_STATUS);
            status_port.write(0);
            status_port.write(VIRTIO_STATUS_ACKNOWLEDGE);
            status_port.write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

            let mut feat_port: Port<u32> = Port::new(io_base);
            let _dev_features = feat_port.read();
            let mut guest_feat: Port<u32> = Port::new(io_base + 4);
            guest_feat.write(0);

            let mut queue_sel: Port<u16> = Port::new(io_base + VIRTIO_PCI_QUEUE_SELECT);
            queue_sel.write(0);
            let mut qs_port: Port<u16> = Port::new(io_base + VIRTIO_PCI_QUEUE_SIZE);
            let max_qs = qs_port.read();
            let qsz = power_of_two_queue(max_qs);
            if qsz == 0 {
                serial_println!("[VIRTIO-INPUT] No event queue available");
                return false;
            }
            if !self.setup_rings(qsz) {
                return false;
            }
            let pfn = self.vq_phys / 4096;
            let mut qa_port: Port<u32> = Port::new(io_base + VIRTIO_PCI_QUEUE_ADDRESS);
            qa_port.write(pfn as u32);
            status_port
                .write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);
        }
        self.used_idx = 0;
        self.ready = true;
        true
    }

    fn init_modern(&mut self, bus: u8, dev: u8, func: u8) -> bool {
        let Some(caps) = parse_virtio_caps(bus, dev, func) else {
            serial_println!("[VIRTIO-INPUT] modern caps missing");
            return false;
        };
        self.modern = true;
        self.common = caps.common;
        self.notify = caps.notify;
        self.notify_mult = caps.notify_mult;
        self.isr = caps.isr;

        mmio_w8(caps.common + CFG_DEVICE_STATUS, 0);
        mmio_w8(caps.common + CFG_DEVICE_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);
        mmio_w8(
            caps.common + CFG_DEVICE_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
        );

        mmio_w32(caps.common + CFG_DEVICE_FEATURE_SELECT, 0);
        let _feat0 = mmio_r32(caps.common + CFG_DEVICE_FEATURE);
        mmio_w32(caps.common + CFG_DEVICE_FEATURE_SELECT, 1);
        let feat1 = mmio_r32(caps.common + CFG_DEVICE_FEATURE);

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
            serial_println!("[VIRTIO-INPUT] FEATURES_OK rejected ({:#x})", st);
            mmio_w8(caps.common + CFG_DEVICE_STATUS, VIRTIO_STATUS_FAILED);
            return false;
        }

        mmio_w16(caps.common + CFG_QUEUE_SELECT, 0);
        let qs = mmio_r16(caps.common + CFG_QUEUE_SIZE);
        let qsz = power_of_two_queue(qs);
        if qsz == 0 {
            serial_println!("[VIRTIO-INPUT] modern eventq size 0");
            return false;
        }
        mmio_w16(caps.common + CFG_QUEUE_SIZE, qsz);
        if !self.setup_rings(qsz) {
            return false;
        }
        let avail_off = (self.queue_size as usize) * 16;
        let used_off = {
            let avail_size = 6 + 2 * self.queue_size as usize;
            align_up(avail_off + avail_size, 4096)
        };
        mmio_w16(caps.common + CFG_QUEUE_MSIX, 0xFFFF);
        mmio_w64(caps.common + CFG_QUEUE_DESC, self.vq_phys);
        mmio_w64(
            caps.common + CFG_QUEUE_DRIVER,
            self.vq_phys + avail_off as u64,
        );
        mmio_w64(
            caps.common + CFG_QUEUE_DEVICE,
            self.vq_phys + used_off as u64,
        );
        self.notify_off = mmio_r16(caps.common + CFG_QUEUE_NOTIFY_OFF);
        mmio_w16(caps.common + CFG_QUEUE_ENABLE, 1);
        enable_status_queue(caps.common);
        mmio_w8(
            caps.common + CFG_DEVICE_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );
        self.used_idx = 0;
        self.ready = true;
        true
    }

    fn drain_events(&mut self, out: &mut Vec<VirtioInputEvent>) {
        if !self.ready {
            return;
        }
        self.ack_isr();
        let qsz = self.queue_size;
        if qsz == 0 {
            return;
        }
        let mut consumed = false;
        let mut n = 0u16;
        loop {
            if n >= qsz.saturating_mul(2) {
                break;
            }
            n = n.saturating_add(1);
            // SAFETY: used idx is the device-owned producer index.
            let device_used_idx = unsafe { core::ptr::read_volatile(self.used_idx_ptr()) };
            fence(Ordering::Acquire);
            if device_used_idx == self.used_idx {
                break;
            }
            let ring_idx = self.used_idx % qsz;
            let desc_id = self.used_elem(ring_idx).id;
            if let Some(evt) = self.read_event(desc_id as usize) {
                out.push(evt);
            }
            let avail_slot = self.avail_idx % qsz;
            // SAFETY: re-arm the descriptor in the avail ring even if the
            // event was unreadable so the queue cannot stall.
            unsafe {
                core::ptr::write_volatile(self.avail_ring_entry(avail_slot), desc_id as u16);
                self.avail_idx = self.avail_idx.wrapping_add(1);
                fence(Ordering::Release);
                core::ptr::write_volatile(self.avail_idx_ptr(), self.avail_idx);
            }
            self.used_idx = self.used_idx.wrapping_add(1);
            consumed = true;
        }
        if consumed {
            self.kick();
        }
    }
}

static VIRTIO_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);
static HW_POINTER_LIVE: AtomicBool = AtomicBool::new(false);

/// Enable virtio-input statusq (queue 1) so DRIVER_OK covers every advertised
/// queue. The guest never produces LED/status reports on QEMU.
fn enable_status_queue(common: u64) {
    mmio_w16(common + CFG_QUEUE_SELECT, 1);
    let qs = mmio_r16(common + CFG_QUEUE_SIZE);
    let qsz = power_of_two_queue(qs);
    if qsz == 0 {
        return;
    }
    mmio_w16(common + CFG_QUEUE_SIZE, qsz);
    let qsz_us = qsz as usize;
    let desc_size = qsz_us * 16;
    let avail_size = 6 + 2 * qsz_us;
    let used_offset = align_up(desc_size + avail_size, 4096);
    let used_size = 6 + 8 * qsz_us;
    let pages = (used_offset + used_size).div_ceil(4096);
    let Some(vq_phys) = crate::vmm::allocate_contiguous_frames(pages) else {
        serial_println!("[VIRTIO-INPUT] statusq alloc failed");
        return;
    };
    // SAFETY: freshly allocated DMA pages exclusive to this unused status queue.
    unsafe {
        core::ptr::write_bytes(
            crate::vmm::phys_to_virt(vq_phys) as *mut u8,
            0,
            pages * 4096,
        );
    }
    mmio_w16(common + CFG_QUEUE_MSIX, 0xFFFF);
    mmio_w64(common + CFG_QUEUE_DESC, vq_phys);
    mmio_w64(common + CFG_QUEUE_DRIVER, vq_phys + desc_size as u64);
    mmio_w64(common + CFG_QUEUE_DEVICE, vq_phys + used_offset as u64);
    mmio_w16(common + CFG_QUEUE_ENABLE, 1);
}

lazy_static::lazy_static! {
    static ref INPUT_DEVICES: Mutex<Vec<VirtioInputDev>> = Mutex::new(Vec::new());
}

static REL_DX: AtomicI32 = AtomicI32::new(0);
static REL_DY: AtomicI32 = AtomicI32::new(0);
static ACCUM_WHEEL: AtomicI32 = AtomicI32::new(0);
static BTN_STATE: AtomicU32 = AtomicU32::new(0);

/// Sticky tablet axes (0..32767). -1 = never seen.
static ABS_CURSOR_X: AtomicI32 = AtomicI32::new(-1);
static ABS_CURSOR_Y: AtomicI32 = AtomicI32::new(-1);
static HAS_ABS_EVENT: AtomicBool = AtomicBool::new(false);
static POINTER_DIRTY: AtomicBool = AtomicBool::new(false);

const BTN_LEFT_BIT: u32 = 1;
const BTN_RIGHT_BIT: u32 = 2;
const BTN_MIDDLE_BIT: u32 = 4;

fn apply_button(code: u16, pressed: bool) -> bool {
    // BTN_TOOL_FINGER / BTN_TOOL_PEN are proximity, not a click. Mapping them
    // to BTN_LEFT made the first QEMU tablet motion look like a press and
    // ran `proof_tree` on the bootloader stack.
    let bit = match code {
        BTN_LEFT | BTN_TOUCH => BTN_LEFT_BIT,
        BTN_RIGHT => BTN_RIGHT_BIT,
        BTN_MIDDLE => BTN_MIDDLE_BIT,
        0x113..=0x117 => 1u32 << (code - 0x110),
        // BTN_TOOL_FINGER / BTN_TOOL_PEN are proximity, not a press.
        _ => return false,
    };
    if pressed {
        BTN_STATE.fetch_or(bit, Ordering::Relaxed);
    } else {
        BTN_STATE.fetch_and(!bit, Ordering::Relaxed);
    }
    true
}

fn pointer_xy() -> (i32, i32) {
    let (sw, sh) = crate::gui::cached_screen_size();
    let (mx, my) = {
        let mouse = crate::gui::input::MOUSE.lock();
        (mouse.x, mouse.y)
    };
    let abs_x = ABS_CURSOR_X.load(Ordering::Relaxed);
    let abs_y = ABS_CURSOR_Y.load(Ordering::Relaxed);
    let x = if abs_x >= 0 && sw > 1 {
        ((abs_x as i64 * sw as i64) / 32768).min(sw as i64 - 1) as i32
    } else {
        mx
    };
    let y = if abs_y >= 0 && sh > 1 {
        ((abs_y as i64 * sh as i64) / 32768).min(sh as i64 - 1) as i32
    } else {
        my
    };
    (x, y)
}

fn commit_pointer() {
    let (x, y) = pointer_xy();
    let wheel = ACCUM_WHEEL.swap(0, Ordering::Relaxed);
    let btns = BTN_STATE.load(Ordering::Relaxed);
    REL_DX.store(0, Ordering::Relaxed);
    REL_DY.store(0, Ordering::Relaxed);
    crate::gui::input::set_absolute_mouse(
        x,
        y,
        btns & BTN_LEFT_BIT != 0,
        btns & BTN_RIGHT_BIT != 0,
        btns & BTN_MIDDLE_BIT != 0,
        wheel.clamp(-127, 127) as i8,
    );
}

fn dispatch_input_event(evt: VirtioInputEvent) {
    match evt.event_type {
        EV_ABS => match evt.code {
            ABS_X => {
                ABS_CURSOR_X.store(evt.value as i32, Ordering::Relaxed);
                HAS_ABS_EVENT.store(true, Ordering::Relaxed);
                POINTER_DIRTY.store(true, Ordering::Relaxed);
            }
            ABS_Y => {
                ABS_CURSOR_Y.store(evt.value as i32, Ordering::Relaxed);
                HAS_ABS_EVENT.store(true, Ordering::Relaxed);
                POINTER_DIRTY.store(true, Ordering::Relaxed);
            }
            _ => {}
        },
        EV_REL => match evt.code {
            REL_X => {
                REL_DX.fetch_add(evt.value as i32, Ordering::Relaxed);
                POINTER_DIRTY.store(true, Ordering::Relaxed);
            }
            REL_Y => {
                REL_DY.fetch_add(evt.value as i32, Ordering::Relaxed);
                POINTER_DIRTY.store(true, Ordering::Relaxed);
            }
            REL_WHEEL_CODE => {
                ACCUM_WHEEL.fetch_add(evt.value as i32, Ordering::Relaxed);
                POINTER_DIRTY.store(true, Ordering::Relaxed);
            }
            _ => {}
        },
        EV_KEY => {
            let pressed = evt.value != 0;
            if apply_button(evt.code, pressed) {
                POINTER_DIRTY.store(true, Ordering::Relaxed);
            } else if evt.code < 0x80 {
                let sc = evt.code as u8;
                if pressed {
                    crate::task::keyboard::add_scancode(sc);
                } else {
                    crate::task::keyboard::add_scancode(0x80 | sc);
                }
            }
        }
        EV_SYN => {
            if HAS_ABS_EVENT.load(Ordering::Relaxed)
                || REL_DX.load(Ordering::Relaxed) != 0
                || REL_DY.load(Ordering::Relaxed) != 0
                || ACCUM_WHEEL.load(Ordering::Relaxed) != 0
            {
                POINTER_DIRTY.store(true, Ordering::Relaxed);
            }
        }
        _ => {}
    }
}

fn flush_pointer() {
    if !POINTER_DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    if HAS_ABS_EVENT.swap(false, Ordering::Relaxed) || ABS_CURSOR_X.load(Ordering::Relaxed) >= 0 {
        commit_pointer();
        REL_DX.store(0, Ordering::Relaxed);
        REL_DY.store(0, Ordering::Relaxed);
        return;
    }
    let dx = REL_DX.swap(0, Ordering::Relaxed);
    let dy = REL_DY.swap(0, Ordering::Relaxed);
    let wheel = ACCUM_WHEEL.swap(0, Ordering::Relaxed);
    if dx != 0 || dy != 0 || wheel != 0 {
        let btns = BTN_STATE.load(Ordering::Relaxed);
        inject_ps2_packet(dx, dy, btns as u8, wheel.clamp(-127, 127) as i8);
    } else {
        commit_pointer();
    }
}

fn inject_ps2_packet(dx: i32, dy: i32, buttons: u8, scroll: i8) {
    let dx = dx.clamp(-255, 255);
    let dy_ps2 = (-dy).clamp(-255, 255);

    let mut byte0: u8 = 0x08;
    byte0 |= buttons & 0x07;
    if dx < 0 {
        byte0 |= 0x10;
    }
    if dy_ps2 < 0 {
        byte0 |= 0x20;
    }

    crate::gui::input::add_mouse_byte(byte0);
    crate::gui::input::add_mouse_byte((dx & 0xFF) as u8);
    crate::gui::input::add_mouse_byte((dy_ps2 & 0xFF) as u8);
    crate::gui::input::add_mouse_byte(scroll as u8);
}

fn tablet_event_self_test() -> bool {
    let (ox, oy) = {
        let mouse = crate::gui::input::MOUSE.lock();
        (mouse.x, mouse.y)
    };
    dispatch_input_event(VirtioInputEvent {
        event_type: EV_ABS,
        code: ABS_X,
        value: 16384,
    });
    dispatch_input_event(VirtioInputEvent {
        event_type: EV_ABS,
        code: ABS_Y,
        value: 8192,
    });
    dispatch_input_event(VirtioInputEvent {
        event_type: EV_SYN,
        code: 0,
        value: 0,
    });
    flush_pointer();
    let (nx, ny) = {
        let mouse = crate::gui::input::MOUSE.lock();
        (mouse.x, mouse.y)
    };
    if nx == ox && ny == oy {
        serial_println!("[VIRTIO-INPUT] GATE FAILED: cursor stayed at {},{}", ox, oy);
        return false;
    }
    serial_println!(
        "[VIRTIO-INPUT] {} x={} y={}",
        GATE_VIRTIO_INPUT_MARKER,
        nx,
        ny
    );
    true
}

/// Initialize VirtIO input devices (legacy I/O BAR or modern MMIO).
pub fn init() {
    serial_println!("[VIRTIO-INPUT] Scanning for VirtIO input devices...");

    let pci_devices = scan_pci_bus();
    let mut count = 0u32;

    serial_println!(
        "[VIRTIO-INPUT] PCI scan found {} device(s) total",
        pci_devices.len()
    );

    for dev in &pci_devices {
        if dev.vendor_id != VIRTIO_PCI_VENDOR {
            continue;
        }
        let subsystem = pci::pci_config_read16(dev.bus, dev.device, dev.function, 0x2E);
        serial_println!(
            "[VIRTIO-INPUT] VirtIO device {:02x}:{:02x}.{} devid={:#x} subsys={} bar0={:#x}",
            dev.bus,
            dev.device,
            dev.function,
            dev.device_id,
            subsystem,
            dev.bar0
        );
        if !is_virtio_input(dev.device_id, subsystem) {
            continue;
        }

        enable_pci_bus_master(dev.bus, dev.device, dev.function);
        let mut input_dev = VirtioInputDev::empty();
        // Prefer modern virtio-pci (same path as VirtIO-GPU). Transitional
        // devices also expose an I/O BAR0; using that legacy map first was
        // dropping tablet events on QEMU cocoa.
        let ok = if input_dev.init_modern(dev.bus, dev.device, dev.function) {
            serial_println!(
                "[VIRTIO-INPUT] modern MMIO at {:02x}:{:02x}.{}",
                dev.bus,
                dev.device,
                dev.function
            );
            true
        } else if dev.bar0 & 1 != 0 {
            let io_base = (dev.bar0 & 0xFFFC) as u16;
            serial_println!(
                "[VIRTIO-INPUT] legacy I/O BAR at {:02x}:{:02x}.{} io_base={:#x}",
                dev.bus,
                dev.device,
                dev.function,
                io_base
            );
            input_dev.init_legacy(io_base)
        } else {
            serial_println!(
                "[VIRTIO-INPUT] bind failed at {:02x}:{:02x}.{}",
                dev.bus,
                dev.device,
                dev.function
            );
            false
        };
        if ok {
            input_dev.kick();
            INPUT_DEVICES.lock().push(input_dev);
            count += 1;
        }
    }

    if count > 0 {
        VIRTIO_INPUT_ACTIVE.store(true, Ordering::SeqCst);
        serial_println!("[VIRTIO-INPUT] {} input device(s) active", count);
    } else {
        serial_println!("[VIRTIO-INPUT] No VirtIO input devices found (PS/2 + USB fallback)");
    }
    let _ = tablet_event_self_test();
}

pub fn poll() {
    if !VIRTIO_INPUT_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let mut pending = Vec::new();
    {
        let mut devs = INPUT_DEVICES.lock();
        for dev in devs.iter_mut() {
            dev.drain_events(&mut pending);
        }
    }
    if pending.is_empty() {
        return;
    }
    for evt in pending {
        if (evt.event_type == EV_ABS || evt.event_type == EV_REL)
            && HW_POINTER_LIVE
                .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            serial_println!("[VIRTIO-INPUT] live pointer events");
        }
        dispatch_input_event(evt);
    }
    flush_pointer();
}

pub fn is_active() -> bool {
    VIRTIO_INPUT_ACTIVE.load(Ordering::Relaxed)
}

/// True after a real virtqueue pointer event (not the software KAT).
pub fn pointer_live() -> bool {
    HW_POINTER_LIVE.load(Ordering::Relaxed)
}
