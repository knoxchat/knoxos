/// Ext2 Filesystem Driver
/// Read/write support for the ext2 filesystem format
/// Compatible with Linux ext2/ext3 (ext3 = ext2 + journal)
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::block::{self, BLOCK_SIZE as SECTOR_SIZE};

/// Ext2 magic number
const EXT2_MAGIC: u16 = 0xEF53;

/// Ext2 superblock offset (always at byte 1024)
const SUPERBLOCK_OFFSET: u64 = 1024;

/// File types in directory entries
const EXT2_FT_UNKNOWN: u8 = 0;
const EXT2_FT_REG_FILE: u8 = 1;
const EXT2_FT_DIR: u8 = 2;
const EXT2_FT_CHRDEV: u8 = 3;
const EXT2_FT_BLKDEV: u8 = 4;
const EXT2_FT_FIFO: u8 = 5;
const EXT2_FT_SOCK: u8 = 6;
const EXT2_FT_SYMLINK: u8 = 7;

/// Inode flags
const EXT2_S_IFREG: u16 = 0x8000;
const EXT2_S_IFDIR: u16 = 0x4000;
const EXT2_S_IFLNK: u16 = 0xA000;
const EXT2_S_IFCHR: u16 = 0x2000;
const EXT2_S_IFBLK: u16 = 0x6000;

/// Root inode number
const EXT2_ROOT_INODE: u32 = 2;

/// Ext2 superblock (on-disk layout, 1024 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext2Superblock {
    s_inodes_count: u32,
    s_blocks_count: u32,
    s_r_blocks_count: u32,
    s_free_blocks_count: u32,
    s_free_inodes_count: u32,
    s_first_data_block: u32,
    s_log_block_size: u32,
    s_log_frag_size: u32,
    s_blocks_per_group: u32,
    s_frags_per_group: u32,
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
    // Extended superblock fields (rev >= 1)
    s_first_ino: u32,
    s_inode_size: u16,
    s_block_group_nr: u16,
    s_feature_compat: u32,
    s_feature_incompat: u32,
    s_feature_ro_compat: u32,
    s_uuid: [u8; 16],
    s_volume_name: [u8; 16],
    s_last_mounted: [u8; 64],
    s_algo_bitmap: u32,
    // Padding to 1024 bytes
    _padding: [u8; 820],
}

/// Block group descriptor (on-disk, 32 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext2BlockGroupDesc {
    bg_block_bitmap: u32,
    bg_inode_bitmap: u32,
    bg_inode_table: u32,
    bg_free_blocks_count: u16,
    bg_free_inodes_count: u16,
    bg_used_dirs_count: u16,
    bg_pad: u16,
    bg_reserved: [u8; 12],
}

/// Ext2 inode (on-disk, 128 bytes minimum)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext2Inode {
    i_mode: u16,
    i_uid: u16,
    i_size: u32,
    i_atime: u32,
    i_ctime: u32,
    i_mtime: u32,
    i_dtime: u32,
    i_gid: u16,
    i_links_count: u16,
    i_blocks: u32,
    i_flags: u32,
    i_osd1: u32,
    i_block: [u32; 15], // 12 direct + 1 indirect + 1 double indirect + 1 triple indirect
    i_generation: u32,
    i_file_acl: u32,
    i_dir_acl: u32,
    i_faddr: u32,
    i_osd2: [u8; 12],
}

/// Directory entry (on-disk)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Ext2DirEntry {
    inode: u32,
    rec_len: u16,
    name_len: u8,
    file_type: u8,
    // name follows (variable length)
}

/// Mounted ext2 filesystem state
struct Ext2Fs {
    device_index: usize,
    block_size: usize,
    superblock: Ext2Superblock,
    groups: Vec<Ext2BlockGroupDesc>,
    inodes_per_group: u32,
    inode_size: u16,
}

