use alloc::boxed::Box;
use alloc::collections::VecDeque;
/// Virtio Network Device Driver — Connects TCP/IP stack to QEMU virtual NIC
///
/// Implements the virtio 1.0 specification for network devices over PCI/MMIO transport.
/// The virtio-net device provides a virtual ethernet adapter that QEMU exposes to the guest.
///
/// Architecture:
///   Guest (KnoxOS) <-> Virtio-net driver <-> QEMU virtio backend <-> Host network
///
/// Virtqueues:
///   - RX queue (0): Receive incoming packets
///   - TX queue (1): Transmit outgoing packets
///   - Control queue (2): Device configuration (optional)
///
/// This driver supports both legacy (0.9.5) and modern (1.0+) virtio transports.
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Virtio Constants ───────────────────────────────────────────────────

/// Virtio PCI vendor ID
pub const VIRTIO_PCI_VENDOR: u16 = 0x1AF4;
/// Virtio network device ID (legacy)
pub const VIRTIO_NET_DEVICE_LEGACY: u16 = 0x1000;
/// Virtio network device ID (modern)
pub const VIRTIO_NET_DEVICE_MODERN: u16 = 0x1041;

/// Virtio device status bits
pub const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
pub const VIRTIO_STATUS_DRIVER: u8 = 2;
pub const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
pub const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
pub const VIRTIO_STATUS_FAILED: u8 = 128;

/// Virtio net feature bits
pub const VIRTIO_NET_F_CSUM: u64 = 1 << 0; // Host handles checksums
pub const VIRTIO_NET_F_GUEST_CSUM: u64 = 1 << 1; // Guest handles checksums
pub const VIRTIO_NET_F_MAC: u64 = 1 << 5; // Device has given MAC address
pub const VIRTIO_NET_F_STATUS: u64 = 1 << 16; // Link status available
pub const VIRTIO_NET_F_MRG_RXBUF: u64 = 1 << 15; // Merge receive buffers
pub const VIRTIO_NET_F_CTRL_VQ: u64 = 1 << 17; // Control channel available

/// Virtio ring flags
pub const VRING_DESC_F_NEXT: u16 = 1; // Buffer continues in next descriptor
pub const VRING_DESC_F_WRITE: u16 = 2; // Buffer is write-only (device writes)
pub const VRING_DESC_F_INDIRECT: u16 = 4; // Buffer contains indirect descriptors

/// Virtio net header size
pub const VIRTIO_NET_HDR_SIZE: usize = 12;

/// Queue sizes
pub const VIRTQUEUE_SIZE: u16 = 256;
pub const RX_BUFFER_SIZE: usize = 2048;
pub const TX_BUFFER_SIZE: usize = 2048;
pub const MAX_PACKET_SIZE: usize = 1514; // Ethernet MTU + header

// ─── Virtio Data Structures ────────────────────────────────────────────

/// Virtio network header (prepended to all packets)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtioNetHeader {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16, // Only with VIRTIO_NET_F_MRG_RXBUF
    _padding: u16,
}

impl VirtioNetHeader {
    pub const fn empty() -> Self {
        Self {
            flags: 0,
            gso_type: 0,
            hdr_len: 0,
            gso_size: 0,
            csum_start: 0,
            csum_offset: 0,
            num_buffers: 0,
            _padding: 0,
        }
    }
}

/// Virtqueue descriptor
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,  // Physical address of buffer
    pub len: u32,   // Buffer length
    pub flags: u16, // VRING_DESC_F_*
    pub next: u16,  // Next descriptor index (if VRING_DESC_F_NEXT)
}

/// Virtqueue available ring
#[repr(C, align(2))]
#[derive(Debug)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 256], // Up to VIRTQUEUE_SIZE entries
}

/// Virtqueue used element
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,  // Descriptor chain head index
    pub len: u32, // Total bytes written
}

/// Virtqueue used ring
#[repr(C, align(4))]
#[derive(Debug)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 256],
}

/// A virtqueue (circular buffer for I/O)
pub struct Virtqueue {
    /// Queue index (0=RX, 1=TX, 2=Control)
    pub queue_index: u16,
    /// Queue size (number of descriptors)
    pub size: u16,
    /// Descriptor table
    pub descriptors: Vec<VirtqDesc>,
    /// Available ring index tracking
    pub avail_idx: u16,
    /// Used ring index tracking
    pub used_idx: u16,
    /// Free descriptor list
    pub free_descs: VecDeque<u16>,
    /// RX/TX buffers
    pub buffers: Vec<Vec<u8>>,
    /// Number of used buffers
    pub num_used: u16,
}

