use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ─── Ext4 Constants ─────────────────────────────────────────────────────

/// Ext2/3/4 magic number
pub(crate) const EXT4_MAGIC: u16 = 0xEF53;

/// Feature flags (incompatible) that indicate ext4
pub(crate) const INCOMPAT_FILETYPE: u32 = 0x0002;
pub(crate) const INCOMPAT_EXTENTS: u32 = 0x0040;
pub(crate) const INCOMPAT_64BIT: u32 = 0x0080;
pub(crate) const INCOMPAT_FLEX_BG: u32 = 0x0200;

/// Feature flags (compatible) for ext4
pub(crate) const COMPAT_DIR_INDEX: u32 = 0x0020;
pub(crate) const COMPAT_HAS_JOURNAL: u32 = 0x0004;

/// Feature flags (read-only compatible)
pub(crate) const RO_COMPAT_HUGE_FILE: u32 = 0x0008;
pub(crate) const RO_COMPAT_GDT_CSUM: u32 = 0x0010;
pub(crate) const RO_COMPAT_DIR_NLINK: u32 = 0x0020;
pub(crate) const RO_COMPAT_EXTRA_ISIZE: u32 = 0x0040;

/// Extent magic
pub(crate) const EXT4_EXT_MAGIC: u16 = 0xF30A;

/// Journal magic
pub(crate) const JBD2_MAGIC: u32 = 0xC03B3998;

/// Maximum extents per header
pub(crate) const EXT4_EXT_MAX_ENTRIES: u16 = 4;

/// Root inode
pub(crate) const EXT4_ROOT_INODE: u32 = 2;
/// Journal inode
pub(crate) const EXT4_JOURNAL_INODE: u32 = 8;

/// File type constants
pub(crate) const EXT4_FT_REG_FILE: u8 = 1;
pub(crate) const EXT4_FT_DIR: u8 = 2;
pub(crate) const EXT4_FT_SYMLINK: u8 = 7;

/// Inode mode flags
pub(crate) const S_IFREG: u16 = 0x8000;
pub(crate) const S_IFDIR: u16 = 0x4000;
pub(crate) const S_IFLNK: u16 = 0xA000;

// ─── On-disk Structures ─────────────────────────────────────────────────

/// Ext4 superblock (1024 bytes, at offset 1024 from start of partition)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(crate) struct Ext4Superblock {
    pub(crate) s_inodes_count: u32,
    pub(crate) s_blocks_count_lo: u32,
    pub(crate) s_r_blocks_count_lo: u32,
    pub(crate) s_free_blocks_count_lo: u32,
    pub(crate) s_free_inodes_count: u32,
    pub(crate) s_first_data_block: u32,
    pub(crate) s_log_block_size: u32,
    pub(crate) s_log_cluster_size: u32,
    pub(crate) s_blocks_per_group: u32,
    pub(crate) s_clusters_per_group: u32,
    pub(crate) s_inodes_per_group: u32,
    pub(crate) s_mtime: u32,
    pub(crate) s_wtime: u32,
    pub(crate) s_mnt_count: u16,
    pub(crate) s_max_mnt_count: u16,
    pub(crate) s_magic: u16,
    pub(crate) s_state: u16,
    pub(crate) s_errors: u16,
    pub(crate) s_minor_rev_level: u16,
    pub(crate) s_lastcheck: u32,
    pub(crate) s_checkinterval: u32,
    pub(crate) s_creator_os: u32,
    pub(crate) s_rev_level: u32,
    pub(crate) s_def_resuid: u16,
    pub(crate) s_def_resgid: u16,
    // Extended fields (rev >= 1)
    pub(crate) s_first_ino: u32,
    pub(crate) s_inode_size: u16,
    pub(crate) s_block_group_nr: u16,
    pub(crate) s_feature_compat: u32,
    pub(crate) s_feature_incompat: u32,
    pub(crate) s_feature_ro_compat: u32,
    pub(crate) s_uuid: [u8; 16],
    pub(crate) s_volume_name: [u8; 16],
    pub(crate) s_last_mounted: [u8; 64],
    pub(crate) s_algorithm_usage_bitmap: u32,
    pub(crate) s_prealloc_blocks: u8,
    pub(crate) s_prealloc_dir_blocks: u8,
    pub(crate) s_reserved_gdt_blocks: u16,
    // Journal fields
    pub(crate) s_journal_uuid: [u8; 16],
    pub(crate) s_journal_inum: u32,
    pub(crate) s_journal_dev: u32,
    pub(crate) s_last_orphan: u32,
    pub(crate) s_hash_seed: [u32; 4],
    pub(crate) s_def_hash_version: u8,
    pub(crate) s_jnl_backup_type: u8,
    pub(crate) s_desc_size: u16,
    pub(crate) s_default_mount_opts: u32,
    pub(crate) s_first_meta_bg: u32,
    pub(crate) s_mkfs_time: u32,
    pub(crate) s_jnl_blocks: [u32; 17],
    // 64-bit support
    pub(crate) s_blocks_count_hi: u32,
    pub(crate) s_r_blocks_count_hi: u32,
    pub(crate) s_free_blocks_count_hi: u32,
    pub(crate) s_min_extra_isize: u16,
    pub(crate) s_want_extra_isize: u16,
    pub(crate) s_flags: u32,
    pub(crate) s_raid_stride: u16,
    pub(crate) s_mmp_interval: u16,
    pub(crate) s_mmp_block: u64,
    pub(crate) s_raid_stripe_width: u32,
    pub(crate) s_log_groups_per_flex: u8,
    pub(crate) s_checksum_type: u8,
    pub(crate) _padding: [u8; 2],
    pub(crate) s_kbytes_written: u64,
    // ... remaining fields to pad to 1024 bytes
    pub(crate) _reserved: [u8; 596],
}

