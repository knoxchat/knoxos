use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// ZFS block pointer — describes a single block on disk
#[derive(Debug, Clone)]
pub struct BlockPointer {
    /// DVA (Data Virtual Address) — up to 3 copies for redundancy
    pub dva: [DiskVirtualAddress; 3],
    /// Logical size (in 512-byte sectors)
    pub lsize: u32,
    /// Physical size (after compression)
    pub psize: u32,
    /// Compression algorithm
    pub compress: Compression,
    /// Checksum algorithm
    pub checksum_type: ChecksumType,
    /// Block type (object type)
    pub block_type: BlockType,
    /// Level in the block tree (0 = data, >0 = indirect)
    pub level: u8,
    /// Birth transaction group
    pub birth_txg: u64,
    /// Fill count (number of non-zero children for indirect blocks)
    pub fill_count: u64,
    /// 256-bit checksum
    pub checksum: [u64; 4],
}

/// Disk Virtual Address — location on a vdev
#[derive(Debug, Clone, Copy, Default)]
pub struct DiskVirtualAddress {
    pub vdev_id: u32,
    pub offset: u64, // Byte offset on the vdev
    pub asize: u32,  // Allocated size
}

/// Compression algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Off,
    Lz4,
    Zstd,
    Gzip,
    Lzo,
    Lzjb,
    Zle,
}

/// Checksum algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumType {
    Inherit,
    On, // Fletcher-4 (default)
    Off,
    Fletcher2,
    Fletcher4,
    Sha256,
    Skein,
    Edonr,
}

/// Block types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    None,
    ObjectDirectory,
    DslDirectory,
    DslDataset,
    DslProps,
    DNode,
    ObjectArray,
    PackedNvlist,
    SpaceMap,
    Zap,
    PlainFileContents,
    DirectoryContents,
    MasterNode,
    DeleteQueue,
    Zvol,
    ZvolProp,
}

/// Virtual device types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdevType {
    Disk,
    Mirror,
    Raidz1,
    Raidz2,
    Raidz3,
    Spare,
    Log,
    Cache,
    Special,
}

/// Virtual device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdevState {
    Unknown,
    Closed,
    Offline,
    Removed,
    Online,
    Degraded,
    Faulted,
}

/// A virtual device in the pool
#[derive(Debug, Clone)]
pub struct Vdev {
    pub id: u32,
    pub vdev_type: VdevType,
    pub state: VdevState,
    pub path: String,
    pub total_space: u64,
    pub allocated: u64,
    pub checksum_errors: u64,
    pub read_errors: u64,
    pub write_errors: u64,
    pub children: Vec<u32>, // Child vdev IDs (for mirror/raidz)
}

/// ZFS pool state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolState {
    Active,
    Exported,
    Destroyed,
    Spare,
    L2Cache,
    Uninitialized,
    Unavail,
    PotentiallyActive,
}

/// ZFS pool health
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolHealth {
    Online,
    Degraded,
    Faulted,
    Offline,
    Removed,
    Unavail,
}

/// ZFS dataset type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetType {
    Filesystem,
    Volume,
    Snapshot,
    Bookmark,
}

/// ZFS property
#[derive(Debug, Clone)]
pub enum ZfsPropValue {
    Uint64(u64),
    Str(String),
    Bool(bool),
}

/// ZFS Dataset
#[derive(Debug, Clone)]
pub struct Dataset {
    pub name: String,
    pub dataset_type: DatasetType,
    pub pool_name: String,
    pub guid: u64,
    pub creation: u64,       // Creation timestamp
    pub used: u64,           // Bytes used
    pub available: u64,      // Bytes available
    pub referenced: u64,     // Bytes referenced
    pub compress_ratio: f32, // Compression ratio
    pub mountpoint: String,
    pub compression: Compression,
    pub checksum: ChecksumType,
    pub dedup: bool,
    pub encryption: bool,
    pub atime: bool,
    pub exec: bool,
    pub readonly: bool,
    pub quota: u64,             // 0 = no quota
    pub reservation: u64,       // 0 = no reservation
    pub record_size: u32,       // Default 128KB
    pub snapshots: Vec<String>, // Snapshot names
    pub clones: Vec<String>,    // Clone names
    pub properties: BTreeMap<String, ZfsPropValue>,
}

/// ZFS Snapshot
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub name: String,    // pool/dataset@snapname
    pub dataset: String, // Parent dataset
    pub creation: u64,
    pub used: u64,       // Space unique to this snapshot
    pub referenced: u64, // Total referenced space
    pub txg: u64,        // Transaction group of snapshot
}

/// Transaction group
#[derive(Debug, Clone)]
pub struct TxgInfo {
    pub txg: u64,
    pub state: TxgState,
    pub birth: u64, // Tick when txg opened
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxgState {
    Open,
    Quiescing,
    Syncing,
    Committed,
}

/// Adaptive Replacement Cache (ARC)
#[derive(Debug)]
pub struct Arc {
    pub max_size: u64,     // Maximum ARC size in bytes
    pub current_size: u64, // Current ARC size
    pub mru_size: u64,     // Most Recently Used cache size
    pub mfu_size: u64,     // Most Frequently Used cache size
    pub hits: u64,
    pub misses: u64,
    pub l2_hits: u64, // L2ARC hits
    pub l2_misses: u64,
    /// Cached blocks (key = block pointer hash)
    pub entries: BTreeMap<u64, ArcEntry>,
}

#[derive(Debug, Clone)]
pub struct ArcEntry {
    pub data: Vec<u8>,
    pub access_count: u32,
    pub last_access: u64,
    pub arc_type: ArcType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcType {
    Mru,      // Most Recently Used
    Mfu,      // Most Frequently Used
    MruGhost, // Ghost of evicted MRU entries
    MfuGhost, // Ghost of evicted MFU entries
}

/// Deduplication table entry
#[derive(Debug, Clone)]
pub struct DdtEntry {
    pub checksum: [u64; 4],
    pub dva: DiskVirtualAddress,
    pub ref_count: u64,
    pub physical_size: u64,
    pub logical_size: u64,
}

/// Scrub state
#[derive(Debug, Clone)]
pub struct ScrubState {
    pub active: bool,
    pub start_time: u64,
    pub end_time: u64,
    pub blocks_scanned: u64,
    pub blocks_repaired: u64,
    pub errors_found: u64,
    pub bytes_scanned: u64,
    pub percent_complete: u8,
}

/// ZFS Storage Pool
#[derive(Debug)]
pub struct ZfsPool {
    pub name: String,
    pub guid: u64,
    pub state: PoolState,
    pub health: PoolHealth,
    pub txg: u64, // Current transaction group
    pub total_space: u64,
    pub allocated: u64,
    pub free: u64,
    pub fragmentation: u8, // Fragmentation percentage
    pub capacity: u8,      // Capacity percentage
    pub dedup_ratio: f32,
    pub version: u32, // Pool version (5000 = feature flags)
    pub vdevs: Vec<Vdev>,
    pub datasets: BTreeMap<String, Dataset>,
    pub snapshots: BTreeMap<String, Snapshot>,
    pub ddt: BTreeMap<u64, DdtEntry>,
    pub scrub: ScrubState,
    pub arc: Arc,
    pub properties: BTreeMap<String, ZfsPropValue>,
}

lazy_static::lazy_static! {
    pub static ref ZFS_POOLS: Mutex<BTreeMap<String, ZfsPool>> = Mutex::new(BTreeMap::new());
}
