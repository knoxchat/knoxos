#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// Block Device Layer - ATA/IDE disk driver
/// Provides block device abstraction for filesystem drivers
/// Compatible with Linux's block device interface
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// Block size in bytes (standard sector)
pub const BLOCK_SIZE: usize = 512;

/// Maximum number of block devices
pub const MAX_BLOCK_DEVICES: usize = 16;

/// ATA I/O port bases
const ATA_PRIMARY_BASE: u16 = 0x1F0;
const ATA_PRIMARY_CTRL: u16 = 0x3F6;
const ATA_SECONDARY_BASE: u16 = 0x170;
const ATA_SECONDARY_CTRL: u16 = 0x376;

/// ATA register offsets
const ATA_REG_DATA: u16 = 0;
const ATA_REG_ERROR: u16 = 1;
const ATA_REG_FEATURES: u16 = 1;
const ATA_REG_SECTOR_COUNT: u16 = 2;
const ATA_REG_LBA_LO: u16 = 3;
const ATA_REG_LBA_MID: u16 = 4;
const ATA_REG_LBA_HI: u16 = 5;
const ATA_REG_DRIVE_HEAD: u16 = 6;
const ATA_REG_STATUS: u16 = 7;
const ATA_REG_COMMAND: u16 = 7;

/// ATA commands
const ATA_CMD_READ_PIO: u8 = 0x20;
const ATA_CMD_WRITE_PIO: u8 = 0x30;
const ATA_CMD_READ_PIO_EXT: u8 = 0x24;
const ATA_CMD_WRITE_PIO_EXT: u8 = 0x34;
const ATA_CMD_IDENTIFY: u8 = 0xEC;
const ATA_CMD_FLUSH: u8 = 0xE7;

/// ATA status bits
const ATA_SR_BSY: u8 = 0x80;
const ATA_SR_DRDY: u8 = 0x40;
const ATA_SR_DRQ: u8 = 0x08;
const ATA_SR_ERR: u8 = 0x01;

/// Block device error types
#[derive(Debug, Clone, Copy)]
pub enum BlockError {
    NotFound,
    IoError,
    InvalidBlock,
    ReadOnly,
    DeviceBusy,
    Timeout,
    NotSupported,
}

/// Block device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDeviceType {
    AtaPrimary,
    AtaSecondary,
    RamDisk,
    Virtio,
}

/// Block device information
#[derive(Debug, Clone)]
pub struct BlockDevice {
    pub name: String,
    pub device_type: BlockDeviceType,
    pub block_size: usize,
    pub total_blocks: u64,
    pub read_only: bool,
    pub model: String,
    pub serial: String,
}

/// ATA drive state
struct AtaDrive {
    base: u16,
    ctrl: u16,
    slave: bool,
    present: bool,
    lba48: bool,
    total_sectors: u64,
    model: String,
    serial: String,
}

/// RAM disk for testing
struct RamDisk {
    data: Vec<u8>,
    block_count: u64,
}

/// Global block device registry
lazy_static::lazy_static! {
    pub static ref BLOCK_DEVICES: Mutex<Vec<BlockDevice>> = Mutex::new(Vec::new());
    static ref ATA_DRIVES: Mutex<Vec<AtaDrive>> = Mutex::new(Vec::new());
    static ref RAM_DISKS: Mutex<Vec<RamDisk>> = Mutex::new(Vec::new());
}

impl AtaDrive {
    /// Wait for drive to be ready
    unsafe fn wait_ready(&self) -> Result<(), BlockError> {
        let mut status_port = Port::<u8>::new(self.base + ATA_REG_STATUS);

        for _ in 0..100_000 {
            let status = status_port.read();
            if status == 0 {
                return Err(BlockError::NotFound);
            }
            if (status & ATA_SR_BSY) == 0 {
                if (status & ATA_SR_ERR) != 0 {
                    return Err(BlockError::IoError);
                }
                return Ok(());
            }
        }
        Err(BlockError::Timeout)
    }

