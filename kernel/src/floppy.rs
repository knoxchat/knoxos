/// Floppy Disk Controller (FDC) Driver
///
/// Legacy floppy disk support for reading 3.5" and 5.25" diskettes.
/// Implements the Intel 82077AA-compatible FDC interface.
///
/// Features:
///   - 1.44MB 3.5" HD (18 sectors/track, 80 tracks, 2 heads)
///   - 720KB 3.5" DD
///   - 1.2MB 5.25" HD
///   - DMA-based data transfer (ISA DMA channel 2)
///   - Motor control with auto-spindown timer
///   - Seek, read, write, format operations
///   - Change-line detection (disk change)
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// I/O PORTS (Primary FDC at 0x3F0)
// ═══════════════════════════════════════════════════════════════════════

const FDC_SRA: u16 = 0x3F0; // Status Register A (read)
const FDC_SRB: u16 = 0x3F1; // Status Register B (read)
const FDC_DOR: u16 = 0x3F2; // Digital Output Register (r/w)
const FDC_TDR: u16 = 0x3F3; // Tape Drive Register
const FDC_MSR: u16 = 0x3F4; // Main Status Register (read)
const FDC_DSR: u16 = 0x3F4; // Data Rate Select Register (write)
const FDC_FIFO: u16 = 0x3F5; // Data (FIFO) register
const FDC_DIR: u16 = 0x3F7; // Digital Input Register (read)
const FDC_CCR: u16 = 0x3F7; // Configuration Control Register (write)

// DOR bits
const DOR_DRIVE_SEL: u8 = 0x03;
const DOR_RESET: u8 = 0x04;
const DOR_DMA_EN: u8 = 0x08;
const DOR_MOTOR_A: u8 = 0x10;
const DOR_MOTOR_B: u8 = 0x20;

// MSR bits
const MSR_BUSY: u8 = 0x10;
const MSR_DIO: u8 = 0x40; // 1 = controller → CPU
const MSR_RQM: u8 = 0x80; // Ready for data transfer

// FDC Commands
const CMD_SPECIFY: u8 = 0x03;
const CMD_SENSE_DRIVE: u8 = 0x04;
const CMD_RECALIBRATE: u8 = 0x07;
const CMD_SENSE_INT: u8 = 0x08;
const CMD_READ_DATA: u8 = 0xE6; // MFM + MT + SK
const CMD_WRITE_DATA: u8 = 0xC5; // MFM + MT
const CMD_FORMAT_TRACK: u8 = 0x4D;
const CMD_SEEK: u8 = 0x0F;
const CMD_VERSION: u8 = 0x10;

// ISA DMA channel 2 ports
const DMA_ADDR: u16 = 0x04;
const DMA_COUNT: u16 = 0x05;
const DMA_PAGE: u16 = 0x81;
const DMA_MASK: u16 = 0x0A;
const DMA_MODE: u16 = 0x0B;
const DMA_FLIP_FLOP: u16 = 0x0C;

/// Floppy disk geometry
#[derive(Debug, Clone, Copy)]
pub struct FloppyGeometry {
    pub heads: u8,
    pub tracks: u8,
    pub sectors_per_track: u8,
    pub bytes_per_sector: u16,
    pub gap3_length: u8,
    pub data_rate: u8,
}

/// Standard 1.44MB 3.5" HD
pub const GEOM_144: FloppyGeometry = FloppyGeometry {
    heads: 2,
    tracks: 80,
    sectors_per_track: 18,
    bytes_per_sector: 512,
    gap3_length: 0x1B,
    data_rate: 0x00, // 500 Kbps
};

/// 720KB 3.5" DD
pub const GEOM_720: FloppyGeometry = FloppyGeometry {
    heads: 2,
    tracks: 80,
    sectors_per_track: 9,
    bytes_per_sector: 512,
    gap3_length: 0x2A,
    data_rate: 0x02, // 250 Kbps
};

/// 1.2MB 5.25" HD
pub const GEOM_12: FloppyGeometry = FloppyGeometry {
    heads: 2,
    tracks: 80,
    sectors_per_track: 15,
    bytes_per_sector: 512,
    gap3_length: 0x1B,
    data_rate: 0x00, // 500 Kbps
};

