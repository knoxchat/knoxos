/// Persistent storage layer for KnoxOS VFS
///
/// Uses the virtio-blk device to persist user files across reboots.
/// Format: Simple header + file entries written sequentially.
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
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const SECTOR_SIZE: usize = 512;
const MAGIC: &[u8; 12] = b"KNOXPERSIST\0";
/// Maximum file size to persist (files larger than this are kept in-memory only)
/// This prevents OOM when persisting very large files like 125MB .deb packages
const MAX_PERSIST_SIZE: usize = 64 * 1024 * 1024; // 64 MB

/// Paths that should be persisted (user data directories)
const PERSIST_PREFIXES: &[&str] = &["/home/", "/root/", "/tmp/", "/var/", "/opt/", "/usr/local/"];

/// Paths that should NOT be persisted (system-generated)
const SKIP_PREFIXES: &[&str] = &[
    "/dev/",
    "/proc/",
    "/sys/",
    "/bin/",
    "/sbin/",
    "/etc/hostname",
    "/etc/os-release",
    "/etc/passwd",
];

/// Check if a path should be persisted
fn should_persist(path: &str) -> bool {
    // Skip system paths
    for skip in SKIP_PREFIXES {
        if path.starts_with(skip) {
            return false;
        }
    }
    // Only persist user-data paths
    for prefix in PERSIST_PREFIXES {
        if path.starts_with(prefix) {
            return true;
        }
    }
    false
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

    // Read the current header to find where to append
    let mut header_buf = [0u8; SECTOR_SIZE];
    if !crate::virtio_blk::read(0, 1, &mut header_buf) {
        return;
    }

    let (entry_count, next_sector) = if &header_buf[0..12] == MAGIC {
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
        (count, next)
    } else {
        // Initialize fresh header
        (0u32, 1u64)
    };

    // Build the entry
    let path_bytes = path.as_bytes();
    let entry_header_size = 4 + 4 + 2 + 2; // path_len + data_len + perms + type
    let entry_size = entry_header_size + path_bytes.len() + data.len();
    let sectors_needed = entry_size.div_ceil(SECTOR_SIZE) as u64;

    // Check capacity
    let capacity = crate::virtio_blk::capacity();
    if next_sector + sectors_needed > capacity {
        crate::serial_println!("[persist] Disk full, cannot persist {}", path);
        return;
    }

    // Build entry buffer (padded to sector boundary)
    let buf_size = (sectors_needed as usize) * SECTOR_SIZE;
    let mut entry_buf = vec![0u8; buf_size];
    let mut offset = 0;

    // Path length
    let path_len = path_bytes.len() as u32;
    entry_buf[offset..offset + 4].copy_from_slice(&path_len.to_le_bytes());
    offset += 4;

    // Data length
    let data_len = data.len() as u32;
    entry_buf[offset..offset + 4].copy_from_slice(&data_len.to_le_bytes());
    offset += 4;

    // Permissions
    entry_buf[offset..offset + 2].copy_from_slice(&permissions.to_le_bytes());
    offset += 2;

    // File type (0 = Regular)
    entry_buf[offset..offset + 2].copy_from_slice(&0u16.to_le_bytes());
    offset += 2;

    // Path
    entry_buf[offset..offset + path_bytes.len()].copy_from_slice(path_bytes);
    offset += path_bytes.len();

    // Data
    entry_buf[offset..offset + data.len()].copy_from_slice(data);

    // Write entry to disk
    if !crate::virtio_blk::write(next_sector, sectors_needed as usize, &entry_buf) {
        crate::serial_println!("[persist] Failed to write entry for {}", path);
        return;
    }

    // Update header
    let new_count = entry_count + 1;
    let new_next = next_sector + sectors_needed;
    let mut new_header = [0u8; SECTOR_SIZE];
    new_header[0..12].copy_from_slice(MAGIC);
    new_header[12..16].copy_from_slice(&new_count.to_le_bytes());
    new_header[16..24].copy_from_slice(&new_next.to_le_bytes());

    if !crate::virtio_blk::write(0, 1, &new_header) {
        crate::serial_println!("[persist] Failed to update header");
        return;
    }

    // Flush to ensure data is on disk
    crate::virtio_blk::flush();

    crate::serial_println!(
        "[persist] Saved {} ({} bytes, {} sectors at sector {})",
        path,
        data.len(),
        sectors_needed,
        next_sector
    );
}

/// Persist a directory entry
pub fn persist_directory(path: &str, permissions: u16) {
    if !should_persist(path) {
        return;
    }
    if !crate::virtio_blk::is_available() {
        return;
    }

    let mut header_buf = [0u8; SECTOR_SIZE];
    if !crate::virtio_blk::read(0, 1, &mut header_buf) {
        return;
    }

    let (entry_count, next_sector) = if &header_buf[0..12] == MAGIC {
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
        (count, next)
    } else {
        (0u32, 1u64)
    };

    let path_bytes = path.as_bytes();
    let entry_header_size = 4 + 4 + 2 + 2;
    let entry_size = entry_header_size + path_bytes.len();
    let sectors_needed = entry_size.div_ceil(SECTOR_SIZE) as u64;

    let capacity = crate::virtio_blk::capacity();
    if next_sector + sectors_needed > capacity {
        return;
    }

    let buf_size = (sectors_needed as usize) * SECTOR_SIZE;
    let mut entry_buf = vec![0u8; buf_size];
    let mut offset = 0;

    let path_len = path_bytes.len() as u32;
    entry_buf[offset..offset + 4].copy_from_slice(&path_len.to_le_bytes());
    offset += 4;

    // Data length = 0 for directories
    entry_buf[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
    offset += 4;

    entry_buf[offset..offset + 2].copy_from_slice(&permissions.to_le_bytes());
    offset += 2;

    // File type 1 = Directory
    entry_buf[offset..offset + 2].copy_from_slice(&1u16.to_le_bytes());
    offset += 2;

    entry_buf[offset..offset + path_bytes.len()].copy_from_slice(path_bytes);

    if !crate::virtio_blk::write(next_sector, sectors_needed as usize, &entry_buf) {
        return;
    }

    let new_count = entry_count + 1;
    let new_next = next_sector + sectors_needed;
    let mut new_header = [0u8; SECTOR_SIZE];
    new_header[0..12].copy_from_slice(MAGIC);
    new_header[12..16].copy_from_slice(&new_count.to_le_bytes());
    new_header[16..24].copy_from_slice(&new_next.to_le_bytes());

    if !crate::virtio_blk::write(0, 1, &new_header) {
        return;
    }

    crate::virtio_blk::flush();
}

/// Restore all persisted files into the VFS on boot
pub fn restore_all() {
    if !crate::virtio_blk::is_available() {
        crate::serial_println!("[persist] No virtio-blk device, skipping restore");
        return;
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
}