    /// Wait for data request
    unsafe fn wait_drq(&self) -> Result<(), BlockError> {
        let mut status_port = Port::<u8>::new(self.base + ATA_REG_STATUS);

        for _ in 0..100_000 {
            let status = status_port.read();
            if (status & ATA_SR_BSY) == 0 {
                if (status & ATA_SR_ERR) != 0 {
                    return Err(BlockError::IoError);
                }
                if (status & ATA_SR_DRQ) != 0 {
                    return Ok(());
                }
            }
        }
        Err(BlockError::Timeout)
    }

    /// Delay by reading alternate status register
    unsafe fn delay_400ns(&self) {
        let mut ctrl_port = Port::<u8>::new(self.ctrl);
        for _ in 0..4 {
            ctrl_port.read();
        }
    }

    /// Select this drive
    unsafe fn select(&self) {
        let mut drive_port = Port::<u8>::new(self.base + ATA_REG_DRIVE_HEAD);
        let drive_val = if self.slave { 0xB0 } else { 0xA0 };
        drive_port.write(drive_val);
        self.delay_400ns();
    }

    /// Read sectors using PIO mode (LBA28)
    pub fn read_sectors(&self, lba: u64, count: u8, buf: &mut [u8]) -> Result<(), BlockError> {
        if lba + count as u64 > self.total_sectors {
            return Err(BlockError::InvalidBlock);
        }
        if buf.len() < (count as usize) * BLOCK_SIZE {
            return Err(BlockError::InvalidBlock);
        }

        unsafe {
            self.select();
            self.wait_ready()?;

            let mut sc_port = Port::<u8>::new(self.base + ATA_REG_SECTOR_COUNT);
            let mut lba_lo = Port::<u8>::new(self.base + ATA_REG_LBA_LO);
            let mut lba_mid = Port::<u8>::new(self.base + ATA_REG_LBA_MID);
            let mut lba_hi = Port::<u8>::new(self.base + ATA_REG_LBA_HI);
            let mut drive_port = Port::<u8>::new(self.base + ATA_REG_DRIVE_HEAD);
            let mut cmd_port = Port::<u8>::new(self.base + ATA_REG_COMMAND);
            let mut data_port = Port::<u16>::new(self.base + ATA_REG_DATA);

            sc_port.write(count);
            lba_lo.write((lba & 0xFF) as u8);
            lba_mid.write(((lba >> 8) & 0xFF) as u8);
            lba_hi.write(((lba >> 16) & 0xFF) as u8);

            let drive_val =
                0xE0 | if self.slave { 0x10 } else { 0x00 } | ((lba >> 24) & 0x0F) as u8;
            drive_port.write(drive_val);

            cmd_port.write(ATA_CMD_READ_PIO);

            for sector in 0..count as usize {
                self.wait_drq()?;

                let offset = sector * BLOCK_SIZE;
                for i in 0..(BLOCK_SIZE / 2) {
                    let word = data_port.read();
                    buf[offset + i * 2] = (word & 0xFF) as u8;
                    buf[offset + i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
                }
            }
        }

        Ok(())
    }

    /// Write sectors using PIO mode (LBA28)
    pub fn write_sectors(&self, lba: u64, count: u8, buf: &[u8]) -> Result<(), BlockError> {
        if lba + count as u64 > self.total_sectors {
            return Err(BlockError::InvalidBlock);
        }
        if buf.len() < (count as usize) * BLOCK_SIZE {
            return Err(BlockError::InvalidBlock);
        }

        unsafe {
            self.select();
            self.wait_ready()?;

            let mut sc_port = Port::<u8>::new(self.base + ATA_REG_SECTOR_COUNT);
            let mut lba_lo = Port::<u8>::new(self.base + ATA_REG_LBA_LO);
            let mut lba_mid = Port::<u8>::new(self.base + ATA_REG_LBA_MID);
            let mut lba_hi = Port::<u8>::new(self.base + ATA_REG_LBA_HI);
            let mut drive_port = Port::<u8>::new(self.base + ATA_REG_DRIVE_HEAD);
            let mut cmd_port = Port::<u8>::new(self.base + ATA_REG_COMMAND);
            let mut data_port = Port::<u16>::new(self.base + ATA_REG_DATA);

            sc_port.write(count);
            lba_lo.write((lba & 0xFF) as u8);
            lba_mid.write(((lba >> 8) & 0xFF) as u8);
            lba_hi.write(((lba >> 16) & 0xFF) as u8);

            let drive_val =
                0xE0 | if self.slave { 0x10 } else { 0x00 } | ((lba >> 24) & 0x0F) as u8;
            drive_port.write(drive_val);

            cmd_port.write(ATA_CMD_WRITE_PIO);

            for sector in 0..count as usize {
                self.wait_drq()?;

                let offset = sector * BLOCK_SIZE;
                for i in 0..(BLOCK_SIZE / 2) {
                    let word =
                        (buf[offset + i * 2] as u16) | ((buf[offset + i * 2 + 1] as u16) << 8);
                    data_port.write(word);
                }

                // Flush after each sector
                let mut flush_port = Port::<u8>::new(self.base + ATA_REG_COMMAND);
                flush_port.write(ATA_CMD_FLUSH);
                self.wait_ready()?;
            }
        }

        Ok(())
    }
}

impl RamDisk {
    fn new(size_bytes: usize) -> Self {
        let block_count = size_bytes.div_ceil(BLOCK_SIZE);
        Self {
            data: vec![0u8; block_count * BLOCK_SIZE],
            block_count: block_count as u64,
        }
    }

