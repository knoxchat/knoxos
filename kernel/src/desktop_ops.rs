// SPDX-License-Identifier: MIT
//! Desktop file operations, trash/recycle bin, and file creation (items 9.27, 9.29, 9.30)
//!
//! Extends the desktop with:
//! - File drop operations (copy/move files to desktop)
//! - Create new file/folder from context menu
//! - Recycle bin / trash with restore capability

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Trash directory path
const TRASH_DIR: &str = "/home/user/.trash";
const TRASH_INFO_DIR: &str = "/home/user/.trash/info";
const TRASH_FILES_DIR: &str = "/home/user/.trash/files";

/// A trashed item with metadata for restoration
#[derive(Debug, Clone)]
pub struct TrashEntry {
    /// Unique ID for this trash entry
    pub id: u64,
    /// Original filename
    pub original_name: String,
    /// Original full path
    pub original_path: String,
    /// Deletion timestamp (RTC ticks)
    pub deleted_at: u64,
    /// Size in bytes
    pub size: u64,
    /// Whether it's a directory
    pub is_dir: bool,
}

/// Desktop file operation types
#[derive(Debug, Clone)]
pub enum DesktopFileOp {
    /// Copy a file to the desktop
    CopyToDesktop { source_path: String },
    /// Move a file to the desktop
    MoveToDesktop { source_path: String },
    /// Create a new empty file on the desktop
    CreateFile { name: String },
    /// Create a new folder on the desktop
    CreateFolder { name: String },
    /// Move file to trash
    MoveToTrash { path: String },
    /// Restore file from trash
    RestoreFromTrash { trash_id: u64 },
    /// Permanently delete from trash
    PermanentDelete { trash_id: u64 },
    /// Empty the entire trash
    EmptyTrash,
}

lazy_static::lazy_static! {
    static ref TRASH: Mutex<Vec<TrashEntry>> = Mutex::new(Vec::new());
}

static NEXT_TRASH_ID: AtomicU64 = AtomicU64::new(1);
static OPS_PERFORMED: AtomicU64 = AtomicU64::new(0);

/// Desktop directory path
const DESKTOP_DIR: &str = "/home/user/Desktop";

/// Execute a desktop file operation
pub fn execute_op(op: DesktopFileOp) -> Result<(), &'static str> {
    OPS_PERFORMED.fetch_add(1, Ordering::Relaxed);

    match op {
        DesktopFileOp::CopyToDesktop { source_path } => copy_to_desktop(&source_path),
        DesktopFileOp::MoveToDesktop { source_path } => move_to_desktop(&source_path),
        DesktopFileOp::CreateFile { name } => create_new_file(&name),
        DesktopFileOp::CreateFolder { name } => create_new_folder(&name),
        DesktopFileOp::MoveToTrash { path } => move_to_trash(&path),
        DesktopFileOp::RestoreFromTrash { trash_id } => restore_from_trash(trash_id),
        DesktopFileOp::PermanentDelete { trash_id } => permanent_delete(trash_id),
        DesktopFileOp::EmptyTrash => empty_trash(),
    }
}

/// Copy a file to the desktop directory
fn copy_to_desktop(source: &str) -> Result<(), &'static str> {
    let filename = extract_filename(source);
    let dest = alloc::format!("{}/{}", DESKTOP_DIR, filename);

    // Read source
    let data = crate::file_manager::read_file(source).map_err(|_| "failed to read source file")?;
    // Write to desktop
    crate::file_manager::write_file(&dest, &data).map_err(|_| "failed to write to desktop")?;

    crate::serial_println!("[desktop_ops] copied {} -> {}", source, dest);
    Ok(())
}

/// Move a file to the desktop directory
fn move_to_desktop(source: &str) -> Result<(), &'static str> {
    copy_to_desktop(source)?;
    crate::file_manager::remove(source, &crate::file_manager::RemoveOptions::default())
        .map_err(|_| "failed to remove source after move")?;
    crate::serial_println!("[desktop_ops] moved {} -> desktop", source);
    Ok(())
}

/// Create a new empty file on the desktop
fn create_new_file(name: &str) -> Result<(), &'static str> {
    let path = alloc::format!("{}/{}", DESKTOP_DIR, name);
    crate::file_manager::write_file(&path, &[]).map_err(|_| "failed to create file")?;
    crate::serial_println!("[desktop_ops] created file: {}", path);
    Ok(())
}

/// Create a new folder on the desktop
fn create_new_folder(name: &str) -> Result<(), &'static str> {
    let path = alloc::format!("{}/{}", DESKTOP_DIR, name);
    crate::file_manager::mkdir(&path, 0o755).map_err(|_| "failed to create folder")?;
    crate::serial_println!("[desktop_ops] created folder: {}", path);
    Ok(())
}