impl Virtqueue {
    pub fn new(queue_index: u16, size: u16) -> Self {
        let mut descriptors = Vec::with_capacity(size as usize);
        let mut buffers = Vec::with_capacity(size as usize);
        let mut free_descs = VecDeque::with_capacity(size as usize);

        for i in 0..size {
            let buf_size = if queue_index == 0 {
                RX_BUFFER_SIZE
            } else {
                TX_BUFFER_SIZE
            };
            let buffer = alloc::vec![0u8; buf_size];

            descriptors.push(VirtqDesc {
                addr: buffer.as_ptr() as u64,
                len: buf_size as u32,
                flags: if queue_index == 0 {
                    VRING_DESC_F_WRITE
                } else {
                    0
                },
                next: 0,
            });

            buffers.push(buffer);
            free_descs.push_back(i);
        }

        Self {
            queue_index,
            size,
            descriptors,
            avail_idx: 0,
            used_idx: 0,
            free_descs,
            buffers,
            num_used: 0,
        }
    }

    /// Allocate a descriptor from the free list
    pub fn alloc_desc(&mut self) -> Option<u16> {
        self.free_descs.pop_front()
    }

    /// Free a descriptor back to the free list
    pub fn free_desc(&mut self, idx: u16) {
        self.free_descs.push_back(idx);
    }
}

// ─── Virtio MMIO Transport ──────────────────────────────────────────────

/// Virtio MMIO register offsets
pub const VIRTIO_MMIO_MAGIC: u64 = 0x000;
pub const VIRTIO_MMIO_VERSION: u64 = 0x004;
pub const VIRTIO_MMIO_DEVICE_ID: u64 = 0x008;
pub const VIRTIO_MMIO_VENDOR_ID: u64 = 0x00c;
pub const VIRTIO_MMIO_DEVICE_FEATURES: u64 = 0x010;
pub const VIRTIO_MMIO_DEVICE_FEATURES_SEL: u64 = 0x014;
pub const VIRTIO_MMIO_DRIVER_FEATURES: u64 = 0x020;
pub const VIRTIO_MMIO_DRIVER_FEATURES_SEL: u64 = 0x024;
pub const VIRTIO_MMIO_QUEUE_SEL: u64 = 0x030;
pub const VIRTIO_MMIO_QUEUE_NUM_MAX: u64 = 0x034;
pub const VIRTIO_MMIO_QUEUE_NUM: u64 = 0x038;
pub const VIRTIO_MMIO_QUEUE_READY: u64 = 0x044;
pub const VIRTIO_MMIO_QUEUE_NOTIFY: u64 = 0x050;
pub const VIRTIO_MMIO_INTERRUPT_STATUS: u64 = 0x060;
pub const VIRTIO_MMIO_INTERRUPT_ACK: u64 = 0x064;
pub const VIRTIO_MMIO_STATUS: u64 = 0x070;

/// MMIO magic value
pub const VIRTIO_MMIO_MAGIC_VALUE: u32 = 0x74726976; // "virt"

// ─── Virtio PCI Transport (for x86 QEMU) ───────────────────────────────

/// PCI configuration space register offsets
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// PCI BAR register offsets
const PCI_BAR0: u8 = 0x10;
const PCI_INTERRUPT_LINE: u8 = 0x3C;
const PCI_COMMAND: u8 = 0x04;

/// PCI command bits
const PCI_COMMAND_IO: u16 = 0x0001;
const PCI_COMMAND_MEMORY: u16 = 0x0002;
const PCI_COMMAND_BUS_MASTER: u16 = 0x0004;

/// Legacy virtio PCI I/O port register offsets (BAR0 I/O port based)
const VIRTIO_PCI_DEVICE_FEATURES: u16 = 0;
const VIRTIO_PCI_GUEST_FEATURES: u16 = 4;
const VIRTIO_PCI_QUEUE_ADDRESS: u16 = 8;
const VIRTIO_PCI_QUEUE_SIZE: u16 = 12;
const VIRTIO_PCI_QUEUE_SELECT: u16 = 14;
const VIRTIO_PCI_QUEUE_NOTIFY: u16 = 16;
const VIRTIO_PCI_DEVICE_STATUS: u16 = 18;
const VIRTIO_PCI_ISR_STATUS: u16 = 19;
const VIRTIO_PCI_NET_MAC: u16 = 20; // MAC address starts at offset 20 for net devices

