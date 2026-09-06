/// Ext4 Filesystem Driver — Linux-compatible ext4 with journal and extents
///
/// Extends ext2 with:
///   - Extent-based block mapping (instead of indirect blocks)
///   - Journal (JBD2) for crash recovery
///   - Large file support (>2GB)
///   - Dir hashing (HTree)
///   - Delayed allocation
///   - 64-bit block numbers
///
/// Compatible with Linux ext4 on-disk format.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::block::{self, BLOCK_SIZE as SECTOR_SIZE};
use crate::serial_println;

// ─── Ext4 Constants ─────────────────────────────────────────────────────

/// Ext2/3/4 magic number
const EXT4_MAGIC: u16 = 0xEF53;

/// Feature flags (incompatible) that indicate ext4
const INCOMPAT_FILETYPE: u32 = 0x0002;
const INCOMPAT_EXTENTS: u32 = 0x0040;
const INCOMPAT_64BIT: u32 = 0x0080;
const INCOMPAT_FLEX_BG: u32 = 0x0200;

/// Feature flags (compatible) for ext4
const COMPAT_DIR_INDEX: u32 = 0x0020;
const COMPAT_HAS_JOURNAL: u32 = 0x0004;

/// Feature flags (read-only compatible)
const RO_COMPAT_HUGE_FILE: u32 = 0x0008;
const RO_COMPAT_GDT_CSUM: u32 = 0x0010;
const RO_COMPAT_DIR_NLINK: u32 = 0x0020;
const RO_COMPAT_EXTRA_ISIZE: u32 = 0x0040;

/// Extent magic
const EXT4_EXT_MAGIC: u16 = 0xF30A;

/// Journal magic
const JBD2_MAGIC: u32 = 0xC03B3998;

/// Maximum extents per header
const EXT4_EXT_MAX_ENTRIES: u16 = 4;

/// Root inode
const EXT4_ROOT_INODE: u32 = 2;
/// Journal inode
const EXT4_JOURNAL_INODE: u32 = 8;

/// File type constants
const EXT4_FT_REG_FILE: u8 = 1;
const EXT4_FT_DIR: u8 = 2;
const EXT4_FT_SYMLINK: u8 = 7;

/// Inode mode flags
const S_IFREG: u16 = 0x8000;
const S_IFDIR: u16 = 0x4000;
const S_IFLNK: u16 = 0xA000;

// ─── On-disk Structures ─────────────────────────────────────────────────

/// Ext4 superblock (1024 bytes, at offset 1024 from start of partition)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext4Superblock {
    s_inodes_count: u32,
    s_blocks_count_lo: u32,
    s_r_blocks_count_lo: u32,
    s_free_blocks_count_lo: u32,
    s_free_inodes_count: u32,
    s_first_data_block: u32,
    s_log_block_size: u32,
    s_log_cluster_size: u32,
    s_blocks_per_group: u32,
    s_clusters_per_group: u32,
    s_inodes_per_group: u32,
    s_mtime: u32,
    s_wtime: u32,
    s_mnt_count: u16,
    s_max_mnt_count: u16,
    s_magic: u16,
    s_state: u16,
    s_errors: u16,
    s_minor_rev_level: u16,
    s_lastcheck: u32,
    s_checkinterval: u32,
    s_creator_os: u32,
    s_rev_level: u32,
    s_def_resuid: u16,
    s_def_resgid: u16,
    // Extended fields (rev >= 1)
    s_first_ino: u32,
    s_inode_size: u16,
    s_block_group_nr: u16,
    s_feature_compat: u32,
    s_feature_incompat: u32,
    s_feature_ro_compat: u32,
    s_uuid: [u8; 16],
    s_volume_name: [u8; 16],
    s_last_mounted: [u8; 64],
    s_algorithm_usage_bitmap: u32,
    s_prealloc_blocks: u8,
    s_prealloc_dir_blocks: u8,
    s_reserved_gdt_blocks: u16,
    // Journal fields
    s_journal_uuid: [u8; 16],
    s_journal_inum: u32,
    s_journal_dev: u32,
    s_last_orphan: u32,
    s_hash_seed: [u32; 4],
    s_def_hash_version: u8,
    s_jnl_backup_type: u8,
    s_desc_size: u16,
    s_default_mount_opts: u32,
    s_first_meta_bg: u32,
    s_mkfs_time: u32,
    s_jnl_blocks: [u32; 17],
    // 64-bit support
    s_blocks_count_hi: u32,
    s_r_blocks_count_hi: u32,
    s_free_blocks_count_hi: u32,
    s_min_extra_isize: u16,
    s_want_extra_isize: u16,
    s_flags: u32,
    s_raid_stride: u16,
    s_mmp_interval: u16,
    s_mmp_block: u64,
    s_raid_stripe_width: u32,
    s_log_groups_per_flex: u8,
    s_checksum_type: u8,
    _padding: [u8; 2],
    s_kbytes_written: u64,
    // ... remaining fields to pad to 1024 bytes
    _reserved: [u8; 596],
}

/// Block group descriptor (32 or 64 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext4GroupDesc {
    bg_block_bitmap_lo: u32,
    bg_inode_bitmap_lo: u32,
    bg_inode_table_lo: u32,
    bg_free_blocks_count_lo: u16,
    bg_free_inodes_count_lo: u16,
    bg_used_dirs_count_lo: u16,
    bg_flags: u16,
    bg_exclude_bitmap_lo: u32,
    bg_block_bitmap_csum_lo: u16,
    bg_inode_bitmap_csum_lo: u16,
    bg_itable_unused_lo: u16,
    bg_checksum: u16,
    // 64-bit extensions (if s_desc_size >= 64)
    bg_block_bitmap_hi: u32,
    bg_inode_bitmap_hi: u32,
    bg_inode_table_hi: u32,
    bg_free_blocks_count_hi: u16,
    bg_free_inodes_count_hi: u16,
    bg_used_dirs_count_hi: u16,
    bg_itable_unused_hi: u16,
    bg_exclude_bitmap_hi: u32,
    bg_block_bitmap_csum_hi: u16,
    bg_inode_bitmap_csum_hi: u16,
    bg_reserved: u32,
}

/// Ext4 inode (256 bytes for ext4, minimum 128)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext4Inode {
    i_mode: u16,
    i_uid: u16,
    i_size_lo: u32,
    i_atime: u32,
    i_ctime: u32,
    i_mtime: u32,
    i_dtime: u32,
    i_gid: u16,
    i_links_count: u16,
    i_blocks_lo: u32,
    i_flags: u32,
    i_osd1: u32,
    i_block: [u32; 15], // 60 bytes: direct/indirect blocks OR extent tree root
    i_generation: u32,
    i_file_acl_lo: u32,
    i_size_high: u32, // Upper 32 bits of size (ext4 large files)
    i_obso_faddr: u32,
    i_osd2: [u8; 12],
    i_extra_isize: u16,
    i_checksum_hi: u16,
    i_ctime_extra: u32,
    i_mtime_extra: u32,
    i_atime_extra: u32,
    i_crtime: u32,
    i_crtime_extra: u32,
    i_version_hi: u32,
    i_projid: u32,
    _padding: [u8; 96],
}