    fn read(&self, lba: u64, count: u8, buf: &mut [u8]) -> Result<(), BlockError> {
        let start = lba as usize * BLOCK_SIZE;
        let end = start + count as usize * BLOCK_SIZE;
        if end > self.data.len() {
            return Err(BlockError::InvalidBlock);
        }
        buf[..end - start].copy_from_slice(&self.data[start..end]);
        Ok(())
    }

    fn write(&mut self, lba: u64, count: u8, buf: &[u8]) -> Result<(), BlockError> {
        let start = lba as usize * BLOCK_SIZE;
        let end = start + count as usize * BLOCK_SIZE;
        if end > self.data.len() {
            return Err(BlockError::InvalidBlock);
        }
        self.data[start..end].copy_from_slice(&buf[..end - start]);
        Ok(())
    }
}

/// Identify an ATA drive
unsafe fn identify_drive(base: u16, ctrl: u16, slave: bool) -> Option<AtaDrive> {
    let mut drive = AtaDrive {
        base,
        ctrl,
        slave,
        present: false,
        lba48: false,
        total_sectors: 0,
        model: String::new(),
        serial: String::new(),
    };

    drive.select();

    // Clear sector count and LBA registers
    let mut sc = Port::<u8>::new(base + ATA_REG_SECTOR_COUNT);
    let mut lo = Port::<u8>::new(base + ATA_REG_LBA_LO);
    let mut mid = Port::<u8>::new(base + ATA_REG_LBA_MID);
    let mut hi = Port::<u8>::new(base + ATA_REG_LBA_HI);
    sc.write(0);
    lo.write(0);
    mid.write(0);
    hi.write(0);

    // Send IDENTIFY command
    let mut cmd = Port::<u8>::new(base + ATA_REG_COMMAND);
    cmd.write(ATA_CMD_IDENTIFY);

    // Check if drive exists
    let mut status_port = Port::<u8>::new(base + ATA_REG_STATUS);
    let status = status_port.read();
    if status == 0 {
        return None; // No drive
    }

    // Wait for BSY to clear
    let mut timeout = 100_000u32;
    loop {
        let s = status_port.read();
        if (s & ATA_SR_BSY) == 0 {
            break;
        }
        timeout -= 1;
        if timeout == 0 {
            return None;
        }
    }

    // Check for ATAPI (not ATA)
    let mid_val = mid.read();
    let hi_val = hi.read();
    if mid_val != 0 || hi_val != 0 {
        return None; // ATAPI device, not ATA
    }

    // Wait for DRQ or ERR
    loop {
        let s = status_port.read();
        if (s & ATA_SR_ERR) != 0 {
            return None;
        }
        if (s & ATA_SR_DRQ) != 0 {
            break;
        }
    }

    // Read 256 words of identification data
    let mut data = [0u16; 256];
    let mut data_port = Port::<u16>::new(base + ATA_REG_DATA);
    for item in &mut data {
        *item = data_port.read();
    }

    // Parse identification data
    drive.present = true;

    // LBA48 support (word 83, bit 10)
    drive.lba48 = (data[83] & (1 << 10)) != 0;

    // Total sectors
    if drive.lba48 {
        drive.total_sectors = (data[100] as u64)
            | ((data[101] as u64) << 16)
            | ((data[102] as u64) << 32)
            | ((data[103] as u64) << 48);
    } else {
        drive.total_sectors = (data[60] as u64) | ((data[61] as u64) << 16);
    }

    // Model string (words 27-46, byte-swapped)
    let mut model_bytes = [0u8; 40];
    for i in 0..20 {
        model_bytes[i * 2] = (data[27 + i] >> 8) as u8;
        model_bytes[i * 2 + 1] = (data[27 + i] & 0xFF) as u8;
    }
    drive.model = String::from_utf8_lossy(&model_bytes).trim().into();

    // Serial number (words 10-19, byte-swapped)
    let mut serial_bytes = [0u8; 20];
    for i in 0..10 {
        serial_bytes[i * 2] = (data[10 + i] >> 8) as u8;
        serial_bytes[i * 2 + 1] = (data[10 + i] & 0xFF) as u8;
    }
    drive.serial = String::from_utf8_lossy(&serial_bytes).trim().into();

    Some(drive)
}

/// Read blocks from a device
pub fn read_blocks(
    device_index: usize,
    lba: u64,
    count: u8,
    buf: &mut [u8],
) -> Result<(), BlockError> {
    let devices = BLOCK_DEVICES.lock();
    let dev = devices.get(device_index).ok_or(BlockError::NotFound)?;

    match dev.device_type {
        BlockDeviceType::AtaPrimary | BlockDeviceType::AtaSecondary => {
            let drives = ATA_DRIVES.lock();
            let drive_idx = if dev.device_type == BlockDeviceType::AtaPrimary {
                0
            } else {
                1
            };
            if let Some(drive) = drives.get(drive_idx) {
                drive.read_sectors(lba, count, buf)
            } else {
                Err(BlockError::NotFound)
            }
        }
        BlockDeviceType::RamDisk => {
            let disks = RAM_DISKS.lock();
            if let Some(disk) = disks.first() {
                disk.read(lba, count, buf)
            } else {
                Err(BlockError::NotFound)
            }
        }
        BlockDeviceType::Virtio => {
            drop(devices);
            // Delegate to virtio-blk driver
            let mut vbuf = alloc::vec![0u8; count as usize * BLOCK_SIZE];
            if crate::virtio_blk::read(lba, count as usize, &mut vbuf) {
                buf[..vbuf.len()].copy_from_slice(&vbuf);
                Ok(())
            } else {
                Err(BlockError::IoError)
            }
        }
    }
}

/// Write blocks to a device
pub fn write_blocks(
    device_index: usize,
    lba: u64,
    count: u8,
    buf: &[u8],
) -> Result<(), BlockError> {
    let devices = BLOCK_DEVICES.lock();
    let dev = devices.get(device_index).ok_or(BlockError::NotFound)?;

    if dev.read_only {
        return Err(BlockError::ReadOnly);
    }

    match dev.device_type {
        BlockDeviceType::AtaPrimary | BlockDeviceType::AtaSecondary => {
            let drives = ATA_DRIVES.lock();
            let drive_idx = if dev.device_type == BlockDeviceType::AtaPrimary {
                0
            } else {
                1
            };
            if let Some(drive) = drives.get(drive_idx) {
                drive.write_sectors(lba, count, buf)
            } else {
                Err(BlockError::NotFound)
            }
        }
        BlockDeviceType::RamDisk => {
            let mut disks = RAM_DISKS.lock();
            if let Some(disk) = disks.first_mut() {
                disk.write(lba, count, buf)
            } else {
                Err(BlockError::NotFound)
            }
        }
        BlockDeviceType::Virtio => {
            drop(devices);
            // Delegate to virtio-blk driver
            if crate::virtio_blk::write(lba, count as usize, buf) {
                Ok(())
            } else {
                Err(BlockError::IoError)
            }
        }
    }
}

/// List all block devices
pub fn list_devices() -> Vec<BlockDevice> {
    BLOCK_DEVICES.lock().clone()
}

/// Create a RAM disk
pub fn create_ramdisk(size_mb: usize) -> usize {
    let disk = RamDisk::new(size_mb * 1024 * 1024);
    let block_count = disk.block_count;

    let mut disks = RAM_DISKS.lock();
    disks.push(disk);
    let disk_index = disks.len() - 1;
    drop(disks);

    let mut devices = BLOCK_DEVICES.lock();
    let dev_index = devices.len();
    devices.push(BlockDevice {
        name: alloc::format!("ram{}", disk_index),
        device_type: BlockDeviceType::RamDisk,
        block_size: BLOCK_SIZE,
        total_blocks: block_count,
        read_only: false,
        model: String::from("KnoxOS RAM Disk"),
        serial: alloc::format!("RAMDISK-{}", disk_index),
    });

    crate::serial_println!(
        "[KnoxOS] Created RAM disk ram{}: {} MB ({} blocks)",
        disk_index,
        size_mb,
        block_count
    );

    dev_index
}

/// Initialize block device subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] Scanning ATA/IDE drives...");

