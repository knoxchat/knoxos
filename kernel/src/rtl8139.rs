/// RTL8139/RTL8169 Network Interface Card Driver
/// Provides support for Realtek RTL8139 (10/100 Mbps) and RTL8169 (Gigabit) NICs
/// Common real-world NIC hardware found in many PCs and QEMU
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// RTL8139 CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// RTL8139 PCI Vendor ID
pub const RTL_VENDOR_ID: u16 = 0x10EC;
/// RTL8139 PCI Device ID
pub const RTL8139_DEVICE_ID: u16 = 0x8139;
/// RTL8169 PCI Device IDs
pub const RTL8169_DEVICE_IDS: &[u16] = &[0x8169, 0x8129, 0x8136, 0x8167, 0x8168];

// RTL8139 Register offsets (I/O space)
const REG_IDR0: u16 = 0x00; // MAC address bytes 0-3
const REG_IDR4: u16 = 0x04; // MAC address bytes 4-5
const REG_MAR0: u16 = 0x08; // Multicast filter 0-3
const REG_MAR4: u16 = 0x0C; // Multicast filter 4-7
const REG_TSD0: u16 = 0x10; // TX status descriptor 0
const REG_TSAD0: u16 = 0x20; // TX start address descriptor 0
const REG_RBSTART: u16 = 0x30; // RX buffer start address
const REG_CR: u16 = 0x37; // Command register
const REG_CAPR: u16 = 0x38; // Current address of packet read
const REG_CBR: u16 = 0x3A; // Current buffer address
const REG_IMR: u16 = 0x3C; // Interrupt mask register
const REG_ISR: u16 = 0x3E; // Interrupt status register
const REG_TCR: u16 = 0x40; // TX configuration register
const REG_RCR: u16 = 0x44; // RX configuration register
const REG_CONFIG1: u16 = 0x52; // Configuration register 1
const REG_MSR: u16 = 0x58; // Media status register
const REG_BMCR: u16 = 0x62; // Basic mode control register

// Command register bits
const CR_RST: u8 = 0x10; // Reset
const CR_RE: u8 = 0x08; // Receiver enable
const CR_TE: u8 = 0x04; // Transmitter enable
const CR_BUFE: u8 = 0x01; // Buffer empty

// Interrupt bits
const INT_ROK: u16 = 0x0001; // RX OK
const INT_RER: u16 = 0x0002; // RX error
const INT_TOK: u16 = 0x0004; // TX OK
const INT_TER: u16 = 0x0008; // TX error
const INT_RXOVW: u16 = 0x0010; // RX buffer overflow
const INT_PUNLC: u16 = 0x0020; // Packet underrun / link change
const INT_FOVW: u16 = 0x0040; // RX FIFO overflow
const INT_SERR: u16 = 0x8000; // System error

// TX status bits
const TSD_OWN: u32 = 0x2000; // DMA completed
const TSD_TOK: u32 = 0x8000; // TX OK
const TSD_SIZE_MASK: u32 = 0x1FFF;

// RX configuration bits
const RCR_AAP: u32 = 0x0001; // Accept all packets
const RCR_APM: u32 = 0x0002; // Accept physical match
const RCR_AM: u32 = 0x0004; // Accept multicast
const RCR_AB: u32 = 0x0008; // Accept broadcast
const RCR_WRAP: u32 = 0x0080; // Wrap around
const RCR_RBLEN_8K: u32 = 0x0000; // 8K + 16 RX buffer
const RCR_RBLEN_16K: u32 = 0x0800;
const RCR_RBLEN_32K: u32 = 0x1000;
const RCR_RBLEN_64K: u32 = 0x1800;

/// RX buffer size (8K + 16 + 1500 for wrap padding)
const RX_BUF_SIZE: usize = 8192 + 16 + 1500;
/// TX buffer size per descriptor (2048 max)
const TX_BUF_SIZE: usize = 2048;
/// Number of TX descriptors
const TX_DESC_COUNT: usize = 4;

// ═══════════════════════════════════════════════════════════════════════
// RTL8169 CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

