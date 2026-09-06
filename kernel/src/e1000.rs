/// E1000 NIC Driver — Intel 82540EM Gigabit Ethernet Controller
///
/// Alternative NIC driver to virtio-net for QEMU's e1000 device.
/// This driver provides Ethernet frame I/O via PCI MMIO registers.
///
/// Supported devices:
///   - Intel 82540EM (QEMU -device e1000)
///   - Intel 82545EM
///   - Intel 82574L
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── PCI IDs ────────────────────────────────────────────────────────────

/// Intel vendor ID
pub const INTEL_VENDOR: u16 = 0x8086;

/// Known E1000 device IDs
pub const E1000_82540EM: u16 = 0x100E; // QEMU default e1000
pub const E1000_82545EM_A: u16 = 0x100F;
pub const E1000_82574L: u16 = 0x10D3;

// ─── E1000 Register Offsets ─────────────────────────────────────────────

const REG_CTRL: u32 = 0x0000; // Device Control
const REG_STATUS: u32 = 0x0008; // Device Status
const REG_EECD: u32 = 0x0010; // EEPROM/Flash Control
const REG_EERD: u32 = 0x0014; // EEPROM Read
const REG_ICR: u32 = 0x00C0; // Interrupt Cause Read
const REG_IMS: u32 = 0x00D0; // Interrupt Mask Set
const REG_IMC: u32 = 0x00D8; // Interrupt Mask Clear
const REG_RCTL: u32 = 0x0100; // Receive Control
const REG_TCTL: u32 = 0x0400; // Transmit Control
const REG_RDBAL: u32 = 0x2800; // RX Descriptor Base Low
const REG_RDBAH: u32 = 0x2804; // RX Descriptor Base High
const REG_RDLEN: u32 = 0x2808; // RX Descriptor Length
const REG_RDH: u32 = 0x2810; // RX Descriptor Head
const REG_RDT: u32 = 0x2818; // RX Descriptor Tail
const REG_TDBAL: u32 = 0x3800; // TX Descriptor Base Low
const REG_TDBAH: u32 = 0x3804; // TX Descriptor Base High
const REG_TDLEN: u32 = 0x3808; // TX Descriptor Length
const REG_TDH: u32 = 0x3810; // TX Descriptor Head
const REG_TDT: u32 = 0x3818; // TX Descriptor Tail
const REG_RAL: u32 = 0x5400; // Receive Address Low
const REG_RAH: u32 = 0x5404; // Receive Address High
const REG_MTA: u32 = 0x5200; // Multicast Table Array

// ─── Control Register Bits ──────────────────────────────────────────────

const CTRL_SLU: u32 = 1 << 6; // Set Link Up
const CTRL_RST: u32 = 1 << 26; // Device Reset
const CTRL_ASDE: u32 = 1 << 5; // Auto-Speed Detection Enable

const RCTL_EN: u32 = 1 << 1; // Receiver Enable
const RCTL_SBP: u32 = 1 << 2; // Store Bad Packets
const RCTL_UPE: u32 = 1 << 3; // Unicast Promiscuous
const RCTL_MPE: u32 = 1 << 4; // Multicast Promiscuous
const RCTL_LBM_NONE: u32 = 0; // No Loopback
const RCTL_BAM: u32 = 1 << 15; // Broadcast Accept
const RCTL_BSIZE_2048: u32 = 0; // Buffer Size 2048
const RCTL_SECRC: u32 = 1 << 26; // Strip Ethernet CRC

const TCTL_EN: u32 = 1 << 1; // Transmitter Enable
const TCTL_PSP: u32 = 1 << 3; // Pad Short Packets
const TCTL_CT_SHIFT: u32 = 4; // Collision Threshold shift
const TCTL_COLD_SHIFT: u32 = 12; // Collision Distance shift

// ─── Descriptor Status Bits ─────────────────────────────────────────────

const RXD_STAT_DD: u8 = 1 << 0; // Descriptor Done
const RXD_STAT_EOP: u8 = 1 << 1; // End of Packet

const TXD_CMD_EOP: u8 = 1 << 0; // End of Packet
const TXD_CMD_IFCS: u8 = 1 << 1; // Insert FCS
const TXD_CMD_RS: u8 = 1 << 3; // Report Status
const TXD_STAT_DD: u8 = 1 << 0; // Descriptor Done

