use alloc::format;
/// Intel i915 Graphics Driver — Intel HD/UHD/Iris GPU support
///
/// Provides:
///   - PCI device detection for Intel VGA (vendor 0x8086, class 0x03)
///   - MMIO BAR mapping and register access
///   - Graphics Memory I/O (GMCH) control
///   - Display pipe/CRTC programming (pipe A/B/C)
///   - Plane enable/disable and base address set
///   - DPLL (Display PLL) configuration
///   - Backlight control via PCH register
///   - EDID reading via GMBUS (I2C)
///   - Mode setting infrastructure
///   - Power well management
///   - GTT (Graphics Translation Table) programming
///
/// Supports device generations from Gen 3 (i915) through Gen 12 (Tiger Lake).
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::pci::{self, PciBar, PciDevice};
use crate::serial_println;

// ─── Intel PCI Vendor ID ────────────────────────────────────────────

const INTEL_VENDOR_ID: u16 = 0x8086;
const PCI_CLASS_DISPLAY: u8 = 0x03;
const PCI_SUBCLASS_VGA: u8 = 0x00;

// ─── GPU Generation ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntelGen {
    Gen3,  // i915, i945
    Gen4,  // G35, G45
    Gen5,  // Ironlake
    Gen6,  // Sandy Bridge
    Gen7,  // Ivy Bridge / Haswell
    Gen8,  // Broadwell
    Gen9,  // Skylake / Kaby Lake / Coffee Lake
    Gen11, // Ice Lake
    Gen12, // Tiger Lake / Alder Lake
    Unknown,
}

impl IntelGen {
    /// Determine generation from PCI device ID
    fn from_device_id(device_id: u16) -> Self {
        match device_id {
            0x2582 | 0x2592 | 0x2772 | 0x27A2 | 0x27AE => Self::Gen3,
            0x2A02 | 0x2A12 | 0x2A42 | 0x2E02 | 0x2E12 | 0x2E22 | 0x2E32 => Self::Gen4,
            0x0042 | 0x0046 => Self::Gen5,
            0x0102 | 0x0112 | 0x0122 | 0x0106 | 0x0116 | 0x0126 => Self::Gen6,
            0x0152 | 0x0162 | 0x0156 | 0x0166 | 0x0402 | 0x0412 | 0x0422 | 0x0406 | 0x0416
            | 0x0426 | 0x0A06 | 0x0A16 | 0x0A26 => Self::Gen7,
            0x1602 | 0x1612 | 0x1622 | 0x1606 | 0x1616 | 0x1626 => Self::Gen8,
            0x1902 | 0x1912 | 0x1916 | 0x1926 | 0x191E | 0x5912 | 0x5916 | 0x5902 | 0x591E
            | 0x3E92 | 0x3E91 | 0x3E98 | 0x9A49 | 0x3EA0 | 0x3EA5 => Self::Gen9,
            0x8A52 | 0x8A5A | 0x8A56 | 0x8A51 => Self::Gen11,
            0x9A40 | 0x4680 | 0x4682 | 0x4690 | 0x4692 | 0x4693 | 0x46A0 | 0x46A6 | 0x46D0 => {
                Self::Gen12
            }
            _ => Self::Unknown,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Gen3 => "Gen3 (i915/i945)",
            Self::Gen4 => "Gen4 (G35/G45)",
            Self::Gen5 => "Gen5 (Ironlake)",
            Self::Gen6 => "Gen6 (Sandy Bridge)",
            Self::Gen7 => "Gen7 (Ivy Bridge/Haswell)",
            Self::Gen8 => "Gen8 (Broadwell)",
            Self::Gen9 => "Gen9 (Skylake+)",
            Self::Gen11 => "Gen11 (Ice Lake)",
            Self::Gen12 => "Gen12 (Tiger Lake+)",
            Self::Unknown => "Unknown Gen",
        }
    }
}

// ─── Display Registers (Gen 9+ offsets) ─────────────────────────────

mod regs {
    // PCI config
    pub const GMCH_CTRL: u8 = 0x50; // GMCH Graphics Control
    pub const BSM: u8 = 0x5C; // Base of Stolen Memory

