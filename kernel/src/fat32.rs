/// FAT32 Filesystem Driver — Full FAT32 read/write support
///
/// Implements the FAT32 filesystem specification for:
///   - Reading and writing files
///   - Directory operations (mkdir, rmdir, readdir)
///   - Long filename (LFN) support
///   - FAT chain management
///   - Free cluster tracking
///
/// This driver works with any block device (virtio-blk, ATA, ramdisk).
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── FAT32 Constants ────────────────────────────────────────────────────

pub const FAT32_SIGNATURE: u16 = 0xAA55;
pub const FAT32_FSINFO_SIGNATURE1: u32 = 0x41615252;
pub const FAT32_FSINFO_SIGNATURE2: u32 = 0x61417272;

pub const FAT32_EOC: u32 = 0x0FFFFFF8; // End of chain marker
pub const FAT32_FREE: u32 = 0x00000000; // Free cluster
pub const FAT32_BAD: u32 = 0x0FFFFFF7; // Bad cluster
pub const FAT32_MASK: u32 = 0x0FFFFFFF; // FAT entry mask (28 bits)

pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
pub const ATTR_LONG_NAME: u8 = 0x0F;

pub const DIR_ENTRY_SIZE: usize = 32;
pub const SECTOR_SIZE: usize = 512;
pub const LFN_CHARS_PER_ENTRY: usize = 13;
pub const LFN_LAST_ENTRY: u8 = 0x40;

// ─── Boot Sector ────────────────────────────────────────────────────────

/// FAT32 Boot Sector (BPB + Extended BPB)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fat32BootSector {
    pub jump_boot: [u8; 3],
    pub oem_name: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub root_entry_count: u16, // 0 for FAT32
    pub total_sectors_16: u16, // 0 for FAT32
    pub media_type: u8,
    pub fat_size_16: u16, // 0 for FAT32
    pub sectors_per_track: u16,
    pub num_heads: u16,
    pub hidden_sectors: u32,
    pub total_sectors_32: u32,
    // FAT32 extended fields
    pub fat_size_32: u32,
    pub ext_flags: u16,
    pub fs_version: u16,
    pub root_cluster: u32,       // Usually 2
    pub fs_info_sector: u16,     // Usually 1
    pub backup_boot_sector: u16, // Usually 6
    pub reserved: [u8; 12],
    pub drive_number: u8,
    pub reserved1: u8,
    pub boot_sig: u8,
    pub volume_id: u32,
    pub volume_label: [u8; 11],
    pub fs_type: [u8; 8],
}

impl Fat32BootSector {
    /// Parse from raw sector data
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 512 {
            return None;
        }
        // Check signature
        if data[510] != 0x55 || data[511] != 0xAA {
            return None;
        }
        let bpb = unsafe { *(data.as_ptr() as *const Fat32BootSector) };
        Some(bpb)
    }

    /// Get total sectors
    pub fn total_sectors(&self) -> u32 {
        if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u32
        } else {
            self.total_sectors_32
        }
    }

    /// Get the first data sector
    pub fn first_data_sector(&self) -> u32 {
        self.reserved_sectors as u32 + (self.num_fats as u32 * self.fat_size_32)
    }

    /// Get the first sector of a cluster
    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.first_data_sector() + (cluster - 2) * self.sectors_per_cluster as u32
    }

    /// Get total data sectors
    pub fn data_sectors(&self) -> u32 {
        self.total_sectors() - self.first_data_sector()
    }

    /// Get total number of clusters
    pub fn total_clusters(&self) -> u32 {
        self.data_sectors() / self.sectors_per_cluster as u32
    }

    /// Get volume label as string
    pub fn volume_label_string(&self) -> String {
        let label = core::str::from_utf8(&self.volume_label)
            .unwrap_or("UNKNOWN")
            .trim();
        String::from(label)
    }
}

// ─── Directory Entry ────────────────────────────────────────────────────

/// FAT32 directory entry (8.3 short name format)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fat32DirEntry {
    pub name: [u8; 8],
    pub ext: [u8; 3],
    pub attr: u8,
    pub nt_reserved: u8,
    pub create_time_tenths: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub access_date: u16,
    pub cluster_hi: u16,
    pub modify_time: u16,
    pub modify_date: u16,
    pub cluster_lo: u16,
    pub file_size: u32,
}

impl Fat32DirEntry {
    /// Get the starting cluster number
    pub fn cluster(&self) -> u32 {
        ((self.cluster_hi as u32) << 16) | (self.cluster_lo as u32)
    }

    /// Get the short filename
    pub fn short_name(&self) -> String {
        let name = core::str::from_utf8(&self.name).unwrap_or("").trim_end();
        let ext = core::str::from_utf8(&self.ext).unwrap_or("").trim_end();

        if ext.is_empty() {
            String::from(name)
        } else {
            alloc::format!("{}.{}", name, ext)
        }
    }

    /// Is this a directory?
    pub fn is_directory(&self) -> bool {
        self.attr & ATTR_DIRECTORY != 0
    }

    /// Is this a file?
    pub fn is_file(&self) -> bool {
        !self.is_directory() && self.attr & ATTR_VOLUME_ID == 0
    }

    /// Is this entry free?
    pub fn is_free(&self) -> bool {
        self.name[0] == 0xE5
    }

    /// Is this the last entry?
    pub fn is_last(&self) -> bool {
        self.name[0] == 0x00
    }

