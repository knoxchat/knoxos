//! Directory and regular-file create/read/write/truncate/rename, plus temp files.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::path::{self};
use crate::serial_println;
use crate::vfs::{FileType, VFS};

use super::constants::*;
use super::meta::{INODE_META, InodeMeta, apply_umask, touch_atime, touch_mtime, update_meta};

// ─── Directory operations ───────────────────────────────────────────

/// mkdir() — create a directory.
pub fn mkdir(path: &str, mode: u16) -> Result<u64, i32> {
    let effective_mode = apply_umask(mode);
    let mut vfs = VFS.lock();
    let ino = vfs.mkdir(path, effective_mode)?;

    // Set up directory metadata
    let meta = InodeMeta::for_directory();
    INODE_META.lock().insert(ino, meta);

    // Update parent mtime
    let parent_dir = path::dirname(path);
    if let Some(parent_ino) = vfs.resolve_path(parent_dir) {
        touch_mtime(parent_ino);
    }

    serial_println!(
        "[file_manager] mkdir: {} (mode={:04o})",
        path,
        effective_mode
    );
    Ok(ino)
}

/// mkdirp() — create a directory and all parent directories (mkdir -p).
pub fn mkdirp(path: &str, mode: u16) -> Result<u64, i32> {
    let normalized = path::normalize_path(path);
    let parts: Vec<&str> = normalized
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut current_path = String::from("/");
    let mut last_ino = 0u64;

    for part in &parts {
        if current_path == "/" {
            current_path = alloc::format!("/{}", part);
        } else {
            current_path = alloc::format!("{}/{}", current_path, part);
        }

        let vfs = VFS.lock();
        if let Some(ino) = vfs.resolve_path(&current_path) {
            last_ino = ino;
            drop(vfs);
            continue;
        }
        drop(vfs);

        last_ino =
            mkdir(&current_path, mode).or_else(|e| if e == EEXIST { Ok(0) } else { Err(e) })?;
    }

    Ok(last_ino)
}

/// rmdir() — remove an empty directory.
pub fn rmdir(path: &str) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    vfs.rmdir(path)?;
    INODE_META.lock().remove(&0); // clean up (inode already removed by VFS)
    Ok(())
}

// ─── File creation and writing ──────────────────────────────────────

/// create() — create a new file or truncate an existing one.
pub fn create(path: &str, mode: u16) -> Result<u64, i32> {
    let effective_mode = apply_umask(mode);
    let mut vfs = VFS.lock();

    if let Some(ino) = vfs.resolve_path(path) {
        // File exists — truncate
        if let Some(inode) = vfs.inodes.iter_mut().find(|i| i.ino == ino) {
            if inode.file_type == FileType::Directory {
                return Err(EISDIR);
            }
            inode.data.clear();
            inode.size = 0;
            touch_mtime(ino);
            return Ok(ino);
        }
    }

    // Create new file
    let success = vfs.create_file_at_path(path, FileType::Regular, &[], effective_mode);
    if !success {
        return Err(EIO);
    }

    let ino = vfs.resolve_path(path).ok_or(EIO)?;
    let meta = InodeMeta::new();
    INODE_META.lock().insert(ino, meta);

    serial_println!(
        "[file_manager] create: {} (mode={:04o})",
        path,
        effective_mode
    );
    Ok(ino)
}

/// write_file() — write data to a file (create if needed).
pub fn write_file(path: &str, data: &[u8]) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    if vfs.write_file(path, data) {
        if let Some(ino) = vfs.resolve_path(path) {
            touch_mtime(ino);
            update_meta(ino, |m| {
                m.blocks = (data.len() as u64).div_ceil(512);
            });
        }
        Ok(())
    } else {
        Err(EIO)
    }
}

/// append_file() — append data to an existing file.
pub fn append_file(path: &str, data: &[u8]) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let existing = vfs.read_file(path).unwrap_or(&[]).to_vec();
    let mut combined = existing;
    combined.extend_from_slice(data);
    if vfs.write_file(path, &combined) {
        if let Some(ino) = vfs.resolve_path(path) {
            touch_mtime(ino);
        }
        Ok(())
    } else {
        Err(EIO)
    }
}

/// read_file() — read entire file contents.
pub fn read_file(path: &str) -> Result<Vec<u8>, i32> {
    let vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type == FileType::Directory {
        return Err(EISDIR);
    }

    touch_atime(ino);
    Ok(inode.data.clone())
}

/// truncate() — truncate a file to a specified length.
pub fn truncate(path: &str, length: u64) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;

    if let Some(inode) = vfs.inodes.iter_mut().find(|i| i.ino == ino) {
        if inode.file_type == FileType::Directory {
            return Err(EISDIR);
        }

        let len = length as usize;
        if len < inode.data.len() {
            inode.data.truncate(len);
        } else {
            inode.data.resize(len, 0); // extend with zeros (sparse)
        }
        inode.size = length;
        touch_mtime(ino);
        Ok(())
    } else {
        Err(ENOENT)
    }
}

// ─── Rename / move ──────────────────────────────────────────────────

/// rename() — atomically rename a file or directory.
/// Handles cross-directory moves within the same filesystem.
pub fn rename(oldpath: &str, newpath: &str) -> Result<(), i32> {
    let mut vfs = VFS.lock();

    // Check if newpath already exists — if so, remove it first
    if let Some(new_ino) = vfs.resolve_path(newpath) {
        if let Some(new_inode) = vfs.get_inode(new_ino) {
            if new_inode.file_type == FileType::Directory && !new_inode.children.is_empty() {
                return Err(ENOTEMPTY);
            }
        }
        // Remove the existing target
        let _ = vfs.unlink(newpath).or_else(|_| vfs.rmdir(newpath));
    }

    vfs.rename(oldpath, newpath)?;

    // Update timestamps
    let old_parent = path::dirname(oldpath);
    let new_parent = path::dirname(newpath);
    if let Some(ino) = vfs.resolve_path(old_parent) {
        touch_mtime(ino);
    }
    if let Some(ino) = vfs.resolve_path(new_parent) {
        touch_mtime(ino);
    }

    Ok(())
}

// ─── Temp file/directory creation ───────────────────────────────────

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create a temporary file in /tmp and return its path.
pub fn mktemp(template: Option<&str>) -> Result<String, i32> {
    let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = if let Some(tmpl) = template {
        if tmpl.contains("XXXXXX") {
            tmpl.replace("XXXXXX", &alloc::format!("{:06x}", count))
        } else {
            alloc::format!("{}.{:06x}", tmpl, count)
        }
    } else {
        alloc::format!("tmp.{:06x}", count)
    };

    let path = alloc::format!("/tmp/{}", name);
    create(&path, 0o600)?;
    Ok(path)
}

/// Create a temporary directory in /tmp and return its path.
pub fn mkdtemp(template: Option<&str>) -> Result<String, i32> {
    let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = if let Some(tmpl) = template {
        if tmpl.contains("XXXXXX") {
            tmpl.replace("XXXXXX", &alloc::format!("{:06x}", count))
        } else {
            alloc::format!("{}.{:06x}", tmpl, count)
        }
    } else {
        alloc::format!("tmpd.{:06x}", count)
    };

    let path = alloc::format!("/tmp/{}", name);
    mkdir(&path, 0o700)?;
    Ok(path)
}
