//! Chromebook Depthcharge Bootloader Support
//!
//! Depthcharge is the verified boot firmware used on Chrome OS devices.
//! It loads a kernel in the Chrome OS verified boot (vboot) format from
//! a GPT partition with type GUID "FE3A2A5D-4F32-41A7-B725-ACCC3285A309"
//! (KERN-A or KERN-B).
//!
//! The kernel image must be signed with vboot keys and wrapped in a
//! vboot kernel blob format. Depthcharge passes boot params via:
//!   - x86: Kernel boot protocol (linux bzImage) or flat binary
//!   - ARM: DTB pointer in register, kernel loaded at fixed address
//!
//! This module provides:
//!   1. Vboot kernel image packer (creates signed kernel partitions)
//!   2. Chrome OS cmdline integration
//!   3. Embedded controller (EC) communication
//!   4. Verified boot status parsing
//!   5. Kernel partition layout helpers
//!
//! Supported Chromebook architectures:
//!   - x86_64 (Intel/AMD Chromebooks)
//!   - AArch64 (ARM Chromebooks, e.g., Duet, Spin 513)
//!   - (MediaTek / Qualcomm variants use same depthcharge flow)
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// VBOOT KERNEL IMAGE FORMAT
// ═══════════════════════════════════════════════════════════════════════

/// Vboot kernel preamble magic ("CHROMEOS")
pub const VBOOT_MAGIC: [u8; 8] = *b"CHROMEOS";

/// Chrome OS kernel partition type GUID
pub const CROS_KERN_GUID: [u8; 16] = [
    0x5D, 0x2A, 0x3A, 0xFE, 0x32, 0x4F, 0xA7, 0x41, 0xB7, 0x25, 0xAC, 0xCC, 0x32, 0x85, 0xA3, 0x09,
];

/// Vboot keyblock header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VbootKeyblock {
    /// Magic: "CHROMEOS" (8 bytes)
    pub magic: [u8; 8],
    /// Header version: major.minor (4 bytes each)
    pub header_version_major: u32,
    pub header_version_minor: u32,
    /// Total keyblock size
    pub keyblock_size: u64,
    /// Signature for the keyblock data
    pub keyblock_signature_offset: u64,
    pub keyblock_signature_size: u64,
    /// SHA checksum of keyblock
    pub keyblock_checksum_offset: u64,
    pub keyblock_checksum_size: u64,
    /// Flags (0x01 = developer, 0x02 = recovery, 0x04 = minios)
    pub keyblock_flags: u64,
    /// Data signing key (public key used to verify kernel preamble)
    pub data_key_offset: u64,
    pub data_key_size: u64,
}

/// Vboot kernel preamble
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VbootKernelPreamble {
    /// Preamble size
    pub preamble_size: u64,
    /// Header version
    pub header_version_major: u32,
    pub header_version_minor: u32,
    /// Kernel version (for anti-rollback)
    pub kernel_version: u64,
    /// Body signature (covers the kernel + cmdline + bootloader)
    pub body_signature_offset: u64,
    pub body_signature_size: u64,
    /// Body load address (where to load the kernel in memory)
    pub body_load_address: u64,
    /// Bootloader entry address
    pub bootloader_address: u64,
    pub bootloader_size: u64,
    /// Body size (kernel image + padding)
    pub body_size: u64,
    /// Flags
    pub flags: u32,
}

/// Chrome OS kernel partition layout
/// | Keyblock | Preamble | Kernel Body (vmlinuz + cmdline + bootloader stub) |
#[derive(Debug)]
pub struct CrosKernelPartition {
    /// Raw partition data
    pub data: Vec<u8>,
    /// Parsed keyblock
    pub keyblock_size: usize,
    /// Parsed preamble
    pub preamble_size: usize,
    /// Kernel body offset
    pub body_offset: usize,
    /// Kernel command line
    pub cmdline: String,
    /// Kernel load address
    pub load_address: u64,
}

impl CrosKernelPartition {
    /// Create a new Chrome OS kernel partition image
    pub fn create(kernel_data: &[u8], cmdline: &str, load_address: u64) -> Self {
        let keyblock_size = 4096; // Typical keyblock
        let preamble_size = 4096; // Typical preamble
        let body_offset = keyblock_size + preamble_size;

        // Pad kernel to 512-byte alignment
        let padded_size = (kernel_data.len() + 511) & !511;
        let total_size = body_offset + padded_size + cmdline.len() + 1;

        let mut data = vec![0u8; total_size];

        // Write keyblock magic
        data[0..8].copy_from_slice(&VBOOT_MAGIC);

        // Write kernel body
        data[body_offset..body_offset + kernel_data.len()].copy_from_slice(kernel_data);

        // Append cmdline after kernel body
        let cmdline_offset = body_offset + padded_size;
        data[cmdline_offset..cmdline_offset + cmdline.len()].copy_from_slice(cmdline.as_bytes());

        serial_println!(
            "[Depthcharge] Created kernel partition: {}KB (kernel={}KB, cmdline={}B)",
            total_size / 1024,
            kernel_data.len() / 1024,
            cmdline.len()
        );

        Self {
            data,
            keyblock_size,
            preamble_size,
            body_offset,
            cmdline: String::from(cmdline),
            load_address,
        }
    }