    /// Is this a long filename entry?
    pub fn is_lfn(&self) -> bool {
        self.attr == ATTR_LONG_NAME
    }

    /// Is this a volume label?
    pub fn is_volume_label(&self) -> bool {
        self.attr & ATTR_VOLUME_ID != 0 && self.attr != ATTR_LONG_NAME
    }
}

/// Long filename (LFN) directory entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fat32LfnEntry {
    pub order: u8,
    pub name1: [u16; 5], // Characters 1-5
    pub attr: u8,        // Always 0x0F
    pub lfn_type: u8,    // Always 0x00
    pub checksum: u8,
    pub name2: [u16; 6], // Characters 6-11
    pub cluster: u16,    // Always 0x0000
    pub name3: [u16; 2], // Characters 12-13
}

impl Fat32LfnEntry {
    /// Extract characters from this LFN entry
    pub fn chars(&self) -> Vec<char> {
        let mut chars = Vec::with_capacity(LFN_CHARS_PER_ENTRY);

        // Copy fields from packed struct to avoid unaligned references
        let n1 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.name1)) };
        let n2 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.name2)) };
        let n3 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.name3)) };

        for c in n1 {
            if c == 0x0000 || c == 0xFFFF {
                return chars;
            }
            chars.push(char::from_u32(c as u32).unwrap_or('?'));
        }
        for c in n2 {
            if c == 0x0000 || c == 0xFFFF {
                return chars;
            }
            chars.push(char::from_u32(c as u32).unwrap_or('?'));
        }
        for c in n3 {
            if c == 0x0000 || c == 0xFFFF {
                return chars;
            }
            chars.push(char::from_u32(c as u32).unwrap_or('?'));
        }

        chars
    }

    /// Get the sequence number (1-based)
    pub fn sequence(&self) -> u8 {
        self.order & 0x3F
    }

    /// Is this the last LFN entry?
    pub fn is_last(&self) -> bool {
        self.order & LFN_LAST_ENTRY != 0
    }
}

// ─── Directory Entry (parsed) ───────────────────────────────────────────

/// Parsed directory entry with long filename support
#[derive(Debug, Clone)]
pub struct FatDirEntry {
    pub name: String,
    pub short_name: String,
    pub is_dir: bool,
    pub size: u32,
    pub cluster: u32,
    pub attr: u8,
    pub create_time: u32,
    pub modify_time: u32,
}

// ─── FAT32 Filesystem ───────────────────────────────────────────────────

/// Read callback for block device I/O
pub type BlockReadFn = fn(sector: u64, buffer: &mut [u8]) -> bool;
pub type BlockWriteFn = fn(sector: u64, data: &[u8]) -> bool;

/// FAT32 filesystem instance
pub struct Fat32Fs {
    /// Boot sector parameters
    pub bpb: Fat32BootSector,
    /// FAT table cache (partial)
    pub fat_cache: Vec<u32>,
    /// Number of cached FAT entries
    pub fat_cache_start: u32,
    pub fat_cache_count: u32,
    /// Is the filesystem mounted
    pub mounted: bool,
    /// Free cluster count (from FSInfo)
    pub free_clusters: u32,
    /// Next free cluster hint
    pub next_free_cluster: u32,
    /// Volume label
    pub label: String,
    /// Mount point in VFS
    pub mount_point: String,
}

impl Default for Fat32Fs {
    fn default() -> Self {
        Self::new()
    }
}

impl Fat32Fs {
    pub fn new() -> Self {
        Self {
            bpb: unsafe { core::mem::zeroed() },
            fat_cache: Vec::new(),
            fat_cache_start: 0,
            fat_cache_count: 0,
            mounted: false,
            free_clusters: 0,
            next_free_cluster: 2,
            label: String::new(),
            mount_point: String::new(),
        }
    }