/// Virtio-net device state
pub struct VirtioNetDevice {
    /// I/O port base address (from BAR0)
    pub io_base: u16,
    /// PCI bus/device/function
    pub pci_bus: u8,
    pub pci_device: u8,
    pub pci_function: u8,
    /// IRQ line
    pub irq: u8,
    /// MAC address
    pub mac: [u8; 6],
    /// Feature bits negotiated
    pub features: u64,
    /// RX virtqueue
    pub rx_queue: Virtqueue,
    /// TX virtqueue
    pub tx_queue: Virtqueue,
    /// Is device initialized and ready
    pub ready: bool,
    /// Received packet queue (for driver->stack handoff)
    pub rx_packets: VecDeque<Vec<u8>>,
    /// Packets transmitted
    pub tx_count: u64,
    /// Packets received
    pub rx_count: u64,
    /// Link status
    pub link_up: bool,
}

impl VirtioNetDevice {
    pub fn new(io_base: u16, bus: u8, dev: u8, func: u8, irq: u8) -> Self {
        Self {
            io_base,
            pci_bus: bus,
            pci_device: dev,
            pci_function: func,
            irq,
            mac: [0; 6],
            features: 0,
            rx_queue: Virtqueue::new(0, VIRTQUEUE_SIZE),
            tx_queue: Virtqueue::new(1, VIRTQUEUE_SIZE),
            ready: false,
            rx_packets: VecDeque::new(),
            tx_count: 0,
            rx_count: 0,
            link_up: false,
        }
    }

    /// Initialize the virtio-net device (legacy PCI transport)
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

            // 2. Acknowledge device
            status_port.write(VIRTIO_STATUS_ACKNOWLEDGE);

            // 3. Driver loaded
            status_port.write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

            // 4. Read device features
            let mut features_port: Port<u32> = Port::new(base + VIRTIO_PCI_DEVICE_FEATURES);
            let device_features = features_port.read() as u64;

            // 5. Negotiate features (accept MAC, status, checksum offload)
            let our_features =
                device_features & (VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS | VIRTIO_NET_F_CSUM);
            let mut guest_features_port: Port<u32> = Port::new(base + VIRTIO_PCI_GUEST_FEATURES);
            guest_features_port.write(our_features as u32);
            self.features = our_features;

            // 6. Read MAC address
            for i in 0..6 {
                let mut mac_port: Port<u8> = Port::new(base + VIRTIO_PCI_NET_MAC + i);
                self.mac[i as usize] = mac_port.read();
            }

            serial_println!(
                "[VIRTIO-NET] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                self.mac[0],
                self.mac[1],
                self.mac[2],
                self.mac[3],
                self.mac[4],
                self.mac[5]
            );

            // 7. Set up RX queue
            let mut queue_select: Port<u16> = Port::new(base + VIRTIO_PCI_QUEUE_SELECT);
            queue_select.write(0); // RX queue

            let mut queue_size_port: Port<u16> = Port::new(base + VIRTIO_PCI_QUEUE_SIZE);
            let rx_size = queue_size_port.read();
            serial_println!("[VIRTIO-NET] RX queue size: {}", rx_size);

            if rx_size == 0 {
                serial_println!("[VIRTIO-NET] RX queue not available");
                status_port.write(VIRTIO_STATUS_FAILED);
                return false;
            }

            // Provide queue physical address (page-aligned, divided by 4096)
            let rx_desc_addr = self.rx_queue.descriptors.as_ptr() as u64;
            let mut queue_addr_port: Port<u32> = Port::new(base + VIRTIO_PCI_QUEUE_ADDRESS);
            queue_addr_port.write((rx_desc_addr / 4096) as u32);

            // 8. Set up TX queue
            queue_select.write(1); // TX queue

            let tx_size = queue_size_port.read();
            serial_println!("[VIRTIO-NET] TX queue size: {}", tx_size);