/// Block group descriptor (32 or 64 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(crate) struct Ext4GroupDesc {
    pub(crate) bg_block_bitmap_lo: u32,
    pub(crate) bg_inode_bitmap_lo: u32,
    pub(crate) bg_inode_table_lo: u32,
    pub(crate) bg_free_blocks_count_lo: u16,
    pub(crate) bg_free_inodes_count_lo: u16,
    pub(crate) bg_used_dirs_count_lo: u16,
    pub(crate) bg_flags: u16,
    pub(crate) bg_exclude_bitmap_lo: u32,
    pub(crate) bg_block_bitmap_csum_lo: u16,
    pub(crate) bg_inode_bitmap_csum_lo: u16,
    pub(crate) bg_itable_unused_lo: u16,
    pub(crate) bg_checksum: u16,
    // 64-bit extensions (if s_desc_size >= 64)
    pub(crate) bg_block_bitmap_hi: u32,
    pub(crate) bg_inode_bitmap_hi: u32,
    pub(crate) bg_inode_table_hi: u32,
    pub(crate) bg_free_blocks_count_hi: u16,
    pub(crate) bg_free_inodes_count_hi: u16,
    pub(crate) bg_used_dirs_count_hi: u16,
    pub(crate) bg_itable_unused_hi: u16,
    pub(crate) bg_exclude_bitmap_hi: u32,
    pub(crate) bg_block_bitmap_csum_hi: u16,
    pub(crate) bg_inode_bitmap_csum_hi: u16,
    pub(crate) bg_reserved: u32,
}

/// Ext4 inode (256 bytes for ext4, minimum 128)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(crate) struct Ext4Inode {
    pub(crate) i_mode: u16,
    pub(crate) i_uid: u16,
    pub(crate) i_size_lo: u32,
    pub(crate) i_atime: u32,
    pub(crate) i_ctime: u32,
    pub(crate) i_mtime: u32,
    pub(crate) i_dtime: u32,
    pub(crate) i_gid: u16,
    pub(crate) i_links_count: u16,
    pub(crate) i_blocks_lo: u32,
    pub(crate) i_flags: u32,
    pub(crate) i_osd1: u32,
    pub(crate) i_block: [u32; 15], // 60 bytes: direct/indirect blocks OR extent tree root
    pub(crate) i_generation: u32,
    pub(crate) i_file_acl_lo: u32,
    pub(crate) i_size_high: u32, // Upper 32 bits of size (ext4 large files)
    pub(crate) i_obso_faddr: u32,
    pub(crate) i_osd2: [u8; 12],
    pub(crate) i_extra_isize: u16,
    pub(crate) i_checksum_hi: u16,
    pub(crate) i_ctime_extra: u32,
    pub(crate) i_mtime_extra: u32,
    pub(crate) i_atime_extra: u32,
    pub(crate) i_crtime: u32,
    pub(crate) i_crtime_extra: u32,
    pub(crate) i_version_hi: u32,
    pub(crate) i_projid: u32,
    pub(crate) _padding: [u8; 96],
}

