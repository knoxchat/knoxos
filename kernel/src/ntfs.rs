/// NTFS — Windows NT File System read/write driver
///
/// Implements MFT parsing, file/directory reads, and basic write support
/// for Windows interoperability.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
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

            let attr_bytes = &raw[offset..offset + attr_len];
            record.attributes.push(parse_attribute(attr_bytes));

            offset += attr_len;
        }

        Ok(record)
    }

    pub fn mft_record_bytes(&self) -> usize {
        let c = self.bpb.clusters_per_mft_record;
        if c < 0 {
            1usize << ((-c) as u32)
        } else {
            self.cluster_size as usize * c as usize
        }
    }

    /// Read `count` sectors from the mounted block device.
    pub fn read_sectors(
        &self,
        device_index: usize,
        lba: u64,
        buf: &mut [u8],
    ) -> Result<(), &'static str> {
        let count = (buf.len() / crate::block::BLOCK_SIZE) as u8;
        if count == 0 {
            return Err("empty sector read");
        }
        crate::block::read_blocks(device_index, lba, count, buf).map_err(|_| "block read failed")
    }

    /// Load an MFT record through the block layer and cache it.
    pub fn load_mft_record(
        &mut self,
        device_index: usize,
        record_num: u64,
    ) -> Result<&MftRecord, &'static str> {
        if self.mft_cache.contains_key(&record_num) {
            return Ok(self.mft_cache.get(&record_num).unwrap());
        }
        let rec_size = self.mft_record_bytes().max(MFT_RECORD_SIZE);
        let bytes_per_sector = self.bpb.bytes_per_sector as u64;
        if bytes_per_sector == 0 {
            return Err("bad bytes per sector");
        }
        let mft_lba = self.bpb.mft_cluster * (self.cluster_size as u64 / bytes_per_sector)
            + record_num * (rec_size as u64 / bytes_per_sector);
        let mut raw = vec![0u8; rec_size];
        self.read_sectors(device_index, mft_lba, &mut raw)?;
        let mut rec = self.parse_mft_record(&raw)?;
        rec.record_number = record_num;
        self.mft_cache.insert(record_num, rec);
        Ok(self.mft_cache.get(&record_num).unwrap())
    }

    /// Read a file's data by MFT record number (resident or run-list).
    pub fn read_file(&self, record_num: u64) -> Result<Vec<u8>, &'static str> {
        self.read_file_from(None, record_num)
    }

    /// Read file data; non-resident runs go through `device_index` when given.
    pub fn read_file_from(
        &self,
        device_index: Option<usize>,
        record_num: u64,
    ) -> Result<Vec<u8>, &'static str> {
        let record = self
            .mft_cache
            .get(&record_num)
            .ok_or("MFT record not cached")?;
        for attr in &record.attributes {
            if let NtfsAttribute::Data(data_attr) = attr {
                if data_attr.resident {
                    return Ok(data_attr.data.clone());
                }
                let Some(dev) = device_index else {
                    return Err("non-resident data needs a block device");
                };
                return self.read_runs(dev, data_attr);
            }
        }
        Err("No $DATA attribute")
    }

    fn read_runs(
        &self,
        device_index: usize,
        data_attr: &DataAttr,
    ) -> Result<Vec<u8>, &'static str> {
        let mut out = Vec::new();
        let mut vcn: i64 = 0;
        let bytes_per_sector = self.bpb.bytes_per_sector as u64;
        for run in &data_attr.runs {
            vcn += run.cluster_offset;
            if vcn < 0 {
                continue;
            }
            let lba = (vcn as u64) * (self.cluster_size as u64 / bytes_per_sector.max(1));
            let bytes = (run.cluster_count as usize) * self.cluster_size as usize;
            let mut buf = vec![0u8; bytes];
            self.read_sectors(device_index, lba, &mut buf)?;
            out.extend_from_slice(&buf);
        }
        if data_attr.real_size > 0 && (data_attr.real_size as usize) < out.len() {
            out.truncate(data_attr.real_size as usize);
        }
        Ok(out)
    }
}

/// Parse one MFT attribute (resident $DATA / $FILE_NAME, else Other).
fn parse_attribute(raw: &[u8]) -> NtfsAttribute {
    if raw.len() < 16 {
        return NtfsAttribute::Other {
            attr_type: 0,
            data: raw.to_vec(),
        };
    }
    let attr_type = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let non_resident = raw[8] != 0;
    match attr_type {
        0x30 if !non_resident && raw.len() >= 24 => {
            let val_len = u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]) as usize;
            let val_off = u16::from_le_bytes([raw[20], raw[21]]) as usize;
            if val_off + val_len <= raw.len() && val_len >= 0x42 {
                let val = &raw[val_off..val_off + val_len];
                let mut parent_bytes = [0u8; 8];
                parent_bytes.copy_from_slice(&val[0..8]);
                let parent = u64::from_le_bytes(parent_bytes) & 0x0000_FFFF_FFFF_FFFF;
                let name_len = val[0x40] as usize;
                let name_type = val[0x41];
                let name_bytes = &val[0x42..];
                let mut name = String::new();
                for i in 0..name_len {
                    let lo = *name_bytes.get(i * 2).unwrap_or(&0);
                    let hi = *name_bytes.get(i * 2 + 1).unwrap_or(&0);
                    if hi == 0 {
                        name.push(lo as char);
                    }
                }
                return NtfsAttribute::FileName(FileNameAttr {
                    parent_record: parent,
                    name,
                    name_type,
                    allocated_size: 0,
                    real_size: 0,
                });
            }
        }
        0x80 if !non_resident && raw.len() >= 24 => {
            let val_len = u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]) as usize;
            let val_off = u16::from_le_bytes([raw[20], raw[21]]) as usize;
            let data = if val_off + val_len <= raw.len() {
                raw[val_off..val_off + val_len].to_vec()
            } else {
                Vec::new()
            };
            return NtfsAttribute::Data(DataAttr {
                resident: true,
                data,
                runs: Vec::new(),
                real_size: val_len as u64,
            });
        }
        _ => {}
    }
    NtfsAttribute::Other {
        attr_type,
        data: raw.to_vec(),
    }
}