    // MMIO registers (offsets from BAR0)
    pub const PIPECONF_A: u32 = 0x70008;
    pub const PIPECONF_B: u32 = 0x71008;
    pub const PIPECONF_C: u32 = 0x72008;
    pub const PIPE_ENABLE: u32 = 1 << 31;
    pub const PIPE_STATE_ACTIVE: u32 = 1 << 30;

    // Display plane
    pub const DSPSURF_A: u32 = 0x7019C;
    pub const DSPCNTR_A: u32 = 0x70180;
    pub const DSPSTRIDE_A: u32 = 0x70188;
    pub const DSPPOS_A: u32 = 0x7018C;
    pub const DSPSIZE_A: u32 = 0x70190;

    // Cursor plane
    pub const CURCNTR_A: u32 = 0x70080;
    pub const CURBASE_A: u32 = 0x70084;
    pub const CURPOS_A: u32 = 0x70088;

    // Pipe source size
    pub const PIPESRC_A: u32 = 0x6001C;

    // Display PLL
    pub const DPLL_A: u32 = 0x06014;
    pub const DPLL_B: u32 = 0x06018;
    pub const FPA0: u32 = 0x06040;
    pub const FPA1: u32 = 0x06044;
    pub const DPLL_ENABLE: u32 = 1 << 31;

    // HTOTAL, HBLANK, HSYNC, VTOTAL, VBLANK, VSYNC for pipe A
    pub const HTOTAL_A: u32 = 0x60000;
    pub const HBLANK_A: u32 = 0x60004;
    pub const HSYNC_A: u32 = 0x60008;
    pub const VTOTAL_A: u32 = 0x6000C;
    pub const VBLANK_A: u32 = 0x60010;
    pub const VSYNC_A: u32 = 0x60014;

    // Power Well Control
    pub const PWR_WELL_CTL2: u32 = 0x45404;
    pub const PWR_WELL_CTL_ENABLE: u32 = 1 << 31;
    pub const PWR_WELL_CTL_STATE: u32 = 1 << 30;

    // Backlight
    pub const BLC_PWM_CTL: u32 = 0x48254;
    pub const BLC_PWM_DATA: u32 = 0x48258;

    // GMBUS (I2C for EDID)
    pub const GMBUS0: u32 = 0x5100; // Clock/Port Select
    pub const GMBUS1: u32 = 0x5104; // Command/Status
    pub const GMBUS2: u32 = 0x5108; // Status
    pub const GMBUS3: u32 = 0x510C; // Data
    pub const GMBUS4: u32 = 0x5110; // IRQ Mask
    pub const GMBUS5: u32 = 0x5120; // 2-byte index

    // GTT
    pub const GFX_FLSH_CNTL: u32 = 0x101008;

    // Fence registers (for tiled rendering)
    pub const FENCE_REG_BASE: u32 = 0x100000;

    // Interrupt
    pub const DEISR: u32 = 0x44000; // Display Engine Interrupt Status
    pub const DEIMR: u32 = 0x44004; // Display Engine Interrupt Mask
    pub const DEIER: u32 = 0x4400C; // Display Engine Interrupt Enable

    // Render engine
    pub const RING_BUFFER_START: u32 = 0x02034;
    pub const RING_BUFFER_CTL: u32 = 0x0203C;
    pub const RING_BUFFER_HEAD: u32 = 0x02034;
    pub const RING_BUFFER_TAIL: u32 = 0x02030;
}

// ─── EDID Block ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EdidInfo {
    pub manufacturer: [u8; 3],
    pub product_code: u16,
    pub width_px: u32,
    pub height_px: u32,
    pub width_mm: u32,
    pub height_mm: u32,
    pub preferred_mode: Option<DisplayMode>,
}

#[derive(Debug, Clone, Copy)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub pixel_clock_khz: u32,
    pub htotal: u32,
    pub hblank_start: u32,
    pub hblank_end: u32,
    pub hsync_start: u32,
    pub hsync_end: u32,
    pub vtotal: u32,
    pub vblank_start: u32,
    pub vblank_end: u32,
    pub vsync_start: u32,
    pub vsync_end: u32,
}

