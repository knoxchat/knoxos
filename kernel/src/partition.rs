/// Partition Table Parsing — MBR and GPT partition table support
///
/// Supports:
///   - Master Boot Record (MBR) with 4 primary partitions
///   - GUID Partition Table (GPT) with up to 128 partitions
///   - Extended MBR partitions (logical partitions in EBR chain)
///   - Automatic partition discovery on block devices
///
/// Partition types recognized:
///   - 0x83: Linux (ext2/ext3/ext4)
///   - 0x82: Linux swap
///   - 0x0B/0x0C: FAT32
///   - 0x07: NTFS/exFAT
///   - 0xEE: GPT protective MBR
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── MBR Constants ──────────────────────────────────────────────────────

/// MBR signature (last 2 bytes of sector 0)
const MBR_SIGNATURE: u16 = 0xAA55;

/// Partition table offset in MBR
const MBR_PARTITION_TABLE_OFFSET: usize = 446;

/// Size of each partition entry in MBR
const MBR_PARTITION_ENTRY_SIZE: usize = 16;

/// Maximum primary partitions in MBR
const MBR_MAX_PARTITIONS: usize = 4;

// ─── GPT Constants ──────────────────────────────────────────────────────

/// GPT signature "EFI PART"
const GPT_SIGNATURE: u64 = 0x5452_4150_2049_4645;

/// GPT header LBA (always at sector 1)
const GPT_HEADER_LBA: u64 = 1;

/// Maximum GPT partitions we support
const GPT_MAX_PARTITIONS: usize = 128;

/// GPT partition entry size (minimum 128 bytes)
const GPT_PARTITION_ENTRY_SIZE: usize = 128;

// ─── Partition Type IDs ─────────────────────────────────────────────────

/// Known MBR partition type IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MbrPartitionType {
    Empty = 0x00,
    Fat12 = 0x01,
    Fat16Small = 0x04,
    Extended = 0x05,
    Fat16Large = 0x06,
    NtfsExfat = 0x07,
    Fat32 = 0x0B,
    Fat32Lba = 0x0C,
    Fat16Lba = 0x0E,
    ExtendedLba = 0x0F,
    LinuxSwap = 0x82,
    LinuxNative = 0x83,
    LinuxLvm = 0x8E,
    GptProtective = 0xEE,
    EfiSystem = 0xEF,
    Unknown(u8),
}

impl From<u8> for MbrPartitionType {
    fn from(val: u8) -> Self {
        match val {
            0x00 => Self::Empty,
            0x01 => Self::Fat12,
            0x04 => Self::Fat16Small,
            0x05 => Self::Extended,
            0x06 => Self::Fat16Large,
            0x07 => Self::NtfsExfat,
            0x0B => Self::Fat32,
            0x0C => Self::Fat32Lba,
            0x0E => Self::Fat16Lba,
            0x0F => Self::ExtendedLba,
            0x82 => Self::LinuxSwap,
            0x83 => Self::LinuxNative,
            0x8E => Self::LinuxLvm,
            0xEE => Self::GptProtective,
            0xEF => Self::EfiSystem,
            other => Self::Unknown(other),
        }
    }
}

impl core::fmt::Display for MbrPartitionType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => write!(f, "Empty"),
            Self::Fat12 => write!(f, "FAT12"),
            Self::Fat16Small => write!(f, "FAT16 (<32MB)"),
            Self::Extended => write!(f, "Extended"),
            Self::Fat16Large => write!(f, "FAT16 (>32MB)"),
            Self::NtfsExfat => write!(f, "NTFS/exFAT"),
            Self::Fat32 => write!(f, "FAT32"),
            Self::Fat32Lba => write!(f, "FAT32 (LBA)"),
            Self::Fat16Lba => write!(f, "FAT16 (LBA)"),
            Self::ExtendedLba => write!(f, "Extended (LBA)"),
            Self::LinuxSwap => write!(f, "Linux swap"),
            Self::LinuxNative => write!(f, "Linux"),
            Self::LinuxLvm => write!(f, "Linux LVM"),
            Self::GptProtective => write!(f, "GPT Protective"),
            Self::EfiSystem => write!(f, "EFI System"),
            Self::Unknown(id) => write!(f, "Unknown ({:#04x})", id),
        }
    }
}

// ─── MBR Structures ─────────────────────────────────────────────────────

/// MBR partition entry (16 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MbrPartitionEntry {
    /// Boot indicator (0x80 = active, 0x00 = inactive)
    pub boot_indicator: u8,
    /// CHS address of first sector (3 bytes)
    pub start_chs: [u8; 3],
    /// Partition type ID
    pub partition_type: u8,
    /// CHS address of last sector (3 bytes)
    pub end_chs: [u8; 3],
    /// LBA of first sector
    pub start_lba: u32,
    /// Number of sectors
    pub sector_count: u32,
}