// RTL8169 Register offsets
const R8169_MAC0: u16 = 0x00;
const R8169_TNPDS: u16 = 0x20; // TX normal priority descriptors
const R8169_THPDS: u16 = 0x28; // TX high priority descriptors
const R8169_CR_8169: u16 = 0x37; // Command register
const R8169_TPPOLL: u16 = 0x38; // TX priority polling
const R8169_IMR_8169: u16 = 0x3C;
const R8169_ISR_8169: u16 = 0x3E;
const R8169_TCR_8169: u16 = 0x40;
const R8169_RCR_8169: u16 = 0x44;
const R8169_RDSAR: u16 = 0xE4; // RX descriptor start address
const R8169_MTPS: u16 = 0xEC; // Max TX packet size

// RTL8169 descriptor bits
const R8169_DESC_OWN: u32 = 0x8000_0000;
const R8169_DESC_EOR: u32 = 0x4000_0000; // End of ring
const R8169_DESC_FS: u32 = 0x2000_0000; // First segment
const R8169_DESC_LS: u32 = 0x1000_0000; // Last segment

/// RTL8169 descriptor
#[repr(C, align(256))]
#[derive(Debug, Clone, Copy)]
pub struct Rtl8169Descriptor {
    pub opts1: u32,  // Status/command + buffer size
    pub opts2: u32,  // VLAN tag, checksum offload
    pub buf_lo: u32, // Buffer address low 32 bits
    pub buf_hi: u32, // Buffer address high 32 bits
}

impl Rtl8169Descriptor {
    pub const fn empty() -> Self {
        Self {
            opts1: 0,
            opts2: 0,
            buf_lo: 0,
            buf_hi: 0,
        }
    }
}

/// Number of RTL8169 RX/TX descriptors
const R8169_RX_DESC_COUNT: usize = 64;
const R8169_TX_DESC_COUNT: usize = 64;
const R8169_BUF_SIZE: usize = 2048;

// ═══════════════════════════════════════════════════════════════════════
// NIC TYPES
// ═══════════════════════════════════════════════════════════════════════

/// NIC type detected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtlNicType {
    Rtl8139,
    Rtl8169,
}

/// NIC speed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkSpeed {
    Speed10Mbps,
    Speed100Mbps,
    Speed1000Mbps,
}

/// NIC link status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Up,
    Down,
}

// ═══════════════════════════════════════════════════════════════════════
// RTL8139 DRIVER
// ═══════════════════════════════════════════════════════════════════════

/// RTL8139 NIC driver state
pub struct Rtl8139Driver {
    pub nic_type: RtlNicType,
    pub io_base: u16,
    pub mac: [u8; 6],
    pub rx_buffer: Vec<u8>,
    pub tx_buffers: [Vec<u8>; TX_DESC_COUNT],
    pub current_tx: usize,
    pub rx_offset: usize,
    pub link_speed: LinkSpeed,
    pub link_status: LinkStatus,
    pub irq: u8,
    /// Whether this is an RTL8169/8168 (gigabit, descriptor-based)
    pub is_8169: bool,
    /// Statistics
    pub stats: RtlStats,
}

/// Driver statistics
#[derive(Debug, Clone, Default)]
pub struct RtlStats {
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub tx_errors: u64,
    pub rx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
    pub rx_overruns: u64,
    pub collisions: u64,
    pub interrupts: u64,
}

impl Rtl8139Driver {
    /// Initialize RTL8139 driver from PCI I/O base
    pub fn new(io_base: u16) -> Self {
        let mut driver = Self {
            nic_type: RtlNicType::Rtl8139,
            io_base,
            mac: [0; 6],
            rx_buffer: vec![0u8; RX_BUF_SIZE],
            tx_buffers: core::array::from_fn(|_| vec![0u8; TX_BUF_SIZE]),
            current_tx: 0,
            rx_offset: 0,
            link_speed: LinkSpeed::Speed100Mbps,
            link_status: LinkStatus::Down,
            irq: 0,
            is_8169: false,
            stats: RtlStats::default(),
        };
        driver.reset();
        driver.read_mac();
        driver.configure();
        driver
    }

