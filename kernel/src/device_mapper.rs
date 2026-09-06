/// Device Mapper — Virtual block device layer
///
/// Provides a generic framework for creating virtual block devices that
/// map I/O requests to underlying physical devices. Used by LVM, dm-crypt,
/// dm-raid, dm-cache, multipath, and other subsystems.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Target types for device mapper
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmTargetType {
    /// Linear mapping to an underlying device
    Linear,
    /// Striped across multiple devices
    Striped,
    /// Mirror (RAID 1)
    Mirror,
    /// Encryption (dm-crypt)
    Crypt,
    /// Snapshot (CoW)
    Snapshot,
    /// Snapshot origin
    SnapshotOrigin,
    /// Zero target (returns zeroes on read, discards writes)
    Zero,
    /// Error target (returns errors)
    Error,
    /// Cache (dm-cache, SSD caching of HDD)
    Cache,
    /// Thin provisioning pool
    ThinPool,
    /// Thin provisioned volume
    Thin,
    /// Multipath (multiple I/O paths to same device)
    Multipath,
    /// Integrity (dm-integrity)
    Integrity,
    /// Verity (dm-verity, read-only verified blocks)
    Verity,
}

/// A mapping table entry
#[derive(Debug, Clone)]
pub struct DmTableEntry {
    /// Start sector in the virtual device
    pub start_sector: u64,
    /// Length in sectors
    pub length: u64,
    /// Target type
    pub target_type: DmTargetType,
    /// Target-specific arguments
    pub target_args: String,
    /// Underlying device path (if applicable)
    pub dest_device: Option<String>,
    /// Offset on the destination device
    pub dest_offset: u64,
}

/// A device mapper device
#[derive(Debug, Clone)]
pub struct DmDevice {
    pub name: String,
    pub uuid: Option<String>,
    pub major: u32,
    pub minor: u32,
    pub table: Vec<DmTableEntry>,
    pub suspended: bool,
    pub read_only: bool,
    pub open_count: u32,
    pub event_number: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref DEVICES: Mutex<BTreeMap<String, DmDevice>> = Mutex::new(BTreeMap::new());
}

static NEXT_MINOR: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

// ═══════════════════════════════════════════════════════════════════════
// DEVICE MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Create a new device mapper device
pub fn dm_create(name: &str, uuid: Option<&str>) -> Result<(), &'static str> {
    let mut devs = DEVICES.lock();
    if devs.contains_key(name) {
        return Err("Device already exists");
    }

    let minor = NEXT_MINOR.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let dev = DmDevice {
        name: String::from(name),
        uuid: uuid.map(String::from),
        major: 253, // dm major number
        minor,
        table: Vec::new(),
        suspended: true, // starts suspended until table is loaded
        read_only: false,
        open_count: 0,
        event_number: 0,
    };

    serial_println!("[dm] Created device '{}' (253:{})", name, minor);
    devs.insert(String::from(name), dev);
    Ok(())
}

/// Load a mapping table into a device
pub fn dm_load(name: &str, table: Vec<DmTableEntry>) -> Result<(), &'static str> {
    let mut devs = DEVICES.lock();
    let dev = devs.get_mut(name).ok_or("Device not found")?;

    if !dev.suspended {
        return Err("Device must be suspended to load table");
    }

    serial_println!("[dm] Loading {} table entries into '{}'", table.len(), name);
    dev.table = table;
    Ok(())
}

/// Resume (activate) a device
pub fn dm_resume(name: &str) -> Result<(), &'static str> {
    let mut devs = DEVICES.lock();
    let dev = devs.get_mut(name).ok_or("Device not found")?;
    dev.suspended = false;
    serial_println!("[dm] Resumed device '{}'", name);
    Ok(())
}

/// Suspend a device
pub fn dm_suspend(name: &str) -> Result<(), &'static str> {
    let mut devs = DEVICES.lock();
    let dev = devs.get_mut(name).ok_or("Device not found")?;
    dev.suspended = true;
    serial_println!("[dm] Suspended device '{}'", name);
    Ok(())
}

/// Remove a device
pub fn dm_remove(name: &str) -> Result<(), &'static str> {
    let mut devs = DEVICES.lock();
    let dev = devs.get(name).ok_or("Device not found")?;
    if dev.open_count > 0 {
        return Err("Device is in use");
    }
    devs.remove(name);
    serial_println!("[dm] Removed device '{}'", name);
    Ok(())
}

/// List all dm devices
pub fn dm_list() -> Vec<(String, u32, u32, bool)> {
    DEVICES
        .lock()
        .values()
        .map(|d| (d.name.clone(), d.major, d.minor, d.suspended))
        .collect()
}

/// Get table for a device
pub fn dm_table(name: &str) -> Result<Vec<DmTableEntry>, &'static str> {
    let devs = DEVICES.lock();
    let dev = devs.get(name).ok_or("Device not found")?;
    Ok(dev.table.clone())
}

// ═══════════════════════════════════════════════════════════════════════
// I/O MAPPING
// ═══════════════════════════════════════════════════════════════════════

/// Map a sector through the device mapper table
pub fn dm_map_sector(name: &str, sector: u64) -> Result<(String, u64), &'static str> {
    let devs = DEVICES.lock();
    let dev = devs.get(name).ok_or("Device not found")?;

    if dev.suspended {
        return Err("Device is suspended");
    }

    for entry in &dev.table {
        if sector >= entry.start_sector && sector < entry.start_sector + entry.length {
            let offset = sector - entry.start_sector;

            match entry.target_type {
                DmTargetType::Linear => {
                    let dest = entry.dest_device.as_ref().ok_or("No destination device")?;
                    return Ok((dest.clone(), entry.dest_offset + offset));
                }
                DmTargetType::Zero => {
                    return Ok((String::from("/dev/zero"), 0));
                }
                DmTargetType::Error => {
                    return Err("I/O error (error target)");
                }
                _ => {
                    let dest = entry.dest_device.as_ref().ok_or("No destination device")?;
                    return Ok((dest.clone(), entry.dest_offset + offset));
                }
            }
        }
    }

    Err("Sector not mapped")
}

// ═══════════════════════════════════════════════════════════════════════
// TARGET HELPERS
// ═══════════════════════════════════════════════════════════════════════

/// Create a linear mapping table entry
pub fn linear_target(start: u64, length: u64, dest_dev: &str, dest_offset: u64) -> DmTableEntry {
    DmTableEntry {
        start_sector: start,
        length,
        target_type: DmTargetType::Linear,
        target_args: alloc::format!("{} {}", dest_dev, dest_offset),
        dest_device: Some(String::from(dest_dev)),
        dest_offset,
    }
}

/// Create a zero target entry
pub fn zero_target(start: u64, length: u64) -> DmTableEntry {
    DmTableEntry {
        start_sector: start,
        length,
        target_type: DmTargetType::Zero,
        target_args: String::new(),
        dest_device: None,
        dest_offset: 0,
    }
}

/// Create a verity target entry
pub fn verity_target(
    start: u64,
    length: u64,
    data_dev: &str,
    hash_dev: &str,
    root_hash: &str,
) -> DmTableEntry {
    DmTableEntry {
        start_sector: start,
        length,
        target_type: DmTargetType::Verity,
        target_args: alloc::format!(
            "1 {} {} 4096 4096 {} {} sha256 {}",
            data_dev,
            hash_dev,
            length / 8,
            length / 8,
            root_hash
        ),
        dest_device: Some(String::from(data_dev)),
        dest_offset: 0,
    }
}

/// Initialize the device mapper subsystem
pub fn init() {
    serial_println!("[dm] Device mapper framework initialized");
}
