//! Trash / recycle bin backed by `/home/.Trash`.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::constants::*;
use super::files::{mkdirp, rename};
use super::links::unlink;
use super::stat::stat;

// ═══════════════════════════════════════════════════════════════════════
// TRASH / RECYCLE BIN
// ═══════════════════════════════════════════════════════════════════════

const TRASH_DIR: &str = "/home/.Trash";

/// A trashed file entry
#[derive(Debug, Clone)]
pub struct TrashEntry {
    pub original_path: String,
    pub trash_name: String,
    pub deleted_at: u64,
    pub size: u64,
}

lazy_static::lazy_static! {
    static ref TRASH: Mutex<Vec<TrashEntry>> = Mutex::new(Vec::new());
}

/// Move a file to trash instead of deleting permanently
pub fn trash_file(path: &str) -> Result<(), i32> {
    // Ensure trash dir exists
    let _ = mkdirp(TRASH_DIR, 0o700);

    let name = path.rsplit('/').next().unwrap_or("file");
    let ts = crate::rtc::unix_time() as u64;
    let trash_name = alloc::format!("{}_{}", ts, name);
    let trash_path = alloc::format!("{}/{}", TRASH_DIR, trash_name);

    rename(path, &trash_path)?;
    let size = stat(&trash_path).map(|m| m.st_size).unwrap_or(0);
    TRASH.lock().push(TrashEntry {
        original_path: String::from(path),
        trash_name,
        deleted_at: ts,
        size,
    });
    Ok(())
}

/// Restore a file from trash
pub fn restore_from_trash(trash_name: &str) -> Result<(), i32> {
    let entry = TRASH
        .lock()
        .iter()
        .find(|e| e.trash_name == trash_name)
        .cloned();

    if let Some(entry) = entry {
        let trash_path = alloc::format!("{}/{}", TRASH_DIR, trash_name);
        rename(&trash_path, &entry.original_path)?;
        TRASH.lock().retain(|e| e.trash_name != trash_name);
        Ok(())
    } else {
        Err(ENOENT)
    }
}

/// Empty the trash permanently
pub fn empty_trash() -> usize {
    let entries: Vec<TrashEntry> = TRASH.lock().drain(..).collect();
    let mut count = 0;
    for entry in &entries {
        let trash_path = alloc::format!("{}/{}", TRASH_DIR, entry.trash_name);
        if unlink(&trash_path).is_ok() {
            count += 1;
        }
    }
    count
}

/// List trash contents
pub fn list_trash() -> Vec<TrashEntry> {
    TRASH.lock().clone()
}