    /// Mount a FAT32 filesystem from a block device
    pub fn mount(&mut self, mount_point: &str) -> bool {
        // Read boot sector (sector 0)
        let mut sector_buf = [0u8; SECTOR_SIZE];
        if !read_block_device(0, &mut sector_buf) {
            serial_println!("[FAT32] Failed to read boot sector");
            return false;
        }

        // Parse BPB
        match Fat32BootSector::from_bytes(&sector_buf) {
            Some(bpb) => {
                self.bpb = bpb;
            }
            None => {
                serial_println!("[FAT32] Invalid boot sector");
                return false;
            }
        }

        // Validate FAT32
        let bytes_per_sector = { self.bpb.bytes_per_sector };
        if bytes_per_sector != 512 {
            serial_println!("[FAT32] Unsupported sector size: {}", bytes_per_sector);
            return false;
        }
        if self.bpb.sectors_per_cluster == 0 {
            serial_println!("[FAT32] Invalid sectors per cluster");
            return false;
        }

        self.label = self.bpb.volume_label_string();
        self.mount_point = String::from(mount_point);
        self.mounted = true;

        let root_cluster = { self.bpb.root_cluster };
        let fat_size_32 = { self.bpb.fat_size_32 };

        serial_println!("[FAT32] Mounted at {}", mount_point);
        serial_println!("[FAT32]   Label: '{}'", self.label);
        serial_println!(
            "[FAT32]   Sectors/cluster: {}",
            self.bpb.sectors_per_cluster
        );
        serial_println!("[FAT32]   Total clusters: {}", self.bpb.total_clusters());
        serial_println!("[FAT32]   Root cluster: {}", root_cluster);
        serial_println!("[FAT32]   FAT size: {} sectors", fat_size_32);

        // Read FSInfo sector
        if self.bpb.fs_info_sector > 0 {
            let mut fsinfo_buf = [0u8; SECTOR_SIZE];
            if read_block_device(self.bpb.fs_info_sector as u64, &mut fsinfo_buf) {
                let sig1 = u32::from_le_bytes([
                    fsinfo_buf[0],
                    fsinfo_buf[1],
                    fsinfo_buf[2],
                    fsinfo_buf[3],
                ]);
                let sig2 = u32::from_le_bytes([
                    fsinfo_buf[484],
                    fsinfo_buf[485],
                    fsinfo_buf[486],
                    fsinfo_buf[487],
                ]);
                if sig1 == FAT32_FSINFO_SIGNATURE1 && sig2 == FAT32_FSINFO_SIGNATURE2 {
                    self.free_clusters = u32::from_le_bytes([
                        fsinfo_buf[488],
                        fsinfo_buf[489],
                        fsinfo_buf[490],
                        fsinfo_buf[491],
                    ]);
                    self.next_free_cluster = u32::from_le_bytes([
                        fsinfo_buf[492],
                        fsinfo_buf[493],
                        fsinfo_buf[494],
                        fsinfo_buf[495],
                    ]);
                    serial_println!("[FAT32]   Free clusters: {}", self.free_clusters);
                }
            }
        }

        // Cache part of the FAT
        self.cache_fat(0, 1024); // Cache first 1024 entries

        true
    }

    /// Cache a portion of the FAT table
    fn cache_fat(&mut self, start_entry: u32, count: u32) {
        let fat_start_sector = self.bpb.reserved_sectors as u64;
        let entries_per_sector = (SECTOR_SIZE / 4) as u32; // 128 entries per sector

        self.fat_cache.clear();
        self.fat_cache_start = start_entry;
        self.fat_cache_count = count;

        let start_sector = fat_start_sector + (start_entry / entries_per_sector) as u64;
        let sector_count = count.div_ceil(entries_per_sector);

        for s in 0..sector_count {
            let mut buf = [0u8; SECTOR_SIZE];
            if read_block_device(start_sector + s as u64, &mut buf) {
                for i in (0..SECTOR_SIZE).step_by(4) {
                    let entry = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
                    self.fat_cache.push(entry & FAT32_MASK);
                }
            }
        }
    }