// ─── Driver State ───────────────────────────────────────────────────

pub struct I915Device {
    pub pci: PciDevice,
    pub generation: IntelGen,
    /// MMIO base address (BAR0)
    pub mmio_base: u64,
    pub mmio_size: u64,
    /// Graphics aperture (BAR2) — stolen memory / GTT window
    pub aperture_base: u64,
    pub aperture_size: u64,
    /// Stolen memory base from GMCH BSM register
    pub stolen_base: u64,
    /// Current display mode
    pub current_mode: Option<DisplayMode>,
    /// EDID info from connected display
    pub edid: Option<EdidInfo>,
    /// Whether the driver has been fully initialized
    pub initialized: bool,
    /// Backlight level (0-100)
    pub backlight: u32,
}

static DETECTED: AtomicBool = AtomicBool::new(false);
static MMIO_BASE: AtomicU64 = AtomicU64::new(0);

lazy_static::lazy_static! {
    pub static ref I915: Mutex<Option<I915Device>> = Mutex::new(None);
}

// ─── MMIO Helpers ───────────────────────────────────────────────────

unsafe fn mmio_read32(base: u64, offset: u32) -> u32 {
    let ptr = (base + offset as u64) as *const u32;
    core::ptr::read_volatile(ptr)
}

unsafe fn mmio_write32(base: u64, offset: u32, value: u32) {
    let ptr = (base + offset as u64) as *mut u32;
    core::ptr::write_volatile(ptr, value);
}

// ─── Initialization ─────────────────────────────────────────────────

/// Probe PCI bus for Intel graphics and initialize the driver
pub fn init() {
    // Find Intel display devices (class 0x03, subclass 0x00 = VGA)
    let devices = pci::find_by_class(PCI_CLASS_DISPLAY, PCI_SUBCLASS_VGA);

    for dev in &devices {
        if dev.vendor_id != INTEL_VENDOR_ID {
            continue;
        }

        let generation = IntelGen::from_device_id(dev.device_id);
        serial_println!(
            "[i915] Found Intel GPU: {:04x}:{:04x} at {} — {}",
            dev.vendor_id,
            dev.device_id,
            dev.bdf_string(),
            generation.name()
        );

        // Decode BAR0 (MMIO)
        let (mmio_base, mmio_size) = match &dev.bars[0] {
            PciBar::Memory {
                base_addr, size, ..
            } => (*base_addr, *size),
            _ => {
                serial_println!("[i915] ERROR: BAR0 is not memory-mapped");
                continue;
            }
        };

        // Decode BAR2 (Graphics Aperture / stolen memory window)
        let (aperture_base, aperture_size) = match &dev.bars[2] {
            PciBar::Memory {
                base_addr, size, ..
            } => (*base_addr, *size),
            _ => (0, 0),
        };

        serial_println!(
            "[i915] MMIO: {:#x} ({}KB), Aperture: {:#x} ({}MB)",
            mmio_base,
            mmio_size / 1024,
            aperture_base,
            aperture_size / (1024 * 1024)
        );

        // Enable PCI bus mastering + memory space access
        pci::enable_bus_mastering(dev.bus, dev.device, dev.function);
        let cmd = pci::pci_config_read16(dev.bus, dev.device, dev.function, 0x04);
        pci::pci_config_write16(dev.bus, dev.device, dev.function, 0x04, cmd | 0x06);

        // Read stolen memory base
        let bsm = pci::pci_config_read32(dev.bus, dev.device, dev.function, regs::BSM);
        let stolen_base = (bsm & 0xFFF00000) as u64;
        serial_println!("[i915] Stolen memory base: {:#x}", stolen_base);

        // Store state
        MMIO_BASE.store(mmio_base, Ordering::SeqCst);
        DETECTED.store(true, Ordering::SeqCst);

        let device = I915Device {
            pci: dev.clone(),
            generation,
            mmio_base,
            mmio_size,
            aperture_base,
            aperture_size,
            stolen_base,
            current_mode: None,
            edid: None,
            initialized: false,
            backlight: 100,
        };

        *I915.lock() = Some(device);

        // Initialize hardware
        init_hardware(mmio_base, generation);

        // Try reading EDID
        if let Some(edid) = read_edid(mmio_base) {
            serial_println!(
                "[i915] Display: {}x{} @ {}Hz",
                edid.width_px,
                edid.height_px,
                edid.preferred_mode
                    .as_ref()
                    .map(|m| m.refresh_hz)
                    .unwrap_or(0)
            );
            if let Some(ref mut dev) = *I915.lock() {
                dev.edid = Some(edid);
            }
        }

        serial_println!("[i915] Driver initialized successfully");
        if let Some(ref mut dev) = *I915.lock() {
            dev.initialized = true;
        }

        return; // Only support first Intel GPU
    }

    serial_println!("[i915] No Intel graphics device found");
}

