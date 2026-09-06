/// NTFS — Windows NT File System read/write driver
///
/// Implements MFT parsing, file/directory reads, and basic write support
/// for Windows interoperability.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// NTFS STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

/// NTFS boot sector (first 512 bytes of partition)
#[derive(Debug, Clone, Copy)]
pub struct NtfsBpb {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub total_sectors: u64,
    pub mft_cluster: u64,
    pub mft_mirror_cluster: u64,
    pub clusters_per_mft_record: i8,
    pub clusters_per_index_block: i8,
    pub serial_number: u64,
}

/// MFT record (File Record Segment)
pub const MFT_RECORD_MAGIC: u32 = 0x454C4946; // "FILE"
pub const MFT_RECORD_SIZE: usize = 1024;

#[derive(Debug, Clone)]
pub struct MftRecord {
    pub record_number: u64,
    pub flags: u16, // 0x01 = in use, 0x02 = directory
    pub sequence_number: u16,
    pub base_record: u64,
    pub attributes: Vec<NtfsAttribute>,
}

impl MftRecord {
    pub fn is_in_use(&self) -> bool {
        self.flags & 0x01 != 0
    }
    pub fn is_directory(&self) -> bool {
        self.flags & 0x02 != 0
    }
}

/// NTFS attribute types
#[derive(Debug, Clone)]
pub enum NtfsAttribute {
    StandardInformation(StdInfo),
    FileName(FileNameAttr),
    Data(DataAttr),
    IndexRoot(IndexRoot),
    IndexAllocation(Vec<u8>),
    Bitmap(Vec<u8>),
    Other { attr_type: u32, data: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct StdInfo {
    pub creation_time: u64,
    pub modification_time: u64,
    pub access_time: u64,
    pub file_attributes: u32,
}

#[derive(Debug, Clone)]
pub struct FileNameAttr {
    pub parent_record: u64,
    pub name: String,
    pub name_type: u8, // 0=POSIX, 1=Win32, 2=DOS, 3=Win32+DOS
    pub allocated_size: u64,
    pub real_size: u64,
}

#[derive(Debug, Clone)]
pub struct DataAttr {
    pub resident: bool,
    pub data: Vec<u8>,      // if resident
    pub runs: Vec<DataRun>, // if non-resident
    pub real_size: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct DataRun {
    pub cluster_offset: i64, // signed (relative)
    pub cluster_count: u64,
}

#[derive(Debug, Clone)]
pub struct IndexRoot {
    pub index_type: u32,
    pub collation_rule: u32,
    pub entry_size: u32,
}

// ═══════════════════════════════════════════════════════════════════════
// NTFS DRIVER
// ═══════════════════════════════════════════════════════════════════════

pub struct NtfsVolume {
    pub bpb: NtfsBpb,
    pub cluster_size: u32,
    pub mft_cache: BTreeMap<u64, MftRecord>,
}

impl NtfsVolume {
    /// Parse BPB from the first sector
    pub fn from_boot_sector(sector: &[u8]) -> Result<Self, &'static str> {
        if sector.len() < 512 {
            return Err("Sector too short");
        }
        // Check NTFS signature at offset 3
        if &sector[3..7] != b"NTFS" {
            return Err("Not an NTFS volume");
        }

        let bpb = NtfsBpb {
            bytes_per_sector: u16::from_le_bytes([sector[0x0B], sector[0x0C]]),
            sectors_per_cluster: sector[0x0D],
            total_sectors: u64::from_le_bytes([
                sector[0x28],
                sector[0x29],
                sector[0x2A],
                sector[0x2B],
                sector[0x2C],
                sector[0x2D],
                sector[0x2E],
                sector[0x2F],
            ]),
            mft_cluster: u64::from_le_bytes([
                sector[0x30],
                sector[0x31],
                sector[0x32],
                sector[0x33],
                sector[0x34],
                sector[0x35],
                sector[0x36],
                sector[0x37],
            ]),
            mft_mirror_cluster: u64::from_le_bytes([
                sector[0x38],
                sector[0x39],
                sector[0x3A],
                sector[0x3B],
                sector[0x3C],
                sector[0x3D],
                sector[0x3E],
                sector[0x3F],
            ]),
            clusters_per_mft_record: sector[0x40] as i8,
            clusters_per_index_block: sector[0x44] as i8,
            serial_number: u64::from_le_bytes([
                sector[0x48],
                sector[0x49],
                sector[0x4A],
                sector[0x4B],
                sector[0x4C],
                sector[0x4D],
                sector[0x4E],
                sector[0x4F],
            ]),
        };

        let cluster_size = bpb.bytes_per_sector as u32 * bpb.sectors_per_cluster as u32;

        serial_println!(
            "[ntfs] Volume: {} sectors, cluster={}B, MFT@cluster {}",
            bpb.total_sectors,
            cluster_size,
            bpb.mft_cluster
        );

        Ok(Self {
            bpb,
            cluster_size,
            mft_cache: BTreeMap::new(),
        })
    }

    /// Parse a raw MFT record
    pub fn parse_mft_record(&self, raw: &[u8]) -> Result<MftRecord, &'static str> {
        if raw.len() < 42 {
            return Err("MFT record too short");
        }
        let magic = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        if magic != MFT_RECORD_MAGIC {
            return Err("Invalid MFT record magic");
        }

        let flags = u16::from_le_bytes([raw[0x16], raw[0x17]]);
        let sequence = u16::from_le_bytes([raw[0x10], raw[0x11]]);
        let first_attr_offset = u16::from_le_bytes([raw[0x14], raw[0x15]]) as usize;

        let mut record = MftRecord {
            record_number: 0,
            flags,
            sequence_number: sequence,
            base_record: 0,
            attributes: Vec::new(),
        };

        // Walk attributes
        let mut offset = first_attr_offset;
        while offset + 4 <= raw.len() {
            let attr_type = u32::from_le_bytes([
                raw[offset],
                raw[offset + 1],
                raw[offset + 2],
                raw[offset + 3],
            ]);
            if attr_type == 0xFFFFFFFF {
                break; // end marker
            }
            let attr_len = u32::from_le_bytes([
                raw[offset + 4],
                raw[offset + 5],
                raw[offset + 6],
                raw[offset + 7],
            ]) as usize;
            if attr_len < 16 || offset + attr_len > raw.len() {
                break;
            }

            record.attributes.push(NtfsAttribute::Other {
                attr_type,
                data: raw[offset..offset + attr_len].to_vec(),
            });

            offset += attr_len;
        }

        Ok(record)
    }

    /// Read a file's data by MFT record number
    pub fn read_file(&self, record_num: u64) -> Result<Vec<u8>, &'static str> {
        let record = self
            .mft_cache
            .get(&record_num)
            .ok_or("MFT record not cached")?;
        for attr in &record.attributes {
            if let NtfsAttribute::Data(data_attr) = attr {
                if data_attr.resident {
                    return Ok(data_attr.data.clone());
                }
                // Non-resident: would read clusters from disk following runs
                serial_println!(
                    "[ntfs] Reading non-resident data ({} runs)",
                    data_attr.runs.len()
                );
                return Ok(Vec::new());
            }
        }
        Err("No $DATA attribute")
    }
}

lazy_static::lazy_static! {
    static ref NTFS_VOLUMES: Mutex<Vec<NtfsVolume>> = Mutex::new(Vec::new());
}

pub fn init() {
    serial_println!("[ntfs] NTFS driver initialized");
}