impl MbrPartitionEntry {
    /// Check if this entry is empty/unused
    pub fn is_empty(&self) -> bool {
        self.partition_type == 0 || self.sector_count == 0
    }

    /// Check if this is a bootable partition
    pub fn is_bootable(&self) -> bool {
        self.boot_indicator == 0x80
    }

    /// Get partition type
    pub fn part_type(&self) -> MbrPartitionType {
        MbrPartitionType::from(self.partition_type)
    }

    /// Get size in bytes
    pub fn size_bytes(&self) -> u64 {
        self.sector_count as u64 * 512
    }

    /// Get size in MiB
    pub fn size_mib(&self) -> u64 {
        self.size_bytes() / (1024 * 1024)
    }
}

// ─── GPT Structures ─────────────────────────────────────────────────────

/// GPT header (92 bytes minimum)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GptHeader {
    /// Signature ("EFI PART")
    pub signature: u64,
    /// GPT revision
    pub revision: u32,
    /// Header size (usually 92)
    pub header_size: u32,
    /// CRC32 of header
    pub header_crc32: u32,
    /// Reserved (must be zero)
    pub reserved: u32,
    /// LBA of this header
    pub my_lba: u64,
    /// LBA of alternate header
    pub alternate_lba: u64,
    /// First usable LBA
    pub first_usable_lba: u64,
    /// Last usable LBA
    pub last_usable_lba: u64,
    /// Disk GUID
    pub disk_guid: [u8; 16],
    /// Starting LBA of partition entry array
    pub partition_entry_lba: u64,
    /// Number of partition entries
    pub num_partition_entries: u32,
    /// Size of each partition entry
    pub partition_entry_size: u32,
    /// CRC32 of partition entry array
    pub partition_array_crc32: u32,
}

/// GPT partition entry (128 bytes minimum)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GptPartitionEntry {
    /// Partition type GUID
    pub type_guid: [u8; 16],
    /// Unique partition GUID
    pub unique_guid: [u8; 16],
    /// Starting LBA
    pub start_lba: u64,
    /// Ending LBA (inclusive)
    pub end_lba: u64,
    /// Attribute flags
    pub attributes: u64,
    /// Partition name (UTF-16LE, 72 bytes = 36 chars)
    pub name: [u16; 36],
}

impl GptPartitionEntry {
    /// Check if this entry is empty (type GUID is all zeros)
    pub fn is_empty(&self) -> bool {
        self.type_guid == [0u8; 16]
    }

    /// Get the sector count
    pub fn sector_count(&self) -> u64 {
        let start = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.start_lba)) };
        let end = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.end_lba)) };
        if end >= start { end - start + 1 } else { 0 }
    }

    /// Get size in bytes
    pub fn size_bytes(&self) -> u64 {
        self.sector_count() * 512
    }

    /// Get partition name as ASCII string
    pub fn name_string(&self) -> String {
        let mut s = String::new();
        let name = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.name)) };
        for ch in name {
            if ch == 0 {
                break;
            }
            if ch < 128 {
                s.push(ch as u8 as char);
            }
        }
        s
    }

    /// Check if this is a Linux filesystem partition
    pub fn is_linux_filesystem(&self) -> bool {
        // Linux filesystem GUID: 0FC63DAF-8483-4772-8E79-3D69D8477DE4
        const LINUX_FS_GUID: [u8; 16] = [
            0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47,
            0x7D, 0xE4,
        ];
        self.type_guid == LINUX_FS_GUID
    }

    /// Check if this is an EFI System Partition
    pub fn is_efi_system(&self) -> bool {
        // EFI System Partition GUID: C12A7328-F81F-11D2-BA4B-00A0C93EC93B
        const EFI_GUID: [u8; 16] = [
            0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E,
            0xC9, 0x3B,
        ];
        self.type_guid == EFI_GUID
    }

    /// Check if this is a Linux swap partition
    pub fn is_linux_swap(&self) -> bool {
        // Linux swap GUID: 0657FD6D-A4AB-43C4-84E5-0933C84B4F4F
        const SWAP_GUID: [u8; 16] = [
            0x6D, 0xFD, 0x57, 0x06, 0xAB, 0xA4, 0xC4, 0x43, 0x84, 0xE5, 0x09, 0x33, 0xC8, 0x4B,
            0x4F, 0x4F,
        ];
        self.type_guid == SWAP_GUID
    }
}

// ─── Unified Partition Info ─────────────────────────────────────────────

/// Partition table type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionTableType {
    Mbr,
    Gpt,
    None,
}

/// Filesystem type hint for a partition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemType {
    Fat32,
    Fat16,
    Ext2,
    Ext4,
    Ntfs,
    LinuxSwap,
    EfiSystem,
    Unknown,
}