/// Build a boot sector + MFT FILE record with resident $DATA.
fn build_gate_volume(payload: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut boot = vec![0u8; 512];
    boot[0] = 0xEB;
    boot[1] = 0x52;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[0x0B] = 0x00;
    boot[0x0C] = 0x02; // 512 bytes/sector
    boot[0x0D] = 1; // 1 sector/cluster
    let total: u64 = 2048;
    boot[0x28..0x30].copy_from_slice(&total.to_le_bytes());
    let mft_cluster: u64 = 4;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x38..0x40].copy_from_slice(&8u64.to_le_bytes());
    boot[0x40] = (-10i8) as u8; // 1024-byte MFT records
    boot[0x44] = 1;
    boot[0x1FE] = 0x55;
    boot[0x1FF] = 0xAA;

    let mut rec = vec![0u8; MFT_RECORD_SIZE];
    rec[0..4].copy_from_slice(&MFT_RECORD_MAGIC.to_le_bytes());
    rec[0x10] = 1; // sequence
    rec[0x14] = 0x38; // first attribute
    rec[0x15] = 0x00;
    rec[0x16] = 0x01; // in use
    rec[0x18..0x1C].copy_from_slice(&(MFT_RECORD_SIZE as u32).to_le_bytes());
    rec[0x1C..0x20].copy_from_slice(&(MFT_RECORD_SIZE as u32).to_le_bytes());

    // $FILE_NAME (type 0x30), resident, name GATE.TXT
    let name = b"GATE.TXT";
    let fname_val_len = 0x42 + name.len() * 2;
    let fname_attr_len = (0x18 + fname_val_len).div_ceil(8) * 8;
    let mut off = 0x38usize;
    rec[off..off + 4].copy_from_slice(&0x30u32.to_le_bytes());
    rec[off + 4..off + 8].copy_from_slice(&(fname_attr_len as u32).to_le_bytes());
    rec[off + 8] = 0; // resident
    rec[off + 16..off + 20].copy_from_slice(&(fname_val_len as u32).to_le_bytes());
    rec[off + 20] = 0x18;
    rec[off + 21] = 0x00;
    rec[off + 0x18 + 0x40] = name.len() as u8;
    rec[off + 0x18 + 0x41] = 3; // Win32+DOS
    for (i, b) in name.iter().enumerate() {
        rec[off + 0x18 + 0x42 + i * 2] = *b;
    }
    off += fname_attr_len;

    // $DATA (type 0x80), resident payload
    let data_attr_len = (0x18 + payload.len()).div_ceil(8) * 8;
    rec[off..off + 4].copy_from_slice(&0x80u32.to_le_bytes());
    rec[off + 4..off + 8].copy_from_slice(&(data_attr_len as u32).to_le_bytes());
    rec[off + 8] = 0;
    rec[off + 16..off + 20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    rec[off + 20] = 0x18;
    rec[off + 21] = 0x00;
    rec[off + 0x18..off + 0x18 + payload.len()].copy_from_slice(payload);
    off += data_attr_len;

    rec[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    (boot, rec)
}

pub const GATE_NTFS_MARKER: &str = "GATE_NTFS block";
const GATE_NTFS_PAYLOAD: &[u8] = b"knoxos-ntfs\n";

fn block_self_test() -> bool {
    let device = crate::block::create_ramdisk(1);
    let (boot, rec) = build_gate_volume(GATE_NTFS_PAYLOAD);
    if crate::block::write_blocks(device, 0, 1, &boot).is_err() {
        serial_println!("[ntfs] GATE_NTFS FAILED: boot write");
        return false;
    }
    if crate::block::write_blocks(device, 4, 2, &rec).is_err() {
        serial_println!("[ntfs] GATE_NTFS FAILED: mft write");
        return false;
    }

    let mut boot_in = vec![0u8; 512];
    if crate::block::read_blocks(device, 0, 1, &mut boot_in).is_err() {
        serial_println!("[ntfs] GATE_NTFS FAILED: boot read");
        return false;
    }
    let mut vol = match NtfsVolume::from_boot_sector(&boot_in) {
        Ok(v) => v,
        Err(e) => {
            serial_println!("[ntfs] GATE_NTFS FAILED: parse boot ({})", e);
            return false;
        }
    };
    if vol.load_mft_record(device, 0).is_err() {
        serial_println!("[ntfs] GATE_NTFS FAILED: mft via block");
        return false;
    }
    match vol.read_file_from(Some(device), 0) {
        Ok(data) if data == GATE_NTFS_PAYLOAD => {
            NTFS_VOLUMES.lock().push(vol);
            serial_println!("[ntfs] {} (MFT $DATA via ramdisk)", GATE_NTFS_MARKER);
            true
        }
        Ok(data) => {
            serial_println!("[ntfs] GATE_NTFS FAILED: payload len={}", data.len());
            false
        }
        Err(e) => {
            serial_println!("[ntfs] GATE_NTFS FAILED: {}", e);
            false
        }
    }
}

lazy_static::lazy_static! {
    static ref NTFS_VOLUMES: Mutex<Vec<NtfsVolume>> = Mutex::new(Vec::new());
}

pub fn init() {
    serial_println!("[ntfs] NTFS driver initialized");
    let _ = block_self_test();
}