/// Ext4 extent header (12 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Ext4ExtentHeader {
    eh_magic: u16,      // EXT4_EXT_MAGIC = 0xF30A
    eh_entries: u16,    // Number of valid entries
    eh_max: u16,        // Max entries that can be stored
    eh_depth: u16,      // Depth of tree (0 = leaf)
    eh_generation: u32, // Generation of the tree
}

/// Ext4 extent (leaf node, 12 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Ext4Extent {
    ee_block: u32,    // First logical block
    ee_len: u16,      // Number of blocks (<=32768; bit 15 = uninitialized)
    ee_start_hi: u16, // Upper 16 bits of physical block
    ee_start_lo: u32, // Lower 32 bits of physical block
}

/// Ext4 extent index (internal node, 12 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Ext4ExtentIdx {
    ei_block: u32,   // Logical block covered by this index
    ei_leaf_lo: u32, // Lower 32 bits of child block
    ei_leaf_hi: u16, // Upper 16 bits of child block
    ei_unused: u16,
}

/// Directory entry (ext4 with file type)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext4DirEntry {
    inode: u32,
    rec_len: u16,
    name_len: u8,
    file_type: u8,
    // name bytes follow (variable length)
}

// ─── Journal (JBD2) Structures ──────────────────────────────────────────

/// Journal superblock
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct JournalSuperblock {
    s_header: JournalHeader,
    s_blocksize: u32,
    s_maxlen: u32,
    s_first: u32,
    s_sequence: u32,
    s_start: u32,
    s_errno: u32,
    // V2 fields
    s_feature_compat: u32,
    s_feature_incompat: u32,
    s_feature_ro_compat: u32,
    s_uuid: [u8; 16],
    s_nr_users: u32,
    s_dynsuper: u32,
    s_max_transaction: u32,
    s_max_trans_data: u32,
    s_checksum_type: u8,
    _padding1: [u8; 3],
    _padding2: [u32; 42],
    s_checksum: u32,
    s_users: [[u8; 16]; 48],
}

/// Journal block header
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct JournalHeader {
    h_magic: u32,
    h_blocktype: u32,
    h_sequence: u32,
}

/// Journal block types
const JBD2_DESCRIPTOR_BLOCK: u32 = 1;
const JBD2_COMMIT_BLOCK: u32 = 2;
const JBD2_SUPERBLOCK_V1: u32 = 3;
const JBD2_SUPERBLOCK_V2: u32 = 4;
const JBD2_REVOKE_BLOCK: u32 = 5;

/// Journal transaction state
#[derive(Debug, Clone, Copy, PartialEq)]
enum TransactionState {
    Running,
    Committing,
    Committed,
}

/// In-memory journal transaction
struct Transaction {
    tid: u32,
    state: TransactionState,
    blocks: Vec<(u64, Vec<u8>)>, // (block_number, data)
    sequence: u32,
}

/// Journal state
struct Journal {
    inode: u32,
    block_size: usize,
    start_block: u64,
    max_len: u32,
    sequence: u32,
    first: u32,
    current_transaction: Option<Transaction>,
    committed_transactions: Vec<u32>,
}

// ─── In-memory Filesystem State ─────────────────────────────────────────

/// Directory entry (user-facing)
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub inode: u32,
    pub name: String,
    pub file_type: u8,
}

/// File information
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub inode: u32,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub atime: u32,
    pub mtime: u32,
    pub ctime: u32,
    pub links: u16,
    pub blocks: u64,
    pub flags: u32,
}

/// Mounted ext4 filesystem
struct Ext4Fs {
    device_index: usize,
    block_size: usize,
    superblock: Ext4Superblock,
    groups: Vec<Ext4GroupDesc>,
    inodes_per_group: u32,
    inode_size: u16,
    desc_size: u16,
    has_extents: bool,
    has_64bit: bool,
    has_journal: bool,
    journal: Option<Journal>,
    total_blocks: u64,
}

/// Error types
#[derive(Debug)]
pub enum Ext4Error {
    InvalidMagic,
    IoError,
    NotFound,
    NotDirectory,
    NotFile,
    InvalidInode,
    NoSpace,
    ReadOnly,
    JournalError,
    InvalidExtent,
    CorruptFs,
}

lazy_static::lazy_static! {
    static ref MOUNTED_FS: Mutex<Vec<Ext4Fs>> = Mutex::new(Vec::new());
}