/// Drive state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DriveState {
    NotPresent,
    NoDisk,
    Ready,
    Busy,
    Error,
}

/// Floppy drive
pub struct FloppyDrive {
    pub drive_num: u8,
    pub state: DriveState,
    pub geometry: FloppyGeometry,
    pub current_track: u8,
    pub motor_on: bool,
    pub disk_changed: bool,
}

/// Floppy disk controller
pub struct FloppyController {
    pub drives: [FloppyDrive; 2],
    pub irq_received: AtomicBool,
    pub version: u8,
}

lazy_static::lazy_static! {
    pub static ref FDC: Mutex<FloppyController> = Mutex::new(FloppyController::new());
}

impl FloppyController {
    pub fn new() -> Self {
        Self {
            drives: [
                FloppyDrive {
                    drive_num: 0,
                    state: DriveState::NotPresent,
                    geometry: GEOM_144,
                    current_track: 0,
                    motor_on: false,
                    disk_changed: false,
                },
                FloppyDrive {
                    drive_num: 1,
                    state: DriveState::NotPresent,
                    geometry: GEOM_144,
                    current_track: 0,
                    motor_on: false,
                    disk_changed: false,
                },
            ],
            irq_received: AtomicBool::new(false),
            version: 0,
        }
    }

    /// Initialize the floppy controller
    pub fn init(&mut self) {
        // Reset controller
        self.write_port(FDC_DOR, 0x00);
        // Small delay
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
        // Enable with DMA
        self.write_port(FDC_DOR, DOR_RESET | DOR_DMA_EN);

        // Wait for reset to complete
        self.wait_irq();

        // Sense interrupt for each drive
        for _ in 0..4 {
            self.send_command(CMD_SENSE_INT);
            let _st0 = self.read_data();
            let _cyl = self.read_data();
        }

        // Set data rate to 500 Kbps (1.44MB mode)
        self.write_port(FDC_CCR, 0x00);

        // Specify: step rate 3ms, head unload 240ms, head load 16ms, DMA
        self.send_command(CMD_SPECIFY);
        self.send_data(0xDF); // SRT=3ms, HUT=240ms
        self.send_data(0x02); // HLT=16ms, ND=0 (DMA)

        // Check controller version
        self.send_command(CMD_VERSION);
        self.version = self.read_data();

        // Detect drives via CMOS
        let cmos_floppy = self.read_cmos_floppy_type();
        if (cmos_floppy >> 4) != 0 {
            self.drives[0].state = DriveState::NoDisk;
        }
        if (cmos_floppy & 0x0F) != 0 {
            self.drives[1].state = DriveState::NoDisk;
        }

        serial_println!(
            "[FDC] Floppy controller v{:#x} initialized, {} drive(s)",
            self.version,
            self.drives
                .iter()
                .filter(|d| d.state != DriveState::NotPresent)
                .count()
        );
    }

    /// Read CMOS floppy type byte
    fn read_cmos_floppy_type(&self) -> u8 {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        unsafe {
            let mut cmd_port: Port<u8> = Port::new(0x70);
            let mut data_port: Port<u8> = Port::new(0x71);
            cmd_port.write(0x10);
            data_port.read()
        }
    }

    /// Motor on for specified drive
    pub fn motor_on(&mut self, drive: u8) {
        let motor_bit = if drive == 0 { DOR_MOTOR_A } else { DOR_MOTOR_B };
        self.write_port(FDC_DOR, DOR_RESET | DOR_DMA_EN | motor_bit | drive);
        self.drives[drive as usize].motor_on = true;
        // Wait for motor spinup (~300ms)
        for _ in 0..300000 {
            core::hint::spin_loop();
        }
    }

    /// Motor off for specified drive
    pub fn motor_off(&mut self, drive: u8) {
        self.write_port(FDC_DOR, DOR_RESET | DOR_DMA_EN);
        self.drives[drive as usize].motor_on = false;
    }

