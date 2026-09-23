use alloc::string::String;
/// Virtio Block Device Driver — Persistent storage via QEMU virtio-blk
///
/// Implements the virtio 1.0 specification for block devices over PCI transport.
/// Provides a block device interface for reading/writing disk sectors that
/// integrates with the existing block.rs layer and filesystem drivers.
///
/// Features:
///   - PCI bus discovery of virtio-blk devices
///   - Legacy virtio transport (compatible with QEMU -drive)
///   - Real virtqueue DMA-based sector read/write (512-byte sectors)
///   - Proper 3-descriptor chain: header → data → status
///   - ISR-based completion polling
///   - Integration with VFS mount system and block layer
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Virtio Block Constants ─────────────────────────────────────────────

/// Virtio block device subsystem ID
pub const VIRTIO_BLK_SUBSYSTEM: u16 = 2;

/// Virtio block feature bits
pub const VIRTIO_BLK_F_SIZE_MAX: u64 = 1 << 1;
pub const VIRTIO_BLK_F_SEG_MAX: u64 = 1 << 2;
pub const VIRTIO_BLK_F_GEOMETRY: u64 = 1 << 4;
pub const VIRTIO_BLK_F_RO: u64 = 1 << 5;
pub const VIRTIO_BLK_F_BLK_SIZE: u64 = 1 << 6;
pub const VIRTIO_BLK_F_FLUSH: u64 = 1 << 9;
pub const VIRTIO_BLK_F_TOPOLOGY: u64 = 1 << 10;

/// Virtio block request types
pub const VIRTIO_BLK_T_IN: u32 = 0; // Read
pub const VIRTIO_BLK_T_OUT: u32 = 1; // Write
pub const VIRTIO_BLK_T_FLUSH: u32 = 4; // Flush
pub const VIRTIO_BLK_T_GET_ID: u32 = 8; // Get device ID

/// Virtio block status codes
pub const VIRTIO_BLK_S_OK: u8 = 0;
pub const VIRTIO_BLK_S_IOERR: u8 = 1;
pub const VIRTIO_BLK_S_UNSUPP: u8 = 2;

/// Sector size
pub const SECTOR_SIZE: usize = 512;

/// Maximum sectors per request
pub const MAX_SECTORS_PER_REQUEST: usize = 128; // 64KB

/// Queue size (must be power of 2)
pub const BLK_QUEUE_SIZE: u16 = 128;

/// Legacy virtio PCI register offsets
pub const VIRTIO_PCI_DEVICE_FEATURES: u16 = 0;
pub const VIRTIO_PCI_GUEST_FEATURES: u16 = 4;
pub const VIRTIO_PCI_QUEUE_ADDRESS: u16 = 8;
pub const VIRTIO_PCI_QUEUE_SIZE: u16 = 12;
pub const VIRTIO_PCI_QUEUE_SELECT: u16 = 14;
pub const VIRTIO_PCI_QUEUE_NOTIFY: u16 = 16;
pub const VIRTIO_PCI_DEVICE_STATUS: u16 = 18;
pub const VIRTIO_PCI_ISR_STATUS: u16 = 19;

/// Block device config (starts after common config)
pub const VIRTIO_BLK_CFG_CAPACITY: u16 = 20; // Total sectors (u64)
pub const VIRTIO_BLK_CFG_SIZE_MAX: u16 = 28; // Max segment size
pub const VIRTIO_BLK_CFG_SEG_MAX: u16 = 32; // Max number of segments
pub const VIRTIO_BLK_CFG_BLK_SIZE: u16 = 40; // Block size

/// Virtqueue descriptor flags
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

// ─── Block Request ──────────────────────────────────────────────────────

/// Virtio block request header (prepended to all requests)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtioBlkReqHeader {
    pub req_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

impl VirtioBlkReqHeader {
    pub fn read(sector: u64) -> Self {
        Self {
            req_type: VIRTIO_BLK_T_IN,
            reserved: 0,
            sector,
        }
    }

    pub fn write(sector: u64) -> Self {
        Self {
            req_type: VIRTIO_BLK_T_OUT,
            reserved: 0,
            sector,
        }
    }