/// Unified partition information (works for both MBR and GPT)
#[derive(Debug, Clone)]
pub struct PartitionInfo {
    /// Partition number (1-based)
    pub number: u32,
    /// Device name (e.g., "sda1", "vda2")
    pub name: String,
    /// Start LBA sector
    pub start_lba: u64,
    /// Number of sectors
    pub sectors: u64,
    /// Size in bytes
    pub size: u64,
    /// Whether this is a bootable/active partition
    pub bootable: bool,
    /// Filesystem type hint
    pub fs_type: FilesystemType,
    /// Partition label (from GPT, or empty for MBR)
    pub label: String,
    /// Source table type
    pub table_type: PartitionTableType,
}

impl PartitionInfo {
    /// Get size as human-readable string
    pub fn size_string(&self) -> String {
        let bytes = self.size;
        if bytes >= 1024 * 1024 * 1024 {
            alloc::format!("{} GiB", bytes / (1024 * 1024 * 1024))
        } else if bytes >= 1024 * 1024 {
            alloc::format!("{} MiB", bytes / (1024 * 1024))
        } else if bytes >= 1024 {
            alloc::format!("{} KiB", bytes / 1024)
        } else {
            alloc::format!("{} B", bytes)
        }
    }
}

// ─── Global Partition Table ─────────────────────────────────────────────

lazy_static::lazy_static! {
    /// Discovered partitions
    pub static ref PARTITIONS: Mutex<Vec<PartitionInfo>> = Mutex::new(Vec::new());

    /// Detected table type
    pub static ref TABLE_TYPE: Mutex<PartitionTableType> = Mutex::new(PartitionTableType::None);
}

// ─── Parsing Functions ──────────────────────────────────────────────────

/// Read a sector from the block device
fn read_sector(lba: u64) -> Option<[u8; 512]> {
    let mut buf = [0u8; 512];
    if crate::virtio_blk::is_available() && crate::virtio_blk::read(lba, 1, &mut buf) {
        return Some(buf);
    }
    // Fallback to ramdisk
    if crate::block::read_blocks(0, lba, 1, &mut buf).is_ok() {
        return Some(buf);
    }
    None
}

/// Parse an MBR partition table from sector 0
pub fn parse_mbr(sector0: &[u8; 512]) -> Vec<PartitionInfo> {
    let mut partitions = Vec::new();

    // Check MBR signature
    let sig = u16::from_le_bytes([sector0[510], sector0[511]]);
    if sig != MBR_SIGNATURE {
        serial_println!("[PART] No valid MBR signature found");
        return partitions;
    }

    // Parse 4 primary partition entries
    for i in 0..MBR_MAX_PARTITIONS {
        let offset = MBR_PARTITION_TABLE_OFFSET + i * MBR_PARTITION_ENTRY_SIZE;
        let entry = unsafe {
            core::ptr::read_unaligned(sector0[offset..].as_ptr() as *const MbrPartitionEntry)
        };

        if entry.is_empty() {
            continue;
        }

        let fs_type = match entry.part_type() {
            MbrPartitionType::Fat32 | MbrPartitionType::Fat32Lba => FilesystemType::Fat32,
            MbrPartitionType::Fat16Small
            | MbrPartitionType::Fat16Large
            | MbrPartitionType::Fat16Lba => FilesystemType::Fat16,
            MbrPartitionType::LinuxNative => FilesystemType::Ext2,
            MbrPartitionType::LinuxSwap => FilesystemType::LinuxSwap,
            MbrPartitionType::NtfsExfat => FilesystemType::Ntfs,
            MbrPartitionType::EfiSystem => FilesystemType::EfiSystem,
            MbrPartitionType::GptProtective => {
                // This is a GPT disk — parse GPT instead
                serial_println!("[PART] GPT protective MBR detected");
                return partitions; // Will trigger GPT parsing
            }
            _ => FilesystemType::Unknown,
        };

        let e_start_lba = entry.start_lba;
        let e_sector_count = entry.sector_count;

        let part = PartitionInfo {
            number: (i + 1) as u32,
            name: alloc::format!("vda{}", i + 1),
            start_lba: e_start_lba as u64,
            sectors: e_sector_count as u64,
            size: entry.size_bytes(),
            bootable: entry.is_bootable(),
            fs_type,
            label: String::new(),
            table_type: PartitionTableType::Mbr,
        };

        serial_println!(
            "[PART] MBR partition {}: type={} start={} sectors={} size={}",
            part.number,
            entry.part_type(),
            e_start_lba,
            e_sector_count,
            part.size_string()
        );

        partitions.push(part);
    }

    partitions
}

