use alloc::vec;

use super::types::{Ext4Error, Ext4Fs, Ext4Inode};

impl Ext4Fs {
    /// Read an inode
    pub(crate) fn read_inode(&self, inode_num: u32) -> Result<Ext4Inode, Ext4Error> {
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
    pub(crate) fn inode_size(&self, inode: &Ext4Inode) -> u64 {
        (inode.i_size_lo as u64) | ((inode.i_size_high as u64) << 32)
    }

    /// Check if inode uses extent tree
    pub(crate) fn uses_extents(&self, inode: &Ext4Inode) -> bool {
        self.has_extents && (inode.i_flags & 0x80000 != 0) // EXT4_EXTENTS_FL
    }

    /// Write an inode back to disk
    pub(crate) fn write_inode(&self, inode_num: u32, inode: &Ext4Inode) -> Result<(), Ext4Error> {
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
    pub(crate) fn alloc_inode(&mut self) -> Result<u32, Ext4Error> {
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
    pub(crate) fn alloc_block(&mut self) -> Result<u64, Ext4Error> {
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
}
