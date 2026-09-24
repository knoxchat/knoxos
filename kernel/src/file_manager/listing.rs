//! Directory listing with metadata, sort orders, and tree walk.
use alloc::string::String;
use alloc::vec::Vec;

use crate::path::{self};
use crate::vfs::{FileType, VFS};

use super::constants::*;
use super::meta::{get_or_create_meta, touch_atime};

// ─── Directory listing with full metadata ───────────────────────────

/// Directory entry with full metadata (like `readdir` + `stat`).
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub ino: u64,
    pub file_type: FileType,
    pub size: u64,
    pub permissions: u16,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub mtime: u64,
    pub atime: u64,
    pub ctime: u64,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

/// List directory entries with full metadata.
pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, i32> {
    let vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type != FileType::Directory {
        return Err(ENOTDIR);
    }

    touch_atime(ino);

    let mut entries = Vec::new();

    for &child_ino in &inode.children {
        if let Some(child) = vfs.get_inode(child_ino) {
            let meta = get_or_create_meta(child_ino);
            entries.push(DirEntry {
                name: child.name.clone(),
                ino: child_ino,
                file_type: child.file_type,
                size: child.size,
                permissions: child.permissions,
                uid: child.uid,
                gid: child.gid,
                nlink: meta.nlink,
                mtime: meta.mtime,
                atime: meta.atime,
                ctime: meta.ctime,
                is_symlink: child.file_type == FileType::SymLink,
                symlink_target: meta.symlink_target.clone(),
            });
        }
    }

    Ok(entries)
}

/// List directory entries sorted by various criteria.
pub fn list_dir_sorted(path: &str, sort: SortOrder, reverse: bool) -> Result<Vec<DirEntry>, i32> {
    let mut entries = list_dir(path)?;

    match sort {
        SortOrder::Name => {
            entries.sort_by(|a, b| {
                let a_dir = a.file_type == FileType::Directory;
                let b_dir = b.file_type == FileType::Directory;
                match (a_dir, b_dir) {
                    (true, false) => core::cmp::Ordering::Less,
                    (false, true) => core::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });
        }
        SortOrder::Size => entries.sort_by_key(|e| e.size),
        SortOrder::Time => entries.sort_by_key(|e| e.mtime),
        SortOrder::Extension => {
            entries.sort_by(|a, b| {
                let a_path = path::Path::new(&a.name);
                let b_path = path::Path::new(&b.name);
                let a_ext = a_path.extension().unwrap_or("");
                let b_ext = b_path.extension().unwrap_or("");
                a_ext.cmp(b_ext)
            });
        }
        SortOrder::Inode => entries.sort_by_key(|e| e.ino),
        SortOrder::None => {} // Keep filesystem order
    }

    if reverse {
        entries.reverse();
    }

    Ok(entries)
}

/// Sort order for directory listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Name,
    Size,
    Time,
    Extension,
    Inode,
    None,
}

// ─── Walk / traverse ────────────────────────────────────────────────

/// Walk a directory tree, calling `callback` for each entry.
/// Like Python's `os.walk()`.
pub fn walk<F>(root: &str, mut callback: F) -> Result<(), i32>
where
    F: FnMut(&str, &[String], &[String]),
{
    walk_inner(root, &mut callback)
}

fn walk_inner<F>(dir: &str, callback: &mut F) -> Result<(), i32>
where
    F: FnMut(&str, &[String], &[String]),
{
    let vfs = VFS.lock();
    let entries = vfs.list_dir(dir).ok_or(ENOENT)?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in &entries {
        let child = path::join(dir, entry);
        if vfs.list_dir(&child).is_some() {
            dirs.push(entry.clone());
        } else {
            files.push(entry.clone());
        }
    }
    drop(vfs);

    callback(dir, &dirs, &files);

    for subdir in &dirs {
        let child = path::join(dir, subdir);
        walk_inner(&child, callback)?;
    }

    Ok(())
}