/// Directory entry (in-memory)
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
    pub uid: u16,
    pub gid: u16,
    pub size: u64,
    pub atime: u32,
    pub mtime: u32,
    pub ctime: u32,
    pub links: u16,
    pub blocks: u32,
}

/// Global mounted filesystems
lazy_static::lazy_static! {
    static ref MOUNTED_FS: Mutex<Vec<Ext2Fs>> = Mutex::new(Vec::new());
}

/// Ext2 error types
#[derive(Debug)]
pub enum Ext2Error {
    InvalidMagic,
    IoError,
    NotFound,
    NotDirectory,
    NotFile,
    InvalidInode,
    NoSpace,
    ReadOnly,
}

impl Ext2Fs {
    /// Read a block from the device
    fn read_block(&self, block_num: u32, buf: &mut [u8]) -> Result<(), Ext2Error> {
        let sectors_per_block = self.block_size / SECTOR_SIZE;
        let start_sector = block_num as u64 * sectors_per_block as u64;

        for i in 0..sectors_per_block {
            let sector = start_sector + i as u64;
            let offset = i * SECTOR_SIZE;
            block::read_blocks(
                self.device_index,
                sector,
                1,
                &mut buf[offset..offset + SECTOR_SIZE],
            )
            .map_err(|_| Ext2Error::IoError)?;
        }
        Ok(())
    }

    /// Read an inode from the filesystem
    fn read_inode(&self, inode_num: u32) -> Result<Ext2Inode, Ext2Error> {
        if inode_num == 0 {
            return Err(Ext2Error::InvalidInode);
        }

        let group = ((inode_num - 1) / self.inodes_per_group) as usize;
        let index = ((inode_num - 1) % self.inodes_per_group) as usize;

        if group >= self.groups.len() {
            return Err(Ext2Error::InvalidInode);
        }

        let inode_table_block = self.groups[group].bg_inode_table;
        let inode_offset = index * self.inode_size as usize;
        let block_offset = inode_offset / self.block_size;
        let offset_in_block = inode_offset % self.block_size;

        let mut block_buf = vec![0u8; self.block_size];
        self.read_block(inode_table_block + block_offset as u32, &mut block_buf)?;

        // Parse inode
        let inode_bytes =
            &block_buf[offset_in_block..offset_in_block + core::mem::size_of::<Ext2Inode>()];
        let inode = unsafe { *(inode_bytes.as_ptr() as *const Ext2Inode) };

        Ok(inode)
    }

    /// Read the data blocks of an inode
    fn read_inode_data(&self, inode: &Ext2Inode) -> Result<Vec<u8>, Ext2Error> {
        let size = inode.i_size as usize;
        let mut data = vec![0u8; size];
        let blocks_needed = size.div_ceil(self.block_size);
        let mut block_buf = vec![0u8; self.block_size];

        for i in 0..blocks_needed {
            let block_num = self.get_block_number(inode, i as u32)?;
            if block_num == 0 {
                // Sparse file - leave zeros
                continue;
            }

            self.read_block(block_num, &mut block_buf)?;

            let offset = i * self.block_size;
            let copy_len = core::cmp::min(self.block_size, size - offset);
            data[offset..offset + copy_len].copy_from_slice(&block_buf[..copy_len]);
        }

        Ok(data)
    }

