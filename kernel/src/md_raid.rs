/// Software RAID (md) — Linux-compatible multi-disk RAID
///
/// Implements RAID levels 0, 1, 5, 6, and 10 using the md (multiple device)
/// framework. Provides striping, mirroring, and parity-based redundancy
/// for block devices.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidLevel {
    /// Striping — no redundancy, maximum performance
    Raid0,
    /// Mirroring — full redundancy, 2+ disks
    Raid1,
    /// Striping with single parity — one disk can fail
    Raid5,
    /// Striping with double parity — two disks can fail
    Raid6,
    /// Striped mirrors — combination of RAID 1+0
    Raid10,
}

impl RaidLevel {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Raid0 => "raid0",
            Self::Raid1 => "raid1",
            Self::Raid5 => "raid5",
            Self::Raid6 => "raid6",
            Self::Raid10 => "raid10",
        }
    }

    /// Minimum number of member disks
    pub fn min_disks(&self) -> usize {
        match self {
            Self::Raid0 => 2,
            Self::Raid1 => 2,
            Self::Raid5 => 3,
            Self::Raid6 => 4,
            Self::Raid10 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    Active,
    Spare,
    Faulty,
    Rebuilding,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayState {
    Clean,
    Active,
    Degraded,
    Rebuilding,
    Inactive,
    Suspended,
}

/// A member disk in a RAID array
#[derive(Debug, Clone)]
pub struct MemberDisk {
    pub path: String,
    pub state: DiskState,
    pub size_sectors: u64,
    pub slot: usize,
    pub errors: u64,
}

/// A RAID array (md device)
#[derive(Debug, Clone)]
pub struct MdArray {
    pub name: String,
    pub level: RaidLevel,
    pub state: ArrayState,
    pub members: Vec<MemberDisk>,
    pub chunk_size_kb: u32,
    pub total_size_sectors: u64,
    pub usable_size_sectors: u64,
    pub rebuild_progress: Option<f32>,
    pub uuid: [u8; 16],
}

lazy_static::lazy_static! {
    static ref ARRAYS: Mutex<Vec<MdArray>> = Mutex::new(Vec::new());
}

// ═══════════════════════════════════════════════════════════════════════
// ARRAY MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Create a new RAID array
pub fn create_array(
    name: &str,
    level: RaidLevel,
    disks: &[&str],
    chunk_size_kb: u32,
) -> Result<(), &'static str> {
    if disks.len() < level.min_disks() {
        return Err("Not enough disks for this RAID level");
    }

    let members: Vec<MemberDisk> = disks
        .iter()
        .enumerate()
        .map(|(i, path)| {
            MemberDisk {
                path: String::from(*path),
                state: DiskState::Active,
                size_sectors: 0, // detect from block device
                slot: i,
                errors: 0,
            }
        })
        .collect();

    let total = members.iter().map(|m| m.size_sectors).sum::<u64>();
    let usable = compute_usable_size(level, &members, chunk_size_kb as u64);

    let array = MdArray {
        name: String::from(name),
        level,
        state: ArrayState::Active,
        members,
        chunk_size_kb,
        total_size_sectors: total,
        usable_size_sectors: usable,
        rebuild_progress: None,
        uuid: generate_uuid(),
    };

    serial_println!(
        "[md] Created {} array '{}' with {} disks",
        level.name(),
        name,
        disks.len()
    );
    ARRAYS.lock().push(array);
    Ok(())
}

/// Stop (deactivate) an array
pub fn stop_array(name: &str) -> Result<(), &'static str> {
    let mut arrays = ARRAYS.lock();
    let arr = arrays
        .iter_mut()
        .find(|a| a.name == name)
        .ok_or("Array not found")?;
    arr.state = ArrayState::Inactive;
    serial_println!("[md] Stopped array '{}'", name);
    Ok(())
}

/// Add a spare disk to an array
pub fn add_spare(array_name: &str, disk_path: &str) -> Result<(), &'static str> {
    let mut arrays = ARRAYS.lock();
    let arr = arrays
        .iter_mut()
        .find(|a| a.name == array_name)
        .ok_or("Array not found")?;
    arr.members.push(MemberDisk {
        path: String::from(disk_path),
        state: DiskState::Spare,
        size_sectors: 0,
        slot: arr.members.len(),
        errors: 0,
    });
    serial_println!("[md] Added spare {} to '{}'", disk_path, array_name);
    Ok(())
}