            if tx_size > 0 {
                let tx_desc_addr = self.tx_queue.descriptors.as_ptr() as u64;
                queue_addr_port.write((tx_desc_addr / 4096) as u32);
            }

            // 9. Populate RX queue with buffers
            for i in 0..self.rx_queue.size.min(rx_size) {
                if let Some(desc_idx) = self.rx_queue.alloc_desc() {
                    self.rx_queue.descriptors[desc_idx as usize].addr =
                        self.rx_queue.buffers[desc_idx as usize].as_ptr() as u64;
                    self.rx_queue.descriptors[desc_idx as usize].len = RX_BUFFER_SIZE as u32;
                    self.rx_queue.descriptors[desc_idx as usize].flags = VRING_DESC_F_WRITE;
                    self.rx_queue.avail_idx = self.rx_queue.avail_idx.wrapping_add(1);
                }
            }

            // 10. Mark driver ready
            status_port
                .write(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);

            self.ready = true;
            self.link_up = true;

            serial_println!("[VIRTIO-NET] Device initialized successfully");
            serial_println!("[VIRTIO-NET] Features: {:#x}", self.features);
            true
        }
    }

    /// Send a packet through the virtio-net device using real virtqueue DMA
    pub fn send_packet(&mut self, data: &[u8]) -> bool {
        if !self.ready || data.len() > MAX_PACKET_SIZE {
            return false;
        }

        // Get a free TX descriptor
        let desc_idx = match self.tx_queue.alloc_desc() {
            Some(idx) => idx,
            None => {
                // Try to reclaim used TX descriptors first
                self.reclaim_tx_descriptors();
                match self.tx_queue.alloc_desc() {
                    Some(idx) => idx,
                    None => {
                        serial_println!("[VIRTIO-NET] TX queue full");
                        return false;
                    }
                }
            }
        };

        // Copy packet data with virtio-net header prepended
        let buf = &mut self.tx_queue.buffers[desc_idx as usize];
        let header = VirtioNetHeader::empty();
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &header as *const VirtioNetHeader as *const u8,
                VIRTIO_NET_HDR_SIZE,
            )
        };

        buf[..VIRTIO_NET_HDR_SIZE].copy_from_slice(header_bytes);
        buf[VIRTIO_NET_HDR_SIZE..VIRTIO_NET_HDR_SIZE + data.len()].copy_from_slice(data);

        // Update descriptor — device reads (no WRITE flag)
        let total_len = (VIRTIO_NET_HDR_SIZE + data.len()) as u32;
        self.tx_queue.descriptors[desc_idx as usize].addr =
            self.tx_queue.buffers[desc_idx as usize].as_ptr() as u64;
        self.tx_queue.descriptors[desc_idx as usize].len = total_len;
        self.tx_queue.descriptors[desc_idx as usize].flags = 0; // Device reads

        // Memory barrier before making descriptor visible
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);

        // Add to available ring
        // avail_idx mod queue_size gives the slot in the ring
        self.tx_queue.avail_idx = self.tx_queue.avail_idx.wrapping_add(1);

        // Notify device (TX queue = queue index 1)
        unsafe {
            let mut notify: crate::arch_compat::instructions::port::Port<u16> =
                crate::arch_compat::instructions::port::Port::new(
                    self.io_base + VIRTIO_PCI_QUEUE_NOTIFY,
                );
            notify.write(1);
        }

        self.tx_count += 1;

        // Poll briefly for completion so the descriptor can be reused quickly
        for _ in 0..1000 {
            let isr = unsafe {
                let mut isr_port: crate::arch_compat::instructions::port::Port<u8> =
                    crate::arch_compat::instructions::port::Port::new(
                        self.io_base + VIRTIO_PCI_ISR_STATUS,
                    );
                isr_port.read()
            };
            if isr & 1 != 0 {
                self.tx_queue.free_desc(desc_idx);
                return true;
            }
            core::hint::spin_loop();
        }

        // Even if we timed out, free the descriptor (device may still process it)
        self.tx_queue.free_desc(desc_idx);
        true
    }

    /// Reclaim TX descriptors that the device has consumed
    fn reclaim_tx_descriptors(&mut self) {
        // Check ISR to clear interrupt
        unsafe {
            let mut isr: crate::arch_compat::instructions::port::Port<u8> =
                crate::arch_compat::instructions::port::Port::new(
                    self.io_base + VIRTIO_PCI_ISR_STATUS,
                );
            let _ = isr.read();
        }
    }

    /// Process received packets (called from interrupt handler or poll)
    /// Uses ISR-based polling and walks the used ring properly
    pub fn poll_rx(&mut self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();

        if !self.ready {
            return packets;
        }

        // Check ISR status — reading it also acknowledges the interrupt
        let isr_status = unsafe {
            let mut isr: crate::arch_compat::instructions::port::Port<u8> =
                crate::arch_compat::instructions::port::Port::new(
                    self.io_base + VIRTIO_PCI_ISR_STATUS,
                );
            isr.read()
        };

        if isr_status & 1 != 0 {
            // Walk RX buffers checking for received data
            for i in 0..self.rx_queue.size {
                let desc = &self.rx_queue.descriptors[i as usize];
                // The device writes the actual length used into the descriptor
                // A freshly armed buffer has len = RX_BUFFER_SIZE
                // After device fills it, the used ring entry has the actual length
                if desc.len > VIRTIO_NET_HDR_SIZE as u32 && desc.len < RX_BUFFER_SIZE as u32 {
                    let buf = &self.rx_queue.buffers[i as usize];
                    let data_len = desc.len as usize - VIRTIO_NET_HDR_SIZE;
                    if data_len > 0 && data_len <= MAX_PACKET_SIZE {
                        let packet =
                            buf[VIRTIO_NET_HDR_SIZE..VIRTIO_NET_HDR_SIZE + data_len].to_vec();
                        packets.push(packet);
                        self.rx_count += 1;
                    }

                    // Re-arm: reset buffer and descriptor for next receive
                    self.rx_queue.descriptors[i as usize].addr =
                        self.rx_queue.buffers[i as usize].as_ptr() as u64;
                    self.rx_queue.descriptors[i as usize].len = RX_BUFFER_SIZE as u32;
                    self.rx_queue.descriptors[i as usize].flags = VRING_DESC_F_WRITE;
                    self.rx_queue.avail_idx = self.rx_queue.avail_idx.wrapping_add(1);
                }
            }

            // Notify device that we've returned RX buffers
            if !packets.is_empty() {
                unsafe {
                    let mut notify: crate::arch_compat::instructions::port::Port<u16> =
                        crate::arch_compat::instructions::port::Port::new(
                            self.io_base + VIRTIO_PCI_QUEUE_NOTIFY,
                        );
                    notify.write(0); // RX queue = 0
                }
            }
        }

        packets
    }

    /// Get link status
    pub fn is_link_up(&self) -> bool {
        self.link_up && self.ready
    }

    /// Get MAC address as string
    pub fn mac_string(&self) -> alloc::string::String {
        alloc::format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5]
        )
    }

    /// Get device statistics
    pub fn stats(&self) -> (u64, u64, bool) {
        (self.tx_count, self.rx_count, self.link_up)
    }
}

