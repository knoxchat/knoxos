//! Symlink-aware path resolution, canonicalization, and type/access queries.
use alloc::string::String;
use alloc::vec::Vec;

use crate::path::{self};
use crate::vfs::{FileType, VFS};

use super::constants::*;
use super::meta::INODE_META;

/// Resolve a path, following symlinks up to SYMLOOP_MAX times.
/// Returns the resolved absolute path and the final inode number.
pub fn resolve_path_follow(path_str: &str) -> Result<(String, u64), i32> {
    resolve_path_inner(path_str, true, 0)
}

/// Resolve a path, NOT following the final symlink (lstat behavior).
pub fn resolve_path_nofollow(path_str: &str) -> Result<(String, u64), i32> {
    resolve_path_inner(path_str, false, 0)
}

fn resolve_path_inner(
    path_str: &str,
    follow_final: bool,
    depth: usize,
) -> Result<(String, u64), i32> {
    if depth > SYMLOOP_MAX {
        return Err(ELOOP);
    }

    if !crate::path::is_valid_path(path_str) {
        return Err(ENAMETOOLONG);
    }

    let vfs = VFS.lock();
    let normalized = path::normalize_path(path_str);
    let parts: Vec<&str> = normalized
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        // Root
        if let Some(root) = vfs.inodes.first() {
            return Ok((String::from("/"), root.ino));
        }
        return Err(ENOENT);
    }

    let mut current_ino = vfs.inodes.first().map(|i| i.ino).ok_or(ENOENT)?;
    let mut resolved = String::from("/");

    for (idx, part) in parts.iter().enumerate() {
        let is_last = idx == parts.len() - 1;

        // Look up the child
        let current = vfs
            .inodes
            .iter()
            .find(|i| i.ino == current_ino)
            .ok_or(ENOENT)?;

        if current.file_type != FileType::Directory {
            return Err(ENOTDIR);
        }

        let mut found = false;
        for &child_ino in &current.children {
            if let Some(child) = vfs.inodes.iter().find(|i| i.ino == child_ino) {
                if child.name == *part {
                    // Check if this is a symlink
                    if child.file_type == FileType::SymLink {
                        let meta_store = INODE_META.lock();
                        if let Some(meta) = meta_store.get(&child_ino) {
                            if let Some(ref target) = meta.symlink_target {
                                if !is_last || follow_final {
                                    // Follow the symlink
                                    let target_path = if target.starts_with('/') {
                                        target.clone()
                                    } else {
                                        path::join(&resolved, target)
                                    };
                                    drop(meta_store);
                                    drop(vfs);

                                    // Build remaining path
                                    let remaining: String = if is_last {
                                        String::new()
                                    } else {
                                        let rest = &parts[idx + 1..];
                                        let mut r = String::new();
                                        for p in rest {
                                            r.push('/');
                                            r.push_str(p);
                                        }
                                        r
                                    };
                                    let full_target =
                                        alloc::format!("{}{}", target_path, remaining);
                                    return resolve_path_inner(
                                        &full_target,
                                        follow_final,
                                        depth + 1,
                                    );
                                }
                            }
                        }
                    }

                    // Not a symlink or not following → use this inode
                    current_ino = child_ino;
                    if (!is_last || resolved != "/") && !resolved.ends_with('/') {
                        resolved.push('/');
                    }
                    if resolved == "/" {
                        resolved = alloc::format!("/{}", part);
                    } else {
                        resolved.push_str(part);
                    }
                    found = true;
                    break;
                }
            }
        }

        if !found {
            return Err(ENOENT);
        }
    }

    Ok((resolved, current_ino))
}

// ─── Path canonicalization ──────────────────────────────────────────

/// Canonicalize a path — resolve all symlinks and return an absolute path.
/// Equivalent to `realpath(3)`.
pub fn canonicalize(path: &str) -> Result<String, i32> {
    let (resolved, _) = resolve_path_follow(path)?;
    Ok(path::normalize_path(&resolved))
}

/// Canonicalize without requiring the final component to exist.
/// Equivalent to `realpath -m`.
pub fn canonicalize_missing(path: &str) -> String {
    match resolve_path_follow(path) {
        Ok((resolved, _)) => path::normalize_path(&resolved),
        Err(_) => {
            // Resolve as much as possible, keep the rest
            let normalized = path::normalize_path(path);
            if normalized.starts_with('/') {
                normalized
            } else {
                let cwd = crate::shell::env::ENV_VARS
                    .lock()
                    .get("PWD")
                    .cloned()
                    .unwrap_or_else(|| String::from("/"));
                path::normalize_path(&path::join(&cwd, &normalized))
            }
        }
    }
}

// ─── Access checks ──────────────────────────────────────────────────

/// access() — check file accessibility.
/// `mode`: 0=F_OK (exists), 1=X_OK, 2=W_OK, 4=R_OK (combinable).
pub fn access(path: &str, mode: u32) -> Result<(), i32> {
    let vfs = VFS.lock();
    vfs.access(path, mode)
}

/// Test if a path exists.
pub fn exists(path: &str) -> bool {
    VFS.lock().resolve_path(path).is_some()
}

/// Test if a path is a directory.
pub fn is_dir(path: &str) -> bool {
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.file_type == FileType::Directory)
        .unwrap_or(false)
}

/// Test if a path is a regular file.
pub fn is_file(path: &str) -> bool {
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.file_type == FileType::Regular)
        .unwrap_or(false)
}

/// Test if a path is a symlink.
pub fn is_symlink(path: &str) -> bool {
    // Use nofollow to detect the symlink itself
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.file_type == FileType::SymLink)
        .unwrap_or(false)
}

/// Test if a file is executable.
pub fn is_executable(path: &str) -> bool {
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.permissions & 0o111 != 0)
        .unwrap_or(false)
}
