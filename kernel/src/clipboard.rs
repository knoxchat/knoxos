/// Global Clipboard — System-wide copy/paste buffer for KnoxOS
///
/// Provides a single shared clipboard accessible by all applications:
/// - Terminal (Ctrl+Shift+C / Ctrl+Shift+V)
/// - Text editor (Ctrl+C / Ctrl+V / Ctrl+X)
/// - File explorer (copy/paste paths)
/// - Any GUI widget
///
/// Supports both text and (future) rich content types.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// The type of content stored in the clipboard
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardContent {
    /// Empty clipboard
    Empty,
    /// Plain text content
    Text(String),
    /// File path(s) for copy/move operations
    FilePaths {
        paths: Vec<String>,
        /// true = cut (move), false = copy
        is_cut: bool,
    },
}

/// Global clipboard state
struct Clipboard {
    content: ClipboardContent,
    /// Monotonic sequence number — increments on every write
    sequence: u64,
}

lazy_static::lazy_static! {
    static ref CLIPBOARD: Mutex<Clipboard> = Mutex::new(Clipboard {
        content: ClipboardContent::Empty,
        sequence: 0,
    });
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API — Text clipboard (most common)
// ═══════════════════════════════════════════════════════════════════════

/// Copy text to the system clipboard
pub fn copy_text(text: &str) {
    let mut cb = CLIPBOARD.lock();
    cb.content = ClipboardContent::Text(String::from(text));
    cb.sequence += 1;
    crate::serial_println!(
        "[KnoxOS] Clipboard: copied {} chars (seq={})",
        text.len(),
        cb.sequence,
    );
}

/// Get text from the system clipboard. Returns None if clipboard is empty
/// or contains non-text content.
pub fn paste_text() -> Option<String> {
    let cb = CLIPBOARD.lock();
    match &cb.content {
        ClipboardContent::Text(s) => Some(s.clone()),
        _ => None,
    }
}

/// Check if clipboard has text content
pub fn has_text() -> bool {
    matches!(CLIPBOARD.lock().content, ClipboardContent::Text(_))
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API — File paths clipboard
// ═══════════════════════════════════════════════════════════════════════

/// Copy file paths to the clipboard (for file manager copy operation)
pub fn copy_files(paths: &[&str]) {
    let mut cb = CLIPBOARD.lock();
    cb.content = ClipboardContent::FilePaths {
        paths: paths.iter().map(|p| String::from(*p)).collect(),
        is_cut: false,
    };
    cb.sequence += 1;
    crate::serial_println!("[KnoxOS] Clipboard: copied {} file path(s)", paths.len(),);
}

/// Cut file paths to the clipboard (for file manager move operation)
pub fn cut_files(paths: &[&str]) {
    let mut cb = CLIPBOARD.lock();
    cb.content = ClipboardContent::FilePaths {
        paths: paths.iter().map(|p| String::from(*p)).collect(),
        is_cut: true,
    };
    cb.sequence += 1;
    crate::serial_println!("[KnoxOS] Clipboard: cut {} file path(s)", paths.len(),);
}

/// Get file paths from the clipboard. Returns None if clipboard doesn't
/// contain file paths.
pub fn paste_files() -> Option<(Vec<String>, bool)> {
    let cb = CLIPBOARD.lock();
    match &cb.content {
        ClipboardContent::FilePaths { paths, is_cut } => Some((paths.clone(), *is_cut)),
        _ => None,
    }
}

/// Check if clipboard has file paths
pub fn has_files() -> bool {
    matches!(CLIPBOARD.lock().content, ClipboardContent::FilePaths { .. })
}

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API — General
// ═══════════════════════════════════════════════════════════════════════

/// Clear the clipboard
pub fn clear() {
    let mut cb = CLIPBOARD.lock();
    cb.content = ClipboardContent::Empty;
    cb.sequence += 1;
}

/// Check if clipboard is empty
pub fn is_empty() -> bool {
    matches!(CLIPBOARD.lock().content, ClipboardContent::Empty)
}

/// Get the current clipboard sequence number (for change detection)
pub fn sequence() -> u64 {
    CLIPBOARD.lock().sequence
}

/// Get a human-readable description of clipboard contents
pub fn describe() -> String {
    let cb = CLIPBOARD.lock();
    match &cb.content {
        ClipboardContent::Empty => String::from("(empty)"),
        ClipboardContent::Text(s) => {
            if s.len() <= 40 {
                alloc::format!("Text: \"{}\"", s)
            } else {
                alloc::format!("Text: \"{}...\" ({} chars)", &s[..37], s.len())
            }
        }
        ClipboardContent::FilePaths { paths, is_cut } => {
            let op = if *is_cut { "Cut" } else { "Copied" };
            if paths.len() == 1 {
                alloc::format!("{}: {}", op, paths[0])
            } else {
                alloc::format!("{}: {} files", op, paths.len())
            }
        }
    }
}

/// Initialize the clipboard system
pub fn init() {
    crate::serial_println!("[KnoxOS] Global clipboard initialized");
}