/// Initialize GPU hardware after MMIO mapping
fn init_hardware(mmio: u64, generation: IntelGen) {
    unsafe {
        // 1. Enable power wells (Gen9+)
        if generation as u8 >= IntelGen::Gen9 as u8 {
            power_well_enable(mmio);
        }

        // 2. Read current pipe state
        let pipeconf_a = mmio_read32(mmio, regs::PIPECONF_A);
        serial_println!(
            "[i915] Pipe A conf: {:#010x} ({})",
            pipeconf_a,
            if pipeconf_a & regs::PIPE_ENABLE != 0 {
                "enabled"
            } else {
                "disabled"
            }
        );

        // 3. If pipe is already active (BIOS/UEFI set it up), just note the current mode
        if pipeconf_a & regs::PIPE_ENABLE != 0 {
            let pipesrc = mmio_read32(mmio, regs::PIPESRC_A);
            let width = ((pipesrc >> 16) & 0xFFF) + 1;
            let height = (pipesrc & 0xFFF) + 1;
            serial_println!("[i915] Pipe A active: {}x{}", width, height);

            if let Some(ref mut dev) = *I915.lock() {
                dev.current_mode = Some(DisplayMode {
                    width,
                    height,
                    refresh_hz: 60,
                    pixel_clock_khz: 0,
                    htotal: width,
                    hblank_start: width,
                    hblank_end: width,
                    hsync_start: width,
                    hsync_end: width,
                    vtotal: height,
                    vblank_start: height,
                    vblank_end: height,
                    vsync_start: height,
                    vsync_end: height,
                });
            }
        }

        // 4. Mask display engine interrupts
        mmio_write32(mmio, regs::DEIMR, 0xFFFFFFFF);
    }
}

/// Enable power well (required for Gen9+ display engines)
unsafe fn power_well_enable(mmio: u64) {
    let ctl = mmio_read32(mmio, regs::PWR_WELL_CTL2);
    if ctl & regs::PWR_WELL_CTL_STATE == 0 {
        // Request power well enable
        mmio_write32(mmio, regs::PWR_WELL_CTL2, ctl | regs::PWR_WELL_CTL_ENABLE);

        // Poll until state indicates powered
        for _ in 0..1000 {
            let val = mmio_read32(mmio, regs::PWR_WELL_CTL2);
            if val & regs::PWR_WELL_CTL_STATE != 0 {
                serial_println!("[i915] Power well enabled");
                return;
            }
            // Busy-wait ~1μs
            for _ in 0..1000 {
                core::hint::spin_loop();
            }
        }
        serial_println!("[i915] WARNING: Power well enable timeout");
    } else {
        serial_println!("[i915] Power well already enabled");
    }
}

// ─── EDID Reading via GMBUS ─────────────────────────────────────────

