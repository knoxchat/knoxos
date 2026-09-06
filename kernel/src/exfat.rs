/// exFAT — Extended File Allocation Table for USB drives > 32GB
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// exFAT STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

/// exFAT boot sector (Volume Boot Record)
#[derive(Debug, Clone, Copy)]
pub struct ExfatBpb {
    pub partition_offset: u64,
    pub volume_length: u64, // total sectors
    pub fat_offset: u32,    // sectors
    pub fat_length: u32,    // sectors
    pub cluster_heap_offset: u32,
    pub cluster_count: u32,
    pub root_dir_cluster: u32,
    pub serial_number: u32,
    pub bytes_per_sector_shift: u8, // 2^n
    pub sectors_per_cluster_shift: u8,
    pub number_of_fats: u8,
    pub volume_flags: u16,
}

impl ExfatBpb {
    pub fn bytes_per_sector(&self) -> u32 {
        1 << self.bytes_per_sector_shift
    }
    pub fn sectors_per_cluster(&self) -> u32 {
        1 << self.sectors_per_cluster_shift
    }
    pub fn bytes_per_cluster(&self) -> u32 {
        self.bytes_per_sector() * self.sectors_per_cluster()
    }
}

/// exFAT directory entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    EndOfDirectory,
    AllocationBitmap,
    UpcaseTable,
    VolumeLabel,
    FileDirectory,
    StreamExtension,
    FileNameExtension,
    Unknown(u8),
}

impl EntryType {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x00 => EntryType::EndOfDirectory,
            0x81 => EntryType::AllocationBitmap,
            0x82 => EntryType::UpcaseTable,
            0x83 => EntryType::VolumeLabel,
            0x85 => EntryType::FileDirectory,
            0xC0 => EntryType::StreamExtension,
            0xC1 => EntryType::FileNameExtension,
            x => EntryType::Unknown(x),
        }
    }
}

/// Parsed exFAT file entry (combined from FileDirectory + StreamExtension + FileNameExtension)
#[derive(Debug, Clone)]
pub struct ExfatFile {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub first_cluster: u32,
    pub contiguous: bool,
    pub create_time: u32,
    pub modify_time: u32,
    pub access_time: u32,
    pub attributes: u16,
}

// ═══════════════════════════════════════════════════════════════════════
// exFAT DRIVER
// ═══════════════════════════════════════════════════════════════════════

pub struct ExfatVolume {
    pub bpb: ExfatBpb,
    pub volume_label: String,
    /// FAT table (cluster -> next cluster mapping)
    pub fat: Vec<u32>,
}

/// Special FAT values
pub const EXFAT_CLUSTER_FREE: u32 = 0x00000000;
pub const EXFAT_CLUSTER_END: u32 = 0xFFFFFFFF;
pub const EXFAT_CLUSTER_BAD: u32 = 0xFFFFFFF7;

impl ExfatVolume {
    /// Parse exFAT VBR from boot sector
    pub fn from_boot_sector(sector: &[u8]) -> Result<Self, &'static str> {
        if sector.len() < 512 {
            return Err("Sector too short");
        }
        // Check signature "EXFAT   " at offset 3
        if &sector[3..11] != b"EXFAT   " {
            return Err("Not an exFAT volume");
        }

        let bpb = ExfatBpb {
            partition_offset: u64::from_le_bytes([
                sector[64], sector[65], sector[66], sector[67], sector[68], sector[69], sector[70],
                sector[71],
            ]),
            volume_length: u64::from_le_bytes([
                sector[72], sector[73], sector[74], sector[75], sector[76], sector[77], sector[78],
                sector[79],
            ]),
            fat_offset: u32::from_le_bytes([sector[80], sector[81], sector[82], sector[83]]),
            fat_length: u32::from_le_bytes([sector[84], sector[85], sector[86], sector[87]]),
            cluster_heap_offset: u32::from_le_bytes([
                sector[88], sector[89], sector[90], sector[91],
            ]),
            cluster_count: u32::from_le_bytes([sector[92], sector[93], sector[94], sector[95]]),
            root_dir_cluster: u32::from_le_bytes([sector[96], sector[97], sector[98], sector[99]]),
            serial_number: u32::from_le_bytes([sector[100], sector[101], sector[102], sector[103]]),
            bytes_per_sector_shift: sector[108],
            sectors_per_cluster_shift: sector[109],
            number_of_fats: sector[110],
            volume_flags: u16::from_le_bytes([sector[106], sector[107]]),
        };

