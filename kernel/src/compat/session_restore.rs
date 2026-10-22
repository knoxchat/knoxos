/// Session Restore — Save and restore desktop state after crash/reboot
///
/// Periodically snapshots open window state to a journal file so that
/// after an unexpected reboot, the user's workspace can be reconstructed:
///   - Window positions, sizes, and content types
///   - Application-specific state (editor files, browser tabs, terminal CWD)
///   - Desktop widget positions
///   - Clipboard contents (text only)
///   - Focus stack order
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Constants ──────────────────────────────────────────────────────

/// Magic header for session file
const SESSION_MAGIC: u32 = 0x4B4E5853; // "KNXS"
/// Session file version
const SESSION_VERSION: u16 = 1;
/// Maximum saved windows
const MAX_SAVED_WINDOWS: usize = 64;
/// Auto-save interval in ticks (~5 seconds at 100 Hz)
const AUTO_SAVE_INTERVAL: u64 = 500;
/// Session journal path (in-memory VFS)
const SESSION_PATH: &str = "/var/lib/knoxos/session.dat";

// ─── Session Snapshot Types ─────────────────────────────────────────

/// A snapshot of the entire desktop session
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    /// When this snapshot was taken
    pub timestamp: u64,
    /// Saved windows
    pub windows: Vec<WindowState>,
    /// Desktop widget state
    pub widgets: Vec<WidgetState>,
    /// Focus order (window IDs front to back)
    pub focus_stack: Vec<u32>,
    /// Clipboard text content
    pub clipboard: Option<String>,
    /// Active workspace/virtual desktop index
    pub active_workspace: u32,
    /// Whether we had a clean shutdown
    pub clean_shutdown: bool,
}

/// Saved state of a single window
#[derive(Debug, Clone)]
pub struct WindowState {
    /// Window ID
    pub id: u32,
    /// Window title
    pub title: String,
    /// Position and size
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Window content type identifier
    pub content_type: String,
    /// Whether window was minimized
    pub minimized: bool,
    /// Whether window was maximized
    pub maximized: bool,
    /// App-specific state (serialized)
    pub app_state: BTreeMap<String, String>,
}

/// Saved state of a desktop widget
#[derive(Debug, Clone)]
pub struct WidgetState {
    pub widget_type: String,
    pub x: i32,
    pub y: i32,
    pub config: BTreeMap<String, String>,
}

// ─── Global State ───────────────────────────────────────────────────

lazy_static::lazy_static! {
    /// Last saved session snapshot
    static ref LAST_SNAPSHOT: Mutex<Option<SessionSnapshot>> = Mutex::new(None);
    /// Pending restore data (loaded at boot, consumed on desktop init)
    static ref PENDING_RESTORE: Mutex<Option<SessionSnapshot>> = Mutex::new(None);
    /// Session journal (serialized snapshots)
    static ref SESSION_JOURNAL: Mutex<Vec<u8>> = Mutex::new(Vec::new());
}

static RESTORE_ENABLED: AtomicBool = AtomicBool::new(true);
static LAST_SAVE_TICK: AtomicU64 = AtomicU64::new(0);
static SNAPSHOTS_TAKEN: AtomicU64 = AtomicU64::new(0);
static RESTORE_PERFORMED: AtomicBool = AtomicBool::new(false);

// ─── Serialization ──────────────────────────────────────────────────

impl SessionSnapshot {
    /// Serialize snapshot to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4096);

