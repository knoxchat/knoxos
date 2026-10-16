//! JBD2 journal replay.
//!
//! Walks a contiguous JBD2 log (descriptor → data → commit), applies
//! committed filesystem blocks, and discards a torn transaction that
//! never got a matching commit. Used by ext4 mount when `fs-ext4` is on,
//! and by the Gate C8 ramdisk self-test on the default boot path.
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::block::{self, BLOCK_SIZE};
use crate::serial_println;

/// JBD2 magic (`JFS2` in big-endian).
pub const JBD2_MAGIC: u32 = 0xC03B3998;
pub const JBD2_DESCRIPTOR_BLOCK: u32 = 1;
pub const JBD2_COMMIT_BLOCK: u32 = 2;
pub const JBD2_SUPERBLOCK_V1: u32 = 3;
pub const JBD2_SUPERBLOCK_V2: u32 = 4;
pub const JBD2_REVOKE_BLOCK: u32 = 5;

pub const JBD2_FLAG_ESCAPE: u16 = 1;
pub const JBD2_FLAG_SAME_UUID: u16 = 2;
pub const JBD2_FLAG_DELETED: u16 = 4;
pub const JBD2_FLAG_LAST_TAG: u16 = 8;

pub const GATE_C8_MARKER: &str = "GATE_C8 jbd2 replay";
const GATE_C8_PAYLOAD: &[u8] = b"knoxos-c8-jbd2\n";
const GATE_C8_UNCOMMITTED: &[u8] = b"should-not-survive\n";

/// 1024-byte journal blocks (two 512-byte sectors).
const JBD2_BLOCK: usize = 1024;
/// Ramdisk LBA of journal block 0 (well clear of a zeroed header).
const RAM_JOURNAL_LBA: u64 = 64;
/// Filesystem block the committed transaction overwrites.
const FS_BLOCK_COMMITTED: u32 = 200;
/// Filesystem block the torn transaction would overwrite.
const FS_BLOCK_TORN: u32 = 201;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JournalState {
    Clean,
    Dirty,
    Replaying,
}

pub struct JournalReplay {
    pub state: JournalState,
    pub replayed_count: u64,
}

lazy_static::lazy_static! {
    static ref JOURNAL: Mutex<JournalReplay> = Mutex::new(JournalReplay {
        state: JournalState::Clean,
        replayed_count: 0,
    });
}