    /// Read a FAT entry (get next cluster in chain)
    pub fn read_fat_entry(&self, cluster: u32) -> u32 {
        // Check cache first
        if cluster >= self.fat_cache_start
            && cluster < self.fat_cache_start + self.fat_cache.len() as u32
        {
            return self.fat_cache[(cluster - self.fat_cache_start) as usize];
        }

        // Read from disk
        let fat_start_sector = self.bpb.reserved_sectors as u64;
        let fat_offset = cluster * 4;
        let sector = fat_start_sector + (fat_offset / SECTOR_SIZE as u32) as u64;
        let offset = (fat_offset % SECTOR_SIZE as u32) as usize;

        let mut buf = [0u8; SECTOR_SIZE];
        if read_block_device(sector, &mut buf) {
            u32::from_le_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]) & FAT32_MASK
        } else {
            FAT32_BAD
        }
    }

    /// Follow a cluster chain and collect all clusters
    pub fn get_cluster_chain(&self, start_cluster: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut cluster = start_cluster;

        while (2..FAT32_EOC).contains(&cluster) {
            chain.push(cluster);
            cluster = self.read_fat_entry(cluster);
            // Safety: prevent infinite loops
            if chain.len() > 1_000_000 {
                break;
            }
        }

        chain
    }

    /// Read a cluster's data
    pub fn read_cluster(&self, cluster: u32) -> Vec<u8> {
        let sector = self.bpb.cluster_to_sector(cluster) as u64;
        let sectors_per_cluster = self.bpb.sectors_per_cluster as usize;
        let cluster_size = sectors_per_cluster * SECTOR_SIZE;
        let mut data = alloc::vec![0u8; cluster_size];

        for s in 0..sectors_per_cluster {
            let offset = s * SECTOR_SIZE;
            read_block_device(sector + s as u64, &mut data[offset..offset + SECTOR_SIZE]);
        }

        data
    }

    /// Read all data from a cluster chain
    pub fn read_chain_data(&self, start_cluster: u32) -> Vec<u8> {
        let chain = self.get_cluster_chain(start_cluster);
        let mut data = Vec::new();
        for cluster in chain {
            data.extend(self.read_cluster(cluster));
        }
        data
    }

    /// Read directory entries from a cluster chain
    pub fn read_directory(&self, start_cluster: u32) -> Vec<FatDirEntry> {
        let data = self.read_chain_data(start_cluster);
        parse_directory_entries(&data)
    }

    /// Read the root directory
    pub fn read_root_directory(&self) -> Vec<FatDirEntry> {
        self.read_directory(self.bpb.root_cluster)
    }

    /// Find a file or directory by path
    pub fn find_entry(&self, path: &str) -> Option<FatDirEntry> {
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if components.is_empty() {
            // Root directory
            return Some(FatDirEntry {
                name: String::from("/"),
                short_name: String::from("/"),
                is_dir: true,
                size: 0,
                cluster: self.bpb.root_cluster,
                attr: ATTR_DIRECTORY,
                create_time: 0,
                modify_time: 0,
            });
        }

        let mut current_cluster = self.bpb.root_cluster;

        for (i, component) in components.iter().enumerate() {
            let entries = self.read_directory(current_cluster);
            let found = entries.iter().find(|e| {
                e.name.eq_ignore_ascii_case(component)
                    || e.short_name.eq_ignore_ascii_case(component)
            });

            match found {
                Some(entry) => {
                    if i == components.len() - 1 {
                        return Some(entry.clone());
                    } else if entry.is_dir {
                        current_cluster = entry.cluster;
                    } else {
                        return None; // Not a directory in the middle of path
                    }
                }
                None => return None,
            }
        }

        None
    }

    /// Read a file's contents
    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let entry = self.find_entry(path)?;
        if entry.is_dir {
            return None;
        }

        let data = self.read_chain_data(entry.cluster);
        // Truncate to actual file size
        Some(data[..entry.size as usize].to_vec())
    }

    /// List directory contents
    pub fn list_directory(&self, path: &str) -> Option<Vec<FatDirEntry>> {
        let entry = self.find_entry(path)?;
        if !entry.is_dir {
            return None;
        }
        Some(self.read_directory(entry.cluster))
    }

    /// Get filesystem statistics
    pub fn stats(&self) -> (u64, u64, u32) {
        let cluster_size = self.bpb.sectors_per_cluster as u64 * SECTOR_SIZE as u64;
        let total = self.bpb.total_clusters() as u64 * cluster_size;
        let free = self.free_clusters as u64 * cluster_size;
        (
            total,
            free,
            self.bpb.sectors_per_cluster as u32 * SECTOR_SIZE as u32,
        )
    }

    // ─── Write Operations ───────────────────────────────────────────────

    /// Write a FAT entry (set next cluster in chain)
    pub fn write_fat_entry(&mut self, cluster: u32, value: u32) -> bool {
        let fat_start_sector = self.bpb.reserved_sectors as u64;
        let fat_offset = cluster * 4;
        let sector = fat_start_sector + (fat_offset / SECTOR_SIZE as u32) as u64;
        let offset = (fat_offset % SECTOR_SIZE as u32) as usize;

        let mut buf = [0u8; SECTOR_SIZE];
        if !read_block_device(sector, &mut buf) {
            return false;
        }

        // Preserve upper 4 bits of existing entry
        let existing = u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]);
        let new_val = (existing & 0xF0000000) | (value & FAT32_MASK);
        buf[offset..offset + 4].copy_from_slice(&new_val.to_le_bytes());

        // Write to all FATs
        let num_fats = self.bpb.num_fats;
        let fat_size = self.bpb.fat_size_32 as u64;
        for fat in 0..num_fats as u64 {
            let fat_sector = sector + fat * fat_size;
            if !write_block_device(fat_sector, &buf) {
                return false;
            }
        }

        // Update cache if in range
        if cluster >= self.fat_cache_start
            && cluster < self.fat_cache_start + self.fat_cache.len() as u32
        {
            self.fat_cache[(cluster - self.fat_cache_start) as usize] = value & FAT32_MASK;
        }

        true
    }

    /// Allocate a free cluster
    pub fn allocate_cluster(&mut self) -> Option<u32> {
        let total = self.bpb.total_clusters() + 2;
        let start = if self.next_free_cluster >= 2 {
            self.next_free_cluster
        } else {
            2
        };

        // Search from hint
        for c in start..total {
            if self.read_fat_entry(c) == FAT32_FREE {
                // Mark as end-of-chain
                self.write_fat_entry(c, FAT32_EOC);
                self.next_free_cluster = c + 1;
                if self.free_clusters > 0 {
                    self.free_clusters -= 1;
                }
                return Some(c);
            }
        }
        // Wrap around
        for c in 2..start {
            if self.read_fat_entry(c) == FAT32_FREE {
                self.write_fat_entry(c, FAT32_EOC);
                self.next_free_cluster = c + 1;
                if self.free_clusters > 0 {
                    self.free_clusters -= 1;
                }
                return Some(c);
            }
        }

        None // Disk full
    }

    /// Allocate a chain of clusters
    pub fn allocate_chain(&mut self, count: u32) -> Option<u32> {
        if count == 0 {
            return None;
        }

        let first = self.allocate_cluster()?;
        let mut prev = first;

        for _ in 1..count {
            let next = self.allocate_cluster()?;
            self.write_fat_entry(prev, next);
            prev = next;
        }

        Some(first)
    }

    /// Free a cluster chain
    pub fn free_chain(&mut self, start_cluster: u32) {
        let chain = self.get_cluster_chain(start_cluster);
        for cluster in chain {
            self.write_fat_entry(cluster, FAT32_FREE);
            self.free_clusters += 1;
        }
    }

    /// Write data to a cluster
    pub fn write_cluster(&self, cluster: u32, data: &[u8]) -> bool {
        let sector = self.bpb.cluster_to_sector(cluster) as u64;
        let sectors_per_cluster = self.bpb.sectors_per_cluster as usize;
        let cluster_size = sectors_per_cluster * SECTOR_SIZE;

        for s in 0..sectors_per_cluster {
            let offset = s * SECTOR_SIZE;
            let end = core::cmp::min(offset + SECTOR_SIZE, data.len());
            if offset >= data.len() {
                // Zero-fill remaining sectors
                let zeros = [0u8; SECTOR_SIZE];
                if !write_block_device(sector + s as u64, &zeros) {
                    return false;
                }
            } else {
                let mut buf = [0u8; SECTOR_SIZE];
                let copy_len = end - offset;
                buf[..copy_len].copy_from_slice(&data[offset..end]);
                if !write_block_device(sector + s as u64, &buf) {
                    return false;
                }
            }
        }
        true
    }

    /// Write file data to a cluster chain, extending as needed
    pub fn write_chain_data(&mut self, start_cluster: u32, data: &[u8]) -> bool {
        let cluster_size = self.bpb.sectors_per_cluster as usize * SECTOR_SIZE;
        let clusters_needed = data.len().div_ceil(cluster_size);
        let mut chain = self.get_cluster_chain(start_cluster);

        // Extend chain if more clusters needed
        while chain.len() < clusters_needed {
            let new = match self.allocate_cluster() {
                Some(c) => c,
                None => return false,
            };
            if let Some(&last) = chain.last() {
                self.write_fat_entry(last, new);
            }
            chain.push(new);
        }

        // Truncate chain if fewer clusters needed
        if clusters_needed < chain.len() {
            // Mark new end
            if clusters_needed > 0 {
                self.write_fat_entry(chain[clusters_needed - 1], FAT32_EOC);
            }
            // Free remaining clusters
            for item in chain.iter().skip(clusters_needed) {
                self.write_fat_entry(*item, FAT32_FREE);
                self.free_clusters += 1;
            }
            chain.truncate(clusters_needed);
        }

        // Write data
        for (i, &cluster) in chain.iter().enumerate() {
            let offset = i * cluster_size;
            let end = core::cmp::min(offset + cluster_size, data.len());
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            cluster_buf[..end - offset].copy_from_slice(&data[offset..end]);
            if !self.write_cluster(cluster, &cluster_buf) {
                return false;
            }
        }

        true
    }

    /// Generate an 8.3 short name from a long filename
    fn generate_short_name(name: &str) -> [u8; 11] {
        let mut short = [0x20u8; 11]; // Space-filled

        let upper = name.to_ascii_uppercase();
        let (base, ext) = match upper.rfind('.') {
            Some(pos) => (&upper[..pos], &upper[pos + 1..]),
            None => (upper.as_str(), ""),
        };

        let base_bytes: Vec<u8> = base
            .bytes()
            .filter(|&b| b != b' ' && b != b'.')
            .take(8)
            .collect();
        let ext_bytes: Vec<u8> = ext.bytes().filter(|&b| b != b' ').take(3).collect();

        for (i, &b) in base_bytes.iter().enumerate() {
            short[i] = b;
        }
        for (i, &b) in ext_bytes.iter().enumerate() {
            short[8 + i] = b;
        }

        short
    }

    /// Compute checksum for a short name (used by LFN entries)
    fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
        let mut sum: u8 = 0;
        for &b in short_name.iter() {
            sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(b);
        }
        sum
    }

    /// Create directory entry data (short name + optional LFN entries)
    fn create_dir_entry_data(name: &str, attr: u8, cluster: u32, size: u32) -> Vec<u8> {
        let short_name = Self::generate_short_name(name);
        let checksum = Self::lfn_checksum(&short_name);
        let mut entries_data = Vec::new();

        // Create LFN entries if name doesn't fit in 8.3
        let needs_lfn = name.len() > 11 || name.contains('.') && !name.is_ascii();
        if needs_lfn || !name.is_empty() {
            let chars: Vec<u16> = name.encode_utf16().collect();
            let lfn_count = chars.len().div_ceil(LFN_CHARS_PER_ENTRY);

            // LFN entries go in reverse order
            for seq in (1..=lfn_count).rev() {
                let mut lfn = [0xFFu8; DIR_ENTRY_SIZE];
                let order = if seq == lfn_count {
                    seq as u8 | LFN_LAST_ENTRY
                } else {
                    seq as u8
                };
                lfn[0] = order;
                lfn[11] = ATTR_LONG_NAME;
                lfn[12] = 0; // Type
                lfn[13] = checksum;
                lfn[26] = 0; // Cluster = 0
                lfn[27] = 0;

                // Fill name characters
                let start = (seq - 1) * LFN_CHARS_PER_ENTRY;
                let char_offsets = [
                    (1, 5),  // name1: bytes 1-10, 5 chars
                    (14, 6), // name2: bytes 14-25, 6 chars
                    (28, 2), // name3: bytes 28-31, 2 chars
                ];

                let mut char_idx = start;
                for (byte_offset, count) in char_offsets {
                    for j in 0..count {
                        let c = if char_idx < chars.len() {
                            chars[char_idx]
                        } else if char_idx == chars.len() {
                            0x0000 // Null terminator
                        } else {
                            0xFFFF // Padding
                        };
                        let pos = byte_offset + j * 2;
                        lfn[pos] = (c & 0xFF) as u8;
                        lfn[pos + 1] = (c >> 8) as u8;
                        char_idx += 1;
                    }
                }

                entries_data.extend_from_slice(&lfn);
            }
        }

        // Short name entry
        let mut entry = [0u8; DIR_ENTRY_SIZE];
        entry[0..11].copy_from_slice(&short_name);
        entry[11] = attr;
        // Cluster
        entry[20] = ((cluster >> 16) & 0xFF) as u8;
        entry[21] = ((cluster >> 24) & 0xFF) as u8;
        entry[26] = (cluster & 0xFF) as u8;
        entry[27] = ((cluster >> 8) & 0xFF) as u8;
        // Size
        entry[28..32].copy_from_slice(&size.to_le_bytes());

        entries_data.extend_from_slice(&entry);
        entries_data
    }

    /// Add an entry to a directory
    fn add_entry_to_dir(
        &mut self,
        dir_cluster: u32,
        name: &str,
        attr: u8,
        cluster: u32,
        size: u32,
    ) -> bool {
        let entry_data = Self::create_dir_entry_data(name, attr, cluster, size);
        let entries_needed = entry_data.len() / DIR_ENTRY_SIZE;

        let data = self.read_chain_data(dir_cluster);
        let num_entries = data.len() / DIR_ENTRY_SIZE;

        // Find consecutive free slots
        let mut consecutive = 0;
        let mut insert_idx = None;

        for i in 0..num_entries {
            let offset = i * DIR_ENTRY_SIZE;
            if data[offset] == 0x00 || data[offset] == 0xE5 {
                consecutive += 1;
                if consecutive >= entries_needed {
                    insert_idx = Some(i + 1 - entries_needed);
                    break;
                }
            } else {
                consecutive = 0;
            }
        }

        // If no space, extend the directory
        if insert_idx.is_none() {
            // Need to allocate more clusters for the directory
            let chain = self.get_cluster_chain(dir_cluster);
            let new_cluster = match self.allocate_cluster() {
                Some(c) => c,
                None => return false,
            };
            if let Some(&last) = chain.last() {
                self.write_fat_entry(last, new_cluster);
            }
            // Zero out the new cluster
            let cluster_size = self.bpb.sectors_per_cluster as usize * SECTOR_SIZE;
            let zeros = alloc::vec![0u8; cluster_size];
            self.write_cluster(new_cluster, &zeros);
            insert_idx = Some(num_entries);
        }

        let insert_at = insert_idx.unwrap();

        // Write the entry data
        let mut data = self.read_chain_data(dir_cluster);
        let byte_offset = insert_at * DIR_ENTRY_SIZE;

        // Ensure data is big enough
        if byte_offset + entry_data.len() > data.len() {
            data.resize(byte_offset + entry_data.len(), 0);
        }

        data[byte_offset..byte_offset + entry_data.len()].copy_from_slice(&entry_data);

        // Write back
        self.write_chain_data(dir_cluster, &data)
    }

    /// Create a new file
    pub fn create_file(&mut self, path: &str) -> bool {
        let (parent_path, file_name) = match path.rfind('/') {
            Some(pos) => {
                let parent = if pos == 0 { "/" } else { &path[..pos] };
                (parent, &path[pos + 1..])
            }
            None => ("/", path),
        };

        let parent = match self.find_entry(parent_path) {
            Some(e) if e.is_dir => e,
            _ => return false,
        };

        // Check if file already exists
        if self.find_entry(path).is_some() {
            return false;
        }

        // Create with no data cluster initially
        self.add_entry_to_dir(parent.cluster, file_name, ATTR_ARCHIVE, 0, 0)
    }

    /// Write data to a file (overwrite)
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> bool {
        let entry = match self.find_entry(path) {
            Some(e) if !e.is_dir => e,
            _ => return false,
        };

        let cluster = if entry.cluster < 2 {
            // File has no data — allocate first cluster
            match self.allocate_cluster() {
                Some(c) => c,
                None => return false,
            }
        } else {
            entry.cluster
        };

        // Write data to cluster chain
        if !self.write_chain_data(cluster, data) {
            return false;
        }

        // Update directory entry with new cluster and size
        self.update_dir_entry(path, cluster, data.len() as u32)
    }

    /// Update a directory entry's cluster and size fields
    fn update_dir_entry(&mut self, path: &str, new_cluster: u32, new_size: u32) -> bool {
        let (parent_path, file_name) = match path.rfind('/') {
            Some(pos) => {
                let parent = if pos == 0 { "/" } else { &path[..pos] };
                (parent, &path[pos + 1..])
            }
            None => ("/", path),
        };

        let parent = match self.find_entry(parent_path) {
            Some(e) if e.is_dir => e,
            _ => return false,
        };

        let data = self.read_chain_data(parent.cluster);
        let num_entries = data.len() / DIR_ENTRY_SIZE;
        let mut data = data;

        for i in 0..num_entries {
            let offset = i * DIR_ENTRY_SIZE;
            let raw = unsafe { *(data[offset..].as_ptr() as *const Fat32DirEntry) };

            if raw.is_last() {
                break;
            }
            if raw.is_free() || raw.is_lfn() || raw.is_volume_label() {
                continue;
            }

            let name = raw.short_name();
            if name.eq_ignore_ascii_case(file_name) {
                // Update cluster
                data[offset + 20] = ((new_cluster >> 16) & 0xFF) as u8;
                data[offset + 21] = ((new_cluster >> 24) & 0xFF) as u8;
                data[offset + 26] = (new_cluster & 0xFF) as u8;
                data[offset + 27] = ((new_cluster >> 8) & 0xFF) as u8;
                // Update size
                data[offset + 28..offset + 32].copy_from_slice(&new_size.to_le_bytes());

                return self.write_chain_data(parent.cluster, &data);
            }
        }

        false
    }

    /// Create a directory
    pub fn mkdir(&mut self, path: &str) -> bool {
        let (parent_path, dir_name) = match path.rfind('/') {
            Some(pos) => {
                let parent = if pos == 0 { "/" } else { &path[..pos] };
                (parent, &path[pos + 1..])
            }
            None => ("/", path),
        };

        let parent = match self.find_entry(parent_path) {
            Some(e) if e.is_dir => e,
            _ => return false,
        };

        // Check if already exists
        if self.find_entry(path).is_some() {
            return false;
        }

        // Allocate cluster for new directory
        let cluster = match self.allocate_cluster() {
            Some(c) => c,
            None => return false,
        };

        // Initialize directory with . and .. entries
        let cluster_size = self.bpb.sectors_per_cluster as usize * SECTOR_SIZE;
        let mut dir_data = alloc::vec![0u8; cluster_size];

        // . entry
        let mut dot = [0x20u8; DIR_ENTRY_SIZE];
        dot[0] = b'.';
        dot[11] = ATTR_DIRECTORY;
        dot[20] = ((cluster >> 16) & 0xFF) as u8;
        dot[21] = ((cluster >> 24) & 0xFF) as u8;
        dot[26] = (cluster & 0xFF) as u8;
        dot[27] = ((cluster >> 8) & 0xFF) as u8;
        dir_data[..DIR_ENTRY_SIZE].copy_from_slice(&dot);

        // .. entry
        let mut dotdot = [0x20u8; DIR_ENTRY_SIZE];
        dotdot[0] = b'.';
        dotdot[1] = b'.';
        dotdot[11] = ATTR_DIRECTORY;
        let parent_c = parent.cluster;
        dotdot[20] = ((parent_c >> 16) & 0xFF) as u8;
        dotdot[21] = ((parent_c >> 24) & 0xFF) as u8;
        dotdot[26] = (parent_c & 0xFF) as u8;
        dotdot[27] = ((parent_c >> 8) & 0xFF) as u8;
        dir_data[DIR_ENTRY_SIZE..2 * DIR_ENTRY_SIZE].copy_from_slice(&dotdot);

        // Write directory cluster
        if !self.write_cluster(cluster, &dir_data) {
            return false;
        }

        // Add entry to parent directory
        self.add_entry_to_dir(parent.cluster, dir_name, ATTR_DIRECTORY, cluster, 0)
    }

    /// Delete a file
    pub fn delete_file(&mut self, path: &str) -> bool {
        let entry = match self.find_entry(path) {
            Some(e) if !e.is_dir => e,
            _ => return false,
        };

        // Free the cluster chain
        if entry.cluster >= 2 {
            self.free_chain(entry.cluster);
        }

        // Mark directory entry as deleted
        self.mark_entry_deleted(path)
    }

    /// Delete an empty directory
    pub fn rmdir(&mut self, path: &str) -> bool {
        let entry = match self.find_entry(path) {
            Some(e) if e.is_dir => e,
            _ => return false,
        };

        // Check directory is empty (only . and ..)
        let contents = self.read_directory(entry.cluster);
        if !contents.is_empty() {
            return false; // Not empty
        }

        // Free cluster chain
        self.free_chain(entry.cluster);

        // Mark directory entry as deleted
        self.mark_entry_deleted(path)
    }

    /// Mark a directory entry as deleted (set first byte to 0xE5)
    fn mark_entry_deleted(&mut self, path: &str) -> bool {
        let (parent_path, file_name) = match path.rfind('/') {
            Some(pos) => {
                let parent = if pos == 0 { "/" } else { &path[..pos] };
                (parent, &path[pos + 1..])
            }
            None => ("/", path),
        };

        let parent = match self.find_entry(parent_path) {
            Some(e) if e.is_dir => e,
            _ => return false,
        };

        let data = self.read_chain_data(parent.cluster);
        let num_entries = data.len() / DIR_ENTRY_SIZE;
        let mut data = data;

        // Find and mark as deleted (including LFN entries)
        let mut found_lfn_start = None;
        for i in 0..num_entries {
            let offset = i * DIR_ENTRY_SIZE;
            let raw = unsafe { *(data[offset..].as_ptr() as *const Fat32DirEntry) };

            if raw.is_last() {
                break;
            }

            if raw.is_lfn() {
                if found_lfn_start.is_none() {
                    found_lfn_start = Some(i);
                }
                continue;
            }

            if raw.is_free() || raw.is_volume_label() {
                found_lfn_start = None;
                continue;
            }

            let name = raw.short_name();
            if name.eq_ignore_ascii_case(file_name) {
                // Mark this entry as deleted
                data[offset] = 0xE5;

                // Also mark associated LFN entries
                if let Some(start) = found_lfn_start {
                    for j in start..i {
                        data[j * DIR_ENTRY_SIZE] = 0xE5;
                    }
                }

                return self.write_chain_data(parent.cluster, &data);
            }

            found_lfn_start = None;
        }

        false
    }

    /// Update FSInfo sector
    pub fn sync_fsinfo(&self) -> bool {
        if self.bpb.fs_info_sector == 0 {
            return true;
        }

        let mut buf = [0u8; SECTOR_SIZE];
        if !read_block_device(self.bpb.fs_info_sector as u64, &mut buf) {
            return false;
        }

        // Update free cluster count and next free hint
        buf[488..492].copy_from_slice(&self.free_clusters.to_le_bytes());
        buf[492..496].copy_from_slice(&self.next_free_cluster.to_le_bytes());

        write_block_device(self.bpb.fs_info_sector as u64, &buf)
    }
}