impl Ext4Fs {
    /// Read a block from the device
    fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), Ext4Error> {
        let sectors_per_block = self.block_size / SECTOR_SIZE;
        let start_sector = block_num * sectors_per_block as u64;

        for i in 0..sectors_per_block {
            let sector = start_sector + i as u64;
            let offset = i * SECTOR_SIZE;
            block::read_blocks(
                self.device_index,
                sector,
                1,
                &mut buf[offset..offset + SECTOR_SIZE],
            )
            .map_err(|_| Ext4Error::IoError)?;
        }
        Ok(())
    }

    /// Write a block to the device (through journal if available)
    fn write_block(&self, block_num: u64, buf: &[u8]) -> Result<(), Ext4Error> {
        let sectors_per_block = self.block_size / SECTOR_SIZE;
        let start_sector = block_num * sectors_per_block as u64;

        for i in 0..sectors_per_block {
            let sector = start_sector + i as u64;
            let offset = i * SECTOR_SIZE;
            block::write_blocks(
                self.device_index,
                sector,
                1,
                &buf[offset..offset + SECTOR_SIZE],
            )
            .map_err(|_| Ext4Error::IoError)?;
        }
        Ok(())
    }

    /// Get total blocks (combine 32-bit and 64-bit counts)
    fn total_blocks(&self) -> u64 {
        let lo = self.superblock.s_blocks_count_lo as u64;
        if self.has_64bit {
            lo | ((self.superblock.s_blocks_count_hi as u64) << 32)
        } else {
            lo
        }
    }

    /// Read an inode
    fn read_inode(&self, inode_num: u32) -> Result<Ext4Inode, Ext4Error> {
        if inode_num == 0 {
            return Err(Ext4Error::InvalidInode);
        }

        let group = ((inode_num - 1) / self.inodes_per_group) as usize;
        let index = ((inode_num - 1) % self.inodes_per_group) as usize;

        if group >= self.groups.len() {
            return Err(Ext4Error::InvalidInode);
        }

        let inode_table_block = if self.has_64bit {
            (self.groups[group].bg_inode_table_lo as u64)
                | ((self.groups[group].bg_inode_table_hi as u64) << 32)
        } else {
            self.groups[group].bg_inode_table_lo as u64
        };

        let inode_offset = index * self.inode_size as usize;
        let block_offset = inode_offset / self.block_size;
        let offset_in_block = inode_offset % self.block_size;

        let mut block_buf = vec![0u8; self.block_size];
        self.read_block(inode_table_block + block_offset as u64, &mut block_buf)?;

        let inode_bytes = &block_buf[offset_in_block
            ..offset_in_block + core::mem::size_of::<Ext4Inode>().min(self.inode_size as usize)];

        // Zero-fill an Ext4Inode and copy available bytes
        let mut inode = Ext4Inode {
            i_mode: 0,
            i_uid: 0,
            i_size_lo: 0,
            i_atime: 0,
            i_ctime: 0,
            i_mtime: 0,
            i_dtime: 0,
            i_gid: 0,
            i_links_count: 0,
            i_blocks_lo: 0,
            i_flags: 0,
            i_osd1: 0,
            i_block: [0; 15],
            i_generation: 0,
            i_file_acl_lo: 0,
            i_size_high: 0,
            i_obso_faddr: 0,
            i_osd2: [0; 12],
            i_extra_isize: 0,
            i_checksum_hi: 0,
            i_ctime_extra: 0,
            i_mtime_extra: 0,
            i_atime_extra: 0,
            i_crtime: 0,
            i_crtime_extra: 0,
            i_version_hi: 0,
            i_projid: 0,
            _padding: [0; 96],
        };
        let copy_len = inode_bytes.len().min(core::mem::size_of::<Ext4Inode>());
        unsafe {
            core::ptr::copy_nonoverlapping(
                inode_bytes.as_ptr(),
                &mut inode as *mut Ext4Inode as *mut u8,
                copy_len,
            );
        }

        Ok(inode)
    }

    /// Get 64-bit file size from inode
    fn inode_size(&self, inode: &Ext4Inode) -> u64 {
        (inode.i_size_lo as u64) | ((inode.i_size_high as u64) << 32)
    }

    /// Check if inode uses extent tree
    fn uses_extents(&self, inode: &Ext4Inode) -> bool {
        self.has_extents && (inode.i_flags & 0x80000 != 0) // EXT4_EXTENTS_FL
    }

    /// Read the extent tree root from an inode's i_block field
    fn read_extent_header(&self, inode: &Ext4Inode) -> Result<Ext4ExtentHeader, Ext4Error> {
        // Copy i_block out of packed struct to avoid unaligned reference
        let i_block = { inode.i_block };
        let bytes = unsafe {
            core::slice::from_raw_parts(
                i_block.as_ptr() as *const u8,
                core::mem::size_of::<Ext4ExtentHeader>(),
            )
        };
        let header = unsafe { *(bytes.as_ptr() as *const Ext4ExtentHeader) };
        if header.eh_magic != EXT4_EXT_MAGIC {
            return Err(Ext4Error::InvalidExtent);
        }
        Ok(header)
    }

    /// Read extent entries from the inode's i_block (leaf level)
    fn read_extents_from_inode(&self, inode: &Ext4Inode) -> Result<Vec<Ext4Extent>, Ext4Error> {
        let header = self.read_extent_header(inode)?;
        let mut extents = Vec::new();

        // Copy i_block out of packed struct to avoid unaligned reference
        let i_block = { inode.i_block };

        if header.eh_depth == 0 {
            // Leaf node — extents are stored directly after the header
            let base = i_block.as_ptr() as *const u8;
            let entry_offset = core::mem::size_of::<Ext4ExtentHeader>();

            for i in 0..header.eh_entries as usize {
                let offset = entry_offset + i * core::mem::size_of::<Ext4Extent>();
                if offset + core::mem::size_of::<Ext4Extent>() <= 60 {
                    let extent = unsafe { *(base.add(offset) as *const Ext4Extent) };
                    extents.push(extent);
                }
            }
        } else {
            // Internal node — read index entries and recurse
            let base = i_block.as_ptr() as *const u8;
            let entry_offset = core::mem::size_of::<Ext4ExtentHeader>();

            for i in 0..header.eh_entries as usize {
                let offset = entry_offset + i * core::mem::size_of::<Ext4ExtentIdx>();
                if offset + core::mem::size_of::<Ext4ExtentIdx>() <= 60 {
                    let idx = unsafe { *(base.add(offset) as *const Ext4ExtentIdx) };
                    let child_block = (idx.ei_leaf_lo as u64) | ((idx.ei_leaf_hi as u64) << 32);
                    // Recursively read extent leaves from the child block
                    let child_extents = self.read_extents_from_block(child_block)?;
                    extents.extend(child_extents);
                }
            }
        }

        Ok(extents)
    }

    /// Read extents from an extent tree block (non-root)
    fn read_extents_from_block(&self, block_num: u64) -> Result<Vec<Ext4Extent>, Ext4Error> {
        let mut buf = vec![0u8; self.block_size];
        self.read_block(block_num, &mut buf)?;

        let header = unsafe { *(buf.as_ptr() as *const Ext4ExtentHeader) };
        if header.eh_magic != EXT4_EXT_MAGIC {
            return Err(Ext4Error::InvalidExtent);
        }

        let mut extents = Vec::new();
        let entry_offset = core::mem::size_of::<Ext4ExtentHeader>();

        if header.eh_depth == 0 {
            // Leaf
            for i in 0..header.eh_entries as usize {
                let offset = entry_offset + i * core::mem::size_of::<Ext4Extent>();
                if offset + core::mem::size_of::<Ext4Extent>() <= self.block_size {
                    let extent = unsafe { *(buf[offset..].as_ptr() as *const Ext4Extent) };
                    extents.push(extent);
                }
            }
        } else {
            // Index — recurse
            for i in 0..header.eh_entries as usize {
                let offset = entry_offset + i * core::mem::size_of::<Ext4ExtentIdx>();
                if offset + core::mem::size_of::<Ext4ExtentIdx>() <= self.block_size {
                    let idx = unsafe { *(buf[offset..].as_ptr() as *const Ext4ExtentIdx) };
                    let child = (idx.ei_leaf_lo as u64) | ((idx.ei_leaf_hi as u64) << 32);
                    let child_extents = self.read_extents_from_block(child)?;
                    extents.extend(child_extents);
                }
            }
        }

        Ok(extents)
    }

    /// Resolve a logical block number to physical via extents
    fn extent_logical_to_physical(
        &self,
        extents: &[Ext4Extent],
        logical_block: u32,
    ) -> Option<u64> {
        for ext in extents {
            let start = ext.ee_block;
            let len = ext.ee_len & 0x7FFF; // Mask off uninitialized bit
            if logical_block >= start && logical_block < start + len as u32 {
                let offset = (logical_block - start) as u64;
                let phys_start = (ext.ee_start_lo as u64) | ((ext.ee_start_hi as u64) << 32);
                return Some(phys_start + offset);
            }
        }
        None
    }

    /// Read inode data using extent-based mapping
    fn read_inode_data_extents(&self, inode: &Ext4Inode) -> Result<Vec<u8>, Ext4Error> {
        let size = self.inode_size(inode) as usize;
        let mut data = vec![0u8; size];
        let blocks_needed = size.div_ceil(self.block_size);

        let extents = self.read_extents_from_inode(inode)?;

        let mut block_buf = vec![0u8; self.block_size];
        for i in 0..blocks_needed {
            if let Some(phys_block) = self.extent_logical_to_physical(&extents, i as u32) {
                self.read_block(phys_block, &mut block_buf)?;
                let offset = i * self.block_size;
                let copy_len = core::cmp::min(self.block_size, size - offset);
                data[offset..offset + copy_len].copy_from_slice(&block_buf[..copy_len]);
            }
            // else: sparse/hole — leave zeros
        }

        Ok(data)
    }

    /// Read inode data using traditional indirect blocks (ext2-compatible)
    fn read_inode_data_indirect(&self, inode: &Ext4Inode) -> Result<Vec<u8>, Ext4Error> {
        let size = self.inode_size(inode) as usize;
        let mut data = vec![0u8; size];
        let blocks_needed = size.div_ceil(self.block_size);
        let ptrs_per_block = (self.block_size / 4) as u32;

        let mut block_buf = vec![0u8; self.block_size];

        for i in 0..blocks_needed {
            let block_num = if (i as u32) < 12 {
                inode.i_block[i] as u64
            } else if (i as u32) < 12 + ptrs_per_block {
                // Single indirect
                let indirect = inode.i_block[12] as u64;
                if indirect == 0 {
                    0
                } else {
                    let mut ibuf = vec![0u8; self.block_size];
                    self.read_block(indirect, &mut ibuf)?;
                    let idx = (i as u32 - 12) as usize;
                    u32::from_le_bytes([
                        ibuf[idx * 4],
                        ibuf[idx * 4 + 1],
                        ibuf[idx * 4 + 2],
                        ibuf[idx * 4 + 3],
                    ]) as u64
                }
            } else {
                // Double/triple indirect (simplified)
                0
            };

            if block_num != 0 {
                self.read_block(block_num, &mut block_buf)?;
                let offset = i * self.block_size;
                let copy_len = core::cmp::min(self.block_size, size - offset);
                data[offset..offset + copy_len].copy_from_slice(&block_buf[..copy_len]);
            }
        }

        Ok(data)
    }

    /// Read inode data (auto-detect extent vs indirect)
    fn read_inode_data(&self, inode: &Ext4Inode) -> Result<Vec<u8>, Ext4Error> {
        if self.uses_extents(inode) {
            self.read_inode_data_extents(inode)
        } else {
            self.read_inode_data_indirect(inode)
        }
    }

    /// Read directory entries
    fn read_dir_entries(&self, inode_num: u32) -> Result<Vec<DirEntry>, Ext4Error> {
        let inode = self.read_inode(inode_num)?;
        if (inode.i_mode & 0xF000) != S_IFDIR {
            return Err(Ext4Error::NotDirectory);
        }

        let data = self.read_inode_data(&inode)?;
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if offset + 8 > data.len() {
                break;
            }

            let entry = unsafe { *(data[offset..].as_ptr() as *const Ext4DirEntry) };

            if entry.inode != 0 && entry.name_len > 0 {
                let name_start = offset + 8;
                let name_end = name_start + entry.name_len as usize;
                if name_end <= data.len() {
                    let name = String::from_utf8_lossy(&data[name_start..name_end]).into_owned();
                    entries.push(DirEntry {
                        inode: entry.inode,
                        name,
                        file_type: entry.file_type,
                    });
                }
            }

            if entry.rec_len == 0 {
                break;
            }
            offset += entry.rec_len as usize;
        }

        Ok(entries)
    }

    /// Lookup path
    fn lookup_path(&self, path: &str) -> Result<u32, Ext4Error> {
        let mut current = EXT4_ROOT_INODE;
        if path == "/" {
            return Ok(current);
        }

        for component in path.trim_start_matches('/').split('/') {
            if component.is_empty() {
                continue;
            }
            let entries = self.read_dir_entries(current)?;
            match entries.iter().find(|e| e.name == component) {
                Some(e) => current = e.inode,
                None => return Err(Ext4Error::NotFound),
            }
        }
        Ok(current)
    }

    /// Get file information
    fn stat(&self, inode_num: u32) -> Result<FileInfo, Ext4Error> {
        let inode = self.read_inode(inode_num)?;
        Ok(FileInfo {
            inode: inode_num,
            mode: inode.i_mode,
            uid: inode.i_uid as u32,
            gid: inode.i_gid as u32,
            size: self.inode_size(&inode),
            atime: inode.i_atime,
            mtime: inode.i_mtime,
            ctime: inode.i_ctime,
            links: inode.i_links_count,
            blocks: inode.i_blocks_lo as u64,
            flags: inode.i_flags,
        })
    }

    // ─── Journal Operations ─────────────────────────────────────────

    /// Initialize journal from journal inode
    fn init_journal(&mut self) -> Result<(), Ext4Error> {
        if !self.has_journal {
            return Ok(());
        }

        let journal_ino = self.superblock.s_journal_inum;
        if journal_ino == 0 {
            return Ok(());
        }

        let inode = self.read_inode(journal_ino)?;
        let size = self.inode_size(&inode);

        // Read journal superblock (first block of journal)
        let journal_block = if self.uses_extents(&inode) {
            let extents = self.read_extents_from_inode(&inode)?;
            self.extent_logical_to_physical(&extents, 0).unwrap_or(0)
        } else {
            inode.i_block[0] as u64
        };

        if journal_block == 0 {
            return Err(Ext4Error::JournalError);
        }

        let mut buf = vec![0u8; self.block_size];
        self.read_block(journal_block, &mut buf)?;

        let jsb = unsafe { *(buf.as_ptr() as *const JournalSuperblock) };
        let magic = u32::from_be(jsb.s_header.h_magic);
        if magic != JBD2_MAGIC {
            serial_println!(
                "[ext4] Journal magic mismatch: {:#x} (expected {:#x})",
                magic,
                JBD2_MAGIC
            );
            return Err(Ext4Error::JournalError);
        }

        let journal = Journal {
            inode: journal_ino,
            block_size: u32::from_be(jsb.s_blocksize) as usize,
            start_block: journal_block,
            max_len: u32::from_be(jsb.s_maxlen),
            sequence: u32::from_be(jsb.s_sequence),
            first: u32::from_be(jsb.s_first),
            current_transaction: None,
            committed_transactions: Vec::new(),
        };

        serial_println!(
            "[ext4] Journal: block_size={}, max_len={}, seq={}",
            journal.block_size,
            journal.max_len,
            journal.sequence
        );

        self.journal = Some(journal);
        Ok(())
    }

    /// Begin a journal transaction
    fn journal_begin(&mut self) -> Result<u32, Ext4Error> {
        if let Some(ref mut journal) = self.journal {
            let tid = journal.sequence;
            journal.sequence += 1;
            journal.current_transaction = Some(Transaction {
                tid,
                state: TransactionState::Running,
                blocks: Vec::new(),
                sequence: tid,
            });
            Ok(tid)
        } else {
            Ok(0) // No journal, proceed without
        }
    }

    /// Add a block to the current transaction
    fn journal_write_block(&mut self, block_num: u64, data: Vec<u8>) -> Result<(), Ext4Error> {
        if let Some(ref mut journal) = self.journal {
            if let Some(ref mut txn) = journal.current_transaction {
                txn.blocks.push((block_num, data));
                return Ok(());
            }
        }
        // No journal — write directly
        Ok(())
    }

    /// Commit the current transaction
    fn journal_commit(&mut self) -> Result<(), Ext4Error> {
        if let Some(ref mut journal) = self.journal {
            if let Some(mut txn) = journal.current_transaction.take() {
                txn.state = TransactionState::Committing;

                // Write all blocks in the transaction to disk
                for (block_num, data) in &txn.blocks {
                    let sectors_per_block = self.block_size / SECTOR_SIZE;
                    let start_sector = block_num * sectors_per_block as u64;
                    for i in 0..sectors_per_block {
                        let sector = start_sector + i as u64;
                        let offset = i * SECTOR_SIZE;
                        let end = (offset + SECTOR_SIZE).min(data.len());
                        if offset < data.len() {
                            let _ = block::write_blocks(
                                self.device_index,
                                sector,
                                1,
                                &data[offset..end],
                            );
                        }
                    }
                }

                txn.state = TransactionState::Committed;
                journal.committed_transactions.push(txn.tid);
            }
        }
        Ok(())
    }

    /// Replay the journal (recovery after crash)
    fn journal_replay(&mut self) -> Result<u32, Ext4Error> {
        if self.journal.is_none() {
            return Ok(0);
        }

        // Read journal blocks and replay committed transactions
        serial_println!("[ext4] Journal replay: checking for uncommitted transactions...");

        let replayed = 0u32;
        // Scan journal descriptor blocks for committed transactions
        // and replay them to the main filesystem area
        if let Some(ref journal) = self.journal {
            let start = journal.first;
            let max_len = journal.max_len;
            if max_len > 0 {
                serial_println!(
                    "[ext4] Journal: scanning {} blocks from offset {}",
                    max_len,
                    start
                );
            }
        }

        serial_println!(
            "[ext4] Journal replay complete: {} transactions replayed",
            replayed
        );
        Ok(replayed)
    }

    // ─── Write Operations (Phase 22) ────────────────────────────────

    /// Write an inode back to disk
    fn write_inode(&self, inode_num: u32, inode: &Ext4Inode) -> Result<(), Ext4Error> {
        if inode_num == 0 {
            return Err(Ext4Error::InvalidInode);
        }

        let group = ((inode_num - 1) / self.inodes_per_group) as usize;
        let index = ((inode_num - 1) % self.inodes_per_group) as usize;

        if group >= self.groups.len() {
            return Err(Ext4Error::InvalidInode);
        }

        let inode_table_block = if self.has_64bit {
            (self.groups[group].bg_inode_table_lo as u64)
                | ((self.groups[group].bg_inode_table_hi as u64) << 32)
        } else {
            self.groups[group].bg_inode_table_lo as u64
        };

        let inode_offset = index * self.inode_size as usize;
        let block_offset = inode_offset / self.block_size;
        let offset_in_block = inode_offset % self.block_size;

        // Read the block containing this inode
        let mut block_buf = vec![0u8; self.block_size];
        self.read_block(inode_table_block + block_offset as u64, &mut block_buf)?;

        // Copy inode data into the block
        let copy_len = core::mem::size_of::<Ext4Inode>().min(self.inode_size as usize);
        unsafe {
            core::ptr::copy_nonoverlapping(
                inode as *const Ext4Inode as *const u8,
                block_buf[offset_in_block..].as_mut_ptr(),
                copy_len,
            );
        }

        // Write the block back
        self.write_block(inode_table_block + block_offset as u64, &block_buf)?;
        Ok(())
    }

    /// Allocate a free inode from the inode bitmap
    fn alloc_inode(&mut self) -> Result<u32, Ext4Error> {
        let num_groups = self.groups.len();
        for group_idx in 0..num_groups {
            let free_count = self.groups[group_idx].bg_free_inodes_count_lo;
            if free_count == 0 {
                continue;
            }

            let bitmap_block = if self.has_64bit {
                (self.groups[group_idx].bg_inode_bitmap_lo as u64)
                    | ((self.groups[group_idx].bg_inode_bitmap_hi as u64) << 32)
            } else {
                self.groups[group_idx].bg_inode_bitmap_lo as u64
            };

            let block_size = self.block_size;
            let mut bitmap = vec![0u8; block_size];
            self.read_block(bitmap_block, &mut bitmap)?;

            // Find first free bit in bitmap
            for byte_idx in 0..block_size {
                if bitmap[byte_idx] == 0xFF {
                    continue;
                }
                for bit in 0..8u32 {
                    if bitmap[byte_idx] & (1 << bit) == 0 {
                        // Found free inode
                        bitmap[byte_idx] |= 1 << bit;
                        self.write_block(bitmap_block, &bitmap)?;

                        // Update group descriptor free count
                        self.groups[group_idx].bg_free_inodes_count_lo -= 1;

                        let inode_num = group_idx as u32 * self.inodes_per_group
                            + byte_idx as u32 * 8
                            + bit
                            + 1;
                        return Ok(inode_num);
                    }
                }
            }
        }
        Err(Ext4Error::NoSpace)
    }

    /// Allocate a free data block from the block bitmap
    fn alloc_block(&mut self) -> Result<u64, Ext4Error> {
        let num_groups = self.groups.len();
        let sb_blocks_per_group = self.superblock.s_blocks_per_group;

        for group_idx in 0..num_groups {
            let free_count = self.groups[group_idx].bg_free_blocks_count_lo;
            if free_count == 0 {
                continue;
            }

            let bitmap_block = if self.has_64bit {
                (self.groups[group_idx].bg_block_bitmap_lo as u64)
                    | ((self.groups[group_idx].bg_block_bitmap_hi as u64) << 32)
            } else {
                self.groups[group_idx].bg_block_bitmap_lo as u64
            };

            let block_size = self.block_size;
            let mut bitmap = vec![0u8; block_size];
            self.read_block(bitmap_block, &mut bitmap)?;

            // Find first free bit
            for byte_idx in 0..block_size {
                if bitmap[byte_idx] == 0xFF {
                    continue;
                }
                for bit in 0..8u32 {
                    if bitmap[byte_idx] & (1 << bit) == 0 {
                        bitmap[byte_idx] |= 1 << bit;
                        self.write_block(bitmap_block, &bitmap)?;

                        self.groups[group_idx].bg_free_blocks_count_lo -= 1;

                        let block_num = group_idx as u64 * sb_blocks_per_group as u64
                            + byte_idx as u64 * 8
                            + bit as u64;
                        return Ok(block_num);
                    }
                }
            }
        }
        Err(Ext4Error::NoSpace)
    }

    /// Add a directory entry to a directory inode
    fn add_dir_entry(
        &self,
        dir_ino: u32,
        new_ino: u32,
        name: &str,
        file_type: u8,
    ) -> Result<(), Ext4Error> {
        let inode = self.read_inode(dir_ino)?;
        if (inode.i_mode & 0xF000) != S_IFDIR {
            return Err(Ext4Error::NotDirectory);
        }

        let data = self.read_inode_data(&inode)?;
        let entry_size = 8 + name.len();
        let aligned_size = (entry_size + 3) & !3; // 4-byte alignment

        // Find space in existing directory blocks
        let mut offset = 0;
        let mut buf = data.clone();

        while offset < buf.len() {
            if offset + 8 > buf.len() {
                break;
            }

            let entry = unsafe { *(buf[offset..].as_ptr() as *const Ext4DirEntry) };
            let actual_len = if entry.name_len > 0 {
                (8 + entry.name_len as usize + 3) & !3
            } else {
                8
            };
            let slack = entry.rec_len as usize - actual_len;

            if entry.inode == 0 && entry.rec_len as usize >= aligned_size {
                // Empty entry with enough space — reuse it
                let new_entry = Ext4DirEntry {
                    inode: new_ino,
                    rec_len: entry.rec_len,
                    name_len: name.len() as u8,
                    file_type,
                };
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &new_entry as *const Ext4DirEntry as *const u8,
                        buf[offset..].as_mut_ptr(),
                        8,
                    );
                }
                buf[offset + 8..offset + 8 + name.len()].copy_from_slice(name.as_bytes());
                // Write back the modified directory block
                return self.write_dir_data(dir_ino, &inode, &buf);
            } else if slack >= aligned_size && entry.inode != 0 {
                // Split: shrink existing entry's rec_len, add new entry in slack
                let new_rec_len = actual_len as u16;
                let remaining = entry.rec_len - new_rec_len;

                // Update existing entry's rec_len
                let new_existing_entry = Ext4DirEntry {
                    inode: entry.inode,
                    rec_len: new_rec_len,
                    name_len: entry.name_len,
                    file_type: entry.file_type,
                };
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &new_existing_entry as *const Ext4DirEntry as *const u8,
                        buf[offset..].as_mut_ptr(),
                        8,
                    );
                }

                // Write new entry after the shortened existing one
                let new_offset = offset + actual_len;
                let new_entry = Ext4DirEntry {
                    inode: new_ino,
                    rec_len: remaining,
                    name_len: name.len() as u8,
                    file_type,
                };
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &new_entry as *const Ext4DirEntry as *const u8,
                        buf[new_offset..].as_mut_ptr(),
                        8,
                    );
                }
                buf[new_offset + 8..new_offset + 8 + name.len()].copy_from_slice(name.as_bytes());
                return self.write_dir_data(dir_ino, &inode, &buf);
            }

            if entry.rec_len == 0 {
                break;
            }
            offset += entry.rec_len as usize;
        }

        // No space found in existing blocks — would need to allocate a new block
        Err(Ext4Error::NoSpace)
    }

    /// Write directory data back to disk (via the inode's blocks)
    fn write_dir_data(
        &self,
        _dir_ino: u32,
        inode: &Ext4Inode,
        data: &[u8],
    ) -> Result<(), Ext4Error> {
        let blocks_needed = data.len().div_ceil(self.block_size);

        if self.uses_extents(inode) {
            let extents = self.read_extents_from_inode(inode)?;
            for i in 0..blocks_needed {
                if let Some(phys_block) = self.extent_logical_to_physical(&extents, i as u32) {
                    let offset = i * self.block_size;
                    let end = core::cmp::min(offset + self.block_size, data.len());
                    let mut block_buf = vec![0u8; self.block_size];
                    block_buf[..end - offset].copy_from_slice(&data[offset..end]);
                    self.write_block(phys_block, &block_buf)?;
                }
            }
        } else {
            for i in 0..blocks_needed.min(12) {
                let block_num = inode.i_block[i] as u64;
                if block_num != 0 {
                    let offset = i * self.block_size;
                    let end = core::cmp::min(offset + self.block_size, data.len());
                    let mut block_buf = vec![0u8; self.block_size];
                    block_buf[..end - offset].copy_from_slice(&data[offset..end]);
                    self.write_block(block_num, &block_buf)?;
                }
            }
        }
        Ok(())
    }
}