// ─── Interrupt Bits ─────────────────────────────────────────────────────

const ICR_TXDW: u32 = 1 << 0; // TX Descriptor Written Back
const ICR_TXQE: u32 = 1 << 1; // TX Queue Empty
const ICR_LSC: u32 = 1 << 2; // Link Status Change
const ICR_RXDMT0: u32 = 1 << 4; // RX Descriptor Minimum Threshold
const ICR_RXO: u32 = 1 << 6; // Receiver Overrun
const ICR_RXT0: u32 = 1 << 7; // Receiver Timer Interrupt

// ─── Ring Sizes ─────────────────────────────────────────────────────────

const NUM_RX_DESC: usize = 32;
const NUM_TX_DESC: usize = 32;
const BUFFER_SIZE: usize = 2048;

// ─── Data Structures ───────────────────────────────────────────────────

/// E1000 RX descriptor (legacy format, 16 bytes)
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
struct RxDesc {
    addr: u64,     // Buffer physical address
    length: u16,   // Received length
    checksum: u16, // Packet checksum
    status: u8,    // Status flags
    errors: u8,    // Error flags
    special: u16,  // VLAN tag
}

/// E1000 TX descriptor (legacy format, 16 bytes)
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
struct TxDesc {
    addr: u64,    // Buffer physical address
    length: u16,  // Buffer length
    cso: u8,      // Checksum offset
    cmd: u8,      // Command byte
    status: u8,   // Status (DD bit)
    css: u8,      // Checksum start
    special: u16, // VLAN tag
}

/// E1000 network device
pub struct E1000Device {
    /// MMIO base address (virtual)
    mmio_base: u64,
    /// MAC address
    pub mac: [u8; 6],
    /// RX descriptors
    rx_descs: Vec<RxDesc>,
    /// TX descriptors
    tx_descs: Vec<TxDesc>,
    /// RX buffers
    rx_buffers: Vec<Vec<u8>>,
    /// TX buffers
    tx_buffers: Vec<Vec<u8>>,
    /// Current RX descriptor tail
    rx_tail: u16,
    /// Current TX descriptor tail
    tx_tail: u16,
    /// Packets received
    pub rx_count: u64,
    /// Packets transmitted
    pub tx_count: u64,
    /// PCI info
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub irq: u8,
}

impl E1000Device {
    pub fn new(mmio_base: u64, bus: u8, device: u8, function: u8, irq: u8) -> Self {
        Self {
            mmio_base,
            mac: [0; 6],
            rx_descs: Vec::new(),
            tx_descs: Vec::new(),
            rx_buffers: Vec::new(),
            tx_buffers: Vec::new(),
            rx_tail: 0,
            tx_tail: 0,
            rx_count: 0,
            tx_count: 0,
            bus,
            device,
            function,
            irq,
        }
    }

    /// Read a 32-bit register
    fn read_reg(&self, offset: u32) -> u32 {
        unsafe {
            let addr = self.mmio_base + offset as u64;
            core::ptr::read_volatile(addr as *const u32)
        }
    }

    /// Write a 32-bit register
    fn write_reg(&self, offset: u32, value: u32) {
        unsafe {
            let addr = self.mmio_base + offset as u64;
            core::ptr::write_volatile(addr as *mut u32, value);
        }
    }