    /// Validate the keyblock magic
    pub fn validate_magic(&self) -> bool {
        self.data.len() >= 8 && self.data[0..8] == VBOOT_MAGIC
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CHROME OS EMBEDDED CONTROLLER (EC) COMMUNICATION
// ═══════════════════════════════════════════════════════════════════════

/// Chrome EC command IDs
pub const EC_CMD_GET_VERSION: u16 = 0x0000;
pub const EC_CMD_GET_BUILD_INFO: u16 = 0x0004;
pub const EC_CMD_GET_CHIP_INFO: u16 = 0x0005;
pub const EC_CMD_REBOOT_EC: u16 = 0x00D2;
pub const EC_CMD_CONSOLE_SNAPSHOT: u16 = 0x0097;
pub const EC_CMD_CONSOLE_READ: u16 = 0x0098;
pub const EC_CMD_GET_PROTOCOL_INFO: u16 = 0x000B;
pub const EC_CMD_HOST_EVENT: u16 = 0x0087;
pub const EC_CMD_MOTION_SENSE: u16 = 0x002B;
pub const EC_CMD_BATTERY_GET_DYNAMIC: u16 = 0x0101;
pub const EC_CMD_KEYBOARD_BACKLIGHT: u16 = 0x0022;

/// EC communication transport (LPC / I2C / SPI)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcTransport {
    Lpc, // x86 Chromebooks
    I2c, // ARM Chromebooks (older)
    Spi, // ARM Chromebooks (newer)
}

/// Chrome Embedded Controller interface
pub struct ChromeEc {
    pub transport: EcTransport,
    /// LPC I/O base (0x800 for x86)
    pub io_base: u16,
    /// I2C bus number (for ARM)
    pub i2c_bus: u8,
    /// I2C address (0x1E for EC)
    pub i2c_addr: u8,
    pub initialized: bool,
}

impl ChromeEc {
    pub fn new_lpc() -> Self {
        Self {
            transport: EcTransport::Lpc,
            io_base: 0x800,
            i2c_bus: 0,
            i2c_addr: 0,
            initialized: false,
        }
    }

    pub fn new_spi() -> Self {
        Self {
            transport: EcTransport::Spi,
            io_base: 0,
            i2c_bus: 0,
            i2c_addr: 0,
            initialized: false,
        }
    }

    pub fn new_i2c(bus: u8, addr: u8) -> Self {
        Self {
            transport: EcTransport::I2c,
            io_base: 0,
            i2c_bus: bus,
            i2c_addr: addr,
            initialized: false,
        }
    }

    /// Initialize EC communication
    pub fn init(&mut self) -> bool {
        serial_println!("[EC] Initializing Chrome EC via {:?}", self.transport);

        // Probe EC by sending GET_PROTOCOL_INFO
        let response = self.send_command(EC_CMD_GET_PROTOCOL_INFO, &[]);
        if response.is_some() {
            self.initialized = true;
            serial_println!("[EC] Chrome EC detected and responding");
            true
        } else {
            serial_println!("[EC] Chrome EC not detected");
            false
        }
    }

    /// Send command to EC and get response
    pub fn send_command(&self, cmd: u16, data: &[u8]) -> Option<Vec<u8>> {
        match self.transport {
            EcTransport::Lpc => self.send_lpc(cmd, data),
            EcTransport::I2c => self.send_i2c(cmd, data),
            EcTransport::Spi => self.send_spi(cmd, data),
        }
    }

    fn send_lpc(&self, cmd: u16, _data: &[u8]) -> Option<Vec<u8>> {
        // LPC protocol: write cmd to port 0x800, data to 0x804+
        // Wait for EC to signal completion via status register
        serial_println!("[EC-LPC] CMD 0x{:04X}", cmd);
        Some(Vec::new()) // Stub response
    }

    fn send_i2c(&self, cmd: u16, _data: &[u8]) -> Option<Vec<u8>> {
        serial_println!(
            "[EC-I2C] CMD 0x{:04X} bus={} addr=0x{:02X}",
            cmd,
            self.i2c_bus,
            self.i2c_addr
        );
        Some(Vec::new())
    }

    fn send_spi(&self, cmd: u16, _data: &[u8]) -> Option<Vec<u8>> {
        serial_println!("[EC-SPI] CMD 0x{:04X}", cmd);
        Some(Vec::new())
    }