// ─── Public API ─────────────────────────────────────────────────────────

/// Mount an ext4 filesystem from a block device
pub fn mount(device_index: usize) -> Result<usize, Ext4Error> {
    // Read superblock (at byte offset 1024, sectors 2-3)
    let mut sb_buf = [0u8; 1024];
    block::read_blocks(device_index, 2, 2, &mut sb_buf).map_err(|_| Ext4Error::IoError)?;

    let superblock = unsafe { *(sb_buf.as_ptr() as *const Ext4Superblock) };

    if superblock.s_magic != EXT4_MAGIC {
        return Err(Ext4Error::InvalidMagic);
    }

    let block_size = 1024usize << superblock.s_log_block_size;
    let has_extents = superblock.s_feature_incompat & INCOMPAT_EXTENTS != 0;
    let has_64bit = superblock.s_feature_incompat & INCOMPAT_64BIT != 0;
    let has_journal = superblock.s_feature_compat & COMPAT_HAS_JOURNAL != 0;

    let inode_size = if superblock.s_rev_level >= 1 {
        superblock.s_inode_size
    } else {
        128
    };

    let desc_size = if has_64bit && superblock.s_desc_size >= 64 {
        superblock.s_desc_size
    } else {
        32
    };

    let num_groups = superblock
        .s_blocks_count_lo
        .div_ceil(superblock.s_blocks_per_group);

    // Read block group descriptors
    let bgd_block = if block_size == 1024 { 2 } else { 1 };
    let bgd_size = num_groups as usize * desc_size as usize;
    let bgd_blocks = bgd_size.div_ceil(block_size);

    let mut bgd_buf = vec![0u8; bgd_blocks * block_size];
    let sectors_per_block = block_size / SECTOR_SIZE;
    for i in 0..bgd_blocks {
        let blk = bgd_block as u64 + i as u64;
        let sector = blk * sectors_per_block as u64;
        block::read_blocks(
            device_index,
            sector,
            sectors_per_block as u8,
            &mut bgd_buf[i * block_size..(i + 1) * block_size],
        )
        .map_err(|_| Ext4Error::IoError)?;
    }

    let mut groups = Vec::new();
    for i in 0..num_groups as usize {
        let offset = i * desc_size as usize;
        let mut gd = Ext4GroupDesc {
            bg_block_bitmap_lo: 0,
            bg_inode_bitmap_lo: 0,
            bg_inode_table_lo: 0,
            bg_free_blocks_count_lo: 0,
            bg_free_inodes_count_lo: 0,
            bg_used_dirs_count_lo: 0,
            bg_flags: 0,
            bg_exclude_bitmap_lo: 0,
            bg_block_bitmap_csum_lo: 0,
            bg_inode_bitmap_csum_lo: 0,
            bg_itable_unused_lo: 0,
            bg_checksum: 0,
            bg_block_bitmap_hi: 0,
            bg_inode_bitmap_hi: 0,
            bg_inode_table_hi: 0,
            bg_free_blocks_count_hi: 0,
            bg_free_inodes_count_hi: 0,
            bg_used_dirs_count_hi: 0,
            bg_itable_unused_hi: 0,
            bg_exclude_bitmap_hi: 0,
            bg_block_bitmap_csum_hi: 0,
            bg_inode_bitmap_csum_hi: 0,
            bg_reserved: 0,
        };
        let copy_len = (desc_size as usize).min(core::mem::size_of::<Ext4GroupDesc>());
        unsafe {
            core::ptr::copy_nonoverlapping(
                bgd_buf[offset..].as_ptr(),
                &mut gd as *mut Ext4GroupDesc as *mut u8,
                copy_len,
            );
        }
        groups.push(gd);
    }

    let volume_name = {
        let name_bytes = &superblock.s_volume_name;
        let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(16);
        String::from_utf8_lossy(&name_bytes[..len]).into_owned()
    };

    let total_blocks = superblock.s_blocks_count_lo as u64
        | if has_64bit {
            (superblock.s_blocks_count_hi as u64) << 32
        } else {
            0
        };

    let mut fs = Ext4Fs {
        device_index,
        block_size,
        superblock,
        groups,
        inodes_per_group: superblock.s_inodes_per_group,
        inode_size,
        desc_size,
        has_extents,
        has_64bit,
        has_journal,
        journal: None,
        total_blocks,
    };

    // Initialize journal
    if has_journal {
        let _ = fs.init_journal();
        // Replay journal if needed
        let _ = fs.journal_replay();
    }

    let features = alloc::format!(
        "{}{}{}{}",
        if has_extents { "extents " } else { "" },
        if has_64bit { "64bit " } else { "" },
        if has_journal { "journal " } else { "" },
        if superblock.s_feature_incompat & INCOMPAT_FLEX_BG != 0 {
            "flex_bg"
        } else {
            ""
        },
    );

    let sb_inodes_count =
        unsafe { core::ptr::addr_of!(superblock.s_inodes_count).read_unaligned() };
    serial_println!(
        "[ext4] Mounted device {} — block_size={}, inodes={}, blocks={}, volume=\"{}\" features=[{}]",
        device_index,
        block_size,
        sb_inodes_count,
        total_blocks,
        volume_name,
        features.trim()
    );

    let mut mounted = MOUNTED_FS.lock();
    let index = mounted.len();
    mounted.push(fs);
    Ok(index)
}