/// Read EDID from the display via GMBUS I2C
fn read_edid(mmio: u64) -> Option<EdidInfo> {
    unsafe {
        // Select GMBUS port (analog = 2, HDMI-B = 5, etc.)
        // Try DDI ports in sequence
        for port in &[2u32, 5, 4, 6] {
            mmio_write32(mmio, regs::GMBUS0, *port);

            // Set up GMBUS1: read 128 bytes from slave 0x50 (EDID)
            let cmd = (0x50 << 1) | 1  // slave address + read bit
                | (128 << 16)           // byte count
                | (1 << 30)             // SW ready
                | (1 << 31); // cycle type: WAIT
            mmio_write32(mmio, regs::GMBUS1, cmd);

            // Read EDID bytes from GMBUS3
            let mut edid_raw = [0u8; 128];
            let mut got_data = false;
            for i in (0..128).step_by(4) {
                // Wait for data ready
                let mut ready = false;
                for _ in 0..10000 {
                    let status = mmio_read32(mmio, regs::GMBUS2);
                    if status & (1 << 11) != 0 {
                        ready = true;
                        break;
                    }
                    if status & (1 << 10) != 0 {
                        break; // NACK — no device
                    }
                    core::hint::spin_loop();
                }
                if !ready {
                    break;
                }

                let data = mmio_read32(mmio, regs::GMBUS3);
                for j in 0..4 {
                    if i + j < 128 {
                        edid_raw[i + j] = ((data >> (j * 8)) & 0xFF) as u8;
                    }
                }
                got_data = true;
            }

            // Stop GMBUS
            mmio_write32(mmio, regs::GMBUS1, 1 << 27);

            if got_data {
                // Verify EDID header: 0x00 FF FF FF FF FF FF 00
                if edid_raw[0] == 0x00
                    && edid_raw[1] == 0xFF
                    && edid_raw[6] == 0xFF
                    && edid_raw[7] == 0x00
                {
                    return Some(parse_edid(&edid_raw));
                }
            }
        }
    }
    None
}

/// Parse EDID block into EdidInfo
fn parse_edid(raw: &[u8; 128]) -> EdidInfo {
    let mfg0 = ((raw[8] as u16) << 8) | raw[9] as u16;
    let manufacturer = [
        (((mfg0 >> 10) & 0x1F) as u8) + b'A' - 1,
        (((mfg0 >> 5) & 0x1F) as u8) + b'A' - 1,
        ((mfg0 & 0x1F) as u8) + b'A' - 1,
    ];
    let product_code = (raw[9] as u16) << 8 | raw[8] as u16;

    // Max image size (cm)
    let width_mm = raw[21] as u32 * 10;
    let height_mm = raw[22] as u32 * 10;

    // Parse preferred timing (Detailed Timing Descriptor at offset 54)
    let dtd = &raw[54..72];
    let pixel_clock_khz = ((dtd[1] as u32) << 8 | dtd[0] as u32) * 10;
    let hactive = dtd[2] as u32 | (((dtd[4] >> 4) as u32) << 8);
    let hblank = dtd[3] as u32 | (((dtd[4] & 0x0F) as u32) << 8);
    let vactive = dtd[5] as u32 | (((dtd[7] >> 4) as u32) << 8);
    let vblank = dtd[6] as u32 | (((dtd[7] & 0x0F) as u32) << 8);
    let hsync_off = dtd[8] as u32 | (((dtd[11] >> 6) as u32) << 8);
    let hsync_w = dtd[9] as u32 | ((((dtd[11] >> 4) & 0x03) as u32) << 8);
    let vsync_off = ((dtd[10] >> 4) as u32) | ((((dtd[11] >> 2) & 0x03) as u32) << 4);
    let vsync_w = (dtd[10] & 0x0F) as u32 | (((dtd[11] & 0x03) as u32) << 4);

    let htotal = hactive + hblank;
    let vtotal = vactive + vblank;
    let refresh_hz = if htotal > 0 && vtotal > 0 {
        (pixel_clock_khz * 1000) / (htotal * vtotal)
    } else {
        60
    };

    let preferred_mode = if pixel_clock_khz > 0 {
        Some(DisplayMode {
            width: hactive,
            height: vactive,
            refresh_hz,
            pixel_clock_khz,
            htotal,
            hblank_start: hactive,
            hblank_end: hactive + hblank,
            hsync_start: hactive + hsync_off,
            hsync_end: hactive + hsync_off + hsync_w,
            vtotal,
            vblank_start: vactive,
            vblank_end: vactive + vblank,
            vsync_start: vactive + vsync_off,
            vsync_end: vactive + vsync_off + vsync_w,
        })
    } else {
        None
    };

    EdidInfo {
        manufacturer,
        product_code,
        width_px: hactive,
        height_px: vactive,
        width_mm,
        height_mm,
        preferred_mode,
    }
}

