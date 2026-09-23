/// Persistent storage layer for KnoxOS VFS
///
/// Uses the virtio-blk device to persist user files across reboots.
/// Format: Simple header + file entries written sequentially, plus a
/// write-ahead log at the end of the disk so a crash after `fsync` of a
/// committed journal record can be replayed (Gate C2).
///
/// Disk layout:
///   Sector 0:     Magic header (KNOXPERSIST + entry count + total sectors used)
///   Sector 1+:    File entries, each consisting of:
///                    - 4 bytes: path length (u32 LE)
///                    - 4 bytes: data length (u32 LE)
///                    - 2 bytes: permissions (u16 LE)
///                    - 2 bytes: file type (u16 LE, 0=Regular, 1=Directory)
///                    - N bytes: path (UTF-8)
///                    - M bytes: file data
///                    - Padded to sector boundary (512 bytes)
///   Last 256 sectors: single-slot WAL (`KNOXJRNL`). Committed records are
///                    applied to the blob store on the next restore.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const SECTOR_SIZE: usize = 512;
const MAGIC: &[u8; 12] = b"KNOXPERSIST\0";
const JOURNAL_MAGIC: &[u8; 12] = b"KNOXJRNL\0\0\0\0";
/// One outstanding transaction; 128 KiB is enough for Gate C2 and typical files.
const JOURNAL_SECTORS: u64 = 256;
const JRNL_EMPTY: u32 = 0;
const JRNL_WRITING: u32 = 1;
const JRNL_COMMITTED: u32 = 2;
const JRNL_HEADER_SIZE: usize = 40;
/// Maximum file size to persist (files larger than this are kept in-memory only)
/// This prevents OOM when persisting very large files like 125MB .deb packages
const MAX_PERSIST_SIZE: usize = 64 * 1024 * 1024; // 64 MB

/// Paths that should NOT be persisted (virtual FS and boot-generated ELFs).
/// Everything else — including `/etc` — is the RAM VFS root snapshot.
const SKIP_PREFIXES: &[&str] = &["/dev/", "/proc/", "/sys/", "/run/", "/bin/", "/sbin/"];