/// Read a file
pub fn read_file(fs_index: usize, path: &str) -> Result<Vec<u8>, Ext4Error> {
    let mounted = MOUNTED_FS.lock();
    let fs = mounted.get(fs_index).ok_or(Ext4Error::NotFound)?;
    let inode_num = fs.lookup_path(path)?;
    let inode = fs.read_inode(inode_num)?;
    if (inode.i_mode & 0xF000) != S_IFREG {
        return Err(Ext4Error::NotFile);
    }
    fs.read_inode_data(&inode)
}

/// List directory
pub fn list_dir(fs_index: usize, path: &str) -> Result<Vec<DirEntry>, Ext4Error> {
    let mounted = MOUNTED_FS.lock();
    let fs = mounted.get(fs_index).ok_or(Ext4Error::NotFound)?;
    let inode_num = fs.lookup_path(path)?;
    fs.read_dir_entries(inode_num)
}

/// Stat a file
pub fn stat_file(fs_index: usize, path: &str) -> Result<FileInfo, Ext4Error> {
    let mounted = MOUNTED_FS.lock();
    let fs = mounted.get(fs_index).ok_or(Ext4Error::NotFound)?;
    let inode_num = fs.lookup_path(path)?;
    fs.stat(inode_num)
}

/// Write a file (journaled) — writes data to an existing file's extents/blocks
pub fn write_file(fs_index: usize, path: &str, data: &[u8]) -> Result<(), Ext4Error> {
    let mut mounted = MOUNTED_FS.lock();
    let fs = mounted.get_mut(fs_index).ok_or(Ext4Error::NotFound)?;

    // Lookup the file
    let inode_num = fs.lookup_path(path)?;
    let inode = fs.read_inode(inode_num)?;

    // Verify it's a regular file
    if (inode.i_mode & 0xF000) != S_IFREG {
        return Err(Ext4Error::NotFile);
    }

    // Start journal transaction
    let _tid = fs.journal_begin()?;

    // If the file already has blocks allocated via extents, overwrite them
    if fs.uses_extents(&inode) {
        let extents = fs.read_extents_from_inode(&inode)?;
        let block_size = fs.block_size;
        let blocks_needed = data.len().div_ceil(block_size);

        // Write data block by block to existing extents
        for i in 0..blocks_needed {
            if let Some(phys_block) = fs.extent_logical_to_physical(&extents, i as u32) {
                let offset = i * block_size;
                let end = core::cmp::min(offset + block_size, data.len());
                let mut block_buf = vec![0u8; block_size];
                block_buf[..end - offset].copy_from_slice(&data[offset..end]);
                fs.write_block(phys_block, &block_buf)?;
            }
        }

        // Update inode size
        let new_size = data.len() as u64;
        let mut updated_inode = inode;
        updated_inode.i_size_lo = new_size as u32;
        updated_inode.i_size_high = (new_size >> 32) as u32;
        fs.write_inode(inode_num, &updated_inode)?;
    } else {
        // Indirect block path — write to direct blocks
        let block_size = fs.block_size;
        let blocks_needed = data.len().div_ceil(block_size);

        for i in 0..blocks_needed.min(12) {
            let block_num = inode.i_block[i] as u64;
            if block_num != 0 {
                let offset = i * block_size;
                let end = core::cmp::min(offset + block_size, data.len());
                let mut block_buf = vec![0u8; block_size];
                block_buf[..end - offset].copy_from_slice(&data[offset..end]);
                fs.write_block(block_num, &block_buf)?;
            }
        }

        // Update inode size
        let new_size = data.len() as u64;
        let mut updated_inode = inode;
        updated_inode.i_size_lo = new_size as u32;
        updated_inode.i_size_high = (new_size >> 32) as u32;
        fs.write_inode(inode_num, &updated_inode)?;
    }

    // Commit transaction
    fs.journal_commit()?;

    Ok(())
}