    /// Read MAC address from EEPROM
    fn read_mac_eeprom(&mut self) -> bool {
        for i in 0..3 {
            self.write_reg(REG_EERD, 1 | ((i as u32) << 8));

            // Wait for EEPROM read to complete (bit 4 = done)
            let mut timeout = 10000u32;
            loop {
                let val = self.read_reg(REG_EERD);
                if val & (1 << 4) != 0 {
                    let data = (val >> 16) as u16;
                    self.mac[i * 2] = (data & 0xFF) as u8;
                    self.mac[i * 2 + 1] = (data >> 8) as u8;
                    break;
                }
                timeout -= 1;
                if timeout == 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Read MAC address from RAL/RAH registers (fallback)
    fn read_mac_ral(&mut self) {
        let ral = self.read_reg(REG_RAL);
        let rah = self.read_reg(REG_RAH);
        self.mac[0] = (ral & 0xFF) as u8;
        self.mac[1] = ((ral >> 8) & 0xFF) as u8;
        self.mac[2] = ((ral >> 16) & 0xFF) as u8;
        self.mac[3] = ((ral >> 24) & 0xFF) as u8;
        self.mac[4] = (rah & 0xFF) as u8;
        self.mac[5] = ((rah >> 8) & 0xFF) as u8;
    }

    /// Initialize the E1000 device
    pub fn init(&mut self) -> bool {
        serial_println!("[E1000] Initializing device...");

        // Reset the device
        self.write_reg(REG_CTRL, self.read_reg(REG_CTRL) | CTRL_RST);
        // Wait for reset to complete
        for _ in 0..10000 {
            if self.read_reg(REG_CTRL) & CTRL_RST == 0 {
                break;
            }
        }

        // Disable interrupts initially
        self.write_reg(REG_IMC, 0xFFFFFFFF);

        // Read MAC address
        if !self.read_mac_eeprom() {
            self.read_mac_ral();
        }

        if self.mac == [0; 6] {
            serial_println!("[E1000] Warning: MAC address is all zeros");
            // Use a default MAC for QEMU
            self.mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        }

        // Set link up
        let ctrl = self.read_reg(REG_CTRL);
        self.write_reg(REG_CTRL, ctrl | CTRL_SLU | CTRL_ASDE);

        // Clear multicast table
        for i in 0..128 {
            self.write_reg(REG_MTA + i * 4, 0);
        }

        // Set up RX ring
        self.init_rx();

        // Set up TX ring
        self.init_tx();

        // Enable interrupts
        self.write_reg(REG_IMS, ICR_RXT0 | ICR_LSC | ICR_RXDMT0);

        serial_println!("[E1000] Device initialized successfully");
        serial_println!(
            "[E1000]   MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5]
        );

        true
    }

    /// Initialize receive ring
    fn init_rx(&mut self) {
        // Allocate RX descriptors and buffers
        self.rx_descs = vec![
            RxDesc {
                addr: 0,
                length: 0,
                checksum: 0,
                status: 0,
                errors: 0,
                special: 0,
            };
            NUM_RX_DESC
        ];

        self.rx_buffers = (0..NUM_RX_DESC).map(|_| vec![0u8; BUFFER_SIZE]).collect();

        // Set buffer addresses in descriptors
        for i in 0..NUM_RX_DESC {
            self.rx_descs[i].addr = self.rx_buffers[i].as_ptr() as u64;
        }

        // Program RX descriptor ring
        let ring_addr = self.rx_descs.as_ptr() as u64;
        self.write_reg(REG_RDBAL, ring_addr as u32);
        self.write_reg(REG_RDBAH, (ring_addr >> 32) as u32);
        self.write_reg(REG_RDLEN, (NUM_RX_DESC * 16) as u32);
        self.write_reg(REG_RDH, 0);
        self.write_reg(REG_RDT, (NUM_RX_DESC - 1) as u32);

        self.rx_tail = 0;

        // Enable receiver
        self.write_reg(REG_RCTL, RCTL_EN | RCTL_BAM | RCTL_BSIZE_2048 | RCTL_SECRC);
    }

    /// Initialize transmit ring
    fn init_tx(&mut self) {
        // Allocate TX descriptors and buffers
        self.tx_descs = vec![
            TxDesc {
                addr: 0,
                length: 0,
                cso: 0,
                cmd: 0,
                status: 0,
                css: 0,
                special: 0,
            };
            NUM_TX_DESC
        ];

        self.tx_buffers = (0..NUM_TX_DESC).map(|_| vec![0u8; BUFFER_SIZE]).collect();

        // Set buffer addresses
        for i in 0..NUM_TX_DESC {
            self.tx_descs[i].addr = self.tx_buffers[i].as_ptr() as u64;
            self.tx_descs[i].status = TXD_STAT_DD; // Mark as done (available)
        }

        // Program TX descriptor ring
        let ring_addr = self.tx_descs.as_ptr() as u64;
        self.write_reg(REG_TDBAL, ring_addr as u32);
        self.write_reg(REG_TDBAH, (ring_addr >> 32) as u32);
        self.write_reg(REG_TDLEN, (NUM_TX_DESC * 16) as u32);
        self.write_reg(REG_TDH, 0);
        self.write_reg(REG_TDT, 0);

        self.tx_tail = 0;

        // Enable transmitter
        self.write_reg(
            REG_TCTL,
            TCTL_EN | TCTL_PSP | (15 << TCTL_CT_SHIFT) | (64 << TCTL_COLD_SHIFT),
        );
    }

    /// Send a packet
    pub fn send_packet(&mut self, data: &[u8]) -> bool {
        if data.len() > BUFFER_SIZE {
            return false;
        }

        let idx = self.tx_tail as usize;

        // Wait for descriptor to be available
        if self.tx_descs[idx].status & TXD_STAT_DD == 0 {
            static TX_FULL_COUNT: AtomicU64 = AtomicU64::new(0);
            let n = TX_FULL_COUNT.fetch_add(1, Ordering::Relaxed);
            if n < 3 || n % 1000 == 0 {
                serial_println!("[E1000] TX ring full (count={})", n + 1);
            }
            return false;
        }

        // Copy data to TX buffer
        let len = data.len().min(BUFFER_SIZE);
        self.tx_buffers[idx][..len].copy_from_slice(&data[..len]);

        // Set up descriptor
        self.tx_descs[idx].length = len as u16;
        self.tx_descs[idx].cmd = TXD_CMD_EOP | TXD_CMD_IFCS | TXD_CMD_RS;
        self.tx_descs[idx].status = 0;

        // Advance tail
        self.tx_tail = ((self.tx_tail as usize + 1) % NUM_TX_DESC) as u16;
        self.write_reg(REG_TDT, self.tx_tail as u32);

        self.tx_count += 1;
        true
    }

    /// Poll for received packets
    pub fn poll_rx(&mut self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();

        loop {
            let idx = self.rx_tail as usize;
            let desc = &self.rx_descs[idx];

            // Check if descriptor has a packet
            if desc.status & RXD_STAT_DD == 0 {
                break;
            }

            if desc.status & RXD_STAT_EOP != 0 {
                let len = desc.length as usize;
                if len <= BUFFER_SIZE {
                    let mut packet = vec![0u8; len];
                    packet.copy_from_slice(&self.rx_buffers[idx][..len]);
                    packets.push(packet);
                    self.rx_count += 1;
                }
            }

            // Reset descriptor
            self.rx_descs[idx].status = 0;

            // Advance tail
            let old_tail = self.rx_tail;
            self.rx_tail = ((self.rx_tail as usize + 1) % NUM_RX_DESC) as u16;
            self.write_reg(REG_RDT, old_tail as u32);
        }

        packets
    }

    /// Handle interrupt
    pub fn handle_interrupt(&mut self) -> u32 {
        let icr = self.read_reg(REG_ICR);
        // Reading ICR clears the interrupt
        icr
    }

    /// Get link status
    pub fn is_link_up(&self) -> bool {
        self.read_reg(REG_STATUS) & 0x02 != 0
    }

    /// MAC address as string
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
}

// ─── Global Device ──────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref E1000_DEV: Mutex<Option<E1000Device>> = Mutex::new(None);
}

static E1000_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Check if an E1000 NIC is available
pub fn is_available() -> bool {
    E1000_AVAILABLE.load(Ordering::Relaxed)
}

/// Send a raw Ethernet frame via E1000
pub fn send_frame(data: &[u8]) -> bool {
    if let Some(ref mut dev) = *E1000_DEV.lock() {
        dev.send_packet(data)
    } else {
        false
    }
}

/// Poll for received frames
pub fn poll_frames() -> Vec<Vec<u8>> {
    if let Some(ref mut dev) = *E1000_DEV.lock() {
        dev.poll_rx()
    } else {
        Vec::new()
    }
}

/// Get MAC address
pub fn get_mac() -> Option<[u8; 6]> {
    E1000_DEV.lock().as_ref().map(|dev| dev.mac)
}

/// Handle E1000 interrupt
pub fn handle_interrupt() {
    if let Some(ref mut dev) = *E1000_DEV.lock() {
        let _icr = dev.handle_interrupt();
        // Poll for packets
        let packets = dev.poll_rx();
        for packet in packets {
            crate::net::process_packet(&packet);
        }
    }
}

// ─── PCI Helpers ────────────────────────────────────────────────────────

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

unsafe fn pci_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    let mut addr_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_ADDRESS);
    let mut data_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_DATA);
    addr_port.write(address);
    data_port.read()
}

