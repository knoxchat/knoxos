//! Symbolic and hard link operations.
use alloc::string::String;

use crate::path::{self};
use crate::serial_println;
use crate::vfs::{FileType, VFS};

use super::constants::*;
use super::meta::{
    INODE_META, InodeMeta, get_or_create_meta, touch_atime, touch_ctime, touch_mtime, update_meta,
};
use super::resolve::{resolve_path_follow, resolve_path_nofollow};

/// symlink() — create a symbolic link.
/// `target` is the path the symlink points to (can be relative or absolute).
/// `linkpath` is where the symlink itself is created.
pub fn symlink(target: &str, linkpath: &str) -> Result<(), i32> {
    if target.is_empty() || linkpath.is_empty() {
        return Err(EINVAL);
    }
    if !path::is_valid_path(linkpath) {
        return Err(ENAMETOOLONG);
    }

    let normalized = path::normalize_path(linkpath);
    let parent_dir = path::dirname(&normalized);
    let link_name = path::basename(&normalized);

    let mut vfs = VFS.lock();
    let parent_ino = vfs.resolve_path(parent_dir).ok_or(ENOENT)?;

    // Check parent is a directory
    {
        let parent = vfs.get_inode(parent_ino).ok_or(ENOENT)?;
        if parent.file_type != FileType::Directory {
            return Err(ENOTDIR);
        }
    }

    // Check link doesn't already exist
    if vfs.resolve_path(&normalized).is_some() {
        return Err(EEXIST);
    }

    // Create the symlink inode (store target path as data)
    let ino = vfs.create_file_under(
        parent_ino,
        link_name,
        FileType::SymLink,
        target.as_bytes(),
        0o777,
    );

    // Store symlink metadata
    let meta = InodeMeta::for_symlink(target);
    INODE_META.lock().insert(ino, meta);

    touch_mtime(parent_ino);

    serial_println!("[file_manager] symlink: {} -> {}", linkpath, target);
    Ok(())
}

/// readlink() — read the target of a symbolic link.
pub fn readlink(path: &str) -> Result<String, i32> {
    let (_, ino) = resolve_path_nofollow(path)?;

    let vfs = VFS.lock();
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;
    if inode.file_type != FileType::SymLink {
        return Err(EINVAL);
    }

    // Try metadata first
    let meta_store = INODE_META.lock();
    if let Some(meta) = meta_store.get(&ino) {
        if let Some(ref target) = meta.symlink_target {
            touch_atime(ino);
            return Ok(target.clone());
        }
    }
    drop(meta_store);

    // Fall back to inode data
    if let Ok(s) = core::str::from_utf8(&inode.data) {
        touch_atime(ino);
        Ok(String::from(s))
    } else {
        Err(EINVAL)
    }
}

// ─── Hard link operations ───────────────────────────────────────────

/// link() — create a hard link.
/// Creates a new directory entry `newpath` pointing to the same inode as `oldpath`.
pub fn link(oldpath: &str, newpath: &str) -> Result<(), i32> {
    let (_, old_ino) = resolve_path_follow(oldpath)?;

    let vfs = VFS.lock();
    let old_inode = vfs.get_inode(old_ino).ok_or(ENOENT)?;

    // Can't hard-link directories (Linux restriction)
    if old_inode.file_type == FileType::Directory {
        return Err(EPERM);
    }

    // Check link count
    let meta = get_or_create_meta(old_ino);
    if meta.nlink >= LINK_MAX {
        return Err(EMLINK);
    }

    drop(vfs);

    // Resolve the new path's parent
    let normalized = path::normalize_path(newpath);
    let parent_dir = path::dirname(&normalized);
    let new_name = path::basename(&normalized);

    let mut vfs = VFS.lock();
    let parent_ino = vfs.resolve_path(parent_dir).ok_or(ENOENT)?;

    // Check parent is a directory
    {
        let parent = vfs.get_inode(parent_ino).ok_or(ENOENT)?;
        if parent.file_type != FileType::Directory {
            return Err(ENOTDIR);
        }
    }

    // Check new name doesn't exist
    if vfs.resolve_path(&normalized).is_some() {
        return Err(EEXIST);
    }

    // For a true hard link, we add the old inode to the new parent's children
    // and update the inode's name for the new entry.
    // In our VFS model, we simulate by creating a copy that shares the same data.
    let old_data = vfs
        .get_inode(old_ino)
        .map(|i| i.data.clone())
        .ok_or(ENOENT)?;
    let old_perms = vfs
        .get_inode(old_ino)
        .map(|i| i.permissions)
        .ok_or(ENOENT)?;

    // Add old_ino to new parent's children list
    if let Some(parent) = vfs.inodes.iter_mut().find(|i| i.ino == parent_ino) {
        parent.children.push(old_ino);
    }

    // Increment link count
    update_meta(old_ino, |m| m.nlink += 1);
    touch_ctime(old_ino);
    touch_mtime(parent_ino);

    serial_println!(
        "[file_manager] link: {} -> {} (ino={})",
        newpath,
        oldpath,
        old_ino
    );
    Ok(())
}

/// unlink() — remove a directory entry and possibly the inode.
pub fn unlink(path: &str) -> Result<(), i32> {
    let (resolved, ino) = resolve_path_nofollow(path)?;

    let vfs = VFS.lock();
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type == FileType::Directory {
        return Err(EISDIR);
    }

    drop(vfs);

    let parent_dir = path::dirname(&resolved);
    let mut vfs = VFS.lock();
    let parent_ino = vfs.resolve_path(parent_dir).ok_or(ENOENT)?;

    // Remove from parent
    if let Some(parent) = vfs.inodes.iter_mut().find(|i| i.ino == parent_ino) {
        parent.children.retain(|&c| c != ino);
    }

    // Decrement link count
    let should_remove = {
        let mut store = INODE_META.lock();
        if let Some(meta) = store.get_mut(&ino) {
            meta.nlink = meta.nlink.saturating_sub(1);
            meta.nlink == 0
        } else {
            true // No metadata means single link
        }
    };

    if should_remove {
        vfs.inodes.retain(|i| i.ino != ino);
        INODE_META.lock().remove(&ino);
    }

    touch_mtime(parent_ino);
    Ok(())
}
