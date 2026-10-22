use crate::block::{self, BLOCK_SIZE as SECTOR_SIZE};

use super::types::{Ext4Error, Ext4Fs};

impl Ext4Fs {
    /// Read a block from the device
    pub(crate) fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), Ext4Error> {
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
    pub(crate) fn write_block(&self, block_num: u64, buf: &[u8]) -> Result<(), Ext4Error> {
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
    pub(crate) fn total_blocks(&self) -> u64 {
        let lo = self.superblock.s_blocks_count_lo as u64;
        if self.has_64bit {
            lo | ((self.superblock.s_blocks_count_hi as u64) << 32)
        } else {
            lo
        }
    }
}