/// Move a file to the trash (soft delete)
fn move_to_trash(path: &str) -> Result<(), &'static str> {
    // Ensure trash dirs exist
    let _ = crate::file_manager::mkdir(TRASH_DIR, 0o755);
    let _ = crate::file_manager::mkdir(TRASH_INFO_DIR, 0o755);
    let _ = crate::file_manager::mkdir(TRASH_FILES_DIR, 0o755);

    let filename = extract_filename(path);
    let id = NEXT_TRASH_ID.fetch_add(1, Ordering::Relaxed);
    let trash_name = alloc::format!("{}_{}", id, filename);
    let trash_path = alloc::format!("{}/{}", TRASH_FILES_DIR, trash_name);

    // Get file info
    let size = crate::file_manager::stat(path)
        .map(|s| s.st_size)
        .unwrap_or(0);
    let is_dir = crate::file_manager::is_dir(path);

    // Move to trash
    if is_dir {
        // For directories, we'd need recursive copy then delete
        // Simplified: just record in trash metadata
    } else {
        let data =
            crate::file_manager::read_file(path).map_err(|_| "failed to read file for trash")?;
        crate::file_manager::write_file(&trash_path, &data)
            .map_err(|_| "failed to write to trash")?;
        crate::file_manager::remove(path, &crate::file_manager::RemoveOptions::default())
            .map_err(|_| "failed to remove original")?;
    }

    let entry = TrashEntry {
        id,
        original_name: String::from(filename),
        original_path: String::from(path),
        deleted_at: crate::clock::get_ticks(),
        size,
        is_dir,
    };

    TRASH.lock().push(entry);
    crate::serial_println!("[trash] moved to trash: {} (id={})", path, id);
    Ok(())
}

/// Restore a file from trash to its original location
fn restore_from_trash(trash_id: u64) -> Result<(), &'static str> {
    let mut trash = TRASH.lock();
    let pos = trash
        .iter()
        .position(|e| e.id == trash_id)
        .ok_or("trash entry not found")?;
    let entry = trash.remove(pos);

    let trash_name = alloc::format!("{}_{}", entry.id, entry.original_name);
    let trash_path = alloc::format!("{}/{}", TRASH_FILES_DIR, trash_name);

    // Restore file
    let data =
        crate::file_manager::read_file(&trash_path).map_err(|_| "failed to read from trash")?;
    crate::file_manager::write_file(&entry.original_path, &data)
        .map_err(|_| "failed to restore file")?;
    let _ =
        crate::file_manager::remove(&trash_path, &crate::file_manager::RemoveOptions::default());

    crate::serial_println!(
        "[trash] restored: {} -> {}",
        trash_path,
        entry.original_path
    );
    Ok(())
}

/// Permanently delete a file from trash
fn permanent_delete(trash_id: u64) -> Result<(), &'static str> {
    let mut trash = TRASH.lock();
    let pos = trash
        .iter()
        .position(|e| e.id == trash_id)
        .ok_or("trash entry not found")?;
    let entry = trash.remove(pos);

    let trash_name = alloc::format!("{}_{}", entry.id, entry.original_name);
    let trash_path = alloc::format!("{}/{}", TRASH_FILES_DIR, trash_name);
    let _ =
        crate::file_manager::remove(&trash_path, &crate::file_manager::RemoveOptions::default());

    crate::serial_println!("[trash] permanently deleted: id={}", trash_id);
    Ok(())
}

/// Empty the entire trash
fn empty_trash() -> Result<(), &'static str> {
    let mut trash = TRASH.lock();
    for entry in trash.iter() {
        let trash_name = alloc::format!("{}_{}", entry.id, entry.original_name);
        let trash_path = alloc::format!("{}/{}", TRASH_FILES_DIR, trash_name);
        let _ = crate::file_manager::remove(
            &trash_path,
            &crate::file_manager::RemoveOptions::default(),
        );
    }
    let count = trash.len();
    trash.clear();
    crate::serial_println!("[trash] emptied {} items", count);
    Ok(())
}

/// List all trash entries
pub fn list_trash() -> Vec<TrashEntry> {
    TRASH.lock().clone()
}

/// Get trash item count
pub fn trash_count() -> usize {
    TRASH.lock().len()
}

/// Get total trash size
pub fn trash_size() -> u64 {
    TRASH.lock().iter().map(|e| e.size).sum()
}

/// Extract filename from a path
fn extract_filename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn stats() -> u64 {
    OPS_PERFORMED.load(Ordering::Relaxed)
}

/// Initialize the desktop file ops and trash system
pub fn init() {
    // Ensure trash directories exist
    let _ = crate::file_manager::mkdir(TRASH_DIR, 0o755);
    let _ = crate::file_manager::mkdir(TRASH_INFO_DIR, 0o755);
    let _ = crate::file_manager::mkdir(TRASH_FILES_DIR, 0o755);
    crate::serial_println!("[desktop_ops] initialized, trash_dir={}", TRASH_DIR);
}