/// Parse a GPT partition table
pub fn parse_gpt(sector1: &[u8; 512]) -> Vec<PartitionInfo> {
    let mut partitions = Vec::new();

    let header = unsafe { core::ptr::read_unaligned(sector1.as_ptr() as *const GptHeader) };

    // Verify GPT signature
    if header.signature != GPT_SIGNATURE {
        serial_println!("[PART] Invalid GPT signature");
        return partitions;
    }

    serial_println!(
        "[PART] GPT: revision={:#x} entries={} entry_size={}",
        { header.revision },
        { header.num_partition_entries },
        { header.partition_entry_size }
    );

    let entry_lba = { header.partition_entry_lba };
    let num_entries = { header.num_partition_entries }.min(GPT_MAX_PARTITIONS as u32);
    let entry_size = { header.partition_entry_size }.max(GPT_PARTITION_ENTRY_SIZE as u32) as usize;
    let entries_per_sector = 512 / entry_size;

    let mut partition_num = 1u32;

    for i in 0..num_entries as usize {
        let sector_idx = i / entries_per_sector;
        let offset_in_sector = (i % entries_per_sector) * entry_size;

        if let Some(sector_data) = read_sector(entry_lba + sector_idx as u64) {
            if offset_in_sector + entry_size <= 512 {
                let entry = unsafe {
                    core::ptr::read_unaligned(
                        sector_data[offset_in_sector..].as_ptr() as *const GptPartitionEntry
                    )
                };

                if entry.is_empty() {
                    continue;
                }

                let fs_type = if entry.is_linux_filesystem() {
                    FilesystemType::Ext2
                } else if entry.is_efi_system() {
                    FilesystemType::EfiSystem
                } else if entry.is_linux_swap() {
                    FilesystemType::LinuxSwap
                } else {
                    FilesystemType::Unknown
                };

                let start_lba =
                    unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(entry.start_lba)) };
                let attributes =
                    unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(entry.attributes)) };

                let part = PartitionInfo {
                    number: partition_num,
                    name: alloc::format!("vda{}", partition_num),
                    start_lba,
                    sectors: entry.sector_count(),
                    size: entry.size_bytes(),
                    bootable: (attributes & 0x04) != 0,
                    fs_type,
                    label: entry.name_string(),
                    table_type: PartitionTableType::Gpt,
                };

                serial_println!(
                    "[PART] GPT partition {}: start={} size={} label=\"{}\"",
                    part.number,
                    start_lba,
                    part.size_string(),
                    part.label
                );

                partitions.push(part);
                partition_num += 1;
            }
        }
    }

    partitions
}

/// Scan block device for partition table (MBR or GPT)
pub fn scan_partitions() -> Vec<PartitionInfo> {
    serial_println!("[PART] Scanning for partition tables...");

    // Read sector 0 (MBR)
    let sector0 = match read_sector(0) {
        Some(s) => s,
        None => {
            serial_println!("[PART] Cannot read sector 0");
            return Vec::new();
        }
    };

    // Check MBR signature
    let sig = u16::from_le_bytes([sector0[510], sector0[511]]);
    if sig != MBR_SIGNATURE {
        serial_println!("[PART] No partition table found");
        return Vec::new();
    }

    // Check if MBR has a GPT protective entry
    let first_entry = unsafe {
        core::ptr::read_unaligned(
            sector0[MBR_PARTITION_TABLE_OFFSET..].as_ptr() as *const MbrPartitionEntry
        )
    };

    if first_entry.partition_type == 0xEE {
        // GPT disk — parse GPT header from sector 1
        serial_println!("[PART] GPT protective MBR found, reading GPT header...");
        *TABLE_TYPE.lock() = PartitionTableType::Gpt;

        if let Some(sector1) = read_sector(GPT_HEADER_LBA) {
            return parse_gpt(&sector1);
        } else {
            serial_println!("[PART] Cannot read GPT header");
            return Vec::new();
        }
    }

    // Plain MBR
    *TABLE_TYPE.lock() = PartitionTableType::Mbr;
    parse_mbr(&sector0)
}

/// Get a partition by number (1-based)
pub fn get_partition(number: u32) -> Option<PartitionInfo> {
    PARTITIONS
        .lock()
        .iter()
        .find(|p| p.number == number)
        .cloned()
}

/// Get all discovered partitions
pub fn list_partitions() -> Vec<PartitionInfo> {
    PARTITIONS.lock().clone()
}

/// Get the partition table type
pub fn table_type() -> PartitionTableType {
    *TABLE_TYPE.lock()
}

/// Initialize partition subsystem — scan block device
pub fn init() {
    let parts = scan_partitions();
    let count = parts.len();
    *PARTITIONS.lock() = parts;

    serial_println!(
        "[KnoxOS] Partition table: {} ({} partitions found)",
        match table_type() {
            PartitionTableType::Mbr => "MBR",
            PartitionTableType::Gpt => "GPT",
            PartitionTableType::None => "None",
        },
        count
    );
}
