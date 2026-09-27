use alloc::vec;
use alloc::vec::Vec;

use super::types::{
    EXT4_EXT_MAGIC, Ext4Error, Ext4Extent, Ext4ExtentHeader, Ext4ExtentIdx, Ext4Fs, Ext4Inode,
};

impl Ext4Fs {
    /// Read the extent tree root from an inode's i_block field
    pub(crate) fn read_extent_header(
        &self,
        inode: &Ext4Inode,
    ) -> Result<Ext4ExtentHeader, Ext4Error> {
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
    pub(crate) fn read_extents_from_inode(
        &self,
        inode: &Ext4Inode,
    ) -> Result<Vec<Ext4Extent>, Ext4Error> {
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
    pub(crate) fn read_extents_from_block(
        &self,
        block_num: u64,
    ) -> Result<Vec<Ext4Extent>, Ext4Error> {
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
    pub(crate) fn extent_logical_to_physical(
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
    pub(crate) fn read_inode_data_extents(&self, inode: &Ext4Inode) -> Result<Vec<u8>, Ext4Error> {
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
    pub(crate) fn read_inode_data_indirect(&self, inode: &Ext4Inode) -> Result<Vec<u8>, Ext4Error> {
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
    pub(crate) fn read_inode_data(&self, inode: &Ext4Inode) -> Result<Vec<u8>, Ext4Error> {
        if self.uses_extents(inode) {
            self.read_inode_data_extents(inode)
        } else {
            self.read_inode_data_indirect(inode)
        }
    }
}