    pub fn flush() -> Self {
        Self {
            req_type: VIRTIO_BLK_T_FLUSH,
            reserved: 0,
            sector: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&self.req_type.to_le_bytes());
        buf[4..8].copy_from_slice(&self.reserved.to_le_bytes());
        buf[8..16].copy_from_slice(&self.sector.to_le_bytes());
        buf
    }
}

// ─── Virtqueue Descriptor ───────────────────────────────────────────────

/// Virtqueue descriptor (16 bytes, matches virtio spec exactly)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct VirtqDesc {
    addr: u64,  // Guest physical address of buffer
    len: u32,   // Length of buffer
    flags: u16, // VRING_DESC_F_NEXT, VRING_DESC_F_WRITE
    next: u16,  // Next descriptor index (if NEXT flag set)
}

/// Virtqueue available ring header (variable-size ring follows)
#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    // ring: [u16; queue_size] follows in memory
    // used_event: u16 follows ring
}

/// Virtqueue used ring element
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,  // Descriptor chain head
    len: u32, // Total bytes written to descriptor chain
}

/// Virtqueue used ring header
#[repr(C)]
struct VirtqUsedRing {
    flags: u16,
    idx: u16,
    // ring: [VirtqUsedElem; queue_size] follows
    // avail_event: u16 follows ring
}

// ─── Virtio Block Device ────────────────────────────────────────────────

/// Virtio block device with real virtqueue DMA
pub struct VirtioBlkDevice {
    /// I/O port base (from PCI BAR0)
    pub io_base: u16,
    /// PCI location
    pub pci_bus: u8,
    pub pci_device: u8,
    pub pci_function: u8,
    /// IRQ
    pub irq: u8,
    /// Total capacity in sectors
    pub capacity: u64,
    /// Block size (usually 512)
    pub block_size: u32,
    /// Read-only flag
    pub read_only: bool,
    /// Device is initialized
    pub ready: bool,
    /// Actual queue size from device
    pub queue_size: u16,

    /// Guest-physical base of the contiguous virtqueue (desc + avail + used).
    vq_phys: u64,
    /// Offset of the available ring within the virtqueue
    avail_off: usize,
    /// Offset of the used ring within the virtqueue (page-aligned)
    used_off: usize,
    /// Guest-physical bounce buffer: 16-byte header + 512-byte sector + status
    bounce_phys: u64,

    /// Avail ring index (shadows the ring's idx field)
    avail_idx: u16,
    /// Used ring index (last consumed)
    used_idx: u16,

    /// Statistics
    pub reads: u64,
    pub writes: u64,
    pub errors: u64,
    /// Device name
    pub name: String,
}

impl VirtioBlkDevice {
    pub fn new(io_base: u16, bus: u8, dev: u8, func: u8, irq: u8) -> Self {
        Self {
            io_base,
            pci_bus: bus,
            pci_device: dev,
            pci_function: func,
            irq,
            capacity: 0,
            block_size: SECTOR_SIZE as u32,
            read_only: false,
            ready: false,
            queue_size: 0,
            vq_phys: 0,
            avail_off: 0,
            used_off: 0,
            bounce_phys: 0,
            avail_idx: 0,
            used_idx: 0,
            reads: 0,
            writes: 0,
            errors: 0,
            name: String::from("vda"),
        }
    }

    fn vq_virt(&self) -> u64 {
        crate::vmm::phys_to_virt(self.vq_phys)
    }

    fn bounce_virt(&self) -> u64 {
        crate::vmm::phys_to_virt(self.bounce_phys)
    }

    fn desc_ptr(&self, i: u16) -> *mut VirtqDesc {
        (self.vq_virt() as *mut VirtqDesc).wrapping_add(i as usize)
    }

    fn avail_flags_ptr(&self) -> *mut u16 {
        (self.vq_virt() + self.avail_off as u64) as *mut u16
    }
    fn avail_idx_ptr(&self) -> *mut u16 {
        unsafe { self.avail_flags_ptr().add(1) }
    }
    fn avail_ring_entry(&self, i: u16) -> *mut u16 {
        unsafe { self.avail_flags_ptr().add(2 + i as usize) }
    }