    /// Get the block number for a logical block index
    fn get_block_number(&self, inode: &Ext2Inode, logical_block: u32) -> Result<u32, Ext2Error> {
        let ptrs_per_block = (self.block_size / 4) as u32;

        if logical_block < 12 {
            // Direct block
            Ok(inode.i_block[logical_block as usize])
        } else if logical_block < 12 + ptrs_per_block {
            // Single indirect
            let indirect_block = inode.i_block[12];
            if indirect_block == 0 {
                return Ok(0);
            }
            let mut buf = vec![0u8; self.block_size];
            self.read_block(indirect_block, &mut buf)?;
            let index = (logical_block - 12) as usize;
            let ptr = u32::from_le_bytes([
                buf[index * 4],
                buf[index * 4 + 1],
                buf[index * 4 + 2],
                buf[index * 4 + 3],
            ]);
            Ok(ptr)
        } else if logical_block < 12 + ptrs_per_block + ptrs_per_block * ptrs_per_block {
            // Double indirect
            let dindir_block = inode.i_block[13];
            if dindir_block == 0 {
                return Ok(0);
            }
            let adjusted = logical_block - 12 - ptrs_per_block;
            let first = adjusted / ptrs_per_block;
            let second = adjusted % ptrs_per_block;

            let mut buf = vec![0u8; self.block_size];
            self.read_block(dindir_block, &mut buf)?;
            let indirect = u32::from_le_bytes([
                buf[first as usize * 4],
                buf[first as usize * 4 + 1],
                buf[first as usize * 4 + 2],
                buf[first as usize * 4 + 3],
            ]);
            if indirect == 0 {
                return Ok(0);
            }

            self.read_block(indirect, &mut buf)?;
            let ptr = u32::from_le_bytes([
                buf[second as usize * 4],
                buf[second as usize * 4 + 1],
                buf[second as usize * 4 + 2],
                buf[second as usize * 4 + 3],
            ]);
            Ok(ptr)
        } else {
            // Triple indirect
            Err(Ext2Error::NotFound) // Simplified for now
        }
    }

    /// Read directory entries from an inode
    fn read_dir_entries(&self, inode_num: u32) -> Result<Vec<DirEntry>, Ext2Error> {
        let inode = self.read_inode(inode_num)?;

        if (inode.i_mode & 0xF000) != EXT2_S_IFDIR {
            return Err(Ext2Error::NotDirectory);
        }

        let data = self.read_inode_data(&inode)?;
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if offset + 8 > data.len() {
                break;
            }

            let entry = unsafe { *(data[offset..].as_ptr() as *const Ext2DirEntry) };

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

    /// Lookup a file by path
    fn lookup_path(&self, path: &str) -> Result<u32, Ext2Error> {
        let mut current_inode = EXT2_ROOT_INODE;

        if path == "/" {
            return Ok(current_inode);
        }

        let path = path.trim_start_matches('/');
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }

            let entries = self.read_dir_entries(current_inode)?;
            let found = entries.iter().find(|e| e.name == component);

            match found {
                Some(entry) => current_inode = entry.inode,
                None => return Err(Ext2Error::NotFound),
            }
        }

        Ok(current_inode)
    }

    /// Get file information
    fn stat(&self, inode_num: u32) -> Result<FileInfo, Ext2Error> {
        let inode = self.read_inode(inode_num)?;
        Ok(FileInfo {
            inode: inode_num,
            mode: inode.i_mode,
            uid: inode.i_uid,
            gid: inode.i_gid,
            size: inode.i_size as u64,
            atime: inode.i_atime,
            mtime: inode.i_mtime,
            ctime: inode.i_ctime,
            links: inode.i_links_count,
            blocks: inode.i_blocks,
        })
    }
}

