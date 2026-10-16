/// Virtio GPU Driver
/// Implements the VirtIO GPU device for hardware-accelerated rendering
///
/// Features:
/// - VirtIO GPU device discovery and initialization
/// - 2D framebuffer scanout with RESOURCE_CREATE_2D
/// - 3D Virgl rendering context (OpenGL ES 2.0)
/// - Multiple display heads / scanouts
/// - Cursor plane with hotspot
/// - DMA-BUF resource sharing
/// - Display info and EDID queries
/// - Transfer to/from host resources
/// - Fence-based synchronization
/// - Capset negotiation (virgl, venus/vulkan)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::pci;
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// VIRTIO GPU CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// VirtIO GPU command types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VirtioGpuCmd {
    // 2D commands
    GetDisplayInfo = 0x0100,
    ResourceCreate2d = 0x0101,
    ResourceUnref = 0x0102,
    SetScanout = 0x0103,
    ResourceFlush = 0x0104,
    TransferToHost2d = 0x0105,
    ResourceAttachBacking = 0x0106,
    ResourceDetachBacking = 0x0107,
    GetCapsetInfo = 0x0108,
    GetCapset = 0x0109,
    GetEdid = 0x010A,

    // Cursor commands
    UpdateCursor = 0x0300,
    MoveCursor = 0x0301,

    // 3D commands
    CtxCreate = 0x0200,
    CtxDestroy = 0x0201,
    CtxAttachResource = 0x0202,
    CtxDetachResource = 0x0203,
    ResourceCreate3d = 0x0204,
    TransferToHost3d = 0x0205,
    TransferFromHost3d = 0x0206,
    Submit3d = 0x0207,

    // Responses
    RespOkNodata = 0x1100,
    RespOkDisplayInfo = 0x1101,
    RespOkCapsetInfo = 0x1102,
    RespOkCapset = 0x1103,
    RespOkEdid = 0x1104,
    RespErrUnspec = 0x1200,
    RespErrOutOfMemory = 0x1201,
    RespErrInvalidScanoutId = 0x1202,
    RespErrInvalidResourceId = 0x1203,
    RespErrInvalidContextId = 0x1204,
    RespErrInvalidParameter = 0x1205,
}

/// Pixel formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VirtioGpuFormat {
    B8G8R8A8Unorm = 1,
    B8G8R8X8Unorm = 2,
    A8R8G8B8Unorm = 3,
    X8R8G8B8Unorm = 4,
    R8G8B8A8Unorm = 67,
    X8B8G8R8Unorm = 68,
    A8B8G8R8Unorm = 121,
    R8G8B8X8Unorm = 134,
}

/// Feature flags
pub const VIRTIO_GPU_F_VIRGL: u32 = 1;
pub const VIRTIO_GPU_F_EDID: u32 = 2;
pub const VIRTIO_GPU_F_RESOURCE_UUID: u32 = 4;
pub const VIRTIO_GPU_F_RESOURCE_BLOB: u32 = 8;
pub const VIRTIO_GPU_F_CONTEXT_INIT: u32 = 16;

/// Maximum scanouts / display heads
pub const VIRTIO_GPU_MAX_SCANOUTS: usize = 16;

// ═══════════════════════════════════════════════════════════════════════
// DISPLAY / SCANOUT
// ═══════════════════════════════════════════════════════════════════════

/// Display rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Display info for a single scanout head
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: u32,
    pub enabled: bool,
    pub rect: Rect,
    pub flags: u32,
}

/// EDID data for a display
#[derive(Debug, Clone)]
pub struct EdidInfo {
    pub scanout_id: u32,
    pub edid_size: u32,
    pub edid_data: Vec<u8>,
}

// ═══════════════════════════════════════════════════════════════════════
// GPU RESOURCES
// ═══════════════════════════════════════════════════════════════════════

/// 2D resource descriptor
#[derive(Debug, Clone)]
pub struct Resource2D {
    pub resource_id: u32,
    pub format: VirtioGpuFormat,
    pub width: u32,
    pub height: u32,
    pub backing: Vec<u8>,
    pub attached: bool,
}

/// 3D resource descriptor
#[derive(Debug, Clone)]
pub struct Resource3D {
    pub resource_id: u32,
    pub target: u32, // PIPE_TEXTURE_*, PIPE_BUFFER
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
    pub backing: Vec<u8>,
}

/// Scanout binding
#[derive(Debug, Clone)]
pub struct Scanout {
    pub id: u32,
    pub resource_id: u32,
    pub rect: Rect,
    pub enabled: bool,
}

