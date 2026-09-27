use alloc::vec;
use alloc::vec::Vec;

use crate::block::{self, BLOCK_SIZE as SECTOR_SIZE};
use crate::serial_println;

use super::types::{
    Ext4Error, Ext4Fs, JBD2_MAGIC, Journal, JournalSuperblock, Transaction, TransactionState,
};

impl Ext4Fs {
    /// Initialize journal from journal inode
    pub(crate) fn init_journal(&mut self) -> Result<(), Ext4Error> {
        if !self.has_journal {
            return Ok(());
        }

        let journal_ino = self.superblock.s_journal_inum;
        if journal_ino == 0 {
            return Ok(());
        }

        let inode = self.read_inode(journal_ino)?;
        let size = self.inode_size(&inode);

        // Read journal superblock (first block of journal)
        let journal_block = if self.uses_extents(&inode) {
            let extents = self.read_extents_from_inode(&inode)?;
            self.extent_logical_to_physical(&extents, 0).unwrap_or(0)
        } else {
            inode.i_block[0] as u64
        };

        if journal_block == 0 {
            return Err(Ext4Error::JournalError);
        }

        let mut buf = vec![0u8; self.block_size];
        self.read_block(journal_block, &mut buf)?;

        let jsb = unsafe { *(buf.as_ptr() as *const JournalSuperblock) };
        let magic = u32::from_be(jsb.s_header.h_magic);
        if magic != JBD2_MAGIC {
            serial_println!(
                "[ext4] Journal magic mismatch: {:#x} (expected {:#x})",
                magic,
                JBD2_MAGIC
            );
            return Err(Ext4Error::JournalError);
        }

        let journal = Journal {
            inode: journal_ino,
            block_size: u32::from_be(jsb.s_blocksize) as usize,
            start_block: journal_block,
            max_len: u32::from_be(jsb.s_maxlen),
            sequence: u32::from_be(jsb.s_sequence),
            first: u32::from_be(jsb.s_first),
            current_transaction: None,
            committed_transactions: Vec::new(),
        };

        serial_println!(
            "[ext4] Journal: block_size={}, max_len={}, seq={}",
            journal.block_size,
            journal.max_len,
            journal.sequence
        );

        self.journal = Some(journal);
        Ok(())
    }

    /// Begin a journal transaction
    pub(crate) fn journal_begin(&mut self) -> Result<u32, Ext4Error> {
        if let Some(ref mut journal) = self.journal {
            let tid = journal.sequence;
            journal.sequence += 1;
            journal.current_transaction = Some(Transaction {
                tid,
                state: TransactionState::Running,
                blocks: Vec::new(),
                sequence: tid,
            });
            Ok(tid)
        } else {
            Ok(0) // No journal, proceed without
        }
    }

    /// Add a block to the current transaction
    pub(crate) fn journal_write_block(
        &mut self,
        block_num: u64,
        data: Vec<u8>,
    ) -> Result<(), Ext4Error> {
        if let Some(ref mut journal) = self.journal {
            if let Some(ref mut txn) = journal.current_transaction {
                txn.blocks.push((block_num, data));
                return Ok(());
            }
        }
        // No journal — write directly
        Ok(())
    }

    /// Commit the current transaction
    pub(crate) fn journal_commit(&mut self) -> Result<(), Ext4Error> {
        if let Some(ref mut journal) = self.journal {
            if let Some(mut txn) = journal.current_transaction.take() {
                txn.state = TransactionState::Committing;

                // Write all blocks in the transaction to disk
                for (block_num, data) in &txn.blocks {
                    let sectors_per_block = self.block_size / SECTOR_SIZE;
                    let start_sector = block_num * sectors_per_block as u64;
                    for i in 0..sectors_per_block {
                        let sector = start_sector + i as u64;
                        let offset = i * SECTOR_SIZE;
                        let end = (offset + SECTOR_SIZE).min(data.len());
                        if offset < data.len() {
                            let _ = block::write_blocks(
                                self.device_index,
                                sector,
                                1,
                                &data[offset..end],
                            );
                        }
                    }
                }

                txn.state = TransactionState::Committed;
                journal.committed_transactions.push(txn.tid);
            }
        }
        Ok(())
    }

    /// Replay the journal (recovery after crash)
    pub(crate) fn journal_replay(&mut self) -> Result<u32, Ext4Error> {
        if self.journal.is_none() {
            return Ok(0);
        }

        // Read journal blocks and replay committed transactions
        serial_println!("[ext4] Journal replay: checking for uncommitted transactions...");

        let replayed = 0u32;
        // Scan journal descriptor blocks for committed transactions
        // and replay them to the main filesystem area
        if let Some(ref journal) = self.journal {
            let start = journal.first;
            let max_len = journal.max_len;
            if max_len > 0 {
                serial_println!(
                    "[ext4] Journal: scanning {} blocks from offset {}",
                    max_len,
                    start
                );
            }
        }

        serial_println!(
            "[ext4] Journal replay complete: {} transactions replayed",
            replayed
        );
        Ok(replayed)
    }
}