// ─── Directory Parsing ──────────────────────────────────────────────────

/// Parse directory entries from raw data, handling LFN
fn parse_directory_entries(data: &[u8]) -> Vec<FatDirEntry> {
    let mut entries = Vec::new();
    let mut lfn_parts: Vec<(u8, Vec<char>)> = Vec::new();
    let num_entries = data.len() / DIR_ENTRY_SIZE;

    for i in 0..num_entries {
        let offset = i * DIR_ENTRY_SIZE;
        if offset + DIR_ENTRY_SIZE > data.len() {
            break;
        }

        let raw = unsafe { *(data[offset..].as_ptr() as *const Fat32DirEntry) };

        if raw.is_last() {
            break;
        }
        if raw.is_free() {
            lfn_parts.clear();
            continue;
        }

        if raw.is_lfn() {
            // Parse LFN entry
            let lfn = unsafe { *(data[offset..].as_ptr() as *const Fat32LfnEntry) };
            lfn_parts.push((lfn.sequence(), lfn.chars()));
        } else if raw.is_volume_label() {
            lfn_parts.clear();
            continue;
        } else {
            // Normal (8.3) entry with optional LFN
            let short_name = raw.short_name();

            // Skip . and .. entries
            if short_name == "." || short_name == ".." {
                lfn_parts.clear();
                continue;
            }

            // Reconstruct long filename from LFN entries
            let name = if !lfn_parts.is_empty() {
                lfn_parts.sort_by_key(|(seq, _)| *seq);
                let mut long_name = String::new();
                for (_, chars) in &lfn_parts {
                    for &c in chars {
                        long_name.push(c);
                    }
                }
                lfn_parts.clear();
                long_name
            } else {
                short_name.clone()
            };

            entries.push(FatDirEntry {
                name,
                short_name,
                is_dir: raw.is_directory(),
                size: raw.file_size,
                cluster: raw.cluster(),
                attr: raw.attr,
                create_time: ((raw.create_date as u32) << 16) | raw.create_time as u32,
                modify_time: ((raw.modify_date as u32) << 16) | raw.modify_time as u32,
            });
        }
    }

    entries
}