        serial_println!(
            "[exfat] Volume: {} clusters, {}B/cluster, root@cluster {}",
            bpb.cluster_count,
            bpb.bytes_per_cluster(),
            bpb.root_dir_cluster
        );

        Ok(Self {
            bpb,
            volume_label: String::from("exFAT"),
            fat: Vec::new(),
        })
    }

    /// Follow cluster chain from FAT
    pub fn get_chain(&self, start_cluster: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut cluster = start_cluster;
        while (2..EXFAT_CLUSTER_BAD).contains(&cluster) {
            chain.push(cluster);
            let idx = cluster as usize;
            if idx >= self.fat.len() {
                break;
            }
            cluster = self.fat[idx];
            if chain.len() > self.bpb.cluster_count as usize {
                break; // prevent infinite loop
            }
        }
        chain
    }

    /// Parse directory entries from raw cluster data
    pub fn parse_directory(&self, data: &[u8]) -> Vec<ExfatFile> {
        let mut files = Vec::new();
        let mut offset = 0;
        let mut current_file: Option<ExfatFile> = None;
        let mut name_parts: Vec<String> = Vec::new();

        while offset + 32 <= data.len() {
            let entry_type = EntryType::from_byte(data[offset]);
            match entry_type {
                EntryType::EndOfDirectory => break,
                EntryType::FileDirectory => {
                    // Finalize previous entry
                    if let Some(mut f) = current_file.take() {
                        f.name = name_parts.join("");
                        files.push(f);
                        name_parts.clear();
                    }
                    let attrs = u16::from_le_bytes([data[offset + 4], data[offset + 5]]);
                    current_file = Some(ExfatFile {
                        name: String::new(),
                        is_directory: attrs & 0x10 != 0,
                        size: 0,
                        first_cluster: 0,
                        contiguous: false,
                        create_time: 0,
                        modify_time: 0,
                        access_time: 0,
                        attributes: attrs,
                    });
                }
                EntryType::StreamExtension => {
                    if let Some(ref mut f) = current_file {
                        f.size = u64::from_le_bytes([
                            data[offset + 8],
                            data[offset + 9],
                            data[offset + 10],
                            data[offset + 11],
                            data[offset + 12],
                            data[offset + 13],
                            data[offset + 14],
                            data[offset + 15],
                        ]);
                        f.first_cluster = u32::from_le_bytes([
                            data[offset + 20],
                            data[offset + 21],
                            data[offset + 22],
                            data[offset + 23],
                        ]);
                        f.contiguous = data[offset + 1] & 0x02 != 0;
                    }
                }
                EntryType::FileNameExtension => {
                    // UTF-16LE name characters at offset+2, up to 15 chars
                    let mut name = String::new();
                    for i in 0..15 {
                        let pos = offset + 2 + i * 2;
                        if pos + 2 > data.len() {
                            break;
                        }
                        let c = u16::from_le_bytes([data[pos], data[pos + 1]]);
                        if c == 0 {
                            break;
                        }
                        if let Some(ch) = char::from_u32(c as u32) {
                            name.push(ch);
                        }
                    }
                    name_parts.push(name);
                }
                _ => {}
            }
            offset += 32;
        }

        // Finalize last entry
        if let Some(mut f) = current_file {
            f.name = name_parts.join("");
            files.push(f);
        }

        files
    }
}

lazy_static::lazy_static! {
    static ref EXFAT_VOLUMES: Mutex<Vec<ExfatVolume>> = Mutex::new(Vec::new());
}

pub fn init() {
    serial_println!("[exfat] exFAT filesystem driver initialized");
}
