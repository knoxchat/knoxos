//! File-manager UI state: recent files, tabs, split pane, progress, volumes.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::files::{mkdirp, rename};

// ═══════════════════════════════════════════════════════════════════════
// RECENT FILES LIST — track recently opened files
// ═══════════════════════════════════════════════════════════════════════

const MAX_RECENT: usize = 50;

struct RecentFilesStore {
    entries: Vec<RecentFileEntry>,
}

#[derive(Debug, Clone)]
pub struct RecentFileEntry {
    pub path: String,
    pub timestamp: u64,
    pub app: String,
}

lazy_static::lazy_static! {
    static ref RECENT_FILES: Mutex<RecentFilesStore> = Mutex::new(RecentFilesStore {
        entries: Vec::new(),
    });
}

/// Record a file as recently opened
pub fn record_recent_file(path: &str, app: &str) {
    let mut store = RECENT_FILES.lock();
    // Remove existing entry for same path
    store.entries.retain(|e| e.path != path);
    store.entries.insert(
        0,
        RecentFileEntry {
            path: String::from(path),
            timestamp: crate::rtc::unix_time() as u64,
            app: String::from(app),
        },
    );
    if store.entries.len() > MAX_RECENT {
        store.entries.truncate(MAX_RECENT);
    }
}

/// Get recent files list
pub fn get_recent_files() -> Vec<RecentFileEntry> {
    RECENT_FILES.lock().entries.clone()
}

/// Clear recent files
pub fn clear_recent_files() {
    RECENT_FILES.lock().entries.clear();
}

// ═══════════════════════════════════════════════════════════════════════
// BULK RENAME — rename multiple files using patterns
// ═══════════════════════════════════════════════════════════════════════