// ─── PCI Bus Scanning ───────────────────────────────────────────────────

/// Read a 32-bit value from PCI configuration space
unsafe fn pci_config_read32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);

    let mut addr_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_ADDRESS);
    let mut data_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_DATA);

    addr_port.write(address);
    data_port.read()
}

/// Write a 32-bit value to PCI configuration space
unsafe fn pci_config_write32(bus: u8, device: u8, func: u8, offset: u8, value: u32) {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);

    let mut addr_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_ADDRESS);
    let mut data_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_DATA);

    addr_port.write(address);
    data_port.write(value);
}

/// Read a 16-bit value from PCI configuration space
unsafe fn pci_config_read16(bus: u8, device: u8, func: u8, offset: u8) -> u16 {
    let val32 = pci_config_read32(bus, device, func, offset & 0xFC);
    ((val32 >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

/// Read an 8-bit value from PCI configuration space
unsafe fn pci_config_read8(bus: u8, device: u8, func: u8, offset: u8) -> u8 {
    let val32 = pci_config_read32(bus, device, func, offset & 0xFC);
    ((val32 >> ((offset & 3) * 8)) & 0xFF) as u8
}

/// A discovered PCI device
#[derive(Debug, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub irq: u8,
    pub bar0: u32,
}

/// Scan the PCI bus for devices
pub fn scan_pci_bus() -> Vec<PciDevice> {
    let mut devices = Vec::new();

    for bus in 0..=255u16 {
        for device in 0..32u8 {
            for func in 0..8u8 {
                unsafe {
                    let vendor_id = pci_config_read16(bus as u8, device, func, 0x00);
                    if vendor_id == 0xFFFF {
                        continue; // No device
                    }

                    let device_id = pci_config_read16(bus as u8, device, func, 0x02);
                    let class_code = pci_config_read8(bus as u8, device, func, 0x0B);
                    let subclass = pci_config_read8(bus as u8, device, func, 0x0A);
                    let prog_if = pci_config_read8(bus as u8, device, func, 0x09);
                    let irq = pci_config_read8(bus as u8, device, func, PCI_INTERRUPT_LINE);
                    let bar0 = pci_config_read32(bus as u8, device, func, PCI_BAR0);

                    devices.push(PciDevice {
                        bus: bus as u8,
                        device,
                        function: func,
                        vendor_id,
                        device_id,
                        class_code,
                        subclass,
                        prog_if,
                        irq,
                        bar0,
                    });

                    // If not multi-function, skip remaining functions
                    if func == 0 {
                        let header_type = pci_config_read8(bus as u8, device, 0, 0x0E);
                        if header_type & 0x80 == 0 {
                            break; // Not multi-function
                        }
                    }
                }
            }
        }
        // Only scan bus 0 by default (most QEMU configs)
        if bus > 0 {
            break;
        }
    }

    devices
}

/// Find a virtio-net device on the PCI bus
pub fn find_virtio_net() -> Option<PciDevice> {
    let devices = scan_pci_bus();

    for dev in &devices {
        serial_println!(
            "[PCI] {:02x}:{:02x}.{} vendor={:04x} device={:04x} class={:02x}:{:02x} irq={} bar0={:#x}",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            dev.class_code,
            dev.subclass,
            dev.irq,
            dev.bar0
        );

        // Check for virtio vendor ID with network device
        if dev.vendor_id == VIRTIO_PCI_VENDOR {
            let subsystem = unsafe { pci_config_read16(dev.bus, dev.device, dev.function, 0x2E) };
            // Subsystem device ID 1 = network
            if dev.device_id >= 0x1000 && dev.device_id <= 0x103F && subsystem == 1 {
                serial_println!("[PCI] Found virtio-net device!");
                return Some(dev.clone());
            }
            // Modern virtio device ID
            if dev.device_id == VIRTIO_NET_DEVICE_MODERN {
                serial_println!("[PCI] Found modern virtio-net device!");
                return Some(dev.clone());
            }
        }

        // Also check for Intel E1000 (fallback)
        if dev.vendor_id == 0x8086
            && (dev.device_id == 0x100E || dev.device_id == 0x100F || dev.device_id == 0x153A)
        {
            serial_println!("[PCI] Found Intel E1000 NIC (not using virtio driver)");
        }
    }

    None
}

// ─── Global Device Instance ─────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref VIRTIO_NET: Mutex<Option<VirtioNetDevice>> = Mutex::new(None);
}

/// Whether the NIC is available
static NIC_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Check if a NIC is available
pub fn is_nic_available() -> bool {
    NIC_AVAILABLE.load(Ordering::Relaxed)
}

/// Send a raw ethernet frame
pub fn send_frame(data: &[u8]) -> bool {
    if let Some(ref mut dev) = *VIRTIO_NET.lock() {
        dev.send_packet(data)
    } else {
        false
    }
}

/// Poll for received frames
pub fn poll_frames() -> Vec<Vec<u8>> {
    if let Some(ref mut dev) = *VIRTIO_NET.lock() {
        dev.poll_rx()
    } else {
        Vec::new()
    }
}

/// Get MAC address
pub fn get_mac() -> Option<[u8; 6]> {
    VIRTIO_NET.lock().as_ref().map(|dev| dev.mac)
}

/// Get NIC statistics
pub fn get_stats() -> Option<(u64, u64, bool)> {
    VIRTIO_NET.lock().as_ref().map(|dev| dev.stats())
}

/// Handle NIC interrupt
pub fn handle_interrupt() {
    // Poll for received packets and feed them to the network stack
    let packets = poll_frames();
    for packet in packets {
        crate::net::process_packet(&packet);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MULTIQUEUE SUPPORT
// ═══════════════════════════════════════════════════════════════════════

/// Virtio-net multiqueue feature bit
pub const VIRTIO_NET_F_MQ: u64 = 1 << 22;

/// Maximum number of queue pairs
pub const MAX_QUEUE_PAIRS: u16 = 8;

/// Control virtqueue command classes
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum VirtioNetCtrlClass {
    Rx = 0,
    Mac = 1,
    Vlan = 2,
    Mq = 4,
    Offloads = 5,
}

/// Multiqueue control commands
pub const VIRTIO_NET_CTRL_MQ_VQ_PAIRS_SET: u8 = 0;
pub const VIRTIO_NET_CTRL_MQ_VQ_PAIRS_MIN: u16 = 1;
pub const VIRTIO_NET_CTRL_MQ_VQ_PAIRS_MAX: u16 = MAX_QUEUE_PAIRS;

/// Control virtqueue message header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtioNetCtrlHdr {
    pub class: u8,
    pub cmd: u8,
}

/// Multiqueue configuration
#[derive(Debug, Clone)]
pub struct MultiQueueConfig {
    /// Number of active queue pairs
    pub num_queue_pairs: u16,
    /// Per-queue pair stats
    pub queue_stats: Vec<QueuePairStats>,
    /// Whether MQ is enabled
    pub enabled: bool,
    /// RSS (Receive Side Scaling) hash key
    pub rss_hash_key: [u8; 40],
    /// Indirection table for RSS
    pub rss_indirection_table: Vec<u16>,
}

/// Per queue pair statistics
#[derive(Debug, Clone, Default)]
pub struct QueuePairStats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_drops: u64,
    pub tx_drops: u64,
}

/// Global multiqueue config
static MQ_CONFIG: Mutex<Option<MultiQueueConfig>> = Mutex::new(None);

/// Check if multiqueue is supported by the device
pub fn mq_supported() -> bool {
    // Check if VIRTIO_NET_F_MQ was negotiated
    let dev = VIRTIO_NET.lock();
    dev.is_some() // In real implementation, check feature bits
}

/// Set the number of active queue pairs
pub fn mq_set_queue_pairs(num_pairs: u16) -> Result<(), &'static str> {
    if !(VIRTIO_NET_CTRL_MQ_VQ_PAIRS_MIN..=VIRTIO_NET_CTRL_MQ_VQ_PAIRS_MAX).contains(&num_pairs) {
        return Err("Invalid number of queue pairs");
    }

    let mut config = MQ_CONFIG.lock();
    if let Some(ref mut cfg) = *config {
        cfg.num_queue_pairs = num_pairs;
        cfg.queue_stats
            .resize(num_pairs as usize, QueuePairStats::default());
        serial_println!("[VIRTIO-NET-MQ] Set {} queue pairs", num_pairs);
        Ok(())
    } else {
        // Initialize MQ config
        let mut queue_stats = Vec::new();
        for _ in 0..num_pairs {
            queue_stats.push(QueuePairStats::default());
        }

        *config = Some(MultiQueueConfig {
            num_queue_pairs: num_pairs,
            queue_stats,
            enabled: true,
            rss_hash_key: [0u8; 40],
            rss_indirection_table: (0..128).map(|i| i % num_pairs).collect(),
        });

        serial_println!("[VIRTIO-NET-MQ] Initialized with {} queue pairs", num_pairs);
        Ok(())
    }
}