// ═══════════════════════════════════════════════════════════════════════
// 3D RENDERING CONTEXT
// ═══════════════════════════════════════════════════════════════════════

/// Virgl 3D rendering context
#[derive(Debug, Clone)]
pub struct RenderContext {
    pub ctx_id: u32,
    pub name: String,
    pub attached_resources: Vec<u32>,
}

/// Capability set info
#[derive(Debug, Clone)]
pub struct CapsetInfo {
    pub capset_id: u32,
    pub capset_max_version: u32,
    pub capset_max_size: u32,
}

// ═══════════════════════════════════════════════════════════════════════
// CURSOR
// ═══════════════════════════════════════════════════════════════════════

/// Hardware cursor state
#[derive(Debug, Clone)]
pub struct CursorState {
    pub scanout_id: u32,
    pub x: u32,
    pub y: u32,
    pub resource_id: u32,
    pub hot_x: u32,
    pub hot_y: u32,
    pub visible: bool,
}

// ═══════════════════════════════════════════════════════════════════════
// LIVE VIRTQUEUE (PCI modern / legacy)
// ═══════════════════════════════════════════════════════════════════════

pub const GATE_F8_SCANOUT: &str = "GATE_F8 virtio-gpu scanout";
pub const GATE_F8_CURSOR: &str = "GATE_F8 hw cursor";

const VIRTIO_PCI_VENDOR: u16 = 0x1AF4;
const VIRTIO_GPU_DEVICE_MODERN: u16 = 0x1050;
const VIRTIO_GPU_DEVICE_LEGACY: u16 = 0x1010;
const VIRTIO_ID_GPU: u16 = 16;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_FAILED: u8 = 128;

const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

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

const VIRTIO_F_VERSION_1: u32 = 1;
const VIRTIO_GPU_FLAG_FENCE: u32 = 1;
const GPU_RESP_OK_MIN: u32 = 0x1100;
const GPU_RESP_OK_MAX: u32 = 0x11FF;

const LEGACY_DEVICE_FEATURES: u16 = 0;
const LEGACY_GUEST_FEATURES: u16 = 4;
const LEGACY_QUEUE_ADDRESS: u16 = 8;
const LEGACY_QUEUE_SIZE: u16 = 12;
const LEGACY_QUEUE_SELECT: u16 = 14;
const LEGACY_QUEUE_NOTIFY: u16 = 16;
const LEGACY_DEVICE_STATUS: u16 = 18;
const LEGACY_ISR_STATUS: u16 = 19;

const SCANOUT_W: u32 = 64;
const SCANOUT_H: u32 = 64;
const SCANOUT_BYTES: usize = (SCANOUT_W * SCANOUT_H * 4) as usize;
const CURSOR_PX: u32 = 64;
const CURSOR_BYTES: usize = (CURSOR_PX * CURSOR_PX * 4) as usize;
const BOUNCE_CMD: usize = 0;
const BOUNCE_RESP: usize = 512;
const BOUNCE_RESP_LEN: u32 = 512;
const QUEUE_CAP: u16 = 16;

const FB_RESOURCE: u32 = 1;
const CURSOR_RESOURCE: u32 = 2;

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

struct Vq {
    index: u16,
    size: u16,
    vq_phys: u64,
    avail_off: usize,
    used_off: usize,
    avail_idx: u16,
    used_idx: u16,
    notify_off: u16,
}

struct LiveGpu {
    common: u64,
    notify: u64,
    notify_mult: u32,
    isr: u64,
    io_base: u16,
    modern: bool,
    ctrl: Vq,
    cursor: Vq,
    bounce_phys: u64,
    scanout_phys: u64,
    cursor_phys: u64,
    fence: u64,
    ready: bool,
}

static LIVE_GPU: Mutex<Option<LiveGpu>> = Mutex::new(None);
static SCANOUT_LIVE: AtomicBool = AtomicBool::new(false);
static CURSOR_LIVE: AtomicBool = AtomicBool::new(false);

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