// ─── Block Device I/O ───────────────────────────────────────────────────

/// Read a sector from the block device
fn read_block_device(sector: u64, buffer: &mut [u8]) -> bool {
    // Try virtio-blk first, then fall back to ramdisk
    if crate::virtio_blk::is_available() {
        crate::virtio_blk::read(sector, 1, buffer)
    } else {
        // Use ramdisk from block.rs
        crate::block::read_blocks(0, sector, 1, buffer).is_ok()
    }
}

/// Write a sector to the block device
fn write_block_device(sector: u64, data: &[u8]) -> bool {
    if crate::virtio_blk::is_available() {
        crate::virtio_blk::write(sector, 1, data)
    } else {
        crate::block::write_blocks(0, sector, 1, data).is_ok()
    }
}

// ─── Global Filesystem Instance ─────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref FAT32_FS: Mutex<Fat32Fs> = Mutex::new(Fat32Fs::new());
}

/// Mount FAT32 filesystem
pub fn mount(mount_point: &str) -> bool {
    FAT32_FS.lock().mount(mount_point)
}

/// Read a file from FAT32
pub fn read_file(path: &str) -> Option<Vec<u8>> {
    FAT32_FS.lock().read_file(path)
}

/// List a directory
pub fn list_dir(path: &str) -> Option<Vec<FatDirEntry>> {
    FAT32_FS.lock().list_directory(path)
}