        // Header
        buf.extend_from_slice(&SESSION_MAGIC.to_le_bytes());
        buf.extend_from_slice(&SESSION_VERSION.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.push(if self.clean_shutdown { 1 } else { 0 });
        buf.extend_from_slice(&self.active_workspace.to_le_bytes());

        // Window count
        let win_count = self.windows.len().min(MAX_SAVED_WINDOWS) as u16;
        buf.extend_from_slice(&win_count.to_le_bytes());

        // Windows
        for win in self.windows.iter().take(MAX_SAVED_WINDOWS) {
            buf.extend_from_slice(&win.id.to_le_bytes());
            write_string(&mut buf, &win.title);
            buf.extend_from_slice(&win.x.to_le_bytes());
            buf.extend_from_slice(&win.y.to_le_bytes());
            buf.extend_from_slice(&win.width.to_le_bytes());
            buf.extend_from_slice(&win.height.to_le_bytes());
            write_string(&mut buf, &win.content_type);
            buf.push(if win.minimized { 1 } else { 0 });
            buf.push(if win.maximized { 1 } else { 0 });

            // App state map
            let state_count = win.app_state.len() as u16;
            buf.extend_from_slice(&state_count.to_le_bytes());
            for (k, v) in &win.app_state {
                write_string(&mut buf, k);
                write_string(&mut buf, v);
            }
        }

        // Focus stack
        let focus_count = self.focus_stack.len() as u16;
        buf.extend_from_slice(&focus_count.to_le_bytes());
        for &id in &self.focus_stack {
            buf.extend_from_slice(&id.to_le_bytes());
        }

        // Clipboard
        if let Some(ref clip) = self.clipboard {
            buf.push(1);
            write_string(&mut buf, clip);
        } else {
            buf.push(0);
        }

        // Widget count
        let widget_count = self.widgets.len() as u16;
        buf.extend_from_slice(&widget_count.to_le_bytes());
        for widget in &self.widgets {
            write_string(&mut buf, &widget.widget_type);
            buf.extend_from_slice(&widget.x.to_le_bytes());
            buf.extend_from_slice(&widget.y.to_le_bytes());
            let cfg_count = widget.config.len() as u16;
            buf.extend_from_slice(&cfg_count.to_le_bytes());
            for (k, v) in &widget.config {
                write_string(&mut buf, k);
                write_string(&mut buf, v);
            }
        }

        buf
    }

    /// Deserialize snapshot from bytes
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        let mut pos = 0;

        // Header
        if data.len() < 15 {
            return None;
        }
        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if magic != SESSION_MAGIC {
            return None;
        }
        pos += 4;

        let version = u16::from_le_bytes([data[pos], data[pos + 1]]);
        if version > SESSION_VERSION {
            return None;
        }
        pos += 2;

        let timestamp = u64::from_le_bytes(data[pos..pos + 8].try_into().ok()?);
        pos += 8;

        let clean_shutdown = data[pos] != 0;
        pos += 1;

        let active_workspace = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
        pos += 4;

        // Windows
        let win_count = u16::from_le_bytes(data[pos..pos + 2].try_into().ok()?) as usize;
        pos += 2;

        let mut windows = Vec::with_capacity(win_count);
        for _ in 0..win_count {
            if pos + 4 > data.len() {
                break;
            }
            let id = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let (title, new_pos) = read_string(data, pos)?;
            pos = new_pos;
            let x = i32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let y = i32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let width = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let height = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let (content_type, new_pos) = read_string(data, pos)?;
            pos = new_pos;
            let minimized = data[pos] != 0;
            pos += 1;
            let maximized = data[pos] != 0;
            pos += 1;

            let state_count = u16::from_le_bytes(data[pos..pos + 2].try_into().ok()?) as usize;
            pos += 2;
            let mut app_state = BTreeMap::new();
            for _ in 0..state_count {
                let (k, np) = read_string(data, pos)?;
                pos = np;
                let (v, np) = read_string(data, pos)?;
                pos = np;
                app_state.insert(k, v);
            }

            windows.push(WindowState {
                id,
                title,
                x,
                y,
                width,
                height,
                content_type,
                minimized,
                maximized,
                app_state,
            });
        }

        // Focus stack
        if pos + 2 > data.len() {
            return None;
        }
        let focus_count = u16::from_le_bytes(data[pos..pos + 2].try_into().ok()?) as usize;
        pos += 2;
        let mut focus_stack = Vec::with_capacity(focus_count);
        for _ in 0..focus_count {
            if pos + 4 > data.len() {
                break;
            }
            let id = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
            pos += 4;
            focus_stack.push(id);
        }

        // Clipboard
        let mut clipboard = None;
        if pos < data.len() && data[pos] == 1 {
            pos += 1;
            let (text, np) = read_string(data, pos)?;
            pos = np;
            clipboard = Some(text);
        } else {
            pos += 1;
        }

