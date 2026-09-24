//! chmod/chown/utimes/touch and GUI permission wrappers.
use crate::path::{self};
use crate::vfs::VFS;

use super::constants::*;
use super::files::create;
use super::meta::{now, touch_ctime, update_meta};
use super::resolve::{resolve_path_follow, resolve_path_nofollow};

// ─── Permission operations ──────────────────────────────────────────

/// chmod() — change file permissions.
pub fn chmod(path: &str, mode: u16) -> Result<(), i32> {
    let (_, ino) = resolve_path_follow(path)?;
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    inode.permissions = mode;
    touch_ctime(ino);
    Ok(())
}

/// fchmod() — change permissions by inode.
pub fn fchmod(ino: u64, mode: u16) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    inode.permissions = mode;
    touch_ctime(ino);
    Ok(())
}

/// chmod_recursive() — chmod -R
pub fn chmod_recursive(path: &str, mode: u16) -> Result<u64, i32> {
    chmod(path, mode)?;
    let mut count = 1u64;

    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);
        let vfs = VFS.lock();
        let is_dir = vfs.list_dir(&child_path).is_some();
        drop(vfs);

        if is_dir {
            count += chmod_recursive(&child_path, mode)?;
        } else {
            chmod(&child_path, mode)?;
            count += 1;
        }
    }

    Ok(count)
}

/// chown() — change file ownership.
pub fn chown(path: &str, uid: u32, gid: u32) -> Result<(), i32> {
    let (_, ino) = resolve_path_follow(path)?;
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    if uid != u32::MAX {
        inode.uid = uid;
    }
    if gid != u32::MAX {
        inode.gid = gid;
    }
    touch_ctime(ino);
    Ok(())
}

/// lchown() — change ownership without following symlinks.
pub fn lchown(path: &str, uid: u32, gid: u32) -> Result<(), i32> {
    let (_, ino) = resolve_path_nofollow(path)?;
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    if uid != u32::MAX {
        inode.uid = uid;
    }
    if gid != u32::MAX {
        inode.gid = gid;
    }
    touch_ctime(ino);
    Ok(())
}

/// chown_recursive() — chown -R
pub fn chown_recursive(path: &str, uid: u32, gid: u32) -> Result<u64, i32> {
    chown(path, uid, gid)?;
    let mut count = 1u64;

    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);
        let vfs = VFS.lock();
        let is_dir = vfs.list_dir(&child_path).is_some();
        drop(vfs);

        if is_dir {
            count += chown_recursive(&child_path, uid, gid)?;
        } else {
            chown(&child_path, uid, gid)?;
            count += 1;
        }
    }

    Ok(count)
}

// ─── Timestamp operations ───────────────────────────────────────────

/// utimes() — set access and modification times.
pub fn utimes(path: &str, atime: u64, mtime: u64) -> Result<(), i32> {
    let (_, ino) = resolve_path_follow(path)?;
    update_meta(ino, |m| {
        m.atime = atime;
        m.mtime = mtime;
        m.ctime = now(); // ctime always updates
    });
    Ok(())
}

/// lutimes() — set times without following symlinks.
pub fn lutimes(path: &str, atime: u64, mtime: u64) -> Result<(), i32> {
    let (_, ino) = resolve_path_nofollow(path)?;
    update_meta(ino, |m| {
        m.atime = atime;
        m.mtime = mtime;
        m.ctime = now();
    });
    Ok(())
}

/// touch() — create file or update timestamps (like the `touch` command).
pub fn touch(path: &str) -> Result<(), i32> {
    let vfs = VFS.lock();
    if let Some(ino) = vfs.resolve_path(path) {
        // File exists — update all timestamps
        let t = now();
        drop(vfs);
        update_meta(ino, |m| {
            m.atime = t;
            m.mtime = t;
            m.ctime = t;
        });
        Ok(())
    } else {
        // File doesn't exist — create it
        drop(vfs);
        create(path, 0o644)?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PERMISSION EDITOR — chmod/chown GUI support
// ═══════════════════════════════════════════════════════════════════════

/// Apply permissions change (GUI-friendly wrapper)
pub fn set_permissions(path: &str, mode: u32) -> Result<(), i32> {
    chmod(path, mode as u16)
}

/// Change ownership (GUI wrapper)
pub fn set_ownership(path: &str, uid: u32, gid: u32) -> Result<(), i32> {
    chown(path, uid, gid)
}