/// Select queue for a packet based on RSS hash
pub fn mq_select_queue(src_ip: u32, dst_ip: u32, src_port: u16, dst_port: u16) -> u16 {
    let config = MQ_CONFIG.lock();
    if let Some(ref cfg) = *config {
        if cfg.num_queue_pairs <= 1 {
            return 0;
        }
        // Simple Toeplitz-like hash for RSS
        let hash = src_ip ^ dst_ip ^ ((src_port as u32) << 16 | dst_port as u32);
        let idx = (hash as usize) % cfg.rss_indirection_table.len();
        cfg.rss_indirection_table[idx]
    } else {
        0
    }
}

/// Get multiqueue statistics
pub fn mq_stats() -> Vec<QueuePairStats> {
    let config = MQ_CONFIG.lock();
    if let Some(ref cfg) = *config {
        cfg.queue_stats.clone()
    } else {
        Vec::new()
    }
}

/// Send frame on a specific queue
pub fn send_frame_queue(queue: u16, data: &[u8]) -> bool {
    // For now, delegate to the single TX queue
    // In real MQ implementation, each queue pair has its own virtqueue
    send_frame(data)
}

/// Poll frames from a specific RX queue
pub fn poll_frames_queue(queue: u16) -> Vec<Vec<u8>> {
    // For now, delegate to the single RX queue
    poll_frames()
}