/// Ext4 extent header (12 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ext4ExtentHeader {
    pub(crate) eh_magic: u16,      // EXT4_EXT_MAGIC = 0xF30A
    pub(crate) eh_entries: u16,    // Number of valid entries
    pub(crate) eh_max: u16,        // Max entries that can be stored
    pub(crate) eh_depth: u16,      // Depth of tree (0 = leaf)
    pub(crate) eh_generation: u32, // Generation of the tree
}

/// Ext4 extent (leaf node, 12 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ext4Extent {
    pub(crate) ee_block: u32,    // First logical block
    pub(crate) ee_len: u16,      // Number of blocks (<=32768; bit 15 = uninitialized)
    pub(crate) ee_start_hi: u16, // Upper 16 bits of physical block
    pub(crate) ee_start_lo: u32, // Lower 32 bits of physical block
}

/// Ext4 extent index (internal node, 12 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ext4ExtentIdx {
    pub(crate) ei_block: u32,   // Logical block covered by this index
    pub(crate) ei_leaf_lo: u32, // Lower 32 bits of child block
    pub(crate) ei_leaf_hi: u16, // Upper 16 bits of child block
    pub(crate) ei_unused: u16,
}

/// Directory entry (ext4 with file type)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(crate) struct Ext4DirEntry {
    pub(crate) inode: u32,
    pub(crate) rec_len: u16,
    pub(crate) name_len: u8,
    pub(crate) file_type: u8,
    // name bytes follow (variable length)
}

// ─── Journal (JBD2) Structures ──────────────────────────────────────────

/// Journal superblock
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(crate) struct JournalSuperblock {
    pub(crate) s_header: JournalHeader,
    pub(crate) s_blocksize: u32,
    pub(crate) s_maxlen: u32,
    pub(crate) s_first: u32,
    pub(crate) s_sequence: u32,
    pub(crate) s_start: u32,
    pub(crate) s_errno: u32,
    // V2 fields
    pub(crate) s_feature_compat: u32,
    pub(crate) s_feature_incompat: u32,
    pub(crate) s_feature_ro_compat: u32,
    pub(crate) s_uuid: [u8; 16],
    pub(crate) s_nr_users: u32,
    pub(crate) s_dynsuper: u32,
    pub(crate) s_max_transaction: u32,
    pub(crate) s_max_trans_data: u32,
    pub(crate) s_checksum_type: u8,
    pub(crate) _padding1: [u8; 3],
    pub(crate) _padding2: [u32; 42],
    pub(crate) s_checksum: u32,
    pub(crate) s_users: [[u8; 16]; 48],
}

/// Journal block header
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(crate) struct JournalHeader {
    pub(crate) h_magic: u32,
    pub(crate) h_blocktype: u32,
    pub(crate) h_sequence: u32,
}

/// Journal block types
pub(crate) const JBD2_DESCRIPTOR_BLOCK: u32 = 1;
pub(crate) const JBD2_COMMIT_BLOCK: u32 = 2;
pub(crate) const JBD2_SUPERBLOCK_V1: u32 = 3;
pub(crate) const JBD2_SUPERBLOCK_V2: u32 = 4;
pub(crate) const JBD2_REVOKE_BLOCK: u32 = 5;

/// Journal transaction state
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TransactionState {
    Running,
    Committing,
    Committed,
}

/// In-memory journal transaction
pub(crate) struct Transaction {
    pub(crate) tid: u32,
    pub(crate) state: TransactionState,
    pub(crate) blocks: Vec<(u64, Vec<u8>)>, // (block_number, data)
    pub(crate) sequence: u32,
}

/// Journal state
pub(crate) struct Journal {
    pub(crate) inode: u32,
    pub(crate) block_size: usize,
    pub(crate) start_block: u64,
    pub(crate) max_len: u32,
    pub(crate) sequence: u32,
    pub(crate) first: u32,
    pub(crate) current_transaction: Option<Transaction>,
    pub(crate) committed_transactions: Vec<u32>,
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
pub(crate) struct Ext4Fs {
    pub(crate) device_index: usize,
    pub(crate) block_size: usize,
    pub(crate) superblock: Ext4Superblock,
    pub(crate) groups: Vec<Ext4GroupDesc>,
    pub(crate) inodes_per_group: u32,
    pub(crate) inode_size: u16,
    pub(crate) desc_size: u16,
    pub(crate) has_extents: bool,
    pub(crate) has_64bit: bool,
    pub(crate) has_journal: bool,
    pub(crate) journal: Option<Journal>,
    pub(crate) total_blocks: u64,
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
    pub(crate) static ref MOUNTED_FS: Mutex<Vec<Ext4Fs>> = Mutex::new(Vec::new());
}