// ─── Mode Setting ───────────────────────────────────────────────────

/// Set display mode on pipe A
pub fn set_mode(mode: &DisplayMode) -> Result<(), &'static str> {
    let guard = I915.lock();
    let dev = guard.as_ref().ok_or("i915 not initialized")?;
    let mmio = dev.mmio_base;

    if !dev.initialized {
        return Err("i915 driver not ready");
    }

    unsafe {
        // 1. Disable pipe A
        let conf = mmio_read32(mmio, regs::PIPECONF_A);
        mmio_write32(mmio, regs::PIPECONF_A, conf & !regs::PIPE_ENABLE);
        wait_pipe_disabled(mmio, regs::PIPECONF_A);

        // 2. Disable display plane
        let dspcntr = mmio_read32(mmio, regs::DSPCNTR_A);
        mmio_write32(mmio, regs::DSPCNTR_A, dspcntr & !(1 << 31));
        mmio_write32(mmio, regs::DSPSURF_A, 0); // Trigger update

        // 3. Program timing registers
        mmio_write32(
            mmio,
            regs::HTOTAL_A,
            ((mode.htotal - 1) << 16) | (mode.width - 1),
        );
        mmio_write32(
            mmio,
            regs::HBLANK_A,
            ((mode.hblank_end - 1) << 16) | (mode.hblank_start - 1),
        );
        mmio_write32(
            mmio,
            regs::HSYNC_A,
            ((mode.hsync_end - 1) << 16) | (mode.hsync_start - 1),
        );
        mmio_write32(
            mmio,
            regs::VTOTAL_A,
            ((mode.vtotal - 1) << 16) | (mode.height - 1),
        );
        mmio_write32(
            mmio,
            regs::VBLANK_A,
            ((mode.vblank_end - 1) << 16) | (mode.vblank_start - 1),
        );
        mmio_write32(
            mmio,
            regs::VSYNC_A,
            ((mode.vsync_end - 1) << 16) | (mode.vsync_start - 1),
        );

        // 4. Set pipe source size
        mmio_write32(
            mmio,
            regs::PIPESRC_A,
            ((mode.width - 1) << 16) | (mode.height - 1),
        );

        // 5. Enable display plane (BGRA 8888 format, bit 26 = pixel format)
        let stride = mode.width * 4;
        mmio_write32(mmio, regs::DSPSTRIDE_A, stride);
        mmio_write32(mmio, regs::DSPPOS_A, 0);
        mmio_write32(
            mmio,
            regs::DSPSIZE_A,
            ((mode.height - 1) << 16) | (mode.width - 1),
        );

        // Enable plane: bit 31=enable, bits 29:26=BGRA8888 (0b0110)
        let plane_ctl = (1u32 << 31) | (0b0110 << 26);
        mmio_write32(mmio, regs::DSPCNTR_A, plane_ctl);

        // 6. Re-enable pipe
        mmio_write32(mmio, regs::PIPECONF_A, regs::PIPE_ENABLE | (1 << 22)); // 8bpc
        wait_pipe_enabled(mmio, regs::PIPECONF_A);
    }

    // Update stored mode
    drop(guard);
    if let Some(ref mut dev) = *I915.lock() {
        dev.current_mode = Some(*mode);
    }

    serial_println!(
        "[i915] Mode set: {}x{} @ {}Hz",
        mode.width,
        mode.height,
        mode.refresh_hz
    );
    Ok(())
}

/// Wait for pipe to become disabled
unsafe fn wait_pipe_disabled(mmio: u64, pipeconf_reg: u32) {
    for _ in 0..10000 {
        let val = mmio_read32(mmio, pipeconf_reg);
        if val & regs::PIPE_STATE_ACTIVE == 0 {
            return;
        }
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }
    serial_println!("[i915] WARNING: Pipe disable timeout");
}