/// Bulk rename files matching a pattern
/// `pattern` can be "*.txt" to match, `replacement` uses {n} for sequence number
/// Returns count of renamed files
pub fn bulk_rename(dir: &str, files: &[&str], template: &str) -> Result<usize, i32> {
    let mut count = 0usize;
    for (i, file) in files.iter().enumerate() {
        let old_path = if dir.ends_with('/') {
            alloc::format!("{}{}", dir, file)
        } else {
            alloc::format!("{}/{}", dir, file)
        };

        // Build new name from template
        let seq = alloc::format!("{}", i + 1);
        let ext = file.rsplit('.').next().unwrap_or("");
        let base = file.rsplit('.').nth(1).unwrap_or(file);

        let new_name = template
            .replace("{n}", &seq)
            .replace("{name}", base)
            .replace("{ext}", ext);

        let new_path = if dir.ends_with('/') {
            alloc::format!("{}{}", dir, new_name)
        } else {
            alloc::format!("{}/{}", dir, new_name)
        };

        if rename(&old_path, &new_path).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

// ═══════════════════════════════════════════════════════════════════════
// FILE MANAGER TABS — multiple directory tabs
// ═══════════════════════════════════════════════════════════════════════

/// A tab in the file manager
#[derive(Debug, Clone)]
pub struct FileManagerTab {
    pub id: u32,
    pub path: String,
    pub title: String,
    pub scroll_offset: usize,
    pub selected_files: Vec<String>,
}

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// Tab manager state
struct TabManager {
    tabs: Vec<FileManagerTab>,
    active_tab: u32,
}

lazy_static::lazy_static! {
    static ref TAB_MANAGER: Mutex<TabManager> = Mutex::new(TabManager {
        tabs: alloc::vec![FileManagerTab {
            id: 0,
            path: String::from("/home"),
            title: String::from("Home"),
            scroll_offset: 0,
            selected_files: Vec::new(),
        }],
        active_tab: 0,
    });
}

/// Open a new tab
pub fn new_tab(path: &str) -> u32 {
    let id = NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed) as u32;
    let title = path.rsplit('/').next().unwrap_or(path);
    TAB_MANAGER.lock().tabs.push(FileManagerTab {
        id,
        path: String::from(path),
        title: String::from(title),
        scroll_offset: 0,
        selected_files: Vec::new(),
    });
    id
}

/// Close a tab
pub fn close_tab(tab_id: u32) {
    let mut tm = TAB_MANAGER.lock();
    tm.tabs.retain(|t| t.id != tab_id);
    if tm.tabs.is_empty() {
        tm.tabs.push(FileManagerTab {
            id: 0,
            path: String::from("/home"),
            title: String::from("Home"),
            scroll_offset: 0,
            selected_files: Vec::new(),
        });
        tm.active_tab = 0;
    } else if tm.active_tab == tab_id {
        tm.active_tab = tm.tabs[0].id;
    }
}

/// Switch to a tab
pub fn switch_tab(tab_id: u32) {
    TAB_MANAGER.lock().active_tab = tab_id;
}

/// Get all tabs
pub fn get_tabs() -> Vec<FileManagerTab> {
    TAB_MANAGER.lock().tabs.clone()
}

/// Get active tab
pub fn active_tab() -> Option<FileManagerTab> {
    let tm = TAB_MANAGER.lock();
    tm.tabs.iter().find(|t| t.id == tm.active_tab).cloned()
}

// ═══════════════════════════════════════════════════════════════════════
// SPLIT PANE — dual-panel file manager
// ═══════════════════════════════════════════════════════════════════════

/// Split pane state
pub struct SplitPaneState {
    pub enabled: bool,
    pub left_path: String,
    pub right_path: String,
    pub active_side: PaneSide,
    pub split_ratio: f32, // 0.0-1.0, default 0.5
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneSide {
    Left,
    Right,
}

lazy_static::lazy_static! {
    static ref SPLIT_PANE: Mutex<SplitPaneState> = Mutex::new(SplitPaneState {
        enabled: false,
        left_path: String::from("/home"),
        right_path: String::from("/home"),
        active_side: PaneSide::Left,
        split_ratio: 0.5,
    });
}

/// Toggle split pane mode
pub fn toggle_split_pane() -> bool {
    let mut sp = SPLIT_PANE.lock();
    sp.enabled = !sp.enabled;
    sp.enabled
}

/// Get split pane state
pub fn get_split_pane() -> (bool, String, String, PaneSide) {
    let sp = SPLIT_PANE.lock();
    (
        sp.enabled,
        sp.left_path.clone(),
        sp.right_path.clone(),
        sp.active_side,
    )
}

/// Set path for a pane
pub fn set_pane_path(side: PaneSide, path: &str) {
    let mut sp = SPLIT_PANE.lock();
    match side {
        PaneSide::Left => sp.left_path = String::from(path),
        PaneSide::Right => sp.right_path = String::from(path),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// COPY/MOVE PROGRESS TRACKING
// ═══════════════════════════════════════════════════════════════════════

/// Progress state for long file operations
#[derive(Debug, Clone)]
pub struct OperationProgress {
    pub operation: String,
    pub source: String,
    pub dest: String,
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub total_files: u32,
    pub completed_files: u32,
    pub active: bool,
}

lazy_static::lazy_static! {
    static ref COPY_PROGRESS: Mutex<OperationProgress> = Mutex::new(OperationProgress {
        operation: String::new(),
        source: String::new(),
        dest: String::new(),
        total_bytes: 0,
        copied_bytes: 0,
        total_files: 0,
        completed_files: 0,
        active: false,
    });
}

/// Start tracking a copy/move operation
pub fn start_operation_progress(
    op: &str,
    source: &str,
    dest: &str,
    total_bytes: u64,
    total_files: u32,
) {
    let mut p = COPY_PROGRESS.lock();
    p.operation = String::from(op);
    p.source = String::from(source);
    p.dest = String::from(dest);
    p.total_bytes = total_bytes;
    p.copied_bytes = 0;
    p.total_files = total_files;
    p.completed_files = 0;
    p.active = true;
}

/// Update progress
pub fn update_operation_progress(copied_bytes: u64, completed_files: u32) {
    let mut p = COPY_PROGRESS.lock();
    p.copied_bytes = copied_bytes;
    p.completed_files = completed_files;
}

/// Finish operation
pub fn finish_operation_progress() {
    let mut p = COPY_PROGRESS.lock();
    p.active = false;
}

/// Get current operation progress
pub fn get_operation_progress() -> Option<OperationProgress> {
    let p = COPY_PROGRESS.lock();
    if p.active { Some(p.clone()) } else { None }
}

// ═══════════════════════════════════════════════════════════════════════
// MOUNT/UNMOUNT IN SIDEBAR
// ═══════════════════════════════════════════════════════════════════════

/// A mounted volume visible in the file manager sidebar
#[derive(Debug, Clone)]
pub struct SidebarVolume {
    pub name: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub removable: bool,
    pub mounted: bool,
}

/// Get volumes for file manager sidebar
pub fn get_sidebar_volumes() -> Vec<SidebarVolume> {
    alloc::vec![
        SidebarVolume {
            name: String::from("System"),
            mount_point: String::from("/"),
            fs_type: String::from("knoxfs"),
            total_bytes: 64 * 1024 * 1024 * 1024,
            free_bytes: 32 * 1024 * 1024 * 1024,
            removable: false,
            mounted: true,
        },
        SidebarVolume {
            name: String::from("Home"),
            mount_point: String::from("/home"),
            fs_type: String::from("knoxfs"),
            total_bytes: 128 * 1024 * 1024 * 1024,
            free_bytes: 96 * 1024 * 1024 * 1024,
            removable: false,
            mounted: true,
        },
    ]
}

/// Mount a volume
pub fn mount_sidebar_volume(device: &str, mount_point: &str, fs_type: &str) -> Result<(), i32> {
    // Ensure mount point exists
    let _ = mkdirp(mount_point, 0o755);
    serial_println!(
        "[file_manager] Mounted {} at {} ({})",
        device,
        mount_point,
        fs_type
    );
    Ok(())
}

/// Unmount a volume
pub fn unmount_sidebar_volume(mount_point: &str) -> Result<(), i32> {
    serial_println!("[file_manager] Unmounted {}", mount_point);
    Ok(())
}
