/// XFS — High-performance 64-bit journaling filesystem
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// XFS superblock (core geometry)
pub const XFS_SB_MAGIC: u32 = 0x58465342; // "XFSB"

#[derive(Debug, Clone, Copy)]
pub struct XfsSuperblock {
    pub magic: u32,
    pub block_size: u32,
    pub total_blocks: u64,
    pub total_extents: u64,
    pub inode_count: u64,
    pub free_blocks: u64,
    pub free_inodes: u64,
    pub root_inode: u64,
    pub ag_count: u32,  // allocation groups
    pub ag_blocks: u32, // blocks per AG
    pub inode_size: u16,
    pub dir_block_log: u8,
    pub log_block_start: u64,
    pub log_block_count: u32,
    pub sector_size: u16,
    pub uuid: [u8; 16],
}

/// Allocation Group header
#[derive(Debug, Clone, Copy)]
pub struct AgHeader {
    pub ag_number: u32,
    pub free_blocks: u32,
    pub free_inodes: u32,
    pub root_btree_block: u32,
}

/// XFS inode (on-disk)
#[derive(Debug, Clone)]
pub struct XfsInode {
    pub inode_number: u64,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub size: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub extent_count: u32,
    pub data_fork_format: DataForkFormat,
    pub extents: Vec<XfsExtent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataForkFormat {
    Local,   // inline data in inode
    Extents, // extent list
    Btree,   // B+tree of extents
}

/// XFS extent (startblock, startoff, blockcount)
#[derive(Debug, Clone, Copy)]
pub struct XfsExtent {
    pub file_offset: u64, // logical file block
    pub start_block: u64, // physical block on disk
    pub block_count: u32,
    pub unwritten: bool,
}

/// XFS volume state
pub struct XfsVolume {
    pub sb: XfsSuperblock,
    pub ag_headers: Vec<AgHeader>,
    pub mounted: bool,
}

impl XfsVolume {
    /// Parse superblock from first 512 bytes
    pub fn from_superblock(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 272 {
            return Err("Superblock too short");
        }
        let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if magic != XFS_SB_MAGIC {
            return Err("Invalid XFS magic");
        }

        let sb = XfsSuperblock {
            magic,
            block_size: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            total_blocks: u64::from_be_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
            total_extents: u64::from_be_bytes([
                data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
            ]),
            inode_count: u64::from_be_bytes([
                data[128], data[129], data[130], data[131], data[132], data[133], data[134],
                data[135],
            ]),
            free_blocks: u64::from_be_bytes([
                data[136], data[137], data[138], data[139], data[140], data[141], data[142],
                data[143],
            ]),
            free_inodes: u64::from_be_bytes([
                data[144], data[145], data[146], data[147], data[148], data[149], data[150],
                data[151],
            ]),
            root_inode: u64::from_be_bytes([
                data[56], data[57], data[58], data[59], data[60], data[61], data[62], data[63],
            ]),
            ag_count: u32::from_be_bytes([data[24], data[25], data[26], data[27]]),
            ag_blocks: u32::from_be_bytes([data[28], data[29], data[30], data[31]]),
            inode_size: u16::from_be_bytes([data[104], data[105]]),
            dir_block_log: data[109],
            log_block_start: u64::from_be_bytes([
                data[48], data[49], data[50], data[51], data[52], data[53], data[54], data[55],
            ]),
            log_block_count: u32::from_be_bytes([data[44], data[45], data[46], data[47]]),
            sector_size: u16::from_be_bytes([data[106], data[107]]),
            uuid: {
                let mut u = [0u8; 16];
                u.copy_from_slice(&data[32..48]);
                u
            },
        };

        serial_println!(
            "[xfs] Volume: {} blocks ({}B/block), {} AGs, inode_size={}",
            sb.total_blocks,
            sb.block_size,
            sb.ag_count,
            sb.inode_size
        );

        Ok(Self {
            sb,
            ag_headers: Vec::new(),
            mounted: false,
        })
    }

    /// Read an extent-based file by following the extent list
    pub fn read_file(&self, inode: &XfsInode) -> Vec<u8> {
        let mut data = Vec::new();
        let mut remaining = inode.size;
        for ext in &inode.extents {
            let bytes = (ext.block_count as u64 * self.sb.block_size as u64).min(remaining);
            // Would read `bytes` from disk at ext.start_block
            data.extend(core::iter::repeat_n(0u8, bytes as usize));
            remaining = remaining.saturating_sub(bytes);
        }
        data
    }

    /// Allocate blocks in a specific AG using B+tree
    pub fn allocate_blocks(&mut self, ag: u32, count: u32) -> Option<u64> {
        if let Some(ag_hdr) = self.ag_headers.get_mut(ag as usize) {
            if ag_hdr.free_blocks >= count {
                ag_hdr.free_blocks -= count;
                // Return block address (ag_number * ag_blocks + offset)
                let block = ag as u64 * self.sb.ag_blocks as u64;
                serial_println!(
                    "[xfs] Allocated {} blocks in AG{} at block {}",
                    count,
                    ag,
                    block
                );
                return Some(block);
            }
        }
        None
    }
}

lazy_static::lazy_static! {
    static ref XFS_VOLUMES: Mutex<Vec<XfsVolume>> = Mutex::new(Vec::new());
}

pub fn init() {
    serial_println!("[xfs] XFS filesystem driver initialized");
}