/// Wait for pipe to become enabled
unsafe fn wait_pipe_enabled(mmio: u64, pipeconf_reg: u32) {
    for _ in 0..10000 {
        let val = mmio_read32(mmio, pipeconf_reg);
        if val & regs::PIPE_STATE_ACTIVE != 0 {
            return;
        }
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }
    serial_println!("[i915] WARNING: Pipe enable timeout");
}

// ─── Backlight Control ──────────────────────────────────────────────

/// Set backlight brightness (0-100)
pub fn set_backlight(percent: u32) {
    let guard = I915.lock();
    if let Some(dev) = guard.as_ref() {
        let mmio = dev.mmio_base;
        let max_val = unsafe { mmio_read32(mmio, regs::BLC_PWM_CTL) >> 16 };
        if max_val > 0 {
            let val = (max_val * percent.min(100)) / 100;
            unsafe {
                mmio_write32(mmio, regs::BLC_PWM_DATA, val);
            }
        }
    }
    drop(guard);
    if let Some(ref mut dev) = *I915.lock() {
        dev.backlight = percent.min(100);
    }
}

/// Get current backlight brightness
pub fn get_backlight() -> u32 {
    I915.lock().as_ref().map(|d| d.backlight).unwrap_or(100)
}

// ─── GTT (Graphics Translation Table) ──────────────────────────────

/// Write a GTT entry (maps a graphics virtual page to physical address)
pub fn gtt_write(mmio: u64, entry_index: u32, phys_addr: u64) {
    // GTT entries are at the end of the MMIO region (or via a separate BAR)
    // Format: bits 38:12 = physical address, bit 0 = valid
    let gtt_entry = (phys_addr & 0x7FFFFFF000) | 1; // valid bit
    let gtt_offset = 0x800000 + entry_index * 8; // Gen8+ uses 8-byte GTT entries
    unsafe {
        let ptr = (mmio + gtt_offset as u64) as *mut u64;
        core::ptr::write_volatile(ptr, gtt_entry);
    }
}

/// Flush GTT TLB
pub fn gtt_flush(mmio: u64) {
    unsafe {
        mmio_write32(mmio, regs::GFX_FLSH_CNTL, 1);
        mmio_read32(mmio, regs::GFX_FLSH_CNTL); // Read-back to ensure completion
    }
}

// ─── Display Surface ────────────────────────────────────────────────

/// Set the display surface base address (for page flipping)
pub fn set_display_surface(phys_addr: u64) {
    let guard = I915.lock();
    if let Some(dev) = guard.as_ref() {
        unsafe {
            mmio_write32(
                dev.mmio_base,
                regs::DSPSURF_A,
                (phys_addr & 0xFFFFF000) as u32,
            );
        }
    }
}

// ─── Query Functions ────────────────────────────────────────────────

/// Check if an Intel GPU was detected
pub fn is_detected() -> bool {
    DETECTED.load(Ordering::SeqCst)
}

/// Get GPU info string
pub fn info_string() -> String {
    let guard = I915.lock();
    match guard.as_ref() {
        Some(dev) => {
            let mode_str = match &dev.current_mode {
                Some(m) => format!("{}x{} @ {}Hz", m.width, m.height, m.refresh_hz),
                None => String::from("no mode"),
            };
            format!(
                "Intel {} [{:04x}:{:04x}] — {} | MMIO {:#x} | Aperture {}MB",
                dev.generation.name(),
                dev.pci.vendor_id,
                dev.pci.device_id,
                mode_str,
                dev.mmio_base,
                dev.aperture_size / (1024 * 1024),
            )
        }
        None => String::from("No Intel GPU detected"),
    }
}

/// Get current resolution
pub fn current_resolution() -> Option<(u32, u32)> {
    I915.lock()
        .as_ref()
        .and_then(|d| d.current_mode.as_ref())
        .map(|m| (m.width, m.height))
}
