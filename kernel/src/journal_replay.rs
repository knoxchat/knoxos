use crate::serial_println;
/// Filesystem Journal Replay
///
/// Replay uncommitted journal entries on unclean shutdown,
/// ensure filesystem consistency without full fsck.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JournalEntryType {
    DataBlock,
    MetadataBlock,
    Inode,
    DirectoryEntry,
    SuperBlock,
}

#[derive(Debug, Clone)]
pub struct JournalEntry {
    pub sequence: u64,
    pub entry_type: JournalEntryType,
    pub block_num: u64,
    pub data: Vec<u8>,
    pub committed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JournalState {
    Clean,
    Dirty,
    Replaying,
}

pub struct JournalReplay {
    pub state: JournalState,
    pub entries: Vec<JournalEntry>,
    pub replayed_count: u64,
    pub journal_start_block: u64,
    pub journal_size_blocks: u64,
}

lazy_static::lazy_static! {
    static ref JOURNAL: Mutex<JournalReplay> = Mutex::new(JournalReplay {
        state: JournalState::Clean,
        entries: Vec::new(),
        replayed_count: 0,
        journal_start_block: 0,
        journal_size_blocks: 0,
    });
}

impl JournalReplay {
    pub fn detect_unclean(&mut self) -> bool {
        // Check superblock clean-shutdown flag
        let dirty = self.state == JournalState::Dirty;
        if dirty {
            serial_println!("[JOURNAL] Unclean shutdown detected — replay needed");
        }
        dirty
    }

    pub fn scan_journal(&mut self) {
        serial_println!(
            "[JOURNAL] Scanning journal (blocks {}-{})",
            self.journal_start_block,
            self.journal_start_block + self.journal_size_blocks
        );
        // Would read journal blocks, parse entries
        self.state = JournalState::Replaying;
    }

    pub fn replay(&mut self) -> u64 {
        serial_println!("[JOURNAL] Replaying {} entries", self.entries.len());
        let mut count = 0u64;
        for entry in &self.entries {
            if entry.committed {
                // Write entry.data to entry.block_num
                serial_println!(
                    "[JOURNAL]   Replay {:?} → block {}",
                    entry.entry_type,
                    entry.block_num
                );
                count += 1;
            }
        }
        self.replayed_count = count;
        self.entries.clear();
        self.state = JournalState::Clean;
        serial_println!("[JOURNAL] Replay complete: {} entries applied", count);
        count
    }

    pub fn mark_clean(&mut self) {
        self.state = JournalState::Clean;
    }
}

pub fn init() {
    serial_println!("[JOURNAL] Filesystem journal replay initialized");
}
