//! Recursive remove (`rm` / `rmdir`).
use crate::path::{self};
use crate::serial_println;
use crate::vfs::{FileType, VFS};

use super::constants::*;
use super::files::rmdir;
use super::links::unlink;

// ─── Recursive remove ───────────────────────────────────────────────

/// Remove options
#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    pub recursive: bool,
    pub force: bool,
    pub verbose: bool,
    /// Remove empty directories (like `rmdir`)
    pub dir: bool,
}

/// rm() — remove files and directories.
pub fn remove(path: &str, opts: &RemoveOptions) -> Result<u64, i32> {
    let vfs = VFS.lock();
    let ino = match vfs.resolve_path(path) {
        Some(ino) => ino,
        None => {
            if opts.force {
                return Ok(0);
            }
            return Err(ENOENT);
        }
    };
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;
    let is_dir = inode.file_type == FileType::Directory;
    drop(vfs);

    if is_dir {
        if !opts.recursive && !opts.dir {
            return Err(EISDIR);
        }
        remove_dir_recursive(path, opts)
    } else {
        unlink(path)?;
        if opts.verbose {
            serial_println!("removed '{}'", path);
        }
        Ok(1)
    }
}

fn remove_dir_recursive(path: &str, opts: &RemoveOptions) -> Result<u64, i32> {
    let mut count = 0u64;

    // Remove children first
    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);

        let vfs = VFS.lock();
        let is_dir = vfs.list_dir(&child_path).is_some();
        drop(vfs);

        if is_dir {
            count += remove_dir_recursive(&child_path, opts)?;
        } else {
            unlink(&child_path)?;
            count += 1;
            if opts.verbose {
                serial_println!("removed '{}'", child_path);
            }
        }
    }

    // Remove the directory itself
    rmdir(path)?;
    count += 1;
    if opts.verbose {
        serial_println!("removed directory '{}'", path);
    }

    Ok(count)
}
