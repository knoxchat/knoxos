/// exFAT — Extended File Allocation Table for USB drives > 32GB
use alloc::string::String;
use alloc::vec;
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

impl ExfatVolume {
    fn load_fat(&mut self, device: usize) -> Result<(), &'static str> {
        let fat_sectors = self.bpb.fat_length.max(1);
        let bytes = fat_sectors as usize * 512;
        let mut buf = vec![0u8; bytes];
        crate::block::read_blocks(
            device,
            self.bpb.fat_offset as u64,
            fat_sectors as u8,
            &mut buf,
        )
        .map_err(|_| "FAT read")?;
        self.fat = buf
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Ok(())
    }

    fn cluster_lba(&self, cluster: u32) -> u64 {
        self.bpb.cluster_heap_offset as u64
            + (cluster.saturating_sub(2) as u64) * self.bpb.sectors_per_cluster() as u64
    }

    fn read_cluster(&self, device: usize, cluster: u32) -> Result<Vec<u8>, &'static str> {
        let spc = self.bpb.sectors_per_cluster().max(1);
        let mut buf = vec![0u8; spc as usize * 512];
        crate::block::read_blocks(device, self.cluster_lba(cluster), spc as u8, &mut buf)
            .map_err(|_| "cluster read")?;
        Ok(buf)
    }

    fn read_file(&self, device: usize, file: &ExfatFile) -> Result<Vec<u8>, &'static str> {
        if file.first_cluster < 2 {
            return Err("bad cluster");
        }
        let data = self.read_cluster(device, file.first_cluster)?;
        let n = (file.size as usize).min(data.len());
        Ok(data[..n].to_vec())
    }
}

fn build_gate_volume(payload: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut boot = vec![0u8; 512];
    boot[0] = 0xEB;
    boot[1] = 0x76;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"EXFAT   ");
    boot[80..84].copy_from_slice(&24u32.to_le_bytes()); // fat_offset
    boot[84..88].copy_from_slice(&1u32.to_le_bytes()); // fat_length
    boot[88..92].copy_from_slice(&32u32.to_le_bytes()); // cluster_heap_offset
    boot[92..96].copy_from_slice(&16u32.to_le_bytes()); // cluster_count
    boot[96..100].copy_from_slice(&2u32.to_le_bytes()); // root_dir_cluster
    boot[100..104].copy_from_slice(&0x4B4E4F58u32.to_le_bytes());
    boot[104] = 0x00;
    boot[105] = 0x01; // revision 1.00
    boot[108] = 9; // 512-byte sectors
    boot[109] = 0; // 1 sector/cluster
    boot[110] = 1; // one FAT
    boot[510] = 0x55;
    boot[511] = 0xAA;

    let mut fat = vec![0u8; 512];
    fat[0..4].copy_from_slice(&0xFFFFFFF8u32.to_le_bytes());
    fat[4..8].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    fat[8..12].copy_from_slice(&EXFAT_CLUSTER_END.to_le_bytes()); // cluster 2 root
    fat[12..16].copy_from_slice(&EXFAT_CLUSTER_END.to_le_bytes()); // cluster 3 file

    let mut root = vec![0u8; 512];
    // FileDirectory
    root[0] = 0x85;
    root[1] = 2; // two secondary entries
    root[4] = 0x20; // archive
    // StreamExtension
    root[32] = 0xC0;
    root[33] = 0x03; // allocation + contiguous
    root[35] = b"GATE.TXT".len() as u8;
    root[40..48].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    root[52..56].copy_from_slice(&3u32.to_le_bytes());
    root[56..64].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    // FileName
    root[64] = 0xC1;
    let name = "GATE.TXT";
    for (i, ch) in name.encode_utf16().enumerate() {
        let b = ch.to_le_bytes();
        root[66 + i * 2] = b[0];
        root[67 + i * 2] = b[1];
    }

    let mut data = vec![0u8; 512];
    data[..payload.len()].copy_from_slice(payload);
    (boot, fat, {
        let mut heap = vec![0u8; 1024];
        heap[..512].copy_from_slice(&root);
        heap[512..].copy_from_slice(&data);
        heap
    })
}

pub const GATE_EXFAT_MARKER: &str = "GATE_EXFAT block";
const GATE_EXFAT_PAYLOAD: &[u8] = b"knoxos-exfat\n";

fn block_self_test() -> bool {
    let device = crate::block::create_ramdisk(1);
    let (boot, fat, heap) = build_gate_volume(GATE_EXFAT_PAYLOAD);
    if crate::block::write_blocks(device, 0, 1, &boot).is_err() {
        serial_println!("[exfat] GATE_EXFAT FAILED: boot write");
        return false;
    }
    if crate::block::write_blocks(device, 24, 1, &fat).is_err() {
        serial_println!("[exfat] GATE_EXFAT FAILED: fat write");
        return false;
    }
    if crate::block::write_blocks(device, 32, 2, &heap).is_err() {
        serial_println!("[exfat] GATE_EXFAT FAILED: heap write");
        return false;
    }

    let mut boot_in = vec![0u8; 512];
    if crate::block::read_blocks(device, 0, 1, &mut boot_in).is_err() {
        serial_println!("[exfat] GATE_EXFAT FAILED: boot read");
        return false;
    }
    let mut vol = match ExfatVolume::from_boot_sector(&boot_in) {
        Ok(v) => v,
        Err(e) => {
            serial_println!("[exfat] GATE_EXFAT FAILED: parse boot ({})", e);
            return false;
        }
    };
    if let Err(e) = vol.load_fat(device) {
        serial_println!("[exfat] GATE_EXFAT FAILED: load fat ({})", e);
        return false;
    }
    let root = match vol.read_cluster(device, vol.bpb.root_dir_cluster) {
        Ok(d) => d,
        Err(e) => {
            serial_println!("[exfat] GATE_EXFAT FAILED: root ({})", e);
            return false;
        }
    };
    let files = vol.parse_directory(&root);
    let Some(file) = files.iter().find(|f| f.name == "GATE.TXT") else {
        serial_println!("[exfat] GATE_EXFAT FAILED: GATE.TXT missing");
        return false;
    };
    match vol.read_file(device, file) {
        Ok(data) if data == GATE_EXFAT_PAYLOAD => {
            EXFAT_VOLUMES.lock().push(vol);
            serial_println!(
                "[exfat] {} (directory $DATA via ramdisk)",
                GATE_EXFAT_MARKER
            );
            true
        }
        Ok(data) => {
            serial_println!("[exfat] GATE_EXFAT FAILED: payload len={}", data.len());
            false
        }
        Err(e) => {
            serial_println!("[exfat] GATE_EXFAT FAILED: {}", e);
            false
        }
    }
}

lazy_static::lazy_static! {
    static ref EXFAT_VOLUMES: Mutex<Vec<ExfatVolume>> = Mutex::new(Vec::new());
}

pub fn init() {
    serial_println!("[exfat] exFAT filesystem driver initialized");
    let _ = block_self_test();
}