    fn used_idx_ptr(&self) -> *const u16 {
        unsafe { ((self.vq_virt() + self.used_off as u64) as *const u16).add(1) }
    }
    fn used_ring_elem(&self, i: u16) -> *const VirtqUsedElem {
        unsafe {
            let base = ((self.vq_virt() + self.used_off as u64) as *const u16).add(2)
                as *const VirtqUsedElem;
            base.add(i as usize)
        }
    }

    /// Initialize the virtio-blk device with real virtqueue DMA
    pub fn init_legacy(&mut self) -> bool {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            use crate::arch_compat::instructions::port::Port;
            #[cfg(not(target_arch = "x86_64"))]
            use crate::arch_compat::instructions::port::Port;
            let base = self.io_base;

            // 1. Reset device
            let mut status_port: Port<u8> = Port::new(base + VIRTIO_PCI_DEVICE_STATUS);
            status_port.write(0);

            // 2. Acknowledge
            status_port.write(crate::virtio_net::VIRTIO_STATUS_ACKNOWLEDGE);

            // 3. Driver
            status_port.write(
                crate::virtio_net::VIRTIO_STATUS_ACKNOWLEDGE
                    | crate::virtio_net::VIRTIO_STATUS_DRIVER,
            );

            // 4. Read and negotiate features
            let mut features_port: Port<u32> = Port::new(base + VIRTIO_PCI_DEVICE_FEATURES);
            let device_features = features_port.read() as u64;

            let our_features = device_features
                & (VIRTIO_BLK_F_SIZE_MAX | VIRTIO_BLK_F_BLK_SIZE | VIRTIO_BLK_F_FLUSH);
            let mut guest_features_port: Port<u32> = Port::new(base + VIRTIO_PCI_GUEST_FEATURES);
            guest_features_port.write(our_features as u32);

            self.read_only = device_features & VIRTIO_BLK_F_RO != 0;

            // 5. Read capacity from device-specific config
            let mut cap_lo: Port<u32> = Port::new(base + VIRTIO_BLK_CFG_CAPACITY);
            let mut cap_hi: Port<u32> = Port::new(base + VIRTIO_BLK_CFG_CAPACITY + 4);
            let capacity_lo = cap_lo.read() as u64;
            let capacity_hi = cap_hi.read() as u64;
            self.capacity = (capacity_hi << 32) | capacity_lo;

            if device_features & VIRTIO_BLK_F_BLK_SIZE != 0 {
                let mut blk_size_port: Port<u32> = Port::new(base + VIRTIO_BLK_CFG_BLK_SIZE);
                self.block_size = blk_size_port.read();
            }

            serial_println!(
                "[VIRTIO-BLK] Capacity: {} sectors ({} MB)",
                self.capacity,
                self.capacity * 512 / 1024 / 1024
            );
            serial_println!("[VIRTIO-BLK] Block size: {} bytes", self.block_size);
            serial_println!("[VIRTIO-BLK] Read-only: {}", self.read_only);

            // 6. Set up request virtqueue (queue 0)
            let mut queue_select: Port<u16> = Port::new(base + VIRTIO_PCI_QUEUE_SELECT);
            queue_select.write(0);

            let mut queue_size_port: Port<u16> = Port::new(base + VIRTIO_PCI_QUEUE_SIZE);
            let qs = queue_size_port.read();
            serial_println!("[VIRTIO-BLK] Queue size: {}", qs);

            if qs == 0 {
                serial_println!("[VIRTIO-BLK] No queue available");
                status_port.write(crate::virtio_net::VIRTIO_STATUS_FAILED);
                return false;
            }

            // Legacy virtio: the device dictates queue size; the used ring is
            // page-aligned after the descriptor table + available ring.
            self.queue_size = qs;
            let qsz = self.queue_size as usize;
            self.avail_off = 16 * qsz;
            let avail_bytes = 6 + 2 * qsz;
            self.used_off = (self.avail_off + avail_bytes + 4095) & !4095;
            let used_bytes = 6 + 8 * qsz;
            let vq_bytes = self.used_off + used_bytes;
            let vq_pages = vq_bytes.div_ceil(4096);

            let Some(vq_phys) = crate::vmm::allocate_contiguous_frames(vq_pages) else {
                serial_println!(
                    "[VIRTIO-BLK] No DMA frames for virtqueue ({} pages)",
                    vq_pages
                );
                status_port.write(crate::virtio_net::VIRTIO_STATUS_FAILED);
                return false;
            };
            let Some(bounce_phys) = crate::vmm::allocate_physical_frame() else {
                serial_println!("[VIRTIO-BLK] No DMA frame for bounce buffer");
                status_port.write(crate::virtio_net::VIRTIO_STATUS_FAILED);
                return false;
            };
            self.vq_phys = vq_phys;
            self.bounce_phys = bounce_phys;

            let vq_virt = crate::vmm::phys_to_virt(vq_phys);
            let bounce_virt = crate::vmm::phys_to_virt(bounce_phys);
            core::ptr::write_bytes(vq_virt as *mut u8, 0, vq_pages * 4096);
            core::ptr::write_bytes(bounce_virt as *mut u8, 0, 4096);

            serial_println!(
                "[VIRTIO-BLK] Virtqueue layout: qsz={} avail_off={} used_off={} pages={}",
                qsz,
                self.avail_off,
                self.used_off,
                vq_pages
            );

            // 8. Tell the device the virtqueue physical page number
            let mut queue_addr_port: Port<u32> = Port::new(base + VIRTIO_PCI_QUEUE_ADDRESS);
            queue_addr_port.write((vq_phys / 4096) as u32);

            // Initialize avail ring
            core::ptr::write_volatile(self.avail_flags_ptr(), 0u16); // no interrupt suppression
            core::ptr::write_volatile(self.avail_idx_ptr(), 0u16);
            self.avail_idx = 0;
            self.used_idx = 0;

            // 9. Mark driver DRIVER_OK
            status_port.write(
                crate::virtio_net::VIRTIO_STATUS_ACKNOWLEDGE
                    | crate::virtio_net::VIRTIO_STATUS_DRIVER
                    | crate::virtio_net::VIRTIO_STATUS_DRIVER_OK,
            );

            self.ready = true;
            serial_println!("[VIRTIO-BLK] Device initialized: /dev/{}", self.name);
            serial_println!(
                "[VIRTIO-BLK] Virtqueue: {} descriptors, DMA base={:#x}",
                self.queue_size,
                self.vq_phys
            );
            true
        }
    }

    /// Submit a 3-descriptor chain: header → data → status
    /// Returns true if the device completed with VIRTIO_BLK_S_OK.
    fn submit_request(
        &mut self,
        header: &VirtioBlkReqHeader,
        data_buf: *mut u8,
        data_len: u32,
        device_writes_data: bool,
    ) -> bool {
        const HDR_OFF: u64 = 0;
        const DATA_OFF: u64 = 16;
        const STATUS_OFF: u64 = 16 + SECTOR_SIZE as u64;

        let bounce = self.bounce_virt();
        let bounce_phys = self.bounce_phys;

        unsafe {
            core::ptr::copy_nonoverlapping(header.to_bytes().as_ptr(), bounce as *mut u8, 16);
            *(bounce as *mut u8).add(STATUS_OFF as usize) = 0xFF;
            if data_len > 0 && !device_writes_data && !data_buf.is_null() {
                core::ptr::copy_nonoverlapping(
                    data_buf,
                    (bounce as *mut u8).add(DATA_OFF as usize),
                    data_len as usize,
                );
            }
        }
        core::sync::atomic::fence(Ordering::SeqCst);

        // Serialized I/O: descriptors 0 → 1 → 2 (or 0 → 1 when there is no data).
        unsafe {
            let has_data = data_len > 0;
            core::ptr::write_volatile(
                self.desc_ptr(0),
                VirtqDesc {
                    addr: bounce_phys + HDR_OFF,
                    len: 16,
                    flags: VRING_DESC_F_NEXT,
                    next: 1,
                },
            );
            if has_data {
                let data_flags = if device_writes_data {
                    VRING_DESC_F_NEXT | VRING_DESC_F_WRITE
                } else {
                    VRING_DESC_F_NEXT
                };
                core::ptr::write_volatile(
                    self.desc_ptr(1),
                    VirtqDesc {
                        addr: bounce_phys + DATA_OFF,
                        len: data_len,
                        flags: data_flags,
                        next: 2,
                    },
                );
                core::ptr::write_volatile(
                    self.desc_ptr(2),
                    VirtqDesc {
                        addr: bounce_phys + STATUS_OFF,
                        len: 1,
                        flags: VRING_DESC_F_WRITE,
                        next: 0,
                    },
                );
            } else {
                core::ptr::write_volatile(
                    self.desc_ptr(1),
                    VirtqDesc {
                        addr: bounce_phys + STATUS_OFF,
                        len: 1,
                        flags: VRING_DESC_F_WRITE,
                        next: 0,
                    },
                );
            }
        }

        let avail_slot = self.avail_idx % self.queue_size;
        unsafe {
            core::ptr::write_volatile(self.avail_ring_entry(avail_slot), 0u16);
        }
        core::sync::atomic::fence(Ordering::Release);
        self.avail_idx = self.avail_idx.wrapping_add(1);
        unsafe {
            core::ptr::write_volatile(self.avail_idx_ptr(), self.avail_idx);
        }

        unsafe {
            let mut notify: crate::arch_compat::instructions::port::Port<u16> =
                crate::arch_compat::instructions::port::Port::new(
                    self.io_base + VIRTIO_PCI_QUEUE_NOTIFY,
                );
            notify.write(0);
        }

        let mut completed = false;
        for iter in 0..500_000u32 {
            if VIRTIO_BLK_IRQ_PENDING.swap(false, Ordering::AcqRel) {
                let device_used_idx = unsafe { core::ptr::read_volatile(self.used_idx_ptr()) };
                if device_used_idx != self.used_idx {
                    while self.used_idx != device_used_idx {
                        let used_slot = self.used_idx % self.queue_size;
                        let _elem =
                            unsafe { core::ptr::read_volatile(self.used_ring_elem(used_slot)) };
                        self.used_idx = self.used_idx.wrapping_add(1);
                    }
                    completed = true;
                    break;
                }
            }

            if iter % 64 == 0 {
                let isr = unsafe {
                    let mut isr_port: crate::arch_compat::instructions::port::Port<u8> =
                        crate::arch_compat::instructions::port::Port::new(
                            self.io_base + VIRTIO_PCI_ISR_STATUS,
                        );
                    isr_port.read()
                };

                if isr & 1 != 0 {
                    let device_used_idx = unsafe { core::ptr::read_volatile(self.used_idx_ptr()) };
                    if device_used_idx != self.used_idx {
                        while self.used_idx != device_used_idx {
                            let used_slot = self.used_idx % self.queue_size;
                            let _elem =
                                unsafe { core::ptr::read_volatile(self.used_ring_elem(used_slot)) };
                            self.used_idx = self.used_idx.wrapping_add(1);
                        }
                        completed = true;
                        break;
                    }
                }
            }

            let device_used_idx = unsafe { core::ptr::read_volatile(self.used_idx_ptr()) };
            if device_used_idx != self.used_idx {
                while self.used_idx != device_used_idx {
                    let used_slot = self.used_idx % self.queue_size;
                    let _elem = unsafe { core::ptr::read_volatile(self.used_ring_elem(used_slot)) };
                    self.used_idx = self.used_idx.wrapping_add(1);
                }
                completed = true;
                break;
            }

            core::hint::spin_loop();
        }

        if !completed {
            serial_println!("[VIRTIO-BLK] Request timed out");
            self.errors += 1;
            return false;
        }

        let status = unsafe { *(self.bounce_virt() as *const u8).add(STATUS_OFF as usize) };
        if status != VIRTIO_BLK_S_OK {
            serial_println!("[VIRTIO-BLK] Request failed with status={}", status);
            self.errors += 1;
            return false;
        }

        if data_len > 0 && device_writes_data && !data_buf.is_null() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    (self.bounce_virt() as *const u8).add(DATA_OFF as usize),
                    data_buf,
                    data_len as usize,
                );
            }
        }

        true
    }

    /// Read sectors from the device using real virtqueue DMA
    pub fn read_sectors(&mut self, start_sector: u64, count: usize, buffer: &mut [u8]) -> bool {
        if !self.ready {
            return false;
        }
        if start_sector + count as u64 > self.capacity {
            serial_println!(
                "[VIRTIO-BLK] Read out of range: sector {} + {}",
                start_sector,
                count
            );
            return false;
        }
        if buffer.len() < count * SECTOR_SIZE {
            return false;
        }

        for i in 0..count {
            let sector = start_sector + i as u64;
            let buf_offset = i * SECTOR_SIZE;

            let header = VirtioBlkReqHeader::read(sector);
            let data_ptr = buffer[buf_offset..].as_mut_ptr();

            if !self.submit_request(&header, data_ptr, SECTOR_SIZE as u32, true) {
                return false;
            }
        }

        self.reads += count as u64;
        true
    }

    /// Write sectors to the device using real virtqueue DMA
    pub fn write_sectors(&mut self, start_sector: u64, count: usize, data: &[u8]) -> bool {
        if !self.ready || self.read_only {
            return false;
        }
        if start_sector + count as u64 > self.capacity {
            return false;
        }
        if data.len() < count * SECTOR_SIZE {
            return false;
        }

        for i in 0..count {
            let sector = start_sector + i as u64;
            let data_offset = i * SECTOR_SIZE;

            let header = VirtioBlkReqHeader::write(sector);
            let data_ptr = data[data_offset..].as_ptr() as *mut u8;

            if !self.submit_request(&header, data_ptr, SECTOR_SIZE as u32, false) {
                return false;
            }
        }

        self.writes += count as u64;
        true
    }

    /// Flush device buffers to disk
    pub fn flush(&mut self) -> bool {
        if !self.ready {
            return false;
        }

        let header = VirtioBlkReqHeader::flush();
        self.submit_request(&header, core::ptr::null_mut(), 0, false)
    }

    /// Get device info string
    pub fn info(&self) -> String {
        alloc::format!(
            "/dev/{}: {} MB, {} sectors, block_size={}, ro={}, reads={}, writes={}, errors={}",
            self.name,
            self.capacity * 512 / 1024 / 1024,
            self.capacity,
            self.block_size,
            self.read_only,
            self.reads,
            self.writes,
            self.errors,
        )
    }
}