        // Widgets
        let mut widgets = Vec::new();
        if pos + 2 <= data.len() {
            let widget_count = u16::from_le_bytes(data[pos..pos + 2].try_into().ok()?) as usize;
            pos += 2;
            for _ in 0..widget_count {
                let (widget_type, np) = read_string(data, pos)?;
                pos = np;
                let wx = i32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
                pos += 4;
                let wy = i32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
                pos += 4;
                let cfg_count = u16::from_le_bytes(data[pos..pos + 2].try_into().ok()?) as usize;
                pos += 2;
                let mut config = BTreeMap::new();
                for _ in 0..cfg_count {
                    let (k, np) = read_string(data, pos)?;
                    pos = np;
                    let (v, np) = read_string(data, pos)?;
                    pos = np;
                    config.insert(k, v);
                }
                widgets.push(WidgetState {
                    widget_type,
                    x: wx,
                    y: wy,
                    config,
                });
            }
        }

        Some(SessionSnapshot {
            timestamp,
            windows,
            widgets,
            focus_stack,
            clipboard,
            active_workspace,
            clean_shutdown,
        })
    }
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(4096) as u16;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&bytes[..len as usize]);
}

fn read_string(data: &[u8], pos: usize) -> Option<(String, usize)> {
    if pos + 2 > data.len() {
        return None;
    }
    let len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
    if pos + 2 + len > data.len() {
        return None;
    }
    let s = core::str::from_utf8(&data[pos + 2..pos + 2 + len]).ok()?;
    Some((String::from(s), pos + 2 + len))
}

// ─── Public API ─────────────────────────────────────────────────────

/// Take a session snapshot (called periodically by the compositor)
pub fn save_snapshot(snapshot: SessionSnapshot) {
    if !RESTORE_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let now = crate::interrupts::get_ticks();
    let last = LAST_SAVE_TICK.load(Ordering::Relaxed);
    if now.saturating_sub(last) < AUTO_SAVE_INTERVAL {
        return; // Throttle saves
    }

    let serialized = snapshot.serialize();

    let mut journal = SESSION_JOURNAL.lock();
    *journal = serialized;

    *LAST_SNAPSHOT.lock() = Some(snapshot);
    LAST_SAVE_TICK.store(now, Ordering::Relaxed);
    SNAPSHOTS_TAKEN.fetch_add(1, Ordering::Relaxed);
}

/// Check if a session restore is pending (call at desktop init)
pub fn has_pending_restore() -> bool {
    PENDING_RESTORE.lock().is_some()
}

/// Consume the pending restore (returns the snapshot to restore)
pub fn take_pending_restore() -> Option<SessionSnapshot> {
    if RESTORE_PERFORMED.load(Ordering::Relaxed) {
        return None;
    }
    RESTORE_PERFORMED.store(true, Ordering::Relaxed);
    PENDING_RESTORE.lock().take()
}

/// Record clean shutdown (prevents restore on next boot)
pub fn mark_clean_shutdown() {
    if let Some(ref mut snap) = *LAST_SNAPSHOT.lock() {
        snap.clean_shutdown = true;
    }
    // Write final journal with clean_shutdown = true
    let mut journal = SESSION_JOURNAL.lock();
    if let Some(snap) = LAST_SNAPSHOT.lock().as_ref() {
        *journal = snap.serialize();
    }
    serial_println!("[session_restore] clean shutdown recorded");
}

/// Enable/disable session restore
pub fn set_enabled(enabled: bool) {
    RESTORE_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Get snapshot count
pub fn snapshot_count() -> u64 {
    SNAPSHOTS_TAKEN.load(Ordering::Relaxed)
}

/// Initialize session restore subsystem
pub fn init() {
    // Try to load journal from previous session
    let journal = SESSION_JOURNAL.lock();
    if !journal.is_empty() {
        if let Some(snapshot) = SessionSnapshot::deserialize(&journal) {
            if !snapshot.clean_shutdown {
                serial_println!(
                    "[session_restore] crash detected — {} windows to restore",
                    snapshot.windows.len()
                );
                *PENDING_RESTORE.lock() = Some(snapshot);
            } else {
                serial_println!("[session_restore] clean shutdown — no restore needed");
            }
        }
    }
    serial_println!(
        "[session_restore] initialized (auto-save every {}ms)",
        AUTO_SAVE_INTERVAL * 10
    );
}
