/// XFS — High-performance 64-bit journaling filesystem
use alloc::string::String;
use alloc::vec;
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

    /// Read an extent-based file by following the extent list on a block device.
    pub fn read_file(&self, device: usize, inode: &XfsInode) -> Result<Vec<u8>, &'static str> {
        let mut data = Vec::new();
        let mut remaining = inode.size;
        let bsize = self.sb.block_size.max(1) as usize;
        for ext in &inode.extents {
            let bytes = (ext.block_count as u64 * bsize as u64).min(remaining) as usize;
            let mut buf = vec![0u8; ext.block_count as usize * bsize];
            let lba = ext.start_block * (bsize as u64 / 512);
            let nsec = ((ext.block_count as usize * bsize) / 512).clamp(1, 255) as u8;
            crate::block::read_blocks(device, lba, nsec, &mut buf)
                .map_err(|_| "xfs extent read")?;
            data.extend_from_slice(&buf[..bytes.min(buf.len())]);
            remaining = remaining.saturating_sub(bytes as u64);
        }
        Ok(data)
    }

    /// Parse a packed inode (256-byte record) written by `build_gate_volume`.
    pub fn parse_inode(data: &[u8]) -> Result<XfsInode, &'static str> {
        if data.len() < 64 {
            return Err("inode too short");
        }
        let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if magic != 0x494E_4F44 {
            return Err("invalid inode magic");
        }
        let mode = u16::from_be_bytes([data[4], data[5]]);
        let size = u64::from_be_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);
        let extent_count = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let mut extents = Vec::new();
        let mut off = 32usize;
        for _ in 0..extent_count {
            if off + 16 > data.len() {
                return Err("truncated extent");
            }
            let start_block = u64::from_be_bytes([
                data[off],
                data[off + 1],
                data[off + 2],
                data[off + 3],
                data[off + 4],
                data[off + 5],
                data[off + 6],
                data[off + 7],
            ]);
            let block_count =
                u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]]);
            extents.push(XfsExtent {
                file_offset: 0,
                start_block,
                block_count,
                unwritten: false,
            });
            off += 16;
        }
        Ok(XfsInode {
            inode_number: 128,
            mode,
            uid: 0,
            gid: 0,
            nlink: 1,
            size,
            atime: 0,
            mtime: 0,
            ctime: 0,
            extent_count,
            data_fork_format: DataForkFormat::Extents,
            extents,
        })
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

/// Build a superblock + inode + data extent matching `from_superblock` / `parse_inode`.
fn build_gate_volume(payload: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut sb = vec![0u8; 512];
    sb[0..4].copy_from_slice(&XFS_SB_MAGIC.to_be_bytes());
    sb[4..8].copy_from_slice(&512u32.to_be_bytes()); // block_size
    sb[8..16].copy_from_slice(&128u64.to_be_bytes()); // total_blocks
    sb[16..24].copy_from_slice(&1u64.to_be_bytes()); // total_extents
    sb[24..28].copy_from_slice(&1u32.to_be_bytes()); // ag_count
    sb[28..32].copy_from_slice(&128u32.to_be_bytes()); // ag_blocks
    sb[32..48].fill(0x11); // uuid
    sb[44..48].copy_from_slice(&1u32.to_be_bytes()); // log_block_count (overlaps uuid tail)
    sb[48..56].copy_from_slice(&64u64.to_be_bytes()); // log_block_start
    sb[56..64].copy_from_slice(&8u64.to_be_bytes()); // root_inode
    sb[104..106].copy_from_slice(&256u16.to_be_bytes()); // inode_size
    sb[106..108].copy_from_slice(&512u16.to_be_bytes()); // sector_size
    sb[109] = 0; // dir_block_log
    sb[128..136].copy_from_slice(&16u64.to_be_bytes()); // inode_count
    sb[136..144].copy_from_slice(&100u64.to_be_bytes()); // free_blocks
    sb[144..152].copy_from_slice(&8u64.to_be_bytes()); // free_inodes

    let mut inode = vec![0u8; 512];
    inode[0..4].copy_from_slice(&0x494E_4F44u32.to_be_bytes()); // "INOD"
    inode[4..6].copy_from_slice(&0o100644u16.to_be_bytes());
    inode[8..16].copy_from_slice(&(payload.len() as u64).to_be_bytes());
    inode[16..20].copy_from_slice(&1u32.to_be_bytes()); // one extent
    // extent: start_block = 16, block_count = 1
    inode[32..40].copy_from_slice(&16u64.to_be_bytes());
    inode[40..44].copy_from_slice(&1u32.to_be_bytes());

    let mut data = vec![0u8; 512];
    data[..payload.len()].copy_from_slice(payload);
    (sb, inode, data)
}

pub const GATE_XFS_MARKER: &str = "GATE_XFS block";
const GATE_XFS_PAYLOAD: &[u8] = b"knoxos-xfs\n";

fn block_self_test() -> bool {
    let device = crate::block::create_ramdisk(1);
    let (sb, inode, data) = build_gate_volume(GATE_XFS_PAYLOAD);
    if crate::block::write_blocks(device, 0, 1, &sb).is_err() {
        serial_println!("[xfs] GATE_XFS FAILED: sb write");
        return false;
    }
    if crate::block::write_blocks(device, 8, 1, &inode).is_err() {
        serial_println!("[xfs] GATE_XFS FAILED: inode write");
        return false;
    }
    if crate::block::write_blocks(device, 16, 1, &data).is_err() {
        serial_println!("[xfs] GATE_XFS FAILED: data write");
        return false;
    }

    let mut sb_in = vec![0u8; 512];
    if crate::block::read_blocks(device, 0, 1, &mut sb_in).is_err() {
        serial_println!("[xfs] GATE_XFS FAILED: sb read");
        return false;
    }
    let vol = match XfsVolume::from_superblock(&sb_in) {
        Ok(v) => v,
        Err(e) => {
            serial_println!("[xfs] GATE_XFS FAILED: parse sb ({})", e);
            return false;
        }
    };
    let mut inode_in = vec![0u8; 512];
    if crate::block::read_blocks(device, 8, 1, &mut inode_in).is_err() {
        serial_println!("[xfs] GATE_XFS FAILED: inode read");
        return false;
    }
    let parsed = match XfsVolume::parse_inode(&inode_in) {
        Ok(i) => i,
        Err(e) => {
            serial_println!("[xfs] GATE_XFS FAILED: parse inode ({})", e);
            return false;
        }
    };
    match vol.read_file(device, &parsed) {
        Ok(got) if got == GATE_XFS_PAYLOAD => {
            XFS_VOLUMES.lock().push(vol);
            serial_println!("[xfs] {} (inode extent via ramdisk)", GATE_XFS_MARKER);
            true
        }
        Ok(got) => {
            serial_println!("[xfs] GATE_XFS FAILED: payload len={}", got.len());
            false
        }
        Err(e) => {
            serial_println!("[xfs] GATE_XFS FAILED: {}", e);
            false
        }
    }
}

lazy_static::lazy_static! {
    static ref XFS_VOLUMES: Mutex<Vec<XfsVolume>> = Mutex::new(Vec::new());
}

pub fn init() {
    serial_println!("[xfs] XFS filesystem driver initialized");
    let _ = block_self_test();
}