// ─── Global Device ──────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref VIRTIO_BLK: Mutex<Option<VirtioBlkDevice>> = Mutex::new(None);
}

static BLK_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Atomic flag set by the IRQ handler to signal I/O completion.
/// Polled by submit_request instead of spinning on the ISR port.
static VIRTIO_BLK_IRQ_PENDING: AtomicBool = AtomicBool::new(false);

/// Handle a virtio-blk interrupt from the IDT handler.
/// Called from `interrupts::virtio_irq_handler` on shared PCI IRQ lines.
/// We try_lock to avoid deadlocking if the main thread holds VIRTIO_BLK.
pub fn handle_interrupt() {
    // Try to acquire the device without blocking (we're in interrupt context)
    if let Some(ref mut guard) = VIRTIO_BLK.try_lock() {
        if let Some(ref dev) = **guard {
            // Read the ISR register to acknowledge the interrupt.
            // Bit 0 = used buffer notification, Bit 1 = config change.
            let isr: u8 = unsafe {
                let mut isr_port: crate::arch_compat::instructions::port::Port<u8> =
                    crate::arch_compat::instructions::port::Port::new(
                        dev.io_base + VIRTIO_PCI_ISR_STATUS,
                    );
                isr_port.read()
            };
            if isr & 1 != 0 {
                // Signal that there's a completion waiting
                VIRTIO_BLK_IRQ_PENDING.store(true, Ordering::Release);
            }
        }
    } else {
        // Can't acquire lock — set the flag anyway so the holder sees it
        VIRTIO_BLK_IRQ_PENDING.store(true, Ordering::Release);
    }
}