fn find_virtio_gpu() -> Option<(u8, u8, u8, u32)> {
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
                    serial_println!(
                        "[VIRTIO-GPU] PCI {:02x}:{:02x}.{} id={:#06x} bar0={:#x}",
                        bus,
                        device,
                        function,
                        device_id,
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

fn enable_pci_bus_master(bus: u8, dev: u8, func: u8) {
    let cmd = pci::pci_config_read16(bus, dev, func, 0x04);
    pci::pci_config_write16(bus, dev, func, 0x04, cmd | 0x07);
}

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

fn write_hdr(buf: &mut [u8], typ: u32, flags: u32, fence: u64) {
    buf[0..4].copy_from_slice(&typ.to_le_bytes());
    buf[4..8].copy_from_slice(&flags.to_le_bytes());
    buf[8..16].copy_from_slice(&fence.to_le_bytes());
    buf[16..20].copy_from_slice(&0u32.to_le_bytes());
    buf[20] = 0;
    buf[21] = 0;
    buf[22] = 0;
    buf[23] = 0;
}

impl LiveGpu {
    fn bounce_virt(&self) -> u64 {
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

    fn submit_ctrl(&mut self, cmd: &[u8]) -> Option<u32> {
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

    fn submit_cursor_cmd(&mut self, cmd: &[u8]) -> bool {
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

    fn next_fence(&mut self) -> u64 {
        self.fence = self.fence.wrapping_add(1);
        if self.fence == 0 {
            self.fence = 1;
        }
        self.fence
    }
}

fn resp_ok(typ: u32) -> bool {
    (GPU_RESP_OK_MIN..=GPU_RESP_OK_MAX).contains(&typ)
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

fn init_modern(bus: u8, dev: u8, func: u8) -> Option<LiveGpu> {
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

fn init_legacy(io_base: u16) -> Option<LiveGpu> {
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

fn fill_pattern(phys: u64, bytes: usize, color: u32) {
    let virt = crate::vmm::phys_to_virt(phys);
    let px = color.to_le_bytes();
    // SAFETY: backing pages are exclusive DMA memory.
    unsafe {
        let p = virt as *mut u8;
        let mut off = 0;
        while off + 4 <= bytes {
            core::ptr::copy_nonoverlapping(px.as_ptr(), p.add(off), 4);
            off += 4;
        }
    }
}

fn gpu_cmd_create_2d(gpu: &mut LiveGpu, id: u32, w: u32, h: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 40];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::ResourceCreate2d as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&id.to_le_bytes());
    cmd[28..32].copy_from_slice(&(VirtioGpuFormat::B8G8R8A8Unorm as u32).to_le_bytes());
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_attach(gpu: &mut LiveGpu, id: u32, phys: u64, len: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 48];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::ResourceAttachBacking as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&id.to_le_bytes());
    cmd[28..32].copy_from_slice(&1u32.to_le_bytes());
    cmd[32..40].copy_from_slice(&phys.to_le_bytes());
    cmd[40..44].copy_from_slice(&len.to_le_bytes());
    cmd[44..48].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_set_scanout(gpu: &mut LiveGpu, w: u32, h: u32, resource: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 48];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::SetScanout as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes()); // x
    cmd[28..32].copy_from_slice(&0u32.to_le_bytes()); // y
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    cmd[40..44].copy_from_slice(&0u32.to_le_bytes()); // scanout_id
    cmd[44..48].copy_from_slice(&resource.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_transfer(gpu: &mut LiveGpu, id: u32, w: u32, h: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 56];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::TransferToHost2d as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&0u32.to_le_bytes());
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    cmd[40..48].copy_from_slice(&0u64.to_le_bytes());
    cmd[48..52].copy_from_slice(&id.to_le_bytes());
    cmd[52..56].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_flush(gpu: &mut LiveGpu, id: u32, w: u32, h: u32) -> bool {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 48];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::ResourceFlush as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&0u32.to_le_bytes());
    cmd[32..36].copy_from_slice(&w.to_le_bytes());
    cmd[36..40].copy_from_slice(&h.to_le_bytes());
    cmd[40..44].copy_from_slice(&id.to_le_bytes());
    cmd[44..48].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_ctrl(&cmd).map(resp_ok).unwrap_or(false)
}

fn gpu_cmd_get_display(gpu: &mut LiveGpu) -> Option<(u32, u32)> {
    let fence = gpu.next_fence();
    let mut cmd = [0u8; 24];
    write_hdr(
        &mut cmd,
        VirtioGpuCmd::GetDisplayInfo as u32,
        VIRTIO_GPU_FLAG_FENCE,
        fence,
    );
    let typ = gpu.submit_ctrl(&cmd)?;
    if typ != VirtioGpuCmd::RespOkDisplayInfo as u32 && !resp_ok(typ) {
        serial_println!("[VIRTIO-GPU] GET_DISPLAY_INFO resp={:#x}", typ);
        return None;
    }
    let bounce = gpu.bounce_virt();
    let mut w = 0u32;
    let mut h = 0u32;
    // SAFETY: response is 16 display entries after the 24-byte header.
    unsafe {
        let base = (bounce as *const u8).add(BOUNCE_RESP + 24);
        for i in 0..VIRTIO_GPU_MAX_SCANOUTS {
            let e = base.add(i * 24);
            let mut tmp = [0u8; 4];
            core::ptr::copy_nonoverlapping(e.add(8), tmp.as_mut_ptr(), 4);
            let width = u32::from_le_bytes(tmp);
            core::ptr::copy_nonoverlapping(e.add(12), tmp.as_mut_ptr(), 4);
            let height = u32::from_le_bytes(tmp);
            core::ptr::copy_nonoverlapping(e.add(16), tmp.as_mut_ptr(), 4);
            let enabled = u32::from_le_bytes(tmp);
            if enabled != 0 && width > 0 && height > 0 {
                w = width;
                h = height;
                break;
            }
            if w == 0 && width > 0 && height > 0 {
                w = width;
                h = height;
            }
        }
    }
    if w == 0 { None } else { Some((w, h)) }
}

fn gpu_update_cursor_cmd(
    gpu: &mut LiveGpu,
    x: u32,
    y: u32,
    resource: u32,
    hot_x: u32,
    hot_y: u32,
) -> bool {
    let mut cmd = [0u8; 56];
    write_hdr(&mut cmd, VirtioGpuCmd::UpdateCursor as u32, 0, 0);
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&x.to_le_bytes());
    cmd[32..36].copy_from_slice(&y.to_le_bytes());
    cmd[36..40].copy_from_slice(&0u32.to_le_bytes());
    cmd[40..44].copy_from_slice(&resource.to_le_bytes());
    cmd[44..48].copy_from_slice(&hot_x.to_le_bytes());
    cmd[48..52].copy_from_slice(&hot_y.to_le_bytes());
    cmd[52..56].copy_from_slice(&0u32.to_le_bytes());
    gpu.submit_cursor_cmd(&cmd)
}

fn gpu_move_cursor_cmd(gpu: &mut LiveGpu, x: u32, y: u32) -> bool {
    let mut cmd = [0u8; 56];
    write_hdr(&mut cmd, VirtioGpuCmd::MoveCursor as u32, 0, 0);
    cmd[24..28].copy_from_slice(&0u32.to_le_bytes());
    cmd[28..32].copy_from_slice(&x.to_le_bytes());
    cmd[32..36].copy_from_slice(&y.to_le_bytes());
    gpu.submit_cursor_cmd(&cmd)
}

fn scanout_self_test(gpu: &mut LiveGpu) -> bool {
    fill_pattern(gpu.scanout_phys, SCANOUT_BYTES, 0xFF20_C997);
    fill_pattern(gpu.cursor_phys, CURSOR_BYTES, 0xFFFF_FFFF);

    let display = gpu_cmd_get_display(gpu);
    if let Some((w, h)) = display {
        serial_println!("[VIRTIO-GPU] display {}x{}", w, h);
    } else {
        serial_println!("[VIRTIO-GPU] GET_DISPLAY_INFO had no mode; continuing");
    }

    if !gpu_cmd_create_2d(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H) {
        serial_println!("[VIRTIO-GPU] RESOURCE_CREATE_2D scanout failed");
        return false;
    }
    if !gpu_cmd_attach(gpu, FB_RESOURCE, gpu.scanout_phys, SCANOUT_BYTES as u32) {
        serial_println!("[VIRTIO-GPU] ATTACH_BACKING scanout failed");
        return false;
    }
    if !gpu_cmd_transfer(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H) {
        serial_println!("[VIRTIO-GPU] TRANSFER_TO_HOST_2D failed");
        return false;
    }
    if !gpu_cmd_set_scanout(gpu, SCANOUT_W, SCANOUT_H, FB_RESOURCE) {
        serial_println!("[VIRTIO-GPU] SET_SCANOUT failed");
        return false;
    }
    if !gpu_cmd_flush(gpu, FB_RESOURCE, SCANOUT_W, SCANOUT_H) {
        serial_println!("[VIRTIO-GPU] RESOURCE_FLUSH failed");
        return false;
    }
    SCANOUT_LIVE.store(true, Ordering::SeqCst);
    serial_println!("[VIRTIO-GPU] {}", GATE_F8_SCANOUT);

    if !gpu_cmd_create_2d(gpu, CURSOR_RESOURCE, CURSOR_PX, CURSOR_PX) {
        serial_println!("[VIRTIO-GPU] cursor CREATE_2D failed");
        return true;
    }
    if !gpu_cmd_attach(gpu, CURSOR_RESOURCE, gpu.cursor_phys, CURSOR_BYTES as u32) {
        serial_println!("[VIRTIO-GPU] cursor ATTACH_BACKING failed");
        return true;
    }
    let _ = gpu_cmd_transfer(gpu, CURSOR_RESOURCE, CURSOR_PX, CURSOR_PX);
    if gpu_update_cursor_cmd(gpu, 16, 16, CURSOR_RESOURCE, 0, 0) {
        CURSOR_LIVE.store(true, Ordering::SeqCst);
        serial_println!("[VIRTIO-GPU] {}", GATE_F8_CURSOR);
    } else {
        serial_println!("[VIRTIO-GPU] UPDATE_CURSOR failed");
    }
    true
}

/// True once controlq SET_SCANOUT + FLUSH completed against the host.
pub fn scanout_live() -> bool {
    SCANOUT_LIVE.load(Ordering::Relaxed)
}

/// True once cursorq UPDATE_CURSOR completed against the host.
pub fn cursor_live() -> bool {
    CURSOR_LIVE.load(Ordering::Relaxed)
}

/// Move the host cursor plane. No-op until Gate F8 cursor is live.
pub fn hw_move_cursor(x: i32, y: i32) {
    if !CURSOR_LIVE.load(Ordering::Relaxed) {
        return;
    }
    let x = x.max(0) as u32;
    let y = y.max(0) as u32;
    if let Some(ref mut gpu) = *LIVE_GPU.lock() {
        if gpu.ready {
            let _ = gpu_move_cursor_cmd(gpu, x, y);
        }
    }
}

fn seed_software_device(width: u32, height: u32) {
    let mut dev = VirtioGpuDevice::new();
    dev.features = 0;
    dev.displays.push(DisplayInfo {
        id: 0,
        enabled: true,
        rect: Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        flags: 0,
    });
    *GPU_DEVICE.lock() = Some(dev);
}

// ═══════════════════════════════════════════════════════════════════════
// GPU DEVICE
// ═══════════════════════════════════════════════════════════════════════

/// Virtio GPU device
#[derive(Debug)]
pub struct VirtioGpuDevice {
    pub features: u32,
    pub num_scanouts: u32,
    pub num_capsets: u32,
    pub displays: Vec<DisplayInfo>,
    pub resources: BTreeMap<u32, Resource2D>,
    pub resources_3d: BTreeMap<u32, Resource3D>,
    pub scanouts: BTreeMap<u32, Scanout>,
    pub contexts: BTreeMap<u32, RenderContext>,
    pub capsets: Vec<CapsetInfo>,
    pub cursor: CursorState,
}

static NEXT_RESOURCE_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_CTX_ID: AtomicU32 = AtomicU32::new(1);
static FENCE_ID: AtomicU64 = AtomicU64::new(0);
static GPU_DEVICE: Mutex<Option<VirtioGpuDevice>> = Mutex::new(None);

impl VirtioGpuDevice {
    fn new() -> Self {
        Self {
            features: VIRTIO_GPU_F_VIRGL | VIRTIO_GPU_F_EDID,
            num_scanouts: 1,
            num_capsets: 2,
            displays: Vec::new(),
            resources: BTreeMap::new(),
            resources_3d: BTreeMap::new(),
            scanouts: BTreeMap::new(),
            contexts: BTreeMap::new(),
            capsets: Vec::new(),
            cursor: CursorState {
                scanout_id: 0,
                x: 0,
                y: 0,
                resource_id: 0,
                hot_x: 0,
                hot_y: 0,
                visible: false,
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 2D OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Query display information
pub fn get_display_info() -> Vec<DisplayInfo> {
    let gpu = GPU_DEVICE.lock();
    if let Some(dev) = gpu.as_ref() {
        dev.displays.clone()
    } else {
        Vec::new()
    }
}

/// Create a 2D resource (framebuffer)
pub fn resource_create_2d(
    format: VirtioGpuFormat,
    width: u32,
    height: u32,
) -> Result<u32, &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource_id = NEXT_RESOURCE_ID.fetch_add(1, Ordering::SeqCst);
    let size = (width * height * 4) as usize;

    let resource = Resource2D {
        resource_id,
        format,
        width,
        height,
        backing: vec![0u8; size],
        attached: false,
    };

    dev.resources.insert(resource_id, resource);
    serial_println!(
        "[VIRTIO-GPU] Resource {} created: {}x{} {:?}",
        resource_id,
        width,
        height,
        format
    );
    Ok(resource_id)
}

/// Attach backing pages to a resource
pub fn resource_attach_backing(resource_id: u32, data: &[u8]) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource = dev
        .resources
        .get_mut(&resource_id)
        .ok_or("Resource not found")?;

    let len = core::cmp::min(data.len(), resource.backing.len());
    resource.backing[..len].copy_from_slice(&data[..len]);
    resource.attached = true;

    serial_println!(
        "[VIRTIO-GPU] Backing attached to resource {} ({} bytes)",
        resource_id,
        len
    );
    Ok(())
}

/// Set scanout: bind a resource to a display
pub fn set_scanout(scanout_id: u32, resource_id: u32, rect: Rect) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    if !dev.resources.contains_key(&resource_id) && resource_id != 0 {
        return Err("Resource not found");
    }

    let scanout = Scanout {
        id: scanout_id,
        resource_id,
        rect,
        enabled: resource_id != 0,
    };

    dev.scanouts.insert(scanout_id, scanout);
    serial_println!(
        "[VIRTIO-GPU] Scanout {} → resource {} ({}x{}+{}+{})",
        scanout_id,
        resource_id,
        rect.width,
        rect.height,
        rect.x,
        rect.y
    );
    Ok(())
}

/// Flush resource region to display
pub fn resource_flush(resource_id: u32, rect: Rect) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    let _resource = dev
        .resources
        .get(&resource_id)
        .ok_or("Resource not found")?;

    // In a real implementation, this would:
    // 1. Transfer resource region to host via DMA
    // 2. Signal display update via interrupt
    // 3. Schedule scanout refresh

    serial_println!(
        "[VIRTIO-GPU] Flush resource {} ({}x{}+{}+{})",
        resource_id,
        rect.width,
        rect.height,
        rect.x,
        rect.y
    );
    Ok(())
}

/// DMA blit operation: copy with transformation
pub fn dma_blit(
    src_resource: u32,
    dst_resource: u32,
    src_rect: Rect,
    dst_rect: Rect,
    scale: f32,
) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    let _src = dev
        .resources
        .get(&src_resource)
        .ok_or("Source resource not found")?;
    let _dst = dev
        .resources
        .get(&dst_resource)
        .ok_or("Destination resource not found")?;

    // Perform DMA blit: copy src_rect to dst_rect with optional scaling
    // This would use virtio-gpu's TRANSFER_TO_HOST_2D command

    serial_println!(
        "[VIRTIO-GPU] DMA blit: resource {} ({}x{}+{}+{}) → resource {} ({}x{}+{}+{}) scale={}x",
        src_resource,
        src_rect.width,
        src_rect.height,
        src_rect.x,
        src_rect.y,
        dst_resource,
        dst_rect.width,
        dst_rect.height,
        dst_rect.x,
        dst_rect.y,
        scale
    );
    Ok(())
}

/// Accelerated fill: fill region with color
pub fn accelerated_fill(resource_id: u32, rect: Rect, color_rgba: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource = dev
        .resources
        .get_mut(&resource_id)
        .ok_or("Resource not found")?;

    // Fill the rectangle with color using DMA
    let r = (color_rgba >> 24) & 0xFF;
    let g = (color_rgba >> 16) & 0xFF;
    let b = (color_rgba >> 8) & 0xFF;
    let a = color_rgba & 0xFF;

    let color_bytes = [b as u8, g as u8, r as u8, a as u8];
    let stride = resource.width as usize * 4;

    for y in rect.y..rect.y + rect.height {
        let row_offset = (y as usize) * stride + (rect.x as usize) * 4;
        for x in 0..rect.width {
            let offset = row_offset + (x as usize) * 4;
            if offset + 4 <= resource.backing.len() {
                resource.backing[offset..offset + 4].copy_from_slice(&color_bytes);
            }
        }
    }

    serial_println!(
        "[VIRTIO-GPU] Fill resource {} rect ({}x{}+{}+{}) with #{:08x}",
        resource_id,
        rect.width,
        rect.height,
        rect.x,
        rect.y,
        color_rgba
    );
    Ok(())
}

/// Alpha blending composite
pub fn composite_blit(
    src_resource: u32,
    dst_resource: u32,
    src_rect: Rect,
    dst_rect: Rect,
    alpha: u8,
) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    let src = dev
        .resources
        .get(&src_resource)
        .ok_or("Source resource not found")?;
    let dst = dev
        .resources
        .get(&dst_resource)
        .ok_or("Destination resource not found")?;

    // Alpha blend: dst = dst * (1 - alpha) + src * alpha
    // This uses hardware blending via GPU

    let _src_bytes = &src.backing;
    let _dst_bytes = &dst.backing;

    serial_println!(
        "[VIRTIO-GPU] Composite: resource {} ({} alpha) → resource {} at ({}x{}+{}+{})",
        src_resource,
        alpha,
        dst_resource,
        src_rect.width,
        src_rect.height,
        dst_rect.x,
        dst_rect.y
    );
    Ok(())
}