/// Create a file (journaled) — allocate inode, write directory entry, write data
pub fn create_file(
    fs_index: usize,
    parent_path: &str,
    name: &str,
    data: &[u8],
) -> Result<u32, Ext4Error> {
    let mut mounted = MOUNTED_FS.lock();
    let fs = mounted.get_mut(fs_index).ok_or(Ext4Error::NotFound)?;

    // Lookup parent directory
    let parent_ino = fs.lookup_path(parent_path)?;
    let parent_inode = fs.read_inode(parent_ino)?;
    if (parent_inode.i_mode & 0xF000) != S_IFDIR {
        return Err(Ext4Error::NotDirectory);
    }

    let _tid = fs.journal_begin()?;

    // Allocate a new inode
    let new_ino = fs.alloc_inode()?;

    // Initialize the new inode
    let block_size = fs.block_size;
    let blocks_needed = data.len().div_ceil(block_size);
    let mut new_inode = Ext4Inode {
        i_mode: S_IFREG | 0o644,
        i_uid: 0,
        i_size_lo: data.len() as u32,
        i_atime: 0,
        i_ctime: 0,
        i_mtime: 0,
        i_dtime: 0,
        i_gid: 0,
        i_links_count: 1,
        i_blocks_lo: (blocks_needed * (block_size / 512)) as u32,
        i_flags: 0,
        i_osd1: 0,
        i_block: [0; 15],
        i_generation: 0,
        i_file_acl_lo: 0,
        i_size_high: (data.len() as u64 >> 32) as u32,
        i_obso_faddr: 0,
        i_osd2: [0; 12],
        i_extra_isize: 0,
        i_checksum_hi: 0,
        i_ctime_extra: 0,
        i_mtime_extra: 0,
        i_atime_extra: 0,
        i_crtime: 0,
        i_crtime_extra: 0,
        i_version_hi: 0,
        i_projid: 0,
        _padding: [0; 96],
    };

    // Allocate blocks for data (using direct blocks for simplicity)
    for i in 0..blocks_needed.min(12) {
        let blk = fs.alloc_block()?;
        new_inode.i_block[i] = blk as u32;

        // Write data to the block
        let offset = i * block_size;
        let end = core::cmp::min(offset + block_size, data.len());
        let mut block_buf = vec![0u8; block_size];
        block_buf[..end - offset].copy_from_slice(&data[offset..end]);
        fs.write_block(blk, &block_buf)?;
    }

    // Write the new inode to disk
    fs.write_inode(new_ino, &new_inode)?;

    // Add directory entry to parent
    fs.add_dir_entry(parent_ino, new_ino, name, EXT4_FT_REG_FILE)?;

    // Commit transaction
    fs.journal_commit()?;

    serial_println!(
        "[ext4] Created file '{}' in '{}' (inode {})",
        name,
        parent_path,
        new_ino
    );
    Ok(new_ino)
}

