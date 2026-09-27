use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::block::{self, BLOCK_SIZE as SECTOR_SIZE};
use crate::serial_println;

use super::types::{
    COMPAT_HAS_JOURNAL, DirEntry, EXT4_FT_DIR, EXT4_FT_REG_FILE, EXT4_MAGIC, Ext4DirEntry,
    Ext4Error, Ext4Fs, Ext4GroupDesc, Ext4Inode, Ext4Superblock, FileInfo, INCOMPAT_64BIT,
    INCOMPAT_EXTENTS, INCOMPAT_FLEX_BG, MOUNTED_FS, S_IFDIR, S_IFREG,
};

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