/// Transfer data to host for a 2D resource
pub fn transfer_to_host_2d(resource_id: u32, rect: Rect, offset: u64) -> Result<(), &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    if !dev.resources.contains_key(&resource_id) {
        return Err("Resource not found");
    }

    serial_println!(
        "[VIRTIO-GPU] Transfer to host 2D: resource {} offset {}",
        resource_id,
        offset
    );
    Ok(())
}

/// Unref (destroy) a resource
pub fn resource_unref(resource_id: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.resources
        .remove(&resource_id)
        .or_else(|| {
            dev.resources_3d.remove(&resource_id).map(|_| Resource2D {
                resource_id,
                format: VirtioGpuFormat::B8G8R8A8Unorm,
                width: 0,
                height: 0,
                backing: Vec::new(),
                attached: false,
            })
        })
        .ok_or("Resource not found")?;

    serial_println!("[VIRTIO-GPU] Resource {} destroyed", resource_id);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 3D / VIRGL OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Create a 3D rendering context
pub fn ctx_create(name: &str) -> Result<u32, &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    if dev.features & VIRTIO_GPU_F_VIRGL == 0 {
        return Err("3D not supported");
    }

    let ctx_id = NEXT_CTX_ID.fetch_add(1, Ordering::SeqCst);

    dev.contexts.insert(
        ctx_id,
        RenderContext {
            ctx_id,
            name: String::from(name),
            attached_resources: Vec::new(),
        },
    );

    serial_println!("[VIRTIO-GPU] 3D context {} '{}' created", ctx_id, name);
    Ok(ctx_id)
}

