//! Disk usage accounting and human-readable size formatting.
use alloc::string::String;

use crate::path::{self};
use crate::vfs::{FileType, VFS};

use super::constants::*;

// ─── Disk usage ─────────────────────────────────────────────────────

/// Calculate disk usage of a file or directory (in bytes).
pub fn disk_usage(path: &str) -> Result<DiskUsage, i32> {
    let vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type != FileType::Directory {
        return Ok(DiskUsage {
            bytes: inode.size,
            files: 1,
            directories: 0,
            blocks: inode.size.div_ceil(4096),
        });
    }

    drop(vfs);
    disk_usage_recursive(path)
}

fn disk_usage_recursive(path: &str) -> Result<DiskUsage, i32> {
    let mut usage = DiskUsage {
        bytes: 4096, // Directory entry overhead
        files: 0,
        directories: 1,
        blocks: 1,
    };

    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);

        let vfs = VFS.lock();
        let child_ino = vfs.resolve_path(&child_path);
        let is_dir = vfs.list_dir(&child_path).is_some();
        let file_size = child_ino
            .and_then(|ino| vfs.get_inode(ino))
            .map(|i| i.size)
            .unwrap_or(0);
        drop(vfs);

        if is_dir {
            let sub = disk_usage_recursive(&child_path)?;
            usage.bytes += sub.bytes;
            usage.files += sub.files;
            usage.directories += sub.directories;
            usage.blocks += sub.blocks;
        } else {
            usage.bytes += file_size;
            usage.files += 1;
            usage.blocks += file_size.div_ceil(4096);
        }
    }

    Ok(usage)
}

/// Disk usage result.
#[derive(Debug, Clone)]
pub struct DiskUsage {
    pub bytes: u64,
    pub files: u64,
    pub directories: u64,
    pub blocks: u64,
}

impl DiskUsage {
    pub fn display_human(&self) -> String {
        let size = format_size_human(self.bytes);
        alloc::format!(
            "{} total ({} files, {} directories, {} blocks)",
            size,
            self.files,
            self.directories,
            self.blocks
        )
    }
}

/// Format bytes as human-readable size.
pub fn format_size_human(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        let gb = bytes / (1024 * 1024 * 1024);
        let frac = (bytes % (1024 * 1024 * 1024)) * 10 / (1024 * 1024 * 1024);
        alloc::format!("{}.{}G", gb, frac)
    } else if bytes >= 1024 * 1024 {
        let mb = bytes / (1024 * 1024);
        let frac = (bytes % (1024 * 1024)) * 10 / (1024 * 1024);
        alloc::format!("{}.{}M", mb, frac)
    } else if bytes >= 1024 {
        let kb = bytes / 1024;
        let frac = (bytes % 1024) * 10 / 1024;
        alloc::format!("{}.{}K", kb, frac)
    } else {
        alloc::format!("{}B", bytes)
    }
}