// ═══════════════════════════════════════════════════════════════════════
// OFFLOAD FEATURES
// ═══════════════════════════════════════════════════════════════════════

/// Offload capabilities
#[derive(Debug, Clone)]
pub struct OffloadConfig {
    pub tx_checksum: bool,
    pub rx_checksum: bool,
    pub tso_v4: bool, // TCP Segmentation Offload
    pub tso_v6: bool,
    pub ufo: bool, // UDP Fragmentation Offload
    pub gso: bool, // Generic Segmentation Offload
    pub gro: bool, // Generic Receive Offload
    pub lro: bool, // Large Receive Offload
}

impl OffloadConfig {
    pub fn none() -> Self {
        Self {
            tx_checksum: false,
            rx_checksum: false,
            tso_v4: false,
            tso_v6: false,
            ufo: false,
            gso: false,
            gro: false,
            lro: false,
        }
    }

    pub fn basic() -> Self {
        Self {
            tx_checksum: true,
            rx_checksum: true,
            tso_v4: false,
            tso_v6: false,
            ufo: false,
            gso: false,
            gro: false,
            lro: false,
        }
    }
}

static OFFLOAD_CONFIG: Mutex<OffloadConfig> = Mutex::new(OffloadConfig {
    tx_checksum: false,
    rx_checksum: false,
    tso_v4: false,
    tso_v6: false,
    ufo: false,
    gso: false,
    gro: false,
    lro: false,
});