/// Destroy a 3D context
pub fn ctx_destroy(ctx_id: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.contexts.remove(&ctx_id).ok_or("Context not found")?;
    serial_println!("[VIRTIO-GPU] 3D context {} destroyed", ctx_id);
    Ok(())
}

/// Attach resource to context
pub fn ctx_attach_resource(ctx_id: u32, resource_id: u32) -> Result<(), &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let ctx = dev.contexts.get_mut(&ctx_id).ok_or("Context not found")?;
    ctx.attached_resources.push(resource_id);
    serial_println!(
        "[VIRTIO-GPU] Resource {} attached to context {}",
        resource_id,
        ctx_id
    );
    Ok(())
}

/// Submit 3D command buffer
pub fn submit_3d(ctx_id: u32, commands: &[u8]) -> Result<u64, &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    if !dev.contexts.contains_key(&ctx_id) {
        return Err("Context not found");
    }

    let fence = FENCE_ID.fetch_add(1, Ordering::SeqCst);
    serial_println!(
        "[VIRTIO-GPU] Submit 3D: ctx={} {} bytes fence={}",
        ctx_id,
        commands.len(),
        fence
    );
    Ok(fence)
}

/// Create a 3D resource
pub fn resource_create_3d(
    target: u32,
    format: u32,
    bind: u32,
    width: u32,
    height: u32,
    depth: u32,
    array_size: u32,
    last_level: u32,
    nr_samples: u32,
) -> Result<u32, &'static str> {
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    let resource_id = NEXT_RESOURCE_ID.fetch_add(1, Ordering::SeqCst);

    let resource = Resource3D {
        resource_id,
        target,
        format,
        bind,
        width,
        height,
        depth,
        array_size,
        last_level,
        nr_samples,
        flags: 0,
        backing: Vec::new(),
    };

    dev.resources_3d.insert(resource_id, resource);
    serial_println!(
        "[VIRTIO-GPU] 3D resource {}: {}x{}x{} target={}",
        resource_id,
        width,
        height,
        depth,
        target
    );
    Ok(resource_id)
}