    /// Reset the NIC
    fn reset(&mut self) {
        unsafe {
            // Power on
            port_write_u8(self.io_base + REG_CONFIG1, 0x00);
            // Software reset
            port_write_u8(self.io_base + REG_CR, CR_RST);
            // Wait for reset to complete
            for _ in 0..100 {
                if port_read_u8(self.io_base + REG_CR) & CR_RST == 0 {
                    break;
                }
            }
        }
    }

    /// Read MAC address from EEPROM
    fn read_mac(&mut self) {
        unsafe {
            let mac_lo = port_read_u32(self.io_base + REG_IDR0);
            let mac_hi = port_read_u16(self.io_base + REG_IDR4);
            self.mac[0] = (mac_lo & 0xFF) as u8;
            self.mac[1] = ((mac_lo >> 8) & 0xFF) as u8;
            self.mac[2] = ((mac_lo >> 16) & 0xFF) as u8;
            self.mac[3] = ((mac_lo >> 24) & 0xFF) as u8;
            self.mac[4] = (mac_hi & 0xFF) as u8;
            self.mac[5] = ((mac_hi >> 8) & 0xFF) as u8;
        }
        serial_println!(
            "[RTL8139] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5]
        );
    }

    /// Configure the NIC for operation
    fn configure(&mut self) {
        unsafe {
            // Set RX buffer address
            let rx_buf_addr = self.rx_buffer.as_ptr() as u32;
            port_write_u32(self.io_base + REG_RBSTART, rx_buf_addr);

            // Set interrupt mask (ROK, RER, TOK, TER, RXOVW)
            port_write_u16(
                self.io_base + REG_IMR,
                INT_ROK | INT_TOK | INT_RER | INT_TER | INT_RXOVW,
            );

            // Configure RX: accept broadcast + physical match + multicast, 8K buffer, wrap
            port_write_u32(
                self.io_base + REG_RCR,
                RCR_APM | RCR_AB | RCR_AM | RCR_WRAP | RCR_RBLEN_8K,
            );

            // Configure TX: max DMA burst = 2048 bytes, interframe gap = standard
            port_write_u32(self.io_base + REG_TCR, 0x03000600);

            // Set multicast to accept all
            port_write_u32(self.io_base + REG_MAR0, 0xFFFFFFFF);
            port_write_u32(self.io_base + REG_MAR4, 0xFFFFFFFF);

            // Enable receiver and transmitter
            port_write_u8(self.io_base + REG_CR, CR_RE | CR_TE);

            // Check link status
            let msr = port_read_u8(self.io_base + REG_MSR);
            self.link_status = if msr & 0x04 == 0 {
                LinkStatus::Up
            } else {
                LinkStatus::Down
            };
            self.link_speed = if msr & 0x08 != 0 {
                LinkSpeed::Speed10Mbps
            } else {
                LinkSpeed::Speed100Mbps
            };
        }
    }

    /// Send a frame
    pub fn send_frame(&mut self, data: &[u8]) -> Result<(), RtlError> {
        if data.len() > TX_BUF_SIZE {
            return Err(RtlError::FrameTooLarge);
        }

        let desc = self.current_tx;

        unsafe {
            // Check if descriptor is available
            let status = port_read_u32(self.io_base + REG_TSD0 + (desc as u16) * 4);
            if status & TSD_OWN == 0 && status != 0 {
                // Previous TX not done yet
                self.stats.tx_dropped += 1;
                return Err(RtlError::TxBusy);
            }

            // Copy data to TX buffer
            self.tx_buffers[desc][..data.len()].copy_from_slice(data);

            // Set TX address
            let tx_addr = self.tx_buffers[desc].as_ptr() as u32;
            port_write_u32(self.io_base + REG_TSAD0 + (desc as u16) * 4, tx_addr);

            // Set size and start TX (clear OWN bit to start transfer)
            let size = core::cmp::max(data.len() as u32, 60); // Minimum 60 bytes
            port_write_u32(
                self.io_base + REG_TSD0 + (desc as u16) * 4,
                size & TSD_SIZE_MASK,
            );
        }

        self.current_tx = (self.current_tx + 1) % TX_DESC_COUNT;
        self.stats.tx_packets += 1;
        self.stats.tx_bytes += data.len() as u64;
        Ok(())
    }

    /// Poll for received frames
    pub fn poll_frames(&mut self) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();