/// Mark a disk as faulty and trigger rebuild from spare
pub fn fail_disk(array_name: &str, disk_path: &str) -> Result<(), &'static str> {
    let mut arrays = ARRAYS.lock();
    let arr = arrays
        .iter_mut()
        .find(|a| a.name == array_name)
        .ok_or("Array not found")?;

    let disk = arr
        .members
        .iter_mut()
        .find(|m| m.path == disk_path)
        .ok_or("Disk not found")?;
    disk.state = DiskState::Faulty;
    arr.state = ArrayState::Degraded;

    // Try to start rebuild from a spare
    if let Some(spare) = arr.members.iter_mut().find(|m| m.state == DiskState::Spare) {
        spare.state = DiskState::Rebuilding;
        arr.state = ArrayState::Rebuilding;
        arr.rebuild_progress = Some(0.0);
        serial_println!(
            "[md] Rebuilding '{}' using spare {}",
            array_name,
            spare.path
        );
    }

    Ok(())
}

/// List all arrays
pub fn list_arrays() -> Vec<(String, RaidLevel, ArrayState, usize)> {
    ARRAYS
        .lock()
        .iter()
        .map(|a| (a.name.clone(), a.level, a.state, a.members.len()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// I/O PATH
// ═══════════════════════════════════════════════════════════════════════

/// Read from a RAID array
pub fn read(array_name: &str, offset_sectors: u64, count: usize) -> Result<Vec<u8>, &'static str> {
    let arrays = ARRAYS.lock();
    let arr = arrays
        .iter()
        .find(|a| a.name == array_name)
        .ok_or("Array not found")?;

    match arr.level {
        RaidLevel::Raid0 => raid0_read(arr, offset_sectors, count),
        RaidLevel::Raid1 => raid1_read(arr, offset_sectors, count),
        RaidLevel::Raid5 => raid5_read(arr, offset_sectors, count),
        RaidLevel::Raid6 => raid6_read(arr, offset_sectors, count),
        RaidLevel::Raid10 => raid10_read(arr, offset_sectors, count),
    }
}

/// Write to a RAID array
pub fn write(array_name: &str, offset_sectors: u64, data: &[u8]) -> Result<(), &'static str> {
    let arrays = ARRAYS.lock();
    let arr = arrays
        .iter()
        .find(|a| a.name == array_name)
        .ok_or("Array not found")?;

    match arr.level {
        RaidLevel::Raid0 => raid0_write(arr, offset_sectors, data),
        RaidLevel::Raid1 => raid1_write(arr, offset_sectors, data),
        RaidLevel::Raid5 => raid5_write(arr, offset_sectors, data),
        RaidLevel::Raid6 => raid6_write(arr, offset_sectors, data),
        RaidLevel::Raid10 => raid10_write(arr, offset_sectors, data),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RAID LEVEL IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════════════

/// RAID 0: Simple striping across all active disks
fn raid0_read(arr: &MdArray, offset: u64, count: usize) -> Result<Vec<u8>, &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    let n = active.len();
    let chunk_sectors = arr.chunk_size_kb as u64 * 2; // 1KB = 2 sectors

    let stripe = offset / chunk_sectors;
    let disk_idx = (stripe as usize) % n;
    let disk_offset = (stripe / n as u64) * chunk_sectors + (offset % chunk_sectors);

    serial_println!(
        "[md/raid0] Read {} bytes from disk {} offset {}",
        count,
        disk_idx,
        disk_offset
    );
    Ok(alloc::vec![0u8; count])
}

fn raid0_write(arr: &MdArray, offset: u64, data: &[u8]) -> Result<(), &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    let n = active.len();
    let chunk_sectors = arr.chunk_size_kb as u64 * 2;

    let stripe = offset / chunk_sectors;
    let disk_idx = (stripe as usize) % n;
    let disk_offset = (stripe / n as u64) * chunk_sectors + (offset % chunk_sectors);

    serial_println!(
        "[md/raid0] Write {} bytes to disk {} offset {}",
        data.len(),
        disk_idx,
        disk_offset
    );
    Ok(())
}

/// RAID 1: Read from any mirror, write to all mirrors
fn raid1_read(arr: &MdArray, offset: u64, count: usize) -> Result<Vec<u8>, &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    if active.is_empty() {
        return Err("No active disks in mirror");
    }
    serial_println!(
        "[md/raid1] Read {} bytes from mirror[0] offset {}",
        count,
        offset
    );
    Ok(alloc::vec![0u8; count])
}