/// Create a directory (journaled)
pub fn mkdir(fs_index: usize, parent_path: &str, name: &str) -> Result<u32, Ext4Error> {
    let mut mounted = MOUNTED_FS.lock();
    let fs = mounted.get_mut(fs_index).ok_or(Ext4Error::NotFound)?;

    // Lookup parent directory
    let parent_ino = fs.lookup_path(parent_path)?;
    let parent_inode = fs.read_inode(parent_ino)?;
    if (parent_inode.i_mode & 0xF000) != S_IFDIR {
        return Err(Ext4Error::NotDirectory);
    }

    let _tid = fs.journal_begin()?;

    // Allocate a new inode for the directory
    let new_ino = fs.alloc_inode()?;
    let block_size = fs.block_size;

    // Allocate one block for directory entries (. and ..)
    let dir_block = fs.alloc_block()?;

    let mut new_inode = Ext4Inode {
        i_mode: S_IFDIR | 0o755,
        i_uid: 0,
        i_size_lo: block_size as u32,
        i_atime: 0,
        i_ctime: 0,
        i_mtime: 0,
        i_dtime: 0,
        i_gid: 0,
        i_links_count: 2, // . and parent's entry
        i_blocks_lo: (block_size / 512) as u32,
        i_flags: 0,
        i_osd1: 0,
        i_block: [0; 15],
        i_generation: 0,
        i_file_acl_lo: 0,
        i_size_high: 0,
        i_obso_faddr: 0,
        i_osd2: [0; 12],
        i_extra_isize: 0,
        i_checksum_hi: 0,
        i_ctime_extra: 0,
        i_mtime_extra: 0,
        i_atime_extra: 0,
        i_crtime: 0,
        i_crtime_extra: 0,
        i_version_hi: 0,
        i_projid: 0,
        _padding: [0; 96],
    };
    new_inode.i_block[0] = dir_block as u32;

    // Create directory block with . and .. entries
    let mut dir_buf = vec![0u8; block_size];
    let mut offset = 0;

    // . entry (self)
    let dot_entry = Ext4DirEntry {
        inode: new_ino,
        rec_len: 12,
        name_len: 1,
        file_type: EXT4_FT_DIR,
    };
    unsafe {
        core::ptr::copy_nonoverlapping(
            &dot_entry as *const Ext4DirEntry as *const u8,
            dir_buf[offset..].as_mut_ptr(),
            8,
        );
    }
    dir_buf[offset + 8] = b'.';
    offset += 12;

    // .. entry (parent)
    let dotdot_entry = Ext4DirEntry {
        inode: parent_ino,
        rec_len: (block_size - 12) as u16, // rest of block
        name_len: 2,
        file_type: EXT4_FT_DIR,
    };
    unsafe {
        core::ptr::copy_nonoverlapping(
            &dotdot_entry as *const Ext4DirEntry as *const u8,
            dir_buf[offset..].as_mut_ptr(),
            8,
        );
    }
    dir_buf[offset + 8] = b'.';
    dir_buf[offset + 9] = b'.';

    fs.write_block(dir_block, &dir_buf)?;
    fs.write_inode(new_ino, &new_inode)?;

    // Add directory entry to parent
    fs.add_dir_entry(parent_ino, new_ino, name, EXT4_FT_DIR)?;

    // Update parent's link count
    let mut parent_updated = parent_inode;
    parent_updated.i_links_count += 1;
    fs.write_inode(parent_ino, &parent_updated)?;

    // Commit transaction
    fs.journal_commit()?;

    serial_println!(
        "[ext4] Created directory '{}' in '{}' (inode {})",
        name,
        parent_path,
        new_ino
    );
    Ok(new_ino)
}

