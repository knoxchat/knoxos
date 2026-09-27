use alloc::string::String;
use alloc::vec::Vec;

use super::types::{
    DirEntry, EXT4_ROOT_INODE, Ext4DirEntry, Ext4Error, Ext4Fs, Ext4Inode, FileInfo, S_IFDIR,
};

impl Ext4Fs {
    /// Read directory entries
    pub(crate) fn read_dir_entries(&self, inode_num: u32) -> Result<Vec<DirEntry>, Ext4Error> {
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
    pub(crate) fn lookup_path(&self, path: &str) -> Result<u32, Ext4Error> {
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
    pub(crate) fn stat(&self, inode_num: u32) -> Result<FileInfo, Ext4Error> {
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

    /// Add a directory entry to a directory inode
    pub(crate) fn add_dir_entry(
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
    pub(crate) fn write_dir_data(
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
                    let mut block_buf = alloc::vec![0u8; self.block_size];
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
                    let mut block_buf = alloc::vec![0u8; self.block_size];
                    block_buf[..end - offset].copy_from_slice(&data[offset..end]);
                    self.write_block(block_num, &block_buf)?;
                }
            }
        }
        Ok(())
    }
}