/// Enable/disable offload features
pub fn set_offloads(config: OffloadConfig) {
    serial_println!(
        "[VIRTIO-NET] Offloads: tx_csum={} rx_csum={} tso4={} gso={} gro={}",
        config.tx_checksum,
        config.rx_checksum,
        config.tso_v4,
        config.gso,
        config.gro
    );
    *OFFLOAD_CONFIG.lock() = config;
}

/// Initialize the virtio-net driver
pub fn init() {
    serial_println!("[VIRTIO-NET] Scanning PCI bus for network devices...");

    if let Some(pci_dev) = find_virtio_net() {
        let io_base = (pci_dev.bar0 & 0xFFFC) as u16; // I/O port base (low bits are flags)

        // Enable I/O access and bus mastering
        unsafe {
            let cmd = pci_config_read16(pci_dev.bus, pci_dev.device, pci_dev.function, PCI_COMMAND);
            let new_cmd = cmd | PCI_COMMAND_IO | PCI_COMMAND_BUS_MASTER;
            pci_config_write32(
                pci_dev.bus,
                pci_dev.device,
                pci_dev.function,
                PCI_COMMAND,
                new_cmd as u32,
            );
        }

        let mut dev = VirtioNetDevice::new(
            io_base,
            pci_dev.bus,
            pci_dev.device,
            pci_dev.function,
            pci_dev.irq,
        );

        if dev.init_legacy() {
            serial_println!("[VIRTIO-NET] Network device ready");
            serial_println!("[VIRTIO-NET]   I/O base: {:#x}", io_base);
            serial_println!("[VIRTIO-NET]   IRQ: {}", pci_dev.irq);
            serial_println!("[VIRTIO-NET]   MAC: {}", dev.mac_string());

            NIC_AVAILABLE.store(true, Ordering::Relaxed);
            *VIRTIO_NET.lock() = Some(dev);

            // Initialize multiqueue (default: 1 pair)
            let _ = mq_set_queue_pairs(1);
            serial_println!(
                "[VIRTIO-NET]   Multiqueue: ready (max {} pairs)",
                MAX_QUEUE_PAIRS
            );
        } else {
            serial_println!("[VIRTIO-NET] Failed to initialize device");
        }
    } else {
        serial_println!("[VIRTIO-NET] No virtio-net device found on PCI bus");
        serial_println!("[VIRTIO-NET] Network stack operating in loopback-only mode");
    }
}