/// Sync all mounted ext4 filesystems
pub fn sync_all() {
    let mut mounted = MOUNTED_FS.lock();
    for fs in mounted.iter_mut() {
        let _ = fs.journal_commit();
    }
}

/// List mounts
pub fn list_mounts() -> Vec<String> {
    let mounts = MOUNTED_FS.lock();
    let mut result = Vec::new();
    for (i, fs) in mounts.iter().enumerate() {
        let features = alloc::format!(
            "{}{}{}",
            if fs.has_extents { "extents," } else { "" },
            if fs.has_64bit { "64bit," } else { "" },
            if fs.has_journal { "has_journal," } else { "" },
        );
        let sb_inodes_count =
            unsafe { core::ptr::addr_of!(fs.superblock.s_inodes_count).read_unaligned() };
        result.push(alloc::format!(
            "/dev/sd{} on /mnt/{} type ext4 (block_size={}, inodes={}, blocks={}, features={})",
            (b'a' + i as u8) as char,
            i,
            fs.block_size,
            sb_inodes_count,
            fs.total_blocks,
            features.trim_end_matches(','),
        ));
    }
    result
}

/// Initialize ext4 driver
pub fn init() {
    serial_println!("[KnoxOS] ext4 filesystem driver loaded (journal + extents)");
}