/// Mount an ext2 filesystem from a block device
pub fn mount(device_index: usize) -> Result<usize, Ext2Error> {
    // Read superblock (at offset 1024, which is sector 2-3)
    let mut sb_buf = [0u8; 1024];
    block::read_blocks(device_index, 2, 2, &mut sb_buf).map_err(|_| Ext2Error::IoError)?;

    let superblock = unsafe { *(sb_buf.as_ptr() as *const Ext2Superblock) };

    // Verify magic number
    if superblock.s_magic != EXT2_MAGIC {
        return Err(Ext2Error::InvalidMagic);
    }

    let block_size = 1024usize << superblock.s_log_block_size;
    let num_groups = superblock
        .s_blocks_count
        .div_ceil(superblock.s_blocks_per_group);

    let inode_size = if superblock.s_rev_level >= 1 {
        superblock.s_inode_size
    } else {
        128
    };

    // Read block group descriptors
    let bgd_block = if block_size == 1024 { 2 } else { 1 };
    let bgd_size = num_groups as usize * core::mem::size_of::<Ext2BlockGroupDesc>();
    let bgd_blocks = bgd_size.div_ceil(block_size);

    let mut bgd_buf = vec![0u8; bgd_blocks * block_size];
    let sectors_per_block = block_size / SECTOR_SIZE;
    for i in 0..bgd_blocks {
        let block = bgd_block as u64 + i as u64;
        let sector = block * sectors_per_block as u64;
        block::read_blocks(
            device_index,
            sector,
            sectors_per_block as u8,
            &mut bgd_buf[i * block_size..(i + 1) * block_size],
        )
        .map_err(|_| Ext2Error::IoError)?;
    }

    let mut groups = Vec::new();
    for i in 0..num_groups as usize {
        let offset = i * core::mem::size_of::<Ext2BlockGroupDesc>();
        let bgd = unsafe { *(bgd_buf[offset..].as_ptr() as *const Ext2BlockGroupDesc) };
        groups.push(bgd);
    }

    let volume_name = {
        let name_bytes = &superblock.s_volume_name;
        let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(16);
        String::from_utf8_lossy(&name_bytes[..len]).into_owned()
    };

    let fs = Ext2Fs {
        device_index,
        block_size,
        superblock,
        groups,
        inodes_per_group: superblock.s_inodes_per_group,
        inode_size,
    };

    let mut mounted = MOUNTED_FS.lock();
    let index = mounted.len();
    mounted.push(fs);

    let sb_inodes = { superblock.s_inodes_count };
    let sb_blocks = { superblock.s_blocks_count };
    crate::serial_println!(
        "[KnoxOS] ext2: Mounted device {} - block_size={}, inodes={}, blocks={}, volume=\"{}\"",
        device_index,
        block_size,
        sb_inodes,
        sb_blocks,
        volume_name
    );

    Ok(index)
}

/// Read a file from a mounted ext2 filesystem
pub fn read_file(fs_index: usize, path: &str) -> Result<Vec<u8>, Ext2Error> {
    let mounted = MOUNTED_FS.lock();
    let fs = mounted.get(fs_index).ok_or(Ext2Error::NotFound)?;

    let inode_num = fs.lookup_path(path)?;
    let inode = fs.read_inode(inode_num)?;

    if (inode.i_mode & 0xF000) != EXT2_S_IFREG {
        return Err(Ext2Error::NotFile);
    }

    fs.read_inode_data(&inode)
}

/// List directory contents
pub fn list_dir(fs_index: usize, path: &str) -> Result<Vec<DirEntry>, Ext2Error> {
    let mounted = MOUNTED_FS.lock();
    let fs = mounted.get(fs_index).ok_or(Ext2Error::NotFound)?;

    let inode_num = fs.lookup_path(path)?;
    fs.read_dir_entries(inode_num)
}

/// Get file information
pub fn stat_file(fs_index: usize, path: &str) -> Result<FileInfo, Ext2Error> {
    let mounted = MOUNTED_FS.lock();
    let fs = mounted.get(fs_index).ok_or(Ext2Error::NotFound)?;

    let inode_num = fs.lookup_path(path)?;
    fs.stat(inode_num)
}

/// List all mounted ext2 filesystems
pub fn list_mounts() -> Vec<String> {
    let mounts = MOUNTED_FS.lock();
    let mut result = Vec::new();
    for (i, fs) in mounts.iter().enumerate() {
        let inodes = { fs.superblock.s_inodes_count };
        let blocks = { fs.superblock.s_blocks_count };
        result.push(alloc::format!(
            "/dev/sd{} on /mnt/{} type ext2 (block_size={}, inodes={}, blocks={})",
            (b'a' + i as u8) as char,
            i,
            fs.block_size,
            inodes,
            blocks,
        ));
    }
    result
}

/// Initialize the ext2 driver
pub fn init() {
    crate::serial_println!("[KnoxOS] ext2 filesystem driver loaded");
}