fn be32(buf: &[u8], off: usize) -> u32 {
    if off + 4 > buf.len() {
        return 0;
    }
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn be16(buf: &[u8], off: usize) -> u16 {
    if off + 2 > buf.len() {
        return 0;
    }
    u16::from_be_bytes([buf[off], buf[off + 1]])
}

fn put_be32(buf: &mut [u8], off: usize, v: u32) {
    if off + 4 <= buf.len() {
        buf[off..off + 4].copy_from_slice(&v.to_be_bytes());
    }
}

fn put_be16(buf: &mut [u8], off: usize, v: u16) {
    if off + 2 <= buf.len() {
        buf[off..off + 2].copy_from_slice(&v.to_be_bytes());
    }
}

fn sectors_per_jblock(jblock_size: usize) -> u64 {
    (jblock_size / BLOCK_SIZE).max(1) as u64
}

fn read_jblock(
    device: usize,
    start_lba: u64,
    jblock_size: usize,
    rel: u32,
    buf: &mut [u8],
) -> bool {
    let spb = sectors_per_jblock(jblock_size);
    let lba = start_lba + u64::from(rel) * spb;
    let count = spb as u8;
    if buf.len() < jblock_size {
        return false;
    }
    block::read_blocks(device, lba, count, &mut buf[..jblock_size]).is_ok()
}

fn write_jblock(device: usize, start_lba: u64, jblock_size: usize, rel: u32, buf: &[u8]) -> bool {
    let spb = sectors_per_jblock(jblock_size);
    let lba = start_lba + u64::from(rel) * spb;
    block::write_blocks(device, lba, spb as u8, &buf[..jblock_size.min(buf.len())]).is_ok()
}

fn write_fs_block(device: usize, jblock_size: usize, fs_block: u32, data: &[u8]) -> bool {
    let spb = sectors_per_jblock(jblock_size);
    let lba = u64::from(fs_block) * spb;
    block::write_blocks(device, lba, spb as u8, &data[..jblock_size.min(data.len())]).is_ok()
}

fn read_fs_block(device: usize, jblock_size: usize, fs_block: u32, buf: &mut [u8]) -> bool {
    let spb = sectors_per_jblock(jblock_size);
    let lba = u64::from(fs_block) * spb;
    block::read_blocks(device, lba, spb as u8, &mut buf[..jblock_size]).is_ok()
}

fn next_rel(rel: u32, first: u32, maxlen: u32) -> u32 {
    let n = rel.saturating_add(1);
    if n >= maxlen { first } else { n }
}

struct Tag {
    fs_block: u32,
    flags: u16,
}

fn parse_tags(desc: &[u8]) -> Vec<Tag> {
    let mut tags = Vec::new();
    let mut off = 12; // after journal_header
    while off + 8 <= desc.len() {
        let fs_block = be32(desc, off);
        let flags = be16(desc, off + 6);
        off += 8;
        if flags & JBD2_FLAG_SAME_UUID == 0 {
            off += 16;
        }
        if flags & JBD2_FLAG_DELETED == 0 && fs_block != 0 {
            tags.push(Tag { fs_block, flags });
        }
        if flags & JBD2_FLAG_LAST_TAG != 0 {
            break;
        }
    }
    tags
}

/// Replay a contiguous JBD2 log starting at `start_lba` (512-byte sectors).
///
/// `s_start == 0` is a clean journal (nothing to apply). A descriptor
/// without a matching commit is discarded. Returns the number of
/// committed transactions written back to the filesystem area.
pub fn replay_contiguous(device: usize, start_lba: u64, jblock_size: usize) -> u32 {
    if jblock_size < 512 || !jblock_size.is_multiple_of(BLOCK_SIZE) {
        return 0;
    }

    let mut jsb = vec![0u8; jblock_size];
    if !read_jblock(device, start_lba, jblock_size, 0, &mut jsb) {
        return 0;
    }
    if be32(&jsb, 0) != JBD2_MAGIC {
        return 0;
    }
    let blocktype = be32(&jsb, 4);
    if blocktype != JBD2_SUPERBLOCK_V1 && blocktype != JBD2_SUPERBLOCK_V2 {
        return 0;
    }

    let sb_block_size = be32(&jsb, 12) as usize;
    let maxlen = be32(&jsb, 16);
    let first = be32(&jsb, 20);
    let s_start = be32(&jsb, 28);
    if sb_block_size != 0 && sb_block_size != jblock_size {
        serial_println!(
            "[jbd2] block size mismatch: sb={} walker={}",
            sb_block_size,
            jblock_size
        );
    }
    if maxlen == 0 || first == 0 {
        return 0;
    }
    if s_start == 0 {
        serial_println!("[jbd2] journal clean (s_start=0)");
        return 0;
    }

    serial_println!(
        "[jbd2] replay: start_lba={} first={} start={} maxlen={}",
        start_lba,
        first,
        s_start,
        maxlen
    );

    let mut rel = s_start;
    let mut scanned = 0u32;
    let mut pending: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut pending_seq = 0u32;
    let mut replayed = 0u32;
    let start_rel = s_start;

    while scanned < maxlen {
        let mut blk = vec![0u8; jblock_size];
        if !read_jblock(device, start_lba, jblock_size, rel, &mut blk) {
            break;
        }
        if be32(&blk, 0) != JBD2_MAGIC {
            break;
        }
        let typ = be32(&blk, 4);
        let seq = be32(&blk, 8);

        match typ {
            JBD2_DESCRIPTOR_BLOCK => {
                pending.clear();
                pending_seq = seq;
                let tags = parse_tags(&blk);
                let mut data_rel = next_rel(rel, first, maxlen);
                for tag in tags {
                    let mut data = vec![0u8; jblock_size];
                    if !read_jblock(device, start_lba, jblock_size, data_rel, &mut data) {
                        pending.clear();
                        break;
                    }
                    if tag.flags & JBD2_FLAG_ESCAPE != 0 && data.len() >= 4 {
                        put_be32(&mut data, 0, JBD2_MAGIC);
                    }
                    pending.push((tag.fs_block, data));
                    data_rel = next_rel(data_rel, first, maxlen);
                    scanned += 1;
                    if data_rel == start_rel && scanned > 0 {
                        break;
                    }
                }
                rel = data_rel;
                scanned += 1;
                if rel == start_rel && scanned > 1 {
                    break;
                }
                continue;
            }
            JBD2_COMMIT_BLOCK => {
                if seq == pending_seq && !pending.is_empty() {
                    for (fs_block, data) in pending.drain(..) {
                        if !write_fs_block(device, jblock_size, fs_block, &data) {
                            serial_println!("[jbd2] failed to apply fs block {}", fs_block);
                        }
                    }
                    replayed += 1;
                    serial_println!("[jbd2] committed transaction seq={}", seq);
                } else {
                    pending.clear();
                }
            }
            JBD2_REVOKE_BLOCK => {
                pending.clear();
            }
            _ => {}
        }

        rel = next_rel(rel, first, maxlen);
        scanned += 1;
        if rel == start_rel {
            break;
        }
    }

    if replayed > 0 {
        // Checkpoint: mark the journal clean so a remount does not re-apply.
        put_be32(&mut jsb, 28, 0);
        let _ = write_jblock(device, start_lba, jblock_size, 0, &jsb);
    }

    JOURNAL.lock().replayed_count = u64::from(replayed);
    JOURNAL.lock().state = JournalState::Clean;

    serial_println!("[jbd2] replay complete: {} transaction(s)", replayed);
    replayed
}

fn write_header(buf: &mut [u8], blocktype: u32, seq: u32) {
    put_be32(buf, 0, JBD2_MAGIC);
    put_be32(buf, 4, blocktype);
    put_be32(buf, 8, seq);
}

fn write_jsb(buf: &mut [u8], start: u32) {
    write_header(buf, JBD2_SUPERBLOCK_V2, 1);
    put_be32(buf, 12, JBD2_BLOCK as u32); // s_blocksize
    put_be32(buf, 16, 16); // s_maxlen
    put_be32(buf, 20, 1); // s_first
    put_be32(buf, 24, 1); // s_sequence
    put_be32(buf, 28, start); // s_start (0 = clean)
}

fn write_descriptor(buf: &mut [u8], seq: u32, fs_block: u32) {
    write_header(buf, JBD2_DESCRIPTOR_BLOCK, seq);
    put_be32(buf, 12, fs_block);
    put_be16(buf, 16, 0); // checksum
    put_be16(buf, 18, JBD2_FLAG_SAME_UUID | JBD2_FLAG_LAST_TAG);
}

/// Commit a JBD2 record without checkpointing the filesystem block, then
/// replay. Committed data must appear; a torn descriptor must not.
pub fn journal_recovery_self_test() -> bool {
    let Some(dev) = block::ramdisk_index() else {
        serial_println!("[jbd2] Gate C8 skipped: no ramdisk");
        return false;
    };

    let mut zero = vec![0u8; JBD2_BLOCK];
    let _ = write_fs_block(dev, JBD2_BLOCK, FS_BLOCK_COMMITTED, &zero);
    let _ = write_fs_block(dev, JBD2_BLOCK, FS_BLOCK_TORN, &zero);

    let mut jsb = vec![0u8; JBD2_BLOCK];
    write_jsb(&mut jsb, 1);
    if !write_jblock(dev, RAM_JOURNAL_LBA, JBD2_BLOCK, 0, &jsb) {
        serial_println!("[jbd2] Gate C8 FAILED: could not write journal superblock");
        return false;
    }

    let mut desc1 = vec![0u8; JBD2_BLOCK];
    write_descriptor(&mut desc1, 1, FS_BLOCK_COMMITTED);
    let mut data1 = vec![0u8; JBD2_BLOCK];
    data1[..GATE_C8_PAYLOAD.len()].copy_from_slice(GATE_C8_PAYLOAD);
    let mut commit = vec![0u8; JBD2_BLOCK];
    write_header(&mut commit, JBD2_COMMIT_BLOCK, 1);

    let mut desc2 = vec![0u8; JBD2_BLOCK];
    write_descriptor(&mut desc2, 2, FS_BLOCK_TORN);
    let mut data2 = vec![0u8; JBD2_BLOCK];
    data2[..GATE_C8_UNCOMMITTED.len()].copy_from_slice(GATE_C8_UNCOMMITTED);

    if !write_jblock(dev, RAM_JOURNAL_LBA, JBD2_BLOCK, 1, &desc1)
        || !write_jblock(dev, RAM_JOURNAL_LBA, JBD2_BLOCK, 2, &data1)
        || !write_jblock(dev, RAM_JOURNAL_LBA, JBD2_BLOCK, 3, &commit)
        || !write_jblock(dev, RAM_JOURNAL_LBA, JBD2_BLOCK, 4, &desc2)
        || !write_jblock(dev, RAM_JOURNAL_LBA, JBD2_BLOCK, 5, &data2)
    {
        serial_println!("[jbd2] Gate C8 FAILED: could not write journal log");
        return false;
    }

    // Crash window: WAL is on disk; filesystem blocks are still zero.
    let mut probe = vec![0u8; JBD2_BLOCK];
    if !read_fs_block(dev, JBD2_BLOCK, FS_BLOCK_COMMITTED, &mut probe)
        || probe[..GATE_C8_PAYLOAD.len()] == GATE_C8_PAYLOAD[..]
    {
        serial_println!("[jbd2] Gate C8 FAILED: filesystem already had committed data");
        return false;
    }

    let replayed = replay_contiguous(dev, RAM_JOURNAL_LBA, JBD2_BLOCK);
    if replayed != 1 {
        serial_println!(
            "[jbd2] Gate C8 FAILED: expected 1 replayed transaction, got {}",
            replayed
        );
        return false;
    }

    if !read_fs_block(dev, JBD2_BLOCK, FS_BLOCK_COMMITTED, &mut probe)
        || probe[..GATE_C8_PAYLOAD.len()] != GATE_C8_PAYLOAD[..]
    {
        serial_println!("[jbd2] Gate C8 FAILED: committed payload missing after replay");
        return false;
    }

    let mut torn = vec![0u8; JBD2_BLOCK];
    if !read_fs_block(dev, JBD2_BLOCK, FS_BLOCK_TORN, &mut torn)
        || torn[..GATE_C8_UNCOMMITTED.len()] == GATE_C8_UNCOMMITTED[..]
    {
        serial_println!("[jbd2] Gate C8 FAILED: uncommitted journal was applied");
        return false;
    }

    // Remount of a checkpointed journal must be a no-op.
    if replay_contiguous(dev, RAM_JOURNAL_LBA, JBD2_BLOCK) != 0 {
        serial_println!("[jbd2] Gate C8 FAILED: clean journal replayed again");
        return false;
    }

    serial_println!("[jbd2] {}", GATE_C8_MARKER);
    true
}

pub fn init() {
    serial_println!("[jbd2] JBD2 journal replay ready");
    let _ = journal_recovery_self_test();
}
