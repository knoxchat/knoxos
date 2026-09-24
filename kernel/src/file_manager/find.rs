//! `find`-style recursive search with name/type/size/perm filters.
use alloc::string::String;
use alloc::vec::Vec;

use crate::path::{self};
use crate::vfs::{FileType, Inode, VFS};

use super::files::rmdir;
use super::links::unlink;

// ─── Find / search ──────────────────────────────────────────────────

/// Search criteria for find().
#[derive(Debug, Clone, Default)]
pub struct FindOptions {
    /// Filter by name glob pattern
    pub name: Option<String>,
    /// Filter by file type
    pub file_type: Option<FindType>,
    /// Max depth (0 = unlimited)
    pub max_depth: usize,
    /// Min depth
    pub min_depth: usize,
    /// Filter by minimum size (bytes)
    pub min_size: Option<u64>,
    /// Filter by maximum size (bytes)
    pub max_size: Option<u64>,
    /// Filter by permission bits
    pub perm: Option<u16>,
    /// Execute action on each match
    pub action: FindAction,
    /// Follow symlinks
    pub follow_symlinks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindType {
    File,
    Directory,
    Symlink,
    Pipe,
    Socket,
    CharDevice,
    BlockDevice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FindAction {
    #[default]
    Print,
    Print0,
    Delete,
    Count,
}

/// Result of a find operation.
pub struct FindResult {
    pub paths: Vec<String>,
    pub count: u64,
}

/// find() — search for files matching criteria.
pub fn find(start_path: &str, opts: &FindOptions) -> FindResult {
    let mut result = FindResult {
        paths: Vec::new(),
        count: 0,
    };

    find_recursive(start_path, opts, 0, &mut result);
    result
}

fn find_recursive(path: &str, opts: &FindOptions, depth: usize, result: &mut FindResult) {
    if opts.max_depth > 0 && depth > opts.max_depth {
        return;
    }

    let vfs = VFS.lock();
    let ino = match vfs.resolve_path(path) {
        Some(ino) => ino,
        None => return,
    };
    let inode = match vfs.get_inode(ino) {
        Some(i) => i.clone(),
        None => return,
    };
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    // Check if current path matches
    if depth >= opts.min_depth {
        let matches = check_find_match(path, &inode, opts);
        if matches {
            result.count += 1;
            match opts.action {
                FindAction::Print | FindAction::Print0 => {
                    result.paths.push(String::from(path));
                }
                FindAction::Delete => {
                    if inode.file_type == FileType::Directory {
                        let _ = rmdir(path);
                    } else {
                        let _ = unlink(path);
                    }
                    result.paths.push(String::from(path));
                }
                FindAction::Count => {} // Just count
            }
        }
    }

    // Recurse into subdirectories
    if inode.file_type == FileType::Directory {
        for entry in &entries {
            let child_path = path::join(path, entry);
            find_recursive(&child_path, opts, depth + 1, result);
        }
    }
}

fn check_find_match(path: &str, inode: &Inode, opts: &FindOptions) -> bool {
    // Check name pattern
    if let Some(ref pattern) = opts.name {
        let name = path::basename(path);
        if !path::glob_match(pattern, name) {
            return false;
        }
    }

    // Check file type
    if let Some(find_type) = opts.file_type {
        let matches = match find_type {
            FindType::File => inode.file_type == FileType::Regular,
            FindType::Directory => inode.file_type == FileType::Directory,
            FindType::Symlink => inode.file_type == FileType::SymLink,
            FindType::Pipe => inode.file_type == FileType::Pipe,
            FindType::Socket => inode.file_type == FileType::Socket,
            FindType::CharDevice => inode.file_type == FileType::CharDevice,
            FindType::BlockDevice => inode.file_type == FileType::BlockDevice,
        };
        if !matches {
            return false;
        }
    }

    // Check size
    if let Some(min) = opts.min_size {
        if inode.size < min {
            return false;
        }
    }
    if let Some(max) = opts.max_size {
        if inode.size > max {
            return false;
        }
    }

    // Check permissions
    if let Some(perm) = opts.perm {
        if inode.permissions & perm != perm {
            return false;
        }
    }

    true
}