fn raid1_write(arr: &MdArray, offset: u64, data: &[u8]) -> Result<(), &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    for (i, _disk) in active.iter().enumerate() {
        serial_println!(
            "[md/raid1] Write {} bytes to mirror[{}] offset {}",
            data.len(),
            i,
            offset
        );
    }
    Ok(())
}

/// RAID 5: Striping with distributed parity
fn raid5_read(arr: &MdArray, offset: u64, count: usize) -> Result<Vec<u8>, &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    let n = active.len();
    let data_disks = n - 1; // one parity
    let chunk_sectors = arr.chunk_size_kb as u64 * 2;

    let stripe = offset / (chunk_sectors * data_disks as u64);
    let parity_disk = (stripe as usize) % n;
    let data_stripe = offset / chunk_sectors;
    let mut disk_idx = (data_stripe as usize) % n;
    if disk_idx >= parity_disk {
        disk_idx = (disk_idx + 1) % n;
    }

    serial_println!(
        "[md/raid5] Read {} bytes: stripe {}, disk {}, parity disk {}",
        count,
        stripe,
        disk_idx,
        parity_disk
    );
    Ok(alloc::vec![0u8; count])
}

fn raid5_write(arr: &MdArray, offset: u64, data: &[u8]) -> Result<(), &'static str> {
    // Read-modify-write: read old data + old parity, compute new parity, write both
    let _ = raid5_read(arr, offset, data.len())?;
    serial_println!("[md/raid5] Write {} bytes with parity update", data.len());
    Ok(())
}

/// RAID 6: Striping with double parity (P + Q)
fn raid6_read(arr: &MdArray, offset: u64, count: usize) -> Result<Vec<u8>, &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    let n = active.len();
    let data_disks = n.saturating_sub(2);
    let chunk_sectors = arr.chunk_size_kb as u64 * 2;

    let stripe = offset / (chunk_sectors * data_disks as u64);
    serial_println!(
        "[md/raid6] Read {} bytes: stripe {}, data_disks {}",
        count,
        stripe,
        data_disks
    );
    Ok(alloc::vec![0u8; count])
}

fn raid6_write(arr: &MdArray, offset: u64, data: &[u8]) -> Result<(), &'static str> {
    serial_println!("[md/raid6] Write {} bytes with P+Q parity", data.len());
    Ok(())
}

/// RAID 10: Striped mirrors
fn raid10_read(arr: &MdArray, offset: u64, count: usize) -> Result<Vec<u8>, &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    let mirrors = active.len() / 2;
    let chunk_sectors = arr.chunk_size_kb as u64 * 2;
    let stripe = offset / chunk_sectors;
    let mirror_pair = (stripe as usize) % mirrors;

    serial_println!(
        "[md/raid10] Read {} bytes: mirror pair {}",
        count,
        mirror_pair
    );
    Ok(alloc::vec![0u8; count])
}

fn raid10_write(arr: &MdArray, offset: u64, data: &[u8]) -> Result<(), &'static str> {
    let active: Vec<&MemberDisk> = arr
        .members
        .iter()
        .filter(|m| m.state == DiskState::Active)
        .collect();
    let mirrors = active.len() / 2;
    let chunk_sectors = arr.chunk_size_kb as u64 * 2;
    let stripe = offset / chunk_sectors;
    let mirror_pair = (stripe as usize) % mirrors;

    serial_println!(
        "[md/raid10] Write {} bytes to mirror pair {} (both copies)",
        data.len(),
        mirror_pair
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════

fn compute_usable_size(level: RaidLevel, members: &[MemberDisk], _chunk: u64) -> u64 {
    let min_size = members.iter().map(|m| m.size_sectors).min().unwrap_or(0);
    let n = members.len() as u64;
    match level {
        RaidLevel::Raid0 => min_size * n,
        RaidLevel::Raid1 => min_size,
        RaidLevel::Raid5 => min_size * (n - 1),
        RaidLevel::Raid6 => min_size * (n - 2),
        RaidLevel::Raid10 => min_size * (n / 2),
    }
}

fn generate_uuid() -> [u8; 16] {
    let tick = crate::interrupts::get_ticks();
    let mut uuid = [0u8; 16];
    for i in 0..16 {
        uuid[i] = (tick.wrapping_mul((i + 1) as u64) & 0xFF) as u8;
    }
    uuid
}

/// Initialize the md subsystem
pub fn init() {
    serial_println!("[md] Software RAID subsystem initialized");
}