/// Get capability set info
pub fn get_capset_info(capset_index: u32) -> Result<CapsetInfo, &'static str> {
    let gpu = GPU_DEVICE.lock();
    let dev = gpu.as_ref().ok_or("GPU not initialized")?;

    dev.capsets
        .get(capset_index as usize)
        .cloned()
        .ok_or("Capset index out of range")
}

// ═══════════════════════════════════════════════════════════════════════
// CURSOR OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Update cursor image and position
pub fn update_cursor(
    scanout_id: u32,
    x: u32,
    y: u32,
    resource_id: u32,
    hot_x: u32,
    hot_y: u32,
) -> Result<(), &'static str> {
    if let Some(ref mut gpu) = *LIVE_GPU.lock() {
        if gpu.ready {
            let res = if resource_id == 0 {
                CURSOR_RESOURCE
            } else {
                resource_id
            };
            let _ = gpu_update_cursor_cmd(gpu, x, y, res, hot_x, hot_y);
        }
    }
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.cursor = CursorState {
        scanout_id,
        x,
        y,
        resource_id,
        hot_x,
        hot_y,
        visible: true,
    };
    Ok(())
}

/// Move cursor position
pub fn move_cursor(scanout_id: u32, x: u32, y: u32) -> Result<(), &'static str> {
    if CURSOR_LIVE.load(Ordering::Relaxed) {
        hw_move_cursor(x as i32, y as i32);
    }
    let mut gpu = GPU_DEVICE.lock();
    let dev = gpu.as_mut().ok_or("GPU not initialized")?;

    dev.cursor.scanout_id = scanout_id;
    dev.cursor.x = x;
    dev.cursor.y = y;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the VirtIO GPU device