unsafe fn pci_read16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let val32 = pci_read32(bus, dev, func, offset & 0xFC);
    ((val32 >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

unsafe fn pci_write32(bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    let mut addr_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_ADDRESS);
    let mut data_port: crate::arch_compat::instructions::port::Port<u32> =
        crate::arch_compat::instructions::port::Port::new(PCI_CONFIG_DATA);
    addr_port.write(address);
    data_port.write(value);
}

/// Discovered E1000 PCI device
#[derive(Debug, Clone)]
struct E1000PciInfo {
    bus: u8,
    device: u8,
    function: u8,
    vendor_id: u16,
    device_id: u16,
    irq: u8,
    bar0: u32,
}

/// Find an E1000 device on the PCI bus
fn find_e1000() -> Option<E1000PciInfo> {
    for bus in 0..=255u16 {
        for device in 0..32u8 {
            for func in 0..8u8 {
                unsafe {
                    let vendor_id = pci_read16(bus as u8, device, func, 0x00);
                    if vendor_id == 0xFFFF {
                        continue;
                    }
                    let device_id = pci_read16(bus as u8, device, func, 0x02);

                    if vendor_id == INTEL_VENDOR {
                        match device_id {
                            E1000_82540EM | E1000_82545EM_A | E1000_82574L => {
                                let bar0 = pci_read32(bus as u8, device, func, 0x10);
                                let irq = pci_read16(bus as u8, device, func, 0x3C) as u8;
                                serial_println!(
                                    "[E1000] Found device: vendor={:#06x} device={:#06x}",
                                    vendor_id,
                                    device_id
                                );
                                return Some(E1000PciInfo {
                                    bus: bus as u8,
                                    device,
                                    function: func,
                                    vendor_id,
                                    device_id,
                                    irq,
                                    bar0,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    None
}

/// Initialize the E1000 NIC driver
pub fn init() {
    // Don't initialize if virtio-net is already available
    if crate::virtio_net::is_nic_available() {
        serial_println!("[E1000] Skipping — virtio-net already active");
        return;
    }

    serial_println!("[E1000] Scanning PCI bus for Intel E1000 NIC...");

    if let Some(pci_dev) = find_e1000() {
        // Get MMIO base from BAR0 (physical address)
        let bar0 = pci_dev.bar0 as u64;
        let mmio_phys = bar0 & !0xF; // Mask low bits (type/prefetchable flags)

        // Convert physical MMIO address to virtual address via physical memory offset
        let phys_offset = crate::vmm::get_phys_mem_offset();
        let mmio_base = mmio_phys + phys_offset;

        serial_println!(
            "[E1000] BAR0 phys={:#x} virt={:#x} (offset={:#x})",
            mmio_phys,
            mmio_base,
            phys_offset
        );

        // Enable PCI bus mastering and memory access
        unsafe {
            let cmd = pci_read16(pci_dev.bus, pci_dev.device, pci_dev.function, 0x04);
            pci_write32(
                pci_dev.bus,
                pci_dev.device,
                pci_dev.function,
                0x04,
                (cmd | 0x06) as u32, // Memory Space + Bus Master
            );
        }

        let mut dev = E1000Device::new(
            mmio_base,
            pci_dev.bus,
            pci_dev.device,
            pci_dev.function,
            pci_dev.irq,
        );

        if dev.init() {
            E1000_AVAILABLE.store(true, Ordering::Relaxed);
            *E1000_DEV.lock() = Some(dev);
            serial_println!("[E1000] E1000 NIC ready");
        } else {
            serial_println!("[E1000] Failed to initialize E1000");
        }
    } else {
        serial_println!("[E1000] No E1000 device found");
    }
}