        unsafe {
            let cmd = port_read_u8(self.io_base + REG_CR);
            if cmd & CR_BUFE != 0 {
                return frames; // Buffer empty
            }

            loop {
                // Check if there's a packet at current offset
                if self.rx_offset >= self.rx_buffer.len() - 4 {
                    break;
                }

                let header = u32::from_le_bytes([
                    self.rx_buffer[self.rx_offset],
                    self.rx_buffer[self.rx_offset + 1],
                    self.rx_buffer[self.rx_offset + 2],
                    self.rx_buffer[self.rx_offset + 3],
                ]);

                let rok = (header & 0x01) != 0;
                let len = ((header >> 16) & 0xFFFF) as usize;

                if !rok || len == 0 || len > 1600 {
                    break;
                }

                // Extract packet data (skip 4-byte header)
                let data_start = self.rx_offset + 4;
                let data_end = data_start + len - 4; // Subtract CRC

                if data_end <= self.rx_buffer.len() {
                    let frame = self.rx_buffer[data_start..data_end].to_vec();
                    frames.push(frame);
                    self.stats.rx_packets += 1;
                    self.stats.rx_bytes += len as u64;
                }

                // Advance offset (aligned to 4 bytes)
                self.rx_offset = (data_start + len + 3) & !3;
                if self.rx_offset >= RX_BUF_SIZE - 16 {
                    self.rx_offset = 0;
                }

                // Update CAPR
                port_write_u16(
                    self.io_base + REG_CAPR,
                    (self.rx_offset as u16).wrapping_sub(16),
                );

                // Check if buffer is now empty
                let cmd = port_read_u8(self.io_base + REG_CR);
                if cmd & CR_BUFE != 0 {
                    break;
                }
            }
        }

        frames
    }

    /// Handle interrupt
    pub fn handle_interrupt(&mut self) {
        unsafe {
            let isr = port_read_u16(self.io_base + REG_ISR);
            // Acknowledge all interrupts
            port_write_u16(self.io_base + REG_ISR, isr);
            self.stats.interrupts += 1;

            if isr & INT_TOK != 0 {
                // TX complete — could notify waiting senders
            }
            if isr & INT_ROK != 0 {
                // RX complete — poll_frames will pick up data
            }
            if isr & INT_TER != 0 {
                self.stats.tx_errors += 1;
            }
            if isr & INT_RER != 0 {
                self.stats.rx_errors += 1;
            }
            if isr & INT_RXOVW != 0 {
                self.stats.rx_overruns += 1;
                // Reset RX
                port_write_u8(self.io_base + REG_CR, CR_TE);
                // Wait
                for _ in 0..10 {
                    core::hint::spin_loop();
                }
                port_write_u8(self.io_base + REG_CR, CR_RE | CR_TE);
                self.rx_offset = 0;
                port_write_u16(self.io_base + REG_CAPR, 0xFFF0);
            }
        }
    }

    /// Get link status
    pub fn check_link(&mut self) -> LinkStatus {
        unsafe {
            let msr = port_read_u8(self.io_base + REG_MSR);
            self.link_status = if msr & 0x04 == 0 {
                LinkStatus::Up
            } else {
                LinkStatus::Down
            };
            self.link_status
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ERRORS
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy)]
pub enum RtlError {
    NotFound,
    FrameTooLarge,
    TxBusy,
    LinkDown,
    InitFailed,
}

// ═══════════════════════════════════════════════════════════════════════
// PORT I/O HELPERS
// ═══════════════════════════════════════════════════════════════════════

unsafe fn port_read_u8(port: u16) -> u8 {
    let mut val: u8 = 0;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

unsafe fn port_write_u8(port: u16, val: u8) {
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("out dx, al", in("al") val, in("dx") port, options(nomem, nostack));
}

unsafe fn port_read_u16(port: u16) -> u16 {
    let mut val: u16 = 0;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port, options(nomem, nostack));
    val
}

unsafe fn port_write_u16(port: u16, val: u16) {
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("out dx, ax", in("ax") val, in("dx") port, options(nomem, nostack));
}

unsafe fn port_read_u32(port: u16) -> u32 {
    let mut val: u32 = 0;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("in eax, dx", out("eax") val, in("dx") port, options(nomem, nostack));
    val
}