/// Check and clear the IRQ pending flag (used by polling loops)
pub fn check_irq_pending() -> bool {
    VIRTIO_BLK_IRQ_PENDING.swap(false, Ordering::AcqRel)
}

/// Check if a block device is available
pub fn is_available() -> bool {
    BLK_AVAILABLE.load(Ordering::Relaxed)
}

/// Read sectors from the virtio block device
pub fn read(start_sector: u64, count: usize, buffer: &mut [u8]) -> bool {
    if let Some(ref mut dev) = *VIRTIO_BLK.lock() {
        dev.read_sectors(start_sector, count, buffer)
    } else {
        false
    }
}

/// Write sectors to the virtio block device
pub fn write(start_sector: u64, count: usize, data: &[u8]) -> bool {
    if let Some(ref mut dev) = *VIRTIO_BLK.lock() {
        dev.write_sectors(start_sector, count, data)
    } else {
        false
    }
}

/// Flush the device
pub fn flush() -> bool {
    if let Some(ref mut dev) = *VIRTIO_BLK.lock() {
        dev.flush()
    } else {
        false
    }
}

/// Get capacity in sectors
pub fn capacity() -> u64 {
    VIRTIO_BLK.lock().as_ref().map(|d| d.capacity).unwrap_or(0)
}

/// Get device info
pub fn info() -> Option<String> {
    VIRTIO_BLK.lock().as_ref().map(|d| d.info())
}