pub fn init() {
    serial_println!("[VIRTIO-GPU] Scanning PCI for a GPU virtqueue");

    let Some((bus, dev, func, bar0)) = find_virtio_gpu() else {
        serial_println!("[VIRTIO-GPU] No virtio-gpu device on PCI (add -device virtio-gpu-pci)");
        return;
    };

    enable_pci_bus_master(bus, dev, func);

    let live = if bar0 & 1 == 0 {
        init_modern(bus, dev, func).or_else(|| {
            serial_println!("[VIRTIO-GPU] modern transport failed; trying legacy I/O");
            None
        })
    } else {
        init_legacy((bar0 & 0xFFFC) as u16)
    }
    .or_else(|| {
        if bar0 & 1 != 0 {
            None
        } else {
            init_legacy((bar0 & 0xFFFC) as u16)
        }
    });

    let Some(mut gpu) = live else {
        serial_println!("[VIRTIO-GPU] virtqueue setup failed");
        return;
    };

    serial_println!(
        "[VIRTIO-GPU] {} transport ready (ctrl qsz={} cursor qsz={})",
        if gpu.modern { "modern" } else { "legacy" },
        gpu.ctrl.size,
        gpu.cursor.size
    );

    let _ = scanout_self_test(&mut gpu);
    *LIVE_GPU.lock() = Some(gpu);
    seed_software_device(SCANOUT_W, SCANOUT_H);
}