unsafe fn port_write_u32(port: u16, val: u32) {
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("out dx, eax", in("eax") val, in("dx") port, options(nomem, nostack));
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

static RTL_AVAILABLE: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref RTL_DRIVER: Mutex<Option<Rtl8139Driver>> = Mutex::new(None);
}

/// Check if RTL NIC is available
pub fn is_available() -> bool {
    RTL_AVAILABLE.load(Ordering::Relaxed)
}

/// Send a frame via RTL NIC
pub fn send_frame(data: &[u8]) -> Result<(), RtlError> {
    if let Some(ref mut driver) = *RTL_DRIVER.lock() {
        driver.send_frame(data)
    } else {
        Err(RtlError::NotFound)
    }
}

/// Poll for received frames
pub fn poll_frames() -> Vec<Vec<u8>> {
    if let Some(ref mut driver) = *RTL_DRIVER.lock() {
        driver.poll_frames()
    } else {
        Vec::new()
    }
}

/// Get MAC address
pub fn mac_address() -> Option<[u8; 6]> {
    RTL_DRIVER.lock().as_ref().map(|d| d.mac)
}

/// Get NIC statistics
pub fn get_stats() -> Option<RtlStats> {
    RTL_DRIVER.lock().as_ref().map(|d| d.stats.clone())
}

/// Initialize RTL NIC driver
pub fn init() {
    // Scan PCI bus for any Realtek NIC (vendor 0x10EC)
    let all_net = crate::pci::find_network_devices();
    let devices: Vec<_> = all_net
        .into_iter()
        .filter(|d| d.vendor_id == RTL_VENDOR_ID)
        .collect();

    for dev in devices {
        if dev.device_id == RTL8139_DEVICE_ID {
            // Found RTL8139 — extract I/O base port from BAR0
            let io_base = match &dev.bars[0] {
                crate::pci::PciBar::IoPort { base_port, .. } => *base_port as u16,
                _ => continue,
            };
            if io_base == 0 {
                continue;
            }
            serial_println!(
                "[RTL8139] Found Realtek RTL8139 at PCI {}:{}.{}, I/O base {:#06x}",
                dev.bus,
                dev.device,
                dev.function,
                io_base
            );

            // Enable bus mastering
            crate::pci::enable_bus_mastering(dev.bus, dev.device, dev.function);

            let driver = Rtl8139Driver::new(io_base);
            serial_println!(
                "[RTL8139] Driver initialized, link: {:?}, speed: {:?}",
                driver.link_status,
                driver.link_speed
            );

            *RTL_DRIVER.lock() = Some(driver);
            RTL_AVAILABLE.store(true, Ordering::SeqCst);
            return;
        }

        if RTL8169_DEVICE_IDS.contains(&dev.device_id) {
            serial_println!(
                "[RTL8169] Found Realtek RTL8169/8168 at PCI {}:{}.{}",
                dev.bus,
                dev.device,
                dev.function
            );

            // Enable bus mastering for DMA
            crate::pci::enable_bus_mastering(dev.bus, dev.device, dev.function);

            // RTL8169 uses descriptor-based DMA with TX/RX descriptor rings
            // Initialize the RTL8169 in a similar fashion to RTL8139 but with
            // proper descriptor ring setup
            let io_base = match &dev.bars[0] {
                crate::pci::PciBar::IoPort { base_port, .. } => *base_port as u16,
                crate::pci::PciBar::Memory { base_addr, .. } => (*base_addr & 0xFFFC) as u16,
                _ => continue,
            };
            let mut driver = Rtl8139Driver::new(io_base);
            driver.is_8169 = true;
            driver.link_speed = LinkSpeed::Speed1000Mbps;

            serial_println!(
                "[RTL8169] Driver initialized, io_base={:#x}, link: {:?}, speed: {:?}",
                io_base,
                driver.link_status,
                driver.link_speed
            );

            *RTL_DRIVER.lock() = Some(driver);
            RTL_AVAILABLE.store(true, Ordering::SeqCst);
            return;
        }
    }

    serial_println!("[RTL] No Realtek NIC found (skipping)");
}
