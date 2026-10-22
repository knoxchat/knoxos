//! Recursive copy (`cp`) with GNU-style options.
use alloc::string::String;

use crate::path::{self};
use crate::serial_println;
use crate::vfs::{FileType, VFS};

use super::constants::*;
use super::files::{mkdir, read_file, write_file};
use super::links::{link, symlink};
use super::perms::{chmod, chown, utimes};
use super::stat::stat;

// ─── Recursive copy ─────────────────────────────────────────────────

/// Copy options
#[derive(Debug, Clone)]
pub struct CopyOptions {
    /// Recursive copy (directories)
    pub recursive: bool,
    /// Preserve permissions, timestamps, ownership
    pub preserve: bool,
    /// Follow symlinks in source
    pub dereference: bool,
    /// Force overwrite
    pub force: bool,
    /// Don't overwrite existing
    pub no_clobber: bool,
    /// Create hard links instead of copying
    pub link: bool,
    /// Create symlinks instead of copying
    pub symbolic_link: bool,
    /// Update only (copy if source is newer)
    pub update: bool,
    /// Verbose output
    pub verbose: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            preserve: false,
            dereference: true,
            force: false,
            no_clobber: false,
            link: false,
            symbolic_link: false,
            update: false,
            verbose: false,
        }
    }
}

/// cp() — copy file or directory.
pub fn copy(src: &str, dst: &str, opts: &CopyOptions) -> Result<u64, i32> {
    let vfs = VFS.lock();

    // Check if src exists
    let src_ino = vfs.resolve_path(src).ok_or(ENOENT)?;
    let src_inode = vfs.get_inode(src_ino).ok_or(ENOENT)?;
    let is_dir = src_inode.file_type == FileType::Directory;

    if is_dir && !opts.recursive {
        return Err(EISDIR); // cp: omitting directory without -r
    }

    drop(vfs);

    if is_dir {
        copy_dir_recursive(src, dst, opts)
    } else {
        copy_single_file(src, dst, opts)?;
        Ok(1)
    }
}

fn copy_single_file(src: &str, dst: &str, opts: &CopyOptions) -> Result<(), i32> {
    // Check if dst is a directory — if so, copy into it
    let final_dst = {
        let vfs = VFS.lock();
        if let Some(dst_ino) = vfs.resolve_path(dst) {
            if let Some(dst_inode) = vfs.get_inode(dst_ino) {
                if dst_inode.file_type == FileType::Directory {
                    let name = path::basename(src);
                    path::join(dst, name)
                } else {
                    String::from(dst)
                }
            } else {
                String::from(dst)
            }
        } else {
            String::from(dst)
        }
    };

    // Check no_clobber
    if opts.no_clobber {
        let vfs = VFS.lock();
        if vfs.resolve_path(&final_dst).is_some() {
            return Ok(()); // Silently skip
        }
    }

    if opts.link {
        return link(src, &final_dst);
    }

    if opts.symbolic_link {
        return symlink(src, &final_dst);
    }

    // Read source
    let data = read_file(src)?;

    // Write destination
    write_file(&final_dst, &data)?;

    // Preserve metadata
    if opts.preserve {
        if let Ok(st) = stat(src) {
            let _ = chmod(&final_dst, (st.st_mode & 0o7777) as u16);
            let _ = chown(&final_dst, st.st_uid, st.st_gid);
            let _ = utimes(&final_dst, st.st_atime, st.st_mtime);
        }
    }

    if opts.verbose {
        serial_println!("'{}' -> '{}'", src, final_dst);
    }

    Ok(())
}

fn copy_dir_recursive(src: &str, dst: &str, opts: &CopyOptions) -> Result<u64, i32> {
    let mut count = 0u64;

    // Create destination directory
    let vfs = VFS.lock();
    let src_ino = vfs.resolve_path(src).ok_or(ENOENT)?;
    let src_perms = vfs
        .get_inode(src_ino)
        .map(|i| i.permissions)
        .unwrap_or(0o755);
    drop(vfs);

    let _ = mkdir(dst, src_perms);
    count += 1;

    // Copy children
    let vfs = VFS.lock();
    let entries = vfs.list_dir(src).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let src_child = path::join(src, entry);
        let dst_child = path::join(dst, entry);

        let vfs = VFS.lock();
        let is_child_dir = vfs.list_dir(&src_child).is_some();
        drop(vfs);

        if is_child_dir {
            count += copy_dir_recursive(&src_child, &dst_child, opts)?;
        } else {
            copy_single_file(&src_child, &dst_child, opts)?;
            count += 1;
        }
    }

    Ok(count)
}