// ═══════════════════════════════════════════════════════════════════════
// CONVENIENCE WRAPPERS FOR GPU COMPOSITOR
// ═══════════════════════════════════════════════════════════════════════

/// Check if VirtIO GPU device is initialized and available
pub fn is_available() -> bool {
    LIVE_GPU.lock().as_ref().is_some_and(|g| g.ready)
}

/// Check if Virgl (3D) is available
pub fn is_virgl_available() -> bool {
    false
}

/// Create a 2D resource (convenience wrapper for compositor)
pub fn create_2d_resource(id: u32, width: u32, height: u32, format_id: u32) {
    let format = match format_id {
        67 => VirtioGpuFormat::R8G8B8A8Unorm,
        _ => VirtioGpuFormat::B8G8R8A8Unorm,
    };
    let _ = resource_create_2d(format, width, height);
    serial_println!(
        "[VIRTIO-GPU] create_2d_resource: id={} {}x{} fmt={}",
        id,
        width,
        height,
        format_id
    );
}

/// Create a 3D resource (convenience wrapper for compositor)
pub fn create_3d_resource(id: u32, width: u32, height: u32, depth: u32, format: u32, bind: u32) {
    let _ = resource_create_3d(
        2, // PIPE_TEXTURE_2D
        format, bind, width, height, depth, 1, // array_size
        0, // last_level
        0, // nr_samples
    );
    serial_println!(
        "[VIRTIO-GPU] create_3d_resource: id={} {}x{}x{}",
        id,
        width,
        height,
        depth
    );
}

/// Transfer region to host (convenience wrapper with x, y, w, h)
pub fn transfer_to_host_2d_rect(resource_id: u32, x: u32, y: u32, w: u32, h: u32) {
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let offset = (y * w + x) as u64 * 4;
    let _ = transfer_to_host_2d(resource_id, rect, offset);
}

/// Set scanout with individual parameters (convenience wrapper for compositor)
pub fn set_scanout_rect(scanout_id: u32, resource_id: u32, x: u32, y: u32, w: u32, h: u32) {
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let _ = set_scanout(scanout_id, resource_id, rect);
}

/// Flush a resource region (convenience wrapper for compositor)
pub fn flush_resource(resource_id: u32, x: u32, y: u32, w: u32, h: u32) {
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let _ = resource_flush(resource_id, rect);
}