/// Check if a path should be persisted across reboot.
fn should_persist(path: &str) -> bool {
    if path == "/" || path.is_empty() {
        return false;
    }
    for skip in SKIP_PREFIXES {
        if path.starts_with(skip) || path == skip.trim_end_matches('/') {
            return false;
        }
    }
    true
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// WAL lives in the last `JOURNAL_SECTORS` of the disk so blob-store data
/// starting at sector 1 is not overwritten.
fn journal_base() -> Option<u64> {
    let cap = crate::virtio_blk::capacity();
    if cap <= JOURNAL_SECTORS + 1 {
        None
    } else {
        Some(cap - JOURNAL_SECTORS)
    }
}

fn data_limit() -> u64 {
    journal_base().unwrap_or_else(crate::virtio_blk::capacity)
}

fn read_store_header() -> Option<(u32, u64)> {
    let mut header_buf = [0u8; SECTOR_SIZE];
    if !crate::virtio_blk::read(0, 1, &mut header_buf) {
        return None;
    }
    if &header_buf[0..12] == MAGIC {
        let count = u32::from_le_bytes([
            header_buf[12],
            header_buf[13],
            header_buf[14],
            header_buf[15],
        ]);
        let next = u64::from_le_bytes([
            header_buf[16],
            header_buf[17],
            header_buf[18],
            header_buf[19],
            header_buf[20],
            header_buf[21],
            header_buf[22],
            header_buf[23],
        ]);
        Some((count, next))
    } else {
        Some((0u32, 1u64))
    }
}

fn write_store_header(entry_count: u32, next_sector: u64) -> bool {
    let mut new_header = [0u8; SECTOR_SIZE];
    new_header[0..12].copy_from_slice(MAGIC);
    new_header[12..16].copy_from_slice(&entry_count.to_le_bytes());
    new_header[16..24].copy_from_slice(&next_sector.to_le_bytes());
    crate::virtio_blk::write(0, 1, &new_header)
}

/// Append one blob-store entry and update the KNOXPERSIST header.
fn apply_entry(path: &str, data: &[u8], permissions: u16, file_type: u16) -> bool {
    let (entry_count, next_sector) = match read_store_header() {
        Some(h) => h,
        None => return false,
    };

    let path_bytes = path.as_bytes();
    let entry_header_size = 4 + 4 + 2 + 2;
    let entry_size = entry_header_size + path_bytes.len() + data.len();
    let sectors_needed = entry_size.div_ceil(SECTOR_SIZE) as u64;

    if next_sector < 1 || next_sector + sectors_needed > data_limit() {
        crate::serial_println!("[persist] Disk full, cannot persist {}", path);
        return false;
    }

    let buf_size = (sectors_needed as usize) * SECTOR_SIZE;
    let mut entry_buf = vec![0u8; buf_size];
    let mut offset = 0;

    let path_len = path_bytes.len() as u32;
    entry_buf[offset..offset + 4].copy_from_slice(&path_len.to_le_bytes());
    offset += 4;
    let data_len = data.len() as u32;
    entry_buf[offset..offset + 4].copy_from_slice(&data_len.to_le_bytes());
    offset += 4;
    entry_buf[offset..offset + 2].copy_from_slice(&permissions.to_le_bytes());
    offset += 2;
    entry_buf[offset..offset + 2].copy_from_slice(&file_type.to_le_bytes());
    offset += 2;
    entry_buf[offset..offset + path_bytes.len()].copy_from_slice(path_bytes);
    offset += path_bytes.len();
    if !data.is_empty() {
        entry_buf[offset..offset + data.len()].copy_from_slice(data);
    }

    if !crate::virtio_blk::write(next_sector, sectors_needed as usize, &entry_buf) {
        crate::serial_println!("[persist] Failed to write entry for {}", path);
        return false;
    }

    let new_count = entry_count + 1;
    let new_next = next_sector + sectors_needed;
    if !write_store_header(new_count, new_next) {
        crate::serial_println!("[persist] Failed to update header");
        return false;
    }

    crate::serial_println!(
        "[persist] Saved {} ({} bytes, {} sectors at sector {})",
        path,
        data.len(),
        sectors_needed,
        next_sector
    );
    true
}

fn journal_write(path: &str, data: &[u8], permissions: u16, file_type: u16, state: u32) -> bool {
    let Some(base) = journal_base() else {
        return false;
    };
    let path_bytes = path.as_bytes();
    let payload_len = path_bytes.len() + data.len();
    let total = JRNL_HEADER_SIZE + payload_len;
    let sectors = total.div_ceil(SECTOR_SIZE) as u64;
    if sectors == 0 || sectors > JOURNAL_SECTORS {
        return false;
    }

    let buf_size = (sectors as usize) * SECTOR_SIZE;
    let mut buf = vec![0u8; buf_size];
    buf[0..12].copy_from_slice(JOURNAL_MAGIC);
    buf[12..16].copy_from_slice(&state.to_le_bytes());
    buf[16..24].copy_from_slice(&1u64.to_le_bytes());
    buf[24..28].copy_from_slice(&(path_bytes.len() as u32).to_le_bytes());
    buf[28..32].copy_from_slice(&(data.len() as u32).to_le_bytes());
    buf[32..34].copy_from_slice(&permissions.to_le_bytes());
    buf[34..36].copy_from_slice(&file_type.to_le_bytes());
    buf[JRNL_HEADER_SIZE..JRNL_HEADER_SIZE + path_bytes.len()].copy_from_slice(path_bytes);
    if !data.is_empty() {
        let data_off = JRNL_HEADER_SIZE + path_bytes.len();
        buf[data_off..data_off + data.len()].copy_from_slice(data);
    }
    let crc = crc32(&buf[JRNL_HEADER_SIZE..JRNL_HEADER_SIZE + payload_len]);
    buf[36..40].copy_from_slice(&crc.to_le_bytes());

    crate::virtio_blk::write(base, sectors as usize, &buf)
}

fn journal_clear() -> bool {
    let Some(base) = journal_base() else {
        return false;
    };
    let mut buf = [0u8; SECTOR_SIZE];
    buf[0..12].copy_from_slice(JOURNAL_MAGIC);
    buf[12..16].copy_from_slice(&JRNL_EMPTY.to_le_bytes());
    crate::virtio_blk::write(base, 1, &buf)
}

fn journal_read() -> Option<(u32, String, Vec<u8>, u16, u16)> {
    let base = journal_base()?;
    let mut first = [0u8; SECTOR_SIZE];
    if !crate::virtio_blk::read(base, 1, &mut first) {
        return None;
    }
    if &first[0..12] != JOURNAL_MAGIC {
        return None;
    }
    let state = u32::from_le_bytes([first[12], first[13], first[14], first[15]]);
    if state == JRNL_EMPTY {
        return None;
    }
    let path_len = u32::from_le_bytes([first[24], first[25], first[26], first[27]]) as usize;
    let data_len = u32::from_le_bytes([first[28], first[29], first[30], first[31]]) as usize;
    let permissions = u16::from_le_bytes([first[32], first[33]]);
    let file_type = u16::from_le_bytes([first[34], first[35]]);
    let stored_crc = u32::from_le_bytes([first[36], first[37], first[38], first[39]]);

    let payload_len = path_len.checked_add(data_len)?;
    let total = JRNL_HEADER_SIZE.checked_add(payload_len)?;
    let sectors = total.div_ceil(SECTOR_SIZE) as u64;
    if path_len > 4096 || sectors == 0 || sectors > JOURNAL_SECTORS {
        return None;
    }

    let buf_size = (sectors as usize) * SECTOR_SIZE;
    let mut buf = vec![0u8; buf_size];
    buf[..SECTOR_SIZE].copy_from_slice(&first);
    if sectors > 1 {
        let remaining = (sectors - 1) as usize;
        if !crate::virtio_blk::read(
            base + 1,
            remaining,
            &mut buf[SECTOR_SIZE..SECTOR_SIZE + remaining * SECTOR_SIZE],
        ) {
            return None;
        }
    }

    let payload = &buf[JRNL_HEADER_SIZE..JRNL_HEADER_SIZE + payload_len];
    if crc32(payload) != stored_crc {
        crate::serial_println!("[persist] Journal CRC mismatch — treating as uncommitted");
        return None;
    }

    let path = match core::str::from_utf8(&payload[..path_len]) {
        Ok(p) => String::from(p),
        Err(_) => return None,
    };
    let data = payload[path_len..].to_vec();
    Some((state, path, data, permissions, file_type))
}

/// Apply a committed WAL record to the blob store. Uncommitted or torn
/// records are discarded. Returns the number of transactions replayed.
pub fn replay_journal() -> u32 {
    match journal_read() {
        Some((state, path, data, permissions, file_type)) if state == JRNL_COMMITTED => {
            crate::serial_println!("[persist] Replaying committed journal for {}", path);
            if apply_entry(&path, &data, permissions, file_type) {
                let _ = journal_clear();
                crate::virtio_blk::flush();
                1
            } else {
                0
            }
        }
        Some((state, path, _, _, _)) => {
            crate::serial_println!(
                "[persist] Discarding uncommitted journal (state={}, path={})",
                state,
                path
            );
            let _ = journal_clear();
            crate::virtio_blk::flush();
            0
        }
        None => 0,
    }
}

fn commit_through_journal(path: &str, data: &[u8], permissions: u16, file_type: u16) -> bool {
    if journal_write(path, data, permissions, file_type, JRNL_COMMITTED) {
        crate::virtio_blk::flush();
        if apply_entry(path, data, permissions, file_type) {
            crate::virtio_blk::flush();
            let _ = journal_clear();
            crate::virtio_blk::flush();
            return true;
        }
        // Apply failed after commit: leave the WAL for the next restore.
        return false;
    }
    // Record did not fit in the journal slot — best-effort direct write.
    if apply_entry(path, data, permissions, file_type) {
        crate::virtio_blk::flush();
        true
    } else {
        false
    }
}

/// Persist a single file to disk immediately after it's written to VFS.
/// This appends/updates the file in the persistent store.
pub fn persist_file(path: &str, data: &[u8], permissions: u16) {
    if !should_persist(path) {
        return;
    }
    if !crate::virtio_blk::is_available() {
        return;
    }
    if data.len() > MAX_PERSIST_SIZE {
        crate::serial_println!(
            "[persist] Skipping {} ({} bytes > {} max), too large for disk persistence",
            path,
            data.len(),
            MAX_PERSIST_SIZE
        );
        return;
    }
    let _ = commit_through_journal(path, data, permissions, 0);
}

/// Persist a directory entry
pub fn persist_directory(path: &str, permissions: u16) {
    if !should_persist(path) {
        return;
    }
    if !crate::virtio_blk::is_available() {
        return;
    }
    let _ = commit_through_journal(path, &[], permissions, 1);
}

/// Restore all persisted files into the VFS on boot
pub fn restore_all() {
    if !crate::virtio_blk::is_available() {
        crate::serial_println!("[persist] No virtio-blk device, skipping restore");
        return;
    }

    let replayed = replay_journal();
    if replayed > 0 {
        crate::serial_println!(
            "[persist] Replayed {} committed journal transaction(s)",
            replayed
        );
    }

    let mut header_buf = [0u8; SECTOR_SIZE];
    if !crate::virtio_blk::read(0, 1, &mut header_buf) {
        crate::serial_println!("[persist] Failed to read header sector");
        return;
    }

    if &header_buf[0..12] != MAGIC {
        crate::serial_println!("[persist] No persistent data found (fresh disk), initializing...");
        // Write empty header
        let mut new_header = [0u8; SECTOR_SIZE];
        new_header[0..12].copy_from_slice(MAGIC);
        new_header[12..16].copy_from_slice(&0u32.to_le_bytes());
        new_header[16..24].copy_from_slice(&1u64.to_le_bytes());
        crate::virtio_blk::write(0, 1, &new_header);
        crate::virtio_blk::flush();
        return;
    }

    let entry_count = u32::from_le_bytes([
        header_buf[12],
        header_buf[13],
        header_buf[14],
        header_buf[15],
    ]);
    let total_sectors = u64::from_le_bytes([
        header_buf[16],
        header_buf[17],
        header_buf[18],
        header_buf[19],
        header_buf[20],
        header_buf[21],
        header_buf[22],
        header_buf[23],
    ]);

    crate::serial_println!(
        "[persist] Found {} entries, {} sectors used",
        entry_count,
        total_sectors
    );

    if entry_count == 0 {
        return;
    }

    let mut current_sector: u64 = 1;
    let mut restored_files = 0u32;
    let mut restored_dirs = 0u32;

    for _i in 0..entry_count {
        // Read the first sector of the entry to get the header
        let mut first_sector = [0u8; SECTOR_SIZE];
        if !crate::virtio_blk::read(current_sector, 1, &mut first_sector) {
            crate::serial_println!("[persist] Failed to read sector {}", current_sector);
            break;
        }

        let path_len = u32::from_le_bytes([
            first_sector[0],
            first_sector[1],
            first_sector[2],
            first_sector[3],
        ]) as usize;
        let data_len = u32::from_le_bytes([
            first_sector[4],
            first_sector[5],
            first_sector[6],
            first_sector[7],
        ]) as usize;
        let permissions = u16::from_le_bytes([first_sector[8], first_sector[9]]);
        let file_type = u16::from_le_bytes([first_sector[10], first_sector[11]]);

        let entry_header_size = 4 + 4 + 2 + 2;
        let entry_size = entry_header_size + path_len + data_len;
        let sectors_needed = entry_size.div_ceil(SECTOR_SIZE) as u64;

        // Read all sectors for this entry
        let buf_size = (sectors_needed as usize) * SECTOR_SIZE;
        let mut entry_buf = vec![0u8; buf_size];

        // Copy the first sector we already read
        entry_buf[..SECTOR_SIZE].copy_from_slice(&first_sector);

        // Read remaining sectors if needed
        if sectors_needed > 1 {
            let remaining = (sectors_needed - 1) as usize;
            if !crate::virtio_blk::read(
                current_sector + 1,
                remaining,
                &mut entry_buf[SECTOR_SIZE..SECTOR_SIZE + remaining * SECTOR_SIZE],
            ) {
                crate::serial_println!(
                    "[persist] Failed to read entry data at sector {}",
                    current_sector
                );
                break;
            }
        }

        // Parse the path
        let path_start = entry_header_size;
        let path_end = path_start + path_len;
        let path = match core::str::from_utf8(&entry_buf[path_start..path_end]) {
            Ok(p) => p,
            Err(_) => {
                crate::serial_println!("[persist] Invalid UTF-8 path at sector {}", current_sector);
                current_sector += sectors_needed;
                continue;
            }
        };

        if file_type == 1 {
            // Directory
            crate::vfs::ensure_directory(path);
            restored_dirs += 1;
        } else {
            // Regular file
            let data_start = path_end;
            let data_end = data_start + data_len;
            let data = &entry_buf[data_start..data_end];

            // Ensure parent directory exists
            if let Some(last_slash) = path.rfind('/') {
                if last_slash > 0 {
                    crate::vfs::ensure_directory(&path[..last_slash]);
                }
            }

            let mut vfs = crate::vfs::VFS.lock();
            vfs.write_file(path, data);
            // Set permissions
            if let Some(ino) = vfs.resolve_path(path) {
                if let Some(inode) = vfs.get_inode_mut(ino) {
                    inode.permissions = permissions;
                }
            }
            drop(vfs);
            restored_files += 1;
        }

        current_sector += sectors_needed;
    }

    crate::serial_println!(
        "[persist] Restored {} files and {} directories from persistent storage",
        restored_files,
        restored_dirs
    );
}

/// Clear all persistent data (format the persistent store)
pub fn clear() {
    if !crate::virtio_blk::is_available() {
        return;
    }
    let mut header = [0u8; SECTOR_SIZE];
    header[0..12].copy_from_slice(MAGIC);
    header[12..16].copy_from_slice(&0u32.to_le_bytes());
    header[16..24].copy_from_slice(&1u64.to_le_bytes());
    crate::virtio_blk::write(0, 1, &header);
    crate::virtio_blk::flush();
    crate::serial_println!("[persist] Persistent storage cleared");
}

/// Compact the persistent store by re-writing only the latest version of each file.
/// This deduplicates entries (since we append updates without removing old entries).
pub fn compact() {
    if !crate::virtio_blk::is_available() {
        return;
    }

    // Collect all current file paths from the VFS that should be persisted
    let vfs = crate::vfs::VFS.lock();
    let mut files_to_persist: Vec<(String, Vec<u8>, u16)> = Vec::new();

    // Walk all inodes and collect persistable files
    for inode in &vfs.inodes {
        // We need the full path - reconstruct it
        // For simplicity, we'll re-persist from VFS after clearing
    }
    drop(vfs);

    // Clear and re-write
    clear();

    // Re-persist all files from VFS that match persist prefixes
    // We need to walk the VFS tree to get full paths
    let paths = collect_persistable_paths();
    for (path, data, perms) in paths {
        persist_file(&path, &data, perms);
    }

    crate::serial_println!("[persist] Compaction complete");
}

/// Collect all persistable file paths from the VFS
fn collect_persistable_paths() -> Vec<(String, Vec<u8>, u16)> {
    let vfs = crate::vfs::VFS.lock();
    let mut result = Vec::new();

    // Build a path map by walking from root
    fn walk(
        vfs: &crate::vfs::VirtualFS,
        ino: u64,
        current_path: &str,
        result: &mut Vec<(String, Vec<u8>, u16)>,
    ) {
        if let Some(inode) = vfs.get_inode(ino) {
            match inode.file_type {
                crate::vfs::FileType::Directory => {
                    for &child_ino in &inode.children {
                        if let Some(child) = vfs.get_inode(child_ino) {
                            let child_path = if current_path == "/" {
                                alloc::format!("/{}", child.name)
                            } else {
                                alloc::format!("{}/{}", current_path, child.name)
                            };
                            walk(vfs, child_ino, &child_path, result);
                        }
                    }
                }
                crate::vfs::FileType::Regular
                    if should_persist(current_path) && !inode.data.is_empty() =>
                {
                    result.push((
                        String::from(current_path),
                        inode.data.clone(),
                        inode.permissions,
                    ));
                }
                _ => {}
            }
        }
    }

    if let Some(root) = vfs.inodes.first() {
        walk(&vfs, root.ino, "/", &mut result);
    }

    result
}

/// Initialize the persistent storage system
pub fn init() {
    crate::serial_println!("[persist] Initializing persistent storage...");
    restore_all();
    let _ = roundtrip_self_test();
    let _ = journal_recovery_self_test();
    let _ = root_persist_self_test();
}

/// Serial marker the integration test waits for once a file has been written
/// to VirtIO-blk, dropped from the RAM VFS, and restored from disk.
pub const GATE_C1_MARKER: &str = "GATE_C1 persist complete";

const GATE_C1_PATH: &str = "/var/lib/knoxos/gate_c1";
const GATE_C1_PAYLOAD: &[u8] = b"knoxos-c1-alive\n";

/// Write a file to the persist blob store, drop it from the RAM VFS, then
/// restore from disk. That is the one-boot equivalent of "survives reboot":
/// the bytes come back from VirtIO-blk, not from the inode cache.
pub fn roundtrip_self_test() -> bool {
    if !crate::virtio_blk::is_available() {
        crate::serial_println!("[persist] Gate C1 skipped: no virtio-blk");
        return false;
    }

    // A previous boot already left the sentinel on disk.
    if let Some(existing) = crate::vfs::read_file_dispatch(GATE_C1_PATH) {
        if existing.as_slice() == GATE_C1_PAYLOAD {
            crate::serial_println!("[persist] {}", GATE_C1_MARKER);
            return true;
        }
    }

    if !crate::vfs::write_file_dispatch(GATE_C1_PATH, GATE_C1_PAYLOAD) {
        crate::serial_println!("[persist] Gate C1 FAILED: could not write {}", GATE_C1_PATH);
        return false;
    }

    if crate::vfs::remove_dispatch(GATE_C1_PATH).is_err() {
        crate::serial_println!("[persist] Gate C1 FAILED: unlink {}", GATE_C1_PATH);
        return false;
    }
    if crate::vfs::read_file_dispatch(GATE_C1_PATH).is_some() {
        crate::serial_println!("[persist] Gate C1 FAILED: RAM copy survived unlink");
        return false;
    }

    restore_all();

    match crate::vfs::read_file_dispatch(GATE_C1_PATH) {
        Some(data) if data.as_slice() == GATE_C1_PAYLOAD => {
            crate::serial_println!("[persist] {}", GATE_C1_MARKER);
            true
        }
        Some(data) => {
            crate::serial_println!(
                "[persist] Gate C1 FAILED: restored {} bytes, expected {}",
                data.len(),
                GATE_C1_PAYLOAD.len()
            );
            false
        }
        None => {
            crate::serial_println!("[persist] Gate C1 FAILED: file missing after restore");
            false
        }
    }
}

/// Serial marker once a committed WAL record is recovered after a simulated
/// crash (journal flushed, blob-store checkpoint skipped).
pub const GATE_C2_MARKER: &str = "GATE_C2 journal recovered";

const GATE_C2_PATH: &str = "/var/lib/knoxos/gate_c2";
const GATE_C2_PAYLOAD: &[u8] = b"knoxos-c2-alive\n";
const GATE_C2_UNCOMMITTED_PATH: &str = "/var/lib/knoxos/gate_c2_uncommitted";
const GATE_C2_UNCOMMITTED_PAYLOAD: &[u8] = b"should-not-survive\n";

/// Commit a journal record without checkpointing the blob store, drop the RAM
/// copy, then restore. That is the one-boot equivalent of `kill -9` after
/// `fsync` of the WAL and before the home-store write: replay must recover
/// committed data and must discard an uncommitted record.
pub fn journal_recovery_self_test() -> bool {
    if !crate::virtio_blk::is_available() {
        crate::serial_println!("[persist] Gate C2 skipped: no virtio-blk");
        return false;
    }
    if journal_base().is_none() {
        crate::serial_println!("[persist] Gate C2 FAILED: disk too small for journal");
        return false;
    }

    // Uncommitted (state=WRITING, no CRC-valid commit) must not appear.
    if !journal_write(
        GATE_C2_UNCOMMITTED_PATH,
        GATE_C2_UNCOMMITTED_PAYLOAD,
        0o644,
        0,
        JRNL_WRITING,
    ) {
        crate::serial_println!("[persist] Gate C2 FAILED: could not write uncommitted journal");
        return false;
    }
    crate::virtio_blk::flush();
    let _ = crate::vfs::remove_dispatch(GATE_C2_UNCOMMITTED_PATH);
    restore_all();
    if crate::vfs::read_file_dispatch(GATE_C2_UNCOMMITTED_PATH).is_some() {
        crate::serial_println!("[persist] Gate C2 FAILED: uncommitted journal was applied");
        return false;
    }

    // Crash window: WAL is committed and flushed; blob store is not updated.
    if !journal_write(GATE_C2_PATH, GATE_C2_PAYLOAD, 0o644, 0, JRNL_COMMITTED) {
        crate::serial_println!("[persist] Gate C2 FAILED: could not write committed journal");
        return false;
    }
    crate::virtio_blk::flush();
    let _ = crate::vfs::remove_dispatch(GATE_C2_PATH);
    if crate::vfs::read_file_dispatch(GATE_C2_PATH).is_some() {
        crate::serial_println!("[persist] Gate C2 FAILED: RAM copy survived unlink");
        return false;
    }

    restore_all();

    match crate::vfs::read_file_dispatch(GATE_C2_PATH) {
        Some(data) if data.as_slice() == GATE_C2_PAYLOAD => {
            crate::serial_println!("[persist] {}", GATE_C2_MARKER);
            true
        }
        Some(data) => {
            crate::serial_println!(
                "[persist] Gate C2 FAILED: recovered {} bytes, expected {}",
                data.len(),
                GATE_C2_PAYLOAD.len()
            );
            false
        }
        None => {
            crate::serial_println!("[persist] Gate C2 FAILED: file missing after journal replay");
            false
        }
    }
}

pub const GATE_C6_MARKER: &str = "GATE_C6 vfs persist";
const GATE_C6_PATH: &str = "/etc/knoxos/gate_c6";
const GATE_C6_PAYLOAD: &[u8] = b"knoxos-c6-root\n";

/// Persist a file **outside** the old `/home`/`/var` prefixes (the RAM VFS
/// root). Unlink, restore from VirtIO-blk, and require the bytes back.
pub fn root_persist_self_test() -> bool {
    if !crate::virtio_blk::is_available() {
        crate::serial_println!("[persist] Gate C6 skipped: no virtio-blk");
        return false;
    }
    if !should_persist(GATE_C6_PATH) {
        crate::serial_println!("[persist] Gate C6 FAILED: /etc is still skipped");
        return false;
    }

    crate::vfs::ensure_directory("/etc/knoxos");
    if !crate::vfs::write_file_dispatch(GATE_C6_PATH, GATE_C6_PAYLOAD) {
        crate::serial_println!("[persist] Gate C6 FAILED: could not write {}", GATE_C6_PATH);
        return false;
    }
    if crate::vfs::remove_dispatch(GATE_C6_PATH).is_err() {
        crate::serial_println!("[persist] Gate C6 FAILED: unlink {}", GATE_C6_PATH);
        return false;
    }
    if crate::vfs::read_file_dispatch(GATE_C6_PATH).is_some() {
        crate::serial_println!("[persist] Gate C6 FAILED: RAM copy survived unlink");
        return false;
    }

    restore_all();

    match crate::vfs::read_file_dispatch(GATE_C6_PATH) {
        Some(data) if data.as_slice() == GATE_C6_PAYLOAD => {
            crate::serial_println!("[persist] {}", GATE_C6_MARKER);
            true
        }
        Some(data) => {
            crate::serial_println!(
                "[persist] Gate C6 FAILED: restored {} bytes, expected {}",
                data.len(),
                GATE_C6_PAYLOAD.len()
            );
            false
        }
        None => {
            crate::serial_println!("[persist] Gate C6 FAILED: /etc file missing after restore");
            false
        }
    }
}