/// Find an entry by path
pub fn find(path: &str) -> Option<FatDirEntry> {
    FAT32_FS.lock().find_entry(path)
}

/// Check if FAT32 is mounted
pub fn is_mounted() -> bool {
    FAT32_FS.lock().mounted
}

/// Create a new file
pub fn create_file(path: &str) -> bool {
    FAT32_FS.lock().create_file(path)
}

/// Write data to a file
pub fn write_file(path: &str, data: &[u8]) -> bool {
    FAT32_FS.lock().write_file(path, data)
}

/// Create a directory
pub fn mkdir(path: &str) -> bool {
    FAT32_FS.lock().mkdir(path)
}

/// Delete a file
pub fn delete_file(path: &str) -> bool {
    FAT32_FS.lock().delete_file(path)
}

/// Remove an empty directory
pub fn rmdir(path: &str) -> bool {
    FAT32_FS.lock().rmdir(path)
}

/// Sync FSInfo
pub fn sync() -> bool {
    FAT32_FS.lock().sync_fsinfo()
}

/// Initialize FAT32 driver
pub fn init() {
    serial_println!("[FAT32] FAT32 filesystem driver initialized");

    // Try to auto-mount if a block device is available
    if crate::virtio_blk::is_available() && mount("/mnt") {
        serial_println!("[FAT32] Auto-mounted virtio-blk at /mnt");
    }
}
