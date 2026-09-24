//! File preview pane and properties dialog data.
use alloc::string::String;
use alloc::vec::Vec;

use crate::vfs::VFS;

use super::constants::*;
use super::links::readlink;
use super::stat::stat;

// ═══════════════════════════════════════════════════════════════════════
// FILE PREVIEW PANE — side panel with file info + preview
// ═══════════════════════════════════════════════════════════════════════

/// Preview information for a file
#[derive(Debug, Clone)]
pub struct FilePreview {
    pub path: String,
    pub file_type: String,
    pub size: u64,
    pub permissions: u32,
    pub owner_uid: u32,
    pub group_gid: u32,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
    pub mime_type: String,
    pub preview_text: Option<String>,
    pub is_image: bool,
    pub is_text: bool,
}

/// Generate a preview for a file
pub fn generate_preview(path: &str) -> Option<FilePreview> {
    let meta = stat(path).ok()?;
    let vfs = VFS.lock();
    let data = vfs.read_file(path)?;
    let data = data.to_vec();
    drop(vfs);

    let ext = path.rsplit('.').next().unwrap_or("");
    let (mime, is_image, is_text) = match ext {
        "txt" | "md" | "log" | "conf" | "cfg" => ("text/plain", false, true),
        "rs" | "c" | "h" | "py" | "js" | "ts" | "sh" => ("text/x-source", false, true),
        "html" | "htm" => ("text/html", false, true),
        "json" => ("application/json", false, true),
        "toml" | "yaml" | "yml" => ("text/x-config", false, true),
        "png" => ("image/png", true, false),
        "jpg" | "jpeg" => ("image/jpeg", true, false),
        "bmp" => ("image/bmp", true, false),
        "pdf" => ("application/pdf", false, false),
        "zip" => ("application/zip", false, false),
        _ => ("application/octet-stream", false, false),
    };

    let preview_text = if is_text && data.len() < 8192 {
        core::str::from_utf8(&data).ok().map(|s| {
            let lines: Vec<&str> = s.lines().take(50).collect();
            let mut out = String::new();
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
            out
        })
    } else {
        None
    };

    Some(FilePreview {
        path: String::from(path),
        file_type: String::from(if meta.st_mode & S_IFDIR != 0 {
            "directory"
        } else if meta.st_mode & S_IFLNK != 0 {
            "symlink"
        } else {
            "file"
        }),
        size: meta.st_size,
        permissions: meta.st_mode & 0o7777,
        owner_uid: meta.st_uid,
        group_gid: meta.st_gid,
        created: meta.st_ctime,
        modified: meta.st_mtime,
        accessed: meta.st_atime,
        mime_type: String::from(mime),
        preview_text,
        is_image,
        is_text,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// FILE PROPERTIES DIALOG — permissions, size, timestamps
// ═══════════════════════════════════════════════════════════════════════

/// Structured file properties for the dialog
#[derive(Debug, Clone)]
pub struct FileProperties {
    pub path: String,
    pub name: String,
    pub file_type: String,
    pub size: u64,
    pub size_human: String,
    pub permissions: u32,
    pub permissions_string: String,
    pub owner_uid: u32,
    pub group_gid: u32,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
    pub link_count: u32,
    pub inode: u64,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

/// Get full file properties
pub fn get_file_properties(path: &str) -> Option<FileProperties> {
    let meta = stat(path).ok()?;
    let name = path.rsplit('/').next().unwrap_or(path);

    let size = meta.st_size;
    let size_human = if size < 1024 {
        alloc::format!("{} B", size)
    } else if size < 1024 * 1024 {
        alloc::format!("{} KB", size / 1024)
    } else if size < 1024 * 1024 * 1024 {
        alloc::format!("{} MB", size / (1024 * 1024))
    } else {
        alloc::format!("{} GB", size / (1024 * 1024 * 1024))
    };

    let perms = meta.st_mode & 0o7777;
    let perm_str = alloc::format!(
        "{}{}{}{}{}{}{}{}{}",
        if perms & 0o400 != 0 { 'r' } else { '-' },
        if perms & 0o200 != 0 { 'w' } else { '-' },
        if perms & 0o100 != 0 { 'x' } else { '-' },
        if perms & 0o040 != 0 { 'r' } else { '-' },
        if perms & 0o020 != 0 { 'w' } else { '-' },
        if perms & 0o010 != 0 { 'x' } else { '-' },
        if perms & 0o004 != 0 { 'r' } else { '-' },
        if perms & 0o002 != 0 { 'w' } else { '-' },
        if perms & 0o001 != 0 { 'x' } else { '-' },
    );

    let is_symlink = meta.st_mode & S_IFLNK == S_IFLNK;
    let symlink_target = if is_symlink {
        readlink(path).ok()
    } else {
        None
    };

    let ft = if meta.st_mode & S_IFDIR != 0 {
        "Directory"
    } else if is_symlink {
        "Symbolic Link"
    } else if meta.st_mode & S_IFCHR != 0 {
        "Character Device"
    } else if meta.st_mode & S_IFBLK != 0 {
        "Block Device"
    } else if meta.st_mode & S_IFIFO != 0 {
        "FIFO"
    } else if meta.st_mode & S_IFSOCK != 0 {
        "Socket"
    } else {
        "Regular File"
    };

    Some(FileProperties {
        path: String::from(path),
        name: String::from(name),
        file_type: String::from(ft),
        size,
        size_human,
        permissions: perms,
        permissions_string: perm_str,
        owner_uid: meta.st_uid,
        group_gid: meta.st_gid,
        created: meta.st_ctime,
        modified: meta.st_mtime,
        accessed: meta.st_atime,
        link_count: meta.st_nlink,
        inode: meta.st_ino,
        is_symlink,
        symlink_target,
    })
}