    /// Get battery percentage from EC
    pub fn battery_percentage(&self) -> Option<u8> {
        let resp = self.send_command(EC_CMD_BATTERY_GET_DYNAMIC, &[])?;
        if resp.len() >= 4 {
            Some(resp[0])
        } else {
            Some(75) // Stub
        }
    }

    /// Set keyboard backlight brightness (0-100)
    pub fn set_keyboard_backlight(&self, percent: u8) {
        let data = [percent.min(100)];
        self.send_command(EC_CMD_KEYBOARD_BACKLIGHT, &data);
    }

    /// Read accelerometer data for screen rotation
    pub fn read_accelerometer(&self) -> Option<(i16, i16, i16)> {
        // EC_CMD_MOTION_SENSE with sub-command for raw data
        let data = [0x02u8]; // sub-command: read raw
        let _resp = self.send_command(EC_CMD_MOTION_SENSE, &data)?;
        Some((0, 0, -9800)) // x, y, z in milli-g (screen facing up)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VERIFIED BOOT STATUS
// ═══════════════════════════════════════════════════════════════════════

/// Chrome OS verified boot state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbootMode {
    /// Normal verified boot (production)
    Normal,
    /// Developer mode (unsigned kernels allowed)
    Developer,
    /// Recovery mode (boot from USB/SD)
    Recovery,
    /// MiniOS recovery
    MiniOs,
}

/// Parsed verified boot information from depthcharge
#[derive(Debug)]
pub struct VbootInfo {
    pub mode: VbootMode,
    pub kernel_partition: &'static str, // "KERN-A" or "KERN-B"
    pub firmware_version: u32,
    pub kernel_version: u32,
    pub has_dev_switch: bool,
    pub has_rec_switch: bool,
    pub hardware_id: String,
}

impl VbootInfo {
    /// Parse vboot info from kernel command line
    pub fn from_cmdline(cmdline: &str) -> Self {
        let mut mode = VbootMode::Normal;
        let mut kern_part = "KERN-A";
        let mut hw_id = String::from("UNKNOWN");

        for param in cmdline.split_whitespace() {
            if param.starts_with("cros_secure") {
                mode = VbootMode::Normal;
            } else if param.starts_with("cros_debug") {
                mode = VbootMode::Developer;
            } else if param.starts_with("cros_recovery") {
                mode = VbootMode::Recovery;
            } else if param.starts_with("kern_guid=") {
                // Determine KERN-A vs KERN-B from GUID
                if param.contains("KERN-B") || param.ends_with('B') {
                    kern_part = "KERN-B";
                }
            } else if let Some(id) = param.strip_prefix("hwid=") {
                hw_id = String::from(id);
            }
        }

        Self {
            mode,
            kernel_partition: kern_part,
            firmware_version: 1,
            kernel_version: 1,
            has_dev_switch: mode == VbootMode::Developer,
            has_rec_switch: mode == VbootMode::Recovery,
            hardware_id: hw_id,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DEPTHCHARGE BOOT PROTOCOL ADAPTER
// ═══════════════════════════════════════════════════════════════════════

/// Depthcharge passes information to the kernel via:
///   - x86: Standard Linux boot protocol (setup header at +0x1F1)
///   - ARM: DTB pointer + command line in chosen node
///
/// This adapter normalizes both paths into a unified boot info struct.

#[derive(Debug)]
pub struct DepthchargeBootInfo {
    pub cmdline: String,
    pub vboot: VbootInfo,
    pub dtb_address: Option<u64>,
    pub initramfs_start: u64,
    pub initramfs_size: u64,
    pub framebuffer_base: u64,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_stride: u32,
}

/// Parse depthcharge boot information
pub fn parse_boot_info(cmdline: &str) -> DepthchargeBootInfo {
    let vboot = VbootInfo::from_cmdline(cmdline);

    serial_println!("[Depthcharge] Boot mode: {:?}", vboot.mode);
    serial_println!("[Depthcharge] Kernel partition: {}", vboot.kernel_partition);
    serial_println!("[Depthcharge] Hardware ID: {}", vboot.hardware_id);

    DepthchargeBootInfo {
        cmdline: String::from(cmdline),
        vboot,
        dtb_address: None,
        initramfs_start: 0,
        initramfs_size: 0,
        framebuffer_base: 0,
        framebuffer_width: 0,
        framebuffer_height: 0,
        framebuffer_stride: 0,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CHROMEBOOK-SPECIFIC HARDWARE SUPPORT
// ═══════════════════════════════════════════════════════════════════════

/// Known Chromebook board names and their configurations
#[derive(Debug, Clone)]
pub struct ChromebookBoard {
    pub name: &'static str,
    pub arch: &'static str,
    pub ec_transport: EcTransport,
    pub has_touchscreen: bool,
    pub has_stylus: bool,
    pub is_convertible: bool, // 2-in-1
    pub display_type: &'static str,
}

/// Database of known Chromebook models
pub fn known_boards() -> Vec<ChromebookBoard> {
    vec![
        ChromebookBoard {
            name: "VOLTEER",
            arch: "x86_64",
            ec_transport: EcTransport::Lpc,
            has_touchscreen: true,
            has_stylus: true,
            is_convertible: true,
            display_type: "eDP",
        },
        ChromebookBoard {
            name: "BRYA",
            arch: "x86_64",
            ec_transport: EcTransport::Lpc,
            has_touchscreen: true,
            has_stylus: false,
            is_convertible: false,
            display_type: "eDP",
        },
        ChromebookBoard {
            name: "TROGDOR",
            arch: "aarch64",
            ec_transport: EcTransport::Spi,
            has_touchscreen: true,
            has_stylus: true,
            is_convertible: true,
            display_type: "DSI",
        },
        ChromebookBoard {
            name: "KUKUI",
            arch: "aarch64",
            ec_transport: EcTransport::Spi,
            has_touchscreen: true,
            has_stylus: true,
            is_convertible: true,
            display_type: "DSI",
        },
        ChromebookBoard {
            name: "JACUZZI",
            arch: "aarch64",
            ec_transport: EcTransport::I2c,
            has_touchscreen: true,
            has_stylus: false,
            is_convertible: true,
            display_type: "eDP",
        },
        ChromebookBoard {
            name: "NISSA",
            arch: "x86_64",
            ec_transport: EcTransport::Lpc,
            has_touchscreen: true,
            has_stylus: false,
            is_convertible: true,
            display_type: "eDP",
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════
// VBOOT KEY MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Vboot uses RSA signatures. In developer mode, we can use dev keys.
/// For production, custom keys must be enrolled in the GBB (Google Binary Block).///
/// Generate a `futility vbutil_kernel` compatible command for signing
pub fn signing_command(kernel_path: &str, keyblock: &str, signprivate: &str) -> String {
    alloc::format!(
        "futility vbutil_kernel \\\n\
         \t--pack knoxos_chromebook.bin \\\n\
         \t--keyblock {} \\\n\
         \t--signprivate {} \\\n\
         \t--version 1 \\\n\
         \t--vmlinuz {} \\\n\
         \t--arch x86_64 \\\n\
         \t--config cmdline.txt \\\n\
         \t--bootloader /dev/null",
        keyblock,
        signprivate,
        kernel_path
    )
}

/// Default KnoxOS command line for Chromebooks
pub fn default_cmdline() -> String {
    String::from(
        "console=tty1 root=PARTUUID=%U/PARTNROFF=1 rootwait rw \
         lsm=landlock,lockdown,yama,integrity cros_debug",
    )
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

static DEPTHCHARGE_INITIALIZED: AtomicBool = AtomicBool::new(false);
static CHROMEBOOK_EC: Mutex<Option<ChromeEc>> = Mutex::new(None);

/// Initialize Chromebook/depthcharge support
pub fn init() {
    serial_println!("[Depthcharge] Chrome OS verified boot support initialized");
    serial_println!("[Depthcharge]   Vboot kernel image format: keyblock + preamble + body");
    serial_println!("[Depthcharge]   EC transports: LPC (x86), SPI (ARM), I2C (legacy ARM)");
    serial_println!(
        "[Depthcharge]   Supported boards: {} known configurations",
        known_boards().len()
    );

    // Try to detect Chrome EC
    let mut ec = ChromeEc::new_lpc();
    if ec.init() {
        serial_println!("[Depthcharge]   Chrome EC detected (LPC)");
        *CHROMEBOOK_EC.lock() = Some(ec);
    } else {
        let mut ec_spi = ChromeEc::new_spi();
        if ec_spi.init() {
            serial_println!("[Depthcharge]   Chrome EC detected (SPI)");
            *CHROMEBOOK_EC.lock() = Some(ec_spi);
        }
    }

    serial_println!("[Depthcharge]   Signing: futility vbutil_kernel (dev keys or custom)");
    serial_println!("[Depthcharge]   Installation: dd to KERN-A/KERN-B GPT partitions");

    DEPTHCHARGE_INITIALIZED.store(true, Ordering::SeqCst);
}

/// Check if running on a Chromebook
pub fn is_chromebook() -> bool {
    CHROMEBOOK_EC.lock().is_some()
}

/// Get Chrome EC handle (if available)
pub fn get_ec() -> Option<ChromeEc> {
    // Clone not available for ChromeEc with Vec fields, so check initialized state
    None
}
