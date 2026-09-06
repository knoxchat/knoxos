/// Recently Used Items Tracker
///
/// Tracks recently used applications, files, and URIs
/// for quick access from the launcher and file dialogs.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Type of recently used item
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecentItemType {
    Application,
    File,
    Directory,
    Uri,
}

/// A recently used item
#[derive(Debug, Clone)]
pub struct RecentItem {
    pub item_type: RecentItemType,
    pub uri: String,
    pub display_name: String,
    pub mime_type: String,
    pub app_name: String,
    pub timestamp: u64,
    pub access_count: u32,
    pub pinned: bool,
}

lazy_static::lazy_static! {
    static ref RECENT_ITEMS: Mutex<Vec<RecentItem>> = Mutex::new(Vec::new());
}

const MAX_ITEMS: usize = 500;

/// Record access to a resource
pub fn record(
    item_type: RecentItemType,
    uri: &str,
    display_name: &str,
    mime_type: &str,
    app: &str,
    timestamp: u64,
) {
    let mut items = RECENT_ITEMS.lock();
    // Update if already exists
    if let Some(existing) = items.iter_mut().find(|i| i.uri == uri) {
        existing.timestamp = timestamp;
        existing.access_count += 1;
        return;
    }
    items.push(RecentItem {
        item_type,
        uri: String::from(uri),
        display_name: String::from(display_name),
        mime_type: String::from(mime_type),
        app_name: String::from(app),
        timestamp,
        access_count: 1,
        pinned: false,
    });
    // Trim to max keeping pinned items
    if items.len() > MAX_ITEMS {
        items.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.timestamp.cmp(&a.timestamp)));
        items.truncate(MAX_ITEMS);
    }
}

/// Get recently used items sorted by most recent
pub fn get_recent(limit: usize) -> Vec<RecentItem> {
    let items = RECENT_ITEMS.lock();
    let mut sorted: Vec<_> = items.clone();
    sorted.sort_by_key(|b| core::cmp::Reverse(b.timestamp));
    sorted.truncate(limit);
    sorted
}

/// Get frequently used items
pub fn get_frequent(limit: usize) -> Vec<RecentItem> {
    let items = RECENT_ITEMS.lock();
    let mut sorted: Vec<_> = items.clone();
    sorted.sort_by_key(|b| core::cmp::Reverse(b.access_count));
    sorted.truncate(limit);
    sorted
}

/// Get recent items for a specific app
pub fn for_app(app_name: &str, limit: usize) -> Vec<RecentItem> {
    let items = RECENT_ITEMS.lock();
    let mut filtered: Vec<_> = items
        .iter()
        .filter(|i| i.app_name == app_name)
        .cloned()
        .collect();
    filtered.sort_by_key(|b| core::cmp::Reverse(b.timestamp));
    filtered.truncate(limit);
    filtered
}

/// Clear all non-pinned items
pub fn clear() {
    RECENT_ITEMS.lock().retain(|i| i.pinned);
}

/// Remove a specific item
pub fn remove(uri: &str) {
    RECENT_ITEMS.lock().retain(|i| i.uri != uri);
}

pub fn init() {
    crate::serial_println!("[RECENT] Recently used items tracker loaded");
}