    // Probe primary master/slave
    let configs = [
        (ATA_PRIMARY_BASE, ATA_PRIMARY_CTRL, false, "Primary Master"),
        (ATA_PRIMARY_BASE, ATA_PRIMARY_CTRL, true, "Primary Slave"),
        (
            ATA_SECONDARY_BASE,
            ATA_SECONDARY_CTRL,
            false,
            "Secondary Master",
        ),
        (
            ATA_SECONDARY_BASE,
            ATA_SECONDARY_CTRL,
            true,
            "Secondary Slave",
        ),
    ];

    let mut drive_count = 0;
    for (base, ctrl, slave, name) in configs.iter() {
        if let Some(drive) = unsafe { identify_drive(*base, *ctrl, *slave) } {
            let size_mb = drive.total_sectors * BLOCK_SIZE as u64 / (1024 * 1024);
            crate::serial_println!(
                "[KnoxOS]   {} ({}): {} - {} MB, {} sectors{}",
                name,
                if *slave { "slave" } else { "master" },
                drive.model,
                size_mb,
                drive.total_sectors,
                if drive.lba48 { " (LBA48)" } else { " (LBA28)" }
            );

            let device_type = if *base == ATA_PRIMARY_BASE {
                BlockDeviceType::AtaPrimary
            } else {
                BlockDeviceType::AtaSecondary
            };

            let mut devices = BLOCK_DEVICES.lock();
            devices.push(BlockDevice {
                name: alloc::format!("sd{}", (b'a' + drive_count) as char),
                device_type,
                block_size: BLOCK_SIZE,
                total_blocks: drive.total_sectors,
                read_only: false,
                model: drive.model.clone(),
                serial: drive.serial.clone(),
            });

            ATA_DRIVES.lock().push(drive);
            drive_count += 1;
        }
    }

    if drive_count == 0 {
        crate::serial_println!("[KnoxOS]   No ATA drives detected");
    }

    // Create a 4MB RAM disk for testing
    create_ramdisk(4);

    crate::serial_println!(
        "[KnoxOS] Block device layer initialized ({} devices)",
        drive_count + 1
    );
}