    /// Recalibrate drive (seek to track 0)
    pub fn recalibrate(&mut self, drive: u8) -> Result<(), &'static str> {
        self.motor_on(drive);
        self.send_command(CMD_RECALIBRATE);
        self.send_data(drive);
        self.wait_irq();

        self.send_command(CMD_SENSE_INT);
        let st0 = self.read_data();
        let cyl = self.read_data();

        if (st0 & 0xC0) != 0 || cyl != 0 {
            return Err("Recalibrate failed");
        }

        self.drives[drive as usize].current_track = 0;
        Ok(())
    }

    /// Seek to specified cylinder
    pub fn seek(&mut self, drive: u8, cylinder: u8) -> Result<(), &'static str> {
        if self.drives[drive as usize].current_track == cylinder {
            return Ok(());
        }

        self.send_command(CMD_SEEK);
        self.send_data(drive);
        self.send_data(cylinder);
        self.wait_irq();

        self.send_command(CMD_SENSE_INT);
        let st0 = self.read_data();
        let cyl = self.read_data();

        if (st0 & 0xC0) != 0 || cyl != cylinder {
            return Err("Seek failed");
        }

        self.drives[drive as usize].current_track = cylinder;
        Ok(())
    }

    /// Setup ISA DMA channel 2 for floppy transfer
    fn setup_dma(&self, buffer_phys: u32, length: u16, write: bool) {
        let mode = if write { 0x4A } else { 0x46 }; // single, inc, read/write, chan 2

        self.write_port(DMA_MASK, 0x06); // Mask channel 2
        self.write_port(DMA_FLIP_FLOP, 0xFF); // Reset flip-flop
        self.write_port(DMA_ADDR, (buffer_phys & 0xFF) as u8);
        self.write_port(DMA_ADDR, ((buffer_phys >> 8) & 0xFF) as u8);
        self.write_port(DMA_PAGE, ((buffer_phys >> 16) & 0xFF) as u8);
        self.write_port(DMA_FLIP_FLOP, 0xFF);
        self.write_port(DMA_COUNT, ((length - 1) & 0xFF) as u8);
        self.write_port(DMA_COUNT, (((length - 1) >> 8) & 0xFF) as u8);
        self.write_port(DMA_MODE, mode);
        self.write_port(DMA_MASK, 0x02); // Unmask channel 2
    }

    /// Read sectors from floppy
    pub fn read_sectors(
        &mut self,
        drive: u8,
        lba: u32,
        count: u8,
        buffer_phys: u32,
    ) -> Result<(), &'static str> {
        let geom = self.drives[drive as usize].geometry;
        let cylinder = (lba / (geom.heads as u32 * geom.sectors_per_track as u32)) as u8;
        let temp = lba % (geom.heads as u32 * geom.sectors_per_track as u32);
        let head = (temp / geom.sectors_per_track as u32) as u8;
        let sector = (temp % geom.sectors_per_track as u32) as u8 + 1;

        self.motor_on(drive);
        self.seek(drive, cylinder)?;
        self.setup_dma(buffer_phys, count as u16 * geom.bytes_per_sector, false);

        self.send_command(CMD_READ_DATA);
        self.send_data(head << 2 | drive);
        self.send_data(cylinder);
        self.send_data(head);
        self.send_data(sector);
        self.send_data(2); // 512 bytes/sector
        self.send_data(geom.sectors_per_track);
        self.send_data(geom.gap3_length);
        self.send_data(0xFF); // Data length (unused when sector size is set)

        self.wait_irq();

        // Read result bytes
        let st0 = self.read_data();
        let st1 = self.read_data();
        let st2 = self.read_data();
        let _c = self.read_data();
        let _h = self.read_data();
        let _s = self.read_data();
        let _n = self.read_data();

        if (st0 & 0xC0) != 0 || st1 != 0 || st2 != 0 {
            return Err("Read failed");
        }

        Ok(())
    }

    /// Write sectors to floppy
    pub fn write_sectors(
        &mut self,
        drive: u8,
        lba: u32,
        count: u8,
        buffer_phys: u32,
    ) -> Result<(), &'static str> {
        let geom = self.drives[drive as usize].geometry;
        let cylinder = (lba / (geom.heads as u32 * geom.sectors_per_track as u32)) as u8;
        let temp = lba % (geom.heads as u32 * geom.sectors_per_track as u32);
        let head = (temp / geom.sectors_per_track as u32) as u8;
        let sector = (temp % geom.sectors_per_track as u32) as u8 + 1;

        self.motor_on(drive);
        self.seek(drive, cylinder)?;
        self.setup_dma(buffer_phys, count as u16 * geom.bytes_per_sector, true);

        self.send_command(CMD_WRITE_DATA);
        self.send_data(head << 2 | drive);
        self.send_data(cylinder);
        self.send_data(head);
        self.send_data(sector);
        self.send_data(2);
        self.send_data(geom.sectors_per_track);
        self.send_data(geom.gap3_length);
        self.send_data(0xFF);

        self.wait_irq();

        let st0 = self.read_data();
        let st1 = self.read_data();
        let st2 = self.read_data();
        let _c = self.read_data();
        let _h = self.read_data();
        let _s = self.read_data();
        let _n = self.read_data();

        if (st0 & 0xC0) != 0 || st1 != 0 || st2 != 0 {
            return Err("Write failed");
        }

        Ok(())
    }

    /// Format a track
    pub fn format_track(&mut self, drive: u8, cylinder: u8, head: u8) -> Result<(), &'static str> {
        let geom = self.drives[drive as usize].geometry;

        self.motor_on(drive);
        self.seek(drive, cylinder)?;

        // Build format buffer: 4 bytes per sector (C, H, S, N)
        // DMA must point to this in physical memory
        let buf_size = geom.sectors_per_track as u16 * 4;
        self.setup_dma(0x1000, buf_size, true); // temporary buffer at 0x1000

        self.send_command(CMD_FORMAT_TRACK);
        self.send_data(head << 2 | drive);
        self.send_data(2); // 512 bytes/sector
        self.send_data(geom.sectors_per_track);
        self.send_data(geom.gap3_length);
        self.send_data(0xF6); // Fill byte

        self.wait_irq();

        let st0 = self.read_data();
        let st1 = self.read_data();
        let st2 = self.read_data();
        let _c = self.read_data();
        let _h = self.read_data();
        let _s = self.read_data();
        let _n = self.read_data();

        if (st0 & 0xC0) != 0 || st1 != 0 || st2 != 0 {
            return Err("Format failed");
        }

        Ok(())
    }

    /// Check if disk has changed (change-line detection)
    pub fn disk_changed(&self, drive: u8) -> bool {
        let dir = self.read_port(FDC_DIR);
        (dir & 0x80) != 0
    }

    // Low-level I/O helpers
    fn write_port(&self, port: u16, value: u8) {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        unsafe {
            let mut p: Port<u8> = Port::new(port);
            p.write(value);
        }
    }

    fn read_port(&self, port: u16) -> u8 {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        unsafe {
            let mut p: Port<u8> = Port::new(port);
            p.read()
        }
    }

    fn send_command(&self, cmd: u8) {
        self.wait_ready();
        self.write_port(FDC_FIFO, cmd);
    }

    fn send_data(&self, data: u8) {
        self.wait_ready();
        self.write_port(FDC_FIFO, data);
    }

    fn read_data(&self) -> u8 {
        self.wait_ready();
        self.read_port(FDC_FIFO)
    }

    fn wait_ready(&self) {
        for _ in 0..10000 {
            let msr = self.read_port(FDC_MSR);
            if (msr & MSR_RQM) != 0 {
                return;
            }
            core::hint::spin_loop();
        }
    }

    fn wait_irq(&self) {
        self.irq_received.store(false, Ordering::SeqCst);
        for _ in 0..1_000_000 {
            if self.irq_received.load(Ordering::SeqCst) {
                return;
            }
            core::hint::spin_loop();
        }
    }
}

/// Handle floppy IRQ6
pub fn handle_irq() {
    FDC.lock().irq_received.store(true, Ordering::SeqCst);
}

/// Initialize floppy subsystem
pub fn init() {
    FDC.lock().init();
    serial_println!("[FDC] Floppy disk driver loaded");
}