/// Register the virtio-blk device with the block layer
fn register_with_block_layer(capacity: u64, read_only: bool) {
    let mut devices = crate::block::BLOCK_DEVICES.lock();
    devices.push(crate::block::BlockDevice {
        name: String::from("vda"),
        device_type: crate::block::BlockDeviceType::Virtio,
        block_size: SECTOR_SIZE,
        total_blocks: capacity,
        read_only,
        model: String::from("QEMU Virtio Block Device"),
        serial: String::from("VIRTIO-BLK-0"),
    });
    serial_println!("[VIRTIO-BLK] Registered /dev/vda with block layer");
}

/// Initialize virtio-blk driver
pub fn init() {
    serial_println!("[VIRTIO-BLK] Scanning PCI bus for block devices...");

    // Scan PCI bus for virtio-blk devices
    let pci_devices = crate::virtio_net::scan_pci_bus();

    for pci_dev in &pci_devices {
        if pci_dev.vendor_id == crate::virtio_net::VIRTIO_PCI_VENDOR {
            // Check subsystem device ID for block device
            let subsystem = unsafe {
                let address = 0x8000_0000u32
                    | ((pci_dev.bus as u32) << 16)
                    | ((pci_dev.device as u32) << 11)
                    | ((pci_dev.function as u32) << 8)
                    | 0x2C;

                let mut addr_port: crate::arch_compat::instructions::port::Port<u32> =
                    crate::arch_compat::instructions::port::Port::new(0xCF8);
                let mut data_port: crate::arch_compat::instructions::port::Port<u32> =
                    crate::arch_compat::instructions::port::Port::new(0xCFC);

                addr_port.write(address);
                (data_port.read() >> 16) as u16
            };

            if subsystem == VIRTIO_BLK_SUBSYSTEM {
                serial_println!(
                    "[VIRTIO-BLK] Found virtio-blk device at {:02x}:{:02x}.{}",
                    pci_dev.bus,
                    pci_dev.device,
                    pci_dev.function
                );

                let io_base = (pci_dev.bar0 & 0xFFFC) as u16;

                // Enable I/O and bus mastering (critical for DMA!)
                unsafe {
                    let cmd_addr = 0x8000_0000u32
                        | ((pci_dev.bus as u32) << 16)
                        | ((pci_dev.device as u32) << 11)
                        | ((pci_dev.function as u32) << 8)
                        | 0x04;

                    let mut addr_port: crate::arch_compat::instructions::port::Port<u32> =
                        crate::arch_compat::instructions::port::Port::new(0xCF8);
                    let mut data_port: crate::arch_compat::instructions::port::Port<u32> =
                        crate::arch_compat::instructions::port::Port::new(0xCFC);

                    addr_port.write(cmd_addr);
                    let cmd = data_port.read();
                    addr_port.write(cmd_addr);
                    data_port.write(cmd | 0x07); // IO + Memory + Bus Master
                }

                let mut dev = VirtioBlkDevice::new(
                    io_base,
                    pci_dev.bus,
                    pci_dev.device,
                    pci_dev.function,
                    pci_dev.irq,
                );

                if dev.init_legacy() {
                    let cap = dev.capacity;
                    let ro = dev.read_only;
                    BLK_AVAILABLE.store(true, Ordering::Relaxed);
                    *VIRTIO_BLK.lock() = Some(dev);
                    register_with_block_layer(cap, ro);
                    return;
                }
            }
        }
    }

    serial_println!("[VIRTIO-BLK] No virtio-blk device found");
    serial_println!("[VIRTIO-BLK] Block I/O using ramdisk fallback");
}
